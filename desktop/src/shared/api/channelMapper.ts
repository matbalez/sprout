import type {
  Channel,
  ChannelDetail,
  ChannelMember,
  ChannelType,
} from "./types";

export type RawChannel = {
  metadata_event_id?: string;
  id: string;
  name: string;
  channel_type: ChannelType;
  visibility: "open" | "private";
  description: string;
  topic: string | null;
  purpose: string | null;
  member_count: number;
  member_pubkeys: string[];
  last_message_at: string | null;
  archived_at: string | null;
  participants: string[];
  participant_pubkeys: string[];
  is_member?: boolean;
  current_user_role?: ChannelMember["role"] | null;
  ttl_seconds: number | null;
  ttl_deadline: string | null;
  payment_policy?: RawChannelPaymentPolicy | null;
  hive_channel?: boolean;
  hive_wallet_bolt12_offer?: string | null;
};

type RawChannelPaymentPolicy = {
  join_payment_required: boolean;
  join_amount_base_units: number;
  post_payment_required: boolean;
  post_amount_base_units: number;
  payment_recipient_pubkey: string;
  payment_recipient_bolt12_offer: string;
  payment_rail: string;
};

export type RawChannelDetail = RawChannel & {
  created_by: string;
  created_at: string;
  updated_at: string;
  topic_set_by: string | null;
  topic_set_at: string | null;
  purpose_set_by: string | null;
  purpose_set_at: string | null;
  topic_required: boolean;
  max_members: number | null;
  nip29_group_id: string | null;
};

export type RawChannelMember = {
  pubkey: string;
  role: ChannelMember["role"];
  is_agent?: boolean;
  joined_at: string;
  display_name: string | null;
};

export function fromRawChannel(channel: RawChannel): Channel {
  return {
    metadataEventId: channel.metadata_event_id ?? channel.id,
    id: channel.id,
    name: channel.name,
    channelType: channel.channel_type,
    visibility: channel.visibility,
    description: channel.description,
    topic: channel.topic,
    purpose: channel.purpose,
    memberCount: channel.member_count,
    memberPubkeys: channel.member_pubkeys ?? [],
    lastMessageAt: channel.last_message_at,
    archivedAt: channel.archived_at,
    participants: channel.participants,
    participantPubkeys: channel.participant_pubkeys,
    isMember: channel.is_member ?? true,
    currentUserRole: channel.current_user_role ?? null,
    ttlSeconds: channel.ttl_seconds,
    ttlDeadline: channel.ttl_deadline,
    paymentPolicy: channel.payment_policy
      ? {
          joinPaymentRequired: channel.payment_policy.join_payment_required,
          joinAmountBaseUnits: channel.payment_policy.join_amount_base_units,
          postPaymentRequired: channel.payment_policy.post_payment_required,
          postAmountBaseUnits: channel.payment_policy.post_amount_base_units,
          paymentRecipientPubkey:
            channel.payment_policy.payment_recipient_pubkey,
          paymentRecipientBolt12Offer:
            channel.payment_policy.payment_recipient_bolt12_offer,
          paymentRail: channel.payment_policy.payment_rail,
        }
      : null,
    hiveChannel: channel.hive_channel ?? false,
    hiveWalletBolt12Offer: channel.hive_wallet_bolt12_offer ?? null,
  };
}

export function fromRawChannelDetail(channel: RawChannelDetail): ChannelDetail {
  return {
    ...fromRawChannel(channel),
    createdBy: channel.created_by,
    createdAt: channel.created_at,
    updatedAt: channel.updated_at,
    topicSetBy: channel.topic_set_by,
    topicSetAt: channel.topic_set_at,
    purposeSetBy: channel.purpose_set_by,
    purposeSetAt: channel.purpose_set_at,
    topicRequired: channel.topic_required,
    maxMembers: channel.max_members,
    nip29GroupId: channel.nip29_group_id,
  };
}

export function fromRawChannelMember(member: RawChannelMember): ChannelMember {
  return {
    pubkey: member.pubkey,
    role: member.role,
    isAgent: member.is_agent ?? false,
    joinedAt: member.joined_at,
    displayName: member.display_name,
  };
}
