use std::{
    collections::BTreeMap,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::{Json, State as AxumState},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose, Engine as _};
use lexe::types::command::PayRequest;
use reqwest::{
    header::{HeaderName, HeaderValue, AUTHORIZATION, CONTENT_TYPE, WWW_AUTHENTICATE},
    Method, Url,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::app_state::AppState;

use super::{
    format::amount_from_sats,
    runtime::ensure_wallet,
    storage::{save_agent_payment_annotation, WalletStorage},
    types::{AgentPaymentBrokerConfig, WalletAgentPaymentAnnotation},
};

const MAX_FETCH_BODY_BYTES: usize = 1024 * 1024;
const DEFAULT_PAYMENT_WAIT_SECS: u64 = 120;
const MAX_PAYMENT_WAIT_SECS: u64 = 600;

#[derive(Clone)]
struct BrokerState {
    app_handle: AppHandle,
    token: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PayLightningBrokerRequest {
    payable: String,
    amount_sats: Option<u64>,
    description: Option<String>,
    consent_event_id: Option<String>,
    agent_pubkey: Option<String>,
    agent_name: Option<String>,
    wait_timeout_secs: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaidFetchBrokerRequest {
    url: String,
    method: Option<String>,
    headers: Option<BTreeMap<String, String>>,
    body: Option<String>,
    consent_event_id: Option<String>,
    agent_pubkey: Option<String>,
    agent_name: Option<String>,
    wait_timeout_secs: Option<u64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentPaymentResult {
    payment_id: String,
    status: String,
    status_message: String,
    amount_sats: Option<u64>,
    fees_sats: u64,
    payment_hash: Option<String>,
    preimage: Option<String>,
    offer_id: Option<String>,
    created_at_ms: u64,
    finalized_at_ms: Option<u64>,
    lexe_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PayLightningBrokerResponse {
    payment: AgentPaymentResult,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PaidFetchBrokerResponse {
    url: String,
    method: String,
    initial_status: u16,
    status: u16,
    headers: BTreeMap<String, String>,
    body_text: Option<String>,
    body_base64: Option<String>,
    payment: Option<AgentPaymentResult>,
    l402_scheme: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BrokerError {
    error: String,
    payment: Option<AgentPaymentResult>,
}

#[derive(Clone)]
struct AgentPaymentContext {
    protocol: &'static str,
    endpoint: Option<String>,
    agent_pubkey: Option<String>,
    agent_name: Option<String>,
    consent_event_id: Option<String>,
    description: Option<String>,
    wait_timeout_secs: Option<u64>,
}

#[derive(Debug)]
struct L402Challenge {
    scheme: String,
    token: String,
    invoice: String,
    request_path: Option<String>,
}

#[derive(Debug)]
enum PaidChallenge {
    LegacyL402(L402Challenge),
    HttpPaymentLightningCharge(HttpPaymentChallenge),
}

#[derive(Debug)]
struct HttpPaymentChallenge {
    id: String,
    realm: String,
    method: String,
    intent: String,
    request: String,
    amount_sats: u64,
    invoice: String,
    payment_hash: Option<String>,
    network: Option<String>,
    expires: Option<String>,
    description: Option<String>,
    digest: Option<String>,
    opaque: Option<String>,
}

struct HttpResponseData {
    status: u16,
    headers: HeaderMap,
    response_headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

pub fn spawn_agent_payment_broker(
    app_handle: AppHandle,
) -> Result<AgentPaymentBrokerConfig, String> {
    let token = broker_token();
    let state = BrokerState {
        app_handle,
        token: token.clone(),
    };
    let app = Router::new()
        .route("/health", get(health_handler))
        .route(
            "/v1/pay_lightning_invoice",
            post(pay_lightning_invoice_handler),
        )
        .route("/v1/paid_fetch", post(paid_fetch_handler))
        .with_state(state);

    let listener = std::net::TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("bind agent payment wallet broker: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("set wallet broker nonblocking: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("read wallet broker local addr: {error}"))?
        .port();
    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("sprout-desktop: failed to create wallet broker listener: {error}");
                return;
            }
        };
        if let Err(error) = axum::serve(listener, app).await {
            eprintln!("sprout-desktop: agent payment wallet broker stopped: {error}");
        }
    });

    let base_url = format!("http://127.0.0.1:{port}");
    eprintln!("sprout-desktop: agent payment wallet broker listening on {base_url}");
    Ok(AgentPaymentBrokerConfig { base_url, token })
}

async fn health_handler() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

async fn pay_lightning_invoice_handler(
    AxumState(state): AxumState<BrokerState>,
    headers: HeaderMap,
    Json(request): Json<PayLightningBrokerRequest>,
) -> impl IntoResponse {
    if let Err(error) = authorize(&headers, &state.token) {
        return broker_error(StatusCode::UNAUTHORIZED, error, None);
    }

    let payable = request.payable.trim().to_string();
    if payable.is_empty() {
        return broker_error(StatusCode::BAD_REQUEST, "payable is required", None);
    }

    let context = AgentPaymentContext {
        protocol: "lightning",
        endpoint: None,
        agent_pubkey: request.agent_pubkey,
        agent_name: request.agent_name,
        consent_event_id: request.consent_event_id,
        description: request.description,
        wait_timeout_secs: request.wait_timeout_secs,
    };

    match submit_agent_payment(&state.app_handle, payable, request.amount_sats, context).await {
        Ok(payment) => {
            (StatusCode::OK, Json(PayLightningBrokerResponse { payment })).into_response()
        }
        Err(error) => broker_error(StatusCode::BAD_GATEWAY, error, None),
    }
}

async fn paid_fetch_handler(
    AxumState(state): AxumState<BrokerState>,
    headers: HeaderMap,
    Json(request): Json<PaidFetchBrokerRequest>,
) -> impl IntoResponse {
    if let Err(error) = authorize(&headers, &state.token) {
        return broker_error(StatusCode::UNAUTHORIZED, error, None);
    }

    match paid_fetch(&state.app_handle, request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err((status, error, payment)) => broker_error(status, error, payment),
    }
}

async fn paid_fetch(
    app_handle: &AppHandle,
    request: PaidFetchBrokerRequest,
) -> Result<PaidFetchBrokerResponse, (StatusCode, String, Option<AgentPaymentResult>)> {
    let url = Url::parse(request.url.trim()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid URL: {error}"),
            None,
        )
    })?;
    let method_text = request
        .method
        .as_deref()
        .unwrap_or("GET")
        .trim()
        .to_ascii_uppercase();
    let method = Method::from_bytes(method_text.as_bytes()).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid HTTP method: {error}"),
            None,
        )
    })?;
    let headers = request.headers.clone().unwrap_or_default();
    let body = request.body.clone().unwrap_or_default();

    let initial = request_endpoint(&url, method.clone(), &headers, &body, None)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, None))?;

    if initial.status != StatusCode::PAYMENT_REQUIRED.as_u16() {
        let (body_text, body_base64) = encode_response_body(initial.body);
        return Ok(PaidFetchBrokerResponse {
            url: url.to_string(),
            method: method_text,
            initial_status: initial.status,
            status: initial.status,
            headers: initial.response_headers,
            body_text,
            body_base64,
            payment: None,
            l402_scheme: None,
        });
    }

    let challenge = parse_paid_challenge_from_headers(&initial.headers)
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, None))?
        .ok_or_else(|| {
            (
                StatusCode::BAD_GATEWAY,
                "HTTP 402 response did not include a usable L402/LSAT or Payment challenge"
                    .to_string(),
                None,
            )
        })?;
    let invoice = challenge.invoice().to_string();

    let context = AgentPaymentContext {
        protocol: challenge.protocol(),
        endpoint: Some(url.to_string()),
        agent_pubkey: request.agent_pubkey,
        agent_name: request.agent_name,
        consent_event_id: request.consent_event_id,
        description: Some(challenge.payment_description(&url)),
        wait_timeout_secs: request.wait_timeout_secs,
    };
    let payment = submit_agent_payment(app_handle, invoice, None, context)
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, None))?;

    if payment.status.trim().eq_ignore_ascii_case("failed") {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Lexe payment failed: {}", payment.status_message),
            Some(payment),
        ));
    }

    let Some(preimage) = payment.preimage.clone() else {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "Lexe payment reached status '{}' without a preimage",
                payment.status
            ),
            Some(payment),
        ));
    };

    let claim_url = challenge
        .claim_url(&url)
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, Some(payment.clone())))?;
    let authorization = challenge
        .authorization(&payment, &preimage)
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, Some(payment.clone())))?;
    let paid = request_endpoint(
        &claim_url,
        method.clone(),
        &headers,
        &body,
        Some(&authorization),
    )
    .await
    .map_err(|error| (StatusCode::BAD_GATEWAY, error, Some(payment.clone())))?;

    Ok(paid_fetch_response_from_paid_response(
        &url,
        method_text,
        initial.status,
        paid,
        payment,
        challenge.scheme_name().to_string(),
    ))
}

async fn submit_agent_payment(
    app_handle: &AppHandle,
    payable: String,
    amount_sats: Option<u64>,
    context: AgentPaymentContext,
) -> Result<AgentPaymentResult, String> {
    let state = app_handle.state::<AppState>();
    let wallet = ensure_wallet(app_handle, &state).await?;
    let amount = amount_sats.map(amount_from_sats).transpose()?;
    let personal_note = context
        .description
        .clone()
        .unwrap_or_else(|| "Sprout agent Lightning payment".to_string());
    let response = wallet
        .pay(PayRequest {
            payable,
            amount,
            message: None,
            personal_note: Some(personal_note),
        })
        .await
        .map_err(|error| format!("send Lexe payment: {error}"))?;

    *state.wallet_state.summary.lock().await = None;

    let wait_secs = context
        .wait_timeout_secs
        .unwrap_or(DEFAULT_PAYMENT_WAIT_SECS)
        .clamp(1, MAX_PAYMENT_WAIT_SECS);
    let storage = WalletStorage::from_app(app_handle)?;
    let payment_id = response.index.to_string();

    match wallet
        .wait_for_payment(response.index, Some(Duration::from_secs(wait_secs)))
        .await
    {
        Ok(payment) => {
            let result = AgentPaymentResult {
                payment_id: payment.index.to_string(),
                status: payment.status.to_string(),
                status_message: payment.status_msg.clone(),
                amount_sats: payment.amount.map(|amount| amount.sats_u64()),
                fees_sats: payment.fees.sats_u64(),
                payment_hash: serialize_lexe_value(payment.hash.as_ref()),
                preimage: serialize_lexe_value(payment.preimage.as_ref()),
                offer_id: serialize_lexe_value(payment.offer_id.as_ref()),
                created_at_ms: payment.created_at.to_millis(),
                finalized_at_ms: payment.finalized_at.map(|time| time.to_millis()),
                lexe_error: None,
            };
            save_payment_annotation(&storage, &context, &result)?;
            Ok(result)
        }
        Err(error) => {
            let now = now_ms();
            let result = AgentPaymentResult {
                payment_id,
                status: "wait_error".to_string(),
                status_message: format!("Lexe wait_for_payment failed: {error}"),
                amount_sats,
                fees_sats: 0,
                payment_hash: None,
                preimage: None,
                offer_id: None,
                created_at_ms: response.created_at.to_millis(),
                finalized_at_ms: None,
                lexe_error: Some(error.to_string()),
            };
            let annotation = WalletAgentPaymentAnnotation {
                payment_id: result.payment_id.clone(),
                agent_pubkey: clean_optional(context.agent_pubkey.as_deref()),
                agent_name: clean_optional(context.agent_name.as_deref()),
                protocol: context.protocol.to_string(),
                endpoint: clean_optional(context.endpoint.as_deref()),
                endpoint_host: endpoint_host(context.endpoint.as_deref()),
                endpoint_path: endpoint_path(context.endpoint.as_deref()),
                consent_event_id: clean_optional(context.consent_event_id.as_deref()),
                status: result.status.clone(),
                status_message: Some(result.status_message.clone()),
                amount_sats: result.amount_sats,
                fees_sats: Some(result.fees_sats),
                payment_hash: result.payment_hash.clone(),
                created_at_ms: result.created_at_ms,
                updated_at_ms: now,
            };
            save_agent_payment_annotation(&storage, annotation)?;
            Ok(result)
        }
    }
}

fn save_payment_annotation(
    storage: &WalletStorage,
    context: &AgentPaymentContext,
    result: &AgentPaymentResult,
) -> Result<(), String> {
    let annotation = WalletAgentPaymentAnnotation {
        payment_id: result.payment_id.clone(),
        agent_pubkey: clean_optional(context.agent_pubkey.as_deref()),
        agent_name: clean_optional(context.agent_name.as_deref()),
        protocol: context.protocol.to_string(),
        endpoint: clean_optional(context.endpoint.as_deref()),
        endpoint_host: endpoint_host(context.endpoint.as_deref()),
        endpoint_path: endpoint_path(context.endpoint.as_deref()),
        consent_event_id: clean_optional(context.consent_event_id.as_deref()),
        status: result.status.clone(),
        status_message: Some(result.status_message.clone()),
        amount_sats: result.amount_sats,
        fees_sats: Some(result.fees_sats),
        payment_hash: result.payment_hash.clone(),
        created_at_ms: result.created_at_ms,
        updated_at_ms: result.finalized_at_ms.unwrap_or_else(now_ms),
    };
    save_agent_payment_annotation(storage, annotation)
}

async fn request_endpoint(
    url: &Url,
    method: Method,
    headers: &BTreeMap<String, String>,
    body: &str,
    authorization: Option<&str>,
) -> Result<HttpResponseData, String> {
    let client = safe_http_client(url).await?;
    let mut request = client.request(method.clone(), url.clone());
    let mut has_content_type = false;
    for (key, value) in headers {
        if key.eq_ignore_ascii_case("authorization") {
            continue;
        }
        if key.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        let name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|error| format!("invalid header name {key:?}: {error}"))?;
        let value = HeaderValue::from_str(value)
            .map_err(|error| format!("invalid value for header {key:?}: {error}"))?;
        request = request.header(name, value);
    }
    if let Some(authorization) = authorization {
        request = request.header(AUTHORIZATION, authorization);
    }
    if method != Method::GET && method != Method::HEAD && !body.is_empty() {
        if should_default_json_content_type(&method, body, has_content_type) {
            request = request.header(CONTENT_TYPE, "application/json");
        }
        request = request.body(body.to_string());
    }

    let response = request
        .send()
        .await
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let response_headers = response_header_map(&headers);
    let body = read_limited_body(response).await?;

    Ok(HttpResponseData {
        status,
        headers,
        response_headers,
        body,
    })
}

async fn safe_http_client(url: &Url) -> Result<reqwest::Client, String> {
    match url.scheme() {
        "http" | "https" => {}
        other => return Err(format!("unsupported URL scheme: {other}")),
    }

    let host = url
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?;
    let port = url.port_or_known_default().unwrap_or(80);
    let ip = check_ssrf(host, port).await?;
    reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .resolve(host, SocketAddr::new(ip, port))
        .build()
        .map_err(|error| format!("build HTTP client: {error}"))
}

async fn check_ssrf(host: &str, port: u16) -> Result<IpAddr, String> {
    let addr = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let addrs: Vec<IpAddr> = tokio::task::spawn_blocking(move || {
        addr.to_socket_addrs()
            .map(|iter| iter.map(|socket| socket.ip()).collect::<Vec<_>>())
    })
    .await
    .map_err(|error| format!("SSRF DNS task failed: {error}"))?
    .map_err(|error| format!("DNS resolution failed: {error}"))?;

    if addrs.is_empty() {
        return Err("DNS resolution returned no addresses".to_string());
    }
    for ip in &addrs {
        if sprout_core::network::is_private_ip(ip) {
            return Err(format!(
                "SSRF blocked: {host} resolved to private/reserved address {ip}"
            ));
        }
    }
    Ok(addrs[0])
}

async fn read_limited_body(mut response: reqwest::Response) -> Result<Vec<u8>, String> {
    if let Some(length) = response.content_length() {
        if length > MAX_FETCH_BODY_BYTES as u64 {
            return Err(format!(
                "response body exceeds {} byte limit",
                MAX_FETCH_BODY_BYTES
            ));
        }
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("reading response body: {error}"))?
    {
        if body.len().saturating_add(chunk.len()) > MAX_FETCH_BODY_BYTES {
            return Err(format!(
                "response body exceeds {} byte limit",
                MAX_FETCH_BODY_BYTES
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn parse_paid_challenge_from_headers(headers: &HeaderMap) -> Result<Option<PaidChallenge>, String> {
    let mut first_error = None;
    for value in headers.get_all(WWW_AUTHENTICATE) {
        let Ok(header) = value.to_str() else {
            continue;
        };
        if let Some(challenge) = parse_l402_challenge(header) {
            return Ok(Some(PaidChallenge::LegacyL402(challenge)));
        }
        match parse_payment_auth_challenge(header) {
            Ok(Some(challenge)) => {
                return Ok(Some(PaidChallenge::HttpPaymentLightningCharge(challenge)))
            }
            Ok(None) => {}
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    if let Some(error) = first_error {
        Err(error)
    } else {
        Ok(None)
    }
}

fn parse_l402_challenge(header: &str) -> Option<L402Challenge> {
    let trimmed = header.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let scheme = parts.next()?.trim();
    if !scheme.eq_ignore_ascii_case("L402") && !scheme.eq_ignore_ascii_case("LSAT") {
        return None;
    }
    let params = parse_auth_params(parts.next().unwrap_or_default());
    let token = params
        .get("macaroon")
        .or_else(|| params.get("token"))
        .or_else(|| params.get("credential"))?
        .to_string();
    let invoice = params
        .get("invoice")
        .or_else(|| params.get("payment_request"))
        .or_else(|| params.get("bolt11"))?
        .to_string();
    if token.trim().is_empty() || invoice.trim().is_empty() {
        return None;
    }
    Some(L402Challenge {
        scheme: scheme.to_ascii_uppercase(),
        token,
        invoice,
        request_path: l402_request_path_from_macaroon(
            params
                .get("macaroon")
                .or_else(|| params.get("token"))
                .or_else(|| params.get("credential"))?,
        ),
    })
}

fn parse_payment_auth_challenge(header: &str) -> Result<Option<HttpPaymentChallenge>, String> {
    let trimmed = header.trim();
    let mut parts = trimmed.splitn(2, char::is_whitespace);
    let Some(scheme) = parts.next().map(str::trim) else {
        return Ok(None);
    };
    if !scheme.eq_ignore_ascii_case("Payment") {
        return Ok(None);
    }

    let params = parse_auth_params(parts.next().unwrap_or_default());
    let required = |key: &str| {
        params
            .get(key)
            .map(String::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| format!("Payment challenge missing required {key:?} parameter"))
    };

    let id = required("id")?.to_string();
    let realm = required("realm")?.to_string();
    let method = required("method")?.to_string();
    let intent = required("intent")?.to_string();
    let request = required("request")?.to_string();

    if !method.eq_ignore_ascii_case("lightning") {
        return Err(format!(
            "unsupported Payment challenge method {method:?}; only lightning is supported"
        ));
    }
    if !intent.eq_ignore_ascii_case("charge") {
        return Err(format!(
            "unsupported Payment challenge intent {intent:?}; only charge is supported"
        ));
    }

    reject_expired_payment_challenge(params.get("expires").map(String::as_str), now_ms())?;

    let decoded_request = decode_base64url(&request)
        .map_err(|error| format!("decode Payment challenge request: {error}"))?;
    let payment_request: PaymentAuthRequest = serde_json::from_slice(&decoded_request)
        .map_err(|error| format!("parse Payment challenge request JSON: {error}"))?;
    if !payment_request.currency.eq_ignore_ascii_case("sat") {
        return Err(format!(
            "unsupported Payment challenge currency {:?}; only sat is supported",
            payment_request.currency
        ));
    }
    let amount_sats = parse_payment_amount_sats(&payment_request.amount)?;
    let invoice = payment_request.method_details.invoice.trim().to_string();
    if invoice.is_empty() {
        return Err("Payment challenge request methodDetails.invoice is empty".to_string());
    }

    Ok(Some(HttpPaymentChallenge {
        id,
        realm,
        method: "lightning".to_string(),
        intent: "charge".to_string(),
        request,
        amount_sats,
        invoice,
        payment_hash: clean_optional(payment_request.method_details.payment_hash.as_deref()),
        network: clean_optional(payment_request.method_details.network.as_deref()),
        expires: clean_optional(params.get("expires").map(String::as_str)),
        description: clean_optional(params.get("description").map(String::as_str)),
        digest: clean_optional(params.get("digest").map(String::as_str)),
        opaque: clean_optional(params.get("opaque").map(String::as_str)),
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentAuthRequest {
    amount: serde_json::Value,
    currency: String,
    method_details: PaymentAuthMethodDetails,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PaymentAuthMethodDetails {
    invoice: String,
    payment_hash: Option<String>,
    network: Option<String>,
}

impl PaidChallenge {
    fn invoice(&self) -> &str {
        match self {
            PaidChallenge::LegacyL402(challenge) => &challenge.invoice,
            PaidChallenge::HttpPaymentLightningCharge(challenge) => &challenge.invoice,
        }
    }

    fn protocol(&self) -> &'static str {
        match self {
            PaidChallenge::LegacyL402(_) => "L402",
            PaidChallenge::HttpPaymentLightningCharge(_) => "http-payment",
        }
    }

    fn scheme_name(&self) -> &str {
        match self {
            PaidChallenge::LegacyL402(challenge) => &challenge.scheme,
            PaidChallenge::HttpPaymentLightningCharge(_) => "Payment",
        }
    }

    fn payment_description(&self, url: &Url) -> String {
        match self {
            PaidChallenge::LegacyL402(_) => format!(
                "Sprout agent L402 payment for {}{}",
                url.host_str().unwrap_or("unknown-host"),
                url.path()
            ),
            PaidChallenge::HttpPaymentLightningCharge(challenge) => {
                challenge.description.clone().unwrap_or_else(|| {
                    let network = challenge
                        .network
                        .as_deref()
                        .map(|network| format!(" on {network}"))
                        .unwrap_or_default();
                    format!(
                        "Sprout agent HTTP Payment Auth lightning charge of {} sats{} for {}{}",
                        challenge.amount_sats,
                        network,
                        url.host_str().unwrap_or("unknown-host"),
                        url.path()
                    )
                })
            }
        }
    }

    fn claim_url(&self, original: &Url) -> Result<Url, String> {
        match self {
            PaidChallenge::LegacyL402(challenge) => l402_claim_url(original, challenge),
            PaidChallenge::HttpPaymentLightningCharge(_) => Ok(original.clone()),
        }
    }

    fn authorization(
        &self,
        payment: &AgentPaymentResult,
        preimage: &str,
    ) -> Result<String, String> {
        match self {
            PaidChallenge::LegacyL402(challenge) => Ok(format!(
                "{} {}:{}",
                challenge.scheme, challenge.token, preimage
            )),
            PaidChallenge::HttpPaymentLightningCharge(challenge) => {
                challenge.authorization(payment, preimage)
            }
        }
    }
}

impl HttpPaymentChallenge {
    fn authorization(
        &self,
        payment: &AgentPaymentResult,
        preimage: &str,
    ) -> Result<String, String> {
        validate_payment_auth_preimage(self.payment_hash.as_deref(), payment, preimage)?;

        let mut challenge = BTreeMap::new();
        insert_auth_field(&mut challenge, "id", Some(self.id.as_str()));
        insert_auth_field(&mut challenge, "realm", Some(self.realm.as_str()));
        insert_auth_field(&mut challenge, "method", Some(self.method.as_str()));
        insert_auth_field(&mut challenge, "intent", Some(self.intent.as_str()));
        insert_auth_field(&mut challenge, "request", Some(self.request.as_str()));
        insert_auth_field(&mut challenge, "description", self.description.as_deref());
        insert_auth_field(&mut challenge, "digest", self.digest.as_deref());
        insert_auth_field(&mut challenge, "expires", self.expires.as_deref());
        insert_auth_field(&mut challenge, "opaque", self.opaque.as_deref());

        let mut payload = BTreeMap::new();
        payload.insert("preimage".to_string(), preimage.to_string());

        let mut credential = BTreeMap::new();
        credential.insert("challenge".to_string(), challenge);
        credential.insert("payload".to_string(), payload);

        let canonical_json = serde_json::to_vec(&credential)
            .map_err(|error| format!("serialize Payment credential: {error}"))?;
        Ok(format!(
            "Payment {}",
            general_purpose::URL_SAFE_NO_PAD.encode(canonical_json)
        ))
    }
}

fn insert_auth_field(map: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), value.to_string());
    }
}

fn paid_fetch_response_from_paid_response(
    url: &Url,
    method: String,
    initial_status: u16,
    paid: HttpResponseData,
    payment: AgentPaymentResult,
    scheme: String,
) -> PaidFetchBrokerResponse {
    let (body_text, body_base64) = encode_response_body(paid.body);
    // Return the provider response as-is, including 4xx/5xx statuses. Some paid
    // endpoints consume a proof before returning an upstream error, so the broker
    // must not hide the paid response behind another payment attempt.
    PaidFetchBrokerResponse {
        url: url.to_string(),
        method,
        initial_status,
        status: paid.status,
        headers: paid.response_headers,
        body_text,
        body_base64,
        payment: Some(payment),
        l402_scheme: Some(scheme),
    }
}

fn decode_base64url(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .or_else(|_| general_purpose::URL_SAFE.decode(value))
        .map_err(|error| error.to_string())
}

fn parse_payment_amount_sats(value: &serde_json::Value) -> Result<u64, String> {
    match value {
        serde_json::Value::Number(number) => number
            .as_u64()
            .ok_or_else(|| "Payment challenge amount must be a positive integer".to_string()),
        serde_json::Value::String(value) => value.trim().parse::<u64>().map_err(|error| {
            format!("Payment challenge amount must be a positive integer: {error}")
        }),
        _ => Err("Payment challenge amount must be a string or integer".to_string()),
    }
}

fn reject_expired_payment_challenge(expires: Option<&str>, now_ms: u64) -> Result<(), String> {
    let Some(expires) = expires.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let expires_ms = parse_payment_expires_ms(expires)?;
    if expires_ms <= now_ms {
        Err("Payment challenge is expired".to_string())
    } else {
        Ok(())
    }
}

fn parse_payment_expires_ms(expires: &str) -> Result<u64, String> {
    if let Ok(value) = expires.parse::<u64>() {
        return if value > 10_000_000_000 {
            Ok(value)
        } else {
            value
                .checked_mul(1000)
                .ok_or_else(|| "Payment challenge expires timestamp overflowed".to_string())
        };
    }

    let parsed = chrono::DateTime::parse_from_rfc3339(expires)
        .map_err(|error| format!("invalid Payment challenge expires timestamp: {error}"))?;
    let millis = parsed.timestamp_millis();
    if millis < 0 {
        Err("Payment challenge expires timestamp is before unix epoch".to_string())
    } else {
        Ok(millis as u64)
    }
}

fn validate_payment_auth_preimage(
    challenge_payment_hash: Option<&str>,
    payment: &AgentPaymentResult,
    preimage: &str,
) -> Result<(), String> {
    let preimage_bytes = decode_hex_32(preimage, "Payment preimage")?;
    let actual_hash = sha256_hex(&preimage_bytes);
    let Some(expected_hash) = challenge_payment_hash else {
        return Ok(());
    };
    let expected_hash = normalize_hex_32(expected_hash, "Payment challenge paymentHash")?;

    if let Some(payment_hash) = payment.payment_hash.as_deref() {
        let payment_hash = normalize_hex_32(payment_hash, "Lexe payment hash")?;
        if payment_hash != expected_hash {
            return Err(format!(
                "Lexe payment hash did not match Payment challenge paymentHash: expected {expected_hash}, got {payment_hash}"
            ));
        }
    }
    if actual_hash != expected_hash {
        return Err(format!(
            "Payment preimage does not match challenge paymentHash: expected {expected_hash}, got {actual_hash}"
        ));
    }
    Ok(())
}

fn decode_hex_32(value: &str, label: &str) -> Result<Vec<u8>, String> {
    let normalized = normalize_hex_32(value, label)?;
    hex::decode(normalized).map_err(|error| format!("{label} is not valid hex: {error}"))
}

fn normalize_hex_32(value: &str, label: &str) -> Result<String, String> {
    let normalized = value.trim().trim_start_matches("0x").to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("{label} must be a 32-byte hex string"));
    }
    Ok(normalized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;

    hex::encode(sha2::Sha256::digest(bytes))
}

fn l402_claim_url(original: &Url, challenge: &L402Challenge) -> Result<Url, String> {
    let Some(request_path) = challenge.request_path.as_deref() else {
        return Ok(original.clone());
    };
    let request_path = request_path.trim();
    if request_path.is_empty() {
        return Ok(original.clone());
    }
    if !is_valid_l402_request_path(request_path) {
        return Err(format!(
            "L402 challenge contained invalid RequestPath caveat: {request_path:?}"
        ));
    }

    let mut claim_url = original.clone();
    if let Some((path, query)) = request_path.split_once('?') {
        claim_url.set_path(path);
        claim_url.set_query(if query.is_empty() { None } else { Some(query) });
    } else {
        claim_url.set_path(request_path);
    }
    Ok(claim_url)
}

fn l402_request_path_from_macaroon(macaroon: &str) -> Option<String> {
    let decoded = decode_l402_macaroon(macaroon.trim())?;
    let needle = b"RequestPath = ";
    let start = decoded
        .windows(needle.len())
        .position(|window| window == needle)?
        + needle.len();
    let end = decoded[start..]
        .iter()
        .position(|byte| *byte == 0 || *byte == b'\n' || *byte == b'\r')
        .map(|offset| start + offset)
        .unwrap_or(decoded.len());
    let request_path = std::str::from_utf8(&decoded[start..end]).ok()?.trim();
    if is_valid_l402_request_path(request_path) {
        Some(request_path.to_string())
    } else {
        None
    }
}

fn decode_l402_macaroon(macaroon: &str) -> Option<Vec<u8>> {
    general_purpose::STANDARD
        .decode(macaroon)
        .or_else(|_| general_purpose::STANDARD_NO_PAD.decode(macaroon))
        .or_else(|_| general_purpose::URL_SAFE.decode(macaroon))
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(macaroon))
        .ok()
}

fn is_valid_l402_request_path(value: &str) -> bool {
    value.starts_with('/')
        && !value.starts_with("//")
        && !value.bytes().any(|byte| byte <= 0x20 || byte == b'#')
}

fn should_default_json_content_type(method: &Method, body: &str, has_content_type: bool) -> bool {
    !has_content_type
        && *method != Method::GET
        && *method != Method::HEAD
        && matches!(
            body.trim_start().as_bytes().first(),
            Some(b'{') | Some(b'[')
        )
}

fn parse_auth_params(input: &str) -> BTreeMap<String, String> {
    let bytes = input.as_bytes();
    let mut index = 0;
    let mut params = BTreeMap::new();

    while index < bytes.len() {
        while index < bytes.len() && (bytes[index] == b',' || bytes[index].is_ascii_whitespace()) {
            index += 1;
        }
        let key_start = index;
        while index < bytes.len()
            && bytes[index] != b'='
            && bytes[index] != b','
            && !bytes[index].is_ascii_whitespace()
        {
            index += 1;
        }
        let key = input[key_start..index].trim().to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if key.is_empty() || index >= bytes.len() || bytes[index] != b'=' {
            while index < bytes.len() && bytes[index] != b',' {
                index += 1;
            }
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }

        let value = if index < bytes.len() && bytes[index] == b'"' {
            index += 1;
            let mut value = String::new();
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' if index + 1 < bytes.len() => {
                        index += 1;
                        value.push(bytes[index] as char);
                    }
                    b'"' => {
                        index += 1;
                        break;
                    }
                    byte => value.push(byte as char),
                }
                index += 1;
            }
            value
        } else {
            let value_start = index;
            while index < bytes.len() && bytes[index] != b',' {
                index += 1;
            }
            input[value_start..index].trim().to_string()
        };
        params.insert(key, value);
    }

    params
}

fn response_header_map(headers: &HeaderMap) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for (key, value) in headers {
        if key == WWW_AUTHENTICATE {
            continue;
        }
        if let Ok(value) = value.to_str() {
            map.insert(key.as_str().to_string(), value.to_string());
        }
    }
    map
}

fn encode_response_body(body: Vec<u8>) -> (Option<String>, Option<String>) {
    match String::from_utf8(body) {
        Ok(text) => (Some(text), None),
        Err(error) => (
            None,
            Some(general_purpose::STANDARD.encode(error.into_bytes())),
        ),
    }
}

fn serialize_lexe_value<T: Serialize>(value: Option<&T>) -> Option<String> {
    let value = value?;
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(value) => Some(value),
        value => Some(value.to_string()),
    }
}

fn authorize(headers: &HeaderMap, token: &str) -> Result<(), String> {
    let expected = format!("Bearer {token}");
    let Some(value) = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err("missing wallet broker authorization".to_string());
    };
    if value == expected {
        Ok(())
    } else {
        Err("invalid wallet broker authorization".to_string())
    }
}

fn broker_error(
    status: StatusCode,
    error: impl Into<String>,
    payment: Option<AgentPaymentResult>,
) -> axum::response::Response {
    (
        status,
        Json(BrokerError {
            error: error.into(),
            payment,
        }),
    )
        .into_response()
}

fn broker_token() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn clean_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn endpoint_host(endpoint: Option<&str>) -> Option<String> {
    Url::parse(endpoint?).ok()?.host_str().map(str::to_string)
}

fn endpoint_path(endpoint: Option<&str>) -> Option<String> {
    let url = Url::parse(endpoint?).ok()?;
    Some(url.path().to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quoted_l402_challenge() {
        let challenge =
            parse_l402_challenge(r#"L402 macaroon="abc.def", invoice="lnbc1example""#).unwrap();

        assert_eq!(challenge.scheme, "L402");
        assert_eq!(challenge.token, "abc.def");
        assert_eq!(challenge.invoice, "lnbc1example");
        assert_eq!(challenge.request_path, None);
    }

    #[test]
    fn parses_lsat_token_alias() {
        let challenge =
            parse_l402_challenge(r#"LSAT token="mac", payment_request="lnbc1pay""#).unwrap();

        assert_eq!(challenge.scheme, "LSAT");
        assert_eq!(challenge.token, "mac");
        assert_eq!(challenge.invoice, "lnbc1pay");
    }

    #[test]
    fn extracts_request_path_from_l402_macaroon() {
        let macaroon =
            general_purpose::STANDARD.encode(b"\0RequestPath = /v1/images/generations\0Model = x");

        assert_eq!(
            l402_request_path_from_macaroon(&macaroon).as_deref(),
            Some("/v1/images/generations")
        );
    }

    #[test]
    fn parses_challenge_request_path() {
        let macaroon = general_purpose::STANDARD.encode(b"\0RequestPath = /v1/images/generations");
        let header = format!(r#"L402 macaroon="{macaroon}", invoice="lnbc1example""#);
        let challenge = parse_l402_challenge(&header).unwrap();

        assert_eq!(
            challenge.request_path.as_deref(),
            Some("/v1/images/generations")
        );
    }

    #[test]
    fn claim_url_uses_request_path_caveat_on_same_origin() {
        let original =
            Url::parse("https://llm402.ai/v1/images/generations/Qwen-Image-2.0").unwrap();
        let challenge = L402Challenge {
            scheme: "L402".to_string(),
            token: "mac".to_string(),
            invoice: "lnbc1example".to_string(),
            request_path: Some("/v1/images/generations".to_string()),
        };

        let claim_url = l402_claim_url(&original, &challenge).unwrap();

        assert_eq!(
            claim_url.as_str(),
            "https://llm402.ai/v1/images/generations"
        );
    }

    #[test]
    fn claim_url_rejects_non_path_request_caveats() {
        let original = Url::parse("https://llm402.ai/v1/images/generations").unwrap();
        let challenge = L402Challenge {
            scheme: "L402".to_string(),
            token: "mac".to_string(),
            invoice: "lnbc1example".to_string(),
            request_path: Some("https://evil.example/pay".to_string()),
        };

        assert!(l402_claim_url(&original, &challenge).is_err());
    }

    #[test]
    fn defaults_json_content_type_for_json_post_body() {
        assert!(should_default_json_content_type(
            &Method::POST,
            r#"{"model":"qwen-image-2.0"}"#,
            false
        ));
        assert!(!should_default_json_content_type(
            &Method::POST,
            r#"{"model":"qwen-image-2.0"}"#,
            true
        ));
        assert!(!should_default_json_content_type(
            &Method::GET,
            r#"{"model":"qwen-image-2.0"}"#,
            false
        ));
    }

    #[test]
    fn ignores_non_l402_challenge() {
        assert!(parse_l402_challenge(r#"Basic realm="x""#).is_none());
    }

    #[test]
    fn parses_payment_auth_lightning_charge_challenge() {
        let payment_hash = "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdc2f9f7cd";
        let request = payment_auth_request(Some(payment_hash));
        let header = format!(
            r#"Payment id="pay_123", realm="8218a705feb1", method="lightning", intent="charge", request="{request}", description="PPQ Data: X (Twitter) User Tweets", expires="2999-01-01T00:00:00Z""#
        );

        let challenge = parse_payment_auth_challenge(&header).unwrap().unwrap();

        assert_eq!(challenge.id, "pay_123");
        assert_eq!(challenge.realm, "8218a705feb1");
        assert_eq!(challenge.method, "lightning");
        assert_eq!(challenge.intent, "charge");
        assert_eq!(challenge.amount_sats, 19);
        assert_eq!(challenge.invoice, "lnbc1ppqexample");
        assert_eq!(challenge.payment_hash.as_deref(), Some(payment_hash));
        assert_eq!(challenge.network.as_deref(), Some("mainnet"));
        assert_eq!(
            challenge.description.as_deref(),
            Some("PPQ Data: X (Twitter) User Tweets")
        );
    }

    #[test]
    fn payment_auth_credential_uses_sorted_json_and_preimage_payload() {
        let preimage = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let payment_hash = sha256_hex(&hex::decode(preimage).unwrap());
        let request = payment_auth_request(Some(&payment_hash));
        let header = format!(
            r#"Payment id="pay_123", realm="8218a705feb1", method="lightning", intent="charge", request="{request}", description="PPQ Data: X (Twitter) User Tweets", expires="2999-01-01T00:00:00Z""#
        );
        let challenge = parse_payment_auth_challenge(&header).unwrap().unwrap();
        let payment = test_payment(Some(payment_hash), Some(preimage.to_string()));

        let authorization = challenge.authorization(&payment, preimage).unwrap();
        let encoded = authorization.strip_prefix("Payment ").unwrap();
        let decoded =
            String::from_utf8(general_purpose::URL_SAFE_NO_PAD.decode(encoded).unwrap()).unwrap();

        assert_eq!(
            decoded,
            format!(
                r#"{{"challenge":{{"description":"PPQ Data: X (Twitter) User Tweets","expires":"2999-01-01T00:00:00Z","id":"pay_123","intent":"charge","method":"lightning","realm":"8218a705feb1","request":"{request}"}},"payload":{{"preimage":"{preimage}"}}}}"#
            )
        );
    }

    #[test]
    fn payment_auth_rejects_unsupported_method_and_intent() {
        let request = payment_auth_request(None);
        let unsupported_method = format!(
            r#"Payment id="pay_123", realm="8218a705feb1", method="card", intent="charge", request="{request}""#
        );
        let unsupported_intent = format!(
            r#"Payment id="pay_123", realm="8218a705feb1", method="lightning", intent="refund", request="{request}""#
        );

        assert!(parse_payment_auth_challenge(&unsupported_method)
            .unwrap_err()
            .contains("unsupported Payment challenge method"));
        assert!(parse_payment_auth_challenge(&unsupported_intent)
            .unwrap_err()
            .contains("unsupported Payment challenge intent"));
    }

    #[test]
    fn payment_auth_rejects_expired_challenge_before_decoding_invoice() {
        let header = r#"Payment id="pay_123", realm="8218a705feb1", method="lightning", intent="charge", request="not-base64", expires="2000-01-01T00:00:00Z""#;

        assert_eq!(
            parse_payment_auth_challenge(header).unwrap_err(),
            "Payment challenge is expired"
        );
    }

    #[test]
    fn payment_auth_rejects_mismatched_payment_hash() {
        let preimage = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
        let payment_hash = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let request = payment_auth_request(Some(payment_hash));
        let header = format!(
            r#"Payment id="pay_123", realm="8218a705feb1", method="lightning", intent="charge", request="{request}""#
        );
        let challenge = parse_payment_auth_challenge(&header).unwrap().unwrap();
        let payment = test_payment(Some(payment_hash.to_string()), Some(preimage.to_string()));

        assert!(challenge
            .authorization(&payment, preimage)
            .unwrap_err()
            .contains("Payment preimage does not match challenge paymentHash"));
    }

    #[test]
    fn provider_5xx_after_payment_is_returned_without_retry_error() {
        let url = Url::parse("https://api.ppq.ai/v1/tweets").unwrap();
        let mut response_headers = BTreeMap::new();
        response_headers.insert("content-type".to_string(), "application/json".to_string());
        let paid = HttpResponseData {
            status: 502,
            headers: HeaderMap::new(),
            response_headers,
            body: br#"{"error":"upstream_error"}"#.to_vec(),
        };
        let payment = test_payment(None, Some("00".repeat(32)));

        let response = paid_fetch_response_from_paid_response(
            &url,
            "POST".to_string(),
            402,
            paid,
            payment,
            "Payment".to_string(),
        );

        assert_eq!(response.initial_status, 402);
        assert_eq!(response.status, 502);
        assert_eq!(response.l402_scheme.as_deref(), Some("Payment"));
        assert_eq!(
            response.body_text.as_deref(),
            Some(r#"{"error":"upstream_error"}"#)
        );
        assert_eq!(
            response
                .payment
                .as_ref()
                .map(|payment| payment.payment_id.as_str()),
            Some("payment_1")
        );
    }

    fn payment_auth_request(payment_hash: Option<&str>) -> String {
        let payment_hash_json = payment_hash
            .map(|payment_hash| format!(r#","paymentHash":"{payment_hash}""#))
            .unwrap_or_default();
        let request = format!(
            r#"{{"amount":"19","currency":"sat","methodDetails":{{"invoice":"lnbc1ppqexample","network":"mainnet"{payment_hash_json}}}}}"#
        );
        general_purpose::URL_SAFE_NO_PAD.encode(request)
    }

    fn test_payment(payment_hash: Option<String>, preimage: Option<String>) -> AgentPaymentResult {
        AgentPaymentResult {
            payment_id: "payment_1".to_string(),
            status: "completed".to_string(),
            status_message: "completed".to_string(),
            amount_sats: Some(19),
            fees_sats: 0,
            payment_hash,
            preimage,
            offer_id: None,
            created_at_ms: 1,
            finalized_at_ms: Some(2),
            lexe_error: None,
        }
    }
}
