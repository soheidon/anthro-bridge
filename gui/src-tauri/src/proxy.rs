use std::collections::HashMap;

use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Json, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use tokio::sync::oneshot;
use futures::StreamExt;

use crate::GatewayConfigResponse;

// ---------------------------------------------------------------------------
// Resolved config for model-based multi-provider routing
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProviderRoute {
    pub display_name: String,
    pub upstream_url: String,
    pub api_key: String,
    #[allow(dead_code)]
    pub api_key_env: String,
    pub force_anthropic_version: Option<String>,
    pub supports_vision: bool,
    pub supports_video: bool,
    pub supports_count_tokens: bool,
}

#[derive(Clone)]
pub struct ProxyConfig {
    /// gateway_model → (provider_id, upstream_model)
    pub model_route: HashMap<String, (String, String)>,
    /// provider_id → route info
    pub providers: HashMap<String, ProviderRoute>,
    /// Fallback provider id (from routing.fallback_provider or active_provider)
    pub fallback_provider: String,
    /// All visible model names (for /v1/models)
    pub all_models: Vec<String>,
    pub server_host: String,
    pub server_port: u16,
    pub enable_cors: bool,
}

pub fn resolve_proxy_config(cfg: &GatewayConfigResponse) -> Result<ProxyConfig, String> {
    let mut providers: HashMap<String, ProviderRoute> = HashMap::new();
    let mut model_route: HashMap<String, (String, String)> = HashMap::new();
    let mut all_models: Vec<String> = Vec::new();

    for (provider_id, p) in &cfg.providers {
        let api_key = std::env::var(&p.api_key_env).map_err(|_| {
            format!(
                "{} not set — set it in the API Key tab first.",
                p.api_key_env
            )
        })?;

        providers.insert(
            provider_id.clone(),
            ProviderRoute {
                display_name: p.display_name.clone(),
                upstream_url: p.upstream_url.clone(),
                api_key,
                api_key_env: p.api_key_env.clone(),
                force_anthropic_version: p.force_anthropic_version.clone(),
                supports_vision: p.supports_vision,
                supports_video: p.supports_video,
                supports_count_tokens: p.supports_count_tokens,
            },
        );

        // Build reverse mapping: gateway_model → (provider_id, upstream_model)
        for (gateway_model, upstream_model) in &p.model_map {
            model_route.insert(
                gateway_model.clone(),
                (provider_id.clone(), upstream_model.clone()),
            );
            all_models.push(gateway_model.clone());
        }
    }

    if model_route.is_empty() {
        return Err("No models configured. Add model_map entries to config.json.".into());
    }

    // fallback: routing.fallback_provider > active_provider > first provider
    let fallback = cfg
        .active_provider
        .clone()
        .or_else(|| cfg.providers.keys().next().cloned())
        .unwrap_or_default();

    Ok(ProxyConfig {
        model_route,
        providers,
        fallback_provider: fallback,
        all_models,
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

/// Look up a model and return (provider_id, provider_route, upstream_model).
fn resolve_model<'a>(
    model: &str,
    config: &'a ProxyConfig,
) -> Result<(&'a str, &'a ProviderRoute, &'a str), (StatusCode, Json<Value>)> {
    let (provider_id, upstream_model) =
        config.model_route.get(model).ok_or_else(|| {
            let available = config.all_models.join(", ");
            (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "type": "error",
                    "error": {
                        "type": "invalid_request_error",
                        "message": format!(
                            "Unknown model '{}'. Available models: {}",
                            model, available
                        )
                    }
                })),
            )
        })?;

    let route = config.providers.get(provider_id).ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "server_error",
                    "message": format!("Provider '{}' not found for model '{}'", provider_id, model)
                }
            })),
        )
    })?;

    Ok((provider_id.as_str(), route, upstream_model.as_str()))
}

fn detect_media_types(messages: &[Value]) -> (bool, bool) {
    let mut has_image = false;
    let mut has_video = false;
    for msg in messages {
        let content = match msg.get("content") {
            Some(Value::Array(arr)) => arr,
            _ => continue,
        };
        for block in content {
            if let Some(t) = block.get("type").and_then(|v| v.as_str()) {
                if t == "image" {
                    has_image = true;
                } else if t == "video" {
                    has_video = true;
                }
            }
        }
    }
    (has_image, has_video)
}

fn check_media_support(
    messages: &[Value],
    route: &ProviderRoute,
) -> Result<(), (StatusCode, Json<Value>)> {
    let (has_image, has_video) = detect_media_types(messages);
    if has_image && !route.supports_vision {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "Provider '{}' does not support image input. \
                         Use a vision-capable model (e.g. claude-minimax-m3 or claude-kimi-k2-6).",
                        route.display_name
                    )
                }
            })),
        ));
    }
    if has_video && !route.supports_video {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "Provider '{}' does not support video input.",
                        route.display_name
                    )
                }
            })),
        ));
    }
    Ok(())
}

fn build_upstream_headers(incoming: &HeaderMap, route: &ProviderRoute) -> HeaderMap {
    let mut headers = HeaderMap::new();

    let auth_value = format!("Bearer {}", route.api_key);
    match auth_value.parse() {
        Ok(v) => {
            headers.insert("Authorization", v);
        }
        Err(e) => {
            tracing::error!(
                "API key contains characters invalid for HTTP header. Key length: {}. Error: {}",
                route.api_key.len(),
                e
            );
        }
    }

    headers.insert("Content-Type", "application/json".parse().unwrap());

    if let Some(ref version) = route.force_anthropic_version {
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

async fn health(State(config): State<std::sync::Arc<ProxyConfig>>) -> Json<Value> {
    let models: Vec<&str> = config.all_models.iter().map(|s| s.as_str()).collect();
    Json(json!({
        "status": "ok",
        "routing": "model-based",
        "fallback_provider": config.fallback_provider,
        "models": models,
        "providers": config.providers.keys().collect::<Vec<_>>(),
    }))
}

async fn list_models(State(config): State<std::sync::Arc<ProxyConfig>>) -> Json<Value> {
    Json(json!({
        "object": "list",
        "data": config.all_models.iter().map(|m| json!({
            "id": m,
            "object": "model",
            "type": "model",
        })).collect::<Vec<_>>(),
    }))
}

async fn proxy_count_tokens(
    State(config): State<std::sync::Arc<ProxyConfig>>,
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

    let model_in = body["model"].as_str().unwrap_or("");
    let (_provider_id, route, upstream_model) = resolve_model(model_in, &config)?;

    if !route.supports_count_tokens {
        return Err((
            StatusCode::NOT_IMPLEMENTED,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "not_supported_error",
                    "message": format!(
                        "Provider '{}' does not support /v1/messages/count_tokens.",
                        route.display_name
                    )
                }
            })),
        ));
    }

    body["model"] = json!(upstream_model);

    let client = build_reqwest_client();
    let upstream_resp = client
        .post(format!("{}/v1/messages/count_tokens", route.upstream_url))
        .headers(build_upstream_headers(&headers, route))
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
    State(config): State<std::sync::Arc<ProxyConfig>>,
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

    let model_in = body["model"].as_str().unwrap_or("");
    let (_provider_id, route, upstream_model) = resolve_model(model_in, &config)?;

    // Check media support for the resolved provider
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(messages, route)?;
    }

    // Rewrite model to upstream model name
    body["model"] = json!(upstream_model);

    let is_stream = body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

    let upstream_headers = build_upstream_headers(&headers, route);
    let client = build_reqwest_client();
    let upstream_req = client
        .post(format!("{}/v1/messages", route.upstream_url))
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

fn create_router(config: std::sync::Arc<ProxyConfig>) -> Router {
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

    tracing::info!(
        "Proxy server listening on {} (model-based routing, {} models, {} providers)",
        addr,
        config.all_models.len(),
        config.providers.len()
    );

    let app = create_router(std::sync::Arc::new(config));

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            tracing::info!("Proxy server shutting down");
        })
        .await
        .map_err(|e| format!("Server error: {}", e).into())
}
