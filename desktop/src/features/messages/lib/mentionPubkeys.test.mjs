import assert from "node:assert/strict";
import test from "node:test";

import { extractMentionPubkeysFromText } from "./mentionPubkeys.ts";

const ALICE =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BOB = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const MIC = "b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6";
const MIC_NEALE =
  "c3dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6b2dc89f6";

test("extracts a selected autocomplete mention from text", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      mentionMap: new Map([["Mic Neale", MIC_NEALE]]),
      text: "good job finding your way back @Mic Neale",
    }),
    [MIC_NEALE],
  );
});

test("selected autocomplete mention wins over same display-name member scan", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [{ displayName: "Mic Neale", pubkey: BOB }],
      mentionMap: new Map([["Mic Neale", MIC_NEALE]]),
      text: "good job finding your way back @Mic Neale",
    }),
    [MIC_NEALE],
  );
});

test("selected longer display name wins over a shorter prefix member", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [{ displayName: "Mic", pubkey: MIC }],
      mentionMap: new Map([["Mic Neale", MIC_NEALE]]),
      text: "good job finding your way back @Mic Neale",
    }),
    [MIC_NEALE],
  );
});

test("raw member scan prefers a longer display name over a shorter prefix", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [
        { displayName: "Mic", pubkey: MIC },
        { displayName: "Mic Neale", pubkey: MIC_NEALE },
      ],
      mentionMap: new Map(),
      text: "good job finding your way back @Mic Neale",
    }),
    [MIC_NEALE],
  );
});

test("raw member scan resolves duplicate display names once", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [
        { displayName: "Mic Neale", pubkey: MIC },
        { displayName: "Mic Neale", pubkey: BOB },
      ],
      mentionMap: new Map(),
      text: "good job finding your way back @Mic Neale",
    }),
    [MIC],
  );
});

test("still extracts a distinct raw member mention alongside a selected mention", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [
        { displayName: "Alice", pubkey: ALICE },
        { displayName: "Mic Neale", pubkey: BOB },
      ],
      mentionMap: new Map([["Mic Neale", MIC_NEALE]]),
      text: "thanks @Mic Neale and @Alice",
    }),
    [MIC_NEALE, ALICE],
  );
});

test("still extracts both prefix names when both are visibly mentioned", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [
        { displayName: "Mic", pubkey: MIC },
        { displayName: "Mic Neale", pubkey: MIC_NEALE },
      ],
      mentionMap: new Map(),
      text: "thanks @Mic and @Mic Neale",
    }),
    [MIC, MIC_NEALE],
  );
});

test("extracts raw member mentions when no autocomplete selection exists", () => {
  assert.deepEqual(
    extractMentionPubkeysFromText({
      members: [{ displayName: "Alice", pubkey: ALICE }],
      mentionMap: new Map(),
      text: "thanks @Alice",
    }),
    [ALICE],
  );
});
