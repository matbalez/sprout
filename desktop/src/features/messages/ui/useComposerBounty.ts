import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { toast } from "sonner";

import {
  formatBountyAmount,
  parseBountyAmountInput,
} from "@/features/messages/lib/messageBounties";
import { getLightningWalletSummary } from "@/features/wallet/api";

export function useComposerBounty({
  disabled,
  editTargetActive,
  focusEditor,
}: {
  disabled: boolean;
  editTargetActive: boolean;
  focusEditor: () => void;
}) {
  const [bountyAmountSats, setBountyAmountSats] = React.useState<number | null>(
    null,
  );
  const [isBountyDialogOpen, setIsBountyDialogOpen] = React.useState(false);
  const [bountyAmountInput, setBountyAmountInput] = React.useState("");
  const [bountyError, setBountyError] = React.useState<string | null>(null);
  const walletSummaryQuery = useQuery({
    enabled: !disabled && !editTargetActive,
    queryKey: ["lightning-wallet", "summary"],
    queryFn: getLightningWalletSummary,
    retry: false,
    staleTime: 30_000,
  });

  const focusEditorSoon = React.useCallback(() => {
    if (typeof window === "undefined") {
      focusEditor();
      return;
    }
    window.requestAnimationFrame(() => focusEditor());
  }, [focusEditor]);

  const bountyDisabled = editTargetActive;

  const handleBountyDialogOpenChange = React.useCallback(
    (open: boolean) => {
      setIsBountyDialogOpen(open);
      if (!open) {
        setBountyError(null);
        focusEditorSoon();
      }
    },
    [focusEditorSoon],
  );

  const handleAddBounty = React.useCallback(() => {
    if (bountyDisabled) {
      toast.error("Message bounties cannot be added while editing.");
      return;
    }

    setBountyAmountInput(bountyAmountSats ? String(bountyAmountSats) : "");
    setBountyError(null);
    setIsBountyDialogOpen(true);
  }, [bountyAmountSats, bountyDisabled]);

  const handleBountyAmountSubmit = React.useCallback(() => {
    try {
      const amount = parseBountyAmountInput(bountyAmountInput);
      const sendable = walletSummaryQuery.data?.lightningSendableBalanceSats;
      if (typeof sendable !== "number") {
        throw new Error(
          walletSummaryQuery.isPending || walletSummaryQuery.isFetching
            ? "Wallet balance is still loading."
            : "Lightning wallet balance is not available.",
        );
      }
      if (amount > sendable) {
        throw new Error(
          `Your wallet can currently send ${formatBountyAmount(sendable)}.`,
        );
      }

      setBountyAmountSats(amount);
      setIsBountyDialogOpen(false);
      setBountyError(null);
      focusEditorSoon();
    } catch (error) {
      setBountyError(
        error instanceof Error ? error.message : "Invalid bounty amount.",
      );
    }
  }, [
    bountyAmountInput,
    focusEditorSoon,
    walletSummaryQuery.data?.lightningSendableBalanceSats,
    walletSummaryQuery.isFetching,
    walletSummaryQuery.isPending,
  ]);

  const sendableBalanceSats =
    walletSummaryQuery.data?.lightningSendableBalanceSats ?? null;
  const isWalletLoading =
    !walletSummaryQuery.data &&
    (walletSummaryQuery.isPending || walletSummaryQuery.isFetching);

  return {
    bountyAmountSats,
    bountyDialog: {
      amountValue: bountyAmountInput,
      error: bountyError,
      isWalletLoading,
      onAmountValueChange: setBountyAmountInput,
      onOpenChange: handleBountyDialogOpenChange,
      onSubmit: handleBountyAmountSubmit,
      open: isBountyDialogOpen,
      sendableBalanceSats,
    },
    bountyDisabled,
    handleAddBounty,
    setBountyAmountSats,
  };
}
