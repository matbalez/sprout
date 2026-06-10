import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildMessageBountyPaidTag,
  buildMessageBountyTag,
  formatBountyConfirmation,
  getDecayedMessageBountyAmount,
  getDisplayMessageBountyAmount,
  getMessageBountyResidualAmount,
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

  it("linearly decays bounty amounts every full five minutes", () => {
    assert.equal(
      getDecayedMessageBountyAmount({
        createdAt: 1_000,
        initialAmountSats: 1_000,
        now: 1_000 + 4 * 60 + 59,
      }),
      1_000,
    );
    assert.equal(
      getDecayedMessageBountyAmount({
        createdAt: 1_000,
        initialAmountSats: 1_000,
        now: 1_000 + 12 * 60,
      }),
      900,
    );
  });

  it("keeps decayed bounties at the residual floor", () => {
    assert.equal(getMessageBountyResidualAmount(1_000), 250);
    assert.equal(
      getDecayedMessageBountyAmount({
        createdAt: 1_000,
        initialAmountSats: 1_000,
        now: 1_000 + 4 * 60 * 60,
      }),
      250,
    );
    assert.equal(getMessageBountyResidualAmount(1), 1);
  });

  it("prefers locked bounty amounts for display", () => {
    assert.equal(
      getDisplayMessageBountyAmount(
        {
          amountSats: 1_000,
          createdAt: 1_000,
          initialAmountSats: 1_000,
          lockedAmountSats: 900,
        },
        1_000 + 4 * 60 * 60,
      ),
      900,
    );
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
