import type { RelayEvent } from "@/shared/api/types";
import type { TimelineTipSummary } from "@/features/messages/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { KIND_REACTION } from "@/shared/constants/kinds";
import { resolveEventAuthorPubkey } from "@/shared/lib/authors";

const HEX_RE = /^[0-9a-f]+$/i;
const TIP_RECEIPT_CONTENT_PREFIX = "sprout-tip:";

function getTipTargetId(tags: string[][]) {
  for (let index = tags.length - 1; index >= 0; index -= 1) {
    const tag = tags[index];
    if (
      tag?.[0] === "e" &&
      typeof tag[1] === "string" &&
      tag[1].length === 64 &&
      HEX_RE.test(tag[1])
    ) {
      return tag[1];
    }
  }

  return null;
}

function getTagValue(tags: string[][], name: string) {
  return tags.find((tag) => tag[0] === name)?.[1] ?? null;
}

function getTaggedPubkey(tags: string[][], name: string) {
  const value = getTagValue(tags, name);
  return value && value.length === 64 && HEX_RE.test(value)
    ? value.toLowerCase()
    : null;
}

function parseTipAmount(tags: string[][]) {
  const amount = getTagValue(tags, "amount");
  if (!amount || !/^[1-9][0-9]*$/.test(amount)) {
    return null;
  }

  const parsed = Number.parseInt(amount, 10);
  return Number.isSafeInteger(parsed) ? parsed : null;
}

export function isMessageTipReceiptEvent(event: RelayEvent) {
  return (
    event.kind === KIND_REACTION &&
    event.content.trim().startsWith(TIP_RECEIPT_CONTENT_PREFIX) &&
    getTagValue(event.tags, "wallet") === "lexe-bolt12" &&
    getTagValue(event.tags, "status") === "sender-confirmed" &&
    getTagValue(event.tags, "tip_id") !== null
  );
}

export function buildTipsByEventId({
  currentPubkeyLower,
  deletedEventIds,
  events,
  eventsById,
  profiles,
}: {
  currentPubkeyLower: string | undefined;
  deletedEventIds: Set<string>;
  events: RelayEvent[];
  eventsById: Map<string, RelayEvent>;
  profiles?: UserProfileLookup;
}) {
  const tipPresence = new Map<
    string,
    {
      targetId: string;
      actorPubkey: string;
      amountSats: number;
      tipId: string;
    }
  >();

  for (const event of events) {
    if (!isMessageTipReceiptEvent(event) || deletedEventIds.has(event.id)) {
      continue;
    }

    const targetId = getTipTargetId(event.tags);
    const target = targetId ? eventsById.get(targetId) : null;
    if (!targetId || !target || deletedEventIds.has(targetId)) {
      continue;
    }

    const amountSats = parseTipAmount(event.tags);
    const recipientPubkey = getTaggedPubkey(event.tags, "p");
    const targetAuthorPubkey = resolveEventAuthorPubkey({
      pubkey: target.pubkey,
      tags: target.tags,
      preferActorTag: true,
      requireChannelTagForPTags: true,
    }).toLowerCase();
    if (
      amountSats === null ||
      recipientPubkey === null ||
      recipientPubkey !== targetAuthorPubkey
    ) {
      continue;
    }

    const actorPubkey = event.pubkey.toLowerCase();
    const tipId = getTagValue(event.tags, "tip_id") || event.id;
    tipPresence.set(`${targetId}:${actorPubkey}:${tipId}`, {
      targetId,
      actorPubkey,
      amountSats,
      tipId,
    });
  }

  const tipsByEventId = new Map<string, TimelineTipSummary>();
  for (const {
    targetId,
    actorPubkey,
    amountSats,
    tipId,
  } of tipPresence.values()) {
    const existing = tipsByEventId.get(targetId) ?? {
      amountSats: 0,
      count: 0,
      tippedByCurrentUser: false,
      users: [],
    };

    existing.amountSats += amountSats;
    existing.count += 1;
    if (currentPubkeyLower && actorPubkey === currentPubkeyLower) {
      existing.tippedByCurrentUser = true;
    }

    const profile = profiles?.[actorPubkey];
    const displayName =
      profile?.displayName?.trim() ||
      profile?.nip05Handle?.trim() ||
      `${actorPubkey.slice(0, 8)}…`;
    existing.users.push({
      pubkey: actorPubkey,
      displayName,
      avatarUrl: profile?.avatarUrl ?? null,
      amountSats,
      tipId,
    });

    tipsByEventId.set(targetId, existing);
  }

  return tipsByEventId;
}
