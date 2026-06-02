import {
  buildReplyTags,
  normalizeMentionPubkeys,
  resolveReplyRootId,
} from "@/features/messages/lib/threading";
import type { Identity, RelayEvent } from "@/shared/api/types";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

export function createOptimisticMessage(
  channelId: string,
  content: string,
  identity: Identity,
  currentMessages: RelayEvent[],
  mentionPubkeys: string[] = [],
  parentEventId: string | null = null,
  mediaTags: string[][] = [],
  annotationTags: string[][] = [],
): RelayEvent {
  const tags: string[][] = [];

  if (parentEventId) {
    tags.push(
      ...buildReplyTags(
        channelId,
        identity.pubkey,
        parentEventId,
        resolveReplyRootId(parentEventId, currentMessages),
        mentionPubkeys,
      ),
    );
  } else {
    const identityPubkey = identity.pubkey.toLowerCase();
    tags.push(
      ["h", channelId],
      ["actor", identityPubkey],
      ["p", identityPubkey],
    );
    for (const pubkey of normalizeMentionPubkeys(
      mentionPubkeys,
      identity.pubkey,
    )) {
      tags.push(["p", pubkey]);
    }
  }

  tags.push(...mediaTags, ...annotationTags);

  return {
    id: `optimistic-${crypto.randomUUID()}`,
    pubkey: identity.pubkey,
    created_at: Math.floor(Date.now() / 1_000),
    kind: KIND_STREAM_MESSAGE,
    tags,
    content,
    sig: "",
    pending: true,
  };
}
