use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::oneshot;

use crate::GatewayConfigResponse;

// ---------------------------------------------------------------------------
// Resolved config for the active provider
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProxyConfig {
    pub active_provider: String,
    pub display_name: String,
    pub upstream_url: String,
    pub api_key: String,
    pub api_key_env: String,
    pub model_map: HashMap<String, String>,
    pub default_model: String,
    pub visible_models: Vec<String>,
    pub force_anthropic_version: Option<String>,
    pub supports_count_tokens: bool,
    pub supports_vision: bool,
    pub supports_video: bool,
    pub server_host: String,
    pub server_port: u16,
    pub enable_cors: bool,
}

pub fn resolve_proxy_config(cfg: &GatewayConfigResponse) -> Result<ProxyConfig, String> {
    let provider = cfg
        .providers
        .get(&cfg.active_provider)
        .ok_or_else(|| {
            format!(
                "Active provider '{}' not found in config",
                cfg.active_provider
            )
        })?;

    let api_key = std::env::var(&provider.api_key_env).map_err(|_| {
        format!(
            "{} not set — set it in the API Key tab first.",
            provider.api_key_env
        )
    })?;

    Ok(ProxyConfig {
        active_provider: cfg.active_provider.clone(),
        display_name: provider.display_name.clone(),
        upstream_url: provider.upstream_url.clone(),
        api_key,
        api_key_env: provider.api_key_env.clone(),
        model_map: provider.model_map.clone(),
        default_model: provider.default_model.clone(),
        visible_models: provider.visible_models.clone(),
        force_anthropic_version: provider.force_anthropic_version.clone(),
        supports_count_tokens: provider.supports_count_tokens,
        supports_vision: provider.supports_vision,
        supports_video: provider.supports_video,
        server_host: cfg.server.host.clone(),
        server_port: cfg.server.port,
        enable_cors: cfg.server.enable_cors,
    })
}

// ---------------------------------------------------------------------------
// HTTP client
// ---------------------------------------------------------------------------

fn build_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build reqwest client")
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn rewrite_model(requested: &str, config: &ProxyConfig) -> String {
    config
        .model_map
        .get(requested)
        .cloned()
        .unwrap_or_else(|| {
            tracing::warn!(
                "Unknown incoming model: {} -> Fallback to default_model: {}",
                requested,
                config.default_model
            );
            config.default_model.clone()
        })
}

fn detect_media_types(messages: &[Value]) -> HashSet<String> {
    let mut found = HashSet::new();
    for msg in messages {
        let content = match msg.get("content") {
            Some(Value::Array(arr)) => arr,
            _ => continue,
        };
        for block in content {
            if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                if t == "image" || t == "video" {
                    found.insert(t.to_string());
                }
            }
        }
    }
    found
}

fn check_media_support(
    messages: &[Value],
    config: &ProxyConfig,
) -> Result<(), (StatusCode, Json<Value>)> {
    let media = detect_media_types(messages);
    if media.contains("image") && !config.supports_vision {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "Provider '{}' does not support image input. \
                         Switch active_provider to a vision-capable provider (e.g. minimax or kimi).",
                        config.display_name
                    )
                }
            })),
        ));
    }
    if media.contains("video") && !config.supports_video {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "Provider '{}' does not support video input. \
                         Switch active_provider to a provider with video support.",
                        config.display_name
                    )
                }
            })),
        ));
    }
    Ok(())
}

fn build_upstream_headers(incoming: &HeaderMap, config: &ProxyConfig) -> HeaderMap {
    let mut headers = HeaderMap::new();

    let auth_value = format!("Bearer {}", config.api_key);
    match auth_value.parse() {
        Ok(v) => {
            headers.insert("Authorization", v);
        }
        Err(e) => {
            tracing::error!(
                "API key contains characters invalid for HTTP header. Key length: {}. Error: {}",
                config.api_key.len(),
                e
            );
        }
    }

    headers.insert("Content-Type", "application/json".parse().unwrap());

    if let Some(ref version) = config.force_anthropic_version {
        match version.parse() {
            Ok(v) => {
                headers.insert("anthropic-version", v);
            }
            Err(e) => {
                tracing::error!(
                    "force_anthropic_version '{}' is not a valid header value: {}",
                    version,
                    e
                );
            }
        }
    } else if let Some(v) = incoming.get("anthropic-version") {
        headers.insert("anthropic-version", v.clone());
    }

    if let Some(beta) = incoming.get("anthropic-beta") {
        headers.insert("anthropic-beta", beta.clone());
    }

    headers
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

async fn health(State(config): State<Arc<ProxyConfig>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "upstream": config.upstream_url,
        "provider": config.active_provider,
        "api_key_env": config.api_key_env,
        "api_key_set": true,
    }))
}

async fn list_models(State(config): State<Arc<ProxyConfig>>) -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": config.visible_models.iter().map(|m| json!({
            "id": m,
            "object": "model",
            "type": "model",
        })).collect::<Vec<_>>(),
    }))
}

async fn proxy_count_tokens(
    State(config): State<Arc<ProxyConfig>>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, (StatusCode, Json<Value>)> {
    if !config.supports_count_tokens {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "not_supported_error",
                    "message": format!(
                        "The active provider '{}' does not support /v1/messages/count_tokens.",
                        config.active_provider
                    )
                }
            })),
        ));
    }

    let mut body: Value =
        serde_json::from_str(&body).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"type": "invalid_request_error", "message": e.to_string()}})),
            )
        })?;

    let model_in = body["model"].as_str().unwrap_or(&config.default_model);
    body["model"] = json!(rewrite_model(model_in, &config));

    let client = build_reqwest_client();
    let upstream_resp = client
        .post(format!("{}/v1/messages/count_tokens", config.upstream_url))
        .headers(build_upstream_headers(&headers, &config))
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
            )
        })?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let resp_body = upstream_resp.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    let mut response = Response::new(Body::from(resp_body));
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    Ok(response)
}

async fn proxy_messages(
    State(config): State<Arc<ProxyConfig>>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let mut body: Value =
        serde_json::from_str(&body).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": {"type": "invalid_request_error", "message": e.to_string()}})),
            )
        })?;

    // Check media support
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(messages, &config)?;
    }

    // Rewrite model only
    let model_in = body["model"].as_str().unwrap_or(&config.default_model);
    body["model"] = json!(rewrite_model(model_in, &config));

    let is_stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let upstream_headers = build_upstream_headers(&headers, &config);
    let client = build_reqwest_client();
    let upstream_req = client
        .post(format!("{}/v1/messages", config.upstream_url))
        .headers(upstream_headers)
        .json(&body);

    if is_stream {
        handle_stream(upstream_req).await
    } else {
        handle_nonstream(upstream_req).await
    }
}

async fn handle_nonstream(
    req: reqwest::RequestBuilder,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let upstream_resp = req.send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let resp_body = upstream_resp.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    let mut response = Response::new(Body::from(resp_body));
    *response.status_mut() = status;
    *response.headers_mut() = resp_headers;
    Ok(response)
}

async fn handle_stream(
    req: reqwest::RequestBuilder,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let upstream_resp = req.send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    if !upstream_resp.status().is_success() {
        let status = upstream_resp.status();
        let body = upstream_resp.text().await.unwrap_or_default();
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": {
                    "type": "proxy_error",
                    "message": format!("Upstream error {}: {}", status.as_u16(), &body[..body.len().min(300)])
                }
            })),
        ));
    }

    let stream = upstream_resp.bytes_stream().map(|chunk| {
        chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    });

    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    response.headers_mut().insert(
        "Content-Type",
        "text/event-stream".parse().unwrap(),
    );
    response
        .headers_mut()
        .insert("Cache-Control", "no-cache".parse().unwrap());
    response
        .headers_mut()
        .insert("Connection", "keep-alive".parse().unwrap());
    response
        .headers_mut()
        .insert("X-Accel-Buffering", "no".parse().unwrap());
    Ok(response)
}

// ---------------------------------------------------------------------------
// Router + server runner
// ---------------------------------------------------------------------------

fn create_router(config: Arc<ProxyConfig>) -> Router {
    let mut router = Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(list_models))
        .route("/v1/messages", post(proxy_messages))
        .route("/v1/messages/count_tokens", post(proxy_count_tokens))
        .with_state(config.clone());

    if config.enable_cors {
        router = router.layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods(tower_http::cors::Any)
                .allow_headers(tower_http::cors::Any),
        );
    }

    router
}

pub async fn run_proxy_server(
    host: String,
    port: u16,
    config: ProxyConfig,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = format!("{}:{}", host, port);
    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        format!("Cannot bind to {}: {}", addr, e)
    })?;

    tracing::info!("Proxy server listening on {}", addr);

    let app = create_router(Arc::new(config));

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            tracing::info!("Proxy server shutting down");
        })
        .await
        .map_err(|e| format!("Server error: {}", e).into())
}
