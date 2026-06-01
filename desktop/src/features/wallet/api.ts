import { invokeTauri } from "@/shared/api/tauri";
import type { Channel, RelayEvent } from "@/shared/api/types";
import { KIND_STREAM_MESSAGE } from "@/shared/constants/kinds";

export const WALLETBOT_CHANNEL_ID = "walletbot-local";
export const WALLETBOT_PUBKEY =
  "0000000000000000000000000000000000000000000000000000000000000001";
export const WALLETBOT_MESSAGES_UPDATED = "walletbot-messages-updated";

export type WalletSummary = {
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

export type WalletSource = "default" | "existing";

export type WalletSourceConfig = {
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

export type MessageTipResult = {
  paymentId: string;
  amountSats: number;
  tipId: string;
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

export function walletBotChannel(): Channel {
  return {
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
    ttlSeconds: null,
    ttlDeadline: null,
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
