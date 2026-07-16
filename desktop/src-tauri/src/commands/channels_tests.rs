// Tests for commands/channels.rs — split into a sibling file to keep
// channels.rs under the per-file line cap.

use super::*;
use nostr::{EventBuilder, Keys, Kind, Tag, Timestamp};

/// Build a signed event for testing with the given kind, content, and tags.
fn ev(kind: u16, content: &str, tags: Vec<Vec<&str>>) -> nostr::Event {
    ev_at(kind, content, tags, Timestamp::now())
}

fn ev_at(kind: u16, content: &str, tags: Vec<Vec<&str>>, created_at: Timestamp) -> nostr::Event {
    let keys = Keys::generate();
    let parsed: Vec<Tag> = tags
        .into_iter()
        .map(|t| Tag::parse(t).expect("parse tag"))
        .collect();
    EventBuilder::new(Kind::from_u16(kind), content)
        .tags(parsed)
        .custom_created_at(created_at)
        .sign_with_keys(&keys)
        .expect("sign")
}

// A 64-hex pubkey (nostr p-tags require 32-byte hex).
const PK_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const PK_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PK_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn directory_cursor_keeps_same_second_tiebreaker() {
    let timestamp = Timestamp::from(1_700_000_000);
    let event = ev_at(39000, "{}", vec![], timestamp);
    let mut filter = serde_json::json!({"kinds": [39000], "limit": DIRECTORY_PAGE_SIZE});

    advance_directory_cursor(&mut filter, std::slice::from_ref(&event));

    assert_eq!(filter["until"], serde_json::json!(timestamp.as_secs()));
    assert_eq!(filter["before_id"], serde_json::json!(event.id.to_hex()));
}

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

#[test]
fn collects_hive_metadata_from_create_and_marker_events() {
    let create = ev(
        9007,
        "",
        vec![
            vec!["h", "chan-1"],
            vec!["hive_channel", "1"],
            vec!["hive_wallet_provider", "lexe"],
        ],
    );
    let marker = ev(
        7,
        "sprout-hive-wallet:v1:11111111-1111-4111-8111-111111111111",
        vec![
            vec!["h", "chan-1"],
            vec![
                "e",
                "1111111111111111111111111111111111111111111111111111111111111111",
                "",
                "root",
            ],
            vec!["hive_channel", "1"],
            vec!["hive_wallet_provider", "lexe"],
            vec!["hive_wallet_bolt12_offer", "lno1hive"],
        ],
    );

    let mut metadata = std::collections::HashMap::new();
    collect_hive_metadata(vec![create, marker], &mut metadata);

    assert_eq!(
        metadata
            .get("chan-1")
            .and_then(|(_, metadata)| metadata.hive_wallet_bolt12_offer.as_deref()),
        Some("lno1hive")
    );
}

#[test]
fn pending_overlay_marks_relay_signed_channel_as_member() {
    // The real production shape: kind:39000 is relay-signed (#1761), so the
    // event's author is never the creator. A fresh channel's owner is
    // classified via the pending-owner overlay (populated by `create_channel`
    // in this same process), not via the event's pubkey.
    let relay_keys = Keys::generate();
    let e = EventBuilder::new(Kind::from_u16(39000), "")
        .tags(vec![
            Tag::parse(["d", "chan-1"]).expect("parse tag"),
            Tag::parse(["name", "n"]).expect("parse tag"),
        ])
        .sign_with_keys(&relay_keys)
        .expect("sign");

    let state = crate::app_state::build_app_state();
    state.mark_pending_owned_channel(PK_A, "chan-1");

    let info = crate::nostr_convert::channel_info_from_event(
        &e,
        None,
        Some(classify_pending_owner(&state, PK_A, Some("chan-1"))),
    )
    .unwrap();
    assert!(info.is_member);
}

#[test]
fn pending_overlay_leaves_unrelated_channel_as_non_member() {
    // A relay-signed channel this identity never created (not in the
    // overlay) must stay `is_member=false` — no over-broad match.
    let relay_keys = Keys::generate();
    let e = EventBuilder::new(Kind::from_u16(39000), "")
        .tags(vec![
            Tag::parse(["d", "chan-1"]).expect("parse tag"),
            Tag::parse(["name", "n"]).expect("parse tag"),
        ])
        .sign_with_keys(&relay_keys)
        .expect("sign");

    let state = crate::app_state::build_app_state();
    // Overlay has a different channel pending for the same identity, not
    // this one.
    state.mark_pending_owned_channel(PK_A, "chan-other");

    let info = crate::nostr_convert::channel_info_from_event(
        &e,
        None,
        Some(classify_pending_owner(&state, PK_A, Some("chan-1"))),
    )
    .unwrap();
    assert!(!info.is_member);
}

#[test]
fn pending_overlay_cleared_once_real_membership_observed() {
    // Once the real kind:39002 lands (modeled here as `get_channels`'s
    // cleanup step: clearing every channel id it just found real membership
    // for), the overlay must stop speaking for that channel — otherwise a
    // later leave would never flip `is_member` back to false.
    let state = crate::app_state::build_app_state();
    state.mark_pending_owned_channel(PK_A, "chan-1");
    assert!(state.is_pending_owned_channel(PK_A, "chan-1"));

    // Mirrors the `for id in &channel_ids { state.clear_pending_owned_channel(&my_pubkey, id) }`
    // step in `get_channels` once "chan-1" appears in PK_A's real member set.
    state.clear_pending_owned_channel(PK_A, "chan-1");
    assert!(!state.is_pending_owned_channel(PK_A, "chan-1"));
}

#[test]
fn pending_overlay_does_not_leak_across_identity_swap() {
    // Regression for the IMPORTANT Thufir flagged on the bare-channel-id
    // overlay: `import_identity`/workspace-apply can replace `state.keys` in
    // process without clearing the overlay. Identity A creates a channel and
    // is recorded pending-owner; if the process then switches to identity B
    // (same `AppState`, same channel id), B must NOT inherit A's entry.
    let state = crate::app_state::build_app_state();
    state.mark_pending_owned_channel(PK_A, "chan-1");

    assert!(state.is_pending_owned_channel(PK_A, "chan-1"));
    assert!(!state.is_pending_owned_channel(PK_B, "chan-1"));
}

#[test]
fn classify_pending_owner_matches_only_the_owning_identity() {
    // Exercises the exact branch-level decision `get_channels`'s open-channel
    // fallthrough makes, not just the underlying `AppState` helpers in
    // isolation.
    let state = crate::app_state::build_app_state();
    state.mark_pending_owned_channel(PK_A, "chan-1");

    assert!(classify_pending_owner(&state, PK_A, Some("chan-1")));
    // Different identity, same channel id: must not match.
    assert!(!classify_pending_owner(&state, PK_B, Some("chan-1")));
    // Same identity, different channel id: must not match.
    assert!(!classify_pending_owner(&state, PK_A, Some("chan-other")));
    // No `d` tag on the event at all: must not match.
    assert!(!classify_pending_owner(&state, PK_A, None));
}

#[test]
fn pending_owner_mark_uses_signer_captured_before_identity_swap() {
    // Regression for the write-side IMPORTANT Thufir flagged in pass 3:
    // `create_channel` used to re-read `state.keys` *after* the submit
    // await, so an identity swap that lands during the in-flight request
    // could mark the overlay under the new identity instead of the one that
    // actually signed the create. The fix captures the signer up front and
    // marks with that captured identity, so a swap that happens afterward
    // (i.e. during what would be the submit await) can't retarget the mark.
    let state = crate::app_state::build_app_state();

    // Mirrors `create_channel`'s new capture-before-submit step: read the
    // signer identity once, before anything that could race with a swap.
    let creator_keys = state.signing_keys().expect("signable");
    let creator_pubkey = creator_keys.public_key().to_hex();

    // Simulate an in-process identity swap landing during the (here,
    // implicit) submit await — e.g. `import_identity` replacing
    // `state.keys` while the create request is in flight.
    *state.keys.lock().expect("lock keys") = Keys::generate();

    // The mark must use the captured signer, not whatever `state.keys`
    // holds now.
    state.mark_pending_owned_channel(&creator_pubkey, "chan-1");

    assert!(state.is_pending_owned_channel(&creator_pubkey, "chan-1"));
    let post_swap_pubkey = state.keys.lock().expect("lock keys").public_key().to_hex();
    assert!(!state.is_pending_owned_channel(&post_swap_pubkey, "chan-1"));
}
