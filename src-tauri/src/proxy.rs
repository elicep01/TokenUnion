use std::net::SocketAddr;

use anyhow::{anyhow, Result};
use axum::{
    body::{to_bytes, Body},
    extract::{Request, State},
    http::{HeaderName, Method, Response, StatusCode},
    response::IntoResponse,
    routing::any,
    Router,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::{net::TcpListener, sync::oneshot};

use crate::{
    db::{RequestInsert, TransactionInsert},
    p2p::{ProxyRelayRequest, ProxyRelayResponse},
    tracker::{usage_from_json_body, usage_from_sse_chunk, UsageStats},
    vault::decrypt_api_key,
    AppState,
};

#[derive(Clone)]
struct ProxyCtx {
    app: AppState,
}

pub async fn run_proxy_server(
    app_state: AppState,
    port: u16,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> Result<()> {
    let app = Router::new()
        .route("/v1", any(proxy_handler))
        .route("/v1/*path", any(proxy_handler))
        .route("/anthropic/v1", any(proxy_handler))
        .route("/anthropic/v1/*path", any(proxy_handler))
        .route("/openai/v1", any(proxy_handler))
        .route("/openai/v1/*path", any(proxy_handler))
        .with_state(ProxyCtx { app: app_state });

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = TcpListener::bind(addr).await?;

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = (&mut shutdown_rx).await;
        })
        .await?;

    Ok(())
}

async fn proxy_handler(State(ctx): State<ProxyCtx>, request: Request) -> impl IntoResponse {
    match proxy_handler_inner(ctx.app, request).await {
        Ok(response) => response,
        Err(err) => {
            let body = serde_json::json!({ "error": err.to_string() }).to_string();
            Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap_or_else(|_| Response::new(Body::from("proxy error")))
        }
    }
}

async fn proxy_handler_inner(app: AppState, request: Request) -> Result<Response<Body>> {
    let (parts, body) = request.into_parts();
    let method: Method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;

    app.db.ensure_daily_reset()?;

    let (provider, upstream_path) = infer_provider_and_path(uri.path(), &headers);
    let query = uri.query().map(|q| q.to_string());

    let request_body = to_bytes(body, 20 * 1024 * 1024).await?.to_vec();
    // Hard security invariant: request/response content is never persisted.
    if app.db.no_content_logging_hardcoded() {
        return Err(anyhow!("log_content must remain false"));
    }
    let request_hash = compute_request_hash(&provider, &method, &upstream_path, query.as_deref(), &request_body);
    enforce_content_filter(&app, &request_body)?;

    let local_state = app
        .db
        .get_local_availability_state()
        .unwrap_or_else(|_| "available".to_string());

    let local_key_exists = app.db.get_provider_key(&provider)?.is_some();
    let local_available = local_key_exists && local_state != "sleeping" && local_state != "paused";

    if local_available {
        let local = forward_local(
            app.clone(),
            &provider,
            method,
            upstream_path,
            query,
            headers,
            request_body,
        )
        .await?;
        persist_usage(
            &app,
            &provider,
            "self",
            None,
            &request_hash,
            &local,
            "local_proxy",
        )?;
        publish_ledger_tx(
            app.clone(),
            TransactionInsert {
                tx_type: "self".to_string(),
                peer_id: None,
                provider: provider.clone(),
                model: local.model.clone(),
                input_tokens: local.input_tokens,
                output_tokens: local.output_tokens,
                request_hash: request_hash.clone(),
            },
        )
        .await;
        let local_body = STANDARD.decode(&local.body_b64).unwrap_or_default();
        return build_response(local.status, to_reqwest_header_map(&local.headers), Body::from(local_body));
    }

    let grant = {
        let p2p = app
            .p2p_handle
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow!("P2P unavailable"))?;
        p2p.request_peer_grant(request_hash.clone(), 1000).await?
    };

    if let Some(grant) = grant {
        let winning_peer = grant.peer_id.clone();
        let relay_req = ProxyRelayRequest {
            request_hash: request_hash.clone(),
            provider: provider.clone(),
            method: method.to_string(),
            path: upstream_path.clone(),
            query,
            headers: headers
                .iter()
                .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
                .collect(),
            body_b64: STANDARD.encode(request_body),
        };

        let relay_res = {
            let p2p = app
                .p2p_handle
                .read()
                .await
                .clone()
                .ok_or_else(|| anyhow!("P2P unavailable"))?;
            p2p.proxy_via_peer(grant.peer_id.clone(), relay_req).await?
        };

        persist_usage(
            &app,
            &provider,
            "borrowed",
            Some(winning_peer.clone()),
            &request_hash,
            &relay_res,
            "borrowed_proxy",
        )?;
        publish_ledger_tx(
            app.clone(),
            TransactionInsert {
                tx_type: "borrowed".to_string(),
                peer_id: Some(winning_peer),
                provider: provider.clone(),
                model: relay_res.model.clone(),
                input_tokens: relay_res.input_tokens,
                output_tokens: relay_res.output_tokens,
                request_hash: request_hash.clone(),
            },
        )
        .await;

        return response_from_proxy_relay(relay_res);
    }

    Err(anyhow!(
        "local key unavailable and no peers granted request"
    ))
}

async fn publish_ledger_tx(app: AppState, tx: TransactionInsert) {
    if let Some(p2p) = app.p2p_handle.read().await.clone() {
        let _ = p2p.publish_transaction(tx).await;
    }
}

async fn forward_local(
    app: AppState,
    provider: &str,
    method: Method,
    path: String,
    query: Option<String>,
    headers: axum::http::HeaderMap,
    request_body: Vec<u8>,
) -> Result<ProxyRelayResponse> {
    let password = app
        .vault_password
        .read()
        .await
        .clone()
        .ok_or_else(|| anyhow!("vault locked; unlock from Vault tab"))?;

    let stored = app
        .db
        .get_provider_key(provider)?
        .ok_or_else(|| anyhow!("no provider key in vault"))?;
    let device_salt = app.db.get_or_create_device_salt()?;
    let api_key = decrypt_api_key(&stored.encrypted_key, &password, &device_salt)?;

    let base_url = if provider == "openai" {
        "https://api.openai.com"
    } else {
        "https://api.anthropic.com"
    };

    let url = format!(
        "{base_url}{path}{}",
        query.as_ref().map(|q| format!("?{q}")).unwrap_or_default()
    );

    let mut upstream = app.http.request(method, url).body(request_body);
    for (name, value) in &headers {
        if should_skip_header(name) {
            continue;
        }
        upstream = upstream.header(name, value);
    }

    if provider == "openai" {
        upstream = upstream.header("authorization", format!("Bearer {api_key}"));
    } else {
        upstream = upstream.header("x-api-key", api_key);
    }

    let upstream_res = upstream.send().await?;
    let status = upstream_res.status().as_u16();
    let response_headers = upstream_res
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.to_string(), v.to_str().ok()?.to_string())))
        .collect::<Vec<_>>();
    let request_id = upstream_res
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_type = upstream_res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("text/event-stream") {
        let stream = upstream_res.bytes_stream();
        let mut aggregate = UsageStats::default();
        let mut all_bytes = Vec::new();
        futures_util::pin_mut!(stream);
        while let Some(item) = stream.next().await {
            if let Ok(chunk) = item {
                if let Ok(text) = std::str::from_utf8(&chunk) {
                    if let Some(usage) = usage_from_sse_chunk(text) {
                        aggregate.input_tokens = usage.input_tokens.max(aggregate.input_tokens);
                        aggregate.output_tokens = usage.output_tokens.max(aggregate.output_tokens);
                        if usage.model.is_some() {
                            aggregate.model = usage.model;
                        }
                    }
                }
                all_bytes.extend_from_slice(&chunk);
            }
        }

        return Ok(ProxyRelayResponse {
            request_hash: "local".to_string(),
            status,
            headers: response_headers,
            body_b64: STANDARD.encode(all_bytes),
            model: aggregate.model,
            input_tokens: aggregate.input_tokens,
            output_tokens: aggregate.output_tokens,
            request_id,
            error: None,
        });
    }

    let bytes = upstream_res.bytes().await?.to_vec();
    let usage = usage_from_json_body(&bytes).unwrap_or_default();
    Ok(ProxyRelayResponse {
        request_hash: "local".to_string(),
        status,
        headers: response_headers,
        body_b64: STANDARD.encode(bytes),
        model: usage.model,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        request_id,
        error: None,
    })
}

fn persist_usage(
    app: &AppState,
    provider: &str,
    tx_type: &str,
    peer_id: Option<String>,
    request_hash: &str,
    response: &ProxyRelayResponse,
    source: &str,
) -> Result<()> {
    app.db.insert_request(&RequestInsert {
        model: response.model.clone(),
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        source: source.to_string(),
        request_id: response.request_id.clone(),
    })?;

    app.db.insert_transaction(&TransactionInsert {
        tx_type: tx_type.to_string(),
        peer_id: peer_id.clone(),
        provider: provider.to_string(),
        model: response.model.clone(),
        input_tokens: response.input_tokens,
        output_tokens: response.output_tokens,
        request_hash: request_hash.to_string(),
    })?;
    app.db.insert_audit_log(
        "outbound",
        peer_id.as_deref(),
        response.model.as_deref(),
        response.input_tokens,
        response.output_tokens,
        request_hash,
    )?;

    Ok(())
}

fn response_from_proxy_relay(relay_res: ProxyRelayResponse) -> Result<Response<Body>> {
    if let Some(err) = relay_res.error {
        let body = serde_json::json!({"error": err}).to_string();
        return Ok(Response::builder()
            .status(relay_res.status)
            .header("content-type", "application/json")
            .body(Body::from(body))?);
    }

    let body = STANDARD.decode(&relay_res.body_b64).unwrap_or_default();
    build_response(relay_res.status, to_reqwest_header_map(&relay_res.headers), Body::from(body))
}

fn build_response(
    status: u16,
    headers: reqwest::header::HeaderMap,
    body: Body,
) -> Result<Response<Body>> {
    let mut builder = Response::builder().status(status);

    for (name, value) in &headers {
        if should_skip_header(name) {
            continue;
        }
        builder = builder.header(name, value);
    }

    Ok(builder.body(body)?)
}

fn should_skip_header(name: &HeaderName) -> bool {
    name == http::header::HOST || name == http::header::CONTENT_LENGTH
}

fn infer_provider_and_path(path: &str, headers: &axum::http::HeaderMap) -> (String, String) {
    if path.starts_with("/openai/") {
        return (
            "openai".to_string(),
            path.trim_start_matches("/openai").to_string(),
        );
    }
    if path.starts_with("/anthropic/") {
        return (
            "anthropic".to_string(),
            path.trim_start_matches("/anthropic").to_string(),
        );
    }
    if let Some(provider) = headers
        .get("x-tokenunion-provider")
        .and_then(|v| v.to_str().ok())
    {
        return (provider.to_string(), path.to_string());
    }
    ("anthropic".to_string(), path.to_string())
}

fn compute_request_hash(
    provider: &str,
    method: &Method,
    path: &str,
    query: Option<&str>,
    body: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(provider.as_bytes());
    hasher.update(method.as_str().as_bytes());
    hasher.update(path.as_bytes());
    if let Some(q) = query {
        hasher.update(q.as_bytes());
    }
    hasher.update(body);
    format!("{:x}", hasher.finalize())
}

fn to_reqwest_header_map(headers: &[(String, String)]) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
            reqwest::header::HeaderValue::from_str(v),
        ) {
            map.insert(name, value);
        }
    }
    map
}

fn enforce_content_filter(app: &AppState, request_body: &[u8]) -> Result<()> {
    let blocked = app
        .db
        .get_setting("blocked_model_patterns")?
        .unwrap_or_default();
    if blocked.trim().is_empty() {
        return Ok(());
    }

    let parsed = serde_json::from_slice::<serde_json::Value>(request_body).ok();
    if let Some(model) = parsed
        .as_ref()
        .and_then(|v| v.get("model"))
        .and_then(|v| v.as_str())
    {
        if blocked
            .split(',')
            .any(|pattern| !pattern.trim().is_empty() && model.contains(pattern.trim()))
        {
            return Err(anyhow!("model blocked by local content filter"));
        }
    }
    Ok(())
}
