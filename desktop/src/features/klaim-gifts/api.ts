import { invokeTauri } from "@/shared/api/tauri";

export type KlaimRegisterResult = {
  ok: boolean;
  statusCode: number;
  channelId: string | null;
  campaignId: string | null;
  defaultAmountSats: number | null;
  maxClaims: number | null;
  error: string | null;
  detail: string | null;
};

export type KlaimPayoutResult = {
  ok: boolean;
  statusCode: number;
  status: string | null;
  amountSats: number | null;
  destinationKind: string | null;
  claimsUsed: number | null;
  maxClaims: number | null;
  error: string | null;
  detail: string | null;
};

export function registerKlaimFaucetChannel(input: {
  campaignId: string;
  channelId: string;
}) {
  return invokeTauri<KlaimRegisterResult>("register_klaim_faucet_channel", {
    campaignId: input.campaignId,
    channelId: input.channelId,
  });
}

export function payKlaimFaucetMember(input: {
  channelId: string;
  nostrPubkey: string;
  bolt12: string;
}) {
  return invokeTauri<KlaimPayoutResult>("pay_klaim_faucet_member", {
    channelId: input.channelId,
    nostrPubkey: input.nostrPubkey,
    bolt12: input.bolt12,
  });
}
