import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  shouldHydrateChannelForJoinPayment,
  sortChannels,
  sortChannelsWithLocalChannels,
} from "./hooks.ts";
import { WALLETBOT_CHANNEL_ID } from "@/features/wallet/api";

const paidJoinPolicy = {
  joinPaymentRequired: true,
  joinAmountBaseUnits: 1234,
  postPaymentRequired: false,
  postAmountBaseUnits: 0,
  paymentRecipientPubkey:
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  paymentRecipientBolt12Offer: "lno1mockpaidjoin",
  paymentRail: "lexe-bolt12",
};

const paidPostOnlyPolicy = {
  ...paidJoinPolicy,
  joinPaymentRequired: false,
  joinAmountBaseUnits: 0,
  postPaymentRequired: true,
  postAmountBaseUnits: 100,
};

function channel(overrides) {
  return {
    metadataEventId: "metadata-1",
    id: "channel-1",
    name: "pay-conor",
    channelType: "stream",
    visibility: "open",
    description: "",
    topic: null,
    purpose: null,
    memberCount: 0,
    memberPubkeys: [],
    lastMessageAt: null,
    archivedAt: null,
    participants: [],
    participantPubkeys: [],
    isMember: false,
    currentUserRole: null,
    ttlSeconds: null,
    ttlDeadline: null,
    paymentPolicy: null,
    hiveChannel: false,
    hiveWalletBolt12Offer: null,
    ...overrides,
  };
}

describe("sortChannels", () => {
  it("includes local WalletBot when caching relay channel rows", () => {
    const sorted = sortChannelsWithLocalChannels([
      channel({ name: "general" }),
    ]);

    assert.equal(
      sorted.some((item) => item.id === WALLETBOT_CHANNEL_ID),
      true,
    );
  });

  it("keeps paid policy when deduping stale channel rows", () => {
    const sorted = sortChannels([
      channel({ paymentPolicy: paidJoinPolicy }),
      channel({ metadataEventId: "metadata-2" }),
    ]);

    assert.equal(sorted.length, 1);
    assert.equal(sorted[0]?.metadataEventId, "metadata-2");
    assert.equal(sorted[0]?.paymentPolicy?.joinAmountBaseUnits, 1234);
  });

  it("keeps hive metadata when deduping stale channel rows", () => {
    const sorted = sortChannels([
      channel({
        hiveChannel: true,
        hiveWalletBolt12Offer: "lno1hive",
      }),
      channel({ metadataEventId: "metadata-2" }),
    ]);

    assert.equal(sorted.length, 1);
    assert.equal(sorted[0]?.metadataEventId, "metadata-2");
    assert.equal(sorted[0]?.hiveChannel, true);
    assert.equal(sorted[0]?.hiveWalletBolt12Offer, "lno1hive");
  });
});

describe("shouldHydrateChannelForJoinPayment", () => {
  it("refreshes when only a post payment policy is known", () => {
    assert.equal(
      shouldHydrateChannelForJoinPayment(
        channel({ paymentPolicy: paidPostOnlyPolicy }),
      ),
      true,
    );
  });

  it("does not refresh when the join payment policy is already known", () => {
    assert.equal(
      shouldHydrateChannelForJoinPayment(
        channel({ paymentPolicy: paidJoinPolicy }),
      ),
      false,
    );
  });
});
