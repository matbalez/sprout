import assert from "node:assert/strict";
import test from "node:test";

import { extractKlaimClaimCode } from "./processKlaimClaimMessage.ts";

test("extracts Klaim claim codes from channel messages", () => {
  assert.equal(extractKlaimClaimCode("klaim XYZ123"), "XYZ123");
  assert.equal(
    extractKlaimClaimCode("please klaim abc-123_now"),
    "abc-123_now",
  );
  assert.equal(extractKlaimClaimCode("KLAIM MixedCase42."), "MixedCase42");
});

test("ignores messages that do not contain a Klaim claim command", () => {
  assert.equal(extractKlaimClaimCode("klaim"), null);
  assert.equal(extractKlaimClaimCode("klaim.cash is neat"), null);
  assert.equal(extractKlaimClaimCode("acclaim XYZ123"), null);
});
