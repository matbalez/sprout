import * as React from "react";
import { useQueryClient } from "@tanstack/react-query";

import { getCachedSearchHitEvent } from "@/app/navigation/searchHitEventCache";
import { useAppNavigation } from "@/app/navigation/useAppNavigation";
import {
  channelsQueryKey,
  shouldHydrateChannelForJoinPayment,
  sortChannels,
  useChannelDetailsQuery,
  useChannelsQuery,
} from "@/features/channels/hooks";
import type { Channel } from "@/shared/api/types";
import { ChannelScreen } from "@/features/channels/ui/ChannelScreen";
import { useProfileQuery } from "@/features/profile/hooks";
import { useIdentityQuery } from "@/shared/api/hooks";
import { getEventById } from "@/shared/api/tauri";
import type { RelayEvent } from "@/shared/api/types";
import { ViewLoadingFallback } from "@/shared/ui/ViewLoadingFallback";

type ChannelRouteScreenProps = {
  channelId: string;
  selectedPostId: string | null;
  targetMessageId: string | null;
  targetReplyId: string | null;
  targetThreadRootId: string | null;
};

export function ChannelRouteScreen({
  channelId,
  selectedPostId,
  targetMessageId,
  targetReplyId,
  targetThreadRootId,
}: ChannelRouteScreenProps) {
  const { closeForumPost, goForumPost } = useAppNavigation();
  const queryClient = useQueryClient();
  const channelsQuery = useChannelsQuery();
  const identityQuery = useIdentityQuery();
  const profileQuery = useProfileQuery();
  const channels = channelsQuery.data ?? [];
  const activeChannel =
    channels.find((channel) => channel.id === channelId) ?? null;
  const channelDetailsQuery = useChannelDetailsQuery(
    channelId,
    activeChannel !== null && shouldHydrateChannelForJoinPayment(activeChannel),
  );
  const hydratedActiveChannel = React.useMemo(() => {
    const detail = channelDetailsQuery.data;
    if (!activeChannel || !detail) {
      return activeChannel;
    }
    return {
      ...activeChannel,
      metadataEventId: detail.metadataEventId ?? activeChannel.metadataEventId,
      paymentPolicy: detail.paymentPolicy ?? activeChannel.paymentPolicy,
      hiveChannel: activeChannel.hiveChannel || detail.hiveChannel,
      hiveWalletBolt12Offer:
        detail.hiveWalletBolt12Offer ?? activeChannel.hiveWalletBolt12Offer,
    };
  }, [activeChannel, channelDetailsQuery.data]);
  React.useEffect(() => {
    const detail = channelDetailsQuery.data;
    if (!detail?.paymentPolicy && !detail?.hiveChannel) {
      return;
    }

    queryClient.setQueryData<Channel[]>(channelsQueryKey, (current = []) =>
      sortChannels(
        current.map((channel) =>
          channel.id === detail.id
            ? {
                ...channel,
                metadataEventId: detail.metadataEventId,
                paymentPolicy: detail.paymentPolicy ?? channel.paymentPolicy,
                hiveChannel: channel.hiveChannel || detail.hiveChannel,
                hiveWalletBolt12Offer:
                  detail.hiveWalletBolt12Offer ??
                  channel.hiveWalletBolt12Offer,
              }
            : channel,
        ),
      ),
    );
  }, [channelDetailsQuery.data, queryClient]);
  const [targetMessageEvents, setTargetMessageEvents] = React.useState<
    RelayEvent[]
  >(() => {
    const cachedTarget = getCachedSearchHitEvent(targetMessageId);
    return cachedTarget ? [cachedTarget] : [];
  });

  React.useEffect(() => {
    let isCancelled = false;

    if ((!targetMessageId && !targetThreadRootId) || selectedPostId) {
      setTargetMessageEvents([]);
      return () => {
        isCancelled = true;
      };
    }

    const cachedTarget = getCachedSearchHitEvent(targetMessageId);
    setTargetMessageEvents(cachedTarget ? [cachedTarget] : []);

    const eventIds = [
      targetMessageId,
      targetThreadRootId && targetThreadRootId !== targetMessageId
        ? targetThreadRootId
        : null,
    ].filter((eventId): eventId is string => eventId !== null);

    void Promise.all(
      eventIds.map(async (eventId) => {
        try {
          return await getEventById(eventId);
        } catch (error) {
          console.error("Failed to load route event", eventId, error);
          return null;
        }
      }),
    ).then((events) => {
      if (!isCancelled) {
        setTargetMessageEvents((currentEvents) => {
          const fetchedEvents = events.filter(
            (event): event is RelayEvent => event !== null,
          );
          const eventsById = new Map<string, RelayEvent>();
          for (const event of [...currentEvents, ...fetchedEvents]) {
            eventsById.set(event.id, event);
          }
          return Array.from(eventsById.values());
        });
      }
    });

    return () => {
      isCancelled = true;
    };
  }, [selectedPostId, targetMessageId, targetThreadRootId]);

  if (channelsQuery.isPending && !activeChannel) {
    return (
      <ViewLoadingFallback
        includeHeader
        kind={selectedPostId ? "forum" : "channel"}
      />
    );
  }

  return (
    <ChannelScreen
      activeChannel={hydratedActiveChannel}
      currentIdentity={identityQuery.data}
      currentProfile={profileQuery.data}
      onCloseForumPost={() => {
        void closeForumPost(channelId);
      }}
      onSelectForumPost={(postId) => {
        void goForumPost(channelId, postId);
      }}
      selectedForumPostId={selectedPostId}
      targetForumReplyId={targetReplyId}
      targetMessageEvents={targetMessageEvents}
      targetMessageId={targetMessageId}
    />
  );
}
