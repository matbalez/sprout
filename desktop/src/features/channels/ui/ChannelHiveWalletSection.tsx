import { Copy, History, RefreshCw, Send, Wallet } from "lucide-react";
import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";

import { channelsQueryKey } from "@/features/channels/hooks";
import { truncatePubkey } from "@/features/profile/lib/identity";
import {
  executeHiveChannelWalletPayouts,
  formatBitcoinAmount,
  generateHiveChannelWalletBolt12Offer,
  getHiveChannelWalletSummary,
  getHiveChannelWalletTransactions,
  previewHiveChannelWalletPayouts,
  revealHiveChannelWalletSeed,
  sendHiveChannelWalletPayment,
} from "@/features/wallet/api";
import type {
  HiveChannelPayoutExecution,
  HiveChannelPayoutPreview,
  WalletTransaction,
} from "@/features/wallet/api";
import {
  formatWalletTransactionTitle,
  walletTransactionNotes,
} from "@/features/wallet/transactions";
import type { Channel, ChannelMember } from "@/shared/api/types";
import { cn } from "@/shared/lib/cn";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/shared/ui/alert-dialog";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";

type ChannelHiveWalletSectionProps = {
  canManageChannel: boolean;
  channel: Channel;
  currentPubkey?: string;
  members: readonly ChannelMember[];
  open: boolean;
};

function resolveHiveShareMemberLabel(
  memberPubkey: string | null,
  members: readonly ChannelMember[],
  currentPubkey: string | undefined,
) {
  if (!memberPubkey) {
    return "unknown";
  }

  const normalizedPubkey = memberPubkey.toLowerCase();
  const member = members.find(
    (candidate) => candidate.pubkey.toLowerCase() === normalizedPubkey,
  );
  const displayName = member?.displayName?.trim();
  if (displayName) {
    return displayName;
  }

  if (currentPubkey?.toLowerCase() === normalizedPubkey) {
    return "You";
  }

  return truncatePubkey(memberPubkey);
}

function HiveWalletOffer({
  canGenerate,
  isGenerating,
  isLoading,
  onGenerate,
  offer,
}: {
  canGenerate: boolean;
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
          {canGenerate ? (
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
          ) : null}
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

function parseWholeBitcoinAmount(value: string) {
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) {
    return null;
  }
  const amount = Number(trimmed);
  if (!Number.isSafeInteger(amount) || amount <= 0) {
    return null;
  }
  return amount;
}

function hivePayoutEmptyMessage(preview: HiveChannelPayoutPreview) {
  if (preview.totalUnattributedRevenueSats === 0) {
    return "No unattributed revenue to pay out.";
  }
  if (preview.skippedNoOwnerRevenueCount > 0) {
    return "No payable revenue shares. Some revenue landed before any ownership stake existed.";
  }
  return "No unpaid revenue shares to pay out.";
}

function HivePayoutConfirmationDialog({
  currentPubkey,
  execution,
  isExecuting,
  members,
  onConfirm,
  onOpenChange,
  open,
  preview,
}: {
  currentPubkey?: string;
  execution: HiveChannelPayoutExecution | undefined;
  isExecuting: boolean;
  members: readonly ChannelMember[];
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  preview: HiveChannelPayoutPreview | null;
}) {
  const recipients = preview?.recipients ?? [];
  const missingOffer = recipients.find((recipient) => !recipient.bolt12Offer);
  const paid = execution?.paid ?? [];

  return (
    <AlertDialog onOpenChange={onOpenChange} open={open}>
      <AlertDialogContent data-testid="hive-payout-confirmation-dialog">
        <AlertDialogHeader>
          <AlertDialogTitle>Pay out hive revenue?</AlertDialogTitle>
          <AlertDialogDescription>
            {preview
              ? `${formatBitcoinAmount(preview.totalPayoutSats)} will be paid sequentially across ${recipients.length} owner${recipients.length === 1 ? "" : "s"}.`
              : "Calculated payouts will appear here."}
          </AlertDialogDescription>
        </AlertDialogHeader>

        {preview ? (
          <div className="space-y-3">
            <ul className="max-h-64 overflow-y-auto rounded-lg border border-border/70 px-3 py-2">
              {recipients.map((recipient) => (
                <li
                  className="flex items-start justify-between gap-3 border-t border-border/60 py-2 first:border-t-0 first:pt-0 last:pb-0"
                  key={recipient.memberPubkey}
                >
                  <div className="min-w-0">
                    <div
                      className="truncate text-sm font-medium"
                      title={recipient.memberPubkey}
                    >
                      {resolveHiveShareMemberLabel(
                        recipient.memberPubkey,
                        members,
                        currentPubkey,
                      )}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {recipient.shares.length} revenue share
                      {recipient.shares.length === 1 ? "" : "s"}
                      {!recipient.bolt12Offer ? " · missing BOLT12" : ""}
                    </div>
                  </div>
                  <div className="shrink-0 text-right text-sm font-medium">
                    {formatBitcoinAmount(recipient.amountSats)}
                  </div>
                </li>
              ))}
            </ul>

            {preview.skippedNoOwnerRevenueCount > 0 ? (
              <p className="text-xs text-muted-foreground">
                {preview.skippedNoOwnerRevenueCount} revenue payment
                {preview.skippedNoOwnerRevenueCount === 1 ? "" : "s"} had no
                ownership stake at the time and will remain unpaid.
              </p>
            ) : null}

            {paid.length ? (
              <p className="text-xs text-muted-foreground">
                Paid {formatBitcoinAmount(execution?.totalPaidSats ?? 0)} so far
                in this run.
              </p>
            ) : null}

            {execution?.failed ? (
              <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {execution.failed.error}
              </p>
            ) : null}

            {missingOffer ? (
              <p className="rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
                {resolveHiveShareMemberLabel(
                  missingOffer.memberPubkey,
                  members,
                  currentPubkey,
                )}{" "}
                does not have a published BOLT12 offer.
              </p>
            ) : null}
          </div>
        ) : null}

        <AlertDialogFooter>
          <AlertDialogCancel asChild>
            <Button disabled={isExecuting} type="button" variant="outline">
              Close
            </Button>
          </AlertDialogCancel>
          <AlertDialogAction asChild>
            <Button
              data-testid="hive-payout-confirm"
              disabled={!preview || Boolean(missingOffer) || isExecuting}
              onClick={(event) => {
                event.preventDefault();
                onConfirm();
              }}
              type="button"
            >
              {isExecuting ? "Paying..." : "Pay out"}
            </Button>
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

export function ChannelHiveWalletSection({
  canManageChannel,
  channel,
  currentPubkey,
  members,
  open,
}: ChannelHiveWalletSectionProps) {
  const queryClient = useQueryClient();
  const [hiveSeed, setHiveSeed] = React.useState<string | null>(null);
  const [hiveSeedError, setHiveSeedError] = React.useState<string | null>(null);
  const [isRevealingHiveSeed, setIsRevealingHiveSeed] = React.useState(false);
  const [hivePayoutPreview, setHivePayoutPreview] =
    React.useState<HiveChannelPayoutPreview | null>(null);
  const [isHivePayoutDialogOpen, setIsHivePayoutDialogOpen] =
    React.useState(false);
  const [hiveSendAmountDraft, setHiveSendAmountDraft] = React.useState("");
  const [hiveSendTargetDraft, setHiveSendTargetDraft] = React.useState("");
  const hiveSummaryQueryKey = ["hive-channel-wallet-summary", channel.id];
  const hiveTransactionsQueryKey = [
    "hive-channel-wallet-transactions",
    channel.id,
  ];
  const hiveSummaryQuery = useQuery({
    enabled: Boolean(open && channel.hiveChannel),
    queryKey: hiveSummaryQueryKey,
    queryFn: () => getHiveChannelWalletSummary(channel.id),
    retry: false,
    staleTime: 30_000,
  });
  const hasLocalHiveSeed = Boolean(hiveSummaryQuery.data?.hasLocalSeed);
  const hiveTransactionsQuery = useQuery({
    enabled: Boolean(open && channel.hiveChannel && hasLocalHiveSeed),
    queryKey: hiveTransactionsQueryKey,
    queryFn: () => getHiveChannelWalletTransactions(channel.id, 20),
    retry: false,
    staleTime: 30_000,
  });
  const hiveWalletBolt12Offer =
    hiveSummaryQuery.data?.bolt12Offer.trim() ||
    channel.hiveWalletBolt12Offer ||
    null;

  React.useEffect(() => {
    if (open) {
      return;
    }
    setHiveSeed(null);
    setHiveSeedError(null);
    setHivePayoutPreview(null);
    setIsHivePayoutDialogOpen(false);
    setHiveSendAmountDraft("");
    setHiveSendTargetDraft("");
  }, [open]);

  const hivePayoutPreviewMutation = useMutation({
    mutationFn: () => previewHiveChannelWalletPayouts(channel.id),
    onSuccess: (preview) => {
      setHivePayoutPreview(preview);
      if (preview.totalPayoutSats <= 0) {
        toast.info(hivePayoutEmptyMessage(preview));
        return;
      }
      setIsHivePayoutDialogOpen(true);
    },
  });
  const executeHivePayoutMutation = useMutation({
    mutationFn: () => executeHiveChannelWalletPayouts(channel.id),
    onSuccess: async (result) => {
      setHivePayoutPreview(result.remainingPreview);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: hiveSummaryQueryKey }),
        queryClient.invalidateQueries({ queryKey: hiveTransactionsQueryKey }),
      ]);
      if (result.status === "completed") {
        toast.success(`Paid out ${formatBitcoinAmount(result.totalPaidSats)}`);
        setIsHivePayoutDialogOpen(false);
      } else if (result.status === "nothing_to_pay") {
        toast.info(hivePayoutEmptyMessage(result.remainingPreview));
      } else {
        toast.error(result.failed?.error ?? "Hive payout stopped.");
      }
    },
  });
  const sendHiveWalletPaymentMutation = useMutation({
    mutationFn: (input: { amountSats: number; payable: string }) =>
      sendHiveChannelWalletPayment({
        channelId: channel.id,
        amountSats: input.amountSats,
        payable: input.payable,
      }),
    onSuccess: async (result) => {
      toast.success(`Sent ${formatBitcoinAmount(result.amountSats)}`);
      setHiveSendAmountDraft("");
      setHiveSendTargetDraft("");
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: hiveSummaryQueryKey }),
        queryClient.invalidateQueries({ queryKey: hiveTransactionsQueryKey }),
      ]);
    },
  });
  const generateHiveOfferMutation = useMutation({
    mutationFn: () => generateHiveChannelWalletBolt12Offer(channel.id),
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

  async function handleRevealHiveSeed() {
    setIsRevealingHiveSeed(true);
    setHiveSeedError(null);
    try {
      setHiveSeed(await revealHiveChannelWalletSeed(channel.id));
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

  function handlePreviewHivePayouts() {
    hivePayoutPreviewMutation.reset();
    executeHivePayoutMutation.reset();
    void hivePayoutPreviewMutation.mutateAsync();
  }

  function handleSendHiveWalletPayment() {
    const amountSats = parseWholeBitcoinAmount(hiveSendAmountDraft);
    const payable = hiveSendTargetDraft.trim();
    if (!amountSats) {
      toast.error("Enter a whole ₿ amount.");
      return;
    }
    if (!payable) {
      toast.error("Enter a payment target.");
      return;
    }
    void sendHiveWalletPaymentMutation.mutateAsync({ amountSats, payable });
  }

  return (
    <>
      <div
        className="space-y-3 rounded-2xl border border-border/70 bg-muted/20 p-3"
        data-testid="channel-management-hive-wallet"
      >
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Wallet className="h-4 w-4 text-muted-foreground" />
            Hive wallet
          </div>
          <div className="rounded-md border border-border/60 bg-background px-2 py-1 text-xs font-medium text-muted-foreground">
            {hiveSummaryQuery.data?.hasLocalSeed
              ? formatBitcoinAmount(hiveSummaryQuery.data.balanceSats)
              : hiveSummaryQuery.data
                ? "Member view"
                : "Hive"}
          </div>
        </div>
        {hiveSummaryQuery.data?.ownershipShares.length ? (
          <div className="space-y-1">
            {hiveSummaryQuery.data.ownershipShares.map((share) => (
              <div
                className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
                key={share.memberPubkey ?? "unknown"}
              >
                <span
                  className="truncate font-medium text-foreground"
                  title={share.memberPubkey ?? "unknown"}
                >
                  {resolveHiveShareMemberLabel(
                    share.memberPubkey,
                    members,
                    currentPubkey,
                  )}
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
          canGenerate={canManageChannel && hasLocalHiveSeed}
          isGenerating={generateHiveOfferMutation.isPending}
          isLoading={hiveSummaryQuery.isPending}
          onGenerate={() => {
            void generateHiveOfferMutation.mutateAsync();
          }}
          offer={hiveWalletBolt12Offer}
        />
        {hasLocalHiveSeed ? (
          <HiveWalletTransactions
            isLoading={hiveTransactionsQuery.isPending}
            transactions={hiveTransactionsQuery.data}
          />
        ) : null}
        {canManageChannel && hasLocalHiveSeed ? (
          <div className="space-y-3 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="min-w-0">
                <div className="text-sm font-medium">Revenue payout</div>
                <div className="text-xs text-muted-foreground">
                  Split unattributed revenue by historical ownership.
                </div>
              </div>
              <Button
                data-testid="channel-management-hive-payout-preview"
                disabled={hivePayoutPreviewMutation.isPending}
                onClick={handlePreviewHivePayouts}
                size="sm"
                type="button"
              >
                <Wallet className="h-4 w-4" />
                {hivePayoutPreviewMutation.isPending
                  ? "Calculating..."
                  : "Pay out"}
              </Button>
            </div>
            {hivePayoutPreviewMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {hivePayoutPreviewMutation.error.message}
              </p>
            ) : null}
          </div>
        ) : null}
        {canManageChannel && hasLocalHiveSeed ? (
          <form
            className="space-y-3 rounded-lg border border-border/70 bg-background/70 px-3 py-3"
            data-testid="channel-management-hive-send"
            onSubmit={(event) => {
              event.preventDefault();
              handleSendHiveWalletPayment();
            }}
          >
            <div className="flex items-center gap-2 text-sm font-medium">
              <Send className="h-4 w-4 text-muted-foreground" />
              Send from hive wallet
            </div>
            <div className="grid gap-2 sm:grid-cols-[8rem_1fr]">
              <Input
                data-testid="channel-management-hive-send-amount"
                disabled={sendHiveWalletPaymentMutation.isPending}
                inputMode="numeric"
                min={1}
                onChange={(event) => setHiveSendAmountDraft(event.target.value)}
                placeholder="1000"
                type="number"
                value={hiveSendAmountDraft}
              />
              <Input
                data-testid="channel-management-hive-send-target"
                disabled={sendHiveWalletPaymentMutation.isPending}
                onChange={(event) => setHiveSendTargetDraft(event.target.value)}
                placeholder="BOLT12, invoice, address, or @name"
                value={hiveSendTargetDraft}
              />
            </div>
            <Button
              data-testid="channel-management-hive-send-submit"
              disabled={sendHiveWalletPaymentMutation.isPending}
              size="sm"
              type="submit"
              variant="outline"
            >
              <Send className="h-4 w-4" />
              {sendHiveWalletPaymentMutation.isPending ? "Sending..." : "Send"}
            </Button>
            {sendHiveWalletPaymentMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {sendHiveWalletPaymentMutation.error.message}
              </p>
            ) : null}
          </form>
        ) : null}
        {hasLocalHiveSeed && hiveTransactionsQuery.error instanceof Error ? (
          <p className="text-sm text-destructive">
            {hiveTransactionsQuery.error.message}
          </p>
        ) : null}
        {generateHiveOfferMutation.error instanceof Error ? (
          <p className="text-sm text-destructive">
            {generateHiveOfferMutation.error.message}
          </p>
        ) : null}
        {canManageChannel && hasLocalHiveSeed ? (
          <>
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
          </>
        ) : null}
      </div>

      <HivePayoutConfirmationDialog
        currentPubkey={currentPubkey}
        execution={executeHivePayoutMutation.data}
        isExecuting={executeHivePayoutMutation.isPending}
        members={members}
        onConfirm={() => {
          executeHivePayoutMutation.reset();
          void executeHivePayoutMutation.mutateAsync();
        }}
        onOpenChange={setIsHivePayoutDialogOpen}
        open={isHivePayoutDialogOpen}
        preview={hivePayoutPreview}
      />
    </>
  );
}
