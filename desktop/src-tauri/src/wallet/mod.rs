mod balance;
mod broker;
mod cashu;
mod discovery;
mod format;
mod hive;
mod mdk;
mod parser;
mod provider;
mod runtime;
mod storage;
mod tips;
mod types;

use lexe::types::{
    auth::ClientCredentials,
    command::{CreateInvoiceRequest, PayRequest},
    payment::{Order, PaymentFilter, PaymentStatus},
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;

use balance::wallet_balances;
pub use broker::spawn_agent_payment_broker;
use discovery::{resolve_bolt12_offer_for_pubkey, resolve_send_payable};
use format::{
    amount_from_sats, format_amount, format_bolt12_offer_message, format_wallet_transaction,
    wallet_transaction_with_annotation,
};
pub(crate) use hive::create_hive_channel_wallet;
pub use hive::{
    execute_hive_channel_wallet_payouts, generate_hive_channel_wallet_bolt12_offer,
    get_hive_channel_wallet_summary, get_hive_channel_wallet_transactions,
    preview_hive_channel_wallet_payouts, reveal_hive_channel_wallet_seed, send_hive_channel_funds,
    send_hive_channel_wallet_payment,
};
use parser::parse_wallet_command;
pub use provider::WalletProvider;
use runtime::{
    clear_wallet_cache, ensure_bolt12_offer, ensure_wallet,
    reset_cached_wallet_if_credentials_missing, spawn_current_profile_bolt12_offer_sync,
    sync_current_profile_bolt12_offer,
};
use storage::{
    current_pubkey, load_agent_payment_annotations, load_agent_payment_settings,
    load_cashu_mint_url, load_existing_client_credential, load_root_seed, load_wallet_provider,
    load_wallet_source, load_walletbot_messages, new_walletbot_message,
    save_agent_payment_settings, save_cashu_mint_url, save_existing_client_credential,
    save_wallet_provider, save_wallet_source, save_walletbot_messages, walletbot_pubkey,
    write_atomic_text, WalletStorage,
};
pub use tips::{
    send_channel_payment, send_message_kudos, send_message_tip,
    send_shared_agent_invocation_payment,
};
use types::{
    validate_cashu_mint_url, WalletBotMessagesPayload, WalletCommand, WalletSource,
    DEFAULT_TRANSACTION_LIMIT, MAX_TRANSACTION_LIMIT, WALLETBOT_MESSAGES_UPDATED,
    WALLETBOT_WELCOME,
};
pub use types::{
    AgentPaymentBrokerConfig, WalletAgentPaymentSettings, WalletBotMessage, WalletPaymentResult,
    WalletRuntimeState, WalletSourceConfig, WalletSummary, WalletTransaction,
};

#[tauri::command]
pub async fn get_lightning_wallet_summary(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    reset_cached_wallet_if_credentials_missing(&app, &state).await?;
    let storage = WalletStorage::from_app(&app)?;
    let active_provider = load_wallet_provider(&storage)?;
    let active_source = if active_provider.capabilities().can_connect_existing_wallet {
        load_wallet_source(&storage)?
    } else {
        WalletSource::Default
    };
    let active_cashu_mint_url = if active_provider == WalletProvider::Cashu {
        Some(load_cashu_mint_url(&storage)?)
    } else {
        None
    };
    if let Some(summary) = state.wallet_state.summary.lock().await.clone() {
        if summary.provider == active_provider
            && summary.wallet_source == active_source
            && summary.cashu_mint_url == active_cashu_mint_url
            && active_provider != WalletProvider::Cashu
            && summary.balance_sats > 0
        {
            return Ok(summary);
        }
    }

    build_wallet_summary(&app, &state).await
}

#[tauri::command]
pub async fn refresh_lightning_wallet(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    reset_cached_wallet_if_credentials_missing(&app, &state).await?;
    sync_selected_wallet(&app, &state).await?;
    build_wallet_summary(&app, &state).await
}

#[tauri::command]
pub async fn generate_cashu_wallet_bolt12_offer(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    reset_cached_wallet_if_credentials_missing(&app, &state).await?;
    let storage = WalletStorage::from_app(&app)?;
    let provider = load_wallet_provider(&storage)?;
    if provider != WalletProvider::Cashu {
        return Err(
            "Cashu offer regeneration is only available when Cashu is selected".to_string(),
        );
    }

    let wallet = ensure_wallet(&app, &state).await?;
    let offer = wallet.cashu_wallet()?.generate_bolt12_offer().await?;
    let offer_path = storage.selected_cashu_storage()?.offer_path;
    write_atomic_text(&offer_path, &offer)?;
    *state.wallet_state.summary.lock().await = None;
    build_wallet_summary(&app, &state).await
}

#[tauri::command]
pub fn get_lightning_wallet_source_config(app: AppHandle) -> Result<WalletSourceConfig, String> {
    WalletStorage::from_app(&app)?.wallet_source_config()
}

#[tauri::command]
pub async fn set_cashu_wallet_mint(
    mint_url: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSourceConfig, String> {
    let storage = WalletStorage::from_app(&app)?;
    let mint_url = validate_cashu_mint_url(&mint_url)?;
    let previous_mint_url = load_cashu_mint_url(&storage)?;
    let provider = load_wallet_provider(&storage)?;

    if provider == WalletProvider::Cashu {
        let wallet = cashu::CashuWallet::load_or_create(&storage, &mint_url).await?;
        drop(wallet);
    }

    save_cashu_mint_url(&storage, &mint_url)?;
    clear_wallet_cache(&state).await;
    if provider == WalletProvider::Cashu {
        if let Err(error) = build_wallet_summary(&app, &state).await {
            let _ = save_cashu_mint_url(&storage, &previous_mint_url);
            clear_wallet_cache(&state).await;
            let _ = build_wallet_summary(&app, &state).await;
            return Err(format!("switch Cashu mint to {mint_url}: {error}"));
        }
    }
    storage.wallet_source_config()
}

#[tauri::command]
pub async fn set_lightning_wallet_provider(
    provider: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSourceConfig, String> {
    let storage = WalletStorage::from_app(&app)?;
    let provider = WalletProvider::from_ui_value(&provider)?;
    save_wallet_provider(&storage, provider)?;
    if !provider.capabilities().can_connect_existing_wallet {
        save_wallet_source(&storage, WalletSource::Default)?;
    }
    clear_wallet_cache(&state).await;
    match build_wallet_summary(&app, &state).await {
        Ok(summary) => {
            if let Err(error) =
                sync_current_profile_bolt12_offer(&state, &summary.bolt12_offer).await
            {
                eprintln!(
                    "buzz-desktop: failed to sync selected wallet provider BOLT12 profile: {error}"
                );
            }
        }
        Err(error) => {
            eprintln!("buzz-desktop: failed to prewarm selected wallet provider: {error}");
        }
    }
    storage.wallet_source_config()
}

#[tauri::command]
pub fn get_lightning_wallet_agent_payment_settings(
    app: AppHandle,
) -> Result<WalletAgentPaymentSettings, String> {
    load_agent_payment_settings(&WalletStorage::from_app(&app)?)
}

#[tauri::command]
pub fn set_lightning_wallet_agent_payment_settings(
    default_agents_to_lexe: bool,
    app: AppHandle,
) -> Result<WalletAgentPaymentSettings, String> {
    let storage = WalletStorage::from_app(&app)?;
    let settings = WalletAgentPaymentSettings {
        default_agents_to_lexe,
    };
    save_agent_payment_settings(&storage, &settings)?;
    Ok(settings)
}

pub(crate) fn default_agents_to_lexe_payments(app: &AppHandle) -> Result<bool, String> {
    Ok(load_agent_payment_settings(&WalletStorage::from_app(app)?)?.default_agents_to_lexe)
}

#[tauri::command]
pub async fn get_mdk_agent_wallet_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<mdk::MdkAgentWalletStatus, String> {
    ensure_selected_mdk_wallet(&app, &state)
        .await?
        .daemon_status()
        .await
}

#[tauri::command]
pub async fn restart_mdk_agent_wallet_daemon(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<mdk::MdkAgentWalletStatus, String> {
    let storage = WalletStorage::from_app(&app)?;
    let wallet = ensure_selected_mdk_wallet(&app, &state).await?;
    let status = wallet.restart_daemon().await?;
    remove_selected_mdk_offer_cache(&storage)?;
    clear_wallet_cache(&state).await;
    build_wallet_summary(&app, &state).await?;
    Ok(status)
}

#[tauri::command]
pub async fn set_lightning_wallet_source(
    source: String,
    client_credential: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSourceConfig, String> {
    let storage = WalletStorage::from_app(&app)?;
    let source = WalletSource::from_ui_value(&source)?;
    let provider = load_wallet_provider(&storage)?;

    if source == WalletSource::Existing && !provider.capabilities().can_connect_existing_wallet {
        return Err(format!(
            "{} does not support connecting an existing Lexe wallet",
            provider.label()
        ));
    }

    if let Some(client_credential) = client_credential.as_deref() {
        let client_credential = client_credential.trim();
        if !client_credential.is_empty() {
            ClientCredentials::from_string(client_credential)
                .map_err(|error| format!("parse Lexe client credential: {error}"))?;
            save_existing_client_credential(&storage, client_credential)?;
        }
    }

    if source == WalletSource::Existing && load_existing_client_credential(&storage)?.is_none() {
        return Err(
            "paste and save a Lexe SDK client credential before using an existing wallet"
                .to_string(),
        );
    }

    save_wallet_source(&storage, source)?;
    clear_wallet_cache(&state).await;
    if let Err(error) = build_wallet_summary(&app, &state).await {
        eprintln!("buzz-desktop: failed to prewarm selected wallet source: {error}");
    }
    storage.wallet_source_config()
}

#[tauri::command]
pub async fn reveal_lightning_wallet_seed(app: AppHandle) -> Result<String, String> {
    let storage = WalletStorage::from_app(&app)?;
    let provider = load_wallet_provider(&storage)?;
    if provider != WalletProvider::Lexe {
        return Err(format!(
            "{} recovery reveal is not available in Buzz yet",
            provider.label()
        ));
    }
    let seed = load_root_seed(&storage)?
        .ok_or_else(|| "wallet seed is not initialized yet".to_string())?;
    Ok(seed.to_mnemonic().to_string())
}

pub async fn prewarm_lightning_wallet(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    build_wallet_summary(&app, &state).await.map(|_| ())
}

#[tauri::command]
pub async fn get_lightning_wallet_transactions(
    limit: Option<usize>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<WalletTransaction>, String> {
    let limit = limit
        .unwrap_or(DEFAULT_TRANSACTION_LIMIT)
        .clamp(1, MAX_TRANSACTION_LIMIT);
    selected_wallet_transactions(&app, &state, limit, false).await
}

#[tauri::command]
pub async fn get_cashu_wallet_diagnostics(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let wallet = ensure_wallet(&app, &state).await?;
    if wallet.provider() != WalletProvider::Cashu {
        return Err("Cashu diagnostics are only available when Cashu is selected".to_string());
    }
    let report = wallet.cashu_wallet()?.diagnostics_report().await?;
    serde_json::to_string_pretty(&report)
        .map_err(|error| format!("serialize Cashu diagnostics: {error}"))
}

#[tauri::command]
pub async fn get_user_wallet_bolt12_offer(
    pubkey: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    resolve_bolt12_offer_for_pubkey(&state, &pubkey).await
}

#[tauri::command]
pub async fn send_lightning_wallet_payment(
    amount_sats: u64,
    payable: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletPaymentResult, String> {
    send_payment(
        app,
        &state,
        amount_sats,
        payable,
        None,
        "Sprout profile payment".to_string(),
    )
    .await
}

#[tauri::command]
pub async fn get_walletbot_messages(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<WalletBotMessage>, String> {
    let storage = WalletStorage::from_app(&app)?;
    let current_pubkey = current_pubkey(&state)?;
    let messages = load_walletbot_messages(&storage, &current_pubkey)?;
    save_walletbot_messages(&storage, &messages)?;
    Ok(messages)
}

#[tauri::command]
pub async fn send_walletbot_command(
    content: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<WalletBotMessage>, String> {
    let storage = WalletStorage::from_app(&app)?;
    let current_pubkey = current_pubkey(&state)?;
    let content = content.trim().to_string();
    let mut messages = load_walletbot_messages(&storage, &current_pubkey)?;

    if content.is_empty() {
        return Ok(messages);
    }

    messages.push(new_walletbot_message(
        "user",
        current_pubkey,
        content.clone(),
    ));

    let reply = match parse_wallet_command(&content) {
        Ok(command) => match execute_wallet_command(command, &app, &state).await {
            Ok(reply) => reply,
            Err(error) => format!("Wallet command failed: {error}"),
        },
        Err(error) => format!("Wallet command failed: {error}"),
    };
    messages.push(new_walletbot_message(
        "bot",
        walletbot_pubkey().to_string(),
        reply,
    ));
    save_walletbot_messages(&storage, &messages)?;

    let _ = app.emit(
        WALLETBOT_MESSAGES_UPDATED,
        WalletBotMessagesPayload {
            messages: messages.clone(),
        },
    );

    Ok(messages)
}

async fn execute_wallet_command(
    command: WalletCommand,
    app: &AppHandle,
    state: &AppState,
) -> Result<String, String> {
    match command {
        WalletCommand::Help => Ok(WALLETBOT_WELCOME.to_string()),
        WalletCommand::GetBalance => get_balance_reply(app, state).await,
        WalletCommand::GetBolt12 => {
            let offer = ensure_bolt12_offer(app, state).await?;
            Ok(format_bolt12_offer_message("BOLT12 offer.", &offer))
        }
        WalletCommand::FundWallet => {
            let offer = ensure_bolt12_offer(app, state).await?;
            Ok(format_bolt12_offer_message(
                "Fund your wallet with this reusable BOLT12 offer.",
                &offer,
            ))
        }
        WalletCommand::GetTransactions => get_transactions_reply(app, state).await,
        WalletCommand::CreateInvoice { amount } => create_invoice_reply(app, state, amount).await,
        WalletCommand::Send { amount, payable } => {
            let payable = resolve_send_payable(state, &payable).await?;
            send_payment_reply(app, state, amount, payable).await
        }
    }
}

async fn sync_selected_wallet(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let wallet = ensure_wallet(app, state).await?;
    match wallet.provider() {
        WalletProvider::Lexe => wallet
            .lexe_wallet()?
            .sync_payments()
            .await
            .map(|_| ())
            .map_err(|error| format!("sync Lexe payments: {error}")),
        WalletProvider::Mdk => wallet.mdk_wallet()?.transactions(1).await.map(|_| ()),
        WalletProvider::Cashu => wallet.cashu_wallet()?.sync().await,
    }
}

async fn ensure_selected_mdk_wallet(
    app: &AppHandle,
    state: &AppState,
) -> Result<std::sync::Arc<mdk::MdkWallet>, String> {
    let wallet = ensure_wallet(app, state).await?;
    if wallet.provider() != WalletProvider::Mdk {
        return Err(
            "MDK Agent Wallet controls are only available when MDK is selected".to_string(),
        );
    }
    wallet.mdk_wallet()
}

fn remove_selected_mdk_offer_cache(storage: &WalletStorage) -> Result<(), String> {
    let offer_path =
        storage.offer_path_for_provider_source(WalletProvider::Mdk, WalletSource::Default);
    match std::fs::remove_file(&offer_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove cached MDK BOLT12 offer: {error}")),
    }
}

async fn selected_wallet_transactions(
    app: &AppHandle,
    state: &AppState,
    limit: usize,
    sync_cashu_first: bool,
) -> Result<Vec<WalletTransaction>, String> {
    let wallet = ensure_wallet(app, state).await?;
    match wallet.provider() {
        WalletProvider::Lexe => {
            let wallet = wallet.lexe_wallet()?;
            wallet
                .sync_payments()
                .await
                .map_err(|error| format!("sync Lexe payments: {error}"))?;
            let response = wallet
                .list_payments(&PaymentFilter::All, Some(Order::Desc), Some(limit), None)
                .map_err(|error| format!("list Lexe payments: {error}"))?;
            let storage = WalletStorage::from_app(app)?;
            let annotations = load_agent_payment_annotations(&storage)?;
            Ok(response
                .payments
                .iter()
                .map(|payment| {
                    let annotation = annotations.get(&payment.index.to_string()).cloned();
                    wallet_transaction_with_annotation(payment, annotation)
                })
                .collect())
        }
        WalletProvider::Mdk => wallet.mdk_wallet()?.transactions(limit).await,
        WalletProvider::Cashu => {
            wallet
                .cashu_wallet()?
                .transactions(limit, sync_cashu_first)
                .await
        }
    }
}

async fn build_wallet_summary(app: &AppHandle, state: &AppState) -> Result<WalletSummary, String> {
    let storage = WalletStorage::from_app(app)?;
    let source_config = storage.wallet_source_config()?;
    let cashu_mint_url = source_config.cashu_mint_url.clone();
    let wallet = ensure_wallet(app, state).await?;
    let bolt12_offer = ensure_bolt12_offer(app, state).await?;
    spawn_current_profile_bolt12_offer_sync(app, bolt12_offer.clone(), "wallet summary");

    let summary = match wallet.provider() {
        WalletProvider::Lexe => {
            let wallet = wallet.lexe_wallet()?;
            let info = wallet
                .node_info()
                .await
                .map_err(|error| format!("load Lexe node info: {error}"))?;
            let balances = wallet_balances(&wallet, &info).await;

            WalletSummary {
                provider: source_config.provider,
                wallet_source: source_config.source,
                has_existing_client_credential: source_config.has_existing_client_credential,
                env: "mainnet".to_string(),
                seed_path: source_config.seed_path,
                existing_client_credential_path: source_config.existing_client_credential_path,
                balance_sats: balances.balance_sats,
                lightning_balance_sats: balances.lightning_balance_sats,
                lightning_sendable_balance_sats: balances.lightning_sendable_balance_sats,
                lightning_max_sendable_balance_sats: balances.lightning_max_sendable_balance_sats,
                onchain_balance_sats: balances.onchain_balance_sats,
                onchain_trusted_balance_sats: balances.onchain_trusted_balance_sats,
                num_channels: info.num_channels,
                num_usable_channels: info.num_usable_channels,
                bolt12_offer,
                cashu_mint_url: None,
            }
        }
        WalletProvider::Mdk => {
            let balance_sats = wallet.mdk_wallet()?.balance_sats().await?;
            WalletSummary {
                provider: source_config.provider,
                wallet_source: source_config.source,
                has_existing_client_credential: source_config.has_existing_client_credential,
                env: "mainnet".to_string(),
                seed_path: source_config.seed_path,
                existing_client_credential_path: source_config.existing_client_credential_path,
                balance_sats,
                lightning_balance_sats: balance_sats,
                lightning_sendable_balance_sats: balance_sats,
                lightning_max_sendable_balance_sats: balance_sats,
                onchain_balance_sats: 0,
                onchain_trusted_balance_sats: 0,
                num_channels: 0,
                num_usable_channels: 0,
                bolt12_offer,
                cashu_mint_url: None,
            }
        }
        WalletProvider::Cashu => {
            let wallet = wallet.cashu_wallet()?;
            wallet.sync().await?;
            let balance_sats = wallet.balance_sats().await?;
            let cashu_storage = storage.selected_cashu_storage()?;
            WalletSummary {
                provider: source_config.provider,
                wallet_source: source_config.source,
                has_existing_client_credential: source_config.has_existing_client_credential,
                env: "mainnet-cashu-test-mint".to_string(),
                seed_path: cashu_storage.seed_path.to_string_lossy().to_string(),
                existing_client_credential_path: source_config.existing_client_credential_path,
                balance_sats,
                lightning_balance_sats: balance_sats,
                lightning_sendable_balance_sats: balance_sats,
                lightning_max_sendable_balance_sats: balance_sats,
                onchain_balance_sats: 0,
                onchain_trusted_balance_sats: 0,
                num_channels: 0,
                num_usable_channels: 0,
                bolt12_offer,
                cashu_mint_url: Some(cashu_mint_url),
            }
        }
    };

    *state.wallet_state.summary.lock().await = Some(summary.clone());
    Ok(summary)
}

async fn get_balance_reply(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let summary = build_wallet_summary(app, state).await?;

    Ok(format!(
        "Balance: {} total; {} Lightning; {} Lightning spendable; {} on-chain trusted.",
        format_amount(summary.balance_sats),
        format_amount(summary.lightning_balance_sats),
        format_amount(summary.lightning_sendable_balance_sats),
        format_amount(summary.onchain_trusted_balance_sats),
    ))
}

async fn get_transactions_reply(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let transactions = selected_wallet_transactions(app, state, 5, true).await?;

    if transactions.is_empty() {
        return Ok("No recent transactions.".to_string());
    }

    Ok(transactions
        .iter()
        .map(format_wallet_transaction)
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn create_invoice_reply(
    app: &AppHandle,
    state: &AppState,
    amount_sats: u64,
) -> Result<String, String> {
    let wallet = ensure_wallet(app, state).await?;
    let invoice = match wallet.provider() {
        WalletProvider::Lexe => {
            let amount = amount_from_sats(amount_sats)?;
            let response = wallet
                .lexe_wallet()?
                .create_invoice(CreateInvoiceRequest {
                    expiration_secs: None,
                    amount: Some(amount),
                    description: Some("Sprout WalletBot invoice".to_string()),
                    personal_note: None,
                    partner_pk: None,
                    partner_prop_fee: None,
                    partner_base_fee: None,
                })
                .await
                .map_err(|error| format!("create Lexe invoice: {error}"))?;
            response.invoice.to_string()
        }
        WalletProvider::Mdk => {
            wallet
                .mdk_wallet()?
                .create_invoice(amount_sats, "Sprout WalletBot invoice")
                .await?
        }
        WalletProvider::Cashu => {
            return Err("Cashu wallet does not support BOLT11 invoice creation yet".to_string())
        }
    };

    Ok(format!(
        "Invoice for {}:\n{}",
        format_amount(amount_sats),
        invoice
    ))
}

async fn send_payment_reply(
    app: &AppHandle,
    state: &AppState,
    amount_sats: u64,
    payable: String,
) -> Result<String, String> {
    let response = send_payment(
        app.clone(),
        state,
        amount_sats,
        payable,
        None,
        "Sprout WalletBot command".to_string(),
    )
    .await?;

    Ok(format!(
        "Sent {}. Payment id: {}",
        format_amount(amount_sats),
        response.payment_id
    ))
}

async fn send_payment(
    app: AppHandle,
    state: &AppState,
    amount_sats: u64,
    payable: String,
    message: Option<String>,
    personal_note: String,
) -> Result<WalletPaymentResult, String> {
    let wallet = ensure_wallet(&app, state).await?;
    let payment_id = match wallet.provider() {
        WalletProvider::Lexe => {
            let amount = amount_from_sats(amount_sats)?;
            let response = wallet
                .lexe_wallet()?
                .pay(PayRequest {
                    payable,
                    amount: Some(amount),
                    message,
                    personal_note: Some(personal_note),
                })
                .await
                .map_err(|error| format!("send Lexe payment: {error}"))?;
            if response.status != PaymentStatus::Completed {
                let status_message = payment_status_message(response.status_msg.as_str());
                return Err(format!(
                    "Lexe payment failed for {}: {status_message}",
                    format_amount(amount_sats)
                ));
            }
            response.index.to_string()
        }
        WalletProvider::Mdk => wallet.mdk_wallet()?.send(&payable, amount_sats).await?,
        WalletProvider::Cashu => wallet.cashu_wallet()?.send(&payable, amount_sats).await?,
    };
    *state.wallet_state.summary.lock().await = None;

    Ok(WalletPaymentResult {
        payment_id,
        amount_sats,
    })
}

fn payment_status_message(status_message: &str) -> String {
    let status_message = status_message.trim();
    if status_message.is_empty() {
        "Lexe marked the payment failed".to_string()
    } else {
        status_message.to_string()
    }
}
