import { invokeTauri } from "@/shared/api/tauri";
import type { Channel, RelayEvent } from "@/shared/api/types";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

export const WALLETBOT_CHANNEL_ID = "walletbot-local";
export const WALLETBOT_PUBKEY =
  "0000000000000000000000000000000000000000000000000000000000000001";
export const WALLETBOT_MESSAGES_UPDATED = "walletbot-messages-updated";

export type WalletSummary = {
  provider: WalletProvider;
  walletSource: WalletSource;
  hasExistingClientCredential: boolean;
  env: string;
  seedPath: string;
  existingClientCredentialPath: string;
  balanceSats: number;
  lightningBalanceSats: number;
  lightningSendableBalanceSats: number;
  lightningMaxSendableBalanceSats: number;
  onchainBalanceSats: number;
  onchainTrustedBalanceSats: number;
  numChannels: number;
  numUsableChannels: number;
  bolt12Offer: string;
};

export type WalletProvider = "lexe" | "mdk";

export type WalletProviderCapabilities = {
  canCreateWallet: boolean;
  canConnectExistingWallet: boolean;
  canReceiveReusableBolt12: boolean;
  canSendBolt12: boolean;
  canGetBalance: boolean;
  canListPayments: boolean;
  canSubscribePayments: boolean;
  canSendBolt11: boolean;
  canCreateBolt11Invoice: boolean;
  canPayWithPreimage: boolean;
};

export type WalletProviderOption = {
  provider: WalletProvider;
  label: string;
  paymentRail: string;
  available: boolean;
  capabilities: WalletProviderCapabilities;
};

export type MdkAgentWalletStatus = {
  running: boolean;
  pid: number | null;
  port: number | null;
  expectedPort: number;
  healthy: boolean;
  nodeRunning: boolean;
  healthError: string | null;
  homeDir: string;
  logPath: string;
};

export type WalletSource = "default" | "existing";

export type WalletSourceConfig = {
  provider: WalletProvider;
  availableProviders: WalletProviderOption[];
  source: WalletSource;
  seedPath: string;
  existingClientCredentialPath: string;
  hasExistingClientCredential: boolean;
};

export type WalletTransaction = {
  id: string;
  rail: string;
  kind: string;
  direction: string;
  status: string;
  statusMessage: string;
  amountSats: number | null;
  feesSats: number;
  message: string | null;
  personalNote: string | null;
  createdAtMs: number;
  updatedAtMs: number;
  agentPayment: WalletAgentPaymentAnnotation | null;
};

export type WalletAgentPaymentAnnotation = {
  paymentId: string;
  agentPubkey: string | null;
  agentName: string | null;
  protocol: string;
  endpoint: string | null;
  endpointHost: string | null;
  endpointPath: string | null;
  consentEventId: string | null;
  status: string;
  statusMessage: string | null;
  amountSats: number | null;
  feesSats: number | null;
  paymentHash: string | null;
  createdAtMs: number;
  updatedAtMs: number;
};

export type WalletAgentPaymentSettings = {
  defaultAgentsToLexe: boolean;
};

export type WalletBotMessage = {
  id: string;
  role: "user" | "bot";
  authorPubkey: string;
  content: string;
  createdAt: number;
};

export type WalletPaymentResult = {
  paymentId: string;
  amountSats: number;
};

export type HiveChannelPayoutShare = {
  revenuePaymentId: string;
  revenueAmountSats: number;
  revenueCreatedAtMs: number;
  amountSats: number;
};

export type HiveChannelPayoutRecipient = {
  memberPubkey: string;
  amountSats: number;
  bolt12Offer: string | null;
  shares: HiveChannelPayoutShare[];
};

export type HiveChannelPayoutPreview = {
  channelId: string;
  totalUnattributedRevenueSats: number;
  totalPayoutSats: number;
  unpaidRevenueCount: number;
  skippedNoOwnerRevenueCount: number;
  alreadyPaidShareCount: number;
  recipients: HiveChannelPayoutRecipient[];
};

export type HiveChannelPayoutPayment = {
  memberPubkey: string;
  amountSats: number;
  paymentId: string;
  messageEventId: string | null;
};

export type HiveChannelPayoutFailure = {
  memberPubkey: string | null;
  amountSats: number | null;
  error: string;
};

export type HiveChannelPayoutExecution = {
  channelId: string;
  status: "completed" | "partial" | "blocked" | "nothing_to_pay";
  totalPaidSats: number;
  paid: HiveChannelPayoutPayment[];
  failed: HiveChannelPayoutFailure | null;
  remainingPreview: HiveChannelPayoutPreview;
};

export type HiveChannelContributionShare = {
  memberPubkey: string | null;
  amountSats: number;
  ownershipPercent: number;
};

export type HiveChannelWalletSummary = {
  channelId: string;
  walletProvider: WalletProvider;
  hasLocalSeed: boolean;
  seedPath: string;
  balanceSats: number;
  lightningBalanceSats: number;
  lightningSendableBalanceSats: number;
  onchainBalanceSats: number;
  bolt12Offer: string;
  totalContributedSats: number;
  ownershipShares: HiveChannelContributionShare[];
};

export type MessageTipResult = {
  paymentId: string;
  amountSats: number;
  tipId: string;
  receiptEventId: string | null;
  receiptAccepted: boolean;
  receiptError: string | null;
};

export type ChannelPaymentResult = {
  paymentId: string;
  amountSats: number;
  nonce: string;
  receiptEventId: string | null;
  receiptAccepted: boolean;
  receiptError: string | null;
};

export type WalletBotMessagesPayload = {
  messages: WalletBotMessage[];
};

export function isWalletBotChannelId(channelId: string | null | undefined) {
  return channelId === WALLETBOT_CHANNEL_ID;
}

export function isWalletBotChannel(channel: Channel | null | undefined) {
  return isWalletBotChannelId(channel?.id);
}

export function walletProviderLabel(provider: WalletProvider) {
  switch (provider) {
    case "lexe":
      return "Lexe";
    case "mdk":
      return "MDK Agent Wallet";
  }
}

export function walletBotChannel(): Channel {
  return {
    metadataEventId: "walletbot",
    id: WALLETBOT_CHANNEL_ID,
    name: "WalletBot",
    channelType: "dm",
    visibility: "private",
    description: "Local Lightning wallet assistant",
    topic: null,
    purpose: null,
    memberCount: 2,
    memberPubkeys: [],
    lastMessageAt: null,
    archivedAt: null,
    participants: ["WalletBot"],
    participantPubkeys: [WALLETBOT_PUBKEY],
    isMember: true,
    currentUserRole: "member",
    ttlSeconds: null,
    ttlDeadline: null,
    paymentPolicy: null,
    hiveChannel: false,
    hiveWalletBolt12Offer: null,
  };
}

export function withWalletBotChannel(channels: Channel[]) {
  if (channels.some((channel) => channel.id === WALLETBOT_CHANNEL_ID)) {
    return channels;
  }

  return [...channels, walletBotChannel()];
}

export function walletBotMessageToRelayEvent(message: WalletBotMessage) {
  const authorPubkey =
    message.role === "bot" ? WALLETBOT_PUBKEY : message.authorPubkey;

  return {
    id: message.id,
    pubkey: authorPubkey,
    created_at: message.createdAt,
    kind: KIND_STREAM_MESSAGE,
    tags: [
      ["h", WALLETBOT_CHANNEL_ID],
      ["p", authorPubkey],
    ],
    content: message.content,
    sig: "",
  } satisfies RelayEvent;
}

export function walletBotMessagesToRelayEvents(messages: WalletBotMessage[]) {
  return messages.map(walletBotMessageToRelayEvent);
}

export function formatBitcoinAmount(sats: number | null | undefined) {
  if (typeof sats !== "number") {
    return "amountless";
  }

  return `₿${new Intl.NumberFormat("en-US").format(sats)}`;
}

export function getLightningWalletSummary() {
  return invokeTauri<WalletSummary>("get_lightning_wallet_summary");
}

export function refreshLightningWallet() {
  return invokeTauri<WalletSummary>("refresh_lightning_wallet");
}

export function getLightningWalletSourceConfig() {
  return invokeTauri<WalletSourceConfig>("get_lightning_wallet_source_config");
}

export function getLightningWalletAgentPaymentSettings() {
  return invokeTauri<WalletAgentPaymentSettings>(
    "get_lightning_wallet_agent_payment_settings",
  );
}

export function setLightningWalletAgentPaymentSettings(input: {
  defaultAgentsToLexe: boolean;
}) {
  return invokeTauri<WalletAgentPaymentSettings>(
    "set_lightning_wallet_agent_payment_settings",
    input,
  );
}

export function setLightningWalletProvider(input: {
  provider: WalletProvider;
}) {
  return invokeTauri<WalletSourceConfig>(
    "set_lightning_wallet_provider",
    input,
  );
}

export function getMdkAgentWalletStatus() {
  return invokeTauri<MdkAgentWalletStatus>("get_mdk_agent_wallet_status");
}

export function restartMdkAgentWalletDaemon() {
  return invokeTauri<MdkAgentWalletStatus>("restart_mdk_agent_wallet_daemon");
}

export function setLightningWalletSource(input: {
  source: WalletSource;
  clientCredential?: string;
}) {
  return invokeTauri<WalletSourceConfig>("set_lightning_wallet_source", input);
}

export function revealLightningWalletSeed() {
  return invokeTauri<string>("reveal_lightning_wallet_seed");
}

export function getLightningWalletTransactions(limit = 20) {
  return invokeTauri<WalletTransaction[]>("get_lightning_wallet_transactions", {
    limit,
  });
}

export function getUserWalletBolt12Offer(pubkey: string) {
  return invokeTauri<string | null>("get_user_wallet_bolt12_offer", {
    pubkey,
  });
}

export function sendLightningWalletPayment(
  amountSats: number,
  payable: string,
) {
  return invokeTauri<WalletPaymentResult>("send_lightning_wallet_payment", {
    amountSats,
    payable,
  });
}

export function getHiveChannelWalletSummary(channelId: string) {
  return invokeTauri<HiveChannelWalletSummary>(
    "get_hive_channel_wallet_summary",
    { channelId },
  );
}

export function getHiveChannelWalletTransactions(
  channelId: string,
  limit = 20,
) {
  return invokeTauri<WalletTransaction[]>(
    "get_hive_channel_wallet_transactions",
    { channelId, limit },
  );
}

export function generateHiveChannelWalletBolt12Offer(channelId: string) {
  return invokeTauri<HiveChannelWalletSummary>(
    "generate_hive_channel_wallet_bolt12_offer",
    { channelId },
  );
}

export function previewHiveChannelWalletPayouts(channelId: string) {
  return invokeTauri<HiveChannelPayoutPreview>(
    "preview_hive_channel_wallet_payouts",
    { channelId },
  );
}

export function executeHiveChannelWalletPayouts(channelId: string) {
  return invokeTauri<HiveChannelPayoutExecution>(
    "execute_hive_channel_wallet_payouts",
    { channelId },
  );
}

export function revealHiveChannelWalletSeed(channelId: string) {
  return invokeTauri<string>("reveal_hive_channel_wallet_seed", { channelId });
}

export function sendHiveChannelFunds(input: {
  channelId: string;
  amountSats: number;
}) {
  return invokeTauri<WalletPaymentResult>("send_hive_channel_funds", input);
}

export function sendHiveChannelWalletPayment(input: {
  channelId: string;
  amountSats: number;
  payable: string;
}) {
  return invokeTauri<WalletPaymentResult>(
    "send_hive_channel_wallet_payment",
    input,
  );
}

export function sendMessageKudos(input: {
  channelId: string;
  recipientPubkey: string;
}) {
  return invokeTauri<WalletPaymentResult>("send_message_kudos", input);
}

export function sendMessageTip(input: {
  channelId: string;
  messageId: string;
  recipientPubkey: string;
}) {
  return invokeTauri<MessageTipResult>("send_message_tip", input);
}

export function sendChannelPayment(input: {
  channelId: string;
  metadataEventId: string;
  recipientPubkey: string;
  bolt12Offer: string;
  amountSats: number;
  purpose: "join" | "post";
}) {
  return invokeTauri<ChannelPaymentResult>("send_channel_payment", input);
}

export function sendSharedAgentInvocationPayment(input: {
  channelId: string;
  ownerPubkey: string;
}) {
  return invokeTauri<WalletPaymentResult>(
    "send_shared_agent_invocation_payment",
    input,
  );
}

export function getWalletBotMessages() {
  return invokeTauri<WalletBotMessage[]>("get_walletbot_messages");
}

export function sendWalletBotCommand(content: string) {
  return invokeTauri<WalletBotMessage[]>("send_walletbot_command", {
    content,
  });
}
