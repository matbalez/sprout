import type { RelayEvent } from "@/shared/api/types";

const HEX_RE = /^[0-9a-f]+$/i;
const BOUNTY_TAG = ["sprout", "message-bounty", "v1"] as const;
const BOUNTY_PAID_TAG = ["sprout", "message-bounty-paid", "v1"] as const;
const BOUNTY_DECAY_INTERVAL_SECONDS = 5 * 60;
const BOUNTY_DECAY_PERCENT_PER_STEP = 5;
const BOUNTY_RESIDUAL_PERCENT = 25;

export type ParsedMessageBounty = {
  amountSats: number;
  recipientPubkey: string;
};

export type MessageBounty = ParsedMessageBounty & {
  createdAt: number;
  initialAmountSats: number;
  lockedAmountSats: number | null;
  lockedResponseMessageId: string | null;
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

function percentFloor(amountSats: number, percent: number) {
  return Number((BigInt(amountSats) * BigInt(percent)) / 100n);
}

export function getMessageBountyResidualAmount(initialAmountSats: number) {
  if (!Number.isSafeInteger(initialAmountSats) || initialAmountSats <= 0) {
    return 0;
  }

  return Math.max(1, percentFloor(initialAmountSats, BOUNTY_RESIDUAL_PERCENT));
}

export function getDecayedMessageBountyAmount(input: {
  createdAt: number;
  initialAmountSats: number;
  now: number;
}) {
  const { createdAt, initialAmountSats, now } = input;
  if (!Number.isSafeInteger(initialAmountSats) || initialAmountSats <= 0) {
    return 0;
  }

  const elapsedSeconds = Math.max(0, Math.floor(now - createdAt));
  const steps = Math.floor(elapsedSeconds / BOUNTY_DECAY_INTERVAL_SECONDS);
  const remainingPercent = Math.max(
    BOUNTY_RESIDUAL_PERCENT,
    100 - steps * BOUNTY_DECAY_PERCENT_PER_STEP,
  );

  return Math.max(
    getMessageBountyResidualAmount(initialAmountSats),
    percentFloor(initialAmountSats, remainingPercent),
  );
}

export function getDisplayMessageBountyAmount(
  bounty: {
    amountSats: number;
    createdAt?: number;
    initialAmountSats?: number;
    lockedAmountSats?: number | null;
  },
  now: number,
) {
  if (typeof bounty.lockedAmountSats === "number") {
    return bounty.lockedAmountSats;
  }

  if (
    typeof bounty.initialAmountSats === "number" &&
    typeof bounty.createdAt === "number"
  ) {
    return getDecayedMessageBountyAmount({
      createdAt: bounty.createdAt,
      initialAmountSats: bounty.initialAmountSats,
      now,
    });
  }

  return bounty.amountSats;
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
): ParsedMessageBounty | null {
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
