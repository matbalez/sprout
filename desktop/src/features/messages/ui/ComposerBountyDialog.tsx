import * as React from "react";

import { formatBountyAmount } from "@/features/messages/lib/messageBounties";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";

type ComposerBountyDialogProps = {
  amountValue: string;
  error: string | null;
  isWalletLoading: boolean;
  onAmountValueChange: (value: string) => void;
  onOpenChange: (open: boolean) => void;
  onSubmit: () => void;
  open: boolean;
  sendableBalanceSats: number | null;
};

export function ComposerBountyDialog({
  amountValue,
  error,
  isWalletLoading,
  onAmountValueChange,
  onOpenChange,
  onSubmit,
  open,
  sendableBalanceSats,
}: ComposerBountyDialogProps) {
  const amountInputId = React.useId();
  const submitDisabled = isWalletLoading || sendableBalanceSats === null;
  const walletStatus = isWalletLoading
    ? "Checking wallet..."
    : sendableBalanceSats === null
      ? "Wallet balance unavailable"
      : `Spendable ${formatBountyAmount(sendableBalanceSats)}`;

  return (
    <Dialog onOpenChange={onOpenChange} open={open}>
      <DialogContent className="max-w-sm rounded-2xl">
        <DialogHeader>
          <DialogTitle>Message bounty</DialogTitle>
          <DialogDescription>Choose a whole ₿ amount.</DialogDescription>
        </DialogHeader>
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            onSubmit();
          }}
        >
          <div className="space-y-2">
            <label
              className="text-sm font-medium text-foreground"
              htmlFor={amountInputId}
            >
              Amount
            </label>
            <div className="relative">
              <span className="-translate-y-1/2 pointer-events-none absolute left-3 top-1/2 text-sm text-muted-foreground">
                ₿
              </span>
              <Input
                autoComplete="off"
                autoFocus
                className="pl-7"
                data-testid="message-bounty-amount-input"
                id={amountInputId}
                inputMode="numeric"
                onChange={(event) => onAmountValueChange(event.target.value)}
                placeholder="2,100"
                value={amountValue}
              />
            </div>
            <p className="text-xs text-muted-foreground">{walletStatus}</p>
            {error ? (
              <p className="text-xs text-destructive" role="alert">
                {error}
              </p>
            ) : null}
          </div>
          <div className="flex justify-end gap-2">
            <Button
              onClick={() => onOpenChange(false)}
              type="button"
              variant="ghost"
            >
              Cancel
            </Button>
            <Button
              data-testid="message-bounty-confirm"
              disabled={submitDisabled}
              type="submit"
            >
              Add
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
