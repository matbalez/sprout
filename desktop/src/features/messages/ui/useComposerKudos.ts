import * as React from "react";
import { useQuery } from "@tanstack/react-query";

import { KUDOS_BALANCE_THRESHOLD_SATS } from "@/features/messages/lib/messageKudos";
import { getLightningWalletSummary } from "@/features/wallet/api";

export function useComposerKudos({
  disabled,
  editTargetActive,
  focusEditor,
}: {
  disabled: boolean;
  editTargetActive: boolean;
  focusEditor: () => void;
}) {
  const [isKudosActive, setIsKudosActive] = React.useState(false);
  const walletSummaryQuery = useQuery({
    enabled: !disabled && !editTargetActive,
    queryKey: ["lightning-wallet", "summary"],
    queryFn: getLightningWalletSummary,
    retry: false,
    staleTime: 30_000,
  });

  const canGiveKudos =
    (walletSummaryQuery.data?.lightningSendableBalanceSats ?? 0) >
    KUDOS_BALANCE_THRESHOLD_SATS;
  const kudosDisabled = !canGiveKudos || editTargetActive;

  const handleGiveKudos = React.useCallback(() => {
    if (kudosDisabled) {
      return;
    }
    setIsKudosActive(true);
    focusEditor();
  }, [focusEditor, kudosDisabled]);

  return {
    handleGiveKudos,
    isKudosActive,
    kudosDisabled,
    setIsKudosActive,
  };
}
