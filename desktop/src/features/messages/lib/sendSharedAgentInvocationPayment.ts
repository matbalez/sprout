import type { QueryClient } from "@tanstack/react-query";

import { sendSharedAgentInvocationPayment } from "@/features/wallet/api";
import { relayClient } from "@/shared/api/relayClient";
import { resolveSharedAgentOwner } from "@/shared/api/tauri";
import type {
  Identity,
  ManagedAgent,
  Profile,
  RelayAgent,
} from "@/shared/api/types";
import type { UserProfileLookup } from "@/features/profile/lib/identity";
import {
  buildSharedAgentInvocationPaymentTarget,
  collectSharedAgentInvocationPaymentTargets,
  formatSharedAgentInvocationPaymentMessage,
  type SharedAgentInvocationPaymentTarget,
} from "@/features/messages/lib/sharedAgentInvocationPayment";
import { normalizePubkey } from "@/shared/lib/pubkey";

async function collectSharedAgentOwnerFallbackTargets({
  currentIdentity,
  currentProfile,
  existingTargets,
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
  existingTargets: readonly SharedAgentInvocationPaymentTarget[];
  managedAgents: readonly ManagedAgent[];
  mentionPubkeys: readonly string[];
  profiles?: UserProfileLookup;
  relayAgents: readonly RelayAgent[];
}) {
  const currentPubkey = normalizePubkey(currentIdentity.pubkey);
  const chargedAgentPubkeys = new Set(
    existingTargets.map((target) => target.agentPubkey),
  );
  const localAgentPubkeys = new Set(
    managedAgents.map((agent) => normalizePubkey(agent.pubkey)),
  );
  const relayAgentNames = new Map(
    relayAgents.map((agent) => [normalizePubkey(agent.pubkey), agent.name]),
  );
  const candidatePubkeys = [
    ...new Set(mentionPubkeys.map((pubkey) => normalizePubkey(pubkey))),
  ].filter(
    (pubkey) =>
      pubkey !== currentPubkey &&
      !chargedAgentPubkeys.has(pubkey) &&
      !localAgentPubkeys.has(pubkey),
  );

  const targets: SharedAgentInvocationPaymentTarget[] = [];
  for (const agentPubkey of candidatePubkeys) {
    let ownerPubkey: string | null = null;
    try {
      ownerPubkey = await resolveSharedAgentOwner(agentPubkey);
    } catch (error) {
      console.error("Failed to resolve shared agent owner", error);
      continue;
    }

    const target = buildSharedAgentInvocationPaymentTarget({
      agentPubkey,
      agentName: relayAgentNames.get(agentPubkey),
      currentIdentity,
      currentProfile,
      ownerPubkey,
      profiles,
    });
    if (!target) {
      continue;
    }

    targets.push(target);
    chargedAgentPubkeys.add(target.agentPubkey);
  }

  return targets;
}

export async function payForSharedAgentInvocations({
  currentIdentity,
  currentProfile,
  channelId,
  managedAgents,
  mentionPubkeys,
  profiles,
  queryClient,
  relayAgents,
}: {
  currentIdentity: Identity;
  channelId: string;
  currentProfile?: Pick<
    Profile,
    "pubkey" | "displayName" | "avatarUrl" | "nip05Handle" | "ownerPubkey"
  > | null;
  managedAgents: readonly ManagedAgent[];
  mentionPubkeys: readonly string[];
  profiles?: UserProfileLookup;
  queryClient: QueryClient;
  relayAgents: readonly RelayAgent[];
}) {
  const targets = collectSharedAgentInvocationPaymentTargets({
    currentIdentity,
    currentProfile,
    managedAgents,
    mentionPubkeys,
    profiles,
    relayAgents,
  });
  const fallbackTargets = await collectSharedAgentOwnerFallbackTargets({
    currentIdentity,
    currentProfile,
    existingTargets: targets,
    managedAgents,
    mentionPubkeys,
    profiles,
    relayAgents,
  });
  const allTargets = [...targets, ...fallbackTargets];
  if (allTargets.length === 0) {
    return [];
  }

  const paidTargets: SharedAgentInvocationPaymentTarget[] = [];
  for (const target of allTargets) {
    const payment = await sendSharedAgentInvocationPayment({
      channelId,
      ownerPubkey: target.ownerPubkey,
    });
    paidTargets.push({ ...target, amountSats: payment.amountSats });
  }

  void queryClient.invalidateQueries({
    queryKey: ["lightning-wallet", "summary"],
  });

  return paidTargets;
}

export function echoSharedAgentInvocationPayments(
  channelId: string,
  targets: readonly SharedAgentInvocationPaymentTarget[],
) {
  for (const target of targets) {
    void relayClient
      .sendMessage(
        channelId,
        formatSharedAgentInvocationPaymentMessage(target),
        [],
        [],
      )
      .catch((error) => {
        console.error("Failed to echo shared agent invocation payment", error);
      });
  }
}
