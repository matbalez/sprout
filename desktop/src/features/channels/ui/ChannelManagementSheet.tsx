import {
  Archive,
  ArchiveRestore,
  Copy,
  DoorClosed,
  DoorOpen,
  FileText,
  Hash,
  History,
  Lock,
  MessageSquare,
  RefreshCw,
  Users,
  Wallet,
  Zap,
} from "lucide-react";
import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";

import {
  channelsQueryKey,
  getPaidJoinAmount,
  useArchiveChannelMutation,
  useChannelDetailsQuery,
  useChannelMembersQuery,
  useDeleteChannelMutation,
  useJoinChannelMutation,
  useLeaveChannelMutation,
  useSetChannelPurposeMutation,
  useSetChannelTopicMutation,
  useUnarchiveChannelMutation,
  useUpdateChannelMutation,
} from "@/features/channels/hooks";
import { compareMembersByRole } from "@/features/channels/lib/memberUtils";
import {
  formatTtlDuration,
  parseTtlDuration,
} from "@/features/channels/lib/ephemeralChannel";
import { ChannelBitcoinGiftsCard } from "@/features/klaim-gifts/ui/ChannelBitcoinGiftsCard";
import { CreateWorkflowDialog } from "@/features/workflows/ui/CreateWorkflowDialog";
import {
  formatBitcoinAmount,
  generateHiveChannelWalletBolt12Offer,
  getHiveChannelWalletTransactions,
  getHiveChannelWalletSummary,
  revealHiveChannelWalletSeed,
} from "@/features/wallet/api";
import type { WalletTransaction } from "@/features/wallet/api";
import {
  formatWalletTransactionTitle,
  walletTransactionNotes,
} from "@/features/wallet/transactions";
import type { Channel } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/shared/ui/sheet";
import { Switch } from "@/shared/ui/switch";
import { Textarea } from "@/shared/ui/textarea";
import { ChannelCanvas } from "./ChannelCanvas";

type ChannelManagementSheetProps = {
  channel: Channel | null;
  currentPubkey?: string;
  onDeleted?: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
};

const DEFAULT_EPHEMERAL_TTL_SECONDS = 24 * 60 * 60;

function MetadataPill({
  icon: Icon,
  label,
}: {
  icon: React.ComponentType<{ className?: string }>;
  label: string;
}) {
  return (
    <div className="inline-flex items-center gap-2 rounded-full border border-border/80 bg-muted/40 px-3 py-1 text-xs font-medium text-muted-foreground">
      <Icon className="h-3.5 w-3.5" />
      <span>{label}</span>
    </div>
  );
}

function ChannelIdRow({ channelId }: { channelId: string }) {
  async function handleCopyChannelId() {
    await navigator.clipboard.writeText(channelId);
    toast.success("Copied channel ID to clipboard");
  }

  return (
    <button
      className="group flex w-full items-center gap-3 rounded-xl border border-border/70 bg-muted/20 px-3 py-2.5 text-left transition-colors hover:border-border hover:bg-muted/40 focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-ring"
      data-testid="channel-management-channel-id"
      onClick={() => {
        void handleCopyChannelId();
      }}
      title="Copy channel ID"
      type="button"
    >
      <div className="min-w-0 flex-1 space-y-1">
        <div className="text-xs font-medium uppercase tracking-wide text-muted-foreground/70">
          Channel ID
        </div>
        <div className="truncate font-mono text-xs text-muted-foreground">
          {channelId}
        </div>
      </div>
      <Copy className="h-4 w-4 shrink-0 text-muted-foreground/45 transition-colors group-hover:text-muted-foreground" />
    </button>
  );
}

function HiveWalletOffer({
  isGenerating,
  isLoading,
  onGenerate,
  offer,
}: {
  isGenerating: boolean;
  isLoading: boolean;
  onGenerate: () => void;
  offer: string | null;
}) {
  async function handleCopyOffer() {
    if (!offer) return;
    await navigator.clipboard.writeText(offer);
    toast.success("BOLT12 offer copied");
  }

  return (
    <div className="space-y-2" data-testid="channel-management-hive-offer">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs font-medium text-muted-foreground">
          BOLT12 offer
        </p>
        <div className="flex items-center gap-2">
          <Button
            data-testid="channel-management-generate-hive-offer"
            disabled={isGenerating}
            onClick={onGenerate}
            size="sm"
            type="button"
            variant="outline"
          >
            <RefreshCw
              className={cn("h-4 w-4", isGenerating && "animate-spin")}
            />
            {isGenerating ? "Generating..." : "New offer"}
          </Button>
          <Button
            data-testid="channel-management-copy-hive-offer"
            disabled={!offer}
            onClick={() => {
              void handleCopyOffer();
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            <Copy className="h-4 w-4" />
            Copy
          </Button>
        </div>
      </div>
      {offer ? (
        <>
          <div className="flex justify-center rounded-lg border border-border/70 bg-background/70 px-3 py-4">
            <div className="rounded-lg bg-white p-3 shadow-sm">
              <QRCodeSVG
                bgColor="#ffffff"
                className="h-auto max-w-full"
                fgColor="#000000"
                level="M"
                size={220}
                value={offer}
              />
            </div>
          </div>
          <code
            className="block max-h-24 overflow-y-auto break-all rounded-lg border border-border bg-muted/40 px-3 py-2 text-xs"
            data-testid="channel-management-hive-offer-value"
          >
            {offer}
          </code>
        </>
      ) : (
        <p className="rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          {isLoading ? "Loading BOLT12 offer..." : "BOLT12 offer unavailable"}
        </p>
      )}
    </div>
  );
}

function formatTransactionTimestamp(createdAtMs: number) {
  if (!Number.isFinite(createdAtMs) || createdAtMs <= 0) {
    return null;
  }

  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(createdAtMs));
}

function HiveWalletTransactionRow({ tx }: { tx: WalletTransaction }) {
  const direction = tx.direction.trim().toLowerCase();
  const amountPrefix =
    direction === "inbound" ? "+" : direction === "outbound" ? "-" : "";
  const amount =
    tx.amountSats === null
      ? "amountless"
      : `${amountPrefix}${formatBitcoinAmount(tx.amountSats)}`;
  const timestamp = formatTransactionTimestamp(tx.createdAtMs);
  const status = tx.status.trim() || "unknown";
  const statusMessage = tx.statusMessage.trim();
  const notes = [
    ...walletTransactionNotes(tx),
    statusMessage && statusMessage.toLowerCase() !== status.toLowerCase()
      ? statusMessage
      : null,
  ].filter((note): note is string => Boolean(note));

  return (
    <li className="space-y-1 border-t border-border/60 py-2 first:border-t-0 first:pt-0 last:pb-0">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-0.5">
          <div className="truncate text-sm font-medium">
            {formatWalletTransactionTitle(tx)}
          </div>
          <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
            {timestamp ? <span>{timestamp}</span> : null}
            <span className="rounded border border-border/60 px-1.5 py-0.5">
              {status}
            </span>
          </div>
        </div>
        <div
          className={cn(
            "shrink-0 text-right text-sm font-medium",
            direction === "inbound"
              ? "text-emerald-600 dark:text-emerald-400"
              : "text-muted-foreground",
          )}
        >
          {amount}
        </div>
      </div>
      {notes.length ? (
        <div className="space-y-0.5">
          {notes.map((note) => (
            <p className="break-words text-xs text-muted-foreground" key={note}>
              {note}
            </p>
          ))}
        </div>
      ) : null}
    </li>
  );
}

function HiveWalletTransactions({
  isLoading,
  transactions,
}: {
  isLoading: boolean;
  transactions: WalletTransaction[] | undefined;
}) {
  const records = transactions ?? [];

  return (
    <div
      className="space-y-2"
      data-testid="channel-management-hive-transactions"
    >
      <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
        <History className="h-3.5 w-3.5" />
        Transaction history
      </div>
      {records.length ? (
        <ul className="rounded-lg border border-border/70 px-3 py-2">
          {records.map((tx) => (
            <HiveWalletTransactionRow key={tx.id} tx={tx} />
          ))}
        </ul>
      ) : (
        <p className="rounded-lg border border-border/70 bg-muted/30 px-3 py-2 text-xs text-muted-foreground">
          {isLoading ? "Loading transactions..." : "No transactions yet"}
        </p>
      )}
    </div>
  );
}

export function ChannelManagementSheet({
  channel,
  currentPubkey,
  onDeleted,
  onOpenChange,
  open,
}: ChannelManagementSheetProps) {
  const { isDark } = useTheme();
  const queryClient = useQueryClient();
  const channelId = channel?.id ?? null;
  const detailsQuery = useChannelDetailsQuery(channelId, open);
  const membersQuery = useChannelMembersQuery(channelId, open);
  const updateChannelDetailsMutation = useUpdateChannelMutation(channelId);
  const updateChannelLifecycleMutation = useUpdateChannelMutation(channelId);
  const setTopicMutation = useSetChannelTopicMutation(channelId);
  const setPurposeMutation = useSetChannelPurposeMutation(channelId);
  const archiveChannelMutation = useArchiveChannelMutation(channelId);
  const unarchiveChannelMutation = useUnarchiveChannelMutation(channelId);
  const deleteChannelMutation = useDeleteChannelMutation(channelId);
  const joinChannelMutation = useJoinChannelMutation(channel);
  const leaveChannelMutation = useLeaveChannelMutation(channelId);

  const detail = detailsQuery.data ?? channel;
  const members = React.useMemo(() => {
    const currentMembers = membersQuery.data ?? [];
    return [...currentMembers].sort((left, right) =>
      compareMembersByRole(left, right, currentPubkey),
    );
  }, [currentPubkey, membersQuery.data]);
  const selfMember =
    members.find((member) => member.pubkey === currentPubkey) ?? null;
  const hasResolvedMembership = membersQuery.data !== undefined;
  const isOwner = selfMember?.role === "owner";
  const canManageChannel =
    selfMember?.role === "owner" || selfMember?.role === "admin";
  const canEditNarrative = selfMember !== null && detail?.channelType !== "dm";
  const isArchived =
    detail?.archivedAt !== null && detail?.archivedAt !== undefined;
  const canJoin =
    hasResolvedMembership &&
    detail?.channelType !== "dm" &&
    detail?.visibility === "open" &&
    !isArchived &&
    selfMember === null;
  const canLeave =
    hasResolvedMembership &&
    detail?.channelType !== "dm" &&
    !isArchived &&
    selfMember !== null;
  const paidJoinAmountBaseUnits = getPaidJoinAmount(detail);
  const joinButtonLabel =
    paidJoinAmountBaseUnits !== null
      ? `Pay ${formatBitcoinAmount(paidJoinAmountBaseUnits)} to join`
      : "Join channel";
  const memberCount =
    members.length || detail?.memberCount || channel?.memberCount || 0;

  const [nameDraft, setNameDraft] = React.useState("");
  const [descriptionDraft, setDescriptionDraft] = React.useState("");
  const [topicDraft, setTopicDraft] = React.useState("");
  const [purposeDraft, setPurposeDraft] = React.useState("");
  const [isPrivateDraft, setIsPrivateDraft] = React.useState(false);
  const [isEphemeralDraft, setIsEphemeralDraft] = React.useState(false);
  const [ttlDraft, setTtlDraft] = React.useState("");
  const [isDeleteDialogOpen, setIsDeleteDialogOpen] = React.useState(false);
  const [isCreateWorkflowOpen, setIsCreateWorkflowOpen] = React.useState(false);
  const [hiveSeed, setHiveSeed] = React.useState<string | null>(null);
  const [hiveSeedError, setHiveSeedError] = React.useState<string | null>(null);
  const [isRevealingHiveSeed, setIsRevealingHiveSeed] = React.useState(false);
  const hiveSummaryQueryKey = ["hive-channel-wallet-summary", detail?.id];
  const hiveSummaryQuery = useQuery({
    enabled: Boolean(open && detail?.hiveChannel),
    queryKey: hiveSummaryQueryKey,
    queryFn: () => getHiveChannelWalletSummary(detail?.id ?? ""),
    retry: false,
    staleTime: 30_000,
  });
  const hiveTransactionsQuery = useQuery({
    enabled: Boolean(open && detail?.hiveChannel),
    queryKey: ["hive-channel-wallet-transactions", detail?.id],
    queryFn: () => getHiveChannelWalletTransactions(detail?.id ?? "", 20),
    retry: false,
    staleTime: 30_000,
  });
  const generateHiveOfferMutation = useMutation({
    mutationFn: () => {
      const selectedChannel = detail ?? channel;
      if (!selectedChannel?.hiveChannel) {
        throw new Error("No hive channel selected.");
      }
      return generateHiveChannelWalletBolt12Offer(selectedChannel.id);
    },
    onSuccess: async (summary) => {
      queryClient.setQueryData(
        ["hive-channel-wallet-summary", summary.channelId],
        summary,
      );
      toast.success("Generated new BOLT12 offer");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: channelsQueryKey }),
        queryClient.invalidateQueries({
          queryKey: ["channels", summary.channelId, "detail"],
        }),
      ]);
    },
  });

  // Sync drafts from server only when the sheet opens or the channel changes —
  // not on every background refetch, which would clobber in-flight edits.
  const syncedForRef = React.useRef<string | null>(null);
  React.useEffect(() => {
    if (!open) {
      // Reset on close so the next open re-syncs from server.
      syncedForRef.current = null;
      setIsDeleteDialogOpen(false);
      setIsCreateWorkflowOpen(false);
      setHiveSeed(null);
      setHiveSeedError(null);
      return;
    }
    if (!detail) {
      return;
    }

    const key = detail.id;
    if (syncedForRef.current === key) {
      return;
    }
    syncedForRef.current = key;

    setNameDraft(detail.name);
    setDescriptionDraft(detail.description);
    setTopicDraft(detail.topic ?? "");
    setPurposeDraft(detail.purpose ?? "");
    setIsPrivateDraft(detail.visibility === "private");
    setIsEphemeralDraft(detail.ttlSeconds !== null);
    setTtlDraft(
      detail.ttlSeconds !== null ? formatTtlDuration(detail.ttlSeconds) : "",
    );
  }, [detail, open]);

  if (!channel) {
    return null;
  }

  function handleDeleteDialogOpenChange(next: boolean) {
    deleteChannelMutation.reset();
    setIsDeleteDialogOpen(next);
  }

  async function handleDeleteChannel() {
    try {
      await deleteChannelMutation.mutateAsync();
      handleDeleteDialogOpenChange(false);
      onOpenChange(false);
      onDeleted?.();
    } catch {
      // The mutation error is rendered inline in the confirmation dialog.
    }
  }

  function handleSheetOpenChange(next: boolean) {
    if (!next) {
      handleDeleteDialogOpenChange(false);
    }

    onOpenChange(next);
  }

  // Parsed seconds for the ephemeral TTL field. `null` when the field is empty
  // or malformed; the form blocks saving on a non-empty malformed value.
  const parsedTtlSeconds = parseTtlDuration(ttlDraft);
  const ttlInvalid =
    isEphemeralDraft && ttlDraft.trim() !== "" && parsedTtlSeconds === null;

  const currentVisibility = detail?.visibility ?? channel.visibility;
  const currentTtlSeconds = detail?.ttlSeconds ?? null;
  const nextVisibility: "open" | "private" = isPrivateDraft
    ? "private"
    : "open";
  const nextTtlSeconds: number | null = isEphemeralDraft
    ? (parsedTtlSeconds ?? DEFAULT_EPHEMERAL_TTL_SECONDS)
    : null;
  const lifecycleDirty =
    nextVisibility !== currentVisibility ||
    nextTtlSeconds !== currentTtlSeconds;

  function handleSaveLifecycle() {
    void updateChannelLifecycleMutation.mutateAsync({
      visibility:
        nextVisibility !== currentVisibility ? nextVisibility : undefined,
      ttlSeconds:
        nextTtlSeconds !== currentTtlSeconds ? nextTtlSeconds : undefined,
    });
  }

  async function handleRevealHiveSeed() {
    if (!resolvedChannel.hiveChannel) return;
    setIsRevealingHiveSeed(true);
    setHiveSeedError(null);
    try {
      setHiveSeed(await revealHiveChannelWalletSeed(resolvedChannel.id));
    } catch (error) {
      setHiveSeedError(
        error instanceof Error
          ? error.message
          : "Failed to reveal hive wallet seed.",
      );
    } finally {
      setIsRevealingHiveSeed(false);
    }
  }

  const resolvedChannel = detail ?? channel;
  const hiveWalletBolt12Offer =
    hiveSummaryQuery.data?.bolt12Offer.trim() ||
    resolvedChannel.hiveWalletBolt12Offer ||
    null;

  return (
    <Sheet onOpenChange={handleSheetOpenChange} open={open}>
      <SheetContent
        className={cn(
          "flex w-full flex-col gap-0 overflow-hidden border-l border-border/80 p-0 shadow-none sm:max-w-xl",
          isDark
            ? "bg-background/85 backdrop-blur-xl supports-[backdrop-filter]:bg-background/75"
            : "bg-background",
        )}
        data-testid="channel-management-sheet"
        side="right"
      >
        <SheetHeader
          className={cn(
            "relative z-10 space-y-4 px-6 py-6 text-left shadow-none",
            isDark
              ? "bg-background/60 backdrop-blur-xl supports-[backdrop-filter]:bg-background/50"
              : "bg-background",
          )}
        >
          <SheetTitle className="pr-8">{channel.name}</SheetTitle>
          <SheetDescription className="sr-only">
            Channel settings
          </SheetDescription>
          <div className="flex flex-wrap items-center gap-2">
            <MetadataPill
              icon={
                channel.channelType === "forum"
                  ? FileText
                  : channel.channelType === "dm"
                    ? MessageSquare
                    : Hash
              }
              label={channel.channelType}
            />
            <MetadataPill
              icon={channel.visibility === "private" ? Lock : DoorOpen}
              label={channel.visibility}
            />
            <MetadataPill icon={Users} label={`${memberCount} members`} />
            {isArchived ? (
              <MetadataPill icon={Archive} label="archived" />
            ) : null}
          </div>
        </SheetHeader>

        <div className="flex-1 space-y-6 overflow-y-auto bg-background px-6 py-6">
          <ChannelIdRow channelId={resolvedChannel.id} />
          {resolvedChannel.hiveChannel ? (
            <div
              className="space-y-3 rounded-xl border border-border/70 bg-muted/20 p-3"
              data-testid="channel-management-hive-wallet"
            >
              <div className="flex items-center justify-between gap-3">
                <div className="flex items-center gap-2 text-sm font-medium">
                  <Wallet className="h-4 w-4 text-muted-foreground" />
                  Hive wallet
                </div>
                <div className="rounded-md border border-border/60 bg-background px-2 py-1 text-xs font-medium text-muted-foreground">
                  {hiveSummaryQuery.data
                    ? formatBitcoinAmount(hiveSummaryQuery.data.balanceSats)
                    : "Local seed required"}
                </div>
              </div>
              {hiveSummaryQuery.data?.ownershipShares.length ? (
                <div className="space-y-1">
                  {hiveSummaryQuery.data.ownershipShares.map((share) => (
                    <div
                      className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
                      key={share.memberPubkey ?? "unknown"}
                    >
                      <span className="truncate font-mono">
                        {share.memberPubkey ?? "unknown"}
                      </span>
                      <span>
                        {share.ownershipPercent.toFixed(1)}% ·{" "}
                        {formatBitcoinAmount(share.amountSats)}
                      </span>
                    </div>
                  ))}
                </div>
              ) : null}
              <HiveWalletOffer
                isGenerating={generateHiveOfferMutation.isPending}
                isLoading={hiveSummaryQuery.isPending}
                onGenerate={() => {
                  void generateHiveOfferMutation.mutateAsync();
                }}
                offer={hiveWalletBolt12Offer}
              />
              <HiveWalletTransactions
                isLoading={hiveTransactionsQuery.isPending}
                transactions={hiveTransactionsQuery.data}
              />
              {hiveTransactionsQuery.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {hiveTransactionsQuery.error.message}
                </p>
              ) : null}
              {generateHiveOfferMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {generateHiveOfferMutation.error.message}
                </p>
              ) : null}
              <Button
                data-testid="channel-management-reveal-hive-seed"
                disabled={isRevealingHiveSeed}
                onClick={() => {
                  void handleRevealHiveSeed();
                }}
                size="sm"
                type="button"
                variant="outline"
              >
                {isRevealingHiveSeed ? "Revealing..." : "Reveal seed phrase"}
              </Button>
              {hiveSeed ? (
                <Textarea
                  className="min-h-20 font-mono text-xs"
                  data-testid="channel-management-hive-seed"
                  readOnly
                  value={hiveSeed}
                />
              ) : null}
              {hiveSeedError ? (
                <p className="text-sm text-destructive">{hiveSeedError}</p>
              ) : null}
            </div>
          ) : null}
          {detailsQuery.error instanceof Error ? (
            <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {detailsQuery.error.message}
            </p>
          ) : null}

          {membersQuery.error instanceof Error ? (
            <p className="rounded-xl border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {membersQuery.error.message}
            </p>
          ) : null}

          {resolvedChannel.channelType !== "dm" &&
          resolvedChannel.visibility === "private" ? (
            <ChannelBitcoinGiftsCard
              canManage={canManageChannel}
              channel={resolvedChannel}
              memberPubkeys={members.map((member) => member.pubkey)}
            />
          ) : null}

          {canJoin ? (
            <div className="space-y-3">
              <Button
                data-testid="channel-management-join"
                disabled={joinChannelMutation.isPending}
                onClick={() => {
                  void joinChannelMutation.mutateAsync();
                }}
                size="sm"
                type="button"
              >
                <DoorOpen className="h-4 w-4" />
                {joinChannelMutation.isPending ? "Joining..." : joinButtonLabel}
              </Button>
              {joinChannelMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {joinChannelMutation.error.message}
                </p>
              ) : null}
            </div>
          ) : null}

          <div data-testid="channel-canvas-section">
            <ChannelCanvas
              canEdit={canEditNarrative}
              channelId={channelId}
              isArchived={isArchived}
            />
          </div>

          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              void updateChannelDetailsMutation.mutateAsync({
                description: descriptionDraft.trim() || undefined,
                name: nameDraft.trim() || undefined,
              });
            }}
          >
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="channel-name">
                Name
              </label>
              <Input
                data-testid="channel-management-name"
                disabled={
                  !canManageChannel || updateChannelDetailsMutation.isPending
                }
                id="channel-name"
                onChange={(event) => setNameDraft(event.target.value)}
                value={nameDraft}
              />
            </div>
            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="channel-description"
              >
                Description
              </label>
              <Textarea
                className="min-h-24"
                data-testid="channel-management-description"
                disabled={
                  !canManageChannel || updateChannelDetailsMutation.isPending
                }
                id="channel-description"
                onChange={(event) => setDescriptionDraft(event.target.value)}
                value={descriptionDraft}
              />
            </div>
            <Button
              data-testid="channel-management-save-details"
              disabled={
                !canManageChannel || updateChannelDetailsMutation.isPending
              }
              size="sm"
              type="submit"
            >
              {updateChannelDetailsMutation.isPending
                ? "Saving..."
                : "Save details"}
            </Button>
            {updateChannelDetailsMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {updateChannelDetailsMutation.error.message}
              </p>
            ) : null}
          </form>

          {resolvedChannel.channelType !== "dm" ? (
            <div
              className="space-y-4"
              data-testid="channel-management-lifecycle"
            >
              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <p className="text-sm font-medium">Private</p>
                  <p className="text-xs text-muted-foreground">
                    Only members can find and join this channel.
                  </p>
                </div>
                <Switch
                  checked={isPrivateDraft}
                  data-testid="channel-management-private-toggle"
                  disabled={
                    !canManageChannel ||
                    updateChannelLifecycleMutation.isPending
                  }
                  onCheckedChange={setIsPrivateDraft}
                />
              </div>

              <div className="flex items-center justify-between gap-4">
                <div className="space-y-0.5">
                  <p className="text-sm font-medium">Ephemeral</p>
                  <p className="text-xs text-muted-foreground">
                    Automatically delete this channel after a set time.
                  </p>
                </div>
                <Switch
                  checked={isEphemeralDraft}
                  data-testid="channel-management-ephemeral-toggle"
                  disabled={
                    !canManageChannel ||
                    updateChannelLifecycleMutation.isPending
                  }
                  onCheckedChange={setIsEphemeralDraft}
                />
              </div>

              {isEphemeralDraft ? (
                <div className="space-y-1.5">
                  <label className="text-sm font-medium" htmlFor="channel-ttl">
                    Timeout
                  </label>
                  <Input
                    aria-invalid={ttlInvalid}
                    data-testid="channel-management-ttl"
                    disabled={
                      !canManageChannel ||
                      updateChannelLifecycleMutation.isPending
                    }
                    id="channel-ttl"
                    onChange={(event) => setTtlDraft(event.target.value)}
                    placeholder="e.g. 1d, 12h, 30m"
                    value={ttlDraft}
                  />
                  <p
                    className={cn(
                      "text-xs",
                      ttlInvalid ? "text-destructive" : "text-muted-foreground",
                    )}
                  >
                    {ttlInvalid
                      ? "Enter a duration like 1d, 12h, or 30m."
                      : "Defaults to 1d when left empty. Resets the deletion countdown from now whenever changed."}
                  </p>
                </div>
              ) : null}

              <Button
                data-testid="channel-management-save-lifecycle"
                disabled={
                  !canManageChannel ||
                  updateChannelLifecycleMutation.isPending ||
                  ttlInvalid ||
                  !lifecycleDirty
                }
                onClick={handleSaveLifecycle}
                size="sm"
                type="button"
              >
                {updateChannelLifecycleMutation.isPending
                  ? "Saving..."
                  : "Save visibility"}
              </Button>
            </div>
          ) : null}

          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              void setTopicMutation.mutateAsync({
                topic: topicDraft.trim(),
              });
            }}
          >
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="channel-topic">
                Topic
              </label>
              <Input
                data-testid="channel-management-topic"
                disabled={!canEditNarrative || setTopicMutation.isPending}
                id="channel-topic"
                onChange={(event) => setTopicDraft(event.target.value)}
                value={topicDraft}
              />
            </div>
            <Button
              data-testid="channel-management-save-topic"
              disabled={!canEditNarrative || setTopicMutation.isPending}
              size="sm"
              type="submit"
              variant="outline"
            >
              {setTopicMutation.isPending ? "Saving..." : "Save topic"}
            </Button>
            {setTopicMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {setTopicMutation.error.message}
              </p>
            ) : null}
          </form>

          <form
            className="space-y-3"
            onSubmit={(event) => {
              event.preventDefault();
              void setPurposeMutation.mutateAsync({
                purpose: purposeDraft.trim(),
              });
            }}
          >
            <div className="space-y-1.5">
              <label className="text-sm font-medium" htmlFor="channel-purpose">
                Purpose
              </label>
              <Input
                data-testid="channel-management-purpose"
                disabled={!canEditNarrative || setPurposeMutation.isPending}
                id="channel-purpose"
                onChange={(event) => setPurposeDraft(event.target.value)}
                value={purposeDraft}
              />
            </div>
            <Button
              data-testid="channel-management-save-purpose"
              disabled={!canEditNarrative || setPurposeMutation.isPending}
              size="sm"
              type="submit"
              variant="outline"
            >
              {setPurposeMutation.isPending ? "Saving..." : "Save purpose"}
            </Button>
            {setPurposeMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {setPurposeMutation.error.message}
              </p>
            ) : null}
          </form>

          {canEditNarrative ? (
            <Button
              data-testid="channel-management-create-workflow"
              onClick={() => setIsCreateWorkflowOpen(true)}
              size="sm"
              type="button"
              variant="outline"
            >
              <Zap className="h-4 w-4" />
              Create workflow
            </Button>
          ) : null}
        </div>

        {resolvedChannel.channelType !== "dm" ? (
          <SheetFooter
            className={cn(
              "border-t border-border/80 px-6 py-4 sm:flex-row sm:justify-start sm:space-x-0",
              isDark
                ? "bg-background/60 backdrop-blur-xl supports-[backdrop-filter]:bg-background/50"
                : "bg-background",
            )}
            data-testid="channel-management-footer"
          >
            <div className="w-full space-y-3">
              <div className="flex items-center gap-2">
                {canLeave ? (
                  <Button
                    data-testid="channel-management-leave"
                    disabled={leaveChannelMutation.isPending}
                    onClick={() => {
                      void leaveChannelMutation.mutateAsync().then(() => {
                        onOpenChange(false);
                      });
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <DoorClosed className="h-4 w-4" />
                    {leaveChannelMutation.isPending ? "Leaving..." : "Leave"}
                  </Button>
                ) : null}
                {isArchived ? (
                  <Button
                    data-testid="channel-management-unarchive"
                    disabled={
                      !canManageChannel || unarchiveChannelMutation.isPending
                    }
                    onClick={() => {
                      void unarchiveChannelMutation.mutateAsync();
                    }}
                    size="sm"
                    type="button"
                  >
                    <ArchiveRestore className="h-4 w-4" />
                    {unarchiveChannelMutation.isPending
                      ? "Restoring..."
                      : "Unarchive"}
                  </Button>
                ) : (
                  <Button
                    data-testid="channel-management-archive"
                    disabled={
                      !canManageChannel || archiveChannelMutation.isPending
                    }
                    onClick={() => {
                      void archiveChannelMutation.mutateAsync();
                    }}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <Archive className="h-4 w-4" />
                    {archiveChannelMutation.isPending
                      ? "Archiving..."
                      : "Archive"}
                  </Button>
                )}
                <div className="flex-1" />
                {isOwner ? (
                  <AlertDialog
                    onOpenChange={handleDeleteDialogOpenChange}
                    open={isDeleteDialogOpen}
                  >
                    <AlertDialogTrigger asChild>
                      <Button
                        data-testid="channel-management-delete"
                        disabled={deleteChannelMutation.isPending}
                        size="sm"
                        type="button"
                        variant="destructive"
                      >
                        Delete
                      </Button>
                    </AlertDialogTrigger>
                    <AlertDialogContent data-testid="channel-delete-confirmation-dialog">
                      <AlertDialogHeader>
                        <AlertDialogTitle>Delete channel?</AlertDialogTitle>
                        <AlertDialogDescription>
                          Delete {resolvedChannel.name} from the workspace list.
                          This action cannot be undone.
                        </AlertDialogDescription>
                      </AlertDialogHeader>
                      {deleteChannelMutation.error instanceof Error ? (
                        <p className="text-sm text-destructive">
                          {deleteChannelMutation.error.message}
                        </p>
                      ) : null}
                      <AlertDialogFooter>
                        <AlertDialogCancel asChild>
                          <Button
                            data-testid="channel-delete-cancel"
                            disabled={deleteChannelMutation.isPending}
                            type="button"
                            variant="outline"
                          >
                            Cancel
                          </Button>
                        </AlertDialogCancel>
                        <AlertDialogAction asChild>
                          <Button
                            data-testid="channel-delete-confirm"
                            disabled={deleteChannelMutation.isPending}
                            onClick={(event) => {
                              event.preventDefault();
                              void handleDeleteChannel();
                            }}
                            type="button"
                            variant="destructive"
                          >
                            {deleteChannelMutation.isPending
                              ? "Deleting..."
                              : "Delete channel"}
                          </Button>
                        </AlertDialogAction>
                      </AlertDialogFooter>
                    </AlertDialogContent>
                  </AlertDialog>
                ) : null}
              </div>
              {leaveChannelMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {leaveChannelMutation.error.message}
                </p>
              ) : null}
              {archiveChannelMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {archiveChannelMutation.error.message}
                </p>
              ) : null}
              {unarchiveChannelMutation.error instanceof Error ? (
                <p className="text-sm text-destructive">
                  {unarchiveChannelMutation.error.message}
                </p>
              ) : null}
            </div>
          </SheetFooter>
        ) : null}
      </SheetContent>

      <CreateWorkflowDialog
        channels={[channel]}
        onOpenChange={setIsCreateWorkflowOpen}
        open={isCreateWorkflowOpen}
      />
    </Sheet>
  );
}
