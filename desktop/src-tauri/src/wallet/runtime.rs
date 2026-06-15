use std::sync::Arc;

use lexe::{
    config::WalletEnvConfig,
    types::{
        auth::{ClientCredentials, CredentialsRef, RootSeed},
        command::CreateOfferRequest,
    },
    wallet::LexeWallet,
};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager};

use crate::{
    app_state::AppState,
    events,
    relay::{query_relay, submit_event},
};

use super::{
    cashu::CashuWallet,
    mdk::MdkWallet,
    provider::{WalletProvider, WalletProviderHandle},
    storage::{
        current_pubkey, load_cashu_mint_url, load_existing_client_credential, load_root_seed,
        load_wallet_provider, load_wallet_source, write_atomic_text, WalletStorage,
    },
    types::{WalletSource, WALLET_BOLT12_OFFER_DESCRIPTION},
};

pub(super) async fn ensure_wallet(
    app: &AppHandle,
    state: &AppState,
) -> Result<WalletProviderHandle, String> {
    let storage = WalletStorage::from_app(app)?;
    storage.ensure_dirs()?;
    let provider = load_wallet_provider(&storage)?;
    let source = effective_wallet_source(provider, load_wallet_source(&storage)?);
    let cashu_mint_url = if provider == WalletProvider::Cashu {
        Some(load_cashu_mint_url(&storage)?)
    } else {
        None
    };

    let mut guard = state.wallet_state.wallet.lock().await;
    let mut provider_guard = state.wallet_state.wallet_provider.lock().await;
    let mut source_guard = state.wallet_state.wallet_source.lock().await;
    let mut cashu_mint_guard = state.wallet_state.cashu_mint_url.lock().await;
    if provider_guard.as_ref() != Some(&provider)
        || source_guard.as_ref() != Some(&source)
        || *cashu_mint_guard != cashu_mint_url
    {
        *guard = None;
        *state.wallet_state.summary.lock().await = None;
    }

    let credentials_available = match (provider, source) {
        (WalletProvider::Lexe, WalletSource::Default) => storage.seed_path.exists(),
        (WalletProvider::Lexe, WalletSource::Existing) => {
            load_existing_client_credential(&storage)?.is_some()
        }
        (WalletProvider::Mdk, WalletSource::Default) => true,
        (WalletProvider::Mdk, WalletSource::Existing) => false,
        (WalletProvider::Cashu, WalletSource::Default) => true,
        (WalletProvider::Cashu, WalletSource::Existing) => false,
    };
    if credentials_available {
        if let Some(wallet) = guard.as_ref() {
            *provider_guard = Some(wallet.provider());
            *source_guard = Some(source);
            *cashu_mint_guard = cashu_mint_url;
            return Ok(wallet.clone());
        }
    } else {
        *guard = None;
        *state.wallet_state.summary.lock().await = None;
    }

    let wallet = match (provider, source) {
        (WalletProvider::Lexe, WalletSource::Default) => {
            WalletProviderHandle::new_lexe(load_default_wallet(&storage).await?)
        }
        (WalletProvider::Lexe, WalletSource::Existing) => {
            WalletProviderHandle::new_lexe(load_existing_wallet(&storage)?)
        }
        (WalletProvider::Mdk, WalletSource::Default) => {
            WalletProviderHandle::new_mdk(MdkWallet::load_or_create(&storage).await?)
        }
        (WalletProvider::Mdk, WalletSource::Existing) => {
            return Err("MDK Agent Wallet does not support existing Lexe credentials".to_string())
        }
        (WalletProvider::Cashu, WalletSource::Default) => WalletProviderHandle::new_cashu(
            CashuWallet::load_or_create(
                &storage,
                cashu_mint_url
                    .as_deref()
                    .ok_or_else(|| "Cashu mint selection is missing".to_string())?,
            )
            .await?,
        ),
        (WalletProvider::Cashu, WalletSource::Existing) => {
            return Err("Cashu does not support existing Lexe credentials".to_string())
        }
    };

    *guard = Some(wallet.clone());
    *provider_guard = Some(wallet.provider());
    *source_guard = Some(source);
    *cashu_mint_guard = cashu_mint_url;
    Ok(wallet)
}

pub(super) async fn ensure_lexe_wallet(
    app: &AppHandle,
    state: &AppState,
) -> Result<Arc<LexeWallet>, String> {
    let storage = WalletStorage::from_app(app)?;
    storage.ensure_dirs()?;
    let source = load_wallet_source(&storage)?;

    let guard = state.wallet_state.wallet.lock().await;
    let provider_guard = state.wallet_state.wallet_provider.lock().await;
    let source_guard = state.wallet_state.wallet_source.lock().await;
    if provider_guard.as_ref() == Some(&WalletProvider::Lexe)
        && source_guard.as_ref() == Some(&source)
    {
        if let Some(wallet) = guard.as_ref() {
            return wallet.lexe_wallet();
        }
    }
    drop(source_guard);
    drop(provider_guard);
    drop(guard);

    let wallet = match source {
        WalletSource::Default => load_default_wallet(&storage).await?,
        WalletSource::Existing => load_existing_wallet(&storage)?,
    };
    Ok(Arc::new(wallet))
}

fn effective_wallet_source(provider: WalletProvider, source: WalletSource) -> WalletSource {
    if provider.capabilities().can_connect_existing_wallet {
        source
    } else {
        WalletSource::Default
    }
}

async fn load_default_wallet(storage: &WalletStorage) -> Result<LexeWallet, String> {
    let root_seed = match load_root_seed(storage)? {
        Some(seed) => seed,
        None => {
            storage.reset_for_new_root_seed()?;
            let seed = RootSeed::generate();
            seed.write_to_path(&storage.seed_path)
                .map_err(|error| format!("write wallet seed: {error}"))?;
            seed
        }
    };

    let wallet = LexeWallet::load_or_fresh(
        WalletEnvConfig::mainnet(),
        CredentialsRef::from(&root_seed),
        Some(storage.lexe_data_dir.clone()),
    )
    .map_err(|error| format!("load Lexe wallet: {error}"))?;

    wallet
        .signup(&root_seed, None)
        .await
        .map_err(|error| format!("Lexe signup/provisioning: {error}"))?;
    wallet
        .provision(CredentialsRef::from(&root_seed))
        .await
        .map_err(|error| format!("Lexe provision: {error}"))?;

    Ok(wallet)
}

fn load_existing_wallet(storage: &WalletStorage) -> Result<LexeWallet, String> {
    let client_credential = load_existing_client_credential(storage)?
        .ok_or_else(|| "Lexe SDK client credential is not saved".to_string())?;
    let client_credential = ClientCredentials::from_string(&client_credential)
        .map_err(|error| format!("parse Lexe client credential: {error}"))?;

    LexeWallet::load_or_fresh(
        WalletEnvConfig::mainnet(),
        CredentialsRef::from(&client_credential),
        Some(storage.existing_lexe_data_dir.clone()),
    )
    .map_err(|error| format!("load existing Lexe wallet: {error}"))
}

pub(super) async fn reset_cached_wallet_if_credentials_missing(
    app: &AppHandle,
    state: &AppState,
) -> Result<(), String> {
    let storage = WalletStorage::from_app(app)?;
    let provider = load_wallet_provider(&storage)?;
    let source = effective_wallet_source(provider, load_wallet_source(&storage)?);
    let credentials_exist = match (provider, source) {
        (WalletProvider::Lexe, WalletSource::Default) => storage.seed_path.exists(),
        (WalletProvider::Lexe, WalletSource::Existing) => {
            load_existing_client_credential(&storage)?.is_some()
        }
        (WalletProvider::Mdk, WalletSource::Default) => true,
        (WalletProvider::Mdk, WalletSource::Existing) => false,
        (WalletProvider::Cashu, WalletSource::Default) => true,
        (WalletProvider::Cashu, WalletSource::Existing) => false,
    };
    if credentials_exist {
        return Ok(());
    }

    clear_wallet_cache(state).await;
    Ok(())
}

pub(super) async fn clear_wallet_cache(state: &AppState) {
    *state.wallet_state.wallet.lock().await = None;
    *state.wallet_state.wallet_provider.lock().await = None;
    *state.wallet_state.wallet_source.lock().await = None;
    *state.wallet_state.cashu_mint_url.lock().await = None;
    *state.wallet_state.summary.lock().await = None;
}

pub(super) async fn ensure_bolt12_offer(
    app: &AppHandle,
    state: &AppState,
) -> Result<String, String> {
    let storage = WalletStorage::from_app(app)?;
    storage.ensure_dirs()?;
    let provider = load_wallet_provider(&storage)?;
    let source = effective_wallet_source(provider, load_wallet_source(&storage)?);
    let offer_path = if provider == WalletProvider::Cashu {
        storage.selected_cashu_storage()?.offer_path
    } else {
        storage.offer_path_for_provider_source(provider, source)
    };
    let cached_offer = std::fs::read_to_string(&offer_path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|offer| !offer.is_empty());

    if provider != WalletProvider::Cashu {
        if let Some(offer) = cached_offer {
            return Ok(offer);
        }
    }

    let wallet = ensure_wallet(app, state).await?;
    let offer = match wallet.provider() {
        WalletProvider::Lexe => {
            let wallet = wallet.lexe_wallet()?;
            let response = wallet
                .create_offer(CreateOfferRequest {
                    description: Some(WALLET_BOLT12_OFFER_DESCRIPTION.to_string()),
                    min_amount: None,
                    expiration_secs: None,
                })
                .await
                .map_err(|error| format!("create Lexe BOLT12 offer: {error}"))?;
            response.offer.to_string()
        }
        WalletProvider::Mdk => wallet.mdk_wallet()?.create_bolt12_offer().await?,
        WalletProvider::Cashu => {
            wallet
                .cashu_wallet()?
                .ensure_bolt12_offer(cached_offer.as_deref())
                .await?
        }
    };
    write_atomic_text(&offer_path, &offer)?;
    Ok(offer)
}

pub(super) fn spawn_current_profile_bolt12_offer_sync(
    app: &AppHandle,
    offer: String,
    context: &'static str,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = sync_current_profile_bolt12_offer(&state, &offer).await {
            eprintln!(
                "sprout-desktop: failed to sync wallet BOLT12 profile after {context}: {error}"
            );
        }
    });
}

pub(super) async fn sync_current_profile_bolt12_offer(
    state: &AppState,
    offer: &str,
) -> Result<(), String> {
    let offer = offer.trim();
    if offer.is_empty() {
        return Ok(());
    }

    let pubkey = current_pubkey(state)?;
    let profiles = query_relay(
        state,
        &[json!({
            "kinds": [0],
            "authors": [pubkey],
            "limit": 1
        })],
    )
    .await?;

    let mut metadata = profiles
        .first()
        .and_then(|event| serde_json::from_str::<Value>(&event.content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if profile_metadata_has_bolt12_offer(&metadata, offer) {
        return Ok(());
    }

    set_profile_metadata_bolt12_offer(&mut metadata, offer);
    submit_event(events::build_profile_metadata(metadata)?, state).await?;
    Ok(())
}

fn profile_metadata_has_bolt12_offer(metadata: &Map<String, Value>, offer: &str) -> bool {
    metadata_string(metadata, "bolt12_offer") == Some(offer)
        && metadata_string(metadata, "bolt12") == Some(offer)
        && metadata_object_string(metadata, "wallet", "bolt12_offer") == Some(offer)
        && metadata_object_string(metadata, "wallet", "bolt12") == Some(offer)
        && metadata
            .get("payments")
            .and_then(Value::as_object)
            .map_or(true, |payments| {
                payments.get("bolt12_offer").and_then(Value::as_str) == Some(offer)
                    && payments.get("bolt12").and_then(Value::as_str) == Some(offer)
            })
}

fn metadata_string<'a>(metadata: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    metadata.get(key).and_then(Value::as_str)
}

fn metadata_object_string<'a>(
    metadata: &'a Map<String, Value>,
    object_key: &str,
    key: &str,
) -> Option<&'a str> {
    metadata
        .get(object_key)
        .and_then(Value::as_object)
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
}

fn set_profile_metadata_bolt12_offer(metadata: &mut Map<String, Value>, offer: &str) {
    metadata.insert("bolt12_offer".into(), Value::String(offer.to_string()));
    metadata.insert("bolt12".into(), Value::String(offer.to_string()));

    let wallet = metadata
        .entry("wallet")
        .or_insert_with(|| Value::Object(Map::new()));
    if !wallet.is_object() {
        *wallet = Value::Object(Map::new());
    }
    if let Some(wallet) = wallet.as_object_mut() {
        wallet.insert("bolt12_offer".into(), Value::String(offer.to_string()));
        wallet.insert("bolt12".into(), Value::String(offer.to_string()));
    }

    if let Some(payments) = metadata.get_mut("payments").and_then(Value::as_object_mut) {
        payments.insert("bolt12_offer".into(), Value::String(offer.to_string()));
        payments.insert("bolt12".into(), Value::String(offer.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Map, Value};

    use super::{profile_metadata_has_bolt12_offer, set_profile_metadata_bolt12_offer};

    #[test]
    fn profile_bolt12_metadata_requires_all_sprout_offer_fields_to_match() {
        let mut metadata = Map::new();
        metadata.insert("bolt12_offer".into(), Value::String("lno1new".into()));

        assert!(!profile_metadata_has_bolt12_offer(&metadata, "lno1new"));

        set_profile_metadata_bolt12_offer(&mut metadata, "lno1new");

        assert!(profile_metadata_has_bolt12_offer(&metadata, "lno1new"));
    }

    #[test]
    fn setting_profile_bolt12_metadata_preserves_wallet_fields() {
        let mut wallet = Map::new();
        wallet.insert("label".into(), Value::String("primary".into()));
        wallet.insert("bolt12_offer".into(), Value::String("lno1old".into()));

        let mut metadata = Map::new();
        metadata.insert("display_name".into(), Value::String("Mat".into()));
        metadata.insert("wallet".into(), Value::Object(wallet));

        set_profile_metadata_bolt12_offer(&mut metadata, "lno1new");

        assert_eq!(
            metadata.get("display_name").and_then(Value::as_str),
            Some("Mat")
        );
        assert_eq!(
            metadata.get("bolt12_offer").and_then(Value::as_str),
            Some("lno1new")
        );
        let wallet = metadata
            .get("wallet")
            .and_then(Value::as_object)
            .expect("wallet object");
        assert_eq!(wallet.get("label").and_then(Value::as_str), Some("primary"));
        assert_eq!(
            wallet.get("bolt12_offer").and_then(Value::as_str),
            Some("lno1new")
        );
    }
}
