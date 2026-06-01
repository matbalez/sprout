import type { Channel } from "@/shared/api/types";

const STORAGE_KEY = "sprout.klaimGifts.v1";
export const KLAIM_GIFT_SETTINGS_CHANGED = "klaim-gift-settings-changed";

export type KlaimGiftMemberStatus =
  | "existing"
  | "pending"
  | "missing-bolt12"
  | "paid"
  | "already-paid"
  | "max-claims"
  | "in-progress"
  | "retryable"
  | "uncertain"
  | "failed";

export type KlaimGiftMemberState = {
  pubkey: string;
  status: KlaimGiftMemberStatus;
  updatedAt: number;
  amountSats?: number | null;
  claimsUsed?: number | null;
  maxClaims?: number | null;
  error?: string | null;
};

export type KlaimGiftChannelConfig = {
  channelId: string;
  enabled: boolean;
  campaignId: string;
  klaimChannelId: string;
  defaultAmountSats: number | null;
  maxClaims: number | null;
  claimsUsed: number | null;
  registeredAt: number;
  updatedAt: number;
  members: Record<string, KlaimGiftMemberState>;
};

export type KlaimGiftState = {
  channels: Record<string, KlaimGiftChannelConfig>;
};

function emptyState(): KlaimGiftState {
  return { channels: {} };
}

function nowMs() {
  return Date.now();
}

function normalizePubkey(pubkey: string) {
  return pubkey.trim().toLowerCase();
}

function canUseLocalStorage() {
  return typeof window !== "undefined" && Boolean(window.localStorage);
}

export function loadKlaimGiftState(): KlaimGiftState {
  if (!canUseLocalStorage()) {
    return emptyState();
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return emptyState();
    }

    const parsed = JSON.parse(raw) as Partial<KlaimGiftState>;
    if (!parsed || typeof parsed !== "object" || !parsed.channels) {
      return emptyState();
    }

    return {
      channels: Object.fromEntries(
        Object.entries(parsed.channels).filter(
          ([channelId, config]) =>
            typeof channelId === "string" &&
            config &&
            typeof config === "object" &&
            typeof config.channelId === "string",
        ),
      ),
    };
  } catch {
    return emptyState();
  }
}

function saveKlaimGiftState(state: KlaimGiftState) {
  if (!canUseLocalStorage()) {
    return;
  }

  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  window.dispatchEvent(new CustomEvent(KLAIM_GIFT_SETTINGS_CHANGED));
}

export function subscribeKlaimGiftSettings(listener: () => void) {
  window.addEventListener(KLAIM_GIFT_SETTINGS_CHANGED, listener);
  return () =>
    window.removeEventListener(KLAIM_GIFT_SETTINGS_CHANGED, listener);
}

export function getKlaimGiftConfig(channelId: string | null | undefined) {
  if (!channelId) {
    return null;
  }

  return loadKlaimGiftState().channels[channelId] ?? null;
}

export function listEnabledKlaimGiftConfigs() {
  return Object.values(loadKlaimGiftState().channels).filter(
    (config) => config.enabled,
  );
}

export function saveKlaimGiftConfig(
  config: Omit<KlaimGiftChannelConfig, "updatedAt"> & {
    updatedAt?: number;
  },
) {
  const state = loadKlaimGiftState();
  state.channels[config.channelId] = {
    ...config,
    updatedAt: config.updatedAt ?? nowMs(),
  };
  saveKlaimGiftState(state);
}

export function disableKlaimGiftConfig(channelId: string) {
  const state = loadKlaimGiftState();
  const current = state.channels[channelId];
  if (!current) {
    return;
  }

  state.channels[channelId] = {
    ...current,
    enabled: false,
    updatedAt: nowMs(),
  };
  saveKlaimGiftState(state);
}

export function buildInitialKlaimGiftConfig(input: {
  campaignId: string;
  channel: Pick<Channel, "id" | "memberPubkeys">;
  defaultAmountSats: number | null;
  maxClaims: number | null;
}) {
  const timestamp = nowMs();
  const members = Object.fromEntries(
    input.channel.memberPubkeys.map((pubkey) => {
      const normalized = normalizePubkey(pubkey);
      return [
        normalized,
        {
          pubkey: normalized,
          status: "existing" as const,
          updatedAt: timestamp,
        },
      ];
    }),
  );

  return {
    channelId: input.channel.id,
    enabled: true,
    campaignId: input.campaignId.trim(),
    klaimChannelId: input.channel.id,
    defaultAmountSats: input.defaultAmountSats,
    maxClaims: input.maxClaims,
    claimsUsed: null,
    registeredAt: timestamp,
    updatedAt: timestamp,
    members,
  } satisfies KlaimGiftChannelConfig;
}

export function markKlaimGiftMember(input: {
  channelId: string;
  pubkey: string;
  status: KlaimGiftMemberStatus;
  amountSats?: number | null;
  claimsUsed?: number | null;
  maxClaims?: number | null;
  error?: string | null;
}) {
  const state = loadKlaimGiftState();
  const config = state.channels[input.channelId];
  if (!config) {
    return;
  }

  const pubkey = normalizePubkey(input.pubkey);
  config.members[pubkey] = {
    pubkey,
    status: input.status,
    updatedAt: nowMs(),
    amountSats: input.amountSats ?? null,
    claimsUsed: input.claimsUsed ?? null,
    maxClaims: input.maxClaims ?? null,
    error: input.error ?? null,
  };
  config.updatedAt = nowMs();
  if (typeof input.claimsUsed === "number") {
    config.claimsUsed = input.claimsUsed;
  }
  if (typeof input.maxClaims === "number") {
    config.maxClaims = input.maxClaims;
  }
  saveKlaimGiftState(state);
}

export function ensureKlaimGiftPendingMembers(channel: Channel) {
  const state = loadKlaimGiftState();
  const config = state.channels[channel.id];
  if (!config?.enabled) {
    return [];
  }

  const added: string[] = [];
  for (const rawPubkey of channel.memberPubkeys) {
    const pubkey = normalizePubkey(rawPubkey);
    if (config.members[pubkey]) {
      continue;
    }

    config.members[pubkey] = {
      pubkey,
      status: "pending",
      updatedAt: nowMs(),
    };
    added.push(pubkey);
  }

  if (added.length > 0) {
    config.updatedAt = nowMs();
    saveKlaimGiftState(state);
  }

  return added;
}

export function isKlaimGiftProcessableStatus(
  status: KlaimGiftMemberStatus | undefined,
) {
  return (
    status === "pending" ||
    status === "missing-bolt12" ||
    status === "retryable"
  );
}
