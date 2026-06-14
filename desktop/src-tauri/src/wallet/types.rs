use std::{collections::HashMap, sync::Arc};

use lexe::wallet::LexeWallet;
use serde::{Deserialize, Serialize};

pub const WALLETBOT_MESSAGES_UPDATED: &str = "walletbot-messages-updated";
pub(crate) const WALLET_DIR_NAME: &str = "lexe-wallet";
pub(crate) const LEXE_DATA_DIR_NAME: &str = "data";
pub(crate) const EXISTING_LEXE_DATA_DIR_NAME: &str = "existing-data";
pub(crate) const SEED_FILE_NAME: &str = "seedphrase.txt";
pub(crate) const OFFER_FILE_NAME: &str = "bolt12_offer.txt";
pub(crate) const EXISTING_OFFER_FILE_NAME: &str = "existing_bolt12_offer.txt";
pub(crate) const WALLET_SOURCE_FILE_NAME: &str = "wallet_source.txt";
pub(crate) const EXISTING_CLIENT_CREDENTIAL_FILE_NAME: &str = "lexe_client_credential.txt";
pub(crate) const MESSAGES_FILE_NAME: &str = "walletbot_messages.json";
pub(crate) const AGENT_PAYMENT_ANNOTATIONS_FILE_NAME: &str = "agent_payment_annotations.json";
pub(crate) const AGENT_PAYMENT_SETTINGS_FILE_NAME: &str = "agent_payment_settings.json";
pub(crate) const HIVE_CHANNELS_DIR_NAME: &str = "hive-channels";
pub(crate) const WALLET_BOLT12_OFFER_DESCRIPTION: &str = "Sprout WalletBot BOLT12 offer";
pub(crate) const WALLETBOT_WELCOME: &str = "WalletBot is local to this Sprout app.\n\nAvailable commands:\n- help\n- get balance\n- get BOLT12\n- fund wallet\n- get transactions\n- create invoice for ₿1,000\n- send ₿500 to <payment target>";
pub(crate) const DEFAULT_TRANSACTION_LIMIT: usize = 20;
pub(crate) const MAX_TRANSACTION_LIMIT: usize = 100;

#[derive(Default)]
pub struct WalletRuntimeState {
    pub(crate) wallet: tokio::sync::Mutex<Option<Arc<LexeWallet>>>,
    pub(crate) wallet_source: tokio::sync::Mutex<Option<WalletSource>>,
    pub(crate) summary: tokio::sync::Mutex<Option<WalletSummary>>,
    pub(crate) hive_wallets: tokio::sync::Mutex<HashMap<String, Arc<LexeWallet>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletSource {
    Default,
    Existing,
}

impl WalletSource {
    pub(crate) fn from_storage_value(value: &str) -> Self {
        match value.trim() {
            "existing" => Self::Existing,
            _ => Self::Default,
        }
    }

    pub(crate) fn from_ui_value(value: &str) -> Result<Self, String> {
        match value.trim() {
            "default" => Ok(Self::Default),
            "existing" => Ok(Self::Existing),
            other => Err(format!("unknown wallet source: {other}")),
        }
    }

    pub(crate) fn as_storage_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Existing => "existing",
        }
    }
}

impl Serialize for WalletSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_storage_value())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSourceConfig {
    pub source: WalletSource,
    pub seed_path: String,
    pub existing_client_credential_path: String,
    pub has_existing_client_credential: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSummary {
    pub wallet_source: WalletSource,
    pub has_existing_client_credential: bool,
    pub env: String,
    pub seed_path: String,
    pub existing_client_credential_path: String,
    pub balance_sats: u64,
    pub lightning_balance_sats: u64,
    pub lightning_sendable_balance_sats: u64,
    pub lightning_max_sendable_balance_sats: u64,
    pub onchain_balance_sats: u64,
    pub onchain_trusted_balance_sats: u64,
    pub num_channels: usize,
    pub num_usable_channels: usize,
    pub bolt12_offer: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletPaymentResult {
    pub payment_id: String,
    pub amount_sats: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutShare {
    pub revenue_payment_id: String,
    pub revenue_amount_sats: u64,
    pub revenue_created_at_ms: u64,
    pub amount_sats: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutRecipient {
    pub member_pubkey: String,
    pub amount_sats: u64,
    pub bolt12_offer: Option<String>,
    pub shares: Vec<HiveChannelPayoutShare>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutPreview {
    pub channel_id: String,
    pub total_unattributed_revenue_sats: u64,
    pub total_payout_sats: u64,
    pub unpaid_revenue_count: usize,
    pub skipped_no_owner_revenue_count: usize,
    pub already_paid_share_count: usize,
    pub recipients: Vec<HiveChannelPayoutRecipient>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutPayment {
    pub member_pubkey: String,
    pub amount_sats: u64,
    pub payment_id: String,
    pub message_event_id: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutFailure {
    pub member_pubkey: Option<String>,
    pub amount_sats: Option<u64>,
    pub error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelPayoutExecution {
    pub channel_id: String,
    pub status: String,
    pub total_paid_sats: u64,
    pub paid: Vec<HiveChannelPayoutPayment>,
    pub failed: Option<HiveChannelPayoutFailure>,
    pub remaining_preview: HiveChannelPayoutPreview,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelContributionShare {
    pub member_pubkey: Option<String>,
    pub amount_sats: u64,
    pub ownership_percent: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HiveChannelWalletSummary {
    pub channel_id: String,
    pub has_local_seed: bool,
    pub seed_path: String,
    pub balance_sats: u64,
    pub lightning_balance_sats: u64,
    pub lightning_sendable_balance_sats: u64,
    pub onchain_balance_sats: u64,
    pub bolt12_offer: String,
    pub total_contributed_sats: u64,
    pub ownership_shares: Vec<HiveChannelContributionShare>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageTipResult {
    pub payment_id: String,
    pub amount_sats: u64,
    pub tip_id: String,
    pub receipt_event_id: Option<String>,
    pub receipt_accepted: bool,
    pub receipt_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPaymentResult {
    pub payment_id: String,
    pub amount_sats: u64,
    pub nonce: String,
    pub receipt_event_id: Option<String>,
    pub receipt_accepted: bool,
    pub receipt_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletTransaction {
    pub id: String,
    pub rail: String,
    pub kind: String,
    pub direction: String,
    pub status: String,
    pub status_message: String,
    pub amount_sats: Option<u64>,
    pub fees_sats: u64,
    pub message: Option<String>,
    pub personal_note: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub agent_payment: Option<WalletAgentPaymentAnnotation>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletAgentPaymentAnnotation {
    pub payment_id: String,
    pub agent_pubkey: Option<String>,
    pub agent_name: Option<String>,
    pub protocol: String,
    pub endpoint: Option<String>,
    pub endpoint_host: Option<String>,
    pub endpoint_path: Option<String>,
    pub consent_event_id: Option<String>,
    pub status: String,
    pub status_message: Option<String>,
    pub amount_sats: Option<u64>,
    pub fees_sats: Option<u64>,
    pub payment_hash: Option<String>,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletAgentPaymentSettings {
    pub default_agents_to_lexe: bool,
}

impl Default for WalletAgentPaymentSettings {
    fn default() -> Self {
        Self {
            default_agents_to_lexe: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AgentPaymentBrokerConfig {
    pub base_url: String,
    pub token: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletBotMessage {
    pub id: String,
    pub role: String,
    pub author_pubkey: String,
    pub content: String,
    pub created_at: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WalletBotMessagesPayload {
    pub messages: Vec<WalletBotMessage>,
}

pub(crate) enum WalletCommand {
    Help,
    GetBalance,
    GetBolt12,
    FundWallet,
    GetTransactions,
    CreateInvoice { amount: u64 },
    Send { amount: u64, payable: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_source_storage_defaults_to_sprout_wallet() {
        assert_eq!(
            WalletSource::from_storage_value("existing"),
            WalletSource::Existing
        );
        assert_eq!(
            WalletSource::from_storage_value("default"),
            WalletSource::Default
        );
        assert_eq!(
            WalletSource::from_storage_value("unknown"),
            WalletSource::Default
        );
        assert_eq!(WalletSource::from_storage_value(""), WalletSource::Default);
    }

    #[test]
    fn wallet_source_ui_values_are_strict() {
        assert_eq!(
            WalletSource::from_ui_value("default").unwrap(),
            WalletSource::Default
        );
        assert_eq!(
            WalletSource::from_ui_value("existing").unwrap(),
            WalletSource::Existing
        );
        assert!(WalletSource::from_ui_value("unknown").is_err());
    }

    #[test]
    fn wallet_source_serializes_for_frontend() {
        assert_eq!(
            serde_json::to_string(&WalletSource::Default).unwrap(),
            "\"default\""
        );
        assert_eq!(
            serde_json::to_string(&WalletSource::Existing).unwrap(),
            "\"existing\""
        );
    }
}
