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

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind, Tag};

    fn ev(kind: u16, content: &str, tags: Vec<Vec<&str>>) -> Event {
        let keys = Keys::generate();
        let parsed: Vec<Tag> = tags
            .into_iter()
            .map(|tag| Tag::parse(tag).expect("parse tag"))
            .collect();
        EventBuilder::new(Kind::from_u16(kind), content)
            .tags(parsed)
            .sign_with_keys(&keys)
            .expect("sign")
    }

    #[test]
    fn parses_paid_create_event() {
        let e = ev(
            9007,
            "",
            vec![
                vec!["h", "123e4567-e89b-12d3-a456-426614174000"],
                vec!["name", "paid"],
                vec!["visibility", "open"],
                vec!["channel_type", "stream"],
                vec!["paid_join", "25"],
                vec!["paid_post", "10"],
                vec!["payment_bolt12_offer", "lno1paidchanneloffer"],
                vec!["payment_rail", "lexe-bolt12"],
            ],
        );

        let policy = payment_policy_from_event(&e).expect("payment policy");
        assert!(policy.join_payment_required);
        assert_eq!(policy.join_amount_base_units, 25);
        assert!(policy.post_payment_required);
        assert_eq!(policy.post_amount_base_units, 10);
        assert_eq!(policy.payment_recipient_pubkey, e.pubkey.to_hex());
        assert_eq!(
            policy.payment_recipient_bolt12_offer,
            "lno1paidchanneloffer"
        );
        assert_eq!(policy.payment_rail, "lexe-bolt12");
    }
}
