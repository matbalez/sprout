import type { QueryClient } from "@tanstack/react-query";

import {
  buildKudosMessageTag,
  KUDOS_PAYMENT_AMOUNT_SATS,
  resolveKudosEchoCreatedAt,
  resolveKudosTargetPubkey,
} from "@/features/messages/lib/messageKudos";
import { formatBitcoinAmount, sendMessageKudos } from "@/features/wallet/api";
import { relayClient } from "@/shared/api/relayClient";

export async function payForKudosMessage({
  channelId,
  currentPubkey,
  mentionPubkeys,
  queryClient,
}: {
  channelId: string;
  currentPubkey: string;
  mentionPubkeys: string[];
  queryClient: QueryClient;
}) {
  const recipientPubkey = resolveKudosTargetPubkey(
    mentionPubkeys,
    currentPubkey,
  );
  await sendMessageKudos({
    channelId,
    recipientPubkey,
  });
  void queryClient.invalidateQueries({
    queryKey: ["lightning-wallet", "summary"],
  });

  return [buildKudosMessageTag()];
}

export function echoKudosPayment(channelId: string, messageCreatedAt: number) {
  void relayClient
    .sendMessage(
      channelId,
      `🤜 ${formatBitcoinAmount(KUDOS_PAYMENT_AMOUNT_SATS)} sent`,
      [],
      [],
      undefined,
      {
        createdAt: resolveKudosEchoCreatedAt(messageCreatedAt),
      },
    )
    .catch((error) => {
      console.error("Failed to echo kudos payment", error);
    });
}
