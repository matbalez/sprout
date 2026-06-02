import { claimKlaimCode } from "@/features/klaim-gifts/api";
import {
  formatKlaimGiftConfirmation,
  resolveKlaimGiftMentionName,
} from "@/features/klaim-gifts/klaimGiftReceipt";
import { getUserWalletBolt12Offer } from "@/features/wallet/api";
import { sendChannelMessage } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";

const KLAIM_CLAIM_PATTERN =
  /(?:^|\s)klaim\s+([A-Za-z0-9][A-Za-z0-9_-]{0,127})(?=$|[\s.,!?;:])/i;

export type KlaimClaimMessageResult = {
  matched: boolean;
  claimed: boolean;
  code: string | null;
  error: string | null;
};

export function extractKlaimClaimCode(content: string) {
  const match = KLAIM_CLAIM_PATTERN.exec(content);
  return match?.[1] ?? null;
}

function emptyResult(): KlaimClaimMessageResult {
  return {
    matched: false,
    claimed: false,
    code: null,
    error: null,
  };
}

export async function processKlaimClaimMessage(input: {
  channelId: string;
  event: RelayEvent;
}): Promise<KlaimClaimMessageResult> {
  const code = extractKlaimClaimCode(input.event.content);
  if (!code) {
    return emptyResult();
  }

  const result: KlaimClaimMessageResult = {
    matched: true,
    claimed: false,
    code,
    error: null,
  };

  try {
    const bolt12 = await getUserWalletBolt12Offer(input.event.pubkey);
    if (!bolt12) {
      return {
        ...result,
        error: "Message author does not have a published BOLT12 offer.",
      };
    }

    const claim = await claimKlaimCode({ code, address: bolt12 });
    if (!claim.ok || claim.amountSats == null) {
      return {
        ...result,
        error:
          claim.detail ??
          claim.error ??
          `Klaim claim failed with HTTP ${claim.statusCode}`,
      };
    }

    const mentionName = await resolveKlaimGiftMentionName(input.event.pubkey);
    await sendChannelMessage(
      input.channelId,
      formatKlaimGiftConfirmation({
        amountSats: claim.amountSats,
        mentionName,
      }),
      null,
      undefined,
      [input.event.pubkey],
    );

    return {
      ...result,
      claimed: true,
    };
  } catch (error) {
    return {
      ...result,
      error:
        error instanceof Error
          ? error.message
          : "Failed to process Klaim claim message.",
    };
  }
}
