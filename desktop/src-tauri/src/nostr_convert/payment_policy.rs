use nostr::Event;

use crate::models::ChannelPaymentPolicyInfo;

use super::first_tag_value;

pub fn payment_policy_from_event(event: &Event) -> Option<ChannelPaymentPolicyInfo> {
    let join_amount = first_tag_value(event, "paid_join")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let post_amount = first_tag_value(event, "paid_post")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    if join_amount == 0 && post_amount == 0 {
        return None;
    }
    let offer = first_tag_value(event, "payment_bolt12_offer")
        .map(str::trim)
        .filter(|offer| !offer.is_empty())?;
    let recipient_pubkey = first_tag_value(event, "payment_recipient")
        .map(str::to_string)
        .unwrap_or_else(|| event.pubkey.to_hex());

    Some(ChannelPaymentPolicyInfo {
        join_payment_required: join_amount > 0,
        join_amount_base_units: join_amount,
        post_payment_required: post_amount > 0,
        post_amount_base_units: post_amount,
        payment_recipient_pubkey: recipient_pubkey,
        payment_recipient_bolt12_offer: offer.to_string(),
        payment_rail: first_tag_value(event, "payment_rail")
            .unwrap_or("lexe-bolt12")
            .to_string(),
    })
}
