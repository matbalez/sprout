import {
  formatBitcoinAmount,
  type WalletTransaction,
} from "@/features/wallet/api";

const SPROUT_TIP_PAYER_MESSAGE_PREFIX = "sprout-tip:v1:";
const SPROUT_MESSAGE_TIP_NOTE = "Sprout message tip";

function normalizedPaymentField(value: string | null | undefined) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function displayPaymentNote(value: string | null | undefined) {
  const note = normalizedPaymentField(value);
  if (!note) {
    return null;
  }

  if (note.startsWith(SPROUT_TIP_PAYER_MESSAGE_PREFIX)) {
    return SPROUT_MESSAGE_TIP_NOTE;
  }

  return note;
}

function isBolt12Payment(tx: WalletTransaction) {
  const rail = tx.rail.trim().toLowerCase();
  return (
    tx.kind.trim().toLowerCase() === "offer" ||
    rail === "offer" ||
    rail.includes("bolt12")
  );
}

export function formatWalletTransactionTitle(tx: WalletTransaction) {
  if (tx.agentPayment) {
    const protocol = tx.agentPayment.protocol.trim().toUpperCase();
    if (protocol === "L402" || protocol === "LSAT") {
      return "agent L402 payment";
    }
    return "agent Lightning payment";
  }

  const direction = tx.direction.trim().toLowerCase();

  if (isBolt12Payment(tx)) {
    if (direction === "inbound" || direction === "incoming") {
      return "inbound BOLT12 payment";
    }
    if (direction === "outbound" || direction === "outgoing") {
      return "outbound BOLT12 payment";
    }
    return "BOLT12 payment";
  }

  return `${tx.direction} ${tx.kind}`.trim();
}

export function formatWalletTransactionMeta(tx: WalletTransaction) {
  return [
    walletTransactionDirectionRail(tx),
    formatWalletTransactionTimestamp(tx.createdAtMs),
    normalizedPaymentField(tx.status),
    tx.feesSats > 0 ? `fee ${formatBitcoinAmount(tx.feesSats)}` : null,
  ]
    .filter((part): part is string => Boolean(part))
    .join(" · ");
}

export function walletTransactionNotes(tx: WalletTransaction) {
  const notes = [
    displayAgentPaymentNote(tx),
    displayPaymentNote(tx.message),
    displayPaymentNote(tx.personalNote),
  ].filter((note): note is string => Boolean(note));

  return [...new Set(notes)];
}

function walletTransactionDirectionRail(tx: WalletTransaction) {
  const direction = walletTransactionDirectionLabel(tx.direction);
  const rail = walletTransactionRailLabel(tx);
  return [direction, rail].filter(Boolean).join(" ");
}

function walletTransactionDirectionLabel(value: string) {
  const direction = value.trim().toLowerCase();
  if (direction === "inbound" || direction === "incoming") {
    return "Inbound";
  }
  if (direction === "outbound" || direction === "outgoing") {
    return "Outbound";
  }
  return direction ? sentenceCase(direction) : null;
}

function walletTransactionRailLabel(tx: WalletTransaction) {
  const kind = tx.kind.trim().toLowerCase();
  const rail = tx.rail.trim().toLowerCase();
  if (kind === "offer" || rail.includes("bolt12")) {
    return "BOLT12 payment";
  }
  if (kind === "invoice" || rail.includes("bolt11")) {
    return "BOLT11 payment";
  }
  if (rail.includes("cashu-token")) {
    return "Cashu token";
  }
  return sentenceCase(tx.kind.trim() || tx.rail.trim());
}

function formatWalletTransactionTimestamp(createdAtMs: number) {
  if (!Number.isFinite(createdAtMs) || createdAtMs <= 0) {
    return null;
  }
  return new Date(createdAtMs).toLocaleString([], {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function sentenceCase(value: string) {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  return `${trimmed.charAt(0).toUpperCase()}${trimmed.slice(1)}`;
}

function displayAgentPaymentNote(tx: WalletTransaction) {
  const annotation = tx.agentPayment;
  if (!annotation) {
    return null;
  }

  const agent = normalizedPaymentField(annotation.agentName);
  const host = normalizedPaymentField(annotation.endpointHost);
  const path = normalizedPaymentField(annotation.endpointPath);
  const endpoint = host
    ? `${host}${path && path !== "/" ? path : ""}`
    : normalizedPaymentField(annotation.endpoint);
  const protocol = normalizedPaymentField(annotation.protocol) ?? "Lightning";

  return [
    agent ? `Agent: ${agent}` : "Agent payment",
    endpoint ? `${protocol}: ${endpoint}` : protocol,
  ].join(" · ");
}
