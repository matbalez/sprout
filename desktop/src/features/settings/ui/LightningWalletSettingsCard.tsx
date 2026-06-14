import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Bot,
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  RefreshCw,
  ShieldCheck,
  TriangleAlert,
  WalletCards,
  Zap,
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";

import {
  formatBitcoinAmount,
  getMdkAgentWalletStatus,
  getLightningWalletAgentPaymentSettings,
  getLightningWalletSummary,
  getLightningWalletSourceConfig,
  getLightningWalletTransactions,
  refreshLightningWallet,
  revealLightningWalletSeed,
  restartMdkAgentWalletDaemon,
  setLightningWalletAgentPaymentSettings,
  setLightningWalletProvider,
  setLightningWalletSource,
  type MdkAgentWalletStatus,
  walletProviderLabel,
  type WalletProvider,
  type WalletProviderOption,
  type WalletSource,
  type WalletSourceConfig,
  type WalletSummary,
} from "@/features/wallet/api";
import {
  formatWalletTransactionTitle,
  walletTransactionNotes,
} from "@/features/wallet/transactions";
import { Button } from "@/shared/ui/button";
import { Spinner } from "@/shared/ui/spinner";
import { Switch } from "@/shared/ui/switch";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/shared/ui/tabs";
import { Textarea } from "@/shared/ui/textarea";

const walletSummaryQueryKey = ["lightning-wallet", "summary"] as const;
const walletSourceQueryKey = ["lightning-wallet", "source"] as const;
const walletTransactionsQueryKey = [
  "lightning-wallet",
  "transactions",
] as const;
const walletAgentPaymentSettingsQueryKey = [
  "lightning-wallet",
  "agent-payment-settings",
] as const;
const mdkAgentWalletStatusQueryKey = [
  "lightning-wallet",
  "mdk-agent-wallet-status",
] as const;

const lexeProviderCapabilities = {
  canCreateWallet: true,
  canConnectExistingWallet: true,
  canReceiveReusableBolt12: true,
  canSendBolt12: true,
  canGetBalance: true,
  canListPayments: true,
  canSubscribePayments: false,
  canSendBolt11: true,
  canCreateBolt11Invoice: true,
  canPayWithPreimage: true,
};

const mdkProviderCapabilities = {
  canCreateWallet: true,
  canConnectExistingWallet: false,
  canReceiveReusableBolt12: true,
  canSendBolt12: true,
  canGetBalance: true,
  canListPayments: true,
  canSubscribePayments: false,
  canSendBolt11: true,
  canCreateBolt11Invoice: true,
  canPayWithPreimage: true,
};

const defaultWalletProviderOptions: WalletProviderOption[] = [
  {
    provider: "lexe",
    label: walletProviderLabel("lexe"),
    paymentRail: "lexe-bolt12",
    available: true,
    capabilities: lexeProviderCapabilities,
  },
  {
    provider: "mdk",
    label: walletProviderLabel("mdk"),
    paymentRail: "mdk-bolt12",
    available: true,
    capabilities: mdkProviderCapabilities,
  },
];

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Wallet request failed";
}

function formatMdkLastCheckedAt(value: number) {
  return new Date(value).toLocaleTimeString([], {
    hour: "numeric",
    minute: "2-digit",
    second: "2-digit",
  });
}

function formatMdkStatusToast(status: MdkAgentWalletStatus) {
  const displayedPort = status.port ?? status.expectedPort;
  if (!status.running) {
    return "MDK daemon is not running";
  }
  if (!status.healthy) {
    return `MDK daemon is unhealthy on port ${displayedPort}: ${
      status.healthError ?? "health check failed"
    }`;
  }
  if (!status.nodeRunning) {
    return `MDK daemon is listening on port ${displayedPort}, node is still starting`;
  }
  return `MDK daemon running on port ${displayedPort}${
    status.pid ? `, pid ${status.pid}` : ""
  }`;
}

async function copyToClipboard(value: string, label: string) {
  await navigator.clipboard.writeText(value);
  toast.success(`${label} copied`);
}

function SummaryContent({ summary }: { summary: WalletSummary }) {
  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-border/70 bg-background/70 px-4 py-3">
        <p className="text-xs font-medium text-muted-foreground">
          Lightning balance
        </p>
        <p className="mt-1 truncate text-2xl font-semibold">
          {formatBitcoinAmount(summary.lightningBalanceSats)}
        </p>
        <div className="mt-3 border-t border-border/60 pt-3">
          <p className="text-xs font-medium text-muted-foreground">
            Spendable balance
          </p>
          <p className="mt-1 truncate text-lg font-semibold">
            {formatBitcoinAmount(summary.lightningSendableBalanceSats)}
          </p>
        </div>
      </div>

      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <p className="text-xs font-medium text-muted-foreground">
            Add funds to your wallet (via BOLT12)
          </p>
          <Button
            onClick={() => copyToClipboard(summary.bolt12Offer, "BOLT12")}
            size="sm"
            type="button"
            variant="outline"
          >
            <Copy className="h-4 w-4" />
            Copy
          </Button>
        </div>
        <div className="flex justify-center rounded-lg border border-border/70 bg-background/70 px-3 py-4">
          <div className="rounded-lg bg-white p-3 shadow-sm">
            <QRCodeSVG
              bgColor="#ffffff"
              className="h-auto max-w-full"
              fgColor="#000000"
              level="M"
              size={280}
              value={summary.bolt12Offer}
            />
          </div>
        </div>
        <code className="block max-h-24 overflow-y-auto break-all rounded-lg border border-border bg-muted/40 px-3 py-2 text-xs">
          {summary.bolt12Offer}
        </code>
      </div>
    </div>
  );
}

function WalletProviderSettings({
  actionError,
  config,
  error,
  isLoading,
  isPending,
  onProviderChange,
  pendingProvider,
}: {
  actionError: unknown;
  config: WalletSourceConfig | undefined;
  error: unknown;
  isLoading: boolean;
  isPending: boolean;
  onProviderChange: (provider: WalletProvider) => void;
  pendingProvider: WalletProvider | undefined;
}) {
  const options = config?.availableProviders?.length
    ? config.availableProviders
    : defaultWalletProviderOptions;
  const selectedProvider = config?.provider ?? "lexe";
  const selectableProviders = options.filter((option) => option.available);
  const canChooseProvider = selectableProviders.length > 1;

  return (
    <div className="mb-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
      <label
        className="mb-2 block text-sm font-medium"
        htmlFor="wallet-provider-select"
      >
        Wallet provider
      </label>
      {isLoading ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          Loading provider...
        </div>
      ) : error ? (
        <div className="flex items-start gap-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(error)}</span>
        </div>
      ) : (
        <select
          aria-label="Wallet provider"
          className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm outline-hidden focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-70"
          disabled={isPending || !canChooseProvider}
          id="wallet-provider-select"
          onChange={(event) =>
            onProviderChange(event.currentTarget.value as WalletProvider)
          }
          value={selectedProvider}
        >
          {options.map((option) => (
            <option
              disabled={!option.available}
              key={option.provider}
              value={option.provider}
            >
              {option.label}
            </option>
          ))}
        </select>
      )}
      {isPending ? (
        <div className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          <span>
            {pendingProvider === "mdk"
              ? "Provisioning new wallet..."
              : `Switching to ${walletProviderLabel(pendingProvider ?? selectedProvider)}...`}
          </span>
        </div>
      ) : null}
      {actionError ? (
        <div className="mt-3 flex items-start gap-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(actionError)}</span>
        </div>
      ) : null}
    </div>
  );
}

function WalletSourceSettings({
  actionError,
  clientCredential,
  config,
  error,
  isLoading,
  isPending,
  isSeedPending,
  onClientCredentialChange,
  onHideSeed,
  onRevealSeed,
  onSaveExistingCredential,
  onSourceChange,
  seedPhrase,
  sourceDraft,
}: {
  actionError: unknown;
  clientCredential: string;
  config: WalletSourceConfig | undefined;
  error: unknown;
  isLoading: boolean;
  isPending: boolean;
  isSeedPending: boolean;
  onClientCredentialChange: (value: string) => void;
  onHideSeed: () => void;
  onRevealSeed: () => void;
  onSaveExistingCredential: () => void;
  onSourceChange: (source: WalletSource) => void;
  seedPhrase: string | undefined;
  sourceDraft: WalletSource;
}) {
  return (
    <div className="mt-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
      <div className="flex min-w-0 items-center gap-2">
        <ShieldCheck className="h-4 w-4" />
        <p className="text-sm font-medium">Wallet source</p>
      </div>

      {isLoading ? (
        <div className="mt-3 flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          Loading wallet source...
        </div>
      ) : error ? (
        <div className="mt-3 flex items-start gap-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(error)}</span>
        </div>
      ) : (
        <Tabs
          className="mt-3"
          onValueChange={(value) => onSourceChange(value as WalletSource)}
          value={sourceDraft}
        >
          <div className="flex flex-col gap-2 sm:flex-row sm:items-center">
            <p className="text-xs font-medium text-muted-foreground">Use</p>
            <TabsList className="grid h-auto w-full grid-cols-1 sm:w-auto sm:grid-cols-2">
              <TabsTrigger
                className="whitespace-normal px-2 py-1.5 text-xs sm:text-sm"
                value="default"
              >
                DEFAULT LEXE WALLET
              </TabsTrigger>
              <TabsTrigger
                className="whitespace-normal px-2 py-1.5 text-xs sm:text-sm"
                value="existing"
              >
                MY EXISTING LEXE WALLET
              </TabsTrigger>
            </TabsList>
          </div>

          <TabsContent value="default">
            <div className="rounded-lg border border-border/70 bg-muted/20 px-3 py-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="min-w-0">
                  <p className="flex items-center gap-2 text-sm font-medium">
                    <ShieldCheck className="h-4 w-4" />
                    Recovery seed
                  </p>
                  <p className="break-all text-xs text-muted-foreground">
                    {config?.seedPath ?? "Wallet seed file path unavailable"}
                  </p>
                </div>
                <Button
                  disabled={isSeedPending}
                  onClick={seedPhrase ? onHideSeed : onRevealSeed}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {isSeedPending ? (
                    <Spinner className="h-4 w-4" />
                  ) : seedPhrase ? (
                    <EyeOff className="h-4 w-4" />
                  ) : (
                    <Eye className="h-4 w-4" />
                  )}
                  {seedPhrase ? "Hide" : "Reveal"}
                </Button>
              </div>
              {seedPhrase ? (
                <div className="mt-3 space-y-2">
                  <code className="block break-words rounded-lg border border-border bg-muted/40 px-3 py-2 text-xs">
                    {seedPhrase}
                  </code>
                  <Button
                    onClick={() => copyToClipboard(seedPhrase, "Recovery seed")}
                    size="sm"
                    type="button"
                    variant="outline"
                  >
                    <Copy className="h-4 w-4" />
                    Copy seed
                  </Button>
                </div>
              ) : null}
            </div>
          </TabsContent>

          <TabsContent value="existing">
            <div className="rounded-lg border border-border/70 bg-muted/20 px-3 py-3">
              <div className="flex items-center gap-2 text-sm font-medium">
                <KeyRound className="h-4 w-4" />
                Lexe SDK client credential
              </div>
              <p className="mt-1 break-all text-xs text-muted-foreground">
                {config?.hasExistingClientCredential
                  ? `Saved at ${config.existingClientCredentialPath}`
                  : "No credential saved"}
              </p>
              <Textarea
                className="mt-3 min-h-24 font-mono text-xs"
                onChange={(event) =>
                  onClientCredentialChange(event.currentTarget.value)
                }
                placeholder={
                  config?.hasExistingClientCredential
                    ? "Paste a replacement credential"
                    : "Paste credential"
                }
                spellCheck={false}
                value={clientCredential}
              />
              <div className="mt-3 flex flex-wrap items-center justify-between gap-2">
                <p className="text-xs text-muted-foreground">
                  {config?.source === "existing"
                    ? "Using existing Lexe wallet"
                    : "Using default Lexe wallet"}
                </p>
                <Button
                  disabled={isPending || !clientCredential.trim()}
                  onClick={onSaveExistingCredential}
                  size="sm"
                  type="button"
                  variant="outline"
                >
                  {isPending ? (
                    <Spinner className="h-4 w-4" />
                  ) : (
                    <KeyRound className="h-4 w-4" />
                  )}
                  Save
                </Button>
              </div>
            </div>
          </TabsContent>
          {actionError ? (
            <div className="mt-3 flex items-start gap-2 text-sm text-destructive">
              <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
              <span>{errorMessage(actionError)}</span>
            </div>
          ) : null}
        </Tabs>
      )}
    </div>
  );
}

function MdkAgentWalletControls({
  error,
  isChecking,
  isLoading,
  isRestarting,
  lastCheckedAt,
  onCheckStatus,
  onRestart,
  restartError,
  status,
}: {
  error: unknown;
  isChecking: boolean;
  isLoading: boolean;
  isRestarting: boolean;
  lastCheckedAt: number | null;
  onCheckStatus: () => void;
  onRestart: () => void;
  restartError: unknown;
  status: MdkAgentWalletStatus | undefined;
}) {
  const actionError = error ?? restartError;
  const displayedPort = status?.port ?? status?.expectedPort;
  const statusLabel = status?.running
    ? status.nodeRunning
      ? "Running"
      : status.healthy
        ? "Starting"
        : "Unhealthy"
    : "Not running";
  const statusDetail = status?.running
    ? [
        displayedPort ? `port ${displayedPort}` : null,
        status.pid ? `pid ${status.pid}` : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : "Daemon is not running";

  return (
    <div className="mt-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="flex items-center gap-2 text-sm font-medium">
            <Zap className="h-4 w-4" />
            MDK agent wallet
          </p>
          <p className="mt-1 text-sm text-muted-foreground">
            {isLoading
              ? "Checking daemon..."
              : `${statusLabel}: ${statusDetail}`}
          </p>
          {status ? (
            <p className="mt-1 text-xs text-muted-foreground">
              Health:{" "}
              {status.healthy
                ? status.nodeRunning
                  ? "ready"
                  : "daemon ready, node starting"
                : (status.healthError ?? "not responding")}
            </p>
          ) : null}
          {lastCheckedAt ? (
            <p className="mt-1 text-xs text-muted-foreground">
              Last checked: {formatMdkLastCheckedAt(lastCheckedAt)}
            </p>
          ) : null}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button
            disabled={isChecking || isRestarting}
            onClick={onCheckStatus}
            size="sm"
            type="button"
            variant="outline"
          >
            {isChecking ? (
              <Spinner className="h-4 w-4" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            Check status
          </Button>
          <Button
            disabled={isRestarting}
            onClick={onRestart}
            size="sm"
            type="button"
            variant="outline"
          >
            {isRestarting ? (
              <Spinner className="h-4 w-4" />
            ) : (
              <RefreshCw className="h-4 w-4" />
            )}
            Restart daemon
          </Button>
        </div>
      </div>

      {status ? (
        <div className="mt-3 space-y-1 text-xs text-muted-foreground">
          <p className="break-all">
            Home: <code>{status.homeDir}</code>
          </p>
          <p className="break-all">
            Log: <code>{status.logPath}</code>
          </p>
        </div>
      ) : null}

      {actionError ? (
        <div className="mt-3 flex items-start gap-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(actionError)}</span>
        </div>
      ) : null}
    </div>
  );
}

function AgentPaymentSettings({
  checked,
  error,
  isLoading,
  isPending,
  onCheckedChange,
}: {
  checked: boolean;
  error: unknown;
  isLoading: boolean;
  isPending: boolean;
  onCheckedChange: (checked: boolean) => void;
}) {
  return (
    <div className="mt-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <label
            className="flex items-center gap-2 text-sm font-medium"
            htmlFor="default-agents-to-lexe-switch"
          >
            <Bot className="h-4 w-4" />
            Default agents to using Lexe for payments
          </label>
          <p className="mt-1 text-sm text-muted-foreground">
            Expose Sprout Lexe payment tools to managed agents unless an agent
            has its own MCP toolset override.
          </p>
        </div>
        {isLoading ? (
          <Spinner className="h-4 w-4 shrink-0" />
        ) : (
          <Switch
            checked={checked}
            data-testid="wallet-default-agents-to-lexe-toggle"
            disabled={isPending}
            id="default-agents-to-lexe-switch"
            onCheckedChange={onCheckedChange}
          />
        )}
      </div>
      {error ? (
        <div className="mt-3 flex items-start gap-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(error)}</span>
        </div>
      ) : null}
    </div>
  );
}

export function LightningWalletSettingsCard() {
  const queryClient = useQueryClient();
  const [sourceDraft, setSourceDraft] = useState<WalletSource>("default");
  const [clientCredential, setClientCredential] = useState("");
  const [isSeedVisible, setIsSeedVisible] = useState(false);
  const [mdkStatusCheckedAt, setMdkStatusCheckedAt] = useState<number | null>(
    null,
  );
  const summaryQuery = useQuery({
    queryKey: walletSummaryQueryKey,
    queryFn: getLightningWalletSummary,
    staleTime: 30_000,
  });
  const sourceConfigQuery = useQuery({
    queryKey: walletSourceQueryKey,
    queryFn: getLightningWalletSourceConfig,
    staleTime: 30_000,
  });
  const agentPaymentSettingsQuery = useQuery({
    queryKey: walletAgentPaymentSettingsQueryKey,
    queryFn: getLightningWalletAgentPaymentSettings,
    staleTime: 30_000,
  });
  const transactionsQuery = useQuery({
    queryKey: walletTransactionsQueryKey,
    queryFn: () => getLightningWalletTransactions(20),
    enabled: summaryQuery.isSuccess,
    staleTime: 30_000,
  });
  const refreshMutation = useMutation({
    mutationFn: refreshLightningWallet,
    onSuccess: (summary) => {
      queryClient.setQueryData(walletSummaryQueryKey, summary);
      void queryClient.invalidateQueries({
        queryKey: walletTransactionsQueryKey,
      });
    },
  });
  const seedMutation = useMutation({
    mutationFn: revealLightningWalletSeed,
    onSuccess: () => {
      setIsSeedVisible(true);
    },
  });
  const sourceMutation = useMutation({
    mutationFn: setLightningWalletSource,
    onSuccess: (config) => {
      queryClient.setQueryData(walletSourceQueryKey, config);
      setSourceDraft(config.source);
      setClientCredential("");
      setIsSeedVisible(false);
      seedMutation.reset();
      void queryClient.invalidateQueries({
        queryKey: mdkAgentWalletStatusQueryKey,
      });
      void queryClient.invalidateQueries({ queryKey: walletSummaryQueryKey });
      void queryClient.invalidateQueries({
        queryKey: walletTransactionsQueryKey,
      });
      toast.success(
        config.source === "existing"
          ? "Existing Lexe wallet selected"
          : "Default Lexe wallet selected",
      );
    },
  });
  const providerMutation = useMutation({
    mutationFn: setLightningWalletProvider,
    onSuccess: (config) => {
      queryClient.setQueryData(walletSourceQueryKey, config);
      setSourceDraft(config.source);
      setClientCredential("");
      setIsSeedVisible(false);
      seedMutation.reset();
      void queryClient.invalidateQueries({
        queryKey: mdkAgentWalletStatusQueryKey,
      });
      void queryClient.invalidateQueries({ queryKey: walletSummaryQueryKey });
      void queryClient.invalidateQueries({
        queryKey: walletTransactionsQueryKey,
      });
      toast.success(
        `Wallet provider set to ${walletProviderLabel(config.provider)}`,
      );
    },
  });
  const agentPaymentSettingsMutation = useMutation({
    mutationFn: setLightningWalletAgentPaymentSettings,
    onSuccess: (settings) => {
      queryClient.setQueryData(walletAgentPaymentSettingsQueryKey, settings);
      toast.success(
        settings.defaultAgentsToLexe
          ? "Agents default to Lexe payments"
          : "Agents no longer default to Lexe payments",
      );
    },
  });
  const summary = summaryQuery.data;
  const sourceConfig = sourceConfigQuery.data;
  const agentPaymentSettings = agentPaymentSettingsQuery.data;
  const providerOptions = sourceConfig?.availableProviders?.length
    ? sourceConfig.availableProviders
    : defaultWalletProviderOptions;
  const selectedProvider =
    sourceConfig?.provider ?? summary?.provider ?? "lexe";
  const selectedProviderOption = providerOptions.find(
    (option) => option.provider === selectedProvider,
  );
  const canConfigureWalletSource =
    selectedProviderOption?.capabilities.canConnectExistingWallet ?? true;
  const pendingProvider = providerMutation.variables?.provider;
  const pendingProviderLabel = walletProviderLabel(
    pendingProvider ?? selectedProvider,
  );
  const isMdkSelected = selectedProvider === "mdk";
  const mdkStatusQuery = useQuery({
    queryKey: mdkAgentWalletStatusQueryKey,
    queryFn: getMdkAgentWalletStatus,
    enabled: isMdkSelected && !providerMutation.isPending,
    staleTime: 10_000,
  });
  const mdkRestartMutation = useMutation({
    mutationFn: restartMdkAgentWalletDaemon,
    onSuccess: (status) => {
      queryClient.setQueryData(mdkAgentWalletStatusQueryKey, status);
      setMdkStatusCheckedAt(Date.now());
      void queryClient.invalidateQueries({ queryKey: walletSummaryQueryKey });
      void queryClient.invalidateQueries({
        queryKey: walletTransactionsQueryKey,
      });
      toast.success("MDK daemon restarted");
    },
    onSettled: () => {
      void queryClient.invalidateQueries({
        queryKey: mdkAgentWalletStatusQueryKey,
      });
    },
  });

  useEffect(() => {
    if (sourceConfig) {
      setSourceDraft(sourceConfig.source);
    }
  }, [sourceConfig]);

  function handleSourceChange(source: WalletSource) {
    setSourceDraft(source);
    if (source === "default" || sourceConfig?.hasExistingClientCredential) {
      sourceMutation.mutate({ source });
    }
  }

  function handleProviderChange(provider: WalletProvider) {
    if (provider !== sourceConfig?.provider) {
      providerMutation.mutate({ provider });
    }
  }

  function handleSaveExistingCredential() {
    sourceMutation.mutate({
      source: "existing",
      clientCredential,
    });
  }

  async function handleMdkStatusCheck() {
    const result = await mdkStatusQuery.refetch();
    setMdkStatusCheckedAt(Date.now());
    if (result.data) {
      toast.success(formatMdkStatusToast(result.data));
    } else if (result.error) {
      toast.error(errorMessage(result.error));
    }
  }

  function handleHideSeed() {
    setIsSeedVisible(false);
    seedMutation.reset();
  }

  return (
    <section className="min-w-0" data-testid="settings-lightning-wallet">
      <div className="mb-3 flex min-w-0 flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <h2 className="flex items-center gap-2 text-sm font-semibold tracking-tight">
            <WalletCards className="h-4 w-4" />
            Lightning Wallet
          </h2>
          <p className="text-sm text-muted-foreground">
            Wallet provider:{" "}
            {walletProviderLabel(
              sourceConfig?.provider ?? summary?.provider ?? "lexe",
            )}
          </p>
        </div>
        <Button
          disabled={refreshMutation.isPending || providerMutation.isPending}
          onClick={() => refreshMutation.mutate()}
          size="sm"
          type="button"
          variant="outline"
        >
          {refreshMutation.isPending ? (
            <Spinner className="h-4 w-4" />
          ) : (
            <RefreshCw className="h-4 w-4" />
          )}
          Refresh
        </Button>
      </div>

      <WalletProviderSettings
        actionError={providerMutation.error}
        config={sourceConfig}
        error={sourceConfigQuery.error}
        isLoading={sourceConfigQuery.isPending}
        isPending={providerMutation.isPending}
        onProviderChange={handleProviderChange}
        pendingProvider={pendingProvider}
      />

      {providerMutation.isPending ? (
        <div className="flex items-center gap-2 rounded-lg border border-border/70 bg-background/70 px-3 py-4 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          {pendingProvider === "mdk"
            ? "Provisioning new wallet..."
            : `Switching to ${pendingProviderLabel}...`}
        </div>
      ) : summaryQuery.isPending ? (
        <div className="flex items-center gap-2 rounded-lg border border-border/70 bg-background/70 px-3 py-4 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          Loading wallet...
        </div>
      ) : summaryQuery.isError ? (
        <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
          <span>{errorMessage(summaryQuery.error)}</span>
        </div>
      ) : summary ? (
        <SummaryContent summary={summary} />
      ) : null}

      {isMdkSelected && !providerMutation.isPending ? (
        <MdkAgentWalletControls
          error={mdkStatusQuery.error}
          isChecking={mdkStatusQuery.isFetching}
          isLoading={mdkStatusQuery.isPending}
          isRestarting={mdkRestartMutation.isPending}
          lastCheckedAt={mdkStatusCheckedAt}
          onCheckStatus={() => void handleMdkStatusCheck()}
          onRestart={() => mdkRestartMutation.mutate()}
          restartError={mdkRestartMutation.error}
          status={mdkStatusQuery.data}
        />
      ) : null}

      {canConfigureWalletSource ? (
        <WalletSourceSettings
          actionError={sourceMutation.error}
          clientCredential={clientCredential}
          config={sourceConfig}
          error={sourceConfigQuery.error}
          isLoading={sourceConfigQuery.isPending}
          isPending={sourceMutation.isPending}
          isSeedPending={seedMutation.isPending}
          onClientCredentialChange={setClientCredential}
          onHideSeed={handleHideSeed}
          onRevealSeed={() => seedMutation.mutate()}
          onSaveExistingCredential={handleSaveExistingCredential}
          onSourceChange={handleSourceChange}
          seedPhrase={isSeedVisible ? seedMutation.data : undefined}
          sourceDraft={sourceDraft}
        />
      ) : null}

      <AgentPaymentSettings
        checked={agentPaymentSettings?.defaultAgentsToLexe ?? true}
        error={
          agentPaymentSettingsQuery.error ?? agentPaymentSettingsMutation.error
        }
        isLoading={agentPaymentSettingsQuery.isPending}
        isPending={agentPaymentSettingsMutation.isPending}
        onCheckedChange={(defaultAgentsToLexe) =>
          agentPaymentSettingsMutation.mutate({ defaultAgentsToLexe })
        }
      />

      <div className="mt-4">
        <div className="mb-2 flex items-center gap-2">
          <Zap className="h-4 w-4" />
          <h3 className="text-sm font-medium">Recent transactions</h3>
        </div>
        <div className="max-h-72 overflow-y-auto rounded-lg border border-border/70 bg-background/70">
          {transactionsQuery.isPending && summaryQuery.isSuccess ? (
            <div className="flex items-center gap-2 px-3 py-4 text-sm text-muted-foreground">
              <Spinner className="h-4 w-4" />
              Loading transactions...
            </div>
          ) : transactionsQuery.isError ? (
            <div className="flex items-start gap-2 px-3 py-3 text-sm text-destructive">
              <TriangleAlert className="mt-0.5 h-4 w-4 shrink-0" />
              <span>{errorMessage(transactionsQuery.error)}</span>
            </div>
          ) : transactionsQuery.data?.length ? (
            transactionsQuery.data.map((tx) => {
              const notes = walletTransactionNotes(tx);

              return (
                <div
                  className="border-b border-border/60 px-3 py-2 last:border-b-0"
                  key={tx.id}
                >
                  <div className="flex items-center justify-between gap-3">
                    <p className="truncate text-sm font-medium">
                      {formatWalletTransactionTitle(tx)}
                    </p>
                    <p className="shrink-0 text-sm font-semibold">
                      {formatBitcoinAmount(tx.amountSats)}
                    </p>
                  </div>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground">
                    {tx.status}
                    {tx.feesSats > 0
                      ? ` · fee ${formatBitcoinAmount(tx.feesSats)}`
                      : ""}
                  </p>
                  {notes.length ? (
                    <p className="mt-1 break-words text-xs text-muted-foreground">
                      Note: {notes.join(" · ")}
                    </p>
                  ) : null}
                </div>
              );
            })
          ) : (
            <p className="px-3 py-4 text-sm text-muted-foreground">
              No recent transactions.
            </p>
          )}
        </div>
      </div>
    </section>
  );
}
