import * as React from "react";
import { useMutation } from "@tanstack/react-query";
import { Send } from "lucide-react";
import { toast } from "sonner";

import {
  formatBitcoinAmount,
  sendLightningWalletPayment,
} from "@/features/wallet/api";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";

function parseBitcoinAmountInput(value: string) {
  const compact = value.trim().replace(/,/g, "");
  if (!/^[1-9][0-9]*$/.test(compact)) {
    return null;
  }

  const amount = Number(compact);
  if (!Number.isSafeInteger(amount)) {
    return null;
  }

  return amount;
}

export function ProfilePaymentForm({
  bolt12Offer,
  displayName,
  pubkey,
}: {
  bolt12Offer: string;
  displayName: string;
  pubkey: string;
}) {
  const [amountInput, setAmountInput] = React.useState("");
  const [successMessage, setSuccessMessage] = React.useState<string | null>(
    null,
  );
  const amountSats = React.useMemo(
    () => parseBitcoinAmountInput(amountInput),
    [amountInput],
  );
  const sendMutation = useMutation({
    mutationFn: async () => {
      if (amountSats === null) {
        throw new Error("Enter a whole ₿ amount.");
      }
      return sendLightningWalletPayment(amountSats, bolt12Offer);
    },
    onSuccess: (result) => {
      const message = `Sent ${formatBitcoinAmount(result.amountSats)}.`;
      setSuccessMessage(message);
      setAmountInput("");
      toast.success(message);
    },
  });

  return (
    <form
      className="mt-4 space-y-2 rounded-lg border border-border/70 bg-background/70 px-3 py-3"
      onSubmit={(event) => {
        event.preventDefault();
        setSuccessMessage(null);
        sendMutation.mutate();
      }}
    >
      <div className="flex items-center gap-2">
        <label className="sr-only" htmlFor={`profile-send-amount-${pubkey}`}>
          Amount
        </label>
        <div className="flex min-w-0 flex-1 items-center rounded-md border border-input bg-background px-3 text-sm">
          <span className="shrink-0 font-semibold text-muted-foreground">
            ₿
          </span>
          <input
            aria-label={`Amount to send to ${displayName}`}
            className="min-w-0 flex-1 bg-transparent px-2 py-2 outline-none placeholder:text-muted-foreground"
            data-testid="user-profile-wallet-send-amount"
            disabled={sendMutation.isPending}
            id={`profile-send-amount-${pubkey}`}
            inputMode="numeric"
            onChange={(event) => {
              setAmountInput(event.currentTarget.value);
              setSuccessMessage(null);
              if (sendMutation.isError) {
                sendMutation.reset();
              }
            }}
            value={amountInput}
          />
        </div>
        <Button
          className="shrink-0"
          data-testid="user-profile-wallet-send"
          disabled={sendMutation.isPending || amountSats === null}
          type="submit"
        >
          {sendMutation.isPending ? (
            <Spinner className="h-4 w-4" />
          ) : (
            <Send className="h-4 w-4" />
          )}
          Send ₿
        </Button>
      </div>
      {successMessage ? (
        <p className="text-xs font-medium text-emerald-600 dark:text-emerald-400">
          {successMessage}
        </p>
      ) : null}
      {sendMutation.isError ? (
        <p className="text-xs text-destructive">
          {sendMutation.error instanceof Error
            ? sendMutation.error.message
            : "Payment failed."}
        </p>
      ) : null}
    </form>
  );
}
