import { formatBitcoinAmount } from "@/features/wallet/api";
import { getUserProfile } from "@/shared/api/tauriProfiles";
import { truncatePubkey } from "@/shared/lib/pubkey";

export function normalizeKlaimMentionName(
  name: string | null | undefined,
): string {
  return (name ?? "")
    .replace(/[\r\n\t]+/g, " ")
    .replace(/\s+/g, " ")
    .replace(/^@+/, "")
    .trim();
}

export function formatKlaimGiftConfirmation(input: {
  amountSats: number;
  mentionName: string;
}) {
  const mentionName =
    normalizeKlaimMentionName(input.mentionName) || "recipient";
  return `${formatBitcoinAmount(input.amountSats)} gifted to @${mentionName}`;
}

export async function resolveKlaimGiftMentionName(pubkey: string) {
  try {
    const profile = await getUserProfile(pubkey);
    return (
      normalizeKlaimMentionName(profile.displayName) ||
      normalizeKlaimMentionName(profile.nip05Handle) ||
      truncatePubkey(pubkey)
    );
  } catch {
    return truncatePubkey(pubkey);
  }
}
