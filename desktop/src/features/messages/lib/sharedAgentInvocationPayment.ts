import type {
  Identity,
  ManagedAgent,
  Profile,
  RelayAgent,
  UserProfileSummary,
} from "@/shared/api/types";
import {
  resolveUserLabel,
  type UserProfileLookup,
} from "@/features/profile/lib/identity";
import { normalizePubkey } from "@/shared/lib/pubkey";

export const SHARED_AGENT_INVOCATION_AMOUNT_SATS = 50;

export type SharedAgentInvocationPaymentTarget = {
  agentPubkey: string;
  agentName: string;
  ownerPubkey: string;
  ownerLabel: string;
  invokerLabel: string;
  amountSats: number;
};

function formatBitcoinAmount(amountSats: number) {
  return `₿${new Intl.NumberFormat("en-US").format(amountSats)}`;
}

function profilesWithCurrentUser(
  profiles: UserProfileLookup | undefined,
  currentProfile: Pick<
    Profile,
    "pubkey" | "displayName" | "avatarUrl" | "nip05Handle" | "ownerPubkey"
  > | null,
) {
  if (!currentProfile) {
    return profiles;
  }

  return {
    ...(profiles ?? {}),
    [normalizePubkey(currentProfile.pubkey)]: {
      displayName: currentProfile.displayName,
      avatarUrl: currentProfile.avatarUrl,
      nip05Handle: currentProfile.nip05Handle,
      ownerPubkey: currentProfile.ownerPubkey,
    } satisfies UserProfileSummary,
  };
}

export function buildSharedAgentInvocationPaymentTarget({
  agentName,
  agentPubkey,
  currentIdentity,
  currentProfile,
  ownerPubkey,
  profiles,
}: {
  agentName?: string | null;
  agentPubkey: string;
  currentIdentity: Identity;
  currentProfile?: Pick<
    Profile,
    "pubkey" | "displayName" | "avatarUrl" | "nip05Handle" | "ownerPubkey"
  > | null;
  ownerPubkey: string | null | undefined;
  profiles?: UserProfileLookup;
}): SharedAgentInvocationPaymentTarget | null {
  const currentPubkey = normalizePubkey(currentIdentity.pubkey);
  const normalizedAgentPubkey = normalizePubkey(agentPubkey);
  const normalizedOwnerPubkey = ownerPubkey
    ? normalizePubkey(ownerPubkey)
    : null;
  if (
    !normalizedOwnerPubkey ||
    normalizedOwnerPubkey === currentPubkey ||
    normalizedAgentPubkey === currentPubkey
  ) {
    return null;
  }

  const profileLookup = profilesWithCurrentUser(
    profiles,
    currentProfile ?? null,
  );
  const invokerLabel = resolveUserLabel({
    pubkey: currentIdentity.pubkey,
    currentPubkey: currentIdentity.pubkey,
    fallbackName: currentIdentity.displayName,
    profiles: profileLookup,
    preferResolvedSelfLabel: true,
  });
  const resolvedAgentName = agentName?.trim()
    ? agentName.trim()
    : resolveUserLabel({
        pubkey: normalizedAgentPubkey,
        currentPubkey: currentIdentity.pubkey,
        profiles: profileLookup,
        preferResolvedSelfLabel: true,
      });

  return {
    agentPubkey: normalizedAgentPubkey,
    agentName: resolvedAgentName,
    ownerPubkey: normalizedOwnerPubkey,
    ownerLabel: resolveUserLabel({
      pubkey: normalizedOwnerPubkey,
      currentPubkey: currentIdentity.pubkey,
      profiles: profileLookup,
      preferResolvedSelfLabel: true,
    }),
    invokerLabel,
    amountSats: SHARED_AGENT_INVOCATION_AMOUNT_SATS,
  };
}

export function collectSharedAgentInvocationPaymentTargets({
  currentIdentity,
  currentProfile,
  managedAgents,
  mentionPubkeys,
  profiles,
  relayAgents,
}: {
  currentIdentity: Identity;
  currentProfile?: Pick<
    Profile,
    "pubkey" | "displayName" | "avatarUrl" | "nip05Handle" | "ownerPubkey"
  > | null;
  managedAgents: readonly ManagedAgent[];
  mentionPubkeys: readonly string[];
  profiles?: UserProfileLookup;
  relayAgents: readonly RelayAgent[];
}): SharedAgentInvocationPaymentTarget[] {
  const mentionedPubkeys = new Set(mentionPubkeys.map(normalizePubkey));
  const localAgentPubkeys = new Set(
    managedAgents.map((agent) => normalizePubkey(agent.pubkey)),
  );

  const targets: SharedAgentInvocationPaymentTarget[] = [];
  const chargedAgentPubkeys = new Set<string>();
  for (const agent of relayAgents) {
    const agentPubkey = normalizePubkey(agent.pubkey);
    if (
      chargedAgentPubkeys.has(agentPubkey) ||
      !mentionedPubkeys.has(agentPubkey) ||
      localAgentPubkeys.has(agentPubkey)
    ) {
      continue;
    }

    const target = buildSharedAgentInvocationPaymentTarget({
      agentPubkey,
      agentName: agent.name,
      currentIdentity,
      currentProfile,
      ownerPubkey: agent.ownerPubkey,
      profiles,
    });
    if (!target) {
      continue;
    }

    targets.push(target);
    chargedAgentPubkeys.add(agentPubkey);
  }

  return targets;
}

export function formatSharedAgentInvocationPaymentMessage(
  target: Pick<
    SharedAgentInvocationPaymentTarget,
    "agentName" | "amountSats" | "ownerLabel"
  >,
) {
  return `➡️ paid ${target.ownerLabel} ${formatBitcoinAmount(
    target.amountSats,
  )} to invoke ${target.agentName}`;
}
