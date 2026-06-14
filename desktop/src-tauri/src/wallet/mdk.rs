use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::managed_agents::login_shell_path;

use super::{
    storage::{write_atomic_text, WalletStorage},
    types::{WalletTransaction, WALLET_BOLT12_OFFER_DESCRIPTION},
};

const MDK_AGENT_WALLET_PACKAGE: &str = "@moneydevkit/agent-wallet@0.20.0";
const MDK_NETWORK: &str = "mainnet";

#[derive(Clone, Debug)]
pub(crate) struct MdkWallet {
    home_dir: PathBuf,
    port: u16,
}

#[derive(Deserialize)]
struct MdkInitResponse {
    status: Option<String>,
}

#[derive(Deserialize)]
struct MdkBalanceResponse {
    balance_sats: u64,
}

#[derive(Deserialize)]
struct MdkBolt12OfferResponse {
    offer: String,
}

#[derive(Deserialize)]
struct MdkInvoiceResponse {
    invoice: String,
}

#[derive(Deserialize)]
struct MdkSendResponse {
    payment_id: String,
    status: String,
}

#[derive(Deserialize)]
struct MdkPaymentsResponse {
    payments: Vec<MdkPayment>,
}

#[derive(Deserialize)]
struct MdkDaemonStatusResponse {
    running: bool,
    pid: Option<u32>,
    port: Option<u16>,
}

#[derive(Deserialize)]
struct MdkDaemonStartResponse {
    started: bool,
    pid: Option<u32>,
    port: Option<u16>,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct MdkDaemonStopResponse {
    stopped: bool,
    reason: Option<String>,
}

#[derive(Deserialize)]
struct MdkHealthEnvelope {
    success: bool,
    data: Option<MdkHealthData>,
    error: Option<MdkHealthError>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MdkHealthData {
    status: Option<String>,
    node_running: Option<bool>,
}

#[derive(Deserialize)]
struct MdkHealthError {
    code: Option<String>,
    message: Option<String>,
}

struct AgentWalletCommandResult<T> {
    value: T,
    status_success: bool,
    status_text: String,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MdkAgentWalletStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub port: Option<u16>,
    pub expected_port: u16,
    pub healthy: bool,
    pub node_running: bool,
    pub health_error: Option<String>,
    pub home_dir: String,
    pub log_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MdkPayment {
    #[serde(alias = "payment_id")]
    payment_id: Option<String>,
    #[serde(alias = "payment_hash")]
    payment_hash: Option<String>,
    #[serde(alias = "amount_sats")]
    amount_sats: Option<u64>,
    direction: Option<String>,
    timestamp: Option<u64>,
    destination: Option<String>,
    status: Option<String>,
    #[serde(alias = "payer_note")]
    payer_note: Option<String>,
}

impl MdkWallet {
    pub(crate) async fn load_or_create(storage: &WalletStorage) -> Result<Self, String> {
        storage.ensure_dirs()?;
        ensure_private_dir(&storage.mdk_home_dir, "MDK wallet home")?;
        let port = load_or_allocate_port(&storage.mdk_port_path)?;
        let wallet = Self {
            home_dir: storage.mdk_home_dir.clone(),
            port,
        };
        wallet.ensure_initialized().await?;
        Ok(wallet)
    }

    pub(crate) async fn balance_sats(&self) -> Result<u64, String> {
        let response = self
            .run_agent_wallet_json::<MdkBalanceResponse>(&["balance"], "load MDK balance")
            .await?;
        Ok(response.balance_sats)
    }

    pub(crate) async fn create_bolt12_offer(&self) -> Result<String, String> {
        let response = self
            .run_agent_wallet_json::<MdkBolt12OfferResponse>(
                &[
                    "receive-bolt12",
                    "--description",
                    WALLET_BOLT12_OFFER_DESCRIPTION,
                ],
                "create MDK BOLT12 offer",
            )
            .await?;
        let offer = response.offer.trim();
        if offer.is_empty() {
            return Err("MDK returned an empty BOLT12 offer".to_string());
        }
        Ok(offer.to_string())
    }

    pub(crate) async fn create_invoice(
        &self,
        amount_sats: u64,
        description: &str,
    ) -> Result<String, String> {
        let amount = amount_sats.to_string();
        let response = self
            .run_agent_wallet_json::<MdkInvoiceResponse>(
                &["receive", &amount, "--description", description],
                "create MDK invoice",
            )
            .await?;
        let invoice = response.invoice.trim();
        if invoice.is_empty() {
            return Err("MDK returned an empty invoice".to_string());
        }
        Ok(invoice.to_string())
    }

    pub(crate) async fn send(&self, destination: &str, amount_sats: u64) -> Result<String, String> {
        let amount = amount_sats.to_string();
        let response = self
            .run_agent_wallet_json::<MdkSendResponse>(
                &["send", destination, &amount],
                "send MDK payment",
            )
            .await?;
        if response.status.trim().eq_ignore_ascii_case("completed") {
            return Ok(response.payment_id);
        }
        Err(format!(
            "MDK payment {} ended with status {}",
            response.payment_id, response.status
        ))
    }

    pub(crate) async fn transactions(
        &self,
        limit: usize,
    ) -> Result<Vec<WalletTransaction>, String> {
        let response = self
            .run_agent_wallet_json::<MdkPaymentsResponse>(&["payments"], "list MDK payments")
            .await?;
        let mut payments = response.payments;
        payments.sort_by_key(|payment| std::cmp::Reverse(payment.timestamp.unwrap_or(0)));
        Ok(payments
            .into_iter()
            .take(limit)
            .map(MdkPayment::into_wallet_transaction)
            .collect())
    }

    pub(crate) async fn daemon_status(&self) -> Result<MdkAgentWalletStatus, String> {
        let response = self
            .run_agent_wallet_json::<MdkDaemonStatusResponse>(
                &["status"],
                "check MDK daemon status",
            )
            .await?;
        Ok(self.status_from_response(response).await)
    }

    pub(crate) async fn restart_daemon(&self) -> Result<MdkAgentWalletStatus, String> {
        let stop_response = self
            .run_agent_wallet_json::<MdkDaemonStopResponse>(&["stop"], "stop MDK daemon")
            .await?;
        if !stop_response.stopped && stop_response.reason.as_deref() != Some("not_running") {
            eprintln!(
                "buzz-desktop: MDK stop returned false before restart: {}",
                stop_response
                    .reason
                    .unwrap_or_else(|| "unknown reason".to_string())
            );
        }
        self.wait_for_port_release(Duration::from_secs(10)).await?;

        let start_response = self
            .run_agent_wallet_json_result::<MdkDaemonStartResponse>(&["start"], "start MDK daemon")
            .await?;
        if start_response.status_success && start_response.value.started {
            let status = self
                .status_from_response(MdkDaemonStatusResponse {
                    running: true,
                    pid: start_response.value.pid,
                    port: start_response.value.port,
                })
                .await;
            if status.healthy && status.node_running {
                return Ok(status);
            }
            let reason = status.health_error.unwrap_or_else(|| {
                if status.healthy {
                    "daemon health is ok, but the node is still starting".to_string()
                } else {
                    "daemon health check failed".to_string()
                }
            });
            let log_excerpt = self
                .daemon_log_excerpt()
                .map(|excerpt| format!("; recent daemon log: {excerpt}"))
                .unwrap_or_default();
            return Err(format!(
                "restart MDK daemon: daemon started but is not ready: {reason}{log_excerpt}"
            ));
        }
        if start_response.status_success
            && start_response.value.reason.as_deref() == Some("already_running")
        {
            return self.daemon_status().await;
        }

        if let Ok(status) = self.daemon_status().await {
            if status.healthy && status.node_running {
                return Ok(status);
            }
        }

        let reason = start_response
            .value
            .reason
            .unwrap_or_else(|| "daemon did not start".to_string());
        let log_excerpt = self
            .daemon_log_excerpt()
            .map(|excerpt| format!("; recent daemon log: {excerpt}"))
            .unwrap_or_default();
        Err(format!(
            "restart MDK daemon: {reason}; start exited with status {}{}{}{}",
            start_response.status_text,
            output_excerpt("stdout", &start_response.stdout),
            output_excerpt("stderr", &start_response.stderr),
            log_excerpt
        ))
    }

    async fn ensure_initialized(&self) -> Result<(), String> {
        if self.config_path().exists() {
            return Ok(());
        }
        let response = self
            .run_agent_wallet_json::<MdkInitResponse>(
                &["init", "--network", MDK_NETWORK],
                "initialize MDK wallet",
            )
            .await?;
        if response.status.as_deref() == Some("initialized") {
            Ok(())
        } else {
            Err("MDK wallet did not report initialized status".to_string())
        }
    }

    fn config_path(&self) -> PathBuf {
        self.home_dir.join(".mdk-wallet").join("config.json")
    }

    async fn status_from_response(
        &self,
        response: MdkDaemonStatusResponse,
    ) -> MdkAgentWalletStatus {
        let (healthy, node_running, health_error) =
            self.daemon_health(response.running, response.port).await;
        MdkAgentWalletStatus {
            running: response.running,
            pid: response.pid,
            port: response.port,
            expected_port: self.port,
            healthy,
            node_running,
            health_error,
            home_dir: self.home_dir.to_string_lossy().to_string(),
            log_path: self
                .home_dir
                .join(".mdk-wallet")
                .join("daemon.log")
                .to_string_lossy()
                .to_string(),
        }
    }

    async fn daemon_health(
        &self,
        running: bool,
        port: Option<u16>,
    ) -> (bool, bool, Option<String>) {
        if !running {
            return (false, false, None);
        }
        let Some(port) = port else {
            return (
                false,
                false,
                Some("daemon status did not report a port".to_string()),
            );
        };
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
        {
            Ok(client) => client,
            Err(error) => return (false, false, Some(format!("build health client: {error}"))),
        };
        let response = match client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return (
                    false,
                    false,
                    Some(format!("health request failed: {error}")),
                )
            }
        };
        if !response.status().is_success() {
            return (
                false,
                false,
                Some(format!("health returned HTTP {}", response.status())),
            );
        }
        let envelope = match response.json::<MdkHealthEnvelope>().await {
            Ok(envelope) => envelope,
            Err(error) => {
                return (
                    false,
                    false,
                    Some(format!("parse health response: {error}")),
                )
            }
        };
        if !envelope.success {
            let message = envelope
                .error
                .and_then(|error| match (error.code, error.message) {
                    (Some(code), Some(message)) => Some(format!("{code}: {message}")),
                    (Some(code), None) => Some(code),
                    (None, Some(message)) => Some(message),
                    (None, None) => None,
                })
                .unwrap_or_else(|| "health endpoint returned an error".to_string());
            return (false, false, Some(message));
        }
        let Some(data) = envelope.data else {
            return (
                false,
                false,
                Some("health response did not include data".to_string()),
            );
        };
        let status_ok = data.status.as_deref() == Some("ok");
        let node_running = data.node_running.unwrap_or(false);
        let health_error = (!status_ok).then(|| {
            format!(
                "health status was {}",
                data.status.unwrap_or_else(|| "missing".to_string())
            )
        });
        (status_ok, node_running, health_error)
    }

    async fn wait_for_port_release(&self, timeout: Duration) -> Result<(), String> {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if port_is_free(self.port) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        Err(format!(
            "restart MDK daemon: port {} was still in use after {}s",
            self.port,
            timeout.as_secs()
        ))
    }

    fn daemon_log_excerpt(&self) -> Option<String> {
        let path = self.home_dir.join(".mdk-wallet").join("daemon.log");
        let contents = std::fs::read_to_string(path).ok()?;
        let lines = contents
            .lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .take(8)
            .collect::<Vec<_>>();
        if lines.is_empty() {
            return None;
        }
        Some(lines.into_iter().rev().collect::<Vec<_>>().join(" | "))
    }

    async fn run_agent_wallet_json<T>(&self, args: &[&str], context: &str) -> Result<T, String>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let result = self.run_agent_wallet_json_result(args, context).await?;
        if !result.status_success {
            let stdout_excerpt = if context == "initialize MDK wallet" {
                String::new()
            } else {
                output_excerpt("stdout", &result.stdout)
            };
            return Err(format!(
                "{context}: agent-wallet exited with status {}{}{}",
                result.status_text,
                stdout_excerpt,
                output_excerpt("stderr", &result.stderr)
            ));
        }
        Ok(result.value)
    }

    async fn run_agent_wallet_json_result<T>(
        &self,
        args: &[&str],
        context: &str,
    ) -> Result<AgentWalletCommandResult<T>, String>
    where
        T: DeserializeOwned + Send + 'static,
    {
        let home_dir = self.home_dir.clone();
        let port = self.port;
        let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let context = context.to_string();
        let join_context = context.clone();
        tauri::async_runtime::spawn_blocking(move || {
            run_agent_wallet_json_result_blocking(home_dir, port, args, &context)
        })
        .await
        .map_err(|error| format!("{join_context}: worker failed: {error}"))?
    }
}

impl MdkPayment {
    fn into_wallet_transaction(self) -> WalletTransaction {
        let destination = clean_optional(self.destination.as_deref());
        let direction = clean_optional(self.direction.as_deref()).unwrap_or_else(|| "info".into());
        let status = clean_optional(self.status.as_deref()).unwrap_or_else(|| "completed".into());
        let created_at_ms = self.timestamp.unwrap_or(0);
        let payment_hash = clean_optional(self.payment_hash.as_deref());
        let id = clean_optional(self.payment_id.as_deref())
            .or_else(|| payment_hash.clone())
            .unwrap_or_else(|| format!("mdk:{direction}:{created_at_ms}"));
        let rail = mdk_payment_rail(&direction, destination.as_deref());
        let kind = if rail == "mdk-bolt12" {
            "offer".to_string()
        } else {
            "payment".to_string()
        };

        WalletTransaction {
            id,
            rail,
            kind,
            direction,
            status,
            status_message: String::new(),
            amount_sats: self.amount_sats,
            fees_sats: 0,
            message: clean_optional(self.payer_note.as_deref()),
            personal_note: destination,
            created_at_ms,
            updated_at_ms: created_at_ms,
            agent_payment: None,
        }
    }
}

fn run_agent_wallet_json_result_blocking<T>(
    home_dir: PathBuf,
    port: u16,
    args: Vec<String>,
    context: &str,
) -> Result<AgentWalletCommandResult<T>, String>
where
    T: DeserializeOwned,
{
    let mut command = Command::new("npx");
    command
        .arg("--yes")
        .arg(MDK_AGENT_WALLET_PACKAGE)
        .args(&args)
        .env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("MDK_WALLET_PORT", port.to_string())
        .env("MDK_WALLET_NETWORK", MDK_NETWORK)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = login_shell_path() {
        command.env("PATH", path);
    }

    let output = command
        .output()
        .map_err(|error| format!("{context}: failed to run npx agent-wallet: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let value = parse_json_output(&stdout).map_err(|error| {
        let stdout_excerpt = if context == "initialize MDK wallet" {
            String::new()
        } else {
            output_excerpt("stdout", &stdout)
        };
        format!(
            "{context}: parse agent-wallet JSON: {error}{}",
            stdout_excerpt
        )
    })?;
    Ok(AgentWalletCommandResult {
        value,
        status_success: output.status.success(),
        status_text: output.status.to_string(),
        stdout,
        stderr,
    })
}

fn parse_json_output<T>(stdout: &str) -> Result<T, serde_json::Error>
where
    T: DeserializeOwned,
{
    let trimmed = stdout.trim();
    match serde_json::from_str::<T>(trimmed) {
        Ok(value) => Ok(value),
        Err(error) => {
            for line in stdout.lines().rev() {
                let line = line.trim();
                if line.starts_with('{') && line.ends_with('}') {
                    return serde_json::from_str::<T>(line);
                }
            }
            Err(error)
        }
    }
}

fn load_or_allocate_port(path: &Path) -> Result<u16, String> {
    match std::fs::read_to_string(path) {
        Ok(value) => {
            let port = parse_port(&value)?;
            return Ok(port);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("read MDK wallet port: {error}")),
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("allocate MDK wallet port: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("read MDK wallet port: {error}"))?
        .port();
    drop(listener);
    write_atomic_text(path, &port.to_string())?;
    Ok(port)
}

fn parse_port(value: &str) -> Result<u16, String> {
    let port = value
        .trim()
        .parse::<u16>()
        .map_err(|error| format!("parse MDK wallet port: {error}"))?;
    if port == 0 {
        return Err("MDK wallet port cannot be 0".to_string());
    }
    Ok(port)
}

fn ensure_private_dir(path: &Path, label: &str) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|error| format!("create {label}: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("set {label} permissions: {error}"))?;
    }
    Ok(())
}

fn port_is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn mdk_payment_rail(direction: &str, destination: Option<&str>) -> String {
    let destination = destination.unwrap_or_default().trim().to_ascii_lowercase();
    if direction.eq_ignore_ascii_case("inbound") || destination.starts_with("lno") {
        "mdk-bolt12".to_string()
    } else {
        "mdk-lightning".to_string()
    }
}

fn clean_optional(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn output_excerpt(label: &str, value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    let max_chars = 600;
    let excerpt = if value.chars().count() > max_chars {
        let mut output = value.chars().take(max_chars).collect::<String>();
        output.push_str("...");
        output
    } else {
        value.to_string()
    };
    format!("; {label}: {excerpt}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mdk_payments_into_wallet_transactions() {
        let payment = MdkPayment {
            payment_id: Some("pay_1".to_string()),
            payment_hash: Some("hash_1".to_string()),
            amount_sats: Some(42),
            direction: Some("outbound".to_string()),
            timestamp: Some(1234),
            destination: Some("lno1target".to_string()),
            status: Some("completed".to_string()),
            payer_note: None,
        };

        let tx = payment.into_wallet_transaction();

        assert_eq!(tx.id, "pay_1");
        assert_eq!(tx.rail, "mdk-bolt12");
        assert_eq!(tx.kind, "offer");
        assert_eq!(tx.amount_sats, Some(42));
        assert_eq!(tx.created_at_ms, 1234);
    }

    #[test]
    fn parses_json_from_last_stdout_line() {
        #[derive(Deserialize)]
        struct Payload {
            ok: bool,
        }

        let payload: Payload = parse_json_output("noise\n{\"ok\":true}\n").unwrap();

        assert!(payload.ok);
    }
}
