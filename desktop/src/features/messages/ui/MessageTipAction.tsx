import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";

import { channelMessagesKey } from "@/features/messages/lib/messageQueryKeys";
import type { TimelineMessage } from "@/features/messages/types";
import {
  formatBitcoinAmount,
  getUserWalletBolt12Offer,
  sendMessageTip,
} from "@/features/wallet/api";
import { KIND_SYSTEM_MESSAGE } from "@/shared/constants/kinds";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";

const MESSAGE_TIP_AMOUNT_SATS = 10;

export function MessageTipAction({
  channelId,
  message,
}: {
  channelId?: string | null;
  message: TimelineMessage;
}) {
  const queryClient = useQueryClient();
  const recipientPubkey = message.pubkey?.toLowerCase() ?? null;
  const canCheckTip =
    Boolean(channelId && recipientPubkey) &&
    !message.pending &&
    !message.accent &&
    message.kind !== KIND_SYSTEM_MESSAGE;

  const walletOfferQuery = useQuery({
    enabled: canCheckTip,
    queryKey: ["user-wallet-bolt12-offer", recipientPubkey],
    queryFn: () => getUserWalletBolt12Offer(recipientPubkey ?? ""),
    retry: false,
    staleTime: 60_000,
  });

  const tipMutation = useMutation({
    mutationFn: async () => {
      if (!channelId || !recipientPubkey) {
        throw new Error("No message author wallet target available.");
      }

      return sendMessageTip({
        channelId,
        messageId: message.id,
        recipientPubkey,
      });
    },
    onSuccess: (result) => {
      const amount = formatBitcoinAmount(result.amountSats);
      if (result.receiptAccepted) {
        toast.success(`Sent ${amount}`);
      } else {
        toast.warning(
          `Sent ${amount}, but could not annotate the message: ${
            result.receiptError ?? "receipt was not accepted"
          }`,
        );
      }

      if (channelId) {
        void queryClient.invalidateQueries({
          queryKey: channelMessagesKey(channelId),
        });
      }
    },
    onError: (error) => {
      toast.error(
        error instanceof Error ? error.message : "Failed to send tip.",
      );
    },
  });

  if (!canCheckTip || !walletOfferQuery.data) {
    return null;
  }

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          aria-label={`Tip ${formatBitcoinAmount(MESSAGE_TIP_AMOUNT_SATS)}`}
          className="h-6 w-6 rounded-full p-0"
          data-testid={`tip-message-${message.id}`}
          disabled={tipMutation.isPending}
          onClick={() => {
            tipMutation.mutate();
          }}
          size="sm"
          type="button"
          variant={
            message.tipSummary?.tippedByCurrentUser ? "secondary" : "ghost"
          }
        >
          {tipMutation.isPending ? (
            <Spinner className="h-3 w-3" />
          ) : (
            <span className="text-xs font-semibold leading-none">₿</span>
          )}
        </Button>
      </TooltipTrigger>
      <TooltipContent>
        Tip {formatBitcoinAmount(MESSAGE_TIP_AMOUNT_SATS)}
      </TooltipContent>
    </Tooltip>
  );
}
