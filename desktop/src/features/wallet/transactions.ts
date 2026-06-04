import type { WalletTransaction } from "@/features/wallet/api";

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
  return (
    tx.kind.trim().toLowerCase() === "offer" ||
    tx.rail.trim().toLowerCase() === "offer"
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
    if (direction === "inbound") {
      return "incoming BOLT12 payment";
    }
    if (direction === "outbound") {
      return "outgoing BOLT12 payment";
    }
    return "BOLT12 payment";
  }

  return `${tx.direction} ${tx.kind}`.trim();
}

export function walletTransactionNotes(tx: WalletTransaction) {
  const notes = [
    displayAgentPaymentNote(tx),
    displayPaymentNote(tx.message),
    displayPaymentNote(tx.personalNote),
  ].filter((note): note is string => Boolean(note));

  return [...new Set(notes)];
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
