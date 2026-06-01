import type { RelayEvent } from "@/shared/api/types";

export const KUDOS_BALANCE_THRESHOLD_SATS = 2_000;
export const KUDOS_PAYMENT_AMOUNT_SATS = 210;

const KUDOS_MESSAGE_TAG = ["sprout", "kudos", "v1"] as const;

export function buildKudosMessageTag(): string[] {
  return [...KUDOS_MESSAGE_TAG];
}

export function isKudosMessageTags(tags: string[][] | undefined) {
  return (
    tags?.some(
      (tag) =>
        tag[0] === KUDOS_MESSAGE_TAG[0] &&
        tag[1] === KUDOS_MESSAGE_TAG[1] &&
        tag[2] === KUDOS_MESSAGE_TAG[2],
    ) ?? false
  );
}

export function isKudosMessageEvent(event: RelayEvent) {
  return isKudosMessageTags(event.tags);
}

export function resolveKudosTargetPubkey(
  mentionPubkeys: string[],
  currentPubkey?: string,
) {
  const current = currentPubkey?.toLowerCase() ?? null;
  const targets = [
    ...new Set(
      mentionPubkeys
        .map((pubkey) => pubkey.trim().toLowerCase())
        .filter(Boolean)
        .filter((pubkey) => pubkey !== current),
    ),
  ];

  if (targets.length === 0) {
    throw new Error("Kudos requires one @mention with a wallet BOLT12 offer.");
  }

  if (targets.length > 1) {
    throw new Error("Kudos can only be sent to one @mention at a time.");
  }

  const target = targets[0];
  if (!target) {
    throw new Error("Kudos requires one @mention with a wallet BOLT12 offer.");
  }

  return target;
}
