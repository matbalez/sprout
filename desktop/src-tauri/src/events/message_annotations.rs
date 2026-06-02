use nostr::Tag;

use super::{check_pubkey, tag};

fn is_positive_integer(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) && value != "0"
}

fn is_hex_event_id(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_bounty_amount(amount: &str) -> Result<(), String> {
    if is_positive_integer(amount) {
        return Ok(());
    }

    Err("message bounty amount must be a positive integer".to_string())
}

pub(super) fn annotation_tags(
    annotation_tags: &[Vec<String>],
    tags: &mut Vec<Tag>,
) -> Result<(), String> {
    for annotation in annotation_tags {
        match annotation.as_slice() {
            [name, feature, version]
                if name == "sprout" && feature == "kudos" && version == "v1" =>
            {
                tags.push(tag(vec!["sprout", "kudos", "v1"])?);
            }
            [name, feature, version, amount_sats, recipient_pubkey]
                if name == "sprout" && feature == "message-bounty" && version == "v1" =>
            {
                validate_bounty_amount(amount_sats)?;
                check_pubkey(recipient_pubkey)?;
                tags.push(tag(vec![
                    "sprout",
                    "message-bounty",
                    "v1",
                    amount_sats.as_str(),
                    &recipient_pubkey.to_ascii_lowercase(),
                ])?);
            }
            [name, feature, version, bounty_message_id, response_message_id, amount_sats, recipient_pubkey]
                if name == "sprout" && feature == "message-bounty-paid" && version == "v1" =>
            {
                if !is_hex_event_id(bounty_message_id) {
                    return Err("message bounty paid tag has invalid bounty event id".to_string());
                }
                if !is_hex_event_id(response_message_id) {
                    return Err("message bounty paid tag has invalid response event id".to_string());
                }
                validate_bounty_amount(amount_sats)?;
                check_pubkey(recipient_pubkey)?;
                tags.push(tag(vec![
                    "sprout",
                    "message-bounty-paid",
                    "v1",
                    bounty_message_id.as_str(),
                    response_message_id.as_str(),
                    amount_sats.as_str(),
                    &recipient_pubkey.to_ascii_lowercase(),
                ])?);
            }
            _ => {
                return Err(format!("unsupported annotation tag: {annotation:?}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::{EventBuilder, Keys, Kind};

    #[test]
    fn accepts_sprout_message_annotations() {
        let mut tags = Vec::new();

        annotation_tags(
            &[
                vec!["sprout".into(), "kudos".into(), "v1".into()],
                vec![
                    "sprout".into(),
                    "message-bounty".into(),
                    "v1".into(),
                    "210".into(),
                    "a".repeat(64),
                ],
                vec![
                    "sprout".into(),
                    "message-bounty-paid".into(),
                    "v1".into(),
                    "b".repeat(64),
                    "c".repeat(64),
                    "210".into(),
                    "d".repeat(64),
                ],
            ],
            &mut tags,
        )
        .unwrap();

        let event = EventBuilder::new(Kind::Custom(9), "great work")
            .tags(tags)
            .sign_with_keys(&Keys::generate())
            .unwrap();
        let tags: Vec<Vec<String>> = event.tags.iter().map(|t| t.as_slice().to_vec()).collect();

        assert!(tags.contains(&vec![
            "sprout".to_string(),
            "kudos".to_string(),
            "v1".to_string(),
        ]));
        assert!(tags.contains(&vec![
            "sprout".to_string(),
            "message-bounty".to_string(),
            "v1".to_string(),
            "210".to_string(),
            "a".repeat(64),
        ]));
        assert!(tags.contains(&vec![
            "sprout".to_string(),
            "message-bounty-paid".to_string(),
            "v1".to_string(),
            "b".repeat(64),
            "c".repeat(64),
            "210".to_string(),
            "d".repeat(64),
        ]));

        let mut tags = Vec::new();
        let err = annotation_tags(&[vec!["e".into(), "forged".into()]], &mut tags).unwrap_err();
        assert!(err.contains("unsupported annotation tag"));
    }

    #[test]
    fn rejects_invalid_bounty_annotations() {
        let mut tags = Vec::new();
        let err = annotation_tags(
            &[vec![
                "sprout".into(),
                "message-bounty".into(),
                "v1".into(),
                "0".into(),
                "a".repeat(64),
            ]],
            &mut tags,
        )
        .unwrap_err();
        assert!(err.contains("positive integer"));

        let err = annotation_tags(
            &[vec![
                "sprout".into(),
                "message-bounty-paid".into(),
                "v1".into(),
                "not-an-event".into(),
                "c".repeat(64),
                "210".into(),
                "d".repeat(64),
            ]],
            &mut tags,
        )
        .unwrap_err();
        assert!(err.contains("invalid bounty event id"));
    }
}
