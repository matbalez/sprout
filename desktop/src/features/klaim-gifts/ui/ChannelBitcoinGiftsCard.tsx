import { Gift, Power, PowerOff } from "lucide-react";
import * as React from "react";
import { useMutation } from "@tanstack/react-query";
import { toast } from "sonner";

import { registerKlaimFaucetChannel } from "@/features/klaim-gifts/api";
import {
  buildInitialKlaimGiftConfig,
  disableKlaimGiftConfig,
  getKlaimGiftConfig,
  saveKlaimGiftConfig,
  subscribeKlaimGiftSettings,
  type KlaimGiftChannelConfig,
} from "@/features/klaim-gifts/storage";
import { formatBitcoinAmount } from "@/features/wallet/api";
import type { Channel } from "@/shared/api/types";
import { Button } from "@/shared/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/shared/ui/dialog";
import { Input } from "@/shared/ui/input";

function isAlreadyRegisteredError(error: string | null | undefined) {
  return (
    error?.toLowerCase().includes("channel is already registered") ?? false
  );
}

function formatCapacity(config: KlaimGiftChannelConfig) {
  if (
    typeof config.claimsUsed === "number" &&
    typeof config.maxClaims === "number"
  ) {
    return `${config.claimsUsed}/${config.maxClaims} claimed`;
  }
  if (typeof config.maxClaims === "number") {
    return `${config.maxClaims} max gifts`;
  }
  return "Capacity set in Klaim";
}

function formatAmount(config: KlaimGiftChannelConfig) {
  if (typeof config.defaultAmountSats === "number") {
    return formatBitcoinAmount(config.defaultAmountSats);
  }

  return "Amount set in Klaim";
}

export function ChannelBitcoinGiftsCard({
  canManage,
  channel,
  memberPubkeys,
}: {
  canManage: boolean;
  channel: Channel;
  memberPubkeys: string[];
}) {
  const [isDialogOpen, setIsDialogOpen] = React.useState(false);
  const [campaignId, setCampaignId] = React.useState("");
  const [config, setConfig] = React.useState<KlaimGiftChannelConfig | null>(
    () => getKlaimGiftConfig(channel.id),
  );

  React.useEffect(() => {
    setConfig(getKlaimGiftConfig(channel.id));
    return subscribeKlaimGiftSettings(() => {
      setConfig(getKlaimGiftConfig(channel.id));
    });
  }, [channel.id]);

  React.useEffect(() => {
    if (!isDialogOpen) {
      setCampaignId("");
    }
  }, [isDialogOpen]);

  const registerMutation = useMutation({
    mutationFn: async () => {
      const trimmedCampaignId = campaignId.trim();
      if (!trimmedCampaignId) {
        throw new Error("Enter a Klaim.cash campaign ID.");
      }

      const result = await registerKlaimFaucetChannel({
        campaignId: trimmedCampaignId,
        channelId: channel.id,
      });
      if (!result.ok && !isAlreadyRegisteredError(result.error)) {
        throw new Error(
          result.detail ?? result.error ?? "Klaim registration failed.",
        );
      }

      const nextConfig = buildInitialKlaimGiftConfig({
        campaignId: trimmedCampaignId,
        channel: {
          id: channel.id,
          memberPubkeys,
        },
        defaultAmountSats: result.defaultAmountSats,
        maxClaims: result.maxClaims,
      });
      saveKlaimGiftConfig(nextConfig);
      return { result, config: nextConfig };
    },
    onSuccess: ({ result }) => {
      setIsDialogOpen(false);
      toast.success(
        result.ok
          ? "Bitcoin gifts enabled."
          : "Bitcoin gifts enabled with existing Klaim registration.",
      );
    },
    onError: (error) => {
      toast.error(
        error instanceof Error
          ? error.message
          : "Failed to enable bitcoin gifts.",
      );
    },
  });

  const isEnabled = config?.enabled === true;

  return (
    <section className="space-y-3 rounded-xl border border-border/80 bg-muted/15 p-3">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 space-y-1">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Gift className="h-4 w-4" />
            <span>Bitcoin gifts</span>
          </div>
          <p className="text-xs leading-5 text-muted-foreground">
            Pay invited members through a Klaim.cash faucet campaign when they
            join this private channel.
          </p>
        </div>
        <div
          className="rounded-full border border-border/80 bg-background px-2.5 py-1 text-2xs font-medium text-muted-foreground"
          data-testid="channel-bitcoin-gifts-status"
        >
          {isEnabled ? "Enabled" : "Off"}
        </div>
      </div>

      {isEnabled && config ? (
        <div className="grid gap-2 rounded-lg border border-border/70 bg-background/65 p-2.5 text-xs text-muted-foreground">
          <div className="flex items-center justify-between gap-3">
            <span>Campaign</span>
            <span className="max-w-48 truncate font-mono">
              {config.campaignId}
            </span>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span>Gift</span>
            <span className="font-medium text-foreground">
              {formatAmount(config)}
            </span>
          </div>
          <div className="flex items-center justify-between gap-3">
            <span>Capacity</span>
            <span>{formatCapacity(config)}</span>
          </div>
        </div>
      ) : null}

      <div className="flex flex-wrap gap-2">
        {!isEnabled ? (
          <Button
            data-testid="channel-enable-bitcoin-gifts"
            disabled={!canManage}
            onClick={() => setIsDialogOpen(true)}
            size="sm"
            type="button"
          >
            <Power className="h-4 w-4" />
            Enable bitcoin gifts
          </Button>
        ) : (
          <Button
            data-testid="channel-disable-bitcoin-gifts"
            disabled={!canManage}
            onClick={() => {
              disableKlaimGiftConfig(channel.id);
              toast.success("Bitcoin gifts disabled on this device.");
            }}
            size="sm"
            type="button"
            variant="outline"
          >
            <PowerOff className="h-4 w-4" />
            Disable
          </Button>
        )}
      </div>

      {!canManage ? (
        <p className="text-xs text-muted-foreground">
          Only channel owners and admins can change this setting.
        </p>
      ) : null}

      <Dialog onOpenChange={setIsDialogOpen} open={isDialogOpen}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>Enable bitcoin gifts</DialogTitle>
            <DialogDescription>
              Enter the Klaim.cash campaign ID for this private channel.
            </DialogDescription>
          </DialogHeader>
          <form
            className="space-y-4"
            onSubmit={(event) => {
              event.preventDefault();
              registerMutation.mutate();
            }}
          >
            <div className="space-y-1.5">
              <label
                className="text-sm font-medium"
                htmlFor="klaim-campaign-id"
              >
                Klaim.cash campaign ID
              </label>
              <Input
                autoFocus
                data-testid="klaim-campaign-id"
                disabled={registerMutation.isPending}
                id="klaim-campaign-id"
                onChange={(event) => setCampaignId(event.target.value)}
                placeholder="summer-gifts"
                value={campaignId}
              />
            </div>
            {registerMutation.error instanceof Error ? (
              <p className="text-sm text-destructive">
                {registerMutation.error.message}
              </p>
            ) : null}
            <div className="flex justify-end gap-2">
              <Button
                disabled={registerMutation.isPending}
                onClick={() => setIsDialogOpen(false)}
                type="button"
                variant="outline"
              >
                Cancel
              </Button>
              <Button
                data-testid="klaim-register-channel"
                disabled={registerMutation.isPending}
                type="submit"
              >
                {registerMutation.isPending ? "Registering..." : "Enable"}
              </Button>
            </div>
          </form>
        </DialogContent>
      </Dialog>
    </section>
  );
}
