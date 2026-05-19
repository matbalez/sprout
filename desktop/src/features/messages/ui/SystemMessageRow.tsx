import { SmilePlus } from "lucide-react";
import Picker from "@emoji-mart/react";
import data from "@emoji-mart/data";
import * as React from "react";

import type { TimelineMessage } from "@/features/messages/types";
import {
  type SystemMessagePayload,
  parseSystemMessagePayload,
} from "@/features/messages/lib/describeSystemEvent";
import { iconForSystemEvent } from "@/features/messages/lib/systemEventIcons";
import { MessageReactions } from "@/features/messages/ui/MessageReactions";
import { useReactionHandler } from "@/features/messages/ui/useReactionHandler";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import { resolveUserLabel } from "@/features/profile/lib/identity";
import { cn } from "@/shared/lib/cn";
import { Button } from "@/shared/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { Spinner } from "@/shared/ui/spinner";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/tooltip";
import { UserAvatar } from "@/shared/ui/UserAvatar";
import { MessageTimestamp } from "./MessageTimestamp";

type SystemMessageDescription = {
  /** Single-line description of the event. */
  text: React.ReactNode;
  /** Pubkey whose avatar to show (target for add/remove, actor otherwise). */
  avatarPubkey: string | undefined;
};

function resolveLabel(
  pubkey: string | undefined,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string {
  if (!pubkey) {
    return "Someone";
  }
  return resolveUserLabel({ pubkey, currentPubkey, profiles });
}

function resolvePersonaSuffix(
  pubkey: string | undefined,
  personaLookup: Map<string, string> | undefined,
): string {
  if (!pubkey || !personaLookup) return "";
  const personaName = personaLookup.get(pubkey.toLowerCase());
  return personaName ? ` (${personaName})` : "";
}

function resolveAvatarUrl(
  pubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
): string | null {
  if (!pubkey || !profiles) return null;
  return profiles[pubkey.toLowerCase()]?.avatarUrl ?? null;
}

function labelWithSuffix(
  pubkey: string | undefined,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
  suffix = "",
): string {
  return `${resolveLabel(pubkey, currentPubkey, profiles)}${suffix}`;
}

function describeSystemEvent(
  payload: SystemMessagePayload,
  currentPubkey: string | undefined,
  profiles: UserProfileLookup | undefined,
  personaLookup?: Map<string, string>,
): SystemMessageDescription | null {
  const personaSuffix = resolvePersonaSuffix(payload.target, personaLookup);
  const actorLabel = labelWithSuffix(payload.actor, currentPubkey, profiles);
  const targetLabel = labelWithSuffix(
    payload.target,
    currentPubkey,
    profiles,
    personaSuffix,
  );

  switch (payload.type) {
    case "member_joined": {
      if (payload.actor === payload.target) {
        return {
          avatarPubkey: payload.target,
          text: (
            <>
              <span className="font-medium">{targetLabel}</span> joined the
              channel
            </>
          ),
        };
      }
      return {
        avatarPubkey: payload.target,
        text: (
          <>
            <span className="font-medium">{targetLabel}</span> was added by{" "}
            {actorLabel}
          </>
        ),
      };
    }
    case "member_left":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> left the channel
          </>
        ),
      };
    case "member_removed":
      return {
        avatarPubkey: payload.target,
        text: (
          <>
            <span className="font-medium">{targetLabel}</span> was removed by{" "}
            {actorLabel}
          </>
        ),
      };
    case "topic_changed":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> changed the topic
            to &ldquo;{payload.topic}&rdquo;
          </>
        ),
      };
    case "purpose_changed":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> changed the
            purpose to &ldquo;{payload.purpose}&rdquo;
          </>
        ),
      };
    case "channel_created":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> created this
            channel
          </>
        ),
      };
    case "channel_archived":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> archived this
            channel
          </>
        ),
      };
    case "channel_unarchived":
      return {
        avatarPubkey: payload.actor,
        text: (
          <>
            <span className="font-medium">{actorLabel}</span> unarchived this
            channel
          </>
        ),
      };
    default:
      return null;
  }
}

export const SystemMessageRow = React.memo(function SystemMessageRow({
  message,
  currentPubkey,
  profiles,
  personaLookup,
  onToggleReaction,
}: {
  message: TimelineMessage;
  currentPubkey?: string;
  profiles?: UserProfileLookup;
  /** Map from lowercase pubkey → persona display name for bot members. */
  personaLookup?: Map<string, string>;
  onToggleReaction?: (
    message: TimelineMessage,
    emoji: string,
    remove: boolean,
  ) => Promise<void>;
}) {
  const [isReactionPickerOpen, setIsReactionPickerOpen] = React.useState(false);
  const {
    reactions,
    canToggle: canToggleReactions,
    pending: reactionPending,
    errorMessage: reactionErrorMessage,
    select: handleReactionSelect,
  } = useReactionHandler(message, onToggleReaction);

  const payload = parseSystemMessagePayload(message.body);
  if (!payload) {
    return null;
  }

  const description = describeSystemEvent(
    payload,
    currentPubkey,
    profiles,
    personaLookup,
  );
  if (!description) {
    return null;
  }

  const Icon = iconForSystemEvent(payload.type);

  const avatarLabel = description.avatarPubkey
    ? resolveUserLabel({
        pubkey: description.avatarPubkey,
        currentPubkey,
        profiles,
        preferResolvedSelfLabel: true,
      })
    : "Someone";

  return (
    <div
      className="group/message relative rounded-lg px-2 py-0.5 transition-colors"
      data-testid="system-message-row"
    >
      <div className="flex items-center gap-2">
        <UserAvatar
          avatarUrl={resolveAvatarUrl(description.avatarPubkey, profiles)}
          className="!h-5 !w-5 shrink-0 text-[8px]"
          displayName={avatarLabel}
          testId="system-message-avatar"
        />
        <Icon className="h-3 w-3 shrink-0 text-muted-foreground" />
        <p className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          {description.text}
        </p>
        <span className="shrink-0 text-[10px] text-muted-foreground/50">
          <MessageTimestamp createdAt={message.createdAt} time={message.time} />
        </span>
        {canToggleReactions ? (
          <div
            className={cn(
              "shrink-0 overflow-hidden rounded-full border border-border/70 bg-background/95 shadow-sm backdrop-blur supports-[backdrop-filter]:bg-background/85 transition-all duration-150 ease-out",
              "max-w-0 border-0 shadow-none opacity-0",
              "group-hover/message:max-w-7 group-hover/message:border group-hover/message:border-border/70 group-hover/message:shadow-sm group-hover/message:opacity-100",
              "group-focus-within/message:max-w-7 group-focus-within/message:border group-focus-within/message:border-border/70 group-focus-within/message:shadow-sm group-focus-within/message:opacity-100",
              isReactionPickerOpen
                ? "max-w-7 border border-border/70 shadow-sm opacity-100"
                : "",
            )}
          >
            <Popover
              onOpenChange={setIsReactionPickerOpen}
              open={isReactionPickerOpen}
            >
              <Tooltip>
                <TooltipTrigger asChild>
                  <PopoverTrigger asChild>
                    <Button
                      aria-label="Open reactions"
                      className="h-5 w-5 rounded-full p-0"
                      disabled={reactionPending}
                      size="sm"
                      type="button"
                      variant={isReactionPickerOpen ? "secondary" : "ghost"}
                    >
                      {reactionPending ? (
                        <Spinner className="h-2.5 w-2.5" />
                      ) : (
                        <SmilePlus className="h-2.5 w-2.5" />
                      )}
                    </Button>
                  </PopoverTrigger>
                </TooltipTrigger>
                <TooltipContent>React</TooltipContent>
              </Tooltip>
              <PopoverContent
                align="end"
                className="w-auto p-0 rounded-2xl overflow-hidden border-0 bg-transparent shadow-none"
                side="top"
                sideOffset={10}
              >
                {reactionErrorMessage ? (
                  <div className="px-3 pt-3 pb-0">
                    <p className="text-xs text-destructive">
                      {reactionErrorMessage}
                    </p>
                  </div>
                ) : null}
                <Picker
                  data={data}
                  onEmojiSelect={(emoji: { native: string }) => {
                    void handleReactionSelect(emoji.native).finally(() => {
                      setIsReactionPickerOpen(false);
                    });
                  }}
                  theme="auto"
                  previewPosition="none"
                  skinTonePosition="search"
                  set="native"
                  maxFrequentRows={2}
                  perLine={8}
                />
              </PopoverContent>
            </Popover>
          </div>
        ) : null}
      </div>
      {reactions.length > 0 ? (
        <div className="ml-7 mt-0.5">
          <MessageReactions
            messageId={message.id}
            reactions={reactions}
            canToggle={canToggleReactions}
            pending={reactionPending}
            onSelect={(emoji) => {
              void handleReactionSelect(emoji);
            }}
          />
          {reactionErrorMessage ? (
            <p className="mt-1 text-xs text-destructive">
              {reactionErrorMessage}
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
});
