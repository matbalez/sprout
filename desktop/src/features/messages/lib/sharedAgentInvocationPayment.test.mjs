import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  buildSharedAgentInvocationPaymentTarget,
  collectSharedAgentInvocationPaymentTargets,
  formatSharedAgentInvocationPaymentMessage,
  SHARED_AGENT_INVOCATION_AMOUNT_SATS,
} from "@/features/messages/lib/sharedAgentInvocationPayment";

const INVOKER =
  "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OWNER =
  "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const AGENT =
  "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const LOCAL_AGENT =
  "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

const identity = {
  pubkey: INVOKER,
  displayName: "Mat",
};

describe("shared agent invocation payments", () => {
  it("collects one payment for a mentioned relay agent owned by someone else", () => {
    const targets = collectSharedAgentInvocationPaymentTargets({
      currentIdentity: identity,
      currentProfile: null,
      managedAgents: [],
      mentionPubkeys: [AGENT, AGENT.toUpperCase()],
      profiles: {
        [OWNER]: {
          displayName: "Owner",
          avatarUrl: null,
          nip05Handle: null,
        },
      },
      relayAgents: [
        {
          pubkey: AGENT,
          name: "SharedAgent",
          agentType: "codex",
          ownerPubkey: OWNER,
          channels: ["general"],
          channelIds: [],
          capabilities: [],
          status: "online",
        },
      ],
    });

    assert.deepEqual(targets, [
      {
        agentPubkey: AGENT,
        agentName: "SharedAgent",
        ownerPubkey: OWNER,
        ownerLabel: "Owner",
        invokerLabel: "Mat",
        amountSats: SHARED_AGENT_INVOCATION_AMOUNT_SATS,
      },
    ]);
  });

  it("does not charge for unowned, self-owned, unmentioned, or local agents", () => {
    const targets = collectSharedAgentInvocationPaymentTargets({
      currentIdentity: identity,
      currentProfile: null,
      managedAgents: [
        {
          pubkey: LOCAL_AGENT,
          name: "LocalAgent",
          status: "deployed",
        },
      ],
      mentionPubkeys: [AGENT, LOCAL_AGENT],
      relayAgents: [
        {
          pubkey: AGENT,
          name: "SelfOwnedAgent",
          agentType: "codex",
          ownerPubkey: INVOKER,
          channels: [],
          channelIds: [],
          capabilities: [],
          status: "online",
        },
        {
          pubkey: LOCAL_AGENT,
          name: "LocalAgent",
          agentType: "codex",
          ownerPubkey: OWNER,
          channels: [],
          channelIds: [],
          capabilities: [],
          status: "online",
        },
        {
          pubkey:
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
          name: "UnmentionedAgent",
          agentType: "codex",
          ownerPubkey: OWNER,
          channels: [],
          channelIds: [],
          capabilities: [],
          status: "online",
        },
        {
          pubkey:
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
          name: "UnownedAgent",
          agentType: "codex",
          ownerPubkey: null,
          channels: [],
          channelIds: [],
          capabilities: [],
          status: "online",
        },
      ],
    });

    assert.deepEqual(targets, []);
  });

  it("formats the channel receipt", () => {
    assert.equal(
      formatSharedAgentInvocationPaymentMessage({
        ownerLabel: "Owner",
        amountSats: 50,
        agentName: "SharedAgent",
      }),
      "➡️ paid Owner ₿50 to invoke SharedAgent",
    );
  });

  it("builds a payment target from a verified owner fallback", () => {
    const target = buildSharedAgentInvocationPaymentTarget({
      agentPubkey: AGENT.toUpperCase(),
      currentIdentity: identity,
      currentProfile: null,
      ownerPubkey: OWNER.toUpperCase(),
      profiles: {
        [AGENT]: {
          displayName: "ChatLunatique",
          avatarUrl: null,
          nip05Handle: null,
        },
        [OWNER]: {
          displayName: "DK",
          avatarUrl: null,
          nip05Handle: null,
        },
      },
    });

    assert.deepEqual(target, {
      agentPubkey: AGENT,
      agentName: "ChatLunatique",
      ownerPubkey: OWNER,
      ownerLabel: "DK",
      invokerLabel: "Mat",
      amountSats: SHARED_AGENT_INVOCATION_AMOUNT_SATS,
    });
  });
});
