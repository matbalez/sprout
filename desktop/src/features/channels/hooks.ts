import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import {
  addChannelMembers,
  archiveChannel,
  createChannel,
  deleteChannel,
  getCanvas,
  getChannelDetails,
  getChannelEarnedTotal,
  getChannelMembers,
  getChannelPostSpendTotal,
  getChannels,
  hideDm,
  joinChannel,
  leaveChannel,
  openDm,
  removeChannelMember,
  setCanvas,
  setChannelPurpose,
  setChannelTopic,
  sendChannelMessage,
  unarchiveChannel,
  updateChannel,
} from "@/shared/api/tauri";
import { KIND_FORUM_POST } from "@/shared/constants/kinds";
import {
  formatBitcoinAmount,
  sendChannelPayment,
  withWalletBotChannel,
} from "@/features/wallet/api";
import type {
  AddChannelMembersInput,
  Channel,
  ChannelDetail,
  CreateChannelInput,
  OpenDmInput,
  SetChannelPurposeInput,
  SetChannelTopicInput,
  UpdateChannelInput,
} from "@/shared/api/types";

export const channelsQueryKey = ["channels"] as const;
export const channelPostSpendTotalQueryKey = (channelId: string) =>
  ["channels", channelId, "post-spend-total"] as const;
export const channelEarnedTotalQueryKey = (channelId: string) =>
  ["channels", channelId, "earned-total"] as const;
const channelDetailQueryKey = (channelId: string) =>
  ["channels", channelId, "detail"] as const;
const channelMembersQueryKey = (channelId: string) =>
  ["channels", channelId, "members"] as const;
const channelTypeOrder = {
  stream: 0,
  forum: 1,
  dm: 2,
} as const;

export function getPaidPostAmountForCurrentUser(
  channel: Channel | null | undefined,
): number | null {
  const policy = channel?.paymentPolicy;
  if (!channel || !policy?.postPaymentRequired) {
    return null;
  }
  if (
    channel.currentUserRole === "owner" ||
    channel.currentUserRole === "admin"
  ) {
    return null;
  }
  return policy.postAmountBaseUnits > 0 ? policy.postAmountBaseUnits : null;
}

export function getPaidJoinAmount(
  channel: Channel | null | undefined,
): number | null {
  const policy = channel?.paymentPolicy;
  if (!policy?.joinPaymentRequired) {
    return null;
  }
  return policy.joinAmountBaseUnits > 0 ? policy.joinAmountBaseUnits : null;
}

export async function postPaidJoinNotice(
  channel: Channel,
  amountBaseUnits: number | null,
) {
  if (!amountBaseUnits || amountBaseUnits <= 0) {
    return;
  }

  await sendChannelMessage(
    channel.id,
    `➡️ paid ${formatBitcoinAmount(amountBaseUnits)} to join channel`,
    null,
    undefined,
    undefined,
    channel.channelType === "forum" ? KIND_FORUM_POST : undefined,
  );
}

export async function hydrateChannelForJoinPayment(
  channel: Channel | string,
): Promise<Channel> {
  if (
    typeof channel !== "string" &&
    !shouldHydrateChannelForJoinPayment(channel)
  ) {
    return channel;
  }

  return getChannelDetails(typeof channel === "string" ? channel : channel.id);
}

export function shouldHydrateChannelForJoinPayment(channel: Channel | string) {
  return typeof channel === "string" || getPaidJoinAmount(channel) === null;
}

export function incrementChannelPostSpendTotal(
  queryClient: ReturnType<typeof useQueryClient>,
  channelId: string,
  amountBaseUnits: number | null,
) {
  if (!amountBaseUnits || amountBaseUnits <= 0) {
    return;
  }
  queryClient.setQueryData<number>(
    channelPostSpendTotalQueryKey(channelId),
    (current = 0) => current + amountBaseUnits,
  );
}

export async function payForChannelAction(
  channel: Channel,
  purpose: "join" | "post",
): Promise<string | null> {
  const policy = channel.paymentPolicy;
  if (!policy) {
    return null;
  }

  const amountSats =
    purpose === "join"
      ? getPaidJoinAmount(channel)
      : getPaidPostAmountForCurrentUser(channel);
  if (!amountSats) {
    return null;
  }

  const result = await sendChannelPayment({
    channelId: channel.id,
    metadataEventId: channel.metadataEventId,
    recipientPubkey: policy.paymentRecipientPubkey,
    bolt12Offer: policy.paymentRecipientBolt12Offer,
    amountSats,
    purpose,
  });
  if (!result.receiptAccepted || !result.receiptEventId) {
    throw new Error(result.receiptError ?? "Payment receipt was not accepted.");
  }
  return result.receiptEventId;
}

export function sortChannels(channels: Channel[]) {
  const uniqueChannels = new Map<string, Channel>();

  for (const channel of channels) {
    const existing = uniqueChannels.get(channel.id);
    if (!existing) {
      uniqueChannels.set(channel.id, channel);
      continue;
    }

    uniqueChannels.set(channel.id, {
      ...existing,
      ...channel,
      isMember: existing.isMember || channel.isMember,
      currentUserRole: channel.currentUserRole ?? existing.currentUserRole,
      memberPubkeys:
        channel.memberPubkeys.length > 0
          ? channel.memberPubkeys
          : existing.memberPubkeys,
      participantPubkeys:
        channel.participantPubkeys.length > 0
          ? channel.participantPubkeys
          : existing.participantPubkeys,
      participants:
        channel.participants.length > 0
          ? channel.participants
          : existing.participants,
      paymentPolicy: channel.paymentPolicy ?? existing.paymentPolicy,
      hiveChannel: existing.hiveChannel || channel.hiveChannel,
      hiveWalletBolt12Offer:
        channel.hiveWalletBolt12Offer ?? existing.hiveWalletBolt12Offer,
    });
  }

  return [...uniqueChannels.values()].sort((left, right) => {
    const typeOrder =
      channelTypeOrder[left.channelType] - channelTypeOrder[right.channelType];

    if (typeOrder !== 0) {
      return typeOrder;
    }

    return left.name.localeCompare(right.name);
  });
}

export function sortChannelsWithLocalChannels(channels: Channel[]) {
  return sortChannels(withWalletBotChannel(channels));
}

async function invalidateChannelState(
  queryClient: ReturnType<typeof useQueryClient>,
  channelId: string | null | undefined,
) {
  await queryClient.invalidateQueries({ queryKey: channelsQueryKey });

  if (!channelId) {
    return;
  }

  await Promise.all([
    queryClient.invalidateQueries({
      queryKey: channelDetailQueryKey(channelId),
    }),
    queryClient.invalidateQueries({
      queryKey: channelMembersQueryKey(channelId),
    }),
  ]);
}

function setChannelArchivedState(
  queryClient: ReturnType<typeof useQueryClient>,
  channelId: string,
  archivedAt: string | null,
) {
  queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
    sortChannels(
      current.map((channel) =>
        channel.id === channelId ? { ...channel, archivedAt } : channel,
      ),
    ),
  );

  queryClient.setQueryData<ChannelDetail | undefined>(
    channelDetailQueryKey(channelId),
    (current) => (current ? { ...current, archivedAt } : current),
  );
}

export function useChannelsQuery(options?: { enabled?: boolean }) {
  return useQuery({
    enabled: options?.enabled ?? true,
    queryKey: channelsQueryKey,
    queryFn: async () => sortChannelsWithLocalChannels(await getChannels()),
    staleTime: 60_000,
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  });
}

export function useCreateChannelMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: CreateChannelInput) => createChannel(input),
    onSuccess: (createdChannel) => {
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels([
          ...current.filter((channel) => channel.id !== createdChannel.id),
          createdChannel,
        ]),
      );
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useOpenDmMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: OpenDmInput) => openDm(input),
    onSuccess: (openedChannel) => {
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels([
          ...current.filter((channel) => channel.id !== openedChannel.id),
          openedChannel,
        ]),
      );
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useHideDmMutation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (channelId: string) => hideDm(channelId),
    onMutate: async (channelId) => {
      await queryClient.cancelQueries({ queryKey: channelsQueryKey });
      const previous = queryClient.getQueryData<Channel[]>(channelsQueryKey);
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        current.filter((channel) => channel.id !== channelId),
      );
      return { previous };
    },
    onError: (_error, _channelId, context) => {
      if (context?.previous) {
        queryClient.setQueryData(channelsQueryKey, context.previous);
      }
    },
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: channelsQueryKey });
    },
  });
}

export function useChannelDetailsQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: ["channels", channelId ?? "none", "detail"],
    queryFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return getChannelDetails(channelId);
    },
    staleTime: 30_000,
  });
}

export function useChannelMembersQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: ["channels", channelId ?? "none", "members"],
    queryFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return getChannelMembers(channelId);
    },
    staleTime: 30_000,
  });
}

export function useChannelPostSpendTotalQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: channelPostSpendTotalQueryKey(channelId ?? "none"),
    queryFn: async () => {
      if (!channelId) {
        return 0;
      }
      return getChannelPostSpendTotal(channelId);
    },
    staleTime: 30_000,
  });
}

export function useChannelEarnedTotalQuery(
  channelId: string | null,
  enabled = true,
) {
  return useQuery({
    enabled: enabled && channelId !== null,
    queryKey: channelEarnedTotalQueryKey(channelId ?? "none"),
    queryFn: async () => {
      if (!channelId) {
        return 0;
      }
      return getChannelEarnedTotal(channelId);
    },
    staleTime: 10_000,
    refetchInterval: enabled && channelId !== null ? 15_000 : false,
  });
}

export function useUpdateChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<UpdateChannelInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return updateChannel({ ...input, channelId });
    },
    onSuccess: (updatedChannel) => {
      if (!channelId) {
        return;
      }

      queryClient.setQueryData<ChannelDetail>(
        channelDetailQueryKey(channelId),
        updatedChannel,
      );
      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        sortChannels(
          current.map((channel) =>
            channel.id === updatedChannel.id ? updatedChannel : channel,
          ),
        ),
      );
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSetChannelTopicMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<SetChannelTopicInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return setChannelTopic({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSetChannelPurposeMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<SetChannelPurposeInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return setChannelPurpose({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useArchiveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await archiveChannel(channelId);
    },
    onSuccess: () => {
      if (!channelId) {
        return;
      }

      setChannelArchivedState(queryClient, channelId, new Date().toISOString());
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useUnarchiveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await unarchiveChannel(channelId);
    },
    onSuccess: () => {
      if (!channelId) {
        return;
      }

      setChannelArchivedState(queryClient, channelId, null);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useDeleteChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await deleteChannel(channelId);
    },
    onSuccess: () => {
      if (!channelId) {
        return;
      }

      queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
        current.filter((channel) => channel.id !== channelId),
      );
      queryClient.removeQueries({
        queryKey: channelDetailQueryKey(channelId),
      });
      queryClient.removeQueries({
        queryKey: channelMembersQueryKey(channelId),
      });
    },
    onSettled: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: channelsQueryKey }),
        queryClient.invalidateQueries({ queryKey: ["managed-agents"] }),
        queryClient.invalidateQueries({ queryKey: ["relay-agents"] }),
      ]);
    },
  });
}

export function useAddChannelMembersMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (input: Omit<AddChannelMembersInput, "channelId">) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      return addChannelMembers({ ...input, channelId });
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useRemoveChannelMemberMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async (pubkey: string) => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await removeChannelMember(channelId, pubkey);
    },
    onSettled: async () => {
      await Promise.all([
        invalidateChannelState(queryClient, channelId),
        queryClient.invalidateQueries({ queryKey: ["managed-agents"] }),
        queryClient.invalidateQueries({ queryKey: ["relay-agents"] }),
      ]);
    },
  });
}

export function useJoinChannelMutation(channel: Channel | string | null) {
  const queryClient = useQueryClient();
  const channelId =
    typeof channel === "string" ? channel : (channel?.id ?? null);

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      const joinPaymentChannel = await hydrateChannelForJoinPayment(
        channel ?? channelId,
      );
      const paymentReceiptEventId = await payForChannelAction(
        joinPaymentChannel,
        "join",
      );
      await joinChannel(channelId, paymentReceiptEventId ?? undefined);
      if (paymentReceiptEventId) {
        try {
          await postPaidJoinNotice(
            joinPaymentChannel,
            getPaidJoinAmount(joinPaymentChannel),
          );
        } catch (error) {
          console.warn("Failed to post paid join notice", error);
        }
      }
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useLeaveChannelMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async () => {
      if (!channelId) {
        throw new Error("No channel selected.");
      }

      await leaveChannel(channelId);
    },
    onSettled: async () => {
      await invalidateChannelState(queryClient, channelId);
    },
  });
}

export function useSelectedChannel(
  channels: Channel[],
  preferredChannelId: string | null,
) {
  const [selectedChannelId, setSelectedChannelId] = React.useState<
    string | null
  >(preferredChannelId);

  const selectedChannel = React.useMemo(
    () =>
      channels.find((channel) => channel.id === selectedChannelId) ??
      channels.find((channel) => channel.channelType !== "forum") ??
      channels[0] ??
      null,
    [channels, selectedChannelId],
  );

  React.useEffect(() => {
    if (!selectedChannel && channels.length === 0) {
      return;
    }

    if (!selectedChannelId && selectedChannel) {
      setSelectedChannelId(selectedChannel.id);
      return;
    }

    if (
      selectedChannelId &&
      !channels.some((channel) => channel.id === selectedChannelId) &&
      selectedChannel
    ) {
      setSelectedChannelId(selectedChannel.id);
    }
  }, [channels, selectedChannel, selectedChannelId]);

  return {
    selectedChannel,
    selectedChannelId,
    setSelectedChannelId,
  };
}

// ── Canvas ────────────────────────────────────────────────────────────────────
export function useCanvasQuery(channelId: string | null, enabled = true) {
  return useQuery({
    queryKey: ["channel-canvas", channelId],
    queryFn: () => {
      if (!channelId) {
        return Promise.reject(new Error("No channel selected"));
      }
      return getCanvas(channelId);
    },
    enabled: enabled && channelId !== null,
  });
}

export function useSetCanvasMutation(channelId: string | null) {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (content: string) => {
      if (!channelId) {
        return Promise.reject(new Error("No channel selected"));
      }
      return setCanvas({ channelId, content });
    },
    onSuccess: () => {
      if (channelId) {
        void queryClient.invalidateQueries({
          queryKey: ["channel-canvas", channelId],
        });
      }
    },
  });
}
