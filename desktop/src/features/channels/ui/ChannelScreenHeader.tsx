import { LogIn, Wallet } from "lucide-react";
import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import { getChannelDescription } from "@/features/channels/lib/channelDescription";
import { ChannelHeaderStatusBadge } from "@/features/channels/ui/ChannelHeaderStatusBadge";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { getPaidJoinAmount } from "@/features/channels/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import {
  formatBitcoinAmount,
  getHiveChannelWalletSummary,
  sendHiveChannelFunds,
} from "@/features/wallet/api";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import type { Channel, PresenceStatus } from "@/shared/api/types";

type ChannelScreenHeaderProps = {
  activeChannel: Channel | null;
  activeChannelEphemeralDisplay: EphemeralChannelDisplay | null;
  activeChannelTitle: string;
  actionsVariant?: "inline" | "compact";
  activeDmAvatarUrl: string | null;
  activeDmPresenceStatus: PresenceStatus | null;
  chromeWrapperRef?: React.Ref<HTMLDivElement>;
  currentPubkey?: string;
  earnedBaseUnits?: number;
  isAddBotOpen?: boolean;
  isJoining?: boolean;
  postSpendBaseUnits?: number;
  showHeaderContent?: boolean;
  showEarned?: boolean;
  showPostSpend?: boolean;
  onAddBotOpenChange?: (open: boolean) => void;
  onJoinChannel?: () => Promise<void>;
  onManageChannel: () => void;
  onToggleMembers: () => void;
};

export function ChannelScreenHeader({
  activeChannel,
  activeChannelEphemeralDisplay,
  activeChannelTitle,
  actionsVariant = "inline",
  activeDmAvatarUrl,
  activeDmPresenceStatus,
  chromeWrapperRef,
  currentPubkey,
  earnedBaseUnits = 0,
  isAddBotOpen,
  isJoining = false,
  onAddBotOpenChange,
  postSpendBaseUnits = 0,
  showHeaderContent = true,
  showEarned = false,
  showPostSpend = false,
  onJoinChannel,
  onManageChannel,
  onToggleMembers,
}: ChannelScreenHeaderProps) {
  const queryClient = useQueryClient();
  const [hiveFundAmount, setHiveFundAmount] = React.useState("");
  const [isHiveFundingOpen, setIsHiveFundingOpen] = React.useState(false);
  const showJoinButton =
    activeChannel !== null &&
    !activeChannel.isMember &&
    activeChannel.visibility === "open" &&
    !activeChannel.archivedAt &&
    onJoinChannel;
  const paidJoinAmountBaseUnits = getPaidJoinAmount(activeChannel);
  const joinButtonLabel =
    paidJoinAmountBaseUnits !== null
      ? `Pay ${formatBitcoinAmount(paidJoinAmountBaseUnits)} to join`
      : "Join";

  const spendBadge =
    activeChannel && showPostSpend ? (
      <div
        className="rounded-md border border-border/60 bg-muted/45 px-2 py-1 text-xs font-medium text-muted-foreground"
        data-testid="channel-post-spend-total"
        title="Total spent on paid joins and posts in this channel"
      >
        Spent {formatBitcoinAmount(postSpendBaseUnits)}
      </div>
    ) : null;

  const earnedBadge =
    activeChannel && showEarned ? (
      <div
        className="rounded-md border border-border/60 bg-muted/45 px-2 py-1 text-xs font-medium text-muted-foreground"
        data-testid="channel-earned-total"
        title="Total earned from paid joins and posts in this channel"
      >
        Earned {formatBitcoinAmount(earnedBaseUnits)}
      </div>
    ) : null;

  const hiveSummaryQuery = useQuery({
    enabled: Boolean(activeChannel?.hiveChannel && activeChannel.isMember),
    queryKey: ["hive-channel-wallet-summary", activeChannel?.id],
    queryFn: () => getHiveChannelWalletSummary(activeChannel?.id ?? ""),
    retry: false,
    staleTime: 30_000,
  });

  const sendHiveFundsMutation = useMutation({
    mutationFn: (amountSats: number) => {
      if (!activeChannel) {
        throw new Error("No active channel.");
      }
      return sendHiveChannelFunds({
        channelId: activeChannel.id,
        amountSats,
      });
    },
    onSuccess: async () => {
      const queryKey = ["hive-channel-wallet-summary", activeChannel?.id];
      await queryClient.invalidateQueries({ queryKey });
      await queryClient.refetchQueries({ queryKey, type: "active" });
      setHiveFundAmount("");
      setIsHiveFundingOpen(false);
    },
  });

  const parsedHiveFundAmount = Number(hiveFundAmount);
  const canSendHiveFunds =
    Number.isSafeInteger(parsedHiveFundAmount) && parsedHiveFundAmount > 0;
  const hiveControls =
    activeChannel?.hiveChannel && activeChannel.isMember ? (
      <Popover onOpenChange={setIsHiveFundingOpen} open={isHiveFundingOpen}>
        <div className="flex items-center gap-1.5">
          <div
            className="rounded-md border border-border/60 bg-muted/45 px-2 py-1 text-xs font-medium text-muted-foreground"
            data-testid="hive-channel-balance"
            title="Hive channel wallet balance"
          >
            {hiveSummaryQuery.data
              ? formatBitcoinAmount(hiveSummaryQuery.data.balanceSats)
              : "Hive"}
          </div>
          <PopoverTrigger asChild>
            <Button
              data-testid="hive-channel-add-funds"
              size="sm"
              type="button"
              variant="outline"
            >
              <Wallet className="mr-1.5 h-3.5 w-3.5" />
              Add funds
            </Button>
          </PopoverTrigger>
        </div>
        <PopoverContent align="end" className="w-64">
          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              if (!canSendHiveFunds) return;
              void sendHiveFundsMutation.mutateAsync(parsedHiveFundAmount);
            }}
          >
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">₿</span>
              <Input
                autoFocus
                data-testid="hive-channel-fund-amount"
                disabled={sendHiveFundsMutation.isPending}
                inputMode="numeric"
                min={1}
                onChange={(event) => {
                  sendHiveFundsMutation.reset();
                  setHiveFundAmount(event.target.value);
                }}
                placeholder="100"
                step={1}
                type="number"
                value={hiveFundAmount}
              />
            </div>
            <Button
              className="w-full"
              data-testid="hive-channel-send-funds"
              disabled={!canSendHiveFunds || sendHiveFundsMutation.isPending}
              size="sm"
              type="submit"
            >
              {sendHiveFundsMutation.isPending ? "Sending..." : "Send"}
            </Button>
            {sendHiveFundsMutation.error instanceof Error ? (
              <p className="text-xs text-destructive">
                {sendHiveFundsMutation.error.message}
              </p>
            ) : null}
          </form>
        </PopoverContent>
      </Popover>
    ) : null;

  const actions = activeChannel ? (
    <div className="flex items-center gap-2">
      {spendBadge}
      {earnedBadge}
      {hiveControls}
      {showJoinButton ? (
        <Button
          disabled={isJoining}
          onClick={() => void onJoinChannel()}
          size="sm"
          variant="default"
        >
          <LogIn className="mr-1.5 h-3.5 w-3.5" />
          {isJoining ? "Joining…" : joinButtonLabel}
        </Button>
      ) : (
        <ChannelMembersBar
          channel={activeChannel}
          currentPubkey={currentPubkey}
          isAddBotOpen={isAddBotOpen}
          onAddBotOpenChange={onAddBotOpenChange}
          onManageChannel={onManageChannel}
          onToggleMembers={onToggleMembers}
          variant={actionsVariant}
        />
      )}
    </div>
  ) : null;

  if (!showHeaderContent) {
    return null;
  }

  return (
    <ChatHeader
      belowSystemChrome
      chromeWrapperRef={chromeWrapperRef}
      density="compact"
      actions={actions}
      channelType={activeChannel?.channelType}
      description={getChannelDescription(activeChannel)}
      leadingContent={
        activeChannel?.channelType === "dm" ? (
          <ProfileAvatar
            avatarUrl={activeDmAvatarUrl}
            className="h-6 w-6 rounded-full text-[10px]"
            iconClassName="h-3.5 w-3.5"
            label={activeChannelTitle}
            testId="chat-header-dm-avatar"
          />
        ) : undefined
      }
      statusBadge={
        <ChannelHeaderStatusBadge
          channelType={activeChannel?.channelType}
          ephemeralDisplay={activeChannelEphemeralDisplay}
          presenceStatus={activeDmPresenceStatus}
        />
      }
      title={activeChannelTitle}
      visibility={activeChannel?.visibility}
    />
  );
}
