use std::{collections::BTreeMap, path::PathBuf, str::FromStr, sync::Arc, time::Duration};

use lexe::{
    config::WalletEnvConfig,
    types::{
        auth::{CredentialsRef, RootSeed},
        bitcoin::Offer,
        command::{CreateOfferRequest, PayRequest},
        payment::{Order, Payment, PaymentDirection, PaymentFilter, PaymentStatus},
    },
    wallet::LexeWallet,
};
use nostr::EventId;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    events,
    relay::{query_relay, submit_event},
    wallet::{
        balance::{wallet_balances, WalletBalances},
        format::{amount_from_sats, format_amount, wallet_transaction_with_annotation},
        runtime::ensure_wallet,
        storage::{current_pubkey, write_atomic_text},
    },
};

use super::types::{
    HiveChannelContributionShare, HiveChannelWalletSummary, WalletPaymentResult, WalletTransaction,
    DEFAULT_TRANSACTION_LIMIT, HIVE_CHANNELS_DIR_NAME, LEXE_DATA_DIR_NAME, MAX_TRANSACTION_LIMIT,
    OFFER_FILE_NAME, SEED_FILE_NAME, WALLET_BOLT12_OFFER_DESCRIPTION, WALLET_DIR_NAME,
};

const HIVE_OFFER_CREATE_ATTEMPTS: usize = 4;
const HIVE_OFFER_CREATE_RETRY_DELAY_MS: u64 = 750;
const CONTRIBUTION_MESSAGE_PREFIX: &str = "sprout-hive-contribution:v1:";

struct HiveChannelWalletStorage {
    root_dir: PathBuf,
    data_dir: PathBuf,
    seed_path: PathBuf,
    offer_path: PathBuf,
}

pub(crate) struct HiveChannelWalletSetup {
    pub(crate) bolt12_offer: Option<String>,
}

impl HiveChannelWalletStorage {
    fn from_app(app: &AppHandle, channel_id: Uuid) -> Result<Self, String> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("app data dir: {error}"))?;
        let root_dir = app_data_dir
            .join(WALLET_DIR_NAME)
            .join(HIVE_CHANNELS_DIR_NAME)
            .join(channel_id.to_string());
        Ok(Self {
            data_dir: root_dir.join(LEXE_DATA_DIR_NAME),
            seed_path: root_dir.join(SEED_FILE_NAME),
            offer_path: root_dir.join(OFFER_FILE_NAME),
            root_dir,
        })
    }

    fn ensure_dirs(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root_dir)
            .map_err(|error| format!("create hive wallet directory: {error}"))?;
        std::fs::create_dir_all(&self.data_dir)
            .map_err(|error| format!("create hive Lexe data directory: {error}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("set hive wallet directory permissions: {error}"))?;
        }

        Ok(())
    }
}

pub(crate) async fn create_hive_channel_wallet(
    app: &AppHandle,
    state: &AppState,
    channel_id: Uuid,
) -> Result<HiveChannelWalletSetup, String> {
    let storage = HiveChannelWalletStorage::from_app(app, channel_id)?;
    let _wallet = ensure_hive_wallet(app, state, channel_id).await?;
    let bolt12_offer = read_cached_hive_bolt12_offer(&storage);
    Ok(HiveChannelWalletSetup { bolt12_offer })
}

#[tauri::command]
pub async fn get_hive_channel_wallet_summary(
    channel_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HiveChannelWalletSummary, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    let storage = HiveChannelWalletStorage::from_app(&app, channel_uuid)?;
    if !storage.seed_path.exists() {
        return Err("hive wallet seed is not stored on this machine".to_string());
    }

    let wallet = ensure_hive_wallet(&app, &state, channel_uuid).await?;
    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync hive Lexe payments: {error}"))?;
    let info = wallet
        .node_info()
        .await
        .map_err(|error| format!("load hive Lexe node info: {error}"))?;
    let balances = wallet_balances(&wallet, &info).await;
    let bolt12_offer = match ensure_hive_bolt12_offer(&wallet, &storage, channel_uuid).await {
        Ok(offer) => {
            publish_hive_channel_wallet_offer_if_missing(&state, channel_uuid, &offer).await;
            offer
        }
        Err(error) => {
            eprintln!(
                "buzz-desktop: hive channel {channel_uuid} BOLT12 offer is not ready: {error}"
            );
            String::new()
        }
    };

    build_hive_channel_wallet_summary(
        channel_id,
        bolt12_offer,
        &storage,
        &wallet,
        &info,
        balances,
        channel_uuid,
    )
}

#[tauri::command]
pub async fn reveal_hive_channel_wallet_seed(
    channel_id: String,
    app: AppHandle,
) -> Result<String, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    let storage = HiveChannelWalletStorage::from_app(&app, channel_uuid)?;
    let seed = RootSeed::read_from_path(&storage.seed_path)
        .map_err(|error| format!("read hive wallet seed: {error}"))?
        .ok_or_else(|| "hive wallet seed is not stored on this machine".to_string())?;
    Ok(seed.to_mnemonic().to_string())
}

#[tauri::command]
pub async fn generate_hive_channel_wallet_bolt12_offer(
    channel_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HiveChannelWalletSummary, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    let storage = HiveChannelWalletStorage::from_app(&app, channel_uuid)?;
    if !storage.seed_path.exists() {
        return Err("hive wallet seed is not stored on this machine".to_string());
    }

    let wallet = ensure_hive_wallet(&app, &state, channel_uuid).await?;
    let offer = create_hive_bolt12_offer(&wallet, channel_uuid).await?;
    publish_hive_channel_wallet_offer(&state, channel_uuid, &offer).await?;
    write_hive_bolt12_offer(&storage, channel_uuid, &offer)?;

    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync hive Lexe payments: {error}"))?;
    let info = wallet
        .node_info()
        .await
        .map_err(|error| format!("load hive Lexe node info: {error}"))?;
    let balances = wallet_balances(&wallet, &info).await;

    build_hive_channel_wallet_summary(
        channel_id,
        offer,
        &storage,
        &wallet,
        &info,
        balances,
        channel_uuid,
    )
}

#[tauri::command]
pub async fn get_hive_channel_wallet_transactions(
    channel_id: String,
    limit: Option<usize>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<WalletTransaction>, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    let storage = HiveChannelWalletStorage::from_app(&app, channel_uuid)?;
    if !storage.seed_path.exists() {
        return Err("hive wallet seed is not stored on this machine".to_string());
    }

    let wallet = ensure_hive_wallet(&app, &state, channel_uuid).await?;
    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync hive Lexe payments: {error}"))?;
    let limit = limit
        .unwrap_or(DEFAULT_TRANSACTION_LIMIT)
        .clamp(1, MAX_TRANSACTION_LIMIT);
    let response = wallet
        .list_payments(&PaymentFilter::All, Some(Order::Desc), Some(limit), None)
        .map_err(|error| format!("list hive Lexe payments: {error}"))?;

    Ok(response
        .payments
        .iter()
        .map(|payment| wallet_transaction_with_annotation(payment, None))
        .collect())
}

#[tauri::command]
pub async fn send_hive_channel_funds(
    channel_id: String,
    amount_sats: u64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletPaymentResult, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    if amount_sats == 0 {
        return Err("hive channel funding amount must be greater than 0".to_string());
    }
    let payer_pubkey = current_pubkey(&state)?;
    let offer = match resolve_hive_channel_offer(&state, &channel_id).await {
        Ok(offer) => offer,
        Err(resolve_error) => {
            let offer =
                local_hive_bolt12_offer_or_error(&app, &state, channel_uuid, resolve_error).await?;
            publish_hive_channel_wallet_offer_if_missing(&state, channel_uuid, &offer).await;
            offer
        }
    };
    let offer_id_for_failure_log = offer_id_for_log(&offer);
    let wallet = ensure_wallet(&app, &state).await?;
    let amount = amount_from_sats(amount_sats)?;
    let response = wallet
        .pay(PayRequest {
            payable: offer,
            amount: Some(amount),
            message: Some(contribution_message(&channel_id, &payer_pubkey)),
            personal_note: Some(format!("Sprout hive channel contribution {channel_id}")),
        })
        .await
        .map_err(|error| format!("send hive channel funds: {error}"))?;
    if response.status != PaymentStatus::Completed {
        let offer_id = response
            .offer_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or(offer_id_for_failure_log);
        eprintln!(
            "buzz-desktop: hive channel {channel_uuid} funding failed: amount_sats={amount_sats} payment_id={} offer_id={offer_id} status_msg={:?}",
            response.index, response.status_msg,
        );
        let status_message = payment_status_message(response.status_msg.as_str());
        return Err(format!(
            "hive channel funding failed for {}: {status_message}",
            format_amount(amount_sats)
        ));
    }
    *state.wallet_state.summary.lock().await = None;

    Ok(WalletPaymentResult {
        payment_id: response.index.to_string(),
        amount_sats,
    })
}

async fn ensure_hive_wallet(
    app: &AppHandle,
    state: &AppState,
    channel_id: Uuid,
) -> Result<Arc<LexeWallet>, String> {
    let cache_key = channel_id.to_string();
    if let Some(wallet) = state.wallet_state.hive_wallets.lock().await.get(&cache_key) {
        return Ok(wallet.clone());
    }

    let storage = HiveChannelWalletStorage::from_app(app, channel_id)?;
    let wallet = Arc::new(load_hive_wallet(&storage).await?);
    state
        .wallet_state
        .hive_wallets
        .lock()
        .await
        .insert(cache_key, wallet.clone());
    Ok(wallet)
}

async fn load_hive_wallet(storage: &HiveChannelWalletStorage) -> Result<LexeWallet, String> {
    storage.ensure_dirs()?;
    let root_seed = match RootSeed::read_from_path(&storage.seed_path)
        .map_err(|error| format!("read hive wallet seed: {error}"))?
    {
        Some(seed) => seed,
        None => {
            let seed = RootSeed::generate();
            seed.write_to_path(&storage.seed_path)
                .map_err(|error| format!("write hive wallet seed: {error}"))?;
            seed
        }
    };

    let wallet = LexeWallet::load_or_fresh(
        WalletEnvConfig::mainnet(),
        CredentialsRef::from(&root_seed),
        Some(storage.data_dir.clone()),
    )
    .map_err(|error| format!("load hive Lexe wallet: {error}"))?;

    wallet
        .signup(&root_seed, None)
        .await
        .map_err(|error| format!("hive Lexe signup/provisioning: {error}"))?;
    wallet
        .provision(CredentialsRef::from(&root_seed))
        .await
        .map_err(|error| format!("hive Lexe provision: {error}"))?;

    Ok(wallet)
}

async fn ensure_hive_bolt12_offer(
    wallet: &LexeWallet,
    storage: &HiveChannelWalletStorage,
    channel_id: Uuid,
) -> Result<String, String> {
    if let Some(offer) = read_cached_hive_bolt12_offer(storage) {
        if offer_uses_current_description(&offer) {
            return Ok(offer);
        }
        eprintln!(
            "buzz-desktop: hive channel {channel_id} cached BOLT12 offer uses legacy description; creating current-shape offer"
        );
    }

    let offer = create_hive_bolt12_offer(wallet, channel_id).await?;
    write_hive_bolt12_offer(storage, channel_id, &offer)?;
    Ok(offer)
}

async fn create_hive_bolt12_offer(wallet: &LexeWallet, channel_id: Uuid) -> Result<String, String> {
    let mut last_error = None;
    for attempt in 1..=HIVE_OFFER_CREATE_ATTEMPTS {
        match wallet
            .create_offer(CreateOfferRequest {
                description: Some(WALLET_BOLT12_OFFER_DESCRIPTION.to_string()),
                min_amount: None,
                expiration_secs: None,
            })
            .await
        {
            Ok(response) => return Ok(response.offer.to_string()),
            Err(error) => {
                last_error = Some(format_anyhow_error("create hive Lexe BOLT12 offer", &error));
                if attempt < HIVE_OFFER_CREATE_ATTEMPTS {
                    tokio::time::sleep(Duration::from_millis(
                        HIVE_OFFER_CREATE_RETRY_DELAY_MS * attempt as u64,
                    ))
                    .await;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        format!("create hive Lexe BOLT12 offer failed for channel {channel_id}")
    }))
}

fn write_hive_bolt12_offer(
    storage: &HiveChannelWalletStorage,
    channel_id: Uuid,
    offer: &str,
) -> Result<(), String> {
    write_atomic_text(&storage.offer_path, offer)
        .map_err(|error| format!("cache hive channel {channel_id} BOLT12 offer: {error}"))
}

fn build_hive_channel_wallet_summary(
    channel_id: String,
    bolt12_offer: String,
    storage: &HiveChannelWalletStorage,
    wallet: &LexeWallet,
    info: &lexe::types::command::NodeInfo,
    balances: WalletBalances,
    channel_uuid: Uuid,
) -> Result<HiveChannelWalletSummary, String> {
    log_hive_wallet_state(channel_uuid, info, &bolt12_offer);
    let ownership_shares = ownership_shares(wallet, &channel_id)?;
    let total_contributed_sats = ownership_shares.iter().map(|share| share.amount_sats).sum();

    Ok(HiveChannelWalletSummary {
        channel_id,
        seed_path: storage.seed_path.to_string_lossy().to_string(),
        balance_sats: balances.balance_sats,
        lightning_balance_sats: balances.lightning_balance_sats,
        lightning_sendable_balance_sats: balances.lightning_sendable_balance_sats,
        onchain_balance_sats: balances.onchain_balance_sats,
        bolt12_offer,
        total_contributed_sats,
        ownership_shares,
    })
}

fn log_hive_wallet_state(channel_id: Uuid, info: &lexe::types::command::NodeInfo, offer: &str) {
    eprintln!(
        "buzz-desktop: hive channel {channel_id} wallet state: user_pk={} node_pk={} balance_sats={} lightning_balance_sats={} lightning_sendable_sats={} num_channels={} num_usable_channels={} offer_id={}",
        info.user_pk,
        info.node_pk,
        info.balance.sats_u64(),
        info.lightning_balance.sats_u64(),
        info.lightning_sendable_balance.sats_u64(),
        info.num_channels,
        info.num_usable_channels,
        offer_id_for_log(offer),
    );
}

fn offer_id_for_log(offer: &str) -> String {
    Offer::from_str(offer.trim())
        .map(|offer| offer.id().to_string())
        .unwrap_or_else(|_| "unparseable".to_string())
}

fn offer_uses_current_description(offer: &str) -> bool {
    let Ok(offer) = Offer::from_str(offer.trim()) else {
        return false;
    };
    offer
        .description()
        .map(|description| description.to_string())
        .as_deref()
        == Some(WALLET_BOLT12_OFFER_DESCRIPTION)
}

async fn local_hive_bolt12_offer_or_error(
    app: &AppHandle,
    state: &AppState,
    channel_id: Uuid,
    resolve_error: String,
) -> Result<String, String> {
    let storage = HiveChannelWalletStorage::from_app(app, channel_id)?;
    if !storage.seed_path.exists() {
        return Err(resolve_error);
    }

    let wallet = ensure_hive_wallet(app, state, channel_id).await?;
    ensure_hive_bolt12_offer(&wallet, &storage, channel_id)
        .await
        .map_err(|offer_error| {
            format!(
                "channel does not have a published hive wallet offer; local hive wallet offer is not ready either: {offer_error}"
            )
        })
}

fn format_anyhow_error(context: &str, error: &anyhow::Error) -> String {
    let chain = error.chain().map(ToString::to_string).collect::<Vec<_>>();
    if chain.is_empty() {
        context.to_string()
    } else {
        format!("{context}: {}", chain.join(": "))
    }
}

fn payment_status_message(status_message: &str) -> String {
    let status_message = status_message.trim();
    if status_message.is_empty() {
        "Lexe marked the payment failed".to_string()
    } else {
        status_message.to_string()
    }
}

fn ownership_shares(
    wallet: &LexeWallet,
    channel_id: &str,
) -> Result<Vec<HiveChannelContributionShare>, String> {
    let response = wallet
        .list_payments(&PaymentFilter::All, Some(Order::Asc), Some(100), None)
        .map_err(|error| format!("list hive Lexe payments: {error}"))?;
    let mut by_pubkey: BTreeMap<String, u64> = BTreeMap::new();

    for payment in response.payments {
        if payment.direction != PaymentDirection::Inbound
            || payment.status != PaymentStatus::Completed
        {
            continue;
        }
        let Some(amount_sats) = payment.amount.map(|amount| amount.sats_u64()) else {
            continue;
        };
        let Some(contributor) = contribution_pubkey(&payment, channel_id) else {
            continue;
        };
        *by_pubkey.entry(contributor).or_default() += amount_sats;
    }

    Ok(ownership_shares_from_contributions(by_pubkey))
}

fn ownership_shares_from_contributions(
    by_pubkey: BTreeMap<String, u64>,
) -> Vec<HiveChannelContributionShare> {
    let total: u64 = by_pubkey.values().sum();
    by_pubkey
        .into_iter()
        .map(
            |(member_pubkey, amount_sats)| HiveChannelContributionShare {
                member_pubkey: Some(member_pubkey),
                amount_sats,
                ownership_percent: if total == 0 {
                    0.0
                } else {
                    (amount_sats as f64 / total as f64) * 100.0
                },
            },
        )
        .collect()
}

fn contribution_message(channel_id: &str, payer_pubkey: &str) -> String {
    format!("{CONTRIBUTION_MESSAGE_PREFIX}{channel_id}:{payer_pubkey}")
}

fn contribution_pubkey(payment: &Payment, channel_id: &str) -> Option<String> {
    payment
        .message
        .as_deref()
        .and_then(|message| parse_contribution_message(message, channel_id))
}

fn parse_contribution_message(message: &str, channel_id: &str) -> Option<String> {
    let remainder = message.strip_prefix(CONTRIBUTION_MESSAGE_PREFIX)?;
    let (message_channel_id, pubkey) = remainder.split_once(':')?;
    if message_channel_id != channel_id {
        return None;
    }
    let pubkey = pubkey.trim();
    if pubkey.len() == 64 && pubkey.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(pubkey.to_string())
    } else {
        None
    }
}

async fn resolve_hive_channel_offer(state: &AppState, channel_id: &str) -> Result<String, String> {
    let relay_events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [7, 9007],
            "#h": [channel_id],
            "limit": 20,
        })],
    )
    .await?;

    relay_events
        .iter()
        .filter(|event| {
            event.kind.as_u16() == 9007
                || (event.kind.as_u16() == 7
                    && events::is_hive_channel_wallet_content(&event.content))
        })
        .filter_map(|event| {
            let offer = first_tag_value(event, "hive_wallet_bolt12_offer")?.trim();
            (!offer.is_empty()).then(|| (event.created_at.as_secs(), offer.to_string()))
        })
        .max_by_key(|(created_at, _)| *created_at)
        .map(|(_, offer)| offer)
        .ok_or_else(|| "channel does not have a hive wallet offer".to_string())
}

fn read_cached_hive_bolt12_offer(storage: &HiveChannelWalletStorage) -> Option<String> {
    let value = std::fs::read_to_string(&storage.offer_path).ok()?;
    let offer = value.trim();
    (!offer.is_empty()).then(|| offer.to_string())
}

async fn publish_hive_channel_wallet_offer_if_missing(
    state: &AppState,
    channel_id: Uuid,
    offer: &str,
) {
    let channel_id_string = channel_id.to_string();
    if resolve_hive_channel_offer(state, &channel_id_string)
        .await
        .ok()
        .as_deref()
        == Some(offer)
    {
        return;
    }

    if let Err(error) = publish_hive_channel_wallet_offer(state, channel_id, offer).await {
        eprintln!(
            "buzz-desktop: failed to publish hive channel {channel_id} wallet metadata: {error}"
        );
    }
}

async fn publish_hive_channel_wallet_offer(
    state: &AppState,
    channel_id: Uuid,
    offer: &str,
) -> Result<(), String> {
    let metadata_event_id = resolve_channel_metadata_event_id(state, channel_id).await?;
    let builder = events::build_hive_channel_wallet_metadata(channel_id, metadata_event_id, offer)
        .map_err(|error| format!("build hive channel wallet metadata: {error}"))?;
    submit_event(builder, state)
        .await
        .map(|_| ())
        .map_err(|error| format!("publish hive channel wallet metadata: {error}"))
}

async fn resolve_channel_metadata_event_id(
    state: &AppState,
    channel_id: Uuid,
) -> Result<EventId, String> {
    let channel_id_string = channel_id.to_string();
    let events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [39000],
            "#d": [channel_id_string],
            "limit": 5,
        })],
    )
    .await?;

    events
        .into_iter()
        .max_by_key(|event| event.created_at.as_secs())
        .map(|event| event.id)
        .ok_or_else(|| format!("channel metadata event not found for hive channel {channel_id}"))
}

fn first_tag_value<'a>(event: &'a nostr::Event, name: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|tag| {
        let parts = tag.as_slice();
        (parts.len() >= 2 && parts[0] == name).then(|| parts[1].as_str())
    })
}

fn parse_channel_uuid(channel_id: &str) -> Result<Uuid, String> {
    Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{ownership_shares_from_contributions, parse_contribution_message};

    #[test]
    fn parses_hive_contribution_message() {
        let pubkey = "a".repeat(64);
        let message = format!("sprout-hive-contribution:v1:channel-1:{pubkey}");

        assert_eq!(
            parse_contribution_message(&message, "channel-1"),
            Some(pubkey)
        );
    }

    #[test]
    fn ignores_contribution_for_other_channel() {
        let pubkey = "a".repeat(64);
        let message = format!("sprout-hive-contribution:v1:channel-1:{pubkey}");

        assert_eq!(parse_contribution_message(&message, "channel-2"), None);
    }

    #[test]
    fn ownership_shares_are_proportional_to_contributions() {
        let alice = "a".repeat(64);
        let bob = "b".repeat(64);
        let shares = ownership_shares_from_contributions(BTreeMap::from([
            (alice.clone(), 700),
            (bob.clone(), 300),
        ]));

        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].member_pubkey.as_deref(), Some(alice.as_str()));
        assert_eq!(shares[0].amount_sats, 700);
        assert_eq!(shares[0].ownership_percent, 70.0);
        assert_eq!(shares[1].member_pubkey.as_deref(), Some(bob.as_str()));
        assert_eq!(shares[1].amount_sats, 300);
        assert_eq!(shares[1].ownership_percent, 30.0);
    }
}
