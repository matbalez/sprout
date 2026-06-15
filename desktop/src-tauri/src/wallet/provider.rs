use std::sync::Arc;

use lexe::wallet::LexeWallet;
use serde::Serialize;

use super::{cashu::CashuWallet, mdk::MdkWallet};

pub(crate) const LEXE_WALLET_PROVIDER_ID: &str = "lexe";
pub(crate) const LEXE_WALLET_PROVIDER_LABEL: &str = "Lexe";
pub(crate) const LEXE_BOLT12_PAYMENT_RAIL: &str = "lexe-bolt12";
pub(crate) const MDK_WALLET_PROVIDER_ID: &str = "mdk";
pub(crate) const MDK_WALLET_PROVIDER_LABEL: &str = "MDK Agent Wallet";
pub(crate) const MDK_BOLT12_PAYMENT_RAIL: &str = "mdk-bolt12";
pub(crate) const CASHU_WALLET_PROVIDER_ID: &str = "cashu";
pub(crate) const CASHU_WALLET_PROVIDER_LABEL: &str = "Cashu";
pub(crate) const CASHU_BOLT12_PAYMENT_RAIL: &str = "cashu-mint-bolt12";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletProvider {
    Lexe,
    Mdk,
    Cashu,
}

impl WalletProvider {
    pub(crate) fn from_storage_value(value: &str) -> Self {
        match value.trim() {
            LEXE_WALLET_PROVIDER_ID => Self::Lexe,
            MDK_WALLET_PROVIDER_ID => Self::Mdk,
            CASHU_WALLET_PROVIDER_ID => Self::Cashu,
            _ => Self::Lexe,
        }
    }

    pub(crate) fn from_ui_value(value: &str) -> Result<Self, String> {
        match value.trim() {
            LEXE_WALLET_PROVIDER_ID => Ok(Self::Lexe),
            MDK_WALLET_PROVIDER_ID => Ok(Self::Mdk),
            CASHU_WALLET_PROVIDER_ID => Ok(Self::Cashu),
            other => Err(format!("unknown wallet provider: {other}")),
        }
    }

    pub(crate) fn as_storage_value(self) -> &'static str {
        match self {
            Self::Lexe => LEXE_WALLET_PROVIDER_ID,
            Self::Mdk => MDK_WALLET_PROVIDER_ID,
            Self::Cashu => CASHU_WALLET_PROVIDER_ID,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Lexe => LEXE_WALLET_PROVIDER_LABEL,
            Self::Mdk => MDK_WALLET_PROVIDER_LABEL,
            Self::Cashu => CASHU_WALLET_PROVIDER_LABEL,
        }
    }

    pub(crate) fn capabilities(self) -> WalletProviderCapabilities {
        match self {
            Self::Lexe => WalletProviderCapabilities {
                can_create_wallet: true,
                can_connect_existing_wallet: true,
                can_receive_reusable_bolt12: true,
                can_send_bolt12: true,
                can_get_balance: true,
                can_list_payments: true,
                can_subscribe_payments: false,
                can_send_bolt11: true,
                can_create_bolt11_invoice: true,
                can_pay_with_preimage: true,
            },
            Self::Mdk => WalletProviderCapabilities {
                can_create_wallet: true,
                can_connect_existing_wallet: false,
                can_receive_reusable_bolt12: true,
                can_send_bolt12: true,
                can_get_balance: true,
                can_list_payments: true,
                can_subscribe_payments: false,
                can_send_bolt11: true,
                can_create_bolt11_invoice: true,
                can_pay_with_preimage: true,
            },
            Self::Cashu => WalletProviderCapabilities {
                can_create_wallet: true,
                can_connect_existing_wallet: false,
                can_receive_reusable_bolt12: true,
                can_send_bolt12: true,
                can_get_balance: true,
                can_list_payments: true,
                can_subscribe_payments: false,
                can_send_bolt11: true,
                can_create_bolt11_invoice: false,
                can_pay_with_preimage: false,
            },
        }
    }
}

impl Serialize for WalletProvider {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_storage_value())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletProviderCapabilities {
    pub can_create_wallet: bool,
    pub can_connect_existing_wallet: bool,
    pub can_receive_reusable_bolt12: bool,
    pub can_send_bolt12: bool,
    pub can_get_balance: bool,
    pub can_list_payments: bool,
    pub can_subscribe_payments: bool,
    pub can_send_bolt11: bool,
    pub can_create_bolt11_invoice: bool,
    pub can_pay_with_preimage: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletProviderOption {
    pub provider: WalletProvider,
    pub label: &'static str,
    pub payment_rail: &'static str,
    pub available: bool,
    pub capabilities: WalletProviderCapabilities,
}

pub(crate) fn available_wallet_providers() -> Vec<WalletProviderOption> {
    vec![
        WalletProviderOption {
            provider: WalletProvider::Lexe,
            label: WalletProvider::Lexe.label(),
            payment_rail: LEXE_BOLT12_PAYMENT_RAIL,
            available: true,
            capabilities: WalletProvider::Lexe.capabilities(),
        },
        WalletProviderOption {
            provider: WalletProvider::Mdk,
            label: WalletProvider::Mdk.label(),
            payment_rail: MDK_BOLT12_PAYMENT_RAIL,
            available: true,
            capabilities: WalletProvider::Mdk.capabilities(),
        },
        WalletProviderOption {
            provider: WalletProvider::Cashu,
            label: WalletProvider::Cashu.label(),
            payment_rail: CASHU_BOLT12_PAYMENT_RAIL,
            available: true,
            capabilities: WalletProvider::Cashu.capabilities(),
        },
    ]
}

#[derive(Clone)]
pub(crate) enum WalletProviderHandle {
    Lexe(Arc<LexeWallet>),
    Mdk(Arc<MdkWallet>),
    Cashu(Arc<CashuWallet>),
}

impl WalletProviderHandle {
    pub(crate) fn new_lexe(wallet: LexeWallet) -> Self {
        Self::Lexe(Arc::new(wallet))
    }

    pub(crate) fn new_mdk(wallet: MdkWallet) -> Self {
        Self::Mdk(Arc::new(wallet))
    }

    pub(crate) fn new_cashu(wallet: CashuWallet) -> Self {
        Self::Cashu(Arc::new(wallet))
    }

    pub(crate) fn provider(&self) -> WalletProvider {
        match self {
            Self::Lexe(_) => WalletProvider::Lexe,
            Self::Mdk(_) => WalletProvider::Mdk,
            Self::Cashu(_) => WalletProvider::Cashu,
        }
    }

    pub(crate) fn lexe_wallet(&self) -> Result<Arc<LexeWallet>, String> {
        match self {
            Self::Lexe(wallet) => Ok(wallet.clone()),
            Self::Mdk(_) => Err("active wallet provider is MDK, not Lexe".to_string()),
            Self::Cashu(_) => Err("active wallet provider is Cashu, not Lexe".to_string()),
        }
    }

    pub(crate) fn mdk_wallet(&self) -> Result<Arc<MdkWallet>, String> {
        match self {
            Self::Mdk(wallet) => Ok(wallet.clone()),
            Self::Lexe(_) => Err("active wallet provider is Lexe, not MDK".to_string()),
            Self::Cashu(_) => Err("active wallet provider is Cashu, not MDK".to_string()),
        }
    }

    pub(crate) fn cashu_wallet(&self) -> Result<Arc<CashuWallet>, String> {
        match self {
            Self::Cashu(wallet) => Ok(wallet.clone()),
            Self::Lexe(_) => Err("active wallet provider is Lexe, not Cashu".to_string()),
            Self::Mdk(_) => Err("active wallet provider is MDK, not Cashu".to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_storage_defaults_unknown_values_to_lexe() {
        assert_eq!(
            WalletProvider::from_storage_value("unknown"),
            WalletProvider::Lexe
        );
        assert_eq!(WalletProvider::from_storage_value(""), WalletProvider::Lexe);
    }

    #[test]
    fn provider_ui_values_are_strict() {
        assert_eq!(
            WalletProvider::from_ui_value("lexe").unwrap(),
            WalletProvider::Lexe
        );
        assert_eq!(
            WalletProvider::from_ui_value("mdk").unwrap(),
            WalletProvider::Mdk
        );
        assert_eq!(
            WalletProvider::from_ui_value("cashu").unwrap(),
            WalletProvider::Cashu
        );
        assert!(WalletProvider::from_ui_value("unknown").is_err());
    }

    #[test]
    fn lexe_declares_core_bolt12_capabilities() {
        let capabilities = WalletProvider::Lexe.capabilities();

        assert!(capabilities.can_create_wallet);
        assert!(capabilities.can_receive_reusable_bolt12);
        assert!(capabilities.can_send_bolt12);
        assert!(capabilities.can_get_balance);
    }

    #[test]
    fn mdk_declares_embedded_bolt12_capabilities_without_existing_connect() {
        let capabilities = WalletProvider::Mdk.capabilities();

        assert!(capabilities.can_create_wallet);
        assert!(!capabilities.can_connect_existing_wallet);
        assert!(capabilities.can_receive_reusable_bolt12);
        assert!(capabilities.can_send_bolt12);
        assert!(capabilities.can_get_balance);
    }

    #[test]
    fn cashu_declares_mint_backed_bolt12_capabilities_without_lexe_connect() {
        let capabilities = WalletProvider::Cashu.capabilities();

        assert!(capabilities.can_create_wallet);
        assert!(!capabilities.can_connect_existing_wallet);
        assert!(capabilities.can_receive_reusable_bolt12);
        assert!(capabilities.can_send_bolt12);
        assert!(capabilities.can_get_balance);
        assert!(capabilities.can_send_bolt11);
        assert!(!capabilities.can_create_bolt11_invoice);
    }
}
