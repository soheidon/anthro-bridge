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
// Model capability resolver — single source of truth for upstream model specs.
// Mirrors TypeScript MODEL_CAPABILITIES in gui/src/modelCapabilities.ts.
//
// ⚠️ SYNC: When adding/editing a model, update BOTH this resolver AND
//          gui/src/modelCapabilities.ts MODEL_CAPABILITIES simultaneously.
//          The two definitions must stay in agreement.
// ---------------------------------------------------------------------------

pub struct ModelCapabilities {
    pub supports_image_url: bool,
    pub supports_image_base64: bool,
    pub supports_video_url: bool,
    pub supports_video_base64: bool,
    pub force_thinking: bool,
}

/// Resolve capabilities for a known upstream model.
/// Unknown / custom models get all-false defaults.
pub fn resolve_model_capabilities(upstream_model: &str) -> ModelCapabilities {
    match upstream_model {
        // ── DeepSeek ──
        "deepseek-v4-pro" => ModelCapabilities {
            supports_image_url: false, supports_image_base64: false,
            supports_video_url: false, supports_video_base64: false,
            force_thinking: false,
        },
        "deepseek-v4-flash" => ModelCapabilities {
            supports_image_url: false, supports_image_base64: false,
            supports_video_url: false, supports_video_base64: false,
            force_thinking: false,
        },
        // ── MiniMax ──
        "MiniMax-M3" => ModelCapabilities {
            supports_image_url: true, supports_image_base64: true,
            supports_video_url: true, supports_video_base64: true,
            force_thinking: false,
        },
        "MiniMax-M2.7-highspeed" => ModelCapabilities {
            supports_image_url: false, supports_image_base64: false,
            supports_video_url: false, supports_video_base64: false,
            force_thinking: true,
        },
        // ── Kimi / Moonshot ──
        "kimi-k2.7-code" => ModelCapabilities {
            supports_image_url: false, supports_image_base64: true,
            supports_video_url: false, supports_video_base64: true,
            force_thinking: true,
        },
        "kimi-k2.6" => ModelCapabilities {
            supports_image_url: false, supports_image_base64: true,
            supports_video_url: false, supports_video_base64: true,
            force_thinking: false,
        },
        // ── Unknown / custom ──
        _ => ModelCapabilities {
            supports_image_url: false, supports_image_base64: false,
            supports_video_url: false, supports_video_base64: false,
            force_thinking: false,
        },
    }
}

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
    pub supports_count_tokens: bool,
}

#[derive(Clone, Debug)]
pub enum ThinkingOverride {
    /// Inject thinking: { type: "disabled" } when user hasn't set it (skip MiniMax)
    Disabled,
    /// Inject thinking: { type: "enabled" } when user hasn't set it
    Enabled,
    /// Force thinking: { type: "enabled" }, overriding any user setting
    Forced,
    /// Do not inject anything
    Default,
}

#[derive(Clone)]
pub struct ModelRouteEntry {
    pub provider_id: String,
    pub upstream_model: String,
    pub thinking: ThinkingOverride,
    /// If true, always inject `thinking: { type: "enabled" }`
    pub force_thinking: bool,
    /// Can receive image blocks with source.type = "url"
    pub supports_image_url: bool,
    /// Can receive image blocks with source.type = "base64"
    pub supports_image_base64: bool,
    /// Can receive video blocks with source.type = "url"
    pub supports_video_url: bool,
    /// Can receive video blocks with source.type = "base64"
    pub supports_video_base64: bool,
}

#[derive(Clone)]
pub struct ProxyConfig {
    /// gateway_model → routing info
    pub model_route: HashMap<String, ModelRouteEntry>,
    /// provider_id → route info
    pub providers: HashMap<String, ProviderRoute>,
    /// Fallback provider id
    pub fallback_provider: String,
    /// All visible model names in display order (for /v1/models)
    pub all_models: Vec<String>,
    pub server_host: String,
    pub server_port: u16,
    pub enable_cors: bool,
    /// Policy for handling image blocks when routing to non-vision models
    pub non_vision_image_policy: String,
}

pub fn resolve_proxy_config(cfg: &GatewayConfigResponse) -> Result<ProxyConfig, String> {
    let mut providers: HashMap<String, ProviderRoute> = HashMap::new();
    let mut model_route: HashMap<String, ModelRouteEntry> = HashMap::new();
    let mut all_models: Vec<String> = Vec::new();

    let active = cfg.active_provider.as_deref();

    // Process providers in stable order
    let mut provider_ids: Vec<&String> = cfg.providers.keys().collect();
    provider_ids.sort();

    // ── Pass 1: Build model route table from active provider only ──
    let effective_active = active.or_else(|| {
        let mut ids: Vec<&String> = cfg.providers.keys().collect();
        ids.sort();
        ids.first().map(|s| s.as_str())
    });

    for provider_id in &provider_ids {
        let is_active = Some(provider_id.as_str()) == effective_active;
        if !is_active {
            continue; // Only the active provider's models are routed
        }
        let p = &cfg.providers[*provider_id];

        if let Some(ref models) = p.models {
            let mut model_names: Vec<&String> = models.keys().collect();
            model_names.sort();
            for gateway_model in model_names {
                let entry = &models[gateway_model];
                let thinking = match entry.thinking_mode.as_deref() {
                    Some("normal") => ThinkingOverride::Disabled,
                    Some("thinking") => ThinkingOverride::Enabled,
                    Some("thinking_only") => ThinkingOverride::Forced,
                    _ => {
                        // Backward compat: derive from force_thinking / thinking fields
                        if entry.force_thinking.unwrap_or(false) {
                            ThinkingOverride::Forced
                        } else if entry.thinking.as_deref() == Some("disabled") {
                            ThinkingOverride::Disabled
                        } else {
                            ThinkingOverride::Default
                        }
                    }
                };

                // Resolve capabilities from upstream_model name (app code, not config.json)
                let caps = resolve_model_capabilities(&entry.upstream_model);
                let force_thinking = caps.force_thinking;
                let supports_image_url = caps.supports_image_url;
                let supports_image_base64 = caps.supports_image_base64;
                let supports_video_url = caps.supports_video_url;
                let supports_video_base64 = caps.supports_video_base64;

                // Active provider wins on model name collision; first non-active provider wins otherwise
                if model_route.contains_key(gateway_model) && !is_active {
                    continue;
                }
                model_route.insert(
                    gateway_model.clone(),
                    ModelRouteEntry {
                        provider_id: (*provider_id).clone(),
                        upstream_model: entry.upstream_model.clone(),
                        thinking,
                        force_thinking,
                        supports_image_url,
                        supports_image_base64,
                        supports_video_url,
                        supports_video_base64,
                    },
                );
                if entry.visible && !all_models.contains(gateway_model) {
                    all_models.push(gateway_model.clone());
                }
            }
        } else {
            // Fallback to legacy model_map — route all aliases, but only expose visible_models
            let visible_set: std::collections::HashSet<&String> = p.visible_models.iter().collect();
            let mut m_names: Vec<&String> = p.model_map.keys().collect();
            m_names.sort();
            for gateway_model in m_names {
                let upstream_model = &p.model_map[gateway_model];

                // Active provider wins on model name collision
                if model_route.contains_key(gateway_model) && !is_active {
                    continue;
                }
                model_route.insert(
                    gateway_model.clone(),
                    ModelRouteEntry {
                        provider_id: (*provider_id).clone(),
                        upstream_model: upstream_model.clone(),
                        thinking: ThinkingOverride::Default,
                        force_thinking: false,
                        supports_image_url: p.supports_vision,
                        supports_image_base64: p.supports_vision,
                        supports_video_url: p.supports_video,
                        supports_video_base64: p.supports_video,
                    },
                );
                if visible_set.contains(gateway_model) && !all_models.contains(gateway_model) {
                    all_models.push(gateway_model.clone());
                }
            }
        }
    }

    if model_route.is_empty() {
        return Err("No models configured. Add models or model_map entries to config.json.".into());
    }

    // ── Pass 2: Only check API keys for providers actually referenced by the route table ──
    let referenced_providers: std::collections::HashSet<&String> =
        model_route.values().map(|e| &e.provider_id).collect();

    for provider_id in &provider_ids {
        if !referenced_providers.contains(provider_id) {
            continue; // Skip providers not used by any active model route
        }
        let p = &cfg.providers[*provider_id];
        let api_key = std::env::var(&p.api_key_env).map_err(|_| {
            format!(
                "{} not set — set it in the API Key tab first.",
                p.api_key_env
            )
        })?;

        providers.insert(
            (*provider_id).clone(),
            ProviderRoute {
                display_name: p.display_name.clone(),
                upstream_url: p.upstream_url.clone(),
                api_key,
                api_key_env: p.api_key_env.clone(),
                force_anthropic_version: p.force_anthropic_version.clone(),
                supports_count_tokens: p.supports_count_tokens,
            },
        );
    }

    let fallback = cfg
        .active_provider
        .clone()
        .or_else(|| cfg.providers.keys().next().cloned())
        .unwrap_or_default();

    // Debug: log each model's resolved capability set
    for (gw_model, entry) in &model_route {
        tracing::info!(
            "model route: {} -> {} | provider={} | img_url={} img_b64={} vid_url={} vid_b64={} force_thinking={} thinking={:?}",
            gw_model,
            entry.upstream_model,
            entry.provider_id,
            entry.supports_image_url,
            entry.supports_image_base64,
            entry.supports_video_url,
            entry.supports_video_base64,
            entry.force_thinking,
            entry.thinking,
        );
    }

    Ok(ProxyConfig {
        model_route,
        providers,
        fallback_provider: fallback,
        all_models,
        server_host: cfg.server.host.clone(),
        server_port: cfg.server.port,
        enable_cors: cfg.server.enable_cors,
        non_vision_image_policy: cfg.non_vision_image_policy.clone(),
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

/// Copy only safe (non-hop-by-hop) response headers from upstream to downstream.
/// Hop-by-hop headers must not be forwarded by proxies per RFC 7230 §6.1.
fn copy_safe_response_headers(src: &HeaderMap, dst: &mut HeaderMap) {
    const BLOCKED: &[&str] = &[
        "connection",
        "keep-alive",
        "proxy-authenticate",
        "proxy-authorization",
        "te",
        "trailer",
        "transfer-encoding",
        "upgrade",
        // Also strip these to avoid conflicts with axum/hyper handling:
        "content-length",
        "content-encoding",
    ];

    for (name, value) in src.iter() {
        let key = name.as_str().to_ascii_lowercase();
        if BLOCKED.contains(&key.as_str()) {
            continue;
        }
        dst.insert(name.clone(), value.clone());
    }
}

/// Look up a model and return (entry, provider_route).
fn resolve_model<'a>(
    model: &str,
    config: &'a ProxyConfig,
) -> Result<(&'a ModelRouteEntry, &'a ProviderRoute), (StatusCode, Json<Value>)> {
    let entry = config.model_route.get(model).ok_or_else(|| {
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

    let route = config.providers.get(&entry.provider_id).ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "server_error",
                    "message": format!("Provider '{}' not found for model '{}'", entry.provider_id, model)
                }
            })),
        )
    })?;

    Ok((entry, route))
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

// ---------------------------------------------------------------------------
// Media sanitization with granular source-type awareness
// ---------------------------------------------------------------------------

/// Content types recognized as image blocks.
const IMAGE_BLOCK_TYPES: &[&str] = &["image", "input_image", "image_url"];

/// Placeholder text inserted when an image block is replaced.
const IMAGE_PLACEHOLDER: &str = "[Image omitted: the selected backend model does not support this image format. If the image is needed, switch to a compatible model.]";

fn is_image_block(block: &Value) -> bool {
    block.get("type")
        .and_then(|v| v.as_str())
        .map(|t| IMAGE_BLOCK_TYPES.contains(&t))
        .unwrap_or(false)
}

/// Classify an image block's source type: "url" or "base64".
/// - Anthropic format: type="image" or "input_image" with source.type
/// - OpenAI-compatible: type="image_url" (always URL)
fn classify_image_source(block: &Value) -> Option<&str> {
    let block_type = block.get("type").and_then(|v| v.as_str())?;
    match block_type {
        "image_url" => Some("url"),
        "image" | "input_image" => block
            .get("source")
            .and_then(|s| s.get("type"))
            .and_then(|v| v.as_str()),
        _ => None,
    }
}

/// Classify a video block's source type: "url" or "base64".
fn classify_video_source(block: &Value) -> Option<&str> {
    let block_type = block.get("type").and_then(|v| v.as_str())?;
    if block_type != "video" {
        return None;
    }
    block
        .get("source")
        .and_then(|s| s.get("type"))
        .and_then(|v| v.as_str())
}

/// Recursively check if any unsupported image or video blocks exist.
fn has_unsupported_media(content: &Value, entry: &ModelRouteEntry) -> bool {
    match content {
        Value::Array(arr) => {
            for item in arr {
                if let Some(source_type) = classify_image_source(item) {
                    let supported = match source_type {
                        "url" => entry.supports_image_url,
                        "base64" => entry.supports_image_base64,
                        _ => false,
                    };
                    if !supported {
                        return true;
                    }
                } else if let Some(source_type) = classify_video_source(item) {
                    let supported = match source_type {
                        "url" => entry.supports_video_url,
                        "base64" => entry.supports_video_base64,
                        _ => false,
                    };
                    if !supported {
                        return true;
                    }
                }
                if let Some(inner) = item.get("content") {
                    if has_unsupported_media(inner, entry) {
                        return true;
                    }
                }
            }
            false
        }
        _ => false,
    }
}

/// Recursively count image blocks in content (handles tool_result.content nesting).
fn count_image_blocks_in_content(content: &Value) -> usize {
    match content {
        Value::Array(arr) => {
            let mut count = 0;
            for item in arr {
                if is_image_block(item) {
                    count += 1;
                }
                if let Some(inner) = item.get("content") {
                    count += count_image_blocks_in_content(inner);
                }
            }
            count
        }
        _ => 0,
    }
}

/// Count total image blocks across all messages.
fn count_image_blocks(messages: &[Value]) -> usize {
    let mut total = 0;
    for msg in messages {
        if let Some(content) = msg.get("content") {
            total += count_image_blocks_in_content(content);
        }
    }
    total
}

/// Recursively sanitize unsupported media blocks in place.
/// Returns the count of sanitized blocks.
fn sanitize_content_blocks_granular(content: &mut Value, policy: &str, entry: &ModelRouteEntry) -> usize {
    let mut count = 0;
    if let Value::Array(arr) = content {
        let mut i = 0;
        while i < arr.len() {
            let block = &arr[i];
            if let Some(source_type) = classify_image_source(block) {
                let supported = match source_type {
                    "url" => entry.supports_image_url,
                    "base64" => entry.supports_image_base64,
                    _ => false,
                };
                if !supported {
                    count += 1;
                    if policy == "replace" {
                        arr[i] = json!({"type": "text", "text": IMAGE_PLACEHOLDER});
                        i += 1;
                    } else {
                        arr.remove(i);
                        // Don't increment i — next element shifts into position
                    }
                } else {
                    i += 1;
                }
            } else if let Some(source_type) = classify_video_source(block) {
                let supported = match source_type {
                    "url" => entry.supports_video_url,
                    "base64" => entry.supports_video_base64,
                    _ => false,
                };
                if !supported {
                    count += 1;
                    // Video: always drop (placeholder text doesn't make sense for video)
                    arr.remove(i);
                    // Don't increment i
                } else {
                    i += 1;
                }
            } else {
                if let Some(inner) = arr[i].get_mut("content") {
                    count += sanitize_content_blocks_granular(inner, policy, entry);
                }
                i += 1;
            }
        }
        // If content is empty after dropping, insert placeholder
        if policy == "drop" && arr.is_empty() {
            arr.push(json!({"type": "text", "text": IMAGE_PLACEHOLDER}));
        }
    }
    count
}

/// Sanitize image/video blocks in the request body based on granular capabilities.
/// Returns (sanitized, image_block_count).
fn sanitize_body_images(
    body: &mut Value,
    entry: &ModelRouteEntry,
    policy: &str,
) -> (bool, usize) {
    // If model supports ALL image and video source types, skip entirely
    if entry.supports_image_url && entry.supports_image_base64
        && entry.supports_video_url && entry.supports_video_base64
    {
        return (false, 0);
    }

    let messages = match body.get_mut("messages").and_then(|v| v.as_array_mut()) {
        Some(arr) => arr,
        None => return (false, 0),
    };

    let count = count_image_blocks(messages);
    if count == 0 {
        return (false, 0);
    }

    if policy == "reject" {
        for msg in messages.iter() {
            if let Some(content) = msg.get("content") {
                if has_unsupported_media(content, entry) {
                    return (false, count); // Caller should reject
                }
            }
        }
        return (false, 0); // All media blocks are supported
    }

    let mut sanitized = 0;
    for msg in messages.iter_mut() {
        if let Some(content) = msg.get_mut("content") {
            sanitized += sanitize_content_blocks_granular(content, policy, entry);
        }
    }

    (sanitized > 0, count)
}

fn check_media_support(
    messages: &[Value],
    entry: &ModelRouteEntry,
    display_name: &str,
    non_vision_image_policy: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    let (has_image, has_video) = detect_media_types(messages);
    let no_image_support = !entry.supports_image_url && !entry.supports_image_base64;
    let no_video_support = !entry.supports_video_url && !entry.supports_video_base64;

    // Image: reject only when policy is "reject" and model supports NO image source
    if has_image && no_image_support && non_vision_image_policy == "reject" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "This conversation contains image input, but the selected backend model '{}' does not support vision. Start a text-only thread, switch to a vision-capable model, or set non_vision_image_policy to 'replace'.",
                        display_name
                    )
                }
            })),
        ));
    }

    // Video: hard-reject only when model supports NO video source type
    if has_video && no_video_support {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "invalid_request_error",
                    "message": format!(
                        "Model '{}' does not support video input.",
                        display_name
                    )
                }
            })),
        ));
    }

    // Reject policy: also reject if there are any unsupported media blocks
    // (handles partial support cases like Kimi: base64 OK, URL not OK)
    if non_vision_image_policy == "reject" {
        for msg in messages {
            if let Some(content) = msg.get("content") {
                if has_unsupported_media(content, entry) {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "type": "error",
                            "error": {
                                "type": "invalid_request_error",
                                "message": format!(
                                    "This conversation contains image/video input in a format not supported by the selected backend model '{}'. Use a compatible format or switch models.",
                                    display_name
                                )
                            }
                        })),
                    ));
                }
            }
        }
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
        "non_vision_image_policy": config.non_vision_image_policy,
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

    let model_in = body["model"].as_str().unwrap_or("").to_string();
    let (entry, route) = resolve_model(&model_in, &config)?;

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

    // Sanitize image blocks for non-vision models (same as proxy_messages)
    let (was_sanitized, image_count) = sanitize_body_images(
        &mut body,
        entry,
        &config.non_vision_image_policy,
    );

    // Check media support (rejects video always; rejects images only when policy == "reject")
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(messages, entry, &route.display_name, &config.non_vision_image_policy)?;
    }

    // Log sanitization info
    if image_count > 0 {
        tracing::info!(
            "POST /v1/messages/count_tokens | model: {} -> {} | provider: {} | image_blocks={} | image_policy={} | sanitized={}",
            model_in, entry.upstream_model, entry.provider_id,
            image_count, config.non_vision_image_policy, was_sanitized
        );
    }

    // Apply thinking override for count_tokens
    match entry.thinking {
        ThinkingOverride::Disabled => {
            if entry.provider_id != "minimax" {
                body["thinking"] = json!({"type": "disabled"});
            }
        }
        ThinkingOverride::Enabled | ThinkingOverride::Forced => {
            body["thinking"] = json!({"type": "enabled"});
        }
        ThinkingOverride::Default => {}
    }

    body["model"] = json!(entry.upstream_model);

    // Log final request routing info
    tracing::info!(
        "POST /v1/messages/count_tokens | claude_model={} upstream_model={} provider={} thinking_mode={:?} final_thinking={}",
        model_in,
        entry.upstream_model,
        entry.provider_id,
        entry.thinking,
        body.get("thinking").map_or("none".to_string(), |v| v.to_string()),
    );

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

    if !status.is_success() {
        let body_preview = String::from_utf8_lossy(&resp_body[..resp_body.len().min(500)]);
        tracing::warn!(
            "POST /v1/messages/count_tokens | upstream error status={} body={}",
            status.as_u16(),
            body_preview,
        );
    } else {
        tracing::info!("POST /v1/messages/count_tokens | status={}", status.as_u16());
    }

    let mut response = Response::new(Body::from(resp_body));
    *response.status_mut() = status;
    copy_safe_response_headers(&resp_headers, response.headers_mut());
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

    let model_in = body["model"].as_str().unwrap_or("").to_string();
    let (entry, route) = resolve_model(&model_in, &config)?;

    // Sanitize image blocks for non-vision models
    let (was_sanitized, image_count) = sanitize_body_images(
        &mut body,
        entry,
        &config.non_vision_image_policy,
    );

    // Check media support (rejects video always; rejects images only when policy == "reject")
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(messages, entry, &route.display_name, &config.non_vision_image_policy)?;
    }

    // Log sanitization info (no base64, no conversation text)
    if image_count > 0 {
        tracing::info!(
            "POST /v1/messages | model: {} -> {} | provider: {} | image_blocks={} | image_policy={} | sanitized={}",
            model_in, entry.upstream_model, entry.provider_id,
            image_count, config.non_vision_image_policy, was_sanitized
        );
    }

    // Apply thinking override based on thinking_mode
    match entry.thinking {
        ThinkingOverride::Disabled => {
            // Always inject disabled — do not rely on API defaults
            // Skip MiniMax: MiniMax-M3 returns content:null when thinking disabled is sent
            if entry.provider_id != "minimax" {
                body["thinking"] = json!({"type": "disabled"});
            }
        }
        ThinkingOverride::Enabled => {
            // Always inject enabled — do not rely on API defaults
            body["thinking"] = json!({"type": "enabled"});
        }
        ThinkingOverride::Forced => {
            // Always force thinking enabled (overrides user setting)
            let old_thinking = body.get("thinking").cloned();
            body["thinking"] = json!({"type": "enabled"});
            if old_thinking.as_ref().map_or(true, |v| v != &json!({"type": "enabled"})) {
                tracing::info!(
                    "POST /v1/messages | model: {} -> {} | thinking_mode=forced: injected thinking=enabled (was {:?})",
                    model_in, entry.upstream_model, old_thinking
                );
            }

            // Clean parameters for fixed-parameter models
            let mut cleaned = Vec::new();
            let allowed_params = [
                ("temperature", json!(1.0)),
                ("top_p", json!(0.95)),
                ("n", json!(1)),
                ("presence_penalty", json!(0.0)),
                ("frequency_penalty", json!(0.0)),
            ];
            for (key, allowed_val) in &allowed_params {
                if let Some(current) = body.get(*key) {
                    if current != allowed_val {
                        tracing::info!(
                            "POST /v1/messages | model: {} -> {} | param_clean: {} {:?} -> {}",
                            model_in, entry.upstream_model, key, current, allowed_val
                        );
                        body[*key] = allowed_val.clone();
                        cleaned.push(*key);
                    }
                } else {
                    body[*key] = allowed_val.clone();
                    cleaned.push(*key);
                }
            }
            if !cleaned.is_empty() {
                tracing::info!(
                    "POST /v1/messages | model: {} -> {} | params_set: {}",
                    model_in, entry.upstream_model, cleaned.join(", ")
                );
            }
        }
        ThinkingOverride::Default => {
            // Pass through whatever the user sent
        }
    }

    // Rewrite model to upstream model name
    body["model"] = json!(entry.upstream_model);

    // Log final request routing info
    tracing::info!(
        "POST /v1/messages | claude_model={} upstream_model={} provider={} thinking_mode={:?} final_thinking={}",
        model_in,
        entry.upstream_model,
        entry.provider_id,
        entry.thinking,
        body.get("thinking").map_or("none".to_string(), |v| v.to_string()),
    );

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

    if !status.is_success() {
        let body_preview = String::from_utf8_lossy(&resp_body[..resp_body.len().min(500)]);
        tracing::warn!(
            "POST /v1/messages | upstream error status={} body={}",
            status.as_u16(),
            body_preview,
        );
    } else {
        tracing::info!("POST /v1/messages | status={}", status.as_u16());
    }

    let mut response = Response::new(Body::from(resp_body));
    *response.status_mut() = status;
    copy_safe_response_headers(&resp_headers, response.headers_mut());

    // Ensure Content-Type is set (fallback to application/json for API responses)
    if !response.headers().contains_key("content-type") {
        response
            .headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
    }

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
        let body_preview = &body[..body.len().min(500)];
        tracing::warn!(
            "POST /v1/messages (stream) | upstream error status={} body={}",
            status.as_u16(),
            body_preview,
        );
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
