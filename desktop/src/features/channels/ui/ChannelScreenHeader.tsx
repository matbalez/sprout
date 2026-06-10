import { LogIn } from "lucide-react";

import { ChatHeader } from "@/features/chat/ui/ChatHeader";
import type { EphemeralChannelDisplay } from "@/features/channels/lib/ephemeralChannel";
import { getChannelDescription } from "@/features/channels/lib/channelDescription";
import { ChannelHeaderStatusBadge } from "@/features/channels/ui/ChannelHeaderStatusBadge";
import { ChannelMembersBar } from "@/features/channels/ui/ChannelMembersBar";
import { getPaidJoinAmount } from "@/features/channels/hooks";
import { ProfileAvatar } from "@/features/profile/ui/ProfileAvatar";
import { formatBitcoinAmount } from "@/features/wallet/api";
import { Button } from "@/shared/ui/button";
import type { Channel, PresenceStatus } from "@/shared/api/types";

type ChannelScreenHeaderProps = {
  activeChannel: Channel | null;
  activeChannelEphemeralDisplay: EphemeralChannelDisplay | null;
  activeChannelTitle: string;
  actionsRightInset?: string;
  actionsVariant?: "inline" | "compact";
  activeDmAvatarUrl: string | null;
  activeDmPresenceStatus: PresenceStatus | null;
  currentPubkey?: string;
  earnedBaseUnits?: number;
  isJoining?: boolean;
  postSpendBaseUnits?: number;
  showHeaderContent?: boolean;
  showEarned?: boolean;
  showPostSpend?: boolean;
  onJoinChannel?: () => Promise<void>;
  onManageChannel: () => void;
  onToggleMembers: () => void;
};

export function ChannelScreenHeader({
  activeChannel,
  activeChannelEphemeralDisplay,
  activeChannelTitle,
  actionsRightInset,
  actionsVariant = "inline",
  activeDmAvatarUrl,
  activeDmPresenceStatus,
  currentPubkey,
  earnedBaseUnits = 0,
  isJoining = false,
  postSpendBaseUnits = 0,
  showHeaderContent = true,
  showEarned = false,
  showPostSpend = false,
  onJoinChannel,
  onManageChannel,
  onToggleMembers,
}: ChannelScreenHeaderProps) {
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

  const actions = activeChannel ? (
    <div className="flex items-center gap-2">
      {spendBadge}
      {earnedBadge}
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
      density="compact"
      actions={actions}
      actionsRightInset={actionsRightInset}
      channelType={activeChannel?.channelType}
      description={getChannelDescription(activeChannel)}
      leadingContent={
        activeChannel?.channelType === "dm" ? (
          <ProfileAvatar
            avatarUrl={activeDmAvatarUrl}
            className="h-6 w-6 rounded-md text-[10px]"
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
