import { X } from "lucide-react";

import { Button } from "@/shared/ui/button";

export function ComposerContextBanner({
  editTarget,
  onCancelEdit,
  onCancelReply,
  replyTarget,
}: {
  editTarget: { author: string; body: string; id: string } | null;
  onCancelEdit?: () => void;
  onCancelReply?: () => void;
  replyTarget: { author: string; body: string; id: string } | null;
}) {
  if (editTarget) {
    return (
      <div
        className="mb-3 flex items-start justify-between gap-3 rounded-2xl border border-primary/30 bg-primary/5 px-3 py-2"
        data-testid="edit-target"
      >
        <div className="min-w-0">
          <p className="text-2xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
            Editing message
          </p>
          <p className="truncate text-sm text-foreground/80">
            {editTarget.body}
          </p>
        </div>
        <Button
          className="shrink-0"
          onClick={onCancelEdit}
          size="sm"
          type="button"
          variant="ghost"
        >
          Cancel
        </Button>
      </div>
    );
  }

  if (!replyTarget) {
    return null;
  }

  return (
    <div
      className="mb-3 flex items-start justify-between gap-3 rounded-2xl border border-border/70 bg-muted/40 px-3 py-2"
      data-testid="reply-target"
    >
      <div className="min-w-0">
        <p className="text-2xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
          Replying to {replyTarget.author}
        </p>
        <p className="truncate text-sm text-foreground/80">
          {replyTarget.body}
        </p>
      </div>
      {onCancelReply ? (
        <Button
          aria-label="Cancel reply"
          className="h-7 w-7 shrink-0 px-0"
          onClick={onCancelReply}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X className="h-4 w-4" />
        </Button>
      ) : null}
    </div>
  );
}
