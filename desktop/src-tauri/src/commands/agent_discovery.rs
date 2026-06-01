use std::{collections::BTreeMap, io::Read};
use tauri::State;

use crate::{
    app_state::AppState,
    managed_agents::{
        command_availability, AcpProviderCatalogEntry, DiscoverManagedAgentPrereqsRequest,
        InstallRuntimeResult, InstallStepResult, ManagedAgentPrereqsInfo, RelayAgentInfo,
        DEFAULT_ACP_COMMAND, DEFAULT_MCP_COMMAND,
    },
    nostr_convert,
    relay::query_relay,
};

fn active_installs() -> &'static std::sync::Mutex<std::collections::HashSet<String>> {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static ACTIVE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn extract_agent_owner_pubkey(event: &nostr::Event) -> Option<String> {
    let agent_hex = event.pubkey.to_hex();
    let agent_pubkey = nostr::PublicKey::from_hex(&agent_hex).ok()?;

    for tag in event.tags.iter() {
        let slice = tag.as_slice();
        if slice.first().map(String::as_str) != Some("auth") || slice.len() != 4 {
            continue;
        }

        let tag_json = serde_json::to_string(slice).ok()?;
        if let Ok(owner) = sprout_sdk::nip_oa::verify_auth_tag(&tag_json, &agent_pubkey) {
            return Some(owner.to_hex());
        }
    }

    None
}

#[tauri::command]
pub async fn resolve_shared_agent_owner(
    target_pubkey: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    let target = nostr::PublicKey::from_hex(&target_pubkey)
        .map_err(|error| format!("invalid target pubkey: {error}"))?;
    let target_pubkey = target.to_hex();

    // Existing shared agents may not have a relay-agent directory entry yet,
    // but their profile or replies can still carry a verifiable NIP-OA auth tag.
    let events = query_relay(
        &state,
        &[
            serde_json::json!({
                "kinds": [0],
                "authors": [target_pubkey.clone()],
                "limit": 1,
            }),
            serde_json::json!({
                "kinds": [9],
                "authors": [target_pubkey.clone()],
                "limit": 50,
            }),
        ],
    )
    .await?;

    let mut owner: Option<(u64, String)> = None;
    for event in events {
        if event.pubkey.to_hex() != target_pubkey {
            continue;
        }

        let Some(owner_pubkey) = extract_agent_owner_pubkey(&event) else {
            continue;
        };
        let created_at = event.created_at.as_secs();
        match owner {
            Some((existing_created_at, _)) if existing_created_at > created_at => {}
            _ => owner = Some((created_at, owner_pubkey)),
        }
    }

    Ok(owner.map(|(_, owner_pubkey)| owner_pubkey))
}

async fn resolve_agent_owner_pubkeys(
    state: &AppState,
    agent_pubkeys: &[String],
) -> Result<BTreeMap<String, String>, String> {
    if agent_pubkeys.is_empty() {
        return Ok(BTreeMap::new());
    }

    let events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [0],
            "authors": agent_pubkeys,
        })],
    )
    .await?;

    let mut owners: BTreeMap<String, (u64, String)> = BTreeMap::new();
    for event in events {
        let Some(owner_pubkey) = extract_agent_owner_pubkey(&event) else {
            continue;
        };
        let agent_pubkey = event.pubkey.to_hex().to_ascii_lowercase();
        let created_at = event.created_at.as_secs();
        match owners.get(&agent_pubkey) {
            Some((existing_created_at, _)) if *existing_created_at > created_at => {}
            _ => {
                owners.insert(agent_pubkey, (created_at, owner_pubkey));
            }
        }
    }

    Ok(owners
        .into_iter()
        .map(|(agent_pubkey, (_, owner_pubkey))| (agent_pubkey, owner_pubkey))
        .collect())
}

#[tauri::command]
pub fn discover_acp_providers() -> Vec<AcpProviderCatalogEntry> {
    crate::managed_agents::clear_resolve_cache();
    crate::managed_agents::discover_acp_providers()
}

#[tauri::command]
pub async fn install_acp_runtime(provider_id: String) -> Result<InstallRuntimeResult, String> {
    tokio::task::spawn_blocking(move || install_acp_runtime_blocking(&provider_id))
        .await
        .map_err(|e| format!("install task panicked: {e}"))?
}

/// Err(_) = infrastructure failure (panic, concurrency guard).
/// Ok({success: false}) = an install step failed (stderr captured in steps).
fn install_acp_runtime_blocking(provider_id: &str) -> Result<InstallRuntimeResult, String> {
    // Prevent concurrent installs for the same provider.
    {
        let mut set = active_installs()
            .lock()
            .map_err(|_| "install lock poisoned".to_string())?;
        if !set.insert(provider_id.to_string()) {
            return Err(format!(
                "an install is already in progress for {provider_id}"
            ));
        }
    }

    struct Guard(String);
    impl Drop for Guard {
        fn drop(&mut self) {
            if let Ok(mut set) = active_installs().lock() {
                set.remove(&self.0);
            }
        }
    }
    let _guard = Guard(provider_id.to_string());

    let provider = crate::managed_agents::known_acp_provider_exact(provider_id)
        .ok_or_else(|| format!("unknown provider: {provider_id}"))?;

    let mut steps = Vec::new();

    // Phase 1: Install CLI if missing and commands are available.
    if let Some(cli) = provider.underlying_cli {
        if crate::managed_agents::resolve_command(cli).is_none() {
            for cmd in provider.cli_install_commands {
                let result = run_install_command("cli", cmd);
                let success = result.success;
                steps.push(result);
                if !success {
                    return Ok(InstallRuntimeResult {
                        success: false,
                        steps,
                    });
                }
            }
        }
    }

    // Phase 2: Install adapter if missing and commands are available.
    let adapter_found = provider
        .commands
        .iter()
        .any(|cmd| crate::managed_agents::resolve_command(cmd).is_some());
    if !adapter_found {
        for cmd in provider.adapter_install_commands {
            let result = run_install_command("adapter", cmd);
            let success = result.success;
            steps.push(result);
            if !success {
                return Ok(InstallRuntimeResult {
                    success: false,
                    steps,
                });
            }
        }
    }

    // Clear the resolve cache so the next discovery picks up new binaries.
    crate::managed_agents::clear_resolve_cache();

    Ok(InstallRuntimeResult {
        success: true,
        steps,
    })
}

fn run_install_command(step: &str, command: &str) -> InstallStepResult {
    let shell_path = crate::managed_agents::login_shell_path();
    let shell = if std::path::Path::new("/bin/zsh").exists() {
        "/bin/zsh"
    } else {
        "/bin/bash"
    };

    let mut cmd = std::process::Command::new(shell);
    cmd.args(["-l", "-c", command]);

    // Strip hermit env vars so npm/node use the user's normal registry and
    // global prefix rather than the project-local hermit-managed paths.
    cmd.env_remove("NPM_CONFIG_PREFIX");
    cmd.env_remove("NPM_CONFIG_CACHE");
    cmd.env_remove("COREPACK_HOME");

    if let Some(ref path) = shell_path {
        cmd.env("PATH", path);
    }

    // Detach from the controlling terminal so install scripts that read from
    // /dev/tty (e.g. Codex's "Start Codex now? [y/N]") fall back to stdin
    // (which is /dev/null) instead of blocking forever.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }

    let mut child = match cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            return InstallStepResult {
                step: step.to_string(),
                command: command.to_string(),
                success: false,
                stdout: String::new(),
                stderr: format!("failed to spawn shell: {e}"),
                exit_code: None,
            };
        }
    };

    // Drain stdout/stderr on background threads to prevent pipe buffer deadlock.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut pipe) = stdout_pipe {
            let _ = pipe.read_to_string(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut pipe) = stderr_pipe {
            let _ = pipe.read_to_string(&mut buf);
        }
        buf
    });

    // Save the PID before moving `child` into the wait thread so we can
    // kill the process on timeout.
    let child_pid = child.id();

    let (tx, rx) = std::sync::mpsc::channel();
    let wait_thread = std::thread::spawn(move || {
        let status = child.wait();
        let _ = tx.send(status);
    });

    // 5-minute timeout for install commands.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            // Timeout: kill the child process via its PID, then join all
            // threads so nothing leaks.
            #[cfg(unix)]
            unsafe {
                libc::kill(child_pid as i32, libc::SIGTERM);
            }
            drop(rx);
            let _ = wait_thread.join();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return InstallStepResult {
                step: step.to_string(),
                command: command.to_string(),
                success: false,
                stdout: String::new(),
                stderr: "install command timed out after 5 minutes".to_string(),
                exit_code: None,
            };
        }

        match rx.recv_timeout(std::time::Duration::from_millis(200).min(remaining)) {
            Ok(Ok(status)) => {
                let _ = wait_thread.join();
                let stdout = stdout_thread.join().unwrap_or_default();
                let stderr_raw = stderr_thread.join().unwrap_or_default();
                return InstallStepResult {
                    step: step.to_string(),
                    command: command.to_string(),
                    success: status.success(),
                    stdout: truncate_output(stdout),
                    stderr: truncate_output(stderr_raw),
                    exit_code: status.code(),
                };
            }
            Ok(Err(e)) => {
                let _ = wait_thread.join();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return InstallStepResult {
                    step: step.to_string(),
                    command: command.to_string(),
                    success: false,
                    stdout: String::new(),
                    stderr: format!("failed to check process status: {e}"),
                    exit_code: None,
                };
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Still running; loop and check deadline again.
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                // wait_thread dropped sender without sending — shouldn't happen.
                let _ = wait_thread.join();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return InstallStepResult {
                    step: step.to_string(),
                    command: command.to_string(),
                    success: false,
                    stdout: String::new(),
                    stderr: "internal error: wait thread disconnected".to_string(),
                    exit_code: None,
                };
            }
        }
    }
}

/// Cap output to head + tail to avoid flooding the UI with large error dumps,
/// while preserving the most useful parts of the output.
fn truncate_output(s: String) -> String {
    const HEAD: usize = 512;
    const TAIL: usize = 1024;
    const LIMIT: usize = HEAD + TAIL;
    if s.len() <= LIMIT {
        return s;
    }
    let head_end = s.floor_char_boundary(HEAD);
    let tail_start = s.floor_char_boundary(s.len().saturating_sub(TAIL));
    let omitted = tail_start - head_end;
    format!(
        "{}\n... ({omitted} bytes omitted) ...\n{}",
        &s[..head_end],
        &s[tail_start..]
    )
}

#[tauri::command]
pub fn discover_managed_agent_prereqs(
    input: DiscoverManagedAgentPrereqsRequest,
) -> ManagedAgentPrereqsInfo {
    let acp_command = input
        .acp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_ACP_COMMAND);
    let mcp_command = input
        .mcp_command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_MCP_COMMAND);

    ManagedAgentPrereqsInfo {
        acp: command_availability(acp_command),
        mcp: command_availability(mcp_command),
    }
}

#[tauri::command]
pub async fn list_relay_agents(state: State<'_, AppState>) -> Result<Vec<RelayAgentInfo>, String> {
    // Query kind:10100 agent profile events from the relay.
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [10100],
        })],
    )
    .await?;

    // The convert helper returns `{"agents": [...]}`. Extract and re-deserialize
    // into the strongly-typed `Vec<RelayAgentInfo>` the frontend expects.
    let value = nostr_convert::agents_from_events(&events);
    let agents = value
        .get("agents")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let mut agents: Vec<RelayAgentInfo> =
        serde_json::from_value(agents).map_err(|e| format!("agent parse failed: {e}"))?;

    let agent_pubkeys: Vec<String> = agents
        .iter()
        .map(|agent| agent.pubkey.to_ascii_lowercase())
        .collect();
    if let Ok(owner_pubkeys) = resolve_agent_owner_pubkeys(&state, &agent_pubkeys).await {
        for agent in &mut agents {
            agent.owner_pubkey = owner_pubkeys
                .get(&agent.pubkey.to_ascii_lowercase())
                .cloned();
        }
    }

    Ok(agents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn event_with_auth(kind: Kind, agent: &Keys, owner: &Keys) -> nostr::Event {
        let agent_hex = agent.public_key().to_hex();
        let agent_compat = nostr::PublicKey::from_hex(&agent_hex).unwrap();
        let owner_secret =
            nostr::SecretKey::from_slice(owner.secret_key().as_secret_bytes()).unwrap();
        let owner_compat = nostr::Keys::new(owner_secret);
        let tag_json = sprout_sdk::nip_oa::compute_auth_tag(&owner_compat, &agent_compat, "")
            .expect("compute auth tag");
        let tag = sprout_sdk::nip_oa::parse_auth_tag(&tag_json).expect("parse auth tag");
        let tag = Tag::parse(tag.as_slice()).expect("convert auth tag");

        EventBuilder::new(kind, "{}")
            .tags([tag])
            .sign_with_keys(agent)
            .expect("sign kind0")
    }

    fn kind0_with_auth(agent: &Keys, owner: &Keys) -> nostr::Event {
        event_with_auth(Kind::Metadata, agent, owner)
    }

    #[test]
    fn extracts_verified_owner_from_kind0_auth_tag() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let event = kind0_with_auth(&agent, &owner);

        assert_eq!(
            extract_agent_owner_pubkey(&event),
            Some(owner.public_key().to_hex())
        );
    }

    #[test]
    fn extracts_verified_owner_from_message_auth_tag() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let event = event_with_auth(Kind::Custom(9), &agent, &owner);

        assert_eq!(
            extract_agent_owner_pubkey(&event),
            Some(owner.public_key().to_hex())
        );
    }

    #[test]
    fn ignores_kind0_without_auth_tag() {
        let agent = Keys::generate();
        let event = EventBuilder::new(Kind::Metadata, "{}")
            .sign_with_keys(&agent)
            .expect("sign kind0");

        assert_eq!(extract_agent_owner_pubkey(&event), None);
    }
}
