use nostr::{EventBuilder, EventId, Kind, Tag};
use uuid::Uuid;

const KIND_REACTION: u16 = 7;
const MAX_REACTION_CONTENT_CHARS: usize = 64;

fn tag(parts: Vec<&str>) -> Result<Tag, String> {
    Tag::parse(parts).map_err(|e| format!("invalid tag: {e}"))
}

fn check_pubkey(pubkey: &str) -> Result<(), String> {
    if pubkey.len() != 64 || !pubkey.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "pubkey must be a 64-character hex string (got {} chars)",
            pubkey.len()
        ));
    }
    Ok(())
}

fn receipt_content(tip_id: &str) -> Result<String, String> {
    let tip_id = tip_id.trim().replace('-', "");
    let content = format!("sprout-tip:{tip_id}");
    if content.chars().count() > MAX_REACTION_CONTENT_CHARS {
        return Err(format!(
            "tip receipt marker exceeds maximum reaction content length of {MAX_REACTION_CONTENT_CHARS} characters"
        ));
    }
    Ok(content)
}

/// Kind 7 — sender-confirmed message tip receipt encoded as a relay-compatible reaction.
pub fn build_message_tip_receipt(
    channel_id: Uuid,
    target_event_id: EventId,
    recipient_pubkey: &str,
    amount_sats: u64,
    tip_id: &str,
) -> Result<EventBuilder, String> {
    check_pubkey(recipient_pubkey)?;
    if amount_sats == 0 {
        return Err("tip amount must be greater than zero".into());
    }
    if tip_id.trim().is_empty() {
        return Err("tip id must not be empty".into());
    }

    let channel_id = channel_id.to_string();
    let target_event_id = target_event_id.to_hex();
    let recipient_pubkey = recipient_pubkey.to_ascii_lowercase();
    let amount = amount_sats.to_string();
    let tip_id = tip_id.trim().to_string();
    let content = receipt_content(&tip_id)?;
    let tags = vec![
        tag(vec!["h", &channel_id])?,
        tag(vec!["e", &target_event_id, "", "root"])?,
        tag(vec!["p", &recipient_pubkey])?,
        tag(vec!["amount", &amount])?,
        tag(vec!["unit", "sprout-bitcoin-base-unit"])?,
        tag(vec!["tip_id", &tip_id])?,
        tag(vec!["wallet", "lexe-bolt12"])?,
        tag(vec!["status", "sender-confirmed"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(KIND_REACTION), content).tags(tags))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::Keys;

    #[test]
    fn receipt_uses_existing_reaction_kind_with_unique_content() {
        let channel_id = Uuid::parse_str("1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb").unwrap();
        let message_id =
            EventId::from_hex("dc91b7ef91fa438a4c8d8904c55113d65e11db006bff3c045568a965514ceedd")
                .unwrap();
        let recipient = "5630c46628625e3d69756723ca537f20e3a5a035266015b079f406d21df7e44e";
        let tip_id = "00000000000040008000000000000000";

        let event = build_message_tip_receipt(channel_id, message_id, recipient, 10, tip_id)
            .unwrap()
            .sign_with_keys(&Keys::generate())
            .unwrap();

        assert_eq!(event.kind, Kind::Custom(KIND_REACTION));
        assert_eq!(event.content, "sprout-tip:00000000000040008000000000000000");
        assert!(event.content.chars().count() <= MAX_REACTION_CONTENT_CHARS);

        let tags: Vec<Vec<String>> = event
            .tags
            .iter()
            .map(|tag| tag.as_slice().to_vec())
            .collect();
        assert!(tags.contains(&vec!["amount".to_string(), "10".to_string()]));
        assert!(tags.contains(&vec!["wallet".to_string(), "lexe-bolt12".to_string()]));
        assert!(tags.contains(&vec!["status".to_string(), "sender-confirmed".to_string()]));
    }
}
