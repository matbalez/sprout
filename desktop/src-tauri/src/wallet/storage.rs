use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lexe::types::auth::RootSeed;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;

use super::{
    provider::{available_wallet_providers, WalletProvider},
    types::{
        WalletAgentPaymentAnnotation, WalletAgentPaymentSettings, WalletBotMessage, WalletSource,
        WalletSourceConfig, AGENT_PAYMENT_ANNOTATIONS_FILE_NAME, AGENT_PAYMENT_SETTINGS_FILE_NAME,
        EXISTING_CLIENT_CREDENTIAL_FILE_NAME, EXISTING_LEXE_DATA_DIR_NAME,
        EXISTING_OFFER_FILE_NAME, LEXE_DATA_DIR_NAME, MESSAGES_FILE_NAME, OFFER_FILE_NAME,
        SEED_FILE_NAME, WALLETBOT_WELCOME, WALLET_DIR_NAME, WALLET_PROVIDER_FILE_NAME,
        WALLET_SOURCE_FILE_NAME,
    },
};

#[derive(Clone)]
pub(crate) struct WalletStorage {
    pub root_dir: PathBuf,
    pub lexe_data_dir: PathBuf,
    pub existing_lexe_data_dir: PathBuf,
    pub seed_path: PathBuf,
    pub offer_path: PathBuf,
    pub existing_offer_path: PathBuf,
    pub wallet_provider_path: PathBuf,
    pub wallet_source_path: PathBuf,
    pub existing_client_credential_path: PathBuf,
    pub messages_path: PathBuf,
    pub agent_payment_annotations_path: PathBuf,
    pub agent_payment_settings_path: PathBuf,
}

impl WalletStorage {
    pub(crate) fn from_app(app: &AppHandle) -> Result<Self, String> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("app data dir: {error}"))?;
        let root_dir = app_data_dir.join(WALLET_DIR_NAME);
        Ok(Self {
            lexe_data_dir: root_dir.join(LEXE_DATA_DIR_NAME),
            existing_lexe_data_dir: root_dir.join(EXISTING_LEXE_DATA_DIR_NAME),
            seed_path: root_dir.join(SEED_FILE_NAME),
            offer_path: root_dir.join(OFFER_FILE_NAME),
            existing_offer_path: root_dir.join(EXISTING_OFFER_FILE_NAME),
            wallet_provider_path: root_dir.join(WALLET_PROVIDER_FILE_NAME),
            wallet_source_path: root_dir.join(WALLET_SOURCE_FILE_NAME),
            existing_client_credential_path: root_dir.join(EXISTING_CLIENT_CREDENTIAL_FILE_NAME),
            messages_path: root_dir.join(MESSAGES_FILE_NAME),
            agent_payment_annotations_path: root_dir.join(AGENT_PAYMENT_ANNOTATIONS_FILE_NAME),
            agent_payment_settings_path: root_dir.join(AGENT_PAYMENT_SETTINGS_FILE_NAME),
            root_dir,
        })
    }

    pub(crate) fn ensure_dirs(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.root_dir)
            .map_err(|error| format!("create wallet directory: {error}"))?;
        std::fs::create_dir_all(&self.lexe_data_dir)
            .map_err(|error| format!("create Lexe data directory: {error}"))?;
        std::fs::create_dir_all(&self.existing_lexe_data_dir)
            .map_err(|error| format!("create existing Lexe data directory: {error}"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root_dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| format!("set wallet directory permissions: {error}"))?;
        }

        Ok(())
    }

    pub(crate) fn wallet_source_config(&self) -> Result<WalletSourceConfig, String> {
        let has_existing_client_credential = load_existing_client_credential(self)?.is_some();

        Ok(WalletSourceConfig {
            provider: load_wallet_provider(self)?,
            available_providers: available_wallet_providers(),
            source: load_wallet_source(self)?,
            seed_path: self.seed_path.to_string_lossy().to_string(),
            existing_client_credential_path: self
                .existing_client_credential_path
                .to_string_lossy()
                .to_string(),
            has_existing_client_credential,
        })
    }

    pub(crate) fn reset_for_new_root_seed(&self) -> Result<(), String> {
        remove_file_if_exists(&self.offer_path, "BOLT12 offer cache")?;
        remove_file_if_exists(&self.messages_path, "WalletBot message history")?;
        if self.lexe_data_dir.exists() {
            std::fs::remove_dir_all(&self.lexe_data_dir)
                .map_err(|error| format!("remove stale Lexe data directory: {error}"))?;
        }
        self.ensure_dirs()
    }

    pub(crate) fn offer_path_for_provider_source(
        &self,
        provider: WalletProvider,
        source: WalletSource,
    ) -> PathBuf {
        match (provider.as_storage_value(), source) {
            ("lexe", WalletSource::Default) => self.offer_path.clone(),
            ("lexe", WalletSource::Existing) => self.existing_offer_path.clone(),
            (provider, source) => self.root_dir.join("providers").join(provider).join(format!(
                "{}_{}",
                source.as_storage_value(),
                OFFER_FILE_NAME
            )),
        }
    }
}

pub(crate) fn load_wallet_provider(storage: &WalletStorage) -> Result<WalletProvider, String> {
    storage.ensure_dirs()?;
    match std::fs::read_to_string(&storage.wallet_provider_path) {
        Ok(value) => Ok(WalletProvider::from_storage_value(&value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(WalletProvider::Lexe),
        Err(error) => Err(format!("read wallet provider: {error}")),
    }
}

pub(crate) fn save_wallet_provider(
    storage: &WalletStorage,
    provider: WalletProvider,
) -> Result<(), String> {
    storage.ensure_dirs()?;
    write_atomic_text(&storage.wallet_provider_path, provider.as_storage_value())
}

pub(crate) fn load_wallet_source(storage: &WalletStorage) -> Result<WalletSource, String> {
    storage.ensure_dirs()?;
    match std::fs::read_to_string(&storage.wallet_source_path) {
        Ok(value) => Ok(WalletSource::from_storage_value(&value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(WalletSource::Default),
        Err(error) => Err(format!("read wallet source: {error}")),
    }
}

pub(crate) fn save_wallet_source(
    storage: &WalletStorage,
    source: WalletSource,
) -> Result<(), String> {
    storage.ensure_dirs()?;
    write_atomic_text(&storage.wallet_source_path, source.as_storage_value())
}

pub(crate) fn load_existing_client_credential(
    storage: &WalletStorage,
) -> Result<Option<String>, String> {
    storage.ensure_dirs()?;
    match std::fs::read_to_string(&storage.existing_client_credential_path) {
        Ok(value) => {
            let value = value.trim().to_string();
            if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("read Lexe client credential: {error}")),
    }
}

pub(crate) fn save_existing_client_credential(
    storage: &WalletStorage,
    client_credential: &str,
) -> Result<(), String> {
    storage.ensure_dirs()?;
    let client_credential = client_credential.trim();
    let previous_client_credential = load_existing_client_credential(storage)?;
    write_atomic_secret_text(&storage.existing_client_credential_path, client_credential)?;
    remove_file_if_exists(&storage.existing_offer_path, "existing BOLT12 offer cache")?;

    if previous_client_credential.as_deref() != Some(client_credential) {
        remove_dir_if_exists(
            &storage.existing_lexe_data_dir,
            "existing Lexe data directory",
        )?;
        storage.ensure_dirs()?;
    }

    Ok(())
}

fn write_atomic_secret_text(path: &Path, content: &str) -> Result<(), String> {
    use atomic_write_file::AtomicWriteFile;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create parent dir: {error}"))?;
    }

    let mut file =
        AtomicWriteFile::open(path).map_err(|error| format!("open atomic write: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("set secret file permissions: {error}"))?;
    }

    file.write_all(content.as_bytes())
        .map_err(|error| format!("write file: {error}"))?;
    file.commit()
        .map_err(|error| format!("commit file: {error}"))
}

pub(crate) fn load_root_seed(storage: &WalletStorage) -> Result<Option<RootSeed>, String> {
    storage.ensure_dirs()?;
    RootSeed::read_from_path(&storage.seed_path)
        .map_err(|error| format!("read wallet seed: {error}"))
}

pub(crate) fn load_walletbot_messages(
    storage: &WalletStorage,
    current_pubkey: &str,
) -> Result<Vec<WalletBotMessage>, String> {
    storage.ensure_dirs()?;
    let mut messages = match std::fs::read_to_string(&storage.messages_path) {
        Ok(content) => serde_json::from_str::<Vec<WalletBotMessage>>(&content)
            .map_err(|error| format!("parse WalletBot messages: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("read WalletBot messages: {error}")),
    };

    if messages.is_empty() {
        messages.push(new_walletbot_message(
            "bot",
            walletbot_pubkey().to_string(),
            WALLETBOT_WELCOME.to_string(),
        ));
    }

    for message in &mut messages {
        if message.role == "user" {
            message.author_pubkey = current_pubkey.to_string();
        } else if message.role == "bot" {
            message.author_pubkey = walletbot_pubkey().to_string();
        }
    }

    Ok(messages)
}

pub(crate) fn save_walletbot_messages(
    storage: &WalletStorage,
    messages: &[WalletBotMessage],
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(messages)
        .map_err(|error| format!("serialize WalletBot messages: {error}"))?;
    write_atomic_text(&storage.messages_path, &content)
}

pub(crate) fn load_agent_payment_annotations(
    storage: &WalletStorage,
) -> Result<BTreeMap<String, WalletAgentPaymentAnnotation>, String> {
    storage.ensure_dirs()?;
    match std::fs::read_to_string(&storage.agent_payment_annotations_path) {
        Ok(content) => {
            serde_json::from_str::<BTreeMap<String, WalletAgentPaymentAnnotation>>(&content)
                .map_err(|error| format!("parse agent payment annotations: {error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
        Err(error) => Err(format!("read agent payment annotations: {error}")),
    }
}

pub(crate) fn save_agent_payment_annotation(
    storage: &WalletStorage,
    annotation: WalletAgentPaymentAnnotation,
) -> Result<(), String> {
    let mut annotations = load_agent_payment_annotations(storage)?;
    annotations.insert(annotation.payment_id.clone(), annotation);
    let content = serde_json::to_string_pretty(&annotations)
        .map_err(|error| format!("serialize agent payment annotations: {error}"))?;
    write_atomic_text(&storage.agent_payment_annotations_path, &content)
}

pub(crate) fn load_agent_payment_settings(
    storage: &WalletStorage,
) -> Result<WalletAgentPaymentSettings, String> {
    storage.ensure_dirs()?;
    match std::fs::read_to_string(&storage.agent_payment_settings_path) {
        Ok(content) => serde_json::from_str::<WalletAgentPaymentSettings>(&content)
            .map_err(|error| format!("parse agent payment settings: {error}")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(WalletAgentPaymentSettings::default())
        }
        Err(error) => Err(format!("read agent payment settings: {error}")),
    }
}

pub(crate) fn save_agent_payment_settings(
    storage: &WalletStorage,
    settings: &WalletAgentPaymentSettings,
) -> Result<(), String> {
    let content = serde_json::to_string_pretty(settings)
        .map_err(|error| format!("serialize agent payment settings: {error}"))?;
    write_atomic_text(&storage.agent_payment_settings_path, &content)
}

pub(crate) fn write_atomic_text(path: &Path, content: &str) -> Result<(), String> {
    use atomic_write_file::AtomicWriteFile;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("create parent dir: {error}"))?;
    }

    let mut file =
        AtomicWriteFile::open(path).map_err(|error| format!("open atomic write: {error}"))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("write file: {error}"))?;
    file.commit()
        .map_err(|error| format!("commit file: {error}"))
}

fn remove_file_if_exists(path: &Path, label: &str) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove stale {label}: {error}")),
    }
}

fn remove_dir_if_exists(path: &Path, label: &str) -> Result<(), String> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("remove stale {label}: {error}")),
    }
}

pub(crate) fn current_pubkey(state: &AppState) -> Result<String, String> {
    let keys = state.keys.lock().map_err(|error| error.to_string())?;
    Ok(keys.public_key().to_hex())
}

pub(crate) fn walletbot_pubkey() -> &'static str {
    "0000000000000000000000000000000000000000000000000000000000000001"
}

pub(crate) fn new_walletbot_message(
    role: &str,
    author_pubkey: String,
    content: String,
) -> WalletBotMessage {
    let created_at = now_secs();
    WalletBotMessage {
        id: walletbot_message_id(role, &author_pubkey, &content, created_at),
        role: role.to_string(),
        author_pubkey,
        content,
        created_at,
    }
}

fn walletbot_message_id(role: &str, author_pubkey: &str, content: &str, created_at: u64) -> String {
    let nonce = uuid::Uuid::new_v4();
    let preimage = format!("walletbot:{role}:{author_pubkey}:{created_at}:{nonce}:{content}");
    hex::encode(Sha256::digest(preimage.as_bytes()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_storage(root_dir: PathBuf) -> WalletStorage {
        WalletStorage {
            lexe_data_dir: root_dir.join(LEXE_DATA_DIR_NAME),
            existing_lexe_data_dir: root_dir.join(EXISTING_LEXE_DATA_DIR_NAME),
            seed_path: root_dir.join(SEED_FILE_NAME),
            offer_path: root_dir.join(OFFER_FILE_NAME),
            existing_offer_path: root_dir.join(EXISTING_OFFER_FILE_NAME),
            wallet_provider_path: root_dir.join(WALLET_PROVIDER_FILE_NAME),
            wallet_source_path: root_dir.join(WALLET_SOURCE_FILE_NAME),
            existing_client_credential_path: root_dir.join(EXISTING_CLIENT_CREDENTIAL_FILE_NAME),
            messages_path: root_dir.join(MESSAGES_FILE_NAME),
            agent_payment_annotations_path: root_dir.join(AGENT_PAYMENT_ANNOTATIONS_FILE_NAME),
            agent_payment_settings_path: root_dir.join(AGENT_PAYMENT_SETTINGS_FILE_NAME),
            root_dir,
        }
    }

    #[test]
    fn wallet_source_config_defaults_to_lexe_provider() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));

        let config = storage.wallet_source_config().unwrap();

        assert_eq!(config.provider, WalletProvider::Lexe);
        assert_eq!(config.available_providers.len(), 1);
        assert_eq!(config.available_providers[0].provider, WalletProvider::Lexe);
    }

    #[test]
    fn wallet_provider_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));

        save_wallet_provider(&storage, WalletProvider::Lexe).unwrap();

        assert_eq!(
            load_wallet_provider(&storage).unwrap(),
            WalletProvider::Lexe
        );
    }

    #[test]
    fn lexe_offer_cache_uses_existing_legacy_paths() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));

        assert_eq!(
            storage.offer_path_for_provider_source(WalletProvider::Lexe, WalletSource::Default),
            storage.offer_path
        );
        assert_eq!(
            storage.offer_path_for_provider_source(WalletProvider::Lexe, WalletSource::Existing),
            storage.existing_offer_path
        );
    }

    #[test]
    fn wallet_source_config_ignores_empty_existing_credential_file() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));
        storage.ensure_dirs().unwrap();
        std::fs::write(&storage.existing_client_credential_path, "   \n").unwrap();

        let config = storage.wallet_source_config().unwrap();

        assert!(!config.has_existing_client_credential);
    }

    #[test]
    fn replacing_existing_credential_clears_existing_wallet_cache() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));
        save_existing_client_credential(&storage, "credential-a").unwrap();

        let stale_data_file = storage.existing_lexe_data_dir.join("stale-cache");
        std::fs::write(&stale_data_file, "stale").unwrap();
        std::fs::write(&storage.existing_offer_path, "lno1old").unwrap();

        save_existing_client_credential(&storage, "credential-b").unwrap();

        assert!(storage.existing_lexe_data_dir.exists());
        assert!(!stale_data_file.exists());
        assert!(!storage.existing_offer_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn existing_credential_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));

        save_existing_client_credential(&storage, "credential-a").unwrap();

        let mode = std::fs::metadata(&storage.existing_client_credential_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn agent_payment_settings_default_agents_to_lexe() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));

        let settings = load_agent_payment_settings(&storage).unwrap();

        assert!(settings.default_agents_to_lexe);
    }

    #[test]
    fn agent_payment_settings_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let storage = test_storage(temp.path().join("wallet"));
        let settings = WalletAgentPaymentSettings {
            default_agents_to_lexe: false,
        };

        save_agent_payment_settings(&storage, &settings).unwrap();

        assert!(
            !load_agent_payment_settings(&storage)
                .unwrap()
                .default_agents_to_lexe
        );
    }
}
