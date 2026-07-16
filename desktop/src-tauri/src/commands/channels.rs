use std::collections::HashMap;

use nostr::EventId;
use tauri::{AppHandle, State};

use crate::{
    app_state::AppState,
    events,
    models::{ChannelDetailInfo, ChannelInfo, ChannelMembersResponse, ChannelPaymentPolicyInfo},
    nostr_convert,
    relay::{query_relay, submit_event, submit_event_with_keys},
    wallet,
};

// ── Reads (pure-nostr via /query) ────────────────────────────────────────────

const CHANNEL_PAYMENT_POLICY_CONTENT: &str = "sprout-paid-channel-policy:v1";
const KIND_CHANNEL_CREATE: u16 = 9007;
const KIND_REACTION: u16 = 7;
const DIRECTORY_PAGE_SIZE: usize = 500;

fn advance_directory_cursor(filter: &mut serde_json::Value, page: &[nostr::Event]) {
    let last = page
        .last()
        .expect("a full relay page always has a last event");
    filter["until"] = serde_json::json!(last.created_at.as_secs());
    filter["before_id"] = serde_json::json!(last.id.to_hex());
}

/// Fetch every page for a historical relay filter using the relay's composite
/// `(until, before_id)` cursor. A timestamp-only cursor can skip rows when more
/// than one page of events shares the same second.
async fn query_relay_all(
    state: &AppState,
    mut filter: serde_json::Value,
) -> Result<Vec<nostr::Event>, String> {
    filter["limit"] = serde_json::json!(DIRECTORY_PAGE_SIZE);
    let mut all = Vec::new();

    loop {
        let page = query_relay(state, &[filter.clone()]).await?;
        let done = page.len() < DIRECTORY_PAGE_SIZE;

        if !done {
            advance_directory_cursor(&mut filter, &page);
        }

        all.extend(page);
        if done {
            return Ok(all);
        }
    }
}

fn first_tag_value<'a>(event: &'a nostr::Event, name: &str) -> Option<&'a str> {
    event.tags.iter().find_map(|tag| {
        let parts = tag.as_slice();
        (parts.len() >= 2 && parts[0] == name).then(|| parts[1].as_str())
    })
}

fn keep_newest_policy(
    policies: &mut HashMap<String, (u64, ChannelPaymentPolicyInfo)>,
    key: &str,
    created_at: u64,
    policy: &ChannelPaymentPolicyInfo,
) {
    let entry = policies
        .entry(key.to_string())
        .or_insert_with(|| (created_at, policy.clone()));
    if created_at >= entry.0 {
        *entry = (created_at, policy.clone());
    }
}

#[derive(Clone, Default)]
struct ChannelHiveMetadata {
    hive_wallet_bolt12_offer: Option<String>,
}

fn collect_payment_policies(
    events: Vec<nostr::Event>,
    newest_by_key: &mut HashMap<String, (u64, ChannelPaymentPolicyInfo)>,
) {
    for event in events {
        let kind = event.kind.as_u16();
        let policy_event = kind == KIND_REACTION && event.content == CHANNEL_PAYMENT_POLICY_CONTENT;
        let create_event = kind == KIND_CHANNEL_CREATE;
        if !policy_event && !create_event {
            continue;
        }
        let Some(policy) = nostr_convert::payment_policy_from_event(&event) else {
            continue;
        };
        let created_at = event.created_at.as_secs();
        if policy_event {
            if let Some(metadata_event_id) = first_tag_value(&event, "e") {
                keep_newest_policy(newest_by_key, metadata_event_id, created_at, &policy);
            }
        }
        if let Some(channel_id) = first_tag_value(&event, "h") {
            keep_newest_policy(newest_by_key, channel_id, created_at, &policy);
        }
    }
}

fn collect_hive_metadata(
    events: Vec<nostr::Event>,
    newest_by_channel: &mut HashMap<String, (u64, ChannelHiveMetadata)>,
) {
    for event in events {
        let kind = event.kind.as_u16();
        let marker_event =
            kind == KIND_REACTION && events::is_hive_channel_wallet_content(&event.content);
        let create_event = kind == KIND_CHANNEL_CREATE;
        if !marker_event && !create_event {
            continue;
        }
        let Some(channel_id) = first_tag_value(&event, "h") else {
            continue;
        };
        let hive_channel = first_tag_value(&event, "hive_channel")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        if !hive_channel {
            continue;
        }
        let metadata = ChannelHiveMetadata {
            hive_wallet_bolt12_offer: first_tag_value(&event, "hive_wallet_bolt12_offer")
                .map(str::to_string),
        };
        let created_at = event.created_at.as_secs();
        let entry = newest_by_channel
            .entry(channel_id.to_string())
            .or_insert_with(|| (created_at, metadata.clone()));
        if created_at >= entry.0 {
            *entry = (created_at, metadata);
        }
    }
}

async fn fetch_channel_payment_policies(
    state: &AppState,
    metadata_events: &[nostr::Event],
) -> Result<HashMap<String, ChannelPaymentPolicyInfo>, String> {
    let metadata_event_ids: Vec<String> = metadata_events
        .iter()
        .map(|event| event.id.to_hex())
        .collect();
    let mut channel_ids: Vec<String> = metadata_events
        .iter()
        .filter_map(|event| first_tag_value(event, "d").map(str::to_string))
        .collect();
    channel_ids.sort();
    channel_ids.dedup();

    if metadata_event_ids.is_empty() && channel_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let mut newest_by_key: HashMap<String, (u64, ChannelPaymentPolicyInfo)> = HashMap::new();
    if !metadata_event_ids.is_empty() {
        let policy_events = query_relay(
            state,
            &[serde_json::json!({
                "kinds": [KIND_REACTION],
                "#e": metadata_event_ids,
                "limit": 5000,
            })],
        )
        .await
        .unwrap_or_default();
        collect_payment_policies(policy_events, &mut newest_by_key);
    }
    if !channel_ids.is_empty() {
        let policy_marker_events = query_relay(
            state,
            &[serde_json::json!({
                "kinds": [KIND_REACTION],
                "#h": channel_ids.clone(),
                "limit": 5000,
            })],
        )
        .await
        .unwrap_or_default();
        collect_payment_policies(policy_marker_events, &mut newest_by_key);

        let create_events = query_relay(
            state,
            &[serde_json::json!({
                "kinds": [KIND_CHANNEL_CREATE],
                "#h": channel_ids,
                "limit": 5000,
            })],
        )
        .await
        .unwrap_or_default();
        collect_payment_policies(create_events, &mut newest_by_key);
    }

    Ok(newest_by_key
        .into_iter()
        .map(|(key, (_, policy))| (key, policy))
        .collect())
}

async fn fetch_channel_hive_metadata(
    state: &AppState,
    metadata_events: &[nostr::Event],
) -> Result<HashMap<String, ChannelHiveMetadata>, String> {
    let mut channel_ids: Vec<String> = metadata_events
        .iter()
        .filter_map(|event| first_tag_value(event, "d").map(str::to_string))
        .collect();
    channel_ids.sort();
    channel_ids.dedup();
    if channel_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let create_events = query_relay(
        state,
        &[serde_json::json!({
            "kinds": [KIND_CHANNEL_CREATE, KIND_REACTION],
            "#h": channel_ids,
            "limit": 5000,
        })],
    )
    .await
    .unwrap_or_default();

    let mut newest_by_channel = HashMap::new();
    collect_hive_metadata(create_events, &mut newest_by_channel);
    Ok(newest_by_channel
        .into_iter()
        .map(|(key, (_, metadata))| (key, metadata))
        .collect())
}

fn apply_payment_policy(
    mut channel: ChannelInfo,
    policies: &HashMap<String, ChannelPaymentPolicyInfo>,
) -> ChannelInfo {
    if channel.payment_policy.is_none() {
        channel.payment_policy = policies
            .get(&channel.metadata_event_id)
            .or_else(|| policies.get(&channel.id))
            .cloned();
    }
    channel
}

fn apply_hive_metadata(
    mut channel: ChannelInfo,
    metadata_by_channel: &HashMap<String, ChannelHiveMetadata>,
) -> ChannelInfo {
    if let Some(metadata) = metadata_by_channel.get(&channel.id) {
        channel.hive_channel = true;
        channel.hive_wallet_bolt12_offer = metadata.hive_wallet_bolt12_offer.clone();
    }
    channel
}

fn apply_detail_payment_policy(
    mut channel: ChannelDetailInfo,
    policies: &HashMap<String, ChannelPaymentPolicyInfo>,
) -> ChannelDetailInfo {
    if channel.payment_policy.is_none() {
        channel.payment_policy = policies
            .get(&channel.metadata_event_id)
            .or_else(|| policies.get(&channel.id))
            .cloned();
    }
    channel
}

fn apply_detail_hive_metadata(
    mut channel: ChannelDetailInfo,
    metadata_by_channel: &HashMap<String, ChannelHiveMetadata>,
) -> ChannelDetailInfo {
    if let Some(metadata) = metadata_by_channel.get(&channel.id) {
        channel.hive_channel = true;
        channel.hive_wallet_bolt12_offer = metadata.hive_wallet_bolt12_offer.clone();
    }
    channel
}

/// Whether an open channel not yet in the real member set should still be
/// classified `is_member=true` via the pending-owner overlay. Pulled out of
/// `get_channels`'s open-channel branch so the exact `(d_tag, my_pubkey,
/// overlay) -> is_member` decision — including the identity binding that
/// keeps one identity's pending entry from covering another's — is directly
/// unit-testable without going through the async relay-backed command.
fn classify_pending_owner(state: &AppState, my_pubkey: &str, d_tag: Option<&str>) -> bool {
    d_tag.is_some_and(|d| state.is_pending_owned_channel(my_pubkey, d))
}

#[tauri::command]
pub async fn get_channels(state: State<'_, AppState>) -> Result<Vec<ChannelInfo>, String> {
    let _profile_start = std::time::Instant::now();
    let my_pubkey = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        keys.public_key().to_hex()
    };

    // Step 1: find all kind:39002 (members) events that mention me, then
    // pull the channel ids out of their `d` tags.
    let member_events = query_relay_all(
        &state,
        serde_json::json!({"kinds": [39002], "#p": [&my_pubkey]}),
    )
    .await?;

    #[cfg(debug_assertions)]
    let t_members = _profile_start.elapsed();

    let mut channel_ids: Vec<String> = member_events
        .iter()
        .filter_map(|ev| {
            ev.tags.iter().find_map(|t| {
                let s = t.as_slice();
                if s.len() >= 2 && s[0] == "d" {
                    Some(s[1].clone())
                } else {
                    None
                }
            })
        })
        .collect();
    channel_ids.sort();
    channel_ids.dedup();

    // The real kind:39002 membership has now resolved for these channels —
    // drop them from the pending-owner overlay (see `AppState::pending_owned_channels`)
    // so a channel this identity created no longer speaks through the overlay
    // once genuine membership is observable, and a later leave correctly
    // flips it back to `is_member=false`.
    for id in &channel_ids {
        state.clear_pending_owned_channel(&my_pubkey, id);
    }

    // Step 2: fetch channel metadata events (kind:39000) for member channels.
    // kind:39000 is addressable: exactly one event per `d` tag, so a limit
    // equal to the number of ids is both necessary and sufficient. Without
    // an explicit limit, multi-value `#d` filters fall through to the relay's
    // default LIMIT and can drop results when there are many channels.
    let meta_events = if !channel_ids.is_empty() {
        query_relay(
            &state,
            &[serde_json::json!({
                "kinds": [39000],
                "#d": channel_ids,
                "limit": channel_ids.len(),
            })],
        )
        .await?
    } else {
        Vec::new()
    };

    #[cfg(debug_assertions)]
    let t_member_meta = _profile_start.elapsed();

    // Step 3: fetch ALL open channel metadata so the channel browser can show
    // discoverable channels the user hasn't joined yet. The relay's access
    // control allows reading kind:39000 for open channels regardless of membership.
    let open_meta_events = query_relay_all(&state, serde_json::json!({"kinds": [39000]})).await?;

    let mut all_meta_events = meta_events.clone();
    all_meta_events.extend(open_meta_events.iter().cloned());
    let payment_policies = fetch_channel_payment_policies(&state, &all_meta_events)
        .await
        .unwrap_or_default();
    let hive_metadata = fetch_channel_hive_metadata(&state, &all_meta_events)
        .await
        .unwrap_or_default();

    #[cfg(debug_assertions)]
    let t_open_meta = _profile_start.elapsed();

    // Merge: member channels (marked as member) + open channels (not yet joined).
    let member_d_tags: std::collections::HashSet<String> = meta_events
        .iter()
        .filter_map(|ev| {
            ev.tags.iter().find_map(|t| {
                let s = t.as_slice();
                if s.len() >= 2 && s[0] == "d" {
                    Some(s[1].clone())
                } else {
                    None
                }
            })
        })
        .collect();

    let mut channels = Vec::with_capacity(meta_events.len() + open_meta_events.len());
    for ev in &meta_events {
        if let Ok(info) = nostr_convert::channel_info_from_event(ev, None, Some(true)) {
            channels.push(apply_hive_metadata(
                apply_payment_policy(info, &payment_policies),
                &hive_metadata,
            ));
        }
    }
    for ev in &open_meta_events {
        // Skip channels already included from the member set.
        let d_tag = ev.tags.iter().find_map(|t| {
            let s = t.as_slice();
            if s.len() >= 2 && s[0] == "d" {
                Some(s[1].clone())
            } else {
                None
            }
        });
        if let Some(ref d) = d_tag {
            if member_d_tags.contains(d) {
                continue;
            }
        }
        // The overlay (`AppState::pending_owned_channels`) marks channels this
        // identity just created via `create_channel` whose kind:39002 owner
        // membership hasn't propagated yet (#1761) — a fresh channel has no
        // member event and would otherwise fall through to `is_member=false`
        // here, disabling the owner's own composer until that snapshot lands.
        // The overlay can only be populated by this process's own
        // `create_channel` call (never by relay data) and is keyed by
        // `(my_pubkey, d_tag)`, so it adds no trust-boundary risk and can
        // never speak for a channel a different identity created; `channel_ids`
        // above clears it once real membership is observed for `my_pubkey`.
        let is_pending_owner = classify_pending_owner(&state, &my_pubkey, d_tag.as_deref());
        if let Ok(info) = nostr_convert::channel_info_from_event(ev, None, Some(is_pending_owner)) {
            channels.push(apply_hive_metadata(
                apply_payment_policy(info, &payment_policies),
                &hive_metadata,
            ));
        }
    }

    // Populate member_count by batch-fetching kind:39002 for every listed
    // channel and counting unique p-tag pubkeys. The kind:40901 summary
    // sidecar that channel_info_from_event prefers isn't emitted by the
    // relay today, so without this step every channel reports 0 members
    // in the channel browser (the active-channel top bar masks this with
    // its own live members query).
    let all_d_tags: Vec<String> = channels.iter().map(|c| c.id.clone()).collect();
    if !all_d_tags.is_empty() {
        let members_events = query_relay(
            &state,
            &[serde_json::json!({
                "kinds": [39002],
                "#d": all_d_tags,
                "limit": all_d_tags.len(),
            })],
        )
        .await
        .unwrap_or_default();

        let membership = collect_members_by_channel(&members_events, &my_pubkey);
        for channel in &mut channels {
            if let Some(info) = membership.get(&channel.id) {
                channel.member_count = info.count;
                channel.member_pubkeys = info.pubkeys.clone();
                channel.current_user_role = info.current_user_role.clone();
            }
        }
    }

    #[cfg(debug_assertions)]
    let t_member_counts = _profile_start.elapsed();

    // Populate last_message_at by fetching the most recent human message per
    // channel. Uses per-channel filters (single #h value each) so the relay can
    // push the query to its indexed channel_id column. Multi-value #h is NOT
    // SQL-pushed and would silently drop quieter channels under the global limit.
    let channel_ids: Vec<String> = channels.iter().map(|c| c.id.clone()).collect();
    if !channel_ids.is_empty() {
        let filters: Vec<serde_json::Value> = channel_ids
            .iter()
            .map(|id| {
                serde_json::json!({
                    "kinds": [9, 40002],
                    "#h": [id],
                    "limit": 1
                })
            })
            .collect();

        let message_events = query_relay(&state, &filters).await.unwrap_or_default();

        let mut last_message_by_channel: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        for ev in &message_events {
            if let Some(ch_id) = ev.tags.iter().find_map(|t| {
                let s = t.as_slice();
                (s.len() >= 2 && s[0] == "h").then(|| s[1].clone())
            }) {
                let ts = ev.created_at.as_secs();
                last_message_by_channel
                    .entry(ch_id)
                    .and_modify(|existing| {
                        if ts > *existing {
                            *existing = ts;
                        }
                    })
                    .or_insert(ts);
            }
        }

        for channel in &mut channels {
            if let Some(&ts) = last_message_by_channel.get(&channel.id) {
                channel.last_message_at = Some(nostr_convert::timestamp_to_iso(ts));
            }
        }
    }

    #[cfg(debug_assertions)]
    let t_last_message = _profile_start.elapsed();

    // NIP-DV: drop DMs the viewer has hidden. The relay maintains a per-viewer
    // parameterized-replaceable snapshot (kind:30622, d=my pubkey) whose `h`
    // tags list currently-hidden DM channel ids. The snapshot also carries
    // `p`=my pubkey so the relay's #p read-gate scopes it to me; we query by
    // `#p` for that reason. Reading the latest one is the only way the client
    // learns hide state, which the relay tracks privately.
    let hidden_dms: std::collections::HashSet<String> = {
        let events = query_relay(
            &state,
            &[serde_json::json!({
                "kinds": [buzz_core_pkg::kind::KIND_DM_VISIBILITY],
                "#p": [&my_pubkey],
                "limit": 1,
            })],
        )
        .await
        .unwrap_or_default();
        events
            .iter()
            .max_by_key(|e| e.created_at.as_secs())
            .map(|e| {
                e.tags
                    .iter()
                    .filter_map(|t| {
                        let s = t.as_slice();
                        (s.len() >= 2 && s[0] == "h").then(|| s[1].clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    if !hidden_dms.is_empty() {
        channels.retain(|c| c.channel_type != "dm" || !hidden_dms.contains(&c.id));
    }

    #[cfg(debug_assertions)]
    {
        let total = _profile_start.elapsed();
        eprintln!(
            "buzz-desktop: get_channels profile channels={} members={:?} member_meta={:?} open_meta={:?} member_counts={:?} last_message={:?} hidden_dm={:?} total={:?}",
            channels.len(),
            t_members,
            t_member_meta - t_members,
            t_open_meta - t_member_meta,
            t_member_counts - t_open_meta,
            t_last_message - t_member_counts,
            total - t_last_message,
            total,
        );
    }

    Ok(channels)
}

struct ChannelMembership {
    count: i64,
    pubkeys: Vec<String>,
    current_user_role: Option<String>,
}

/// Build a `channel_id → membership` map from a batch of kind:39002 events.
/// Events without a `d` tag are skipped; member dedupe is delegated to
/// [`nostr_convert::channel_members_from_event`] so the parsing rules match the
/// per-channel `get_channel_members` path.
fn collect_members_by_channel(
    events: &[nostr::Event],
    current_pubkey: &str,
) -> std::collections::HashMap<String, ChannelMembership> {
    let mut map: std::collections::HashMap<String, ChannelMembership> =
        std::collections::HashMap::with_capacity(events.len());
    let current_pubkey = current_pubkey.to_ascii_lowercase();
    for ev in events {
        let Some(d) = ev.tags.iter().find_map(|t| {
            let s = t.as_slice();
            (s.len() >= 2 && s[0] == "d").then(|| s[1].clone())
        }) else {
            continue;
        };
        let Ok(resp) = nostr_convert::channel_members_from_event(ev) else {
            continue;
        };
        let pubkeys: Vec<String> = resp.members.iter().map(|m| m.pubkey.clone()).collect();
        let current_user_role = resp
            .members
            .iter()
            .find(|member| member.pubkey.eq_ignore_ascii_case(&current_pubkey))
            .map(|member| member.role.clone());
        map.insert(
            d,
            ChannelMembership {
                count: pubkeys.len() as i64,
                pubkeys,
                current_user_role,
            },
        );
    }
    map
}

fn channel_payment_receipt_amount(
    event: &nostr::Event,
    channel_id: &str,
    purposes: &[&str],
) -> Option<u64> {
    let purpose = first_tag_value(event, "purpose")?;
    if !purposes.contains(&purpose) {
        return None;
    }
    let expected_content_prefix = format!("sprout-channel:{purpose}:");
    if !event.content.starts_with(&expected_content_prefix) {
        return None;
    }
    if first_tag_value(event, "h") != Some(channel_id) {
        return None;
    }
    if first_tag_value(event, "status") != Some("sender-confirmed") {
        return None;
    }
    first_tag_value(event, "amount")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|amount| *amount > 0)
}

#[tauri::command]
pub async fn get_channel_post_spend_total(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    parse_channel_uuid(&channel_id)?;
    let my_pubkey = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        keys.public_key().to_hex()
    };

    let receipt_events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [KIND_REACTION],
            "authors": [my_pubkey],
            "#h": [channel_id.clone()],
            "limit": 5000,
        })],
    )
    .await?;

    Ok(receipt_events
        .iter()
        .filter_map(|event| channel_payment_receipt_amount(event, &channel_id, &["join", "post"]))
        .sum())
}

#[tauri::command]
pub async fn get_channel_earned_total(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<u64, String> {
    parse_channel_uuid(&channel_id)?;
    let my_pubkey = {
        let keys = state.keys.lock().map_err(|e| e.to_string())?;
        keys.public_key().to_hex()
    };

    let receipt_events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [KIND_REACTION],
            "#h": [channel_id.clone()],
            "#p": [my_pubkey],
            "limit": 5000,
        })],
    )
    .await?;

    Ok(receipt_events
        .iter()
        .filter_map(|event| channel_payment_receipt_amount(event, &channel_id, &["join", "post"]))
        .sum())
}

#[tauri::command]
pub async fn get_channel_details(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [39000],
            "#d": [channel_id],
            "limit": 1
        })],
    )
    .await?;

    let event = events
        .first()
        .ok_or_else(|| "channel not found".to_string())?;
    let payment_policies = fetch_channel_payment_policies(&state, std::slice::from_ref(event))
        .await
        .unwrap_or_default();
    let hive_metadata = fetch_channel_hive_metadata(&state, std::slice::from_ref(event))
        .await
        .unwrap_or_default();
    let detail = nostr_convert::channel_detail_from_event(event)?;
    Ok(apply_detail_hive_metadata(
        apply_detail_payment_policy(detail, &payment_policies),
        &hive_metadata,
    ))
}

#[tauri::command]
pub async fn get_channel_members(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<ChannelMembersResponse, String> {
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [39002],
            "#d": [channel_id],
            "limit": 1
        })],
    )
    .await?;

    let mut response = events
        .first()
        .map(nostr_convert::channel_members_from_event)
        .transpose()?
        .ok_or_else(|| "channel members not found".to_string())?;

    // Batch-fetch kind:0 profiles to populate display names.
    let pubkeys: Vec<String> = response.members.iter().map(|m| m.pubkey.clone()).collect();
    if !pubkeys.is_empty() {
        let profile_events = query_relay(
            &state,
            &[serde_json::json!({
                "kinds": [0],
                "authors": pubkeys,
                "limit": pubkeys.len()
            })],
        )
        .await
        .unwrap_or_default();

        // Build pubkey → profile display metadata from kind:0 events.
        let mut profile_map = std::collections::HashMap::new();
        for ev in &profile_events {
            let pk = ev.pubkey.to_hex();
            if let Ok(profile) = nostr_convert::profile_info_from_event(ev) {
                profile_map.insert(
                    pk,
                    (
                        profile.display_name,
                        nostr_convert::profile_has_valid_oa_owner(ev),
                    ),
                );
            }
        }

        // Populate profile-derived fields on each member.
        for member in &mut response.members {
            if member.role == "bot" {
                member.is_agent = true;
            }
            if let Some((display_name, is_agent)) = profile_map.get(&member.pubkey) {
                if member.display_name.is_none() {
                    member.display_name = display_name.clone();
                }
                member.is_agent = member.is_agent || *is_agent;
            }
        }
    }

    Ok(response)
}

// ── Writes (signed events) ──────────────────────────────────────────────────

fn parse_channel_uuid(channel_id: &str) -> Result<uuid::Uuid, String> {
    uuid::Uuid::parse_str(channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))
}

#[tauri::command]
pub async fn create_channel(
    name: String,
    channel_type: String,
    visibility: String,
    description: Option<String>,
    ttl_seconds: Option<i32>,
    paid_join_amount: Option<u64>,
    paid_post_amount: Option<u64>,
    payment_bolt12_offer: Option<String>,
    hive_channel: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ChannelInfo, String> {
    let channel_uuid = uuid::Uuid::new_v4();

    let vis = match visibility.as_str() {
        "open" | "private" => visibility.as_str(),
        other => return Err(format!("invalid visibility: {other}")),
    };
    let ct = match channel_type.as_str() {
        "stream" | "forum" => channel_type.as_str(),
        other => return Err(format!("invalid channel_type: {other}")),
    };
    let hive_channel = hive_channel.unwrap_or(false);
    let join_amount = paid_join_amount.unwrap_or(0);
    let post_amount = paid_post_amount.unwrap_or(0);
    if hive_channel && vis != "private" {
        return Err("hive channels must be private".to_string());
    }
    if hive_channel && (join_amount > 0 || post_amount > 0) {
        return Err("hive channels cannot also be paid channels".to_string());
    }
    let hive_wallet_setup = if hive_channel {
        Some(wallet::create_hive_channel_wallet(&app, &state, channel_uuid).await?)
    } else {
        None
    };
    let hive_wallet_provider = hive_wallet_setup
        .as_ref()
        .map(|setup| setup.wallet_provider.as_storage_value());
    let hive_wallet_bolt12_offer = hive_wallet_setup
        .as_ref()
        .and_then(|setup| setup.bolt12_offer.as_deref());

    let builder = events::build_create_channel(
        channel_uuid,
        &name,
        vis,
        ct,
        description.as_deref(),
        ttl_seconds,
        paid_join_amount,
        paid_post_amount,
        payment_bolt12_offer.as_deref(),
        hive_channel,
        hive_wallet_provider,
        hive_wallet_bolt12_offer,
    )?;

    // Capture the signing identity before submission so the pending-owner
    // mark below is bound to whoever actually signed this create — not
    // whoever `state.keys` holds once the network round-trip completes. An
    // in-process identity swap while the request is in flight must not be
    // able to retarget the mark onto the new identity.
    let creator_keys = state.signing_keys()?;
    let creator_pubkey = creator_keys.public_key().to_hex();
    submit_event_with_keys(builder, &state, &creator_keys, None).await?;

    // Mark this channel pending-owner: we just created it, so we know we're
    // the owner, but the relay's kind:39002 membership entry (#1761) is
    // provisioned asynchronously. `get_channels` consults this overlay to
    // classify us as `is_member=true` until that entry is observable. Bound
    // to the identity that signed the create above, so an in-process
    // identity swap can neither inherit nor retarget this entry.
    let channel_uuid_string = channel_uuid.to_string();
    state.mark_pending_owned_channel(&creator_pubkey, &channel_uuid_string);

    // Re-fetch the canonical metadata event to return ChannelInfo.
    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [39000],
            "#d": [channel_uuid_string],
            "limit": 1
        })],
    )
    .await?;

    let mut channel = events
        .first()
        .map(|ev| nostr_convert::channel_info_from_event(ev, None, None))
        .transpose()?
        .ok_or_else(|| "channel created but metadata not yet available".to_string())?;
    channel.current_user_role = Some("owner".to_string());
    if hive_channel {
        channel.hive_channel = true;
        channel.hive_wallet_bolt12_offer = hive_wallet_setup
            .as_ref()
            .and_then(|setup| setup.bolt12_offer.clone());
    }

    if join_amount > 0 || post_amount > 0 {
        let offer = payment_bolt12_offer
            .as_deref()
            .map(str::trim)
            .filter(|offer| !offer.is_empty())
            .ok_or_else(|| "paid channels require a BOLT12 offer".to_string())?;
        let recipient_pubkey = {
            let keys = state.keys.lock().map_err(|e| e.to_string())?;
            keys.public_key().to_hex()
        };
        let metadata_event_id = EventId::from_hex(&channel.metadata_event_id)
            .map_err(|error| format!("invalid channel metadata event id: {error}"))?;
        let policy_builder = events::build_channel_payment_policy(
            channel_uuid,
            metadata_event_id,
            &recipient_pubkey,
            paid_join_amount,
            paid_post_amount,
            offer,
        )?;
        submit_event(policy_builder, &state).await?;
        channel.payment_policy = Some(ChannelPaymentPolicyInfo {
            join_payment_required: join_amount > 0,
            join_amount_base_units: join_amount,
            post_payment_required: post_amount > 0,
            post_amount_base_units: post_amount,
            payment_recipient_pubkey: recipient_pubkey,
            payment_recipient_bolt12_offer: offer.to_string(),
            payment_rail: "lexe-bolt12".to_string(),
        });
    }

    Ok(channel)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelInput {
    pub channel_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    /// Absent = leave unchanged, `null` = clear (permanent), seconds = set.
    #[serde(default, deserialize_with = "crate::util::double_option")]
    pub ttl_seconds: Option<Option<i32>>,
}

#[tauri::command]
pub async fn update_channel(
    input: UpdateChannelInput,
    state: State<'_, AppState>,
) -> Result<ChannelDetailInfo, String> {
    let uuid = parse_channel_uuid(&input.channel_id)?;
    let builder = events::build_update_channel(
        uuid,
        input.name.as_deref(),
        input.description.as_deref(),
        input.visibility.as_deref(),
        input.ttl_seconds,
    )?;
    submit_event(builder, &state).await?;

    let events = query_relay(
        &state,
        &[serde_json::json!({
            "kinds": [39000],
            "#d": [input.channel_id],
            "limit": 1
        })],
    )
    .await?;

    let event = events
        .first()
        .ok_or_else(|| "channel updated but metadata not yet available".to_string())?;
    let payment_policies = fetch_channel_payment_policies(&state, std::slice::from_ref(event))
        .await
        .unwrap_or_default();
    let hive_metadata = fetch_channel_hive_metadata(&state, std::slice::from_ref(event))
        .await
        .unwrap_or_default();
    let detail = nostr_convert::channel_detail_from_event(event)?;
    Ok(apply_detail_hive_metadata(
        apply_detail_payment_policy(detail, &payment_policies),
        &hive_metadata,
    ))
}

#[tauri::command]
pub async fn set_channel_topic(
    channel_id: String,
    topic: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_set_topic(uuid, &topic)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn set_channel_purpose(
    channel_id: String,
    purpose: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_set_purpose(uuid, &purpose)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn archive_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_archive(uuid)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn unarchive_channel(
    channel_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_unarchive(uuid)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_delete_channel(uuid)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn add_channel_members(
    channel_id: String,
    pubkeys: Vec<String>,
    role: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let role_str = match role.as_deref() {
        Some("admin") => Some("admin"),
        Some("bot") => Some("bot"),
        Some("guest") => Some("guest"),
        Some("member") | None => None,
        Some(other) => return Err(format!("invalid role: {other}")),
    };

    let mut added = Vec::new();
    let mut errors = Vec::<serde_json::Value>::new();

    for pubkey in &pubkeys {
        let builder = match events::build_add_member(uuid, pubkey, role_str) {
            Ok(b) => b,
            Err(e) => {
                errors.push(serde_json::json!({"pubkey": pubkey, "error": e}));
                continue;
            }
        };
        match submit_event(builder, &state).await {
            Ok(_) => added.push(pubkey.clone()),
            Err(e) => errors.push(serde_json::json!({"pubkey": pubkey, "error": e})),
        }
    }

    Ok(serde_json::json!({ "added": added, "errors": errors }))
}

#[tauri::command]
pub async fn remove_channel_member(
    channel_id: String,
    pubkey: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_remove_member(uuid, &pubkey)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn change_channel_member_role(
    channel_id: String,
    pubkey: String,
    role: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    // Only allow permission-tier roles for humans and bot/guest for bots.
    // Owner changes require a dedicated transfer-ownership flow.
    let role_str = match role.as_str() {
        "admin" | "member" | "guest" | "bot" => role.as_str(),
        "owner" => return Err("cannot assign owner role — use transfer ownership".into()),
        other => return Err(format!("invalid role: {other}")),
    };
    let builder = events::build_add_member(uuid, &pubkey, Some(role_str))?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn join_channel(
    channel_id: String,
    payment_receipt_event_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_join(uuid, payment_receipt_event_id.as_deref())?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[tauri::command]
pub async fn leave_channel(channel_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = parse_channel_uuid(&channel_id)?;
    let builder = events::build_leave(uuid)?;
    submit_event(builder, &state).await?;
    Ok(())
}

#[cfg(test)]
#[path = "channels_tests.rs"]
mod tests;
