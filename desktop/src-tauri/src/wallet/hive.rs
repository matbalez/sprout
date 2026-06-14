use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

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
use nostr::{Event, EventId};
use serde_json::Value;
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

use super::{
    discovery::{resolve_bolt12_offer_for_pubkey, resolve_send_payable},
    types::{
        HiveChannelContributionShare, HiveChannelPayoutExecution, HiveChannelPayoutFailure,
        HiveChannelPayoutPayment, HiveChannelPayoutPreview, HiveChannelPayoutRecipient,
        HiveChannelPayoutShare, HiveChannelWalletSummary, WalletPaymentResult, WalletTransaction,
        DEFAULT_TRANSACTION_LIMIT, HIVE_CHANNELS_DIR_NAME, LEXE_DATA_DIR_NAME,
        MAX_TRANSACTION_LIMIT, OFFER_FILE_NAME, SEED_FILE_NAME, WALLET_BOLT12_OFFER_DESCRIPTION,
        WALLET_DIR_NAME,
    },
};

const HIVE_OFFER_CREATE_ATTEMPTS: usize = 4;
const HIVE_OFFER_CREATE_RETRY_DELAY_MS: u64 = 750;
const CONTRIBUTION_MESSAGE_PREFIX: &str = "sprout-hive-contribution:v1:";
const HIVE_PAYOUT_CLIENT_MARKER: &str = "sprout-hive-payout:v1";
const HIVE_PAYOUT_HISTORY_LIMIT: usize = 1000;
const HIVE_CONTRIBUTION_HISTORY_LIMIT: usize = 1000;

struct HiveChannelWalletStorage {
    root_dir: PathBuf,
    data_dir: PathBuf,
    seed_path: PathBuf,
    offer_path: PathBuf,
}

pub(crate) struct HiveChannelWalletSetup {
    pub(crate) bolt12_offer: Option<String>,
}

#[derive(Clone, Debug)]
struct HivePaymentRecord {
    id: String,
    amount_sats: u64,
    created_at_ms: u64,
    contributor_pubkey: Option<String>,
    is_revenue: bool,
}

#[derive(Clone, Debug)]
struct HiveContributionRecord {
    id: String,
    contributor_pubkey: String,
    amount_sats: u64,
}

#[derive(Clone, Debug)]
struct ContributionMessage {
    contributor_pubkey: String,
    contribution_id: Option<String>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct PaidRevenueShareKey {
    revenue_payment_id: String,
    recipient_pubkey: String,
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
        return build_seedless_hive_channel_wallet_summary(
            &state,
            channel_id,
            channel_uuid,
            &storage,
        )
        .await;
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
    let ownership_shares =
        wallet_backed_ownership_shares(&state, &wallet, &channel_id, channel_uuid).await?;
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
        &info,
        balances,
        ownership_shares,
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
    let ownership_shares =
        wallet_backed_ownership_shares(&state, &wallet, &channel_id, channel_uuid).await?;

    build_hive_channel_wallet_summary(
        channel_id,
        offer,
        &storage,
        &info,
        balances,
        ownership_shares,
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
pub async fn preview_hive_channel_wallet_payouts(
    channel_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HiveChannelPayoutPreview, String> {
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
    let paid_shares = paid_hive_payout_share_keys(&state, &channel_id).await?;
    build_hive_payout_preview(&state, &wallet, &channel_id, &paid_shares).await
}

#[tauri::command]
pub async fn execute_hive_channel_wallet_payouts(
    channel_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<HiveChannelPayoutExecution, String> {
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
    let paid_shares = paid_hive_payout_share_keys(&state, &channel_id).await?;
    let preview = build_hive_payout_preview(&state, &wallet, &channel_id, &paid_shares).await?;
    if preview.total_payout_sats == 0 {
        return Ok(HiveChannelPayoutExecution {
            channel_id,
            status: "nothing_to_pay".to_string(),
            total_paid_sats: 0,
            paid: vec![],
            failed: None,
            remaining_preview: preview,
        });
    }

    if let Some(recipient) = preview
        .recipients
        .iter()
        .find(|recipient| recipient.bolt12_offer.is_none())
    {
        return Ok(HiveChannelPayoutExecution {
            channel_id,
            status: "blocked".to_string(),
            total_paid_sats: 0,
            paid: vec![],
            failed: Some(HiveChannelPayoutFailure {
                member_pubkey: Some(recipient.member_pubkey.clone()),
                amount_sats: Some(recipient.amount_sats),
                error: "recipient does not have a published BOLT12 offer".to_string(),
            }),
            remaining_preview: preview,
        });
    }

    let mut paid = Vec::new();
    let mut failed = None;
    for recipient in &preview.recipients {
        let Some(offer) = recipient.bolt12_offer.as_ref() else {
            continue;
        };
        let amount = amount_from_sats(recipient.amount_sats)?;
        let response = match wallet
            .pay(PayRequest {
                payable: offer.clone(),
                amount: Some(amount),
                message: Some(format!("sprout-hive-payout:v1:{channel_id}")),
                personal_note: Some(format!(
                    "Sprout hive channel payout {channel_id} to {}",
                    recipient.member_pubkey
                )),
            })
            .await
        {
            Ok(response) => response,
            Err(error) => {
                failed = Some(HiveChannelPayoutFailure {
                    member_pubkey: Some(recipient.member_pubkey.clone()),
                    amount_sats: Some(recipient.amount_sats),
                    error: format!("send hive payout: {error}"),
                });
                break;
            }
        };

        if response.status != PaymentStatus::Completed {
            failed = Some(HiveChannelPayoutFailure {
                member_pubkey: Some(recipient.member_pubkey.clone()),
                amount_sats: Some(recipient.amount_sats),
                error: payment_status_message(response.status_msg.as_str()),
            });
            break;
        }

        let payment_id = response.index.to_string();
        let message_event_id =
            match publish_hive_payout_message(&state, channel_uuid, recipient, &payment_id).await {
                Ok(event_id) => Some(event_id),
                Err(error) => {
                    paid.push(HiveChannelPayoutPayment {
                        member_pubkey: recipient.member_pubkey.clone(),
                        amount_sats: recipient.amount_sats,
                        payment_id,
                        message_event_id: None,
                    });
                    failed = Some(HiveChannelPayoutFailure {
                        member_pubkey: Some(recipient.member_pubkey.clone()),
                        amount_sats: Some(recipient.amount_sats),
                        error,
                    });
                    break;
                }
            };

        paid.push(HiveChannelPayoutPayment {
            member_pubkey: recipient.member_pubkey.clone(),
            amount_sats: recipient.amount_sats,
            payment_id,
            message_event_id,
        });
    }

    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync hive Lexe payments after payout: {error}"))?;
    let refreshed_paid_shares = paid_hive_payout_share_keys(&state, &channel_id)
        .await
        .unwrap_or_default();
    let remaining_preview =
        build_hive_payout_preview(&state, &wallet, &channel_id, &refreshed_paid_shares).await?;
    let total_paid_sats = paid.iter().map(|payment| payment.amount_sats).sum();
    let status = if failed.is_some() {
        "partial"
    } else {
        "completed"
    }
    .to_string();

    Ok(HiveChannelPayoutExecution {
        channel_id,
        status,
        total_paid_sats,
        paid,
        failed,
        remaining_preview,
    })
}

#[tauri::command]
pub async fn send_hive_channel_wallet_payment(
    channel_id: String,
    amount_sats: u64,
    payable: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletPaymentResult, String> {
    let channel_uuid = parse_channel_uuid(&channel_id)?;
    if amount_sats == 0 {
        return Err("hive channel payment amount must be greater than 0".to_string());
    }
    let storage = HiveChannelWalletStorage::from_app(&app, channel_uuid)?;
    if !storage.seed_path.exists() {
        return Err("hive wallet seed is not stored on this machine".to_string());
    }

    let payable = resolve_send_payable(&state, &payable).await?;
    let wallet = ensure_hive_wallet(&app, &state, channel_uuid).await?;
    let amount = amount_from_sats(amount_sats)?;
    let response = wallet
        .pay(PayRequest {
            payable,
            amount: Some(amount),
            message: None,
            personal_note: Some(format!("Sprout hive channel payment {channel_id}")),
        })
        .await
        .map_err(|error| format!("send hive channel wallet payment: {error}"))?;
    if response.status != PaymentStatus::Completed {
        let status_message = payment_status_message(response.status_msg.as_str());
        return Err(format!(
            "hive channel payment failed for {}: {status_message}",
            format_amount(amount_sats)
        ));
    }

    Ok(WalletPaymentResult {
        payment_id: response.index.to_string(),
        amount_sats,
    })
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
    let contribution_id = Uuid::new_v4().to_string();
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
            message: Some(contribution_message(
                &channel_id,
                &payer_pubkey,
                &contribution_id,
            )),
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
    publish_hive_contribution_marker_if_missing(
        &state,
        channel_uuid,
        &HiveContributionRecord {
            id: contribution_id,
            contributor_pubkey: payer_pubkey,
            amount_sats,
        },
    )
    .await;

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
    info: &lexe::types::command::NodeInfo,
    balances: WalletBalances,
    ownership_shares: Vec<HiveChannelContributionShare>,
    channel_uuid: Uuid,
) -> Result<HiveChannelWalletSummary, String> {
    log_hive_wallet_state(channel_uuid, info, &bolt12_offer);
    let total_contributed_sats = ownership_shares.iter().map(|share| share.amount_sats).sum();

    Ok(HiveChannelWalletSummary {
        channel_id,
        has_local_seed: true,
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

async fn build_seedless_hive_channel_wallet_summary(
    state: &AppState,
    channel_id: String,
    channel_uuid: Uuid,
    storage: &HiveChannelWalletStorage,
) -> Result<HiveChannelWalletSummary, String> {
    let ownership_shares = visible_hive_ownership_shares(state, &channel_id).await?;
    let total_contributed_sats = ownership_shares.iter().map(|share| share.amount_sats).sum();
    let bolt12_offer = resolve_hive_channel_offer(state, &channel_uuid.to_string())
        .await
        .unwrap_or_default();

    Ok(HiveChannelWalletSummary {
        channel_id,
        has_local_seed: false,
        seed_path: storage.seed_path.to_string_lossy().to_string(),
        balance_sats: 0,
        lightning_balance_sats: 0,
        lightning_sendable_balance_sats: 0,
        onchain_balance_sats: 0,
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

async fn build_hive_payout_preview(
    state: &AppState,
    wallet: &LexeWallet,
    channel_id: &str,
    paid_shares: &HashSet<PaidRevenueShareKey>,
) -> Result<HiveChannelPayoutPreview, String> {
    let records = completed_hive_payment_records(wallet, channel_id)?;
    let mut preview = calculate_hive_payout_preview(channel_id.to_string(), records, paid_shares);

    for recipient in &mut preview.recipients {
        recipient.bolt12_offer =
            resolve_bolt12_offer_for_pubkey(state, &recipient.member_pubkey).await?;
    }

    Ok(preview)
}

fn completed_hive_payment_records(
    wallet: &LexeWallet,
    channel_id: &str,
) -> Result<Vec<HivePaymentRecord>, String> {
    let response = wallet
        .list_payments(
            &PaymentFilter::All,
            Some(Order::Asc),
            Some(HIVE_PAYOUT_HISTORY_LIMIT),
            None,
        )
        .map_err(|error| format!("list hive Lexe payments: {error}"))?;

    Ok(response
        .payments
        .into_iter()
        .filter_map(|payment| hive_payment_record_from_payment(payment, channel_id))
        .collect())
}

fn hive_payment_record_from_payment(
    payment: Payment,
    channel_id: &str,
) -> Option<HivePaymentRecord> {
    if payment.direction != PaymentDirection::Inbound || payment.status != PaymentStatus::Completed
    {
        return None;
    }
    let amount_sats = payment.amount.map(|amount| amount.sats_u64())?;
    if amount_sats == 0 {
        return None;
    }

    let contributor_pubkey = contribution_pubkey(&payment, channel_id);
    Some(HivePaymentRecord {
        id: payment.index.to_string(),
        amount_sats,
        created_at_ms: payment.created_at.to_millis(),
        is_revenue: contributor_pubkey.is_none(),
        contributor_pubkey,
    })
}

fn calculate_hive_payout_preview(
    channel_id: String,
    records: Vec<HivePaymentRecord>,
    paid_shares: &HashSet<PaidRevenueShareKey>,
) -> HiveChannelPayoutPreview {
    let mut stakes = BTreeMap::<String, u64>::new();
    let mut recipients = BTreeMap::<String, HiveChannelPayoutRecipient>::new();
    let mut total_unattributed_revenue_sats = 0;
    let mut unpaid_revenue_ids = BTreeSet::<String>::new();
    let mut skipped_no_owner_revenue_count = 0;
    let mut already_paid_share_count = 0;

    let mut records = records;
    records.sort_by(|left, right| {
        left.created_at_ms
            .cmp(&right.created_at_ms)
            .then_with(|| payment_record_sort_rank(left).cmp(&payment_record_sort_rank(right)))
            .then_with(|| left.id.cmp(&right.id))
    });

    for record in records {
        if let Some(contributor_pubkey) = record.contributor_pubkey {
            *stakes.entry(contributor_pubkey).or_default() += record.amount_sats;
            continue;
        }

        if !record.is_revenue {
            continue;
        }
        total_unattributed_revenue_sats += record.amount_sats;
        if stakes.is_empty() {
            skipped_no_owner_revenue_count += 1;
            continue;
        }

        for (recipient_pubkey, amount_sats) in allocate_revenue_shares(record.amount_sats, &stakes)
        {
            let key = PaidRevenueShareKey {
                revenue_payment_id: record.id.clone(),
                recipient_pubkey: recipient_pubkey.clone(),
            };
            if paid_shares.contains(&key) {
                already_paid_share_count += 1;
                continue;
            }
            unpaid_revenue_ids.insert(record.id.clone());
            let recipient = recipients
                .entry(recipient_pubkey.clone())
                .or_insert_with(|| HiveChannelPayoutRecipient {
                    member_pubkey: recipient_pubkey.clone(),
                    amount_sats: 0,
                    bolt12_offer: None,
                    shares: vec![],
                });
            recipient.amount_sats += amount_sats;
            recipient.shares.push(HiveChannelPayoutShare {
                revenue_payment_id: record.id.clone(),
                revenue_amount_sats: record.amount_sats,
                revenue_created_at_ms: record.created_at_ms,
                amount_sats,
            });
        }
    }

    let recipients = recipients.into_values().collect::<Vec<_>>();
    let total_payout_sats = recipients
        .iter()
        .map(|recipient| recipient.amount_sats)
        .sum();

    HiveChannelPayoutPreview {
        channel_id,
        total_unattributed_revenue_sats,
        total_payout_sats,
        unpaid_revenue_count: unpaid_revenue_ids.len(),
        skipped_no_owner_revenue_count,
        already_paid_share_count,
        recipients,
    }
}

fn payment_record_sort_rank(record: &HivePaymentRecord) -> u8 {
    if record.contributor_pubkey.is_some() {
        1
    } else {
        0
    }
}

fn allocate_revenue_shares(
    revenue_amount_sats: u64,
    stakes: &BTreeMap<String, u64>,
) -> Vec<(String, u64)> {
    let total_stake: u64 = stakes.values().sum();
    if revenue_amount_sats == 0 || total_stake == 0 {
        return vec![];
    }

    let mut allocations = stakes
        .iter()
        .filter_map(|(pubkey, stake)| {
            if *stake == 0 {
                return None;
            }
            let weighted = revenue_amount_sats as u128 * *stake as u128;
            let total = total_stake as u128;
            Some((pubkey.clone(), (weighted / total) as u64, weighted % total))
        })
        .collect::<Vec<_>>();
    let allocated: u64 = allocations.iter().map(|(_, amount, _)| *amount).sum();
    let remaining = revenue_amount_sats.saturating_sub(allocated) as usize;

    allocations.sort_by(|left, right| {
        Reverse(left.2)
            .cmp(&Reverse(right.2))
            .then_with(|| left.0.cmp(&right.0))
    });
    for (_, amount, _) in allocations.iter_mut().take(remaining) {
        *amount += 1;
    }
    allocations.sort_by(|left, right| left.0.cmp(&right.0));

    allocations
        .into_iter()
        .filter_map(|(pubkey, amount, _)| (amount > 0).then_some((pubkey, amount)))
        .collect()
}

async fn paid_hive_payout_share_keys(
    state: &AppState,
    channel_id: &str,
) -> Result<HashSet<PaidRevenueShareKey>, String> {
    let relay_events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [9],
            "#h": [channel_id],
            "limit": HIVE_PAYOUT_HISTORY_LIMIT,
        })],
    )
    .await?;

    Ok(relay_events
        .iter()
        .flat_map(paid_hive_payout_share_keys_from_event)
        .collect())
}

fn paid_hive_payout_share_keys_from_event(event: &nostr::Event) -> Vec<PaidRevenueShareKey> {
    event
        .tags
        .iter()
        .filter_map(paid_hive_payout_share_key)
        .collect()
}

fn paid_hive_payout_share_key(tag: &nostr::Tag) -> Option<PaidRevenueShareKey> {
    let parts = tag.as_slice();
    if parts.len() < 5 || parts[0] != "client" || parts[1] != HIVE_PAYOUT_CLIENT_MARKER {
        return None;
    }
    let revenue_payment_id = parts[2].trim();
    let recipient_pubkey = parts[3].trim().to_ascii_lowercase();
    if revenue_payment_id.is_empty()
        || recipient_pubkey.len() != 64
        || !recipient_pubkey.chars().all(|ch| ch.is_ascii_hexdigit())
    {
        return None;
    }
    Some(PaidRevenueShareKey {
        revenue_payment_id: revenue_payment_id.to_string(),
        recipient_pubkey,
    })
}

async fn publish_hive_payout_message(
    state: &AppState,
    channel_id: Uuid,
    recipient: &HiveChannelPayoutRecipient,
    payment_id: &str,
) -> Result<String, String> {
    let amount = format_amount(recipient.amount_sats);
    let mention_label = hive_member_mention_label(state, &recipient.member_pubkey).await;
    let content = format!("➡️ paid out {amount} to @{mention_label}");
    let client_tags = recipient
        .shares
        .iter()
        .map(|share| {
            vec![
                "client".to_string(),
                HIVE_PAYOUT_CLIENT_MARKER.to_string(),
                share.revenue_payment_id.clone(),
                recipient.member_pubkey.clone(),
                share.amount_sats.to_string(),
                payment_id.to_string(),
            ]
        })
        .collect::<Vec<_>>();
    let mentions = [recipient.member_pubkey.as_str()];
    let builder = events::build_message_with_client_tags(
        channel_id,
        &content,
        None,
        None,
        &mentions,
        &[],
        &[],
        &[],
        None,
        &[],
        &client_tags,
    )
    .map_err(|error| format!("build hive payout message: {error}"))?;
    submit_event(builder, state)
        .await
        .map(|response| response.event_id)
        .map_err(|error| format!("publish hive payout message: {error}"))
}

async fn hive_member_mention_label(state: &AppState, pubkey: &str) -> String {
    match query_relay(
        state,
        &[serde_json::json!({
            "kinds": [0],
            "authors": [pubkey],
            "limit": 20,
        })],
    )
    .await
    {
        Ok(events) => events
            .iter()
            .max_by_key(|event| event.created_at.as_secs())
            .and_then(profile_mention_label_from_event)
            .unwrap_or_else(|| short_pubkey(pubkey).to_string()),
        Err(error) => {
            eprintln!("buzz-desktop: failed to resolve hive payout mention label: {error}");
            short_pubkey(pubkey).to_string()
        }
    }
}

fn profile_mention_label_from_event(event: &Event) -> Option<String> {
    let metadata: Value = serde_json::from_str(&event.content).ok()?;
    ["display_name", "displayName", "name", "username"]
        .into_iter()
        .filter_map(|field| metadata.get(field).and_then(Value::as_str))
        .find_map(clean_mention_label)
}

fn clean_mention_label(value: &str) -> Option<String> {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let without_at = collapsed.trim_start_matches('@').trim();
    if without_at.is_empty() {
        None
    } else {
        Some(without_at.chars().take(80).collect())
    }
}

fn short_pubkey(pubkey: &str) -> &str {
    pubkey.get(..8).unwrap_or(pubkey)
}

async fn wallet_backed_ownership_shares(
    state: &AppState,
    wallet: &LexeWallet,
    channel_id: &str,
    channel_uuid: Uuid,
) -> Result<Vec<HiveChannelContributionShare>, String> {
    let records = completed_hive_contribution_records(wallet, channel_id)?;
    publish_hive_contribution_markers_if_missing(state, channel_uuid, &records).await;
    Ok(ownership_shares_from_contribution_records(records))
}

async fn visible_hive_ownership_shares(
    state: &AppState,
    channel_id: &str,
) -> Result<Vec<HiveChannelContributionShare>, String> {
    let records = hive_contribution_records_from_relay(state, channel_id).await?;
    Ok(ownership_shares_from_contribution_records(records))
}

fn completed_hive_contribution_records(
    wallet: &LexeWallet,
    channel_id: &str,
) -> Result<Vec<HiveContributionRecord>, String> {
    let response = wallet
        .list_payments(
            &PaymentFilter::All,
            Some(Order::Asc),
            Some(HIVE_PAYOUT_HISTORY_LIMIT),
            None,
        )
        .map_err(|error| format!("list hive Lexe payments: {error}"))?;
    let mut records = Vec::new();

    for payment in response.payments {
        if payment.direction != PaymentDirection::Inbound
            || payment.status != PaymentStatus::Completed
        {
            continue;
        }
        let Some(amount_sats) = payment.amount.map(|amount| amount.sats_u64()) else {
            continue;
        };
        if amount_sats == 0 {
            continue;
        }
        let Some(contribution) = contribution_from_payment(&payment, channel_id) else {
            continue;
        };
        records.push(HiveContributionRecord {
            id: contribution
                .contribution_id
                .unwrap_or_else(|| format!("lexe:{}", payment.index)),
            contributor_pubkey: contribution.contributor_pubkey,
            amount_sats,
        });
    }

    Ok(records)
}

fn ownership_shares_from_contribution_records(
    records: Vec<HiveContributionRecord>,
) -> Vec<HiveChannelContributionShare> {
    let mut by_pubkey: BTreeMap<String, u64> = BTreeMap::new();
    for record in records {
        *by_pubkey.entry(record.contributor_pubkey).or_default() += record.amount_sats;
    }
    ownership_shares_from_contributions(by_pubkey)
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

fn contribution_message(channel_id: &str, payer_pubkey: &str, contribution_id: &str) -> String {
    format!("{CONTRIBUTION_MESSAGE_PREFIX}{channel_id}:{payer_pubkey}:{contribution_id}")
}

fn contribution_pubkey(payment: &Payment, channel_id: &str) -> Option<String> {
    contribution_from_payment(payment, channel_id)
        .map(|contribution| contribution.contributor_pubkey)
}

fn contribution_from_payment(payment: &Payment, channel_id: &str) -> Option<ContributionMessage> {
    payment
        .message
        .as_deref()
        .and_then(|message| parse_contribution_message_parts(message, channel_id))
}

fn parse_contribution_message_parts(
    message: &str,
    channel_id: &str,
) -> Option<ContributionMessage> {
    let remainder = message.strip_prefix(CONTRIBUTION_MESSAGE_PREFIX)?;
    let mut parts = remainder.split(':');
    let message_channel_id = parts.next()?;
    let pubkey = parts.next()?;
    if message_channel_id != channel_id {
        return None;
    }
    let pubkey = pubkey.trim();
    if pubkey.len() != 64 || !pubkey.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let contribution_id = match parts.next() {
        None => None,
        Some(value) => {
            let value = value.trim();
            if !is_valid_hive_contribution_id(value) || parts.next().is_some() {
                return None;
            }
            Some(value.to_string())
        }
    };

    Some(ContributionMessage {
        contributor_pubkey: pubkey.to_ascii_lowercase(),
        contribution_id,
    })
}

fn is_valid_hive_contribution_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '.'))
}

async fn publish_hive_contribution_markers_if_missing(
    state: &AppState,
    channel_id: Uuid,
    records: &[HiveContributionRecord],
) {
    if records.is_empty() {
        return;
    }

    let channel_id_string = channel_id.to_string();
    let existing_ids = match hive_contribution_ids_from_relay(state, &channel_id_string).await {
        Ok(ids) => ids,
        Err(error) => {
            eprintln!(
                "buzz-desktop: failed to query hive channel {channel_id} contribution markers: {error}"
            );
            HashSet::new()
        }
    };
    let metadata_event_id = match resolve_channel_metadata_event_id(state, channel_id).await {
        Ok(event_id) => event_id,
        Err(error) => {
            eprintln!(
                "buzz-desktop: failed to resolve hive channel {channel_id} metadata for contribution marker: {error}"
            );
            return;
        }
    };

    for record in records {
        if existing_ids.contains(&record.id) {
            continue;
        }
        if let Err(error) =
            publish_hive_contribution_marker(state, channel_id, metadata_event_id, record).await
        {
            eprintln!(
                "buzz-desktop: failed to publish hive channel {channel_id} contribution marker {}: {error}",
                record.id
            );
        }
    }
}

async fn publish_hive_contribution_marker_if_missing(
    state: &AppState,
    channel_id: Uuid,
    record: &HiveContributionRecord,
) {
    publish_hive_contribution_markers_if_missing(state, channel_id, std::slice::from_ref(record))
        .await;
}

async fn publish_hive_contribution_marker(
    state: &AppState,
    channel_id: Uuid,
    metadata_event_id: EventId,
    record: &HiveContributionRecord,
) -> Result<(), String> {
    let builder = events::build_hive_channel_contribution_metadata(
        channel_id,
        metadata_event_id,
        &record.id,
        &record.contributor_pubkey,
        record.amount_sats,
    )
    .map_err(|error| format!("build hive contribution metadata: {error}"))?;
    submit_event(builder, state)
        .await
        .map(|_| ())
        .map_err(|error| format!("publish hive contribution metadata: {error}"))
}

async fn hive_contribution_ids_from_relay(
    state: &AppState,
    channel_id: &str,
) -> Result<HashSet<String>, String> {
    Ok(hive_contribution_records_from_relay(state, channel_id)
        .await?
        .into_iter()
        .map(|record| record.id)
        .collect())
}

async fn hive_contribution_records_from_relay(
    state: &AppState,
    channel_id: &str,
) -> Result<Vec<HiveContributionRecord>, String> {
    let relay_events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [7],
            "#h": [channel_id],
            "limit": HIVE_CONTRIBUTION_HISTORY_LIMIT,
        })],
    )
    .await?;
    let channel_owner_pubkey = resolve_channel_owner_pubkey(state, channel_id).await.ok();
    let mut by_id = BTreeMap::<String, (u64, String, HiveContributionRecord)>::new();

    for event in relay_events {
        if event.kind.as_u16() != 7 || !events::is_hive_channel_contribution_content(&event.content)
        {
            continue;
        }
        let Some(record) = hive_contribution_record_from_event(&event) else {
            continue;
        };
        if !hive_contribution_marker_authorized(&event, &record, channel_owner_pubkey.as_deref()) {
            continue;
        }
        let sort_key = (event.created_at.as_secs(), event.id.to_hex());
        let entry = by_id
            .entry(record.id.clone())
            .or_insert_with(|| (sort_key.0, sort_key.1.clone(), record.clone()));
        if (sort_key.0, sort_key.1.as_str()) >= (entry.0, entry.1.as_str()) {
            *entry = (sort_key.0, sort_key.1, record);
        }
    }

    Ok(by_id.into_values().map(|(_, _, record)| record).collect())
}

fn hive_contribution_record_from_event(event: &Event) -> Option<HiveContributionRecord> {
    let id = first_tag_value(event, "hive_contribution_id")?.trim();
    if !is_valid_hive_contribution_id(id) {
        None
    } else {
        let contributor_pubkey = first_tag_value(event, "hive_contributor_pubkey")?
            .trim()
            .to_ascii_lowercase();
        if contributor_pubkey.len() != 64
            || !contributor_pubkey.chars().all(|ch| ch.is_ascii_hexdigit())
        {
            return None;
        }
        let amount_sats = first_tag_value(event, "hive_contribution_amount_sats")?
            .parse::<u64>()
            .ok()?;
        if amount_sats == 0 {
            return None;
        }
        Some(HiveContributionRecord {
            id: id.to_string(),
            contributor_pubkey,
            amount_sats,
        })
    }
}

fn hive_contribution_marker_authorized(
    event: &Event,
    record: &HiveContributionRecord,
    channel_owner_pubkey: Option<&str>,
) -> bool {
    let author = event.pubkey.to_hex();
    author == record.contributor_pubkey || channel_owner_pubkey == Some(author.as_str())
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
    resolve_channel_metadata_event(state, channel_id)
        .await
        .map(|event| event.id)
}

async fn resolve_channel_owner_pubkey(
    state: &AppState,
    channel_id: &str,
) -> Result<String, String> {
    let channel_uuid = parse_channel_uuid(channel_id)?;
    resolve_channel_metadata_event(state, channel_uuid)
        .await
        .map(|event| event.pubkey.to_hex())
}

async fn resolve_channel_metadata_event(
    state: &AppState,
    channel_id: Uuid,
) -> Result<Event, String> {
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
        .map(|event| event)
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
    use std::collections::{BTreeMap, HashSet};

    use super::{
        allocate_revenue_shares, calculate_hive_payout_preview,
        ownership_shares_from_contributions, parse_contribution_message_parts, HivePaymentRecord,
        PaidRevenueShareKey,
    };

    #[test]
    fn parses_legacy_hive_contribution_message() {
        let pubkey = "a".repeat(64);
        let message = format!("sprout-hive-contribution:v1:channel-1:{pubkey}");
        let parsed = parse_contribution_message_parts(&message, "channel-1").unwrap();

        assert_eq!(parsed.contributor_pubkey, pubkey);
        assert_eq!(parsed.contribution_id, None);
    }

    #[test]
    fn parses_hive_contribution_message_with_id() {
        let pubkey = "a".repeat(64);
        let message = format!("sprout-hive-contribution:v1:channel-1:{pubkey}:contribution-1");
        let parsed = parse_contribution_message_parts(&message, "channel-1").unwrap();

        assert_eq!(parsed.contributor_pubkey, pubkey);
        assert_eq!(parsed.contribution_id.as_deref(), Some("contribution-1"));
    }

    #[test]
    fn ignores_contribution_for_other_channel() {
        let pubkey = "a".repeat(64);
        let message = format!("sprout-hive-contribution:v1:channel-1:{pubkey}");

        assert!(parse_contribution_message_parts(&message, "channel-2").is_none());
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

    #[test]
    fn allocates_revenue_remainders_deterministically() {
        let alice = "a".repeat(64);
        let bob = "b".repeat(64);
        let allocations = allocate_revenue_shares(
            101,
            &BTreeMap::from([(alice.clone(), 70), (bob.clone(), 30)]),
        );

        assert_eq!(allocations, vec![(alice, 71), (bob, 30)]);
    }

    #[test]
    fn payout_preview_uses_contemporaneous_ownership_and_paid_markers() {
        let alice = "a".repeat(64);
        let bob = "b".repeat(64);
        let records = vec![
            HivePaymentRecord {
                id: "revenue-before-owners".to_string(),
                amount_sats: 50,
                created_at_ms: 1,
                contributor_pubkey: None,
                is_revenue: true,
            },
            HivePaymentRecord {
                id: "alice-contribution".to_string(),
                amount_sats: 70,
                created_at_ms: 2,
                contributor_pubkey: Some(alice.clone()),
                is_revenue: false,
            },
            HivePaymentRecord {
                id: "bob-contribution".to_string(),
                amount_sats: 30,
                created_at_ms: 3,
                contributor_pubkey: Some(bob.clone()),
                is_revenue: false,
            },
            HivePaymentRecord {
                id: "revenue-one".to_string(),
                amount_sats: 101,
                created_at_ms: 4,
                contributor_pubkey: None,
                is_revenue: true,
            },
            HivePaymentRecord {
                id: "bob-later-contribution".to_string(),
                amount_sats: 100,
                created_at_ms: 5,
                contributor_pubkey: Some(bob.clone()),
                is_revenue: false,
            },
            HivePaymentRecord {
                id: "revenue-two".to_string(),
                amount_sats: 100,
                created_at_ms: 6,
                contributor_pubkey: None,
                is_revenue: true,
            },
        ];
        let paid = HashSet::from([PaidRevenueShareKey {
            revenue_payment_id: "revenue-one".to_string(),
            recipient_pubkey: alice.clone(),
        }]);

        let preview = calculate_hive_payout_preview("channel-1".to_string(), records, &paid);

        assert_eq!(preview.total_unattributed_revenue_sats, 251);
        assert_eq!(preview.skipped_no_owner_revenue_count, 1);
        assert_eq!(preview.already_paid_share_count, 1);
        assert_eq!(preview.unpaid_revenue_count, 2);
        assert_eq!(preview.total_payout_sats, 130);
        assert_eq!(preview.recipients.len(), 2);
        assert_eq!(preview.recipients[0].member_pubkey, alice);
        assert_eq!(preview.recipients[0].amount_sats, 35);
        assert_eq!(preview.recipients[1].member_pubkey, bob);
        assert_eq!(preview.recipients[1].amount_sats, 95);
    }

    #[test]
    fn payout_preview_does_not_count_same_timestamp_contributions() {
        let alice = "a".repeat(64);
        let records = vec![
            HivePaymentRecord {
                id: "alice-contribution".to_string(),
                amount_sats: 100,
                created_at_ms: 10,
                contributor_pubkey: Some(alice),
                is_revenue: false,
            },
            HivePaymentRecord {
                id: "revenue".to_string(),
                amount_sats: 50,
                created_at_ms: 10,
                contributor_pubkey: None,
                is_revenue: true,
            },
        ];

        let preview =
            calculate_hive_payout_preview("channel-1".to_string(), records, &HashSet::new());

        assert_eq!(preview.total_payout_sats, 0);
        assert_eq!(preview.skipped_no_owner_revenue_count, 1);
    }
}
