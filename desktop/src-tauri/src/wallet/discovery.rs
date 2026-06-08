use std::collections::HashMap;

use nostr::{Event, PublicKey};
use serde_json::{json, Value};

use crate::app_state::AppState;
use crate::relay::query_relay;

use super::parser::{normalize_username, username_target_from_payable};

struct UserProfileDiscovery {
    pubkey: PublicKey,
    display_name: String,
}

struct LexeBotDiscovery {
    owner: PublicKey,
    display_name: String,
    bolt12_offer: String,
    created_at: u64,
    event_id: String,
}

struct ResolvedLexeBotTarget {
    display_name: String,
    bolt12_offer: String,
}

pub(crate) async fn resolve_send_payable(
    state: &AppState,
    payable: &str,
) -> Result<String, String> {
    let Some(username) = username_target_from_payable(payable) else {
        if payable.trim().starts_with('@') {
            return Err("invalid @username payment target".to_string());
        }
        return Ok(payable.trim().to_string());
    };

    let target = resolve_lexebot_offer_for_username(state, &username).await?;
    eprintln!(
        "sprout-desktop: resolved WalletBot target @{username} to {}",
        target.display_name
    );
    Ok(target.bolt12_offer)
}

pub(crate) async fn resolve_bolt12_offer_for_pubkey(
    state: &AppState,
    pubkey: &str,
) -> Result<Option<String>, String> {
    let target = PublicKey::from_hex(pubkey)
        .map_err(|error| format!("invalid user pubkey for wallet lookup: {error}"))?;

    let direct_profiles = query_relay(
        state,
        &[json!({
            "kinds": [0],
            "authors": [target.to_hex()],
            "limit": 1
        })],
    )
    .await?;
    if let Some(offer) = direct_profiles
        .iter()
        .find_map(direct_bolt12_offer_from_profile)
    {
        return Ok(Some(offer));
    }

    let profiles = query_relay(state, &[json!({"kinds": [0], "limit": 500})]).await?;
    let records = profiles
        .iter()
        .filter_map(lexebot_discovery_from_profile)
        .filter(|record| record.owner == target)
        .collect::<Vec<_>>();

    Ok(latest_lexebot_discovery(&records).map(|record| record.bolt12_offer.clone()))
}

async fn resolve_lexebot_offer_for_username(
    state: &AppState,
    username: &str,
) -> Result<ResolvedLexeBotTarget, String> {
    let profiles = query_relay(state, &[json!({"kinds": [0], "limit": 500})]).await?;
    let owner = find_user_profile_by_username(&profiles, username)?;
    let records = profiles
        .iter()
        .filter_map(lexebot_discovery_from_profile)
        .filter(|record| record.owner == owner.pubkey)
        .collect::<Vec<_>>();

    match latest_lexebot_discovery(&records) {
        None => Err(format!(
            "found @{username}, but could not find a verified LexeBot BOLT12 offer for that user"
        )),
        Some(record) => Ok(ResolvedLexeBotTarget {
            display_name: record.display_name.clone(),
            bolt12_offer: record.bolt12_offer.clone(),
        }),
    }
}

fn latest_lexebot_discovery(records: &[LexeBotDiscovery]) -> Option<&LexeBotDiscovery> {
    records.iter().max_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.event_id.cmp(&right.event_id))
    })
}

fn find_user_profile_by_username(
    events: &[Event],
    username: &str,
) -> Result<UserProfileDiscovery, String> {
    let mut matches = HashMap::<String, UserProfileDiscovery>::new();
    for event in events {
        let Some(profile) = user_profile_from_profile(event, username) else {
            continue;
        };
        matches.entry(profile.pubkey.to_hex()).or_insert(profile);
    }

    let mut matches = matches.into_values().collect::<Vec<_>>();
    matches.sort_by(|left, right| left.display_name.cmp(&right.display_name));

    match matches.as_slice() {
        [] => Err(format!(
            "could not find a Sprout user profile matching @{username}"
        )),
        [profile] => Ok(UserProfileDiscovery {
            pubkey: profile.pubkey,
            display_name: profile.display_name.clone(),
        }),
        _ => {
            let labels = matches
                .iter()
                .map(|profile| {
                    format!(
                        "{} ({})",
                        profile.display_name,
                        profile.pubkey.to_hex().chars().take(8).collect::<String>()
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!(
                "found multiple Sprout user profiles matching @{username}: {labels}"
            ))
        }
    }
}

fn user_profile_from_profile(event: &Event, username: &str) -> Option<UserProfileDiscovery> {
    let metadata: Value = serde_json::from_str(&event.content).ok()?;
    if metadata.get("lexebot").is_some() || metadata.get("sparkbot").is_some() {
        return None;
    }
    if !profile_matches_username(&metadata, username) {
        return None;
    }

    Some(UserProfileDiscovery {
        pubkey: event.pubkey,
        display_name: profile_display_name(&metadata).unwrap_or_else(|| {
            format!(
                "@{}",
                event.pubkey.to_hex().chars().take(8).collect::<String>()
            )
        }),
    })
}

fn profile_matches_username(metadata: &Value, username: &str) -> bool {
    ["display_name", "displayName", "name", "username"]
        .into_iter()
        .filter_map(|field| metadata.get(field).and_then(Value::as_str))
        .filter_map(normalize_username)
        .any(|candidate| candidate == username)
        || metadata
            .get("nip05")
            .and_then(Value::as_str)
            .and_then(|nip05| nip05.split('@').next())
            .and_then(normalize_username)
            .is_some_and(|candidate| candidate == username)
}

fn profile_display_name(metadata: &Value) -> Option<String> {
    metadata
        .get("display_name")
        .or_else(|| metadata.get("displayName"))
        .or_else(|| metadata.get("name"))
        .and_then(Value::as_str)
        .and_then(clean_display_name)
}

fn clean_display_name(value: &str) -> Option<String> {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        None
    } else {
        Some(collapsed.chars().take(80).collect())
    }
}

fn lexebot_discovery_from_profile(event: &Event) -> Option<LexeBotDiscovery> {
    let metadata: Value = serde_json::from_str(&event.content).ok()?;
    let lexebot = metadata.get("lexebot")?;
    let owner = PublicKey::from_hex(lexebot.get("owner_pubkey")?.as_str()?).ok()?;
    let display_name = profile_display_name(&metadata)
        .map(|name| lexebot_command_name(&name))
        .unwrap_or_else(|| "LexeBot".to_string());
    let bolt12_offer = lexebot.get("bolt12_offer")?.as_str()?.to_string();
    if !bolt12_offer.to_ascii_lowercase().starts_with("lno1") {
        return None;
    }

    let owner_auth = lexebot.get("owner_auth")?;
    let attested_owner =
        sprout_sdk::nip_oa::verify_auth_tag(&owner_auth.to_string(), &event.pubkey).ok()?;
    if attested_owner != owner {
        return None;
    }

    Some(LexeBotDiscovery {
        owner,
        display_name,
        bolt12_offer,
        created_at: event.created_at.as_secs(),
        event_id: event.id.to_hex(),
    })
}

fn direct_bolt12_offer_from_profile(event: &Event) -> Option<String> {
    let metadata: Value = serde_json::from_str(&event.content).ok()?;
    let offer = [
        metadata.get("bolt12_offer"),
        metadata.get("bolt12"),
        metadata
            .get("wallet")
            .and_then(|wallet| wallet.get("bolt12_offer")),
        metadata
            .get("wallet")
            .and_then(|wallet| wallet.get("bolt12")),
        metadata
            .get("payments")
            .and_then(|payments| payments.get("bolt12_offer")),
        metadata
            .get("payments")
            .and_then(|payments| payments.get("bolt12")),
    ]
    .into_iter()
    .flatten()
    .filter_map(Value::as_str)
    .map(str::trim)
    .find(|offer| offer.to_ascii_lowercase().starts_with("lno1"))
    .map(str::to_string);
    offer
}

fn lexebot_command_name(display_name: &str) -> String {
    let normalized = display_name.trim();
    if normalized.eq_ignore_ascii_case("lexebot") {
        return "LexeBot".to_string();
    }
    normalized.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Timestamp};
    use serde_json::json;

    fn owner_auth_json(agent: &Keys, owner: &Keys) -> Value {
        let agent_hex = agent.public_key().to_hex();
        let agent_pubkey = nostr::PublicKey::from_hex(&agent_hex).expect("agent pubkey");
        let owner_secret =
            nostr::SecretKey::from_slice(owner.secret_key().as_secret_bytes()).unwrap();
        let owner_keys = nostr::Keys::new(owner_secret);
        let tag_json = sprout_sdk::nip_oa::compute_auth_tag(&owner_keys, &agent_pubkey, "")
            .expect("compute auth tag");
        serde_json::from_str(&tag_json).expect("owner auth json")
    }

    fn lexebot_event(owner: &Keys, agent: &Keys, offer: &str, created_at: u64) -> Event {
        let content = json!({
            "name": "LexeBot",
            "lexebot": {
                "owner_pubkey": owner.public_key().to_hex(),
                "bolt12_offer": offer,
                "owner_auth": owner_auth_json(agent, owner),
            }
        })
        .to_string();

        EventBuilder::new(Kind::Metadata, content)
            .custom_created_at(Timestamp::from(created_at))
            .sign_with_keys(agent)
            .expect("sign lexebot profile")
    }

    #[test]
    fn lexebot_discovery_includes_event_time() {
        let owner = Keys::generate();
        let agent = Keys::generate();
        let event = lexebot_event(&owner, &agent, "lno1validoffer", 1234);

        let record = lexebot_discovery_from_profile(&event).expect("verified lexebot");
        assert_eq!(record.owner, owner.public_key());
        assert_eq!(record.bolt12_offer, "lno1validoffer");
        assert_eq!(record.created_at, 1234);
        assert_eq!(record.event_id, event.id.to_hex());
    }

    #[test]
    fn latest_lexebot_discovery_prefers_most_recent_offer() {
        let owner = Keys::generate();
        let old_agent = Keys::generate();
        let new_agent = Keys::generate();
        let old_event = lexebot_event(&owner, &old_agent, "lno1oldoffer", 1000);
        let new_event = lexebot_event(&owner, &new_agent, "lno1newoffer", 2000);
        let records = vec![
            lexebot_discovery_from_profile(&old_event).expect("old verified lexebot"),
            lexebot_discovery_from_profile(&new_event).expect("new verified lexebot"),
        ];

        let record = latest_lexebot_discovery(&records).expect("latest record");
        assert_eq!(record.bolt12_offer, "lno1newoffer");
    }

    #[test]
    fn latest_lexebot_discovery_tie_breaks_deterministically() {
        let records = vec![
            LexeBotDiscovery {
                owner: Keys::generate().public_key(),
                display_name: "LexeBot".into(),
                bolt12_offer: "lno1lowid".into(),
                created_at: 1000,
                event_id: "0".repeat(64),
            },
            LexeBotDiscovery {
                owner: Keys::generate().public_key(),
                display_name: "LexeBot".into(),
                bolt12_offer: "lno1highid".into(),
                created_at: 1000,
                event_id: "f".repeat(64),
            },
        ];

        let record = latest_lexebot_discovery(&records).expect("latest record");
        assert_eq!(record.bolt12_offer, "lno1highid");
    }
}
