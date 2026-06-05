use nostr::{EventBuilder, EventId, Kind};
use uuid::Uuid;

use super::{check_pubkey, tag};

const KIND_REACTION: u16 = 7;
const CHANNEL_PAYMENT_POLICY_CONTENT: &str = "sprout-paid-channel-policy:v1";
const MAX_CHANNEL_PAYMENT_RECEIPT_CONTENT_CHARS: usize = 64;

fn channel_payment_receipt_content(purpose: &str, nonce: &str) -> Result<String, String> {
    let nonce = nonce.trim().replace('-', "");
    let content = format!("sprout-channel:{purpose}:{nonce}");
    if content.chars().count() > MAX_CHANNEL_PAYMENT_RECEIPT_CONTENT_CHARS {
        return Err(format!(
            "payment receipt marker exceeds maximum reaction content length of {MAX_CHANNEL_PAYMENT_RECEIPT_CONTENT_CHARS} characters"
        ));
    }
    Ok(content)
}

/// Kind 7 — paid-channel policy encoded as a relay-compatible reaction.
pub fn build_channel_payment_policy(
    channel_id: Uuid,
    metadata_event_id: EventId,
    recipient_pubkey: &str,
    paid_join_amount: Option<u64>,
    paid_post_amount: Option<u64>,
    payment_bolt12_offer: &str,
) -> Result<EventBuilder, String> {
    check_pubkey(recipient_pubkey)?;
    let join_amount = paid_join_amount.unwrap_or(0);
    let post_amount = paid_post_amount.unwrap_or(0);
    if join_amount == 0 && post_amount == 0 {
        return Err("paid channel policy requires a join or post amount".into());
    }
    let offer = payment_bolt12_offer.trim();
    if offer.is_empty() {
        return Err("paid channels require a BOLT12 offer".into());
    }

    let channel_id = channel_id.to_string();
    let metadata_event_id = metadata_event_id.to_hex();
    let recipient_pubkey = recipient_pubkey.to_ascii_lowercase();
    let join_amount = join_amount.to_string();
    let post_amount = post_amount.to_string();
    let tags = vec![
        tag(vec!["h", &channel_id])?,
        tag(vec!["e", &metadata_event_id, "", "root"])?,
        tag(vec!["p", &recipient_pubkey])?,
        tag(vec!["paid_join", &join_amount])?,
        tag(vec!["paid_post", &post_amount])?,
        tag(vec!["payment_recipient", &recipient_pubkey])?,
        tag(vec!["payment_bolt12_offer", offer])?,
        tag(vec!["payment_rail", "lexe-bolt12"])?,
        tag(vec!["status", "active"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(KIND_REACTION), CHANNEL_PAYMENT_POLICY_CONTENT).tags(tags))
}

/// Kind 7 — sender-confirmed paid-channel receipt encoded as a relay-compatible reaction.
pub fn build_channel_payment_receipt(
    channel_id: Uuid,
    metadata_event_id: EventId,
    recipient_pubkey: &str,
    amount_sats: u64,
    purpose: &str,
    nonce: &str,
) -> Result<EventBuilder, String> {
    check_pubkey(recipient_pubkey)?;
    if amount_sats == 0 {
        return Err("payment amount must be greater than zero".into());
    }
    if purpose != "join" && purpose != "post" {
        return Err("payment purpose must be join or post".into());
    }
    let nonce = nonce.trim();
    if nonce.is_empty() {
        return Err("payment nonce must not be empty".into());
    }
    let content = channel_payment_receipt_content(purpose, nonce)?;

    let channel_id = channel_id.to_string();
    let metadata_event_id = metadata_event_id.to_hex();
    let amount = amount_sats.to_string();
    let recipient_pubkey = recipient_pubkey.to_ascii_lowercase();
    let tags = vec![
        tag(vec!["h", &channel_id])?,
        tag(vec!["e", &metadata_event_id, "", "root"])?,
        tag(vec!["p", &recipient_pubkey])?,
        tag(vec!["purpose", purpose])?,
        tag(vec!["amount", &amount])?,
        tag(vec!["unit", "sprout-bitcoin-base-unit"])?,
        tag(vec!["nonce", nonce])?,
        tag(vec!["wallet", "lexe-bolt12"])?,
        tag(vec!["status", "sender-confirmed"])?,
    ];
    Ok(EventBuilder::new(Kind::Custom(KIND_REACTION), content).tags(tags))
}
