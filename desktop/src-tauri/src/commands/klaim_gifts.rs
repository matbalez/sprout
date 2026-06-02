use std::time::Duration;

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::app_state::AppState;

const KLAIM_BASE_URL: &str = "https://klaim.cash";

#[derive(Debug, Deserialize)]
struct KlaimErrorBody {
    error: Option<String>,
    detail: Option<String>,
    max_claims: Option<u64>,
    claims_used: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct KlaimRegisterSuccess {
    channel_id: String,
    campaign_id: String,
    default_amount_sats: u64,
    max_claims: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlaimRegisterResult {
    ok: bool,
    status_code: u16,
    channel_id: Option<String>,
    campaign_id: Option<String>,
    default_amount_sats: Option<u64>,
    max_claims: Option<u64>,
    error: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KlaimPayoutSuccess {
    status: String,
    amount_sats: u64,
    destination_kind: String,
    claims_used: u64,
    max_claims: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlaimPayoutResult {
    ok: bool,
    status_code: u16,
    status: Option<String>,
    amount_sats: Option<u64>,
    destination_kind: Option<String>,
    claims_used: Option<u64>,
    max_claims: Option<u64>,
    error: Option<String>,
    detail: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KlaimClaimSuccess {
    amount_sats: u64,
    destination: String,
    destination_kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KlaimClaimResult {
    ok: bool,
    status_code: u16,
    amount_sats: Option<u64>,
    destination: Option<String>,
    destination_kind: Option<String>,
    error: Option<String>,
    detail: Option<String>,
}

fn error_message_for_status(status: StatusCode, body: &str) -> KlaimErrorBody {
    match serde_json::from_str::<KlaimErrorBody>(body) {
        Ok(parsed) => parsed,
        Err(_) => KlaimErrorBody {
            error: Some(format!(
                "Klaim returned HTTP {}{}",
                status.as_u16(),
                if body.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", body.trim())
                }
            )),
            detail: None,
            max_claims: None,
            claims_used: None,
        },
    }
}

#[tauri::command]
pub async fn claim_klaim_code(
    state: State<'_, AppState>,
    code: String,
    address: String,
) -> Result<KlaimClaimResult, String> {
    let code = code.trim();
    let address = address.trim();
    if code.is_empty() {
        return Err("Klaim claim code is required.".to_string());
    }
    if address.is_empty() {
        return Err("Klaim claim address is required.".to_string());
    }

    let response = state
        .http_client
        .post(format!("{KLAIM_BASE_URL}/api/claim"))
        .timeout(Duration::from_secs(180))
        .json(&serde_json::json!({
            "code": code,
            "address": address,
        }))
        .send()
        .await
        .map_err(|error| format!("Failed to reach Klaim: {error}"))?;

    let status = response.status();
    let status_code = status.as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Klaim response: {error}"))?;

    if status == StatusCode::OK {
        let parsed: KlaimClaimSuccess = serde_json::from_str(&body)
            .map_err(|error| format!("Failed to parse Klaim claim response: {error}"))?;
        return Ok(KlaimClaimResult {
            ok: true,
            status_code,
            amount_sats: Some(parsed.amount_sats),
            destination: Some(parsed.destination),
            destination_kind: Some(parsed.destination_kind),
            error: None,
            detail: None,
        });
    }

    let parsed = error_message_for_status(status, &body);
    Ok(KlaimClaimResult {
        ok: false,
        status_code,
        amount_sats: None,
        destination: None,
        destination_kind: None,
        error: parsed.error,
        detail: parsed.detail,
    })
}

#[tauri::command]
pub async fn register_klaim_faucet_channel(
    state: State<'_, AppState>,
    campaign_id: String,
    channel_id: String,
) -> Result<KlaimRegisterResult, String> {
    let campaign_id = campaign_id.trim();
    let channel_id = channel_id.trim();
    if campaign_id.is_empty() {
        return Err("Klaim campaign ID is required.".to_string());
    }
    if channel_id.is_empty() {
        return Err("Channel ID is required.".to_string());
    }

    let response = state
        .http_client
        .post(format!("{KLAIM_BASE_URL}/api/sprout/register"))
        .timeout(Duration::from_secs(30))
        .json(&serde_json::json!({
            "campaign_id": campaign_id,
            "channel_id": channel_id,
        }))
        .send()
        .await
        .map_err(|error| format!("Failed to reach Klaim: {error}"))?;

    let status = response.status();
    let status_code = status.as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Klaim response: {error}"))?;

    if status == StatusCode::CREATED {
        let parsed: KlaimRegisterSuccess = serde_json::from_str(&body)
            .map_err(|error| format!("Failed to parse Klaim registration response: {error}"))?;
        return Ok(KlaimRegisterResult {
            ok: true,
            status_code,
            channel_id: Some(parsed.channel_id),
            campaign_id: Some(parsed.campaign_id),
            default_amount_sats: Some(parsed.default_amount_sats),
            max_claims: Some(parsed.max_claims),
            error: None,
            detail: None,
        });
    }

    let parsed = error_message_for_status(status, &body);
    Ok(KlaimRegisterResult {
        ok: false,
        status_code,
        channel_id: None,
        campaign_id: None,
        default_amount_sats: None,
        max_claims: parsed.max_claims,
        error: parsed.error,
        detail: parsed.detail,
    })
}

#[tauri::command]
pub async fn pay_klaim_faucet_member(
    state: State<'_, AppState>,
    channel_id: String,
    nostr_pubkey: String,
    bolt12: String,
) -> Result<KlaimPayoutResult, String> {
    let channel_id = channel_id.trim();
    let nostr_pubkey = nostr_pubkey.trim();
    let bolt12 = bolt12.trim();
    if channel_id.is_empty() {
        return Err("Channel ID is required.".to_string());
    }
    if nostr_pubkey.is_empty() {
        return Err("Member pubkey is required.".to_string());
    }
    if !bolt12.starts_with("lno1") {
        return Err("Member BOLT12 offer is required.".to_string());
    }

    let response = state
        .http_client
        .post(format!("{KLAIM_BASE_URL}/api/sprout/payout"))
        .timeout(Duration::from_secs(180))
        .json(&serde_json::json!({
            "channel_id": channel_id,
            "nostr_pubkey": nostr_pubkey,
            "bolt12": bolt12,
        }))
        .send()
        .await
        .map_err(|error| format!("Failed to reach Klaim: {error}"))?;

    let status = response.status();
    let status_code = status.as_u16();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Failed to read Klaim response: {error}"))?;

    if status == StatusCode::OK {
        let parsed: KlaimPayoutSuccess = serde_json::from_str(&body)
            .map_err(|error| format!("Failed to parse Klaim payout response: {error}"))?;
        return Ok(KlaimPayoutResult {
            ok: true,
            status_code,
            status: Some(parsed.status),
            amount_sats: Some(parsed.amount_sats),
            destination_kind: Some(parsed.destination_kind),
            claims_used: Some(parsed.claims_used),
            max_claims: Some(parsed.max_claims),
            error: None,
            detail: None,
        });
    }

    let parsed = error_message_for_status(status, &body);
    Ok(KlaimPayoutResult {
        ok: false,
        status_code,
        status: None,
        amount_sats: None,
        destination_kind: None,
        claims_used: parsed.claims_used,
        max_claims: parsed.max_claims,
        error: parsed.error,
        detail: parsed.detail,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn preserves_structured_error_fields() {
        let parsed = error_message_for_status(
            StatusCode::CONFLICT,
            r#"{"ok":false,"error":"channel has reached its max claims","max_claims":5,"claims_used":5}"#,
        );

        assert_eq!(
            parsed.error.as_deref(),
            Some("channel has reached its max claims")
        );
        assert_eq!(parsed.max_claims, Some(5));
        assert_eq!(parsed.claims_used, Some(5));
    }

    #[test]
    fn falls_back_for_non_json_errors() {
        let parsed = error_message_for_status(StatusCode::BAD_GATEWAY, "bad gateway");

        assert_eq!(
            parsed.error.as_deref(),
            Some("Klaim returned HTTP 502: bad gateway")
        );
    }

    #[test]
    fn ignores_unneeded_ok_field_in_errors() {
        let parsed = serde_json::from_value::<KlaimErrorBody>(serde_json::json!({
            "ok": false,
            "error": "wallet temporarily unavailable, try again shortly"
        }))
        .expect("extra fields should be ignored");

        assert_eq!(
            parsed.error.as_deref(),
            Some("wallet temporarily unavailable, try again shortly")
        );
    }

    #[test]
    fn success_payloads_ignore_extra_ok_field() {
        let parsed = serde_json::from_value::<KlaimPayoutSuccess>(serde_json::json!({
            "ok": true,
            "status": "paid",
            "amount_sats": 2100,
            "destination_kind": "bolt12",
            "claims_used": 1,
            "max_claims": 5
        }))
        .expect("extra fields should be ignored");

        assert_eq!(parsed.amount_sats, 2100);
        assert_eq!(parsed.claims_used, 1);
    }

    #[test]
    fn claim_success_payloads_ignore_extra_ok_field() {
        let parsed = serde_json::from_value::<KlaimClaimSuccess>(serde_json::json!({
            "ok": true,
            "amount_sats": 200,
            "destination": "lno1recipient",
            "destination_kind": "bolt12"
        }))
        .expect("extra fields should be ignored");

        assert_eq!(parsed.amount_sats, 200);
        assert_eq!(parsed.destination, "lno1recipient");
        assert_eq!(parsed.destination_kind, "bolt12");
    }

    #[test]
    fn error_parser_accepts_unknown_json_shape() {
        let parsed = error_message_for_status(StatusCode::INTERNAL_SERVER_ERROR, "{}");

        assert_eq!(parsed.error, None);
    }

    #[test]
    fn arbitrary_json_still_parses_as_empty_error_body() {
        let value: Value = serde_json::json!({"ok": false});
        let parsed =
            serde_json::from_value::<KlaimErrorBody>(value).expect("all fields are optional");

        assert!(parsed.error.is_none());
    }
}
