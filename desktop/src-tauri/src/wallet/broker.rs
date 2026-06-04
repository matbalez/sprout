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
    header::{HeaderName, HeaderValue, AUTHORIZATION, WWW_AUTHENTICATE},
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

struct L402Challenge {
    scheme: String,
    token: String,
    invoice: String,
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

    let challenge = parse_l402_from_headers(&initial.headers).ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            "HTTP 402 response did not include a usable L402/LSAT challenge".to_string(),
            None,
        )
    })?;

    let context = AgentPaymentContext {
        protocol: "L402",
        endpoint: Some(url.to_string()),
        agent_pubkey: request.agent_pubkey,
        agent_name: request.agent_name,
        consent_event_id: request.consent_event_id,
        description: Some(format!(
            "Sprout agent L402 payment for {}{}",
            url.host_str().unwrap_or("unknown-host"),
            url.path()
        )),
        wait_timeout_secs: request.wait_timeout_secs,
    };
    let payment = submit_agent_payment(app_handle, challenge.invoice.clone(), None, context)
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

    let authorization = format!("{} {}:{}", challenge.scheme, challenge.token, preimage);
    let paid = request_endpoint(&url, method.clone(), &headers, &body, Some(&authorization))
        .await
        .map_err(|error| (StatusCode::BAD_GATEWAY, error, Some(payment.clone())))?;
    let (body_text, body_base64) = encode_response_body(paid.body);

    Ok(PaidFetchBrokerResponse {
        url: url.to_string(),
        method: method_text,
        initial_status: initial.status,
        status: paid.status,
        headers: paid.response_headers,
        body_text,
        body_base64,
        payment: Some(payment),
        l402_scheme: Some(challenge.scheme),
    })
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
    for (key, value) in headers {
        if key.eq_ignore_ascii_case("authorization") {
            continue;
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

fn parse_l402_from_headers(headers: &HeaderMap) -> Option<L402Challenge> {
    for value in headers.get_all(WWW_AUTHENTICATE) {
        let Ok(header) = value.to_str() else {
            continue;
        };
        if let Some(challenge) = parse_l402_challenge(header) {
            return Some(challenge);
        }
    }
    None
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
    })
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
    fn ignores_non_l402_challenge() {
        assert!(parse_l402_challenge(r#"Basic realm="x""#).is_none());
    }
}
