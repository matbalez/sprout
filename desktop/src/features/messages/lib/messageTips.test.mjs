import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildTipsByEventId,
  isMessageTipReceiptEvent,
} from "@/features/messages/lib/messageTips";
import { KIND_REACTION, KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

const CHANNEL_ID = "1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb";
const TARGET_ID =
  "dc91b7ef91fa438a4c8d8904c55113d65e11db006bff3c045568a965514ceedd";
const RECEIPT_ID =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECIPIENT =
  "5630c46628625e3d69756723ca537f20e3a5a035266015b079f406d21df7e44e";
const TIPPER =
  "874ebdf65e1bf366574fc7c1be936ab7538ac2a5daead9405f3338c8109bc7e0";
const TIP_ID = "00000000000040008000000000000000";

function event(overrides) {
  return {
    id: overrides.id,
    pubkey: overrides.pubkey,
    created_at: overrides.created_at ?? 1_764_000_000,
    kind: overrides.kind,
    tags: overrides.tags ?? [],
    content: overrides.content ?? "",
    sig: "",
  };
}

function targetEvent() {
  return event({
    id: TARGET_ID,
    pubkey: RECIPIENT,
    kind: KIND_STREAM_MESSAGE,
    content: "message worth tipping",
    tags: [["h", CHANNEL_ID]],
  });
}

function tipReceiptEvent() {
  return event({
    id: RECEIPT_ID,
    pubkey: TIPPER,
    kind: KIND_REACTION,
    content: `sprout-tip:${TIP_ID}`,
    tags: [
      ["h", CHANNEL_ID],
      ["e", TARGET_ID, "", "root"],
      ["p", RECIPIENT],
      ["amount", "10"],
      ["unit", "sprout-bitcoin-base-unit"],
      ["tip_id", TIP_ID],
      ["wallet", "lexe-bolt12"],
      ["status", "sender-confirmed"],
    ],
  });
}

describe("message tip receipts", () => {
  it("reads relay-compatible kind 7 tip receipts", () => {
    const target = targetEvent();
    const receipt = tipReceiptEvent();
    const eventsById = new Map([[target.id, target]]);

    assert.equal(isMessageTipReceiptEvent(receipt), true);

    const tips = buildTipsByEventId({
      currentPubkeyLower: TIPPER,
      deletedEventIds: new Set(),
      events: [target, receipt],
      eventsById,
    });

    const summary = tips.get(TARGET_ID);
    assert.equal(summary?.amountSats, 10);
    assert.equal(summary?.count, 1);
    assert.equal(summary?.tippedByCurrentUser, true);
    assert.equal(summary?.users[0]?.pubkey, TIPPER);
    assert.equal(summary?.users[0]?.amountSats, 10);
    assert.equal(summary?.users[0]?.tipId, TIP_ID);
  });

  it("does not classify ordinary reactions as tip receipts", () => {
    const reaction = event({
      id: RECEIPT_ID,
      pubkey: TIPPER,
      kind: KIND_REACTION,
      content: "+",
      tags: [["e", TARGET_ID]],
    });

    assert.equal(isMessageTipReceiptEvent(reaction), false);
  });
});
