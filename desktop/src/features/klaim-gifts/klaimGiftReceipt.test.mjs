import assert from "node:assert/strict";
import test from "node:test";

import {
  formatKlaimGiftConfirmation,
  normalizeKlaimMentionName,
} from "./klaimGiftReceipt.ts";

test("formats Klaim gift receipts with a username mention only", () => {
  assert.equal(
    formatKlaimGiftConfirmation({
      amountSats: 200,
      mentionName: "bob",
    }),
    "₿200 gifted to @bob",
  );
});

test("normalizes display names for receipt mentions", () => {
  assert.equal(normalizeKlaimMentionName("@Bob\nBuilder"), "Bob Builder");
});
