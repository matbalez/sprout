import assert from "node:assert/strict";
import test from "node:test";

import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

import { formatTimelineMessages } from "./formatTimelineMessages.ts";
import { buildMessageBountyTag } from "./messageBounties.ts";

const HEX64_A =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HEX64_B =
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PUBKEY_A =
  "1111111111111111111111111111111111111111111111111111111111111111";
const PUBKEY_B =
  "2222222222222222222222222222222222222222222222222222222222222222";
const CHANNEL_ID = "36411e44-0e2d-4cfe-bd6e-567eb169db9f";
const BOUNTY_CHANNEL_ID = "1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb";
const BOUNTY_ID =
  "3333333333333333333333333333333333333333333333333333333333333333";
const FIRST_RESPONSE_ID =
  "4444444444444444444444444444444444444444444444444444444444444444";
const LATER_RESPONSE_ID =
  "5555555555555555555555555555555555555555555555555555555555555555";
const SENDER =
  "6666666666666666666666666666666666666666666666666666666666666666";
const RECIPIENT =
  "7777777777777777777777777777777777777777777777777777777777777777";

function streamMessage(overrides = {}) {
  return {
    id: HEX64_A,
    pubkey: PUBKEY_A,
    kind: 9,
    created_at: 1_700_000_000,
    content: "hello world",
    tags: [["h", CHANNEL_ID]],
    sig: "sig",
    ...overrides,
  };
}

function deletionEvent(kind, targetId, overrides = {}) {
  return {
    id: HEX64_B,
    pubkey: PUBKEY_B,
    kind,
    created_at: 1_700_000_001,
    content: "",
    tags: [
      ["h", CHANNEL_ID],
      ["e", targetId],
    ],
    sig: "sig",
    ...overrides,
  };
}

function event(overrides) {
  return {
    id: overrides.id,
    pubkey: overrides.pubkey,
    created_at: overrides.created_at,
    kind: overrides.kind ?? KIND_STREAM_MESSAGE,
    tags: overrides.tags ?? [],
    content: overrides.content ?? "",
    sig: "",
  };
}

test("kind:5 (NIP-09) deletion hides the target message", () => {
  const events = [streamMessage(), deletionEvent(5, HEX64_A)];
  const out = formatTimelineMessages(events, null, undefined, null);
  assert.equal(
    out.length,
    0,
    "the kind:9 message should be filtered out by the kind:5 deletion",
  );
});

test("kind:9005 (NIP-29 / Buzz-native) deletion hides the target message", () => {
  const events = [streamMessage(), deletionEvent(9005, HEX64_A)];
  const out = formatTimelineMessages(events, null, undefined, null);
  assert.equal(
    out.length,
    0,
    "the kind:9 message should be filtered out by the kind:9005 deletion",
  );
});

test("non-deletion event kinds do NOT hide the target message", () => {
  const reaction = {
    id: HEX64_B,
    pubkey: PUBKEY_B,
    kind: 7,
    created_at: 1_700_000_001,
    content: "+",
    tags: [
      ["h", CHANNEL_ID],
      ["e", HEX64_A],
    ],
    sig: "sig",
  };
  const events = [streamMessage(), reaction];
  const out = formatTimelineMessages(events, null, undefined, null);
  assert.equal(out.length, 1, "the kind:9 message should still be visible");
});

test("deletion target with non-hex e tag value is ignored", () => {
  const bogusDeletion = deletionEvent(9005, HEX64_A, {
    tags: [
      ["h", CHANNEL_ID],
      ["e", "not-hex"],
    ],
  });
  const events = [streamMessage(), bogusDeletion];
  const out = formatTimelineMessages(events, null, undefined, null);
  assert.equal(
    out.length,
    1,
    "malformed deletion tag should not match anything",
  );
});

test("locks the decayed bounty amount on the first target response", () => {
  const bounty = event({
    id: BOUNTY_ID,
    pubkey: SENDER,
    created_at: 1_000,
    content: "please reply",
    tags: [
      ["h", BOUNTY_CHANNEL_ID],
      buildMessageBountyTag({
        amountSats: 1_000,
        recipientPubkey: RECIPIENT,
      }),
    ],
  });
  const firstResponse = event({
    id: FIRST_RESPONSE_ID,
    pubkey: RECIPIENT,
    created_at: 1_000 + 12 * 60,
    content: "replying now",
    tags: [
      ["h", BOUNTY_CHANNEL_ID],
      ["e", BOUNTY_ID, "", "reply"],
    ],
  });
  const laterResponse = event({
    id: LATER_RESPONSE_ID,
    pubkey: RECIPIENT,
    created_at: 1_000 + 20 * 60,
    content: "another reply",
    tags: [
      ["h", BOUNTY_CHANNEL_ID],
      ["e", BOUNTY_ID, "", "reply"],
    ],
  });

  const messages = formatTimelineMessages(
    [bounty, firstResponse, laterResponse],
    null,
    SENDER,
    null,
  );

  assert.equal(messages[0]?.bounty?.amountSats, 900);
  assert.equal(messages[0]?.bounty?.lockedAmountSats, 900);
  assert.equal(messages[0]?.bounty?.lockedResponseMessageId, FIRST_RESPONSE_ID);
  assert.equal(messages[1]?.bountyPayment?.amountSats, 900);
  assert.equal(messages[1]?.bountyPayment?.responseMessageId, FIRST_RESPONSE_ID);
  assert.equal(messages[2]?.bountyPayment, undefined);
});
