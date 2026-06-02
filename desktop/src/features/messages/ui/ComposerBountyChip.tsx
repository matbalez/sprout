import { X } from "lucide-react";

import { formatBountyAmount } from "@/features/messages/lib/messageBounties";

export function ComposerBountyChip({
  amountSats,
  onRemove,
}: {
  amountSats: number;
  onRemove: () => void;
}) {
  return (
    <div className="mb-2 flex items-center">
      <span className="group/bounty inline-flex items-center gap-1 rounded-full border border-emerald-500/60 bg-emerald-300 px-2 py-1 text-xs font-bold uppercase text-emerald-950 shadow-sm dark:bg-emerald-400/90">
        Bounty: {formatBountyAmount(amountSats)}
        <button
          aria-label="Remove bounty"
          className="ml-0.5 inline-flex h-4 w-4 items-center justify-center rounded-full opacity-0 transition-opacity hover:bg-zinc-950/10 focus-visible:opacity-100 focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-zinc-950/40 group-hover/bounty:opacity-100"
          onClick={onRemove}
          type="button"
        >
          <X className="h-3 w-3" />
        </button>
      </span>
    </div>
  );
}
