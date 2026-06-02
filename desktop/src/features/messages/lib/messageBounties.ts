import type { RelayEvent } from "@/shared/api/types";

const HEX_RE = /^[0-9a-f]+$/i;
const BOUNTY_TAG = ["sprout", "message-bounty", "v1"] as const;
const BOUNTY_PAID_TAG = ["sprout", "message-bounty-paid", "v1"] as const;

export type MessageBounty = {
  amountSats: number;
  recipientPubkey: string;
  paid: boolean;
};

export type MessageBountyPaidReceipt = {
  bountyMessageId: string;
  responseMessageId: string;
  amountSats: number;
  recipientPubkey: string;
};

export type MessageBountyPaymentAction = {
  amountSats: number;
  bountyMessageId: string;
  recipientLabel: string;
  recipientPubkey: string;
  responseMessageId: string;
};

function isHexEventId(value: string | undefined) {
  return typeof value === "string" && value.length === 64 && HEX_RE.test(value);
}

function isHexPubkey(value: string | undefined) {
  return typeof value === "string" && value.length === 64 && HEX_RE.test(value);
}

export function parseBountyAmount(value: string | undefined) {
  if (!value || !/^[1-9][0-9]*$/.test(value)) {
    return null;
  }

  const amount = Number.parseInt(value, 10);
  return Number.isSafeInteger(amount) ? amount : null;
}

export function parseBountyAmountInput(raw: string) {
  const normalized = raw.replace(/^₿\s*/, "").replace(/,/g, "").trim();
  if (!/^[1-9][0-9]*$/.test(normalized)) {
    throw new Error("Enter a whole ₿ amount.");
  }

  const amount = Number.parseInt(normalized, 10);
  if (!Number.isSafeInteger(amount)) {
    throw new Error("That bounty amount is too large.");
  }

  return amount;
}

export function formatBountyAmount(amountSats: number) {
  return `₿${new Intl.NumberFormat("en-US").format(amountSats)}`;
}

export function buildMessageBountyTag(input: {
  amountSats: number;
  recipientPubkey: string;
}): string[] {
  return [
    ...BOUNTY_TAG,
    String(input.amountSats),
    input.recipientPubkey.toLowerCase(),
  ];
}

export function parseMessageBountyTags(
  tags: string[][] | undefined,
): Omit<MessageBounty, "paid"> | null {
  const tag = tags?.find(
    (candidate) =>
      candidate[0] === BOUNTY_TAG[0] &&
      candidate[1] === BOUNTY_TAG[1] &&
      candidate[2] === BOUNTY_TAG[2],
  );
  if (!tag) {
    return null;
  }

  const amountSats = parseBountyAmount(tag[3]);
  const recipientPubkey = tag[4]?.toLowerCase();
  if (amountSats === null || !isHexPubkey(recipientPubkey)) {
    return null;
  }

  return {
    amountSats,
    recipientPubkey,
  };
}

export function buildMessageBountyPaidTag(input: {
  amountSats: number;
  bountyMessageId: string;
  recipientPubkey: string;
  responseMessageId: string;
}): string[] {
  return [
    ...BOUNTY_PAID_TAG,
    input.bountyMessageId,
    input.responseMessageId,
    String(input.amountSats),
    input.recipientPubkey.toLowerCase(),
  ];
}

export function parseMessageBountyPaidTags(
  tags: string[][] | undefined,
): MessageBountyPaidReceipt | null {
  const tag = tags?.find(
    (candidate) =>
      candidate[0] === BOUNTY_PAID_TAG[0] &&
      candidate[1] === BOUNTY_PAID_TAG[1] &&
      candidate[2] === BOUNTY_PAID_TAG[2],
  );
  if (!tag) {
    return null;
  }

  const amountSats = parseBountyAmount(tag[5]);
  const recipientPubkey = tag[6]?.toLowerCase();
  if (
    !isHexEventId(tag[3]) ||
    !isHexEventId(tag[4]) ||
    amountSats === null ||
    !isHexPubkey(recipientPubkey)
  ) {
    return null;
  }

  return {
    bountyMessageId: tag[3],
    responseMessageId: tag[4],
    amountSats,
    recipientPubkey,
  };
}

export function isMessageBountyPaidEvent(event: RelayEvent) {
  return parseMessageBountyPaidTags(event.tags) !== null;
}

export function resolveBountyTargetPubkey(mentionPubkeys: string[]) {
  const targets = [
    ...new Set(
      mentionPubkeys
        .map((pubkey) => pubkey.trim().toLowerCase())
        .filter(Boolean),
    ),
  ];

  if (targets.length !== 1) {
    throw new Error("Message bounties require exactly one @mention.");
  }

  const target = targets[0];
  if (!isHexPubkey(target)) {
    throw new Error("Message bounties require a valid @mention.");
  }

  return target;
}

export function formatBountyConfirmation(input: {
  amountSats: number;
  recipientLabel: string;
}) {
  const recipientLabel =
    input.recipientLabel
      .replace(/[\r\n\t]+/g, " ")
      .replace(/\s+/g, " ")
      .replace(/^@+/, "")
      .trim() || "recipient";
  return `➡️ paid @${recipientLabel} ${formatBountyAmount(input.amountSats)} bounty`;
}
