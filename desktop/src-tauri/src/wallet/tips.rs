use nostr::{EventId, PublicKey};
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::{app_state::AppState, events, relay::submit_event};

use super::{
    discovery::resolve_bolt12_offer_for_pubkey,
    send_payment,
    storage::current_pubkey,
    types::{ChannelPaymentResult, MessageTipResult, WalletPaymentResult},
};

const MESSAGE_TIP_AMOUNT_SATS: u64 = 10;
const MESSAGE_KUDOS_AMOUNT_SATS: u64 = 210;
const SHARED_AGENT_INVOCATION_AMOUNT_SATS: u64 = 50;
const LEXE_PAYER_MESSAGE_MAX_CHARS: usize = 200;

#[tauri::command]
pub async fn send_channel_payment(
    channel_id: String,
    metadata_event_id: String,
    recipient_pubkey: String,
    bolt12_offer: String,
    amount_sats: u64,
    purpose: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ChannelPaymentResult, String> {
    let channel_uuid =
        Uuid::parse_str(&channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let metadata_event_id = EventId::from_hex(&metadata_event_id)
        .map_err(|error| format!("invalid channel metadata event id: {error}"))?;
    let recipient = PublicKey::from_hex(&recipient_pubkey)
        .map_err(|error| format!("invalid recipient pubkey: {error}"))?;
    let recipient_pubkey = recipient.to_hex();
    let purpose = purpose.trim().to_string();
    match purpose.as_str() {
        "join" | "post" => {}
        other => return Err(format!("invalid channel payment purpose: {other}")),
    }
    if amount_sats == 0 {
        return Err("channel payment amount must be greater than zero".to_string());
    }
    if bolt12_offer.trim().is_empty() {
        return Err("channel payment requires a BOLT12 offer".to_string());
    }

    let nonce = Uuid::new_v4().simple().to_string();
    let payer_message = channel_payment_payer_message(channel_uuid, &purpose, &nonce, amount_sats);
    let payment = send_payment(
        app,
        &state,
        amount_sats,
        bolt12_offer.trim().to_string(),
        Some(payer_message),
        format!("Sprout paid channel {purpose}"),
    )
    .await?;

    let receipt = events::build_channel_payment_receipt(
        channel_uuid,
        metadata_event_id,
        &recipient_pubkey,
        amount_sats,
        &purpose,
        &nonce,
    )?;

    match submit_event(receipt, &state).await {
        Ok(result) => Ok(ChannelPaymentResult {
            payment_id: payment.payment_id,
            amount_sats,
            nonce,
            receipt_event_id: Some(result.event_id),
            receipt_accepted: true,
            receipt_error: None,
        }),
        Err(error) => Ok(ChannelPaymentResult {
            payment_id: payment.payment_id,
            amount_sats,
            nonce,
            receipt_event_id: None,
            receipt_accepted: false,
            receipt_error: Some(error),
        }),
    }
}

#[tauri::command]
pub async fn send_message_kudos(
    channel_id: String,
    recipient_pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletPaymentResult, String> {
    let _channel_uuid =
        Uuid::parse_str(&channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let recipient = PublicKey::from_hex(&recipient_pubkey)
        .map_err(|error| format!("invalid recipient pubkey: {error}"))?;
    let recipient_pubkey = recipient.to_hex();
    let current_pubkey = current_pubkey(&state)?;
    if current_pubkey.eq_ignore_ascii_case(&recipient_pubkey) {
        return Err("cannot give kudos to yourself".to_string());
    }

    let payable = resolve_bolt12_offer_for_pubkey(&state, &recipient_pubkey)
        .await?
        .ok_or_else(|| "mentioned user has not published a wallet BOLT12 offer".to_string())?;

    send_payment(
        app,
        &state,
        MESSAGE_KUDOS_AMOUNT_SATS,
        payable,
        Some("Sprout kudos".to_string()),
        "Sprout kudos".to_string(),
    )
    .await
}

#[tauri::command]
pub async fn send_message_tip(
    channel_id: String,
    message_id: String,
    recipient_pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<MessageTipResult, String> {
    let channel_uuid =
        Uuid::parse_str(&channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let target_event_id =
        EventId::from_hex(&message_id).map_err(|error| format!("invalid message id: {error}"))?;
    let recipient = PublicKey::from_hex(&recipient_pubkey)
        .map_err(|error| format!("invalid recipient pubkey: {error}"))?;
    let recipient_pubkey = recipient.to_hex();
    let current_pubkey = current_pubkey(&state)?;
    if current_pubkey.eq_ignore_ascii_case(&recipient_pubkey) {
        return Err("cannot tip your own message".to_string());
    }

    let payable = resolve_bolt12_offer_for_pubkey(&state, &recipient_pubkey)
        .await?
        .ok_or_else(|| "message author has not published a wallet BOLT12 offer".to_string())?;
    let tip_id = Uuid::new_v4().simple().to_string();
    let payer_message = message_tip_payer_message(channel_uuid, target_event_id, &tip_id);
    let payment = send_payment(
        app,
        &state,
        MESSAGE_TIP_AMOUNT_SATS,
        payable,
        Some(payer_message),
        "Sprout message tip".to_string(),
    )
    .await?;

    let receipt = events::build_message_tip_receipt(
        channel_uuid,
        target_event_id,
        &recipient_pubkey,
        MESSAGE_TIP_AMOUNT_SATS,
        &tip_id,
    )?;

    match submit_event(receipt, &state).await {
        Ok(result) => Ok(MessageTipResult {
            payment_id: payment.payment_id,
            amount_sats: MESSAGE_TIP_AMOUNT_SATS,
            tip_id,
            receipt_event_id: Some(result.event_id),
            receipt_accepted: true,
            receipt_error: None,
        }),
        Err(error) => Ok(MessageTipResult {
            payment_id: payment.payment_id,
            amount_sats: MESSAGE_TIP_AMOUNT_SATS,
            tip_id,
            receipt_event_id: None,
            receipt_accepted: false,
            receipt_error: Some(error),
        }),
    }
}

#[tauri::command]
pub async fn send_shared_agent_invocation_payment(
    channel_id: String,
    owner_pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<WalletPaymentResult, String> {
    let _channel_uuid =
        Uuid::parse_str(&channel_id).map_err(|_| format!("invalid channel UUID: {channel_id}"))?;
    let owner = PublicKey::from_hex(&owner_pubkey)
        .map_err(|error| format!("invalid owner pubkey: {error}"))?;
    let owner_pubkey = owner.to_hex();
    let current_pubkey = current_pubkey(&state)?;
    if current_pubkey.eq_ignore_ascii_case(&owner_pubkey) {
        return Err("cannot pay yourself to invoke your own shared agent".to_string());
    }

    let payable = resolve_bolt12_offer_for_pubkey(&state, &owner_pubkey)
        .await?
        .ok_or_else(|| "agent owner has not published a wallet BOLT12 offer".to_string())?;

    send_payment(
        app,
        &state,
        SHARED_AGENT_INVOCATION_AMOUNT_SATS,
        payable,
        Some("Sprout shared agent invocation".to_string()),
        "Sprout shared agent invocation".to_string(),
    )
    .await
}

fn message_tip_payer_message(channel_id: Uuid, message_id: EventId, tip_id: &str) -> String {
    let channel_id = channel_id.simple().to_string();
    let message_id = message_id.to_hex();
    let tip_id = tip_id.replace('-', "");
    let message =
        format!("sprout-tip:v1:{channel_id}:{message_id}:{tip_id}:{MESSAGE_TIP_AMOUNT_SATS}");
    debug_assert!(message.chars().count() <= LEXE_PAYER_MESSAGE_MAX_CHARS);
    message
}

fn channel_payment_payer_message(
    channel_id: Uuid,
    purpose: &str,
    nonce: &str,
    amount_sats: u64,
) -> String {
    let channel_id = channel_id.simple().to_string();
    let nonce = nonce.replace('-', "");
    let message = format!("sprout-channel:v1:{channel_id}:{purpose}:{nonce}:{amount_sats}");
    debug_assert!(message.chars().count() <= LEXE_PAYER_MESSAGE_MAX_CHARS);
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payer_message_fits_lexe_limit() {
        let channel_id = Uuid::parse_str("1069491a-ccdc-43a6-bbfb-2d9f8b4d0afb").unwrap();
        let message_id =
            EventId::from_hex("dc91b7ef91fa438a4c8d8904c55113d65e11db006bff3c045568a965514ceedd")
                .unwrap();
        let tip_id = "00000000-0000-4000-8000-000000000000";

        let message = message_tip_payer_message(channel_id, message_id, tip_id);

        assert_eq!(
            message,
            "sprout-tip:v1:1069491accdc43a6bbfb2d9f8b4d0afb:dc91b7ef91fa438a4c8d8904c55113d65e11db006bff3c045568a965514ceedd:00000000000040008000000000000000:10"
        );
        assert!(message.chars().count() <= LEXE_PAYER_MESSAGE_MAX_CHARS);
    }
}
