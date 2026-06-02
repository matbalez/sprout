import {
  classifyKlaimPayoutResult,
  klaimPayoutErrorLabel,
  shouldPostKlaimGiftConfirmation,
} from "./klaimGiftResults";
import {
  formatKlaimGiftConfirmation,
  resolveKlaimGiftMentionName,
} from "./klaimGiftReceipt";
import { payKlaimFaucetMember } from "@/features/klaim-gifts/api";
import {
  getKlaimGiftConfig,
  isKlaimGiftProcessableStatus,
  markKlaimGiftMember,
} from "@/features/klaim-gifts/storage";
import { getUserWalletBolt12Offer } from "@/features/wallet/api";
import { sendChannelMessage } from "@/shared/api/tauri";
import type { Channel } from "@/shared/api/types";
import { normalizePubkey } from "@/shared/lib/pubkey";

type KlaimGiftChannel = Pick<Channel, "id" | "channelType" | "visibility">;

export type KlaimGiftProcessingResult = {
  attempted: string[];
  paid: string[];
  skipped: string[];
  failed: Array<{ pubkey: string; error: string }>;
};

function emptyResult(): KlaimGiftProcessingResult {
  return {
    attempted: [],
    paid: [],
    skipped: [],
    failed: [],
  };
}

function uniquePubkeys(pubkeys: string[]) {
  return [...new Set(pubkeys.map(normalizePubkey).filter(Boolean))];
}

function hasReachedGiftCapacity(input: {
  claimsUsed: number | null;
  maxClaims: number | null;
}) {
  return (
    typeof input.claimsUsed === "number" &&
    typeof input.maxClaims === "number" &&
    input.claimsUsed >= input.maxClaims
  );
}

export async function processKlaimGiftMembers(input: {
  channel: KlaimGiftChannel;
  pubkeys: string[];
}): Promise<KlaimGiftProcessingResult> {
  const result = emptyResult();
  const config = getKlaimGiftConfig(input.channel.id);
  if (
    !config?.enabled ||
    input.channel.channelType === "dm" ||
    input.channel.visibility !== "private"
  ) {
    return result;
  }

  for (const pubkey of uniquePubkeys(input.pubkeys)) {
    const latestConfig = getKlaimGiftConfig(input.channel.id);
    const memberState = latestConfig?.members[pubkey];
    if (
      !latestConfig?.enabled ||
      hasReachedGiftCapacity(latestConfig) ||
      (memberState && !isKlaimGiftProcessableStatus(memberState.status))
    ) {
      result.skipped.push(pubkey);
      continue;
    }

    result.attempted.push(pubkey);
    markKlaimGiftMember({
      channelId: input.channel.id,
      pubkey,
      status: "pending",
    });

    try {
      const bolt12 = await getUserWalletBolt12Offer(pubkey);
      if (!bolt12) {
        const error = "Member does not have a published BOLT12 offer yet.";
        markKlaimGiftMember({
          channelId: input.channel.id,
          pubkey,
          status: "missing-bolt12",
          error,
        });
        result.failed.push({ pubkey, error });
        continue;
      }

      const payoutConfig = getKlaimGiftConfig(input.channel.id);
      if (!payoutConfig?.enabled || hasReachedGiftCapacity(payoutConfig)) {
        result.skipped.push(pubkey);
        continue;
      }

      const payout = await payKlaimFaucetMember({
        channelId: payoutConfig.klaimChannelId,
        nostrPubkey: pubkey,
        bolt12,
      });
      const status = classifyKlaimPayoutResult(payout);
      markKlaimGiftMember({
        channelId: input.channel.id,
        pubkey,
        status,
        amountSats: payout.amountSats,
        claimsUsed: payout.claimsUsed,
        maxClaims: payout.maxClaims,
        error: payout.ok ? null : klaimPayoutErrorLabel(payout),
      });

      if (
        shouldPostKlaimGiftConfirmation(payout) &&
        payout.amountSats != null
      ) {
        const mentionName = await resolveKlaimGiftMentionName(pubkey);
        await sendChannelMessage(
          input.channel.id,
          formatKlaimGiftConfirmation({
            amountSats: payout.amountSats,
            mentionName,
          }),
          null,
          undefined,
          [pubkey],
        );
        result.paid.push(pubkey);
      } else if (!payout.ok) {
        result.failed.push({ pubkey, error: klaimPayoutErrorLabel(payout) });
      }
    } catch (error) {
      const message =
        error instanceof Error ? error.message : "Failed to process gift.";
      markKlaimGiftMember({
        channelId: input.channel.id,
        pubkey,
        status: "retryable",
        error: message,
      });
      result.failed.push({ pubkey, error: message });
    }
  }

  return result;
}
