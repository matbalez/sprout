import { useEffect, useEffectEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";

import {
  channelMessagesKey,
  dedupeMessagesById,
  normalizeTimelineMessages,
  sortMessages,
} from "@/features/messages/lib/messageQueryKeys";
import {
  buildReplyTags,
  getChannelIdFromTags,
  getThreadReference,
  normalizeMentionPubkeys,
  resolveReplyRootId,
} from "@/features/messages/lib/threading";
import { createOptimisticMessage } from "@/features/messages/lib/optimisticMessage";
import { splitOutgoingTags } from "@/features/messages/lib/imetaMediaMarkdown";
import { relayClient } from "@/shared/api/relayClient";
import { customEmojiQueryKey } from "@/features/custom-emoji/hooks";
import {
  getPaidPostAmountForCurrentUser,
  incrementChannelPostSpendTotal,
  payForChannelAction,
} from "@/features/channels/hooks";
import { reactionEmojiUrl } from "@/shared/api/customEmoji";
import type { CustomEmoji } from "@/shared/lib/remarkCustomEmoji";
import {
  addReaction,
  deleteMessage,
  editMessage,
  removeReaction,
  sendChannelMessage,
} from "@/shared/api/tauri";
import type {
  Channel,
  Identity,
  ManagedAgent,
  Profile,
  RelayAgent,
  RelayEvent,
} from "@/shared/api/types";
import { buildKudosMessageTag } from "@/features/messages/lib/messageKudos";
import {
  echoKudosPayment,
  payForKudosMessage,
} from "@/features/messages/lib/sendMessageKudos";
import {
  echoSharedAgentInvocationPayments,
  payForSharedAgentInvocations,
} from "@/features/messages/lib/sendSharedAgentInvocationPayment";
import {
  getWalletBotMessages,
  isWalletBotChannel,
  isWalletBotChannelId,
  sendWalletBotCommand,
  WALLETBOT_MESSAGES_UPDATED,
  type WalletBotMessagesPayload,
  walletBotMessagesToRelayEvents,
} from "@/features/wallet/api";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
// Same .mjs the renderer uses, so the cache-update projection can't drift
// from the on-render overlay.
import { applyEditTagOverlay } from "@/features/messages/lib/applyEditTagOverlay.mjs";
import {
  buildMessageBountyTag,
  resolveBountyTargetPubkey,
} from "@/features/messages/lib/messageBounties";
import {
  KIND_STREAM_MESSAGE,
  KIND_SYSTEM_MESSAGE,
} from "@/shared/constants/kinds";

type MessageQueryContext = {
  optimisticId: string;
  previousMessages: RelayEvent[];
  queryKey: ReturnType<typeof channelMessagesKey>;
};
type WalletBotMutationResult = RelayEvent & {
  walletBotEvents?: RelayEvent[];
};

const CHANNEL_HISTORY_LIMIT = 200;

function getLocalRenderKey(message: RelayEvent) {
  return message.localKey ?? message.id;
}

function isMatchingPendingMessage(pending: RelayEvent, incoming: RelayEvent) {
  if (
    !pending.pending ||
    pending.content !== incoming.content ||
    pending.kind !== incoming.kind ||
    pending.pubkey.toLowerCase() !== incoming.pubkey.toLowerCase() ||
    getChannelIdFromTags(pending.tags) !== getChannelIdFromTags(incoming.tags)
  ) {
    return false;
  }

  const pendingThread = getThreadReference(pending.tags);
  const incomingThread = getThreadReference(incoming.tags);

  return (
    pendingThread.parentId === incomingThread.parentId &&
    pendingThread.rootId === incomingThread.rootId
  );
}

function mergeMessagesWithNormalizer(
  current: RelayEvent[],
  incoming: RelayEvent,
  normalize: (messages: RelayEvent[]) => RelayEvent[],
): RelayEvent[] {
  const normalizedCurrent = dedupeMessagesById(current);
  const replacedPending = normalizedCurrent.find((message) =>
    isMatchingPendingMessage(message, incoming),
  );
  const incomingWithLocalKey = replacedPending
    ? {
        ...incoming,
        localKey: replacedPending.localKey ?? replacedPending.id,
      }
    : incoming;
  const incomingLocalKey = getLocalRenderKey(incomingWithLocalKey);
  const deduped = normalizedCurrent.filter(
    (message) =>
      message.id !== incoming.id &&
      getLocalRenderKey(message) !== incomingLocalKey &&
      !isMatchingPendingMessage(message, incoming),
  );

  return normalize([...deduped, incomingWithLocalKey]);
}

export function mergeMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  return mergeMessagesWithNormalizer(current, incoming, sortMessages);
}

export function mergeTimelineCacheMessages(
  current: RelayEvent[],
  incoming: RelayEvent,
): RelayEvent[] {
  return mergeMessagesWithNormalizer(
    current,
    incoming,
    normalizeTimelineMessages,
  );
}

export function useChannelMessagesQuery(channel: Channel | null) {
  const queryClient = useQueryClient();
  const queryKey = channelMessagesKey(channel?.id ?? "none");

  return useQuery({
    enabled: channel !== null && channel.channelType !== "forum",
    placeholderData: () => queryClient.getQueryData<RelayEvent[]>(queryKey),
    queryKey,
    queryFn: async () => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      if (isWalletBotChannel(channel)) {
        const messages = await getWalletBotMessages();
        return normalizeTimelineMessages(
          walletBotMessagesToRelayEvents(messages),
        );
      }

      const history = await relayClient.fetchChannelHistory(
        channel.id,
        CHANNEL_HISTORY_LIMIT,
      );
      const currentMessages =
        queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
      const mergedHistory = normalizeTimelineMessages([
        ...currentMessages,
        ...history,
      ]);

      return mergedHistory;
    },
    staleTime: 5 * 60 * 1_000,
    gcTime: 5 * 60 * 1_000,
  });
}

export function useChannelSubscription(channel: Channel | null) {
  const queryClient = useQueryClient();
  const channelId = channel?.id ?? null;
  const channelType = channel?.channelType ?? null;
  const syncLatestHistory = useEffectEvent(async () => {
    if (!channelId) {
      return;
    }

    const history = await relayClient.fetchChannelHistory(
      channelId,
      CHANNEL_HISTORY_LIMIT,
    );

    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current = []) => {
        const mergedHistory = normalizeTimelineMessages([
          ...current,
          ...history,
        ]);

        return mergedHistory;
      },
    );
  });

  const appendMessage = useEffectEvent((event: RelayEvent) => {
    if (!channelId) {
      return;
    }

    queryClient.setQueryData<RelayEvent[]>(
      channelMessagesKey(channelId),
      (current = []) => mergeTimelineCacheMessages(current, event),
    );

    if (event.kind === KIND_SYSTEM_MESSAGE) {
      try {
        const payload = JSON.parse(event.content) as { type?: string };
        if (
          payload.type === "member_joined" ||
          payload.type === "member_left" ||
          payload.type === "member_removed"
        ) {
          void queryClient.invalidateQueries({
            queryKey: ["channels", channelId, "members"],
          });
          void queryClient.invalidateQueries({
            queryKey: ["channels"],
            exact: true,
          });
        }
      } catch {
        // Non-JSON system message — ignore.
      }
    }
  });

  useEffect(() => {
    if (!channelId || channelType === "forum") {
      return;
    }

    if (isWalletBotChannelId(channelId)) {
      let isDisposed = false;
      let unlisten: (() => void) | null = null;

      listen<WalletBotMessagesPayload>(WALLETBOT_MESSAGES_UPDATED, (event) => {
        if (isDisposed) {
          return;
        }

        queryClient.setQueryData<RelayEvent[]>(
          channelMessagesKey(channelId),
          normalizeTimelineMessages(
            walletBotMessagesToRelayEvents(event.payload.messages),
          ),
        );
      })
        .then((fn) => {
          if (isDisposed) {
            fn();
            return;
          }
          unlisten = fn;
        })
        .catch((error) => {
          console.error("Failed to listen for WalletBot messages", error);
        });

      return () => {
        isDisposed = true;
        unlisten?.();
      };
    }

    let isDisposed = false;
    let cleanup: (() => Promise<void>) | undefined;
    const disposeReconnectListener = relayClient.subscribeToReconnects(() => {
      void syncLatestHistory().catch((error) => {
        if (!isDisposed) {
          console.error(
            "Failed to refresh channel history after reconnecting",
            channelId,
            error,
          );
        }
      });
    });

    relayClient
      .subscribeToChannel(channelId, (event) => {
        if (!isDisposed) {
          appendMessage(event);
        }
      })
      .then((dispose) => {
        if (isDisposed) {
          void dispose();
          return;
        }

        cleanup = dispose;

        void syncLatestHistory().catch((error) => {
          if (!isDisposed) {
            console.error(
              "Failed to refresh channel history after subscribing",
              channelId,
              error,
            );
          }
        });
      })
      .catch((error) => {
        console.error("Failed to subscribe to channel", channelId, error);
      });

    return () => {
      isDisposed = true;
      disposeReconnectListener();
      if (cleanup) {
        void cleanup();
      }
    };
  }, [channelId, channelType, queryClient]);
}

export function useSendMessageMutation(
  channel: Channel | null,
  identity: Identity | undefined,
  sharedAgentInvocationPayments?: {
    currentProfile?: Pick<
      Profile,
      "pubkey" | "displayName" | "avatarUrl" | "nip05Handle"
    > | null;
    managedAgents: readonly ManagedAgent[];
    profiles?: UserProfileLookup;
    relayAgents: readonly RelayAgent[];
  },
) {
  const queryClient = useQueryClient();

  return useMutation<
    RelayEvent,
    Error,
    {
      content: string;
      mentionPubkeys?: string[];
      parentEventId?: string | null;
      mediaTags?: string[][];
      kudos?: boolean;
      bountyAmountSats?: number | null;
    },
    MessageQueryContext | undefined
  >({
    mutationFn: async ({
      content,
      bountyAmountSats,
      kudos,
      mentionPubkeys,
      parentEventId,
      mediaTags,
    }) => {
      if (!channel || channel.channelType === "forum") {
        throw new Error("This channel does not support message sending yet.");
      }

      if (!identity) {
        throw new Error("No identity available for sending messages.");
      }

      const normalizedMentionPubkeys = mentionPubkeys ?? [];
      // `mediaTags` arrives as the merged outgoing tag set (imeta + NIP-30
      // emoji). Split it so each kind goes to its own validated Tauri arg —
      // emoji tags must NOT ride the imeta-only `media` channel (that gate
      // rejects any non-imeta prefix, which silently dropped emoji sends).
      const {
        mediaTags: imetaTags,
        emojiTags,
        mentionTags,
      } = splitOutgoingTags(mediaTags);

      if (isWalletBotChannel(channel)) {
        if (
          parentEventId ||
          (mediaTags && mediaTags.length > 0) ||
          kudos ||
          typeof bountyAmountSats === "number"
        ) {
          throw new Error("WalletBot only supports plain commands.");
        }

        const messages = await sendWalletBotCommand(content);
        const events = normalizeTimelineMessages(
          walletBotMessagesToRelayEvents(messages),
        );
        const lastEvent = events[events.length - 1];
        if (!lastEvent) {
          throw new Error("WalletBot did not return a message.");
        }

        return {
          ...lastEvent,
          walletBotEvents: events,
        } satisfies WalletBotMutationResult;
      }

      const bountyTargetPubkey =
        typeof bountyAmountSats === "number"
          ? resolveBountyTargetPubkey(normalizedMentionPubkeys)
          : null;
      const sharedAgentPaymentTargets = sharedAgentInvocationPayments
        ? await payForSharedAgentInvocations({
            channelId: channel.id,
            currentIdentity: identity,
            currentProfile: sharedAgentInvocationPayments.currentProfile,
            managedAgents: sharedAgentInvocationPayments.managedAgents,
            mentionPubkeys: normalizedMentionPubkeys,
            profiles: sharedAgentInvocationPayments.profiles,
            queryClient,
            relayAgents: sharedAgentInvocationPayments.relayAgents,
          })
        : [];
      const annotationTags = [
        ...(kudos
          ? await payForKudosMessage({
              channelId: channel.id,
              currentPubkey: identity.pubkey,
              mentionPubkeys: normalizedMentionPubkeys,
              queryClient,
            })
          : []),
      ];
      if (typeof bountyAmountSats === "number") {
        if (!bountyTargetPubkey) {
          throw new Error("Message bounties require exactly one @mention.");
        }
        annotationTags.push(
          buildMessageBountyTag({
            amountSats: bountyAmountSats,
            recipientPubkey: bountyTargetPubkey,
          }),
        );
      }
      const paidPostReceiptEventId = await payForChannelAction(channel, "post");
      const paymentTags = paidPostReceiptEventId
        ? [["payment", paidPostReceiptEventId, "post"]]
        : [];

      // Messages carrying media OR custom-emoji tags MUST go through REST so
      // the relay's tag validation runs. The WebSocket path emits no extra
      // tags, so emoji-only messages would otherwise lose their emoji tag.
      if (
        parentEventId ||
        imetaTags.length > 0 ||
        emojiTags.length > 0 ||
        paidPostReceiptEventId
      ) {
        const cachedMessages =
          queryClient.getQueryData<RelayEvent[]>(
            channelMessagesKey(channel.id),
          ) ?? [];
        const result = await sendChannelMessage(
          channel.id,
          content,
          parentEventId ?? null,
          imetaTags,
          normalizedMentionPubkeys,
          undefined,
          annotationTags,
          emojiTags,
          paidPostReceiptEventId,
          mentionTags,
        );

        // Build tags matching relay-emitted shape: h, actor, author p, mention ps, reply es, imeta, emoji, annotations.
        // For replies, buildReplyTags already includes actor, ["p", author], and ["h", channel].
        // For non-replies (media-only), we add them ourselves.
        const replyTags = parentEventId
          ? buildReplyTags(
              channel.id,
              identity.pubkey,
              parentEventId,
              resolveReplyRootId(parentEventId, cachedMessages),
              normalizedMentionPubkeys,
            )
          : [];
        const baseTags = parentEventId
          ? replyTags // buildReplyTags includes h + actor + author p + mention ps
          : [
              ["h", channel.id],
              ["actor", identity.pubkey.toLowerCase()],
              ["p", identity.pubkey.toLowerCase()],
            ]; // non-reply: add ourselves

        const sentMessage = {
          id: result.eventId,
          pubkey: identity.pubkey,
          created_at: result.createdAt,
          kind: KIND_STREAM_MESSAGE,
          tags: [
            ...baseTags,
            // For non-replies, add mention p-tags here (replies get them via buildReplyTags)
            ...(!parentEventId
              ? normalizeMentionPubkeys(
                  normalizedMentionPubkeys,
                  identity.pubkey,
                ).map((pk) => ["p", pk])
              : []),
            ...imetaTags,
            ...emojiTags,
            ...annotationTags,
            ...paymentTags,
            ...mentionTags,
          ],
          content: content.trim(),
          sig: "",
        };

        if (kudos) {
          echoKudosPayment(channel.id, sentMessage.created_at);
        }
        if (sharedAgentPaymentTargets.length > 0) {
          echoSharedAgentInvocationPayments(
            channel.id,
            sharedAgentPaymentTargets,
          );
        }

        return sentMessage;
      }

      const sentMessage = await relayClient.sendMessage(
        channel.id,
        content,
        normalizedMentionPubkeys,
        [...annotationTags, ...mentionTags],
        identity.pubkey,
      );

      if (kudos) {
        echoKudosPayment(channel.id, sentMessage.created_at);
      }
      if (sharedAgentPaymentTargets.length > 0) {
        echoSharedAgentInvocationPayments(
          channel.id,
          sharedAgentPaymentTargets,
        );
      }

      return sentMessage;
    },
    onMutate: async ({
      content,
      bountyAmountSats,
      kudos,
      mentionPubkeys,
      parentEventId,
      mediaTags,
    }) => {
      if (
        !channel ||
        !identity ||
        channel.channelType === "forum" ||
        getPaidPostAmountForCurrentUser(channel) !== null
      ) {
        return undefined;
      }

      const queryKey = channelMessagesKey(channel.id);
      await queryClient.cancelQueries({ queryKey });

      const previousMessages =
        queryClient.getQueryData<RelayEvent[]>(queryKey) ?? [];
      const bountyAnnotationTags =
        typeof bountyAmountSats === "number"
          ? (() => {
              try {
                return [
                  buildMessageBountyTag({
                    amountSats: bountyAmountSats,
                    recipientPubkey: resolveBountyTargetPubkey(
                      mentionPubkeys ?? [],
                    ),
                  }),
                ];
              } catch {
                return [];
              }
            })()
          : [];
      const optimisticMessage = createOptimisticMessage(
        channel.id,
        content.trim(),
        identity,
        previousMessages,
        mentionPubkeys ?? [],
        parentEventId ?? null,
        mediaTags ?? [],
        [...(kudos ? [buildKudosMessageTag()] : []), ...bountyAnnotationTags],
      );

      queryClient.setQueryData<RelayEvent[]>(
        queryKey,
        mergeTimelineCacheMessages(previousMessages, optimisticMessage),
      );

      return {
        optimisticId: optimisticMessage.id,
        previousMessages,
        queryKey,
      };
    },
    onError: (_error, _variables, context) => {
      if (!context) {
        return;
      }

      queryClient.setQueryData(context.queryKey, context.previousMessages);
    },
    onSuccess: (message, _variables, context) => {
      if (channel) {
        incrementChannelPostSpendTotal(
          queryClient,
          channel.id,
          getPaidPostAmountForCurrentUser(channel),
        );
      }

      if (!context) {
        if (channel) {
          queryClient.setQueryData<RelayEvent[]>(
            channelMessagesKey(channel.id),
            (current = []) => mergeTimelineCacheMessages(current, message),
          );
        }
        return;
      }

      const walletBotEvents = (message as WalletBotMutationResult)
        .walletBotEvents;
      if (walletBotEvents) {
        queryClient.setQueryData<RelayEvent[]>(
          context.queryKey,
          normalizeTimelineMessages(walletBotEvents),
        );
        return;
      }

      queryClient.setQueryData<RelayEvent[]>(context.queryKey, (current = []) =>
        mergeTimelineCacheMessages(current, {
          ...message,
          localKey: context.optimisticId,
        }),
      );
    },
  });
}

export function useToggleReactionMutation() {
  const queryClient = useQueryClient();
  return useMutation<
    void,
    Error,
    {
      eventId: string;
      emoji: string;
      remove: boolean;
    }
  >({
    mutationFn: async ({ eventId, emoji, remove }) => {
      if (remove) {
        await removeReaction(eventId, emoji);
        return;
      }

      // Custom-emoji reaction: emoji is `:shortcode:`. Resolve its image URL
      // from the cached workspace palette so the kind:7 carries the NIP-30
      // `["emoji", shortcode, url]` tag. Unicode reactions resolve to no URL.
      const emojiUrl = reactionEmojiUrl(
        emoji,
        queryClient.getQueryData<CustomEmoji[]>(customEmojiQueryKey),
      );
      await addReaction(eventId, emoji, emojiUrl);
    },
  });
}

export function useDeleteMessageMutation(channel: Channel | null) {
  const queryClient = useQueryClient();

  return useMutation<void, Error, { eventId: string }>({
    mutationFn: async ({ eventId }) => {
      if (!channel) {
        throw new Error("No channel selected.");
      }
      await deleteMessage(channel.id, eventId);
    },
    onSuccess: (_data, { eventId }) => {
      if (!channel) return;
      queryClient.setQueryData<RelayEvent[]>(
        channelMessagesKey(channel.id),
        (current = []) => current.filter((message) => message.id !== eventId),
      );
    },
  });
}

export function useEditMessageMutation(channel: Channel | null) {
  const queryClient = useQueryClient();

  return useMutation<
    void,
    Error,
    {
      eventId: string;
      content: string;
      mediaTags?: string[][];
    }
  >({
    mutationFn: async ({ eventId, content, mediaTags }) => {
      if (!channel) {
        throw new Error("No channel selected.");
      }

      // `mediaTags` arrives as the merged outgoing set (imeta + NIP-30 emoji).
      // Split so each rides its own validated Tauri arg — emoji tags must NOT
      // go through the imeta-only `mediaTags` channel (the Rust `imeta_tags`
      // guard rejects any non-imeta prefix), mirroring the send path.
      const { mediaTags: imetaTags, emojiTags } = splitOutgoingTags(mediaTags);

      await editMessage(channel.id, eventId, content, imetaTags, emojiTags);
    },
    onSuccess: (_data, { eventId, content, mediaTags }) => {
      if (!channel) {
        return;
      }

      queryClient.setQueryData<RelayEvent[]>(
        channelMessagesKey(channel.id),
        (current = []) =>
          current.map((message) => {
            if (message.id !== eventId) return message;
            // Apply-on-success cache update: reflect the edit's new content
            // and imeta tag set immediately, so the local cache matches
            // what the receiver overlay (formatTimelineMessages) will
            // produce when the edit event arrives back from the relay.
            // (Not a true optimistic update — runs in onSuccess, not
            // onMutate. Worth bearing the cost only because the edit event
            // round-trip can lag perceptibly.)
            const nextTags = mediaTags
              ? applyEditTagOverlay(message.tags, mediaTags)
              : message.tags;
            return { ...message, content, tags: nextTags };
          }),
      );
    },
  });
}
