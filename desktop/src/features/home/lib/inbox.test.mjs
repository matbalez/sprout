import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { buildInboxItems } from "./inbox.ts";
import { buildMessageBountyTag } from "@/features/messages/lib/messageBounties.ts";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

const CHANNEL_ID = "1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb";
const BOUNTY_ID =
  "1111111111111111111111111111111111111111111111111111111111111111";
const RESPONSE_ID =
  "2222222222222222222222222222222222222222222222222222222222222222";
const SENDER =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECIPIENT =
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

function feedItem(overrides) {
  return {
    id: overrides.id,
    kind: overrides.kind ?? KIND_STREAM_MESSAGE,
    pubkey: overrides.pubkey,
    content: overrides.content ?? "",
    createdAt: overrides.createdAt,
    channelId: CHANNEL_ID,
    channelName: "paid-chat",
    tags: overrides.tags ?? [],
    category: overrides.category ?? "activity",
  };
}

function feedResponse(items) {
  return {
    feed: {
      mentions: items.filter((item) => item.category === "mention"),
      needsAction: items.filter((item) => item.category === "needs_action"),
      activity: items.filter((item) => item.category === "activity"),
      agentActivity: items.filter(
        (item) => item.category === "agent_activity",
      ),
    },
    meta: {
      generatedAt: 0,
      since: 0,
      total: items.length,
    },
  };
}

describe("buildInboxItems bounty summaries", () => {
  it("locks the list bounty amount from the first target response", () => {
    const realDateNow = Date.now;
    Date.now = () => (1_000 + 20 * 60) * 1_000;

    try {
      const bounty = feedItem({
        id: BOUNTY_ID,
        pubkey: SENDER,
        createdAt: 1_000,
        content: "please reply",
        category: "mention",
        tags: [
          ["h", CHANNEL_ID],
          buildMessageBountyTag({
            amountSats: 1_500,
            recipientPubkey: RECIPIENT,
          }),
        ],
      });
      const firstResponse = feedItem({
        id: RESPONSE_ID,
        pubkey: RECIPIENT,
        createdAt: 1_000 + 15 * 60,
        content: "replying now",
        tags: [
          ["h", CHANNEL_ID],
          ["e", BOUNTY_ID, "", "reply"],
        ],
      });

      const items = buildInboxItems({
        currentPubkey: RECIPIENT,
        feed: feedResponse([bounty, firstResponse]),
        profiles: {},
      });

      assert.equal(items.length, 1);
      assert.equal(items[0]?.bounty?.amountSats, 1_275);
      assert.equal(items[0]?.bounty?.lockedAmountSats, 1_275);
      assert.equal(items[0]?.bounty?.lockedResponseMessageId, RESPONSE_ID);
    } finally {
      Date.now = realDateNow;
    }
  });
});
