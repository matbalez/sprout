import { X } from "lucide-react";

export function ComposerKudosChip({ onRemove }: { onRemove: () => void }) {
  return (
    <div className="mb-2 flex items-center">
      <span className="group/kudos inline-flex items-center gap-1 rounded-full border border-amber-500/60 bg-amber-300 px-2 py-1 text-xs font-bold uppercase text-zinc-950 shadow-sm">
        Kudos
        <button
          aria-label="Remove Kudos"
          className="ml-0.5 inline-flex h-4 w-4 items-center justify-center rounded-full opacity-0 transition-opacity hover:bg-zinc-950/10 focus-visible:opacity-100 focus-visible:outline-hidden focus-visible:ring-1 focus-visible:ring-zinc-950/40 group-hover/kudos:opacity-100"
          onClick={onRemove}
          type="button"
        >
          <X className="h-3 w-3" />
        </button>
      </span>
    </div>
  );
}
