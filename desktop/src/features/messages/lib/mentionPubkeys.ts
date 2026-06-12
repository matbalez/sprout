import type { ChannelMember } from "@/shared/api/types";

type MentionMember = Pick<ChannelMember, "displayName" | "pubkey">;

type MentionCandidate = {
  displayName: string;
  displayNameLower: string;
  order: number;
  pubkey: string;
  source: "selected" | "member";
};

function normalizeDisplayName(name: string) {
  return name.replace(/\s+/g, " ").trim().toLowerCase();
}

function normalizePubkey(pubkey: string) {
  return pubkey.trim().toLowerCase();
}

function hasMentionPrefix(text: string, atIndex: number) {
  if (atIndex === 0) {
    return true;
  }

  if (/\s/.test(text[atIndex - 1])) {
    return true;
  }

  for (const marker of ["***", "___", "**", "__", "*", "_"]) {
    if (text.slice(atIndex - marker.length, atIndex) === marker) {
      return true;
    }
  }

  return false;
}

function hasMentionBoundary(text: string, index: number) {
  return index >= text.length || /[\s,;.!?:)\]}*_]/.test(text[index]);
}

function isBetterCandidate(
  candidate: MentionCandidate,
  best: MentionCandidate | null,
) {
  if (!best) {
    return true;
  }

  if (candidate.displayName.length !== best.displayName.length) {
    return candidate.displayName.length > best.displayName.length;
  }

  if (candidate.source !== best.source) {
    return candidate.source === "selected";
  }

  return candidate.order < best.order;
}

function findMentionCandidateAt(
  text: string,
  atIndex: number,
  candidates: readonly MentionCandidate[],
) {
  if (text[atIndex] !== "@" || !hasMentionPrefix(text, atIndex)) {
    return null;
  }

  const mentionTextLower = text.slice(atIndex + 1).toLowerCase();
  let best: MentionCandidate | null = null;

  for (const candidate of candidates) {
    if (!mentionTextLower.startsWith(candidate.displayNameLower)) {
      continue;
    }

    const endIndex = atIndex + 1 + candidate.displayName.length;
    if (!hasMentionBoundary(text, endIndex)) {
      continue;
    }

    if (isBetterCandidate(candidate, best)) {
      best = candidate;
    }
  }

  return best;
}

export function extractMentionPubkeysFromText({
  excludedDisplayNames = [],
  managedAgentNamesByPubkey = new Map(),
  members = [],
  mentionMap,
  text,
}: {
  excludedDisplayNames?: Iterable<string>;
  managedAgentNamesByPubkey?: ReadonlyMap<string, string>;
  members?: readonly MentionMember[];
  mentionMap: ReadonlyMap<string, string>;
  text: string;
}) {
  const pubkeys: string[] = [];
  const candidates: MentionCandidate[] = [];
  const displayNames = new Set<string>();

  for (const displayName of excludedDisplayNames) {
    const normalized = normalizeDisplayName(displayName);
    if (normalized) {
      displayNames.add(normalized);
    }
  }

  const pushPubkey = (pubkey: string) => {
    const normalized = normalizePubkey(pubkey);
    if (normalized && !pubkeys.includes(normalized)) {
      pubkeys.push(normalized);
    }
  };

  const addCandidate = (
    displayName: string,
    pubkey: string,
    source: MentionCandidate["source"],
  ) => {
    const normalizedDisplayName = normalizeDisplayName(displayName);
    if (!normalizedDisplayName || displayNames.has(normalizedDisplayName)) {
      return;
    }

    const normalizedPubkey = normalizePubkey(pubkey);
    if (!normalizedPubkey) {
      return;
    }

    displayNames.add(normalizedDisplayName);
    candidates.push({
      displayName,
      displayNameLower: displayName.toLowerCase(),
      order: candidates.length,
      pubkey: normalizedPubkey,
      source,
    });
  };

  for (const [displayName, pubkey] of mentionMap) {
    addCandidate(displayName, pubkey, "selected");
  }

  for (const member of members) {
    const memberPubkey = normalizePubkey(member.pubkey);
    const name =
      member.displayName ?? managedAgentNamesByPubkey.get(memberPubkey);
    if (!name) {
      continue;
    }

    addCandidate(name, member.pubkey, "member");
  }

  for (let index = 0; index < text.length; index += 1) {
    if (text[index] !== "@") {
      continue;
    }

    const candidate = findMentionCandidateAt(text, index, candidates);
    if (!candidate) {
      continue;
    }

    pushPubkey(candidate.pubkey);
    index += candidate.displayName.length;
  }

  return pubkeys;
}
