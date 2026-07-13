export type TimelineReaction = {
  emoji: string;
  /** Custom (image) emoji URL from the reaction's NIP-30 `emoji` tag, if any. */
  emojiUrl?: string;
  count: number;
  reactedByCurrentUser?: boolean;
  users: Array<{
    pubkey: string;
    displayName: string;
    avatarUrl: string | null;
  }>;
};

export type TimelineTipSummary = {
  amountSats: number;
  count: number;
  tippedByCurrentUser?: boolean;
  users: Array<{
    pubkey: string;
    displayName: string;
    avatarUrl: string | null;
    amountSats: number;
    tipId: string;
  }>;
};

export type TimelineMessageBounty = {
  amountSats: number;
  createdAt: number;
  initialAmountSats: number;
  lockedAmountSats: number | null;
  lockedResponseMessageId: string | null;
  recipientPubkey: string;
  paid: boolean;
  recipientIsCurrentUser: boolean;
};

export type TimelineMessageBountyPayment = {
  amountSats: number;
  bountyMessageId: string;
  recipientLabel: string;
  recipientPubkey: string;
  responseMessageId: string;
};

export type TimelineMessage = {
  id: string;
  /** Stable local key used to avoid remounting optimistic rows on send ack. */
  renderKey?: string;
  createdAt: number;
  pubkey?: string;
  /**
   * Raw signer pubkey (`event.pubkey`), normalized to lowercase hex.
   * Distinct from `pubkey`, which is the resolved display-author and may be
   * overridden by `actor` or `p` tags. Use this field — not `pubkey` — for
   * security-sensitive checks such as authenticating config-nudge cards.
   */
  signerPubkey?: string;
  author: string;
  avatarUrl?: string | null;
  role?: string;
  /** For bot messages, the display name of the persona this bot was created from. */
  personaDisplayName?: string;
  /** For bot messages, the respond-to mode (who can interact with this bot). */
  respondTo?: "owner-only" | "allowlist" | "anyone";
  time: string;
  body: string;
  parentId?: string | null;
  rootId?: string | null;
  depth: number;
  accent?: boolean;
  pending?: boolean;
  edited?: boolean;
  highlighted?: boolean;
  kudos?: boolean;
  kind?: number;
  tags?: string[][];
  reactions?: TimelineReaction[];
  tipSummary?: TimelineTipSummary;
  bounty?: TimelineMessageBounty;
  bountyPayment?: TimelineMessageBountyPayment;
};
