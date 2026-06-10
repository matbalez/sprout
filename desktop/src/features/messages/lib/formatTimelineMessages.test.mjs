import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

import { formatTimelineMessages } from "./formatTimelineMessages.ts";
import { buildMessageBountyTag } from "./messageBounties.ts";

const CHANNEL_ID = "1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb";
const BOUNTY_ID =
  "1111111111111111111111111111111111111111111111111111111111111111";
const FIRST_RESPONSE_ID =
  "2222222222222222222222222222222222222222222222222222222222222222";
const LATER_RESPONSE_ID =
  "3333333333333333333333333333333333333333333333333333333333333333";
const SENDER =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECIPIENT =
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

describe("formatTimelineMessages bounty decay", () => {
  it("locks the decayed bounty amount on the first target response", () => {
    const bounty = event({
      id: BOUNTY_ID,
      pubkey: SENDER,
      created_at: 1_000,
      content: "please reply",
      tags: [
        ["h", CHANNEL_ID],
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
        ["h", CHANNEL_ID],
        ["e", BOUNTY_ID, "", "reply"],
      ],
    });
    const laterResponse = event({
      id: LATER_RESPONSE_ID,
      pubkey: RECIPIENT,
      created_at: 1_000 + 20 * 60,
      content: "another reply",
      tags: [
        ["h", CHANNEL_ID],
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
    assert.equal(
      messages[0]?.bounty?.lockedResponseMessageId,
      FIRST_RESPONSE_ID,
    );
    assert.equal(messages[1]?.bountyPayment?.amountSats, 900);
    assert.equal(
      messages[1]?.bountyPayment?.responseMessageId,
      FIRST_RESPONSE_ID,
    );
    assert.equal(messages[2]?.bountyPayment, undefined);
  });
});
