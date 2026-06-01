import * as React from "react";

import type { TimelineTipSummary } from "@/features/messages/types";
import { formatBitcoinAmount } from "@/features/wallet/api";
import { cn } from "@/shared/lib/cn";
import { Popover, PopoverContent, PopoverTrigger } from "@/shared/ui/popover";
import { UserAvatar } from "@/shared/ui/UserAvatar";

const MAX_VISIBLE_TIPPERS = 10;

function TipPopoverContent({ summary }: { summary: TimelineTipSummary }) {
  const visible = summary.users.slice(0, MAX_VISIBLE_TIPPERS);
  const overflow = summary.users.length - MAX_VISIBLE_TIPPERS;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2 border-b border-border/50 pb-1">
        <span className="text-lg font-semibold">
          {formatBitcoinAmount(summary.amountSats)}
        </span>
        <span className="text-xs text-muted-foreground">
          {summary.count} {summary.count === 1 ? "tip" : "tips"}
        </span>
      </div>
      <div className="flex flex-col gap-1.5">
        {visible.map((user) => (
          <div className="flex min-w-0 items-center gap-2" key={user.tipId}>
            <UserAvatar
              avatarUrl={user.avatarUrl}
              displayName={user.displayName}
              size="xs"
            />
            <span className="truncate text-sm">{user.displayName}</span>
            <span className="ml-auto shrink-0 text-xs text-muted-foreground">
              {formatBitcoinAmount(user.amountSats)}
            </span>
          </div>
        ))}
      </div>
      {overflow > 0 && (
        <span className="text-xs text-muted-foreground">+{overflow} more</span>
      )}
    </div>
  );
}

export function MessageTips({
  summary,
  className,
}: {
  summary?: TimelineTipSummary;
  className?: string;
}) {
  const [open, setOpen] = React.useState(false);
  const openTimeout = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimeout = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimers = React.useCallback(() => {
    if (openTimeout.current) {
      clearTimeout(openTimeout.current);
      openTimeout.current = null;
    }
    if (closeTimeout.current) {
      clearTimeout(closeTimeout.current);
      closeTimeout.current = null;
    }
  }, []);

  const handleMouseEnter = React.useCallback(() => {
    if (!summary || summary.users.length === 0) return;
    clearTimers();
    openTimeout.current = setTimeout(() => setOpen(true), 200);
  }, [summary, clearTimers]);

  const scheduleClose = React.useCallback(() => {
    clearTimers();
    closeTimeout.current = setTimeout(() => setOpen(false), 150);
  }, [clearTimers]);

  const handleFocus = React.useCallback(() => {
    if (!summary || summary.users.length === 0) return;
    clearTimers();
    setOpen(true);
  }, [summary, clearTimers]);

  React.useEffect(() => clearTimers, [clearTimers]);

  if (!summary || summary.amountSats <= 0) {
    return null;
  }

  const pill = (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-medium transition-colors",
        summary.tippedByCurrentUser
          ? "border-primary/40 bg-primary/10 text-primary"
          : "border-border/70 bg-muted/70 text-foreground/90",
      )}
    >
      <span>₿</span>
      <span>{formatBitcoinAmount(summary.amountSats).slice(1)}</span>
    </span>
  );

  if (summary.users.length === 0) {
    return <div className={className}>{pill}</div>;
  }

  return (
    <div className={className}>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          {/* biome-ignore lint/a11y/noStaticElementInteractions: span delegates hover/focus to the noninteractive tip pill */}
          <span
            className="inline-flex"
            onMouseEnter={handleMouseEnter}
            onMouseLeave={scheduleClose}
            onFocus={handleFocus}
            onBlur={scheduleClose}
          >
            {pill}
          </span>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          className="w-auto min-w-48 max-w-64 p-3"
          onCloseAutoFocus={(event) => event.preventDefault()}
          onMouseEnter={handleMouseEnter}
          onMouseLeave={scheduleClose}
          onOpenAutoFocus={(event) => event.preventDefault()}
          side="top"
          sideOffset={6}
        >
          <TipPopoverContent summary={summary} />
        </PopoverContent>
      </Popover>
    </div>
  );
}
