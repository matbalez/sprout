import assert from "node:assert/strict";
import test from "node:test";

import { buildKudosMessageTag } from "@/features/messages/lib/messageKudos";
import { shouldUseValidatedMessageSendPath } from "@/features/messages/lib/messageSendPath";

test("plain kudos messages use the validated Tauri send path", () => {
  assert.equal(
    shouldUseValidatedMessageSendPath({
      annotationTags: [buildKudosMessageTag()],
    }),
    true,
  );
});

test("plain unannotated messages can use the websocket send path", () => {
  assert.equal(shouldUseValidatedMessageSendPath({}), false);
});

test("non-kudos structured tags use the validated Tauri send path", () => {
  assert.equal(
    shouldUseValidatedMessageSendPath({
      emojiTags: [["emoji", "party", "https://example.test/party.png"]],
    }),
    true,
  );
  assert.equal(
    shouldUseValidatedMessageSendPath({
      mentionTags: [["mention", "a".repeat(64)]],
    }),
    true,
  );
});
