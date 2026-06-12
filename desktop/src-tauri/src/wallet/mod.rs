mod balance;
mod broker;
mod discovery;
mod format;
mod parser;
mod runtime;
mod storage;
mod tips;
mod types;

use lexe::types::{
    auth::ClientCredentials,
    command::{CreateInvoiceRequest, PayRequest},
    payment::{Order, PaymentFilter},
};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::app_state::AppState;

use balance::wallet_balances;
pub use broker::spawn_agent_payment_broker;
use discovery::{resolve_bolt12_offer_for_pubkey, resolve_send_payable};
use format::{
    amount_from_sats, format_amount, format_bolt12_offer_message, format_payment,
    wallet_transaction_with_annotation,
};
use parser::parse_wallet_command;
use runtime::{
    clear_wallet_cache, ensure_bolt12_offer, ensure_wallet,
    reset_cached_wallet_if_credentials_missing, spawn_current_profile_bolt12_offer_sync,
};
use storage::{
    current_pubkey, load_agent_payment_annotations, load_agent_payment_settings,
    load_existing_client_credential, load_root_seed, load_wallet_source, load_walletbot_messages,
    new_walletbot_message, save_agent_payment_settings, save_existing_client_credential,
    save_wallet_source, save_walletbot_messages, walletbot_pubkey, WalletStorage,
};
pub use tips::{
    send_channel_payment, send_message_kudos, send_message_tip,
    send_shared_agent_invocation_payment,
};
pub use types::{
    AgentPaymentBrokerConfig, WalletAgentPaymentSettings, WalletBotMessage, WalletPaymentResult,
    WalletRuntimeState, WalletSourceConfig, WalletSummary, WalletTransaction,
};
use types::{
    WalletBotMessagesPayload, WalletCommand, WalletSource, DEFAULT_TRANSACTION_LIMIT,
    MAX_TRANSACTION_LIMIT, WALLETBOT_MESSAGES_UPDATED, WALLETBOT_WELCOME,
};

#[tauri::command]
pub async fn get_lightning_wallet_summary(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSummary, String> {
    reset_cached_wallet_if_credentials_missing(&app, &state).await?;
    let active_source = load_wallet_source(&WalletStorage::from_app(&app)?)?;
    if let Some(summary) = state.wallet_state.summary.lock().await.clone() {
        if summary.wallet_source == active_source && summary.balance_sats > 0 {
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
    let wallet = ensure_wallet(&app, &state).await?;
    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync Lexe payments: {error}"))?;
    build_wallet_summary(&app, &state).await
}

#[tauri::command]
pub fn get_lightning_wallet_source_config(app: AppHandle) -> Result<WalletSourceConfig, String> {
    WalletStorage::from_app(&app)?.wallet_source_config()
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
pub async fn set_lightning_wallet_source(
    source: String,
    client_credential: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletSourceConfig, String> {
    let storage = WalletStorage::from_app(&app)?;
    let source = WalletSource::from_ui_value(&source)?;

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
        eprintln!("sprout-desktop: failed to prewarm selected Lexe wallet: {error}");
    }
    storage.wallet_source_config()
}

#[tauri::command]
pub async fn reveal_lightning_wallet_seed(app: AppHandle) -> Result<String, String> {
    let storage = WalletStorage::from_app(&app)?;
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
    let wallet = ensure_wallet(&app, &state).await?;
    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync Lexe payments: {error}"))?;
    let limit = limit
        .unwrap_or(DEFAULT_TRANSACTION_LIMIT)
        .clamp(1, MAX_TRANSACTION_LIMIT);
    let response = wallet
        .list_payments(&PaymentFilter::All, Some(Order::Desc), Some(limit), None)
        .map_err(|error| format!("list Lexe payments: {error}"))?;
    let storage = WalletStorage::from_app(&app)?;
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
                "Fund your Lexe wallet with this reusable BOLT12 offer.",
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

async fn build_wallet_summary(app: &AppHandle, state: &AppState) -> Result<WalletSummary, String> {
    let storage = WalletStorage::from_app(app)?;
    let source_config = storage.wallet_source_config()?;
    let wallet = ensure_wallet(app, state).await?;
    let info = wallet
        .node_info()
        .await
        .map_err(|error| format!("load Lexe node info: {error}"))?;
    let bolt12_offer = ensure_bolt12_offer(app, state).await?;
    spawn_current_profile_bolt12_offer_sync(app, bolt12_offer.clone(), "wallet summary");
    let balances = wallet_balances(&wallet, &info).await;

    let summary = WalletSummary {
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
    };

    *state.wallet_state.summary.lock().await = Some(summary.clone());
    Ok(summary)
}

async fn get_balance_reply(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let wallet = ensure_wallet(app, state).await?;
    let info = wallet
        .node_info()
        .await
        .map_err(|error| format!("load Lexe node info: {error}"))?;
    let balances = wallet_balances(&wallet, &info).await;

    Ok(format!(
        "Balance: {} total; {} Lightning; {} Lightning spendable; {} on-chain trusted.",
        format_amount(balances.balance_sats),
        format_amount(balances.lightning_balance_sats),
        format_amount(balances.lightning_sendable_balance_sats),
        format_amount(balances.onchain_trusted_balance_sats),
    ))
}

async fn get_transactions_reply(app: &AppHandle, state: &AppState) -> Result<String, String> {
    let wallet = ensure_wallet(app, state).await?;
    wallet
        .sync_payments()
        .await
        .map_err(|error| format!("sync Lexe payments: {error}"))?;
    let response = wallet
        .list_payments(&PaymentFilter::All, Some(Order::Desc), Some(5), None)
        .map_err(|error| format!("list Lexe payments: {error}"))?;

    if response.payments.is_empty() {
        return Ok("No recent transactions.".to_string());
    }

    Ok(response
        .payments
        .iter()
        .map(format_payment)
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn create_invoice_reply(
    app: &AppHandle,
    state: &AppState,
    amount_sats: u64,
) -> Result<String, String> {
    let wallet = ensure_wallet(app, state).await?;
    let amount = amount_from_sats(amount_sats)?;
    let response = wallet
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

    Ok(format!(
        "Invoice for {}:\n{}",
        format_amount(amount_sats),
        response.invoice
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
    let amount = amount_from_sats(amount_sats)?;
    let response = wallet
        .pay(PayRequest {
            payable,
            amount: Some(amount),
            message,
            personal_note: Some(personal_note),
        })
        .await
        .map_err(|error| format!("send Lexe payment: {error}"))?;
    *state.wallet_state.summary.lock().await = None;

    Ok(WalletPaymentResult {
        payment_id: response.index.to_string(),
        amount_sats,
    })
}
