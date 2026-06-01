import assert from "node:assert/strict";
import test from "node:test";

import {
  classifyKlaimPayoutResult,
  klaimPayoutErrorLabel,
  shouldPostKlaimGiftConfirmation,
} from "./klaimGiftResults.ts";

function payoutResult(overrides = {}) {
  return {
    ok: false,
    statusCode: 409,
    status: null,
    amountSats: null,
    destinationKind: null,
    claimsUsed: null,
    maxClaims: null,
    error: "this nostr pubkey has already been paid on this channel",
    detail: null,
    ...overrides,
  };
}

test("classifies successful Klaim payouts as paid", () => {
  const result = payoutResult({
    ok: true,
    statusCode: 200,
    status: "paid",
    amountSats: 2100,
  });

  assert.equal(classifyKlaimPayoutResult(result), "paid");
  assert.equal(shouldPostKlaimGiftConfirmation(result), true);
});

test("does not retry already-paid or max-claims conflicts", () => {
  assert.equal(classifyKlaimPayoutResult(payoutResult()), "already-paid");
  assert.equal(
    classifyKlaimPayoutResult(
      payoutResult({ error: "channel has reached its max claims" }),
    ),
    "max-claims",
  );
});

test("parks in-progress and uncertain responses", () => {
  assert.equal(
    classifyKlaimPayoutResult(
      payoutResult({
        error:
          "a payout for this nostr pubkey is already in progress or pending operator reconciliation",
      }),
    ),
    "in-progress",
  );
  assert.equal(
    classifyKlaimPayoutResult(
      payoutResult({
        statusCode: 500,
        error: "payment status uncertain - do not retry",
      }),
    ),
    "uncertain",
  );
});

test("retries clean payment failures and wallet outages", () => {
  assert.equal(
    classifyKlaimPayoutResult(
      payoutResult({ statusCode: 422, error: "could not pay destination" }),
    ),
    "retryable",
  );
  assert.equal(
    classifyKlaimPayoutResult(
      payoutResult({
        statusCode: 503,
        error: "wallet temporarily unavailable, try again shortly",
      }),
    ),
    "retryable",
  );
});

test("combines Klaim error detail for admin-facing labels", () => {
  assert.equal(
    klaimPayoutErrorLabel(
      payoutResult({
        error: "could not pay destination",
        detail: "offer is expired",
      }),
    ),
    "could not pay destination: offer is expired",
  );
});
