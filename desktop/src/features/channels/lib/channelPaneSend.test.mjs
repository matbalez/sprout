import assert from "node:assert/strict";
import test from "node:test";

import { sendChannelPaneMessage } from "./channelPaneSend.ts";

test("channel pane send forwards kudos options to the message sender", async () => {
  const received = {};

  await sendChannelPaneMessage({
    completeWelcomeComposerBanner: () => {
      throw new Error("welcome banner should not complete");
    },
    content: "@DK nice work",
    mediaTags: [["imeta", "url https://example.test/image.png"]],
    mentionPubkeys: ["d".repeat(64)],
    onSendMessage: async (content, mentionPubkeys, mediaTags, options) => {
      received.content = content;
      received.mentionPubkeys = mentionPubkeys;
      received.mediaTags = mediaTags;
      received.options = options;
    },
    options: { bountyAmountSats: null, kudos: true },
    shouldCompleteWelcomeBanner: false,
  });

  assert.equal(received.content, "@DK nice work");
  assert.deepEqual(received.mentionPubkeys, ["d".repeat(64)]);
  assert.deepEqual(received.mediaTags, [
    ["imeta", "url https://example.test/image.png"],
  ]);
  assert.deepEqual(received.options, { bountyAmountSats: null, kudos: true });
});
