import { formatBitcoinAmount } from "@/features/wallet/api";
import { getUserProfile } from "@/shared/api/tauri";

function shortPubkey(pubkey: string) {
  return `${pubkey.slice(0, 8)}...${pubkey.slice(-4)}`;
}

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
      shortPubkey(pubkey)
    );
  } catch {
    return shortPubkey(pubkey);
  }
}
