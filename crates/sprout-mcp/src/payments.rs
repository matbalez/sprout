use std::collections::BTreeMap;
use std::time::Duration;

use rmcp::schemars;
use serde::{Deserialize, Serialize};

const BROKER_URL_ENV: &str = "SPROUT_WALLET_BROKER_URL";
const BROKER_TOKEN_ENV: &str = "SPROUT_WALLET_BROKER_TOKEN";
const AGENT_NAME_ENV: &str = "SPROUT_AGENT_NAME";
const AGENT_PUBKEY_ENV: &str = "SPROUT_AGENT_PUBKEY";

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PayLightningInvoiceParams {
    /// BOLT11 invoice, BOLT12 offer, Lightning address, or other Lexe-supported payable string.
    pub payable: String,
    /// Amount in sats. Required only when the payable does not encode its own amount.
    #[serde(default)]
    pub amount_sats: Option<u64>,
    /// Local payment note stored with the wallet transaction.
    #[serde(default)]
    pub description: Option<String>,
    /// Sprout event ID or other local reference proving the user authorized this payment.
    #[serde(default)]
    pub consent_event_id: Option<String>,
    /// Seconds to wait for Lexe to return a terminal payment state. Broker clamps the maximum.
    #[serde(default)]
    pub wait_timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PaidFetchParams {
    /// HTTP or HTTPS URL to fetch.
    pub url: String,
    /// HTTP method. Defaults to GET.
    #[serde(default)]
    pub method: Option<String>,
    /// Request headers. Authorization is ignored and replaced by the L402 proof after payment.
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    /// UTF-8 request body for non-GET/non-HEAD requests.
    #[serde(default)]
    pub body: Option<String>,
    /// Sprout event ID or other local reference proving the user authorized this payment.
    #[serde(default)]
    pub consent_event_id: Option<String>,
    /// Seconds to wait for Lexe to return a terminal payment state. Broker clamps the maximum.
    #[serde(default)]
    pub wait_timeout_secs: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrokerPayLightningRequest<'a> {
    payable: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_sats: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    consent_event_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wait_timeout_secs: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrokerPaidFetchRequest<'a> {
    url: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    method: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<&'a BTreeMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    consent_event_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_pubkey: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    wait_timeout_secs: Option<u64>,
}

pub(crate) async fn pay_lightning_invoice(params: PayLightningInvoiceParams) -> String {
    let request = BrokerPayLightningRequest {
        payable: params.payable.trim(),
        amount_sats: params.amount_sats,
        description: params
            .description
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        consent_event_id: params
            .consent_event_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        agent_pubkey: clean_env(AGENT_PUBKEY_ENV),
        agent_name: clean_env(AGENT_NAME_ENV),
        wait_timeout_secs: params.wait_timeout_secs,
    };

    if request.payable.is_empty() {
        return "Error: payable is required".to_string();
    }

    broker_post("/v1/pay_lightning_invoice", &request).await
}

pub(crate) async fn paid_fetch(params: PaidFetchParams) -> String {
    let request = BrokerPaidFetchRequest {
        url: params.url.trim(),
        method: params
            .method
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        headers: params.headers.as_ref(),
        body: params.body.as_deref(),
        consent_event_id: params
            .consent_event_id
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty()),
        agent_pubkey: clean_env(AGENT_PUBKEY_ENV),
        agent_name: clean_env(AGENT_NAME_ENV),
        wait_timeout_secs: params.wait_timeout_secs,
    };

    if request.url.is_empty() {
        return "Error: url is required".to_string();
    }

    broker_post("/v1/paid_fetch", &request).await
}

async fn broker_post<T: Serialize + ?Sized>(path: &str, request: &T) -> String {
    let Ok(base_url) = std::env::var(BROKER_URL_ENV) else {
        return format!("Error: {BROKER_URL_ENV} is not configured. Start this agent from Sprout desktop with the payments toolset enabled.");
    };
    let Ok(token) = std::env::var(BROKER_TOKEN_ENV) else {
        return format!("Error: {BROKER_TOKEN_ENV} is not configured. Start this agent from Sprout desktop with the payments toolset enabled.");
    };

    let base_url = base_url.trim().trim_end_matches('/');
    if base_url.is_empty() || token.trim().is_empty() {
        return "Error: Sprout wallet broker configuration is empty".to_string();
    }

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
    {
        Ok(client) => client,
        Err(error) => return format!("Error: failed to build wallet broker client: {error}"),
    };

    let response = match client
        .post(format!("{base_url}{path}"))
        .bearer_auth(token)
        .json(request)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => return format!("Error: Sprout wallet broker request failed: {error}"),
    };

    let status = response.status();
    let text = match response.text().await {
        Ok(text) => text,
        Err(error) => return format!("Error: failed to read wallet broker response: {error}"),
    };

    if status.is_success() {
        text
    } else if text.trim().is_empty() {
        format!("Error: Sprout wallet broker returned HTTP {status}")
    } else {
        format!("Error: Sprout wallet broker returned HTTP {status}: {text}")
    }
}

fn clean_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
