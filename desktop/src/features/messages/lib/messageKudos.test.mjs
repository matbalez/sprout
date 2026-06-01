import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildKudosMessageTag,
  isKudosMessageTags,
  resolveKudosTargetPubkey,
} from "@/features/messages/lib/messageKudos";

const ALICE =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BOB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

describe("message kudos", () => {
  it("identifies Sprout kudos annotation tags", () => {
    assert.equal(isKudosMessageTags([buildKudosMessageTag()]), true);
    assert.equal(isKudosMessageTags([["kudos", "legacy"]]), false);
  });

  it("requires exactly one non-self mention target", () => {
    assert.equal(resolveKudosTargetPubkey([ALICE], BOB), ALICE);
    assert.throws(() => resolveKudosTargetPubkey([], BOB), /requires one/);
    assert.throws(() => resolveKudosTargetPubkey([BOB], BOB), /requires one/);
    assert.throws(
      () => resolveKudosTargetPubkey([ALICE, BOB]),
      /only be sent to one/,
    );
  });
});
