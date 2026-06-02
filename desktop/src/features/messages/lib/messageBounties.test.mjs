import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildMessageBountyPaidTag,
  buildMessageBountyTag,
  formatBountyConfirmation,
  parseBountyAmountInput,
  parseMessageBountyPaidTags,
  parseMessageBountyTags,
  resolveBountyTargetPubkey,
} from "./messageBounties.ts";

const ALICE =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BOB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const BOUNTY_ID =
  "1111111111111111111111111111111111111111111111111111111111111111";
const RESPONSE_ID =
  "2222222222222222222222222222222222222222222222222222222222222222";

describe("message bounties", () => {
  it("requires exactly one bounty target mention while allowing self targets", () => {
    assert.equal(resolveBountyTargetPubkey([ALICE]), ALICE);
    assert.equal(
      resolveBountyTargetPubkey([ALICE, ALICE.toUpperCase()]),
      ALICE,
    );
    assert.throws(() => resolveBountyTargetPubkey([]), /exactly one/);
    assert.throws(() => resolveBountyTargetPubkey([ALICE, BOB]), /exactly one/);
  });

  it("builds and parses bounty tags", () => {
    const tag = buildMessageBountyTag({
      amountSats: 200,
      recipientPubkey: ALICE.toUpperCase(),
    });

    assert.deepEqual(parseMessageBountyTags([tag]), {
      amountSats: 200,
      recipientPubkey: ALICE,
    });
  });

  it("parses composer bounty amount input", () => {
    assert.equal(parseBountyAmountInput("₿2,100"), 2100);
    assert.equal(parseBountyAmountInput("  2100  "), 2100);
    assert.throws(() => parseBountyAmountInput("0"), /whole ₿ amount/);
    assert.throws(() => parseBountyAmountInput("1.5"), /whole ₿ amount/);
  });

  it("builds and parses bounty paid tags", () => {
    const tag = buildMessageBountyPaidTag({
      amountSats: 200,
      bountyMessageId: BOUNTY_ID,
      recipientPubkey: ALICE,
      responseMessageId: RESPONSE_ID,
    });

    assert.deepEqual(parseMessageBountyPaidTags([tag]), {
      amountSats: 200,
      bountyMessageId: BOUNTY_ID,
      recipientPubkey: ALICE,
      responseMessageId: RESPONSE_ID,
    });
  });

  it("formats bounty payment confirmations", () => {
    assert.equal(
      formatBountyConfirmation({
        amountSats: 200,
        recipientLabel: "@Bob\nBuilder",
      }),
      "➡️ paid @Bob Builder ₿200 bounty",
    );
  });
});
