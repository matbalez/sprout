// Tests for commands/channels.rs — split into a sibling file to keep
// channels.rs under the per-file line cap.

use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag};

/// Build a signed event for testing with the given kind, content, and tags.
fn ev(kind: u16, content: &str, tags: Vec<Vec<&str>>) -> nostr::Event {
    let keys = Keys::generate();
    let parsed: Vec<Tag> = tags
        .into_iter()
        .map(|t| Tag::parse(t).expect("parse tag"))
        .collect();
    EventBuilder::new(Kind::from_u16(kind), content)
        .tags(parsed)
        .sign_with_keys(&keys)
        .expect("sign")
}

// A 64-hex pubkey (nostr p-tags require 32-byte hex).
const PK_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PK_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PK_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn counts_unique_p_tags_per_channel() {
    let e1 = ev(
        39002,
        "",
        vec![
            vec!["d", "chan-1"],
            vec!["p", PK_A, "", "member"],
            vec!["p", PK_B, "", "admin"],
        ],
    );
    let e2 = ev(
        39002,
        "",
        vec![vec!["d", "chan-2"], vec!["p", PK_C, "", "member"]],
    );

    let membership = collect_members_by_channel(&[e1, e2], PK_B);
    assert_eq!(membership.get("chan-1").map(|m| m.count), Some(2));
    assert_eq!(membership.get("chan-2").map(|m| m.count), Some(1));
    assert_eq!(membership.len(), 2);
    assert_eq!(
        membership
            .get("chan-1")
            .and_then(|m| m.current_user_role.as_deref()),
        Some("admin")
    );

    let mut pks: Vec<&str> = membership["chan-1"]
        .pubkeys
        .iter()
        .map(|s| s.as_str())
        .collect();
    pks.sort();
    assert_eq!(pks, vec![PK_A, PK_B]);
}

#[test]
fn dedupes_repeated_pubkeys() {
    let e = ev(
        39002,
        "",
        vec![
            vec!["d", "chan-1"],
            vec!["p", PK_A, "", "member"],
            vec!["p", PK_A, "", "admin"], // duplicate pubkey, different role
            vec!["p", PK_B, "", "member"],
        ],
    );
    let membership = collect_members_by_channel(&[e], PK_A);
    assert_eq!(membership.get("chan-1").map(|m| m.count), Some(2));
}

#[test]
fn skips_event_without_d_tag() {
    let e = ev(39002, "", vec![vec!["p", PK_A, "", "member"]]);
    let membership = collect_members_by_channel(&[e], PK_A);
    assert!(membership.is_empty());
}

#[test]
fn zero_member_channel_is_recorded() {
    // A channel with a members event but no p-tags should report 0,
    // not be absent from the map (the caller relies on `get` returning
    // `Some(0)` to overwrite a default).
    let e = ev(39002, "", vec![vec!["d", "chan-1"]]);
    let membership = collect_members_by_channel(&[e], PK_A);
    assert_eq!(membership.get("chan-1").map(|m| m.count), Some(0));
    assert!(membership["chan-1"].pubkeys.is_empty());
}

#[test]
fn empty_input_yields_empty_map() {
    let membership = collect_members_by_channel(&[], PK_A);
    assert!(membership.is_empty());
}

#[test]
fn payment_receipt_amount_filters_join_and_post_receipts() {
    let post = ev(
        7,
        "sprout-channel:post:nonce1",
        vec![
            vec!["h", "chan-1"],
            vec!["purpose", "post"],
            vec!["status", "sender-confirmed"],
            vec!["amount", "10"],
        ],
    );
    let join = ev(
        7,
        "sprout-channel:join:nonce2",
        vec![
            vec!["h", "chan-1"],
            vec!["purpose", "join"],
            vec!["status", "sender-confirmed"],
            vec!["amount", "25"],
        ],
    );

    assert_eq!(
        channel_payment_receipt_amount(&post, "chan-1", &["join", "post"]),
        Some(10)
    );
    assert_eq!(
        channel_payment_receipt_amount(&join, "chan-1", &["join", "post"]),
        Some(25)
    );
    assert_eq!(
        channel_payment_receipt_amount(&post, "chan-1", &["join"]),
        None
    );
}

#[test]
fn collects_payment_policies_by_metadata_and_channel_id() {
    let marker = ev(
        7,
        CHANNEL_PAYMENT_POLICY_CONTENT,
        vec![
            vec!["e", "metadata-event-id"],
            vec!["h", "chan-1"],
            vec!["paid_post", "10"],
            vec!["payment_bolt12_offer", "lno1marker"],
        ],
    );
    let create = ev(
        9007,
        "",
        vec![
            vec!["h", "chan-2"],
            vec!["paid_join", "25"],
            vec!["paid_post", "5"],
            vec!["payment_bolt12_offer", "lno1create"],
        ],
    );

    let mut policies = std::collections::HashMap::new();
    collect_payment_policies(vec![marker, create], &mut policies);

    assert_eq!(
        policies
            .get("metadata-event-id")
            .map(|(_, policy)| policy.post_amount_base_units),
        Some(10)
    );
    assert_eq!(
        policies
            .get("chan-1")
            .map(|(_, policy)| policy.post_amount_base_units),
        Some(10)
    );
    assert_eq!(
        policies
            .get("chan-2")
            .map(|(_, policy)| policy.join_amount_base_units),
        Some(25)
    );
}
