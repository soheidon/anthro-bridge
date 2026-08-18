use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;

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

use crate::model_capabilities;
use crate::openrouter;
use crate::GatewayConfigResponse;

// ---------------------------------------------------------------------------
// Request ID counter
// ---------------------------------------------------------------------------

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);

// ---------------------------------------------------------------------------
// Log context for model identity normalization
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ModelIdentityLogContext {
    request_id: u64,
    request_model: String,
    canonical_gateway_model: String,
    upstream_model: String,
}

// ---------------------------------------------------------------------------
// Normalization result types
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum NonstreamNormalizationOutcome {
    Changed {
        body: Vec<u8>,
        original_model: String,
    },
    AlreadyCanonical,
    ModelFieldMissing,
    InvalidResponseShape,
}

struct SseNormalizationResult {
    frame: Vec<u8>,
    original_model: String,
}

// ---------------------------------------------------------------------------
// Model capability resolver — re-exported from the shared model_capabilities
// module for backward compatibility. All capability logic lives in
// gui/src-tauri/src/model_capabilities.rs.
// ---------------------------------------------------------------------------

pub use model_capabilities::ModelCapabilities;

/// Resolve capabilities for a known upstream model.
/// Delegates to the shared static resolver.
pub fn resolve_model_capabilities(upstream_model: &str) -> ModelCapabilities {
    model_capabilities::resolve_static_model_capabilities(upstream_model)
}

// Re-export shared classification helpers for backward compatibility
use model_capabilities::{
    is_gemini_model, is_inclusionai_model, is_ling_free_model, is_ling_non_thinking_model,
    is_openai_gpt56_model, is_poolside_reasoning_model, is_stepfun_model, is_tencent_hy3,
};

// ---------------------------------------------------------------------------
// Resolved config for model-based multi-provider routing
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ProviderRoute {
    pub provider_id: String,
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
    /// Canonical Gateway model ID (e.g. "claude-opus-5", not the alias key)
    pub gateway_model: String,
    pub provider_id: String,
    pub upstream_model: String,
    pub thinking: ThinkingOverride,
    /// If true, always inject `thinking: { type: "enabled" }`
    pub force_thinking: bool,
    /// If set, inject `reasoning_effort` when thinking is enabled (DeepSeek Opus)
    pub reasoning_effort: Option<String>,
    /// Can receive image blocks with source.type = "url"
    pub supports_image_url: bool,
    /// Can receive image blocks with source.type = "base64"
    pub supports_image_base64: bool,
    /// Can receive video blocks with source.type = "url"
    pub supports_video_url: bool,
    /// Can receive video blocks with source.type = "base64"
    pub supports_video_base64: bool,
    /// If true, do NOT send `thinking` parameter upstream (K3)
    pub suppress_thinking_parameter: bool,
    /// If set, inject this reasoning_effort value (e.g. "max" for K3)
    pub forced_reasoning_effort: Option<String>,
    /// Raw thinking_mode from config (for OpenRouter reasoning translation)
    pub thinking_mode_raw: Option<String>,
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
    /// Runtime-updatable normalization toggle (shared with ProxyState)
    pub normalize_response_model_identity: Arc<AtomicBool>,
}

/// Validate a model entry's canonical reference.
/// Returns the canonical Gateway model ID to use for `ModelRouteEntry.gateway_model`.
fn validate_canonical_target<'a>(
    model_key: &str,
    entry: &crate::ModelEntry,
    models: &'a std::collections::HashMap<String, crate::ModelEntry>,
) -> Result<String, String> {
    let Some(canonical) = entry.canonical.as_deref() else {
        // No canonical set — this entry IS the canonical route
        return Ok(model_key.to_string());
    };
    if canonical == model_key {
        return Err(format!("model alias '{}' references itself", model_key));
    }
    let target = models.get(canonical).ok_or_else(|| {
        format!(
            "model alias '{}' references missing canonical model '{}'",
            model_key, canonical
        )
    })?;
    if target.canonical.is_some() {
        return Err(format!(
            "model alias '{}' references another alias '{}'",
            model_key, canonical
        ));
    }
    Ok(canonical.to_string())
}

/// Build the runtime proxy routing table from config.
///
/// Route→upstream extraction is delegated to the shared extractors in
/// `model_routing` (`resolve_from_models` / `resolve_from_model_map`), so the
/// proxy and the auto-compact resolver read the same upstream model for a
/// given route. The typed `models`/`model_map` branch below decides WHICH map
/// is authoritative (the shared functions never interpret field presence), and
/// the OpenRouter branch routes only from the active profile's `models`. If
/// the typed branch and the shared resolver ever drift,
/// `proxy_routes_agree_with_model_routing_resolution` will fail.
pub fn resolve_proxy_config(
    cfg: &GatewayConfigResponse,
    openrouter_models: &[openrouter::OpenRouterModel],
    normalize_response_model_identity: Arc<AtomicBool>,
) -> Result<ProxyConfig, String> {
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

        // ── OpenRouter: route from the active profile ──
        if *provider_id == "openrouter" && !p.openrouter_profiles.is_empty() {
            let active_id = cfg.active_openrouter_profile_id.as_deref();
            let active_profile = p
                .openrouter_profiles
                .iter()
                .find(|prof| Some(prof.id.as_str()) == active_id)
                .unwrap_or(&p.openrouter_profiles[0]); // transient fallback

            let profile_raw = serde_json::to_value(active_profile).map_err(|e| {
                format!(
                    "failed to serialize OpenRouter profile '{}' routing config: {e}",
                    active_profile.id
                )
            })?;

            let mut model_names: Vec<&String> = active_profile.models.keys().collect();
            model_names.sort();
            for gateway_model in model_names {
                let entry = &active_profile.models[gateway_model];
                // OpenRouter: always pass through — do not override thinking/budget
                let thinking = ThinkingOverride::Default;

                // Resolve capabilities — shared effective resolver
                let cache_lookup = if openrouter_models.is_empty() {
                    model_capabilities::OpenRouterCacheLookup::Unavailable
                } else if let Some(m) = openrouter_models
                    .iter()
                    .find(|m| m.id == entry.upstream_model)
                {
                    model_capabilities::OpenRouterCacheLookup::Hit(m)
                } else {
                    model_capabilities::OpenRouterCacheLookup::Miss
                };
                let caps = model_capabilities::resolve_effective_model_capabilities(
                    &entry.upstream_model,
                    p.supports_vision,
                    cache_lookup,
                );

                if model_route.contains_key(gateway_model) && !is_active {
                    continue;
                }

                let resolved_gateway_model =
                    match validate_canonical_target(gateway_model, entry, &active_profile.models) {
                        Ok(m) => m,
                        Err(error) => {
                            tracing::warn!(
                                provider = %provider_id,
                                model = %gateway_model,
                                profile = %active_profile.id,
                                error = %error,
                                "Skipping invalid model alias"
                            );
                            continue;
                        }
                    };

                let shared_result =
                    crate::model_routing::resolve_from_models(&profile_raw, gateway_model);
                debug_assert_eq!(
                    shared_result.as_deref(),
                    Some(entry.upstream_model.as_str()),
                    "route '{}' diverged after serialization",
                    gateway_model
                );
                let upstream_model = shared_result.ok_or_else(|| {
                    format!(
                        "route '{}' could not be resolved from serialized OpenRouter profile '{}'",
                        gateway_model, active_profile.id
                    )
                })?;

                tracing::debug!(
                    profile_id = %active_profile.id,
                    gateway_model = %gateway_model,
                    upstream = %entry.upstream_model,
                    "Route built from OpenRouter profile"
                );

                model_route.insert(
                    gateway_model.clone(),
                    ModelRouteEntry {
                        gateway_model: resolved_gateway_model,
                        provider_id: (*provider_id).clone(),
                        upstream_model: upstream_model,
                        thinking,
                        force_thinking: caps.force_thinking,
                        reasoning_effort: entry.reasoning_effort.clone(),
                        supports_image_url: caps.supports_image_url,
                        supports_image_base64: caps.supports_image_base64,
                        supports_video_url: caps.supports_video_url,
                        supports_video_base64: caps.supports_video_base64,
                        suppress_thinking_parameter: caps.suppress_thinking_parameter,
                        forced_reasoning_effort: caps.forced_reasoning_effort.map(String::from),
                        thinking_mode_raw: entry.thinking_mode.clone(),
                    },
                );
                if entry.visible && !all_models.contains(gateway_model) {
                    all_models.push(gateway_model.clone());
                }
            }
            // Skip the legacy models/model_map fallthrough — profiles take over
            continue;
        }

        // Serialize the active provider once so the shared model_routing
        // extractors can read the raw scope. The typed `if let Some(models)`
        // branch decides which map is authoritative — the shared functions
        // never interpret field presence — so this re-serialization cannot
        // change the models-vs-model_map decision.
        let provider_raw = serde_json::to_value(p).map_err(|e| {
            format!(
                "failed to serialize provider '{}' routing config: {e}",
                provider_id
            )
        })?;

        if let Some(ref models) = p.models {
            let mut model_names: Vec<&String> = models.keys().collect();
            model_names.sort();
            for gateway_model in model_names {
                let entry = &models[gateway_model];
                let thinking = if *provider_id == "openrouter" {
                    // OpenRouter: always pass through — do not override thinking/budget from Claude Code
                    ThinkingOverride::Default
                } else {
                    match entry.thinking_mode.as_deref() {
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
                    }
                };

                // Resolve capabilities — dynamic for OpenRouter, static for other providers.
                //
                // For OpenRouter, `force_thinking` is always `false` (the only
                // existing model that was thinking-only via OpenRouter — Laguna —
                // uses saved-config-driven reasoning translation rather than the
                // proxy forcing thinking). The semantic of `force_thinking` here
                // is "force-inject thinking: {type: enabled} regardless of the
                // request body's `thinking` field", which OpenRouter does not
                // need.
                //
                // For Hy3 (tencent/hy3, tencent/hy3:free) we explicitly keep
                // `force_thinking = false` so it's never accidentally turned on
                // by changing upstream capability plumbing; this is the same
                // answer as the generic OpenRouter branch but is documented here
                // to make the Hy3 contract explicit.
                let (
                    force_thinking,
                    supports_image_url,
                    supports_image_base64,
                    supports_video_url,
                    supports_video_base64,
                    suppress_thinking_parameter,
                    forced_reasoning_effort,
                ) = if *provider_id == "openrouter" && !openrouter_models.is_empty() {
                    if let Some((vis, vid, _think, _tools)) =
                        openrouter::resolve_capabilities_from_cache(
                            &entry.upstream_model,
                            openrouter_models,
                        )
                    {
                        (false, vis, vis, vid, vid, false, None)
                    } else {
                        // Custom model — unknown capabilities, don't strip anything.
                        (false, true, true, false, false, false, None)
                    }
                } else if *provider_id == "openrouter" {
                    // No cache yet — conservative defaults (video unknown without cache).
                    (
                        false,
                        p.supports_vision,
                        p.supports_vision,
                        false,
                        false,
                        false,
                        None,
                    )
                } else {
                    let caps = resolve_model_capabilities(&entry.upstream_model);
                    (
                        caps.force_thinking,
                        caps.supports_image_url,
                        caps.supports_image_base64,
                        caps.supports_video_url,
                        caps.supports_video_base64,
                        caps.suppress_thinking_parameter,
                        caps.forced_reasoning_effort.map(|s| s.to_string()),
                    )
                };

                // Hy3 contract: force_thinking always false.
                let force_thinking = if is_tencent_hy3(&entry.upstream_model) {
                    false
                } else {
                    force_thinking
                };

                // Active provider wins on model name collision; first non-active provider wins otherwise
                if model_route.contains_key(gateway_model) && !is_active {
                    continue;
                }

                // Validate canonical reference (same Provider only)
                let resolved_gateway_model =
                    match validate_canonical_target(gateway_model, entry, models) {
                        Ok(m) => m,
                        Err(error) => {
                            tracing::warn!(
                                provider = %provider_id,
                                model = %gateway_model,
                                error = %error,
                                "Skipping invalid model alias"
                            );
                            continue;
                        }
                    };

                let shared_result =
                    crate::model_routing::resolve_from_models(&provider_raw, gateway_model);
                debug_assert_eq!(
                    shared_result.as_deref(),
                    Some(entry.upstream_model.as_str()),
                    "route '{}' diverged after serialization",
                    gateway_model
                );
                let upstream_model = shared_result.ok_or_else(|| {
                    format!(
                        "route '{}' could not be resolved from serialized models of provider '{}'",
                        gateway_model, provider_id
                    )
                })?;

                model_route.insert(
                    gateway_model.clone(),
                    ModelRouteEntry {
                        gateway_model: resolved_gateway_model,
                        provider_id: (*provider_id).clone(),
                        upstream_model: upstream_model,
                        thinking,
                        force_thinking,
                        reasoning_effort: entry.reasoning_effort.clone(),
                        supports_image_url,
                        supports_image_base64,
                        supports_video_url,
                        supports_video_base64,
                        suppress_thinking_parameter,
                        forced_reasoning_effort,
                        thinking_mode_raw: entry.thinking_mode.clone(),
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
                let upstream_model = crate::model_routing::resolve_from_model_map(
                    &provider_raw,
                    gateway_model,
                )
                .ok_or_else(|| {
                    format!(
                        "route '{}' could not be resolved from legacy model_map of provider '{}'",
                        gateway_model, provider_id
                    )
                })?;

                // Active provider wins on model name collision
                if model_route.contains_key(gateway_model) && !is_active {
                    continue;
                }
                model_route.insert(
                    gateway_model.clone(),
                    ModelRouteEntry {
                        gateway_model: gateway_model.clone(), // legacy: no canonical, key is its own identity
                        provider_id: (*provider_id).clone(),
                        upstream_model: upstream_model,
                        thinking: ThinkingOverride::Default,
                        force_thinking: false,
                        reasoning_effort: None,
                        supports_image_url: p.supports_vision,
                        supports_image_base64: p.supports_vision,
                        supports_video_url: p.supports_video,
                        supports_video_base64: p.supports_video,
                        suppress_thinking_parameter: false,
                        forced_reasoning_effort: None,
                        thinking_mode_raw: None,
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
                provider_id: (*provider_id).clone(),
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
        normalize_response_model_identity,
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
        // Strip to avoid conflicts with axum/hyper handling:
        "content-length",
        // NOTE: Content-Encoding is NOT blocked — it is an end-to-end
        // representation header. Non-identity encoded bodies pass through
        // with their Content-Encoding header intact.
    ];

    for (name, value) in src.iter() {
        let key = name.as_str().to_ascii_lowercase();
        if BLOCKED.contains(&key.as_str()) {
            continue;
        }
        dst.insert(name.clone(), value.clone());
    }
}

/// Determine whether a response's Content-Encoding allows in-place body transformation.
/// - No header → transformable (identity by default)
/// - `identity` (case-insensitive, trimmed) → transformable
/// - Any other value, or a value that cannot be decoded as UTF-8 → NOT transformable
fn is_transformable_content_encoding(headers: &HeaderMap) -> bool {
    match headers.get("content-encoding") {
        None => true,
        Some(value) => value
            .to_str()
            .map(|encoding| encoding.trim().eq_ignore_ascii_case("identity"))
            .unwrap_or(false),
    }
}

/// Char-boundary-safe string truncation. Avoids panics when slicing UTF-8
/// strings at arbitrary byte offsets.
fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

// ── Response Model Normalization ──────────────────────────────────────────────

/// Detected token-cap failure kind. Returned by diagnostic observers when a
/// reasoning model produces reasoning-only output after hitting a limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenCapFailureKind {
    /// Anthropic Messages: stop_reason="max_tokens" with reasoning but no
    /// non-empty text or tool_use blocks.
    AnthropicMaxTokens,
}

/// Lightweight SSE event observer that tracks stream content to detect
/// the reasoning-only token-cap failure pattern.
///
/// State machine: accumulates evidence across SSE events, finalizes on
/// `message_stop`. Whitespace-only text is treated as empty.
#[derive(Debug, Clone)]
struct TokenCapDiagnosticState {
    has_reasoning: bool,
    has_nonempty_text: bool,
    has_tool_use: bool,
    stop_reason: Option<String>,
    saw_message_stop: bool,
    warning_emitted: bool,
}

impl TokenCapDiagnosticState {
    fn new() -> Self {
        Self {
            has_reasoning: false,
            has_nonempty_text: false,
            has_tool_use: false,
            stop_reason: None,
            saw_message_stop: false,
            warning_emitted: false,
        }
    }

    /// Observe a parsed SSE event JSON. Call for each `data:` line value.
    /// Returns `Some(TokenCapFailureKind)` when the diagnostic pattern is
    /// confirmed at `message_stop`.
    fn observe(&mut self, event: &serde_json::Value) -> Option<TokenCapFailureKind> {
        if self.warning_emitted {
            return None;
        }

        let obj = match event.as_object() {
            Some(o) => o,
            None => return None,
        };

        let event_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match event_type {
            "content_block_start" => {
                if let Some(block) = obj.get("content_block").and_then(|v| v.as_object()) {
                    let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match block_type {
                        "thinking" | "redacted_thinking" => self.has_reasoning = true,
                        "tool_use" => self.has_tool_use = true,
                        "text" => {
                            let text = block
                                .get("text")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !text.trim().is_empty() {
                                self.has_nonempty_text = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Some(delta) = obj.get("delta").and_then(|v| v.as_object()) {
                    let delta_type = delta.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if delta_type == "text_delta" {
                        let text = delta.get("text").and_then(|v| v.as_str()).unwrap_or("");
                        if !text.trim().is_empty() {
                            self.has_nonempty_text = true;
                        }
                    }
                }
            }
            "message_delta" => {
                if let Some(delta) = obj.get("delta").and_then(|v| v.as_object()) {
                    if let Some(sr) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                        if sr == "max_tokens" {
                            self.stop_reason = Some(sr.to_string());
                        }
                    }
                }
            }
            "message_stop" => {
                self.saw_message_stop = true;
                return self.finalize();
            }
            _ => {}
        }

        None
    }

    /// Finalize the diagnostic: return `Some` only when all conditions are met
    /// and the pattern hasn't been warned about yet.
    fn finalize(&mut self) -> Option<TokenCapFailureKind> {
        if self.warning_emitted {
            return None;
        }
        if !self.has_reasoning {
            return None;
        }
        if self.has_nonempty_text || self.has_tool_use {
            return None;
        }
        if self.stop_reason.as_deref() != Some("max_tokens") {
            return None;
        }
        if !self.saw_message_stop {
            return None;
        }
        self.warning_emitted = true;
        Some(TokenCapFailureKind::AnthropicMaxTokens)
    }
}

/// Check a non-stream Anthropic Messages JSON response for the reasoning-only
/// token-cap failure pattern. Returns `Some` when:
/// - `stop_reason == "max_tokens"`
/// - At least one `type: "thinking"` or `"redacted_thinking"` block present
/// - No non-empty `type: "text"` or `type: "tool_use"` blocks
/// - `stop_reason == "end_turn"` is explicitly NOT a failure
/// Map Anthro Bridge reasoning effort values to DeepSeek's effective levels.
///
/// DeepSeek official API (V4-Pro-0813 / V4-Flash-0731):
///   low  → low,  medium → high,  high → high,  xhigh → high,  max → max
///
/// Returns `None` for unrecognized values (caller should skip injection).
fn normalize_deepseek_reasoning_effort(effort: &str) -> Option<&'static str> {
    match effort {
        "low" => Some("low"),
        "medium" | "high" | "xhigh" => Some("high"),
        "max" => Some("max"),
        _ => None,
    }
}

/// Apply DeepSeek-specific reasoning effort to the request payload.
///
/// DeepSeek uses `output_config.effort` (not the flat `reasoning_effort` key).
/// This function:
///   1. Unconditionally removes stale `reasoning_effort` and `output_config.effort`
///   2. If thinking is enabled and effort is valid, inserts normalized effort
///   3. Cleans up empty `output_config`
fn apply_deepseek_reasoning_effort(body: &mut serde_json::Value, effort: Option<&str>) {
    // 1) Unconditionally remove old representations.
    if let Some(obj) = body.as_object_mut() {
        obj.remove("reasoning_effort");
    }
    if let Some(oc) = body
        .get_mut("output_config")
        .and_then(|v| v.as_object_mut())
    {
        oc.remove("effort");
    }

    // 2) If thinking is enabled and effort is valid, insert normalized value.
    let thinking_enabled = matches!(
        body.get("thinking"),
        Some(v) if v.get("type").and_then(|t| t.as_str()) == Some("enabled")
    );
    if thinking_enabled {
        if let Some(normalized) = effort.and_then(normalize_deepseek_reasoning_effort) {
            let oc = body
                .as_object_mut()
                .and_then(|obj| {
                    obj.entry("output_config")
                        .or_insert_with(|| json!({}))
                        .as_object_mut()
                });
            if let Some(oc) = oc {
                oc.insert("effort".to_string(), json!(normalized));
            }
        }
    }

    // 3) Remove output_config itself only if it became empty.
    if let Some(oc) = body.get("output_config").and_then(|v| v.as_object()) {
        if oc.is_empty() {
            body.as_object_mut().map(|obj| obj.remove("output_config"));
        }
    }
}

/// Apply reasoning effort for direct providers other than DeepSeek and OpenRouter.
/// MiniMax, Kimi, MiMo: flat `reasoning_effort` key when thinking is enabled.
fn apply_direct_provider_reasoning_effort(body: &mut serde_json::Value, effort: Option<&str>) {
    if let Some(effort) = effort {
        if matches!(
            body.get("thinking"),
            Some(v) if v.get("type").and_then(|t| t.as_str()) == Some("enabled")
        ) {
            body["reasoning_effort"] = json!(effort);
        }
    }
}

/// Apply thinking override based on `entry.thinking`.
///
/// Replicates the thinking-override logic from proxy_messages():
/// - MiniMax M3: delegated to `apply_thinking_override_for_minimax_m3`
/// - Disabled: inject `thinking: disabled` (skip MiniMax M2.x)
/// - Enabled: inject `thinking: enabled`
/// - Forced + suppress: remove thinking, inject reasoning_effort (K3)
/// - Forced: force `thinking: enabled`, clean params for fixed-parameter models
/// - Default: pass through
fn apply_thinking_override(body: &mut serde_json::Value, entry: &ModelRouteEntry) {
    if apply_thinking_override_for_minimax_m3(entry, body) {
        return;
    }

    match entry.thinking {
        ThinkingOverride::Disabled => {
            // Skip MiniMax M2.x: MiniMax returns content:null when thinking disabled is sent
            if entry.provider_id != "minimax" {
                body["thinking"] = json!({"type": "disabled"});
            }
        }
        ThinkingOverride::Enabled => {
            body["thinking"] = json!({"type": "enabled"});
        }
        ThinkingOverride::Forced => {
            if entry.suppress_thinking_parameter {
                // K3: do NOT send thinking parameter; use reasoning_effort from config instead
                body.as_object_mut().map(|o| o.remove("thinking"));
                let effort = entry
                    .forced_reasoning_effort
                    .as_deref()
                    .or(entry.reasoning_effort.as_deref())
                    .unwrap_or("max");
                body["reasoning_effort"] = json!(effort);
                tracing::info!(
                    "POST /v1/messages | model: {} -> {} | thinking_mode=forced+suppress: removed thinking, reasoning_effort={}",
                    entry.gateway_model, entry.upstream_model, effort
                );

                let params_to_remove = [
                    "temperature",
                    "top_p",
                    "n",
                    "presence_penalty",
                    "frequency_penalty",
                ];
                for key in &params_to_remove {
                    if body
                        .as_object_mut()
                        .map_or(false, |o| o.remove(*key).is_some())
                    {
                        tracing::info!(
                            "POST /v1/messages | model: {} -> {} | param_removed: {}",
                            entry.gateway_model,
                            entry.upstream_model,
                            key
                        );
                    }
                }
            } else {
                let old_thinking = body.get("thinking").cloned();
                body["thinking"] = json!({"type": "enabled"});
                if old_thinking
                    .as_ref()
                    .map_or(true, |v| v != &json!({"type": "enabled"}))
                {
                    tracing::info!(
                        "POST /v1/messages | model: {} -> {} | thinking_mode=forced: injected thinking=enabled (was {:?})",
                        entry.gateway_model, entry.upstream_model, old_thinking
                    );
                }

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
                                entry.gateway_model, entry.upstream_model, key, current, allowed_val
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
                        entry.gateway_model,
                        entry.upstream_model,
                        cleaned.join(", ")
                    );
                }
            }
        }
        ThinkingOverride::Default => {}
    }
}

/// Apply thinking override + provider-specific reasoning effort injection.
///
/// This is the production function called by `proxy_messages()`.
/// Tests call the same function to validate the actual payload path.
fn apply_route_request_transforms(body: &mut serde_json::Value, entry: &ModelRouteEntry) {
    apply_thinking_override(body, entry);

    if entry.provider_id == "deepseek" {
        apply_deepseek_reasoning_effort(body, entry.reasoning_effort.as_deref());
    } else if entry.provider_id != "openrouter" {
        apply_direct_provider_reasoning_effort(body, entry.reasoning_effort.as_deref());
    }
}

fn detect_nonstream_token_cap_failure(body: &serde_json::Value) -> Option<TokenCapFailureKind> {
    let obj = body.as_object()?;
    let stop_reason = obj.get("stop_reason")?.as_str()?;
    if stop_reason != "max_tokens" {
        return None;
    }
    let content = obj.get("content")?.as_array()?;
    let mut has_reasoning = false;
    let mut has_nonempty_text = false;
    let mut has_tool_use = false;
    for block in content {
        let block_obj = match block.as_object() {
            Some(o) => o,
            None => continue,
        };
        let block_type = block_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "thinking" | "redacted_thinking" => has_reasoning = true,
            "tool_use" => has_tool_use = true,
            "text" => {
                let text = block_obj.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if !text.trim().is_empty() {
                    has_nonempty_text = true;
                }
            }
            _ => {}
        }
    }
    if has_reasoning && !has_nonempty_text && !has_tool_use {
        Some(TokenCapFailureKind::AnthropicMaxTokens)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamTerminalOutcome {
    Normalized,
    NoModelChangeObserved,
    StreamError,
    StreamCancelled,
}

impl StreamTerminalOutcome {
    fn as_skip_reason(self) -> Option<&'static str> {
        match self {
            Self::Normalized => None,
            Self::NoModelChangeObserved => Some("no_model_change_observed"),
            Self::StreamError => Some("stream_error"),
            Self::StreamCancelled => Some("stream_cancelled"),
        }
    }
}

/// Normalize the `model` field in a non-streaming JSON response body.
/// Returns a typed outcome so every call path produces exactly one log entry.
fn normalize_nonstream_model(
    body_bytes: &[u8],
    gateway_model: &str,
) -> NonstreamNormalizationOutcome {
    let mut v: Value = match serde_json::from_slice(body_bytes) {
        Ok(v) => v,
        Err(_) => return NonstreamNormalizationOutcome::InvalidResponseShape,
    };
    {
        let obj = match v.as_object() {
            Some(o) => o,
            None => return NonstreamNormalizationOutcome::InvalidResponseShape,
        };
        let m = match obj.get("model").and_then(|v| v.as_str()) {
            Some(m) => m,
            None => return NonstreamNormalizationOutcome::ModelFieldMissing,
        };
        if m == gateway_model {
            return NonstreamNormalizationOutcome::AlreadyCanonical;
        }
    }
    // Re-read model after borrow scope ends
    let original_model = v
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    v.as_object_mut()
        .unwrap()
        .insert("model".into(), Value::String(gateway_model.into()));
    match serde_json::to_vec(&v) {
        Ok(body) => NonstreamNormalizationOutcome::Changed {
            body,
            original_model,
        },
        Err(_) => NonstreamNormalizationOutcome::InvalidResponseShape,
    }
}

// ── SSE Frame-Based Normalization ────────────────────────────────────────────

/// Find the end position of a complete SSE frame in the buffer.
/// Supports both LF (`\n\n`) and CRLF (`\r\n\r\n`) delimiters.
/// When both are present, the one with the earlier start position wins.
/// Returns the end position (after the delimiter) suitable for drain.
fn find_sse_frame_end(buf: &[u8]) -> Option<usize> {
    // Look for \n\n (LF double-newline)
    let lf_pos = buf.windows(2).position(|w| w == b"\n\n");
    // Look for \r\n\r\n (CRLF double-newline)
    let crlf_pos = buf.windows(4).position(|w| w == b"\r\n\r\n");

    match (lf_pos, crlf_pos) {
        (Some(lf), Some(crlf)) => {
            // Earlier start position wins
            if lf <= crlf {
                Some(lf + 2) // after \n\n
            } else {
                Some(crlf + 4) // after \r\n\r\n
            }
        }
        (Some(lf), None) => Some(lf + 2),
        (None, Some(crlf)) => Some(crlf + 4),
        (None, None) => None,
    }
}

/// Check if a byte slice looks like a `data:` line prefix (LF or CRLF terminated).
/// Returns true if the line starts with "data:" or "data: ".
fn is_data_line(line: &[u8]) -> bool {
    line.starts_with(b"data:") || line.starts_with(b"data: ")
}

/// Find the byte range of the JSON value portion of a single `data:` line.
/// Returns the range within `frame` (after "data: " or "data:").
fn data_line_value_range(line: &[u8], line_start: usize) -> Option<std::ops::Range<usize>> {
    let (prefix_len, _) = if line.starts_with(b"data: ") {
        (6usize, true)
    } else if line.starts_with(b"data:") {
        (5usize, false)
    } else {
        return None;
    };
    let value_start = line_start + prefix_len;
    // Find end of value (before line ending)
    let line_end = line_start + line.len();
    // Strip trailing LF or CRLF from the value range
    let mut value_end = line_end;
    if line.ends_with(b"\r\n") {
        value_end -= 2;
    } else if line.ends_with(b"\n") {
        value_end -= 1;
    }
    if value_start >= value_end {
        return None;
    }
    Some(value_start..value_end)
}

/// Iterate lines in a frame (splitting on LF or CRLF) and count data: lines.
/// Returns (count, first_data_value_range).
/// Only returns the range when exactly one data: line is found.
fn find_single_sse_data_value(frame: &[u8]) -> Option<std::ops::Range<usize>> {
    let mut count = 0u32;
    let mut value_range: Option<std::ops::Range<usize>> = None;
    let mut pos = 0;

    while pos < frame.len() {
        // Find the end of this line (next LF)
        let lf_offset = frame[pos..].iter().position(|&b| b == b'\n');
        let line_end = match lf_offset {
            Some(off) => pos + off + 1, // include the LF
            None => frame.len(),        // last line without terminator
        };
        let line = &frame[pos..line_end];

        if is_data_line(line) {
            count += 1;
            if count == 1 {
                value_range = data_line_value_range(line, pos);
            } else {
                // More than one data: line — return None
                return None;
            }
        }

        pos = line_end;
    }

    if count == 1 {
        value_range
    } else {
        None
    }
}

/// Transform a complete SSE frame for model normalization.
/// Returns `Some(SseNormalizationResult)` if the frame has exactly one data: line containing a
/// message_start event with a message.model field that differs from gateway_model.
/// Returns `None` for passthrough (multiple data lines, non-message_start, etc.).
fn transform_complete_sse_frame(
    frame: &[u8],
    gateway_model: &str,
) -> Option<SseNormalizationResult> {
    let range = find_single_sse_data_value(frame)?;
    let data_bytes = &frame[range.clone()];
    let data_str = std::str::from_utf8(data_bytes).ok()?;
    let trimmed = data_str.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return None;
    }

    let leading_ws = data_str.len() - data_str.trim_start().len();
    let trailing_ws = data_str.len() - data_str.trim_end().len();

    let mut event: Value = serde_json::from_str(trimmed).ok()?;
    let obj = event.as_object_mut()?;

    if obj.get("type")?.as_str()? != "message_start" {
        return None;
    }

    let message = obj.get_mut("message")?.as_object_mut()?;
    let m = message.get("model")?.as_str()?;
    if m == gateway_model {
        return None;
    }
    let original_model = m.to_string();
    message.insert("model".into(), Value::String(gateway_model.into()));

    let new_json = serde_json::to_vec(&event).ok()?;

    // Rebuild: original prefix + leading ws + new JSON + trailing ws + original suffix
    let mut result = Vec::with_capacity(frame.len() + new_json.len());
    result.extend_from_slice(&frame[..range.start]);
    if leading_ws > 0 {
        result.extend_from_slice(&data_bytes[..leading_ws]);
    }
    result.extend_from_slice(&new_json);
    if trailing_ws > 0 {
        result.extend_from_slice(&data_bytes[data_bytes.len() - trailing_ws..]);
    }
    result.extend_from_slice(&frame[range.end..]);
    Some(SseNormalizationResult {
        frame: result,
        original_model,
    })
}

/// Diagnostic-only SSE stream wrapper. Parses each frame just enough to feed
/// the token-cap observer, then forwards the raw bytes unchanged. Used only for
/// the no-normalization path; the normalization path observes inside
/// `SseModelNormalizationStream` instead.
struct SseTokenCapDiagnosticStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    log_context: ModelIdentityLogContext,
    buffer: Vec<u8>,
    done: bool,
    diag: TokenCapDiagnosticState,
}

impl SseTokenCapDiagnosticStream {
    fn new(
        inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
        log_context: ModelIdentityLogContext,
    ) -> Self {
        Self {
            inner,
            log_context,
            buffer: Vec::with_capacity(8192),
            done: false,
            diag: TokenCapDiagnosticState::new(),
        }
    }
}

impl Stream for SseTokenCapDiagnosticStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(frame_end) = find_sse_frame_end(&self.buffer) {
                let frame = &self.buffer[..frame_end];
                // Observe frame for diagnostics without modifying it
                if !self.diag.warning_emitted {
                    if let Some(data_range) = find_single_sse_data_value(frame) {
                        let data_bytes = &frame[data_range];
                        if let Ok(trimmed) = std::str::from_utf8(data_bytes) {
                            let trimmed = trimmed.trim();
                            if !trimmed.is_empty() && trimmed != "[DONE]" {
                                if let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) {
                                    if let Some(kind) = self.diag.observe(&event) {
                                        let reason = match kind {
                                            TokenCapFailureKind::AnthropicMaxTokens => "max_tokens",
                                        };
                                        tracing::warn!(
                                            request_id = self.log_context.request_id,
                                            upstream_model = %self.log_context.upstream_model,
                                            stop_reason = reason,
                                            "Reasoning-only response reached the per-turn token limit: \
                                             no non-empty text or tool_use block was produced. \
                                             The client may be unable to continue this conversation."
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                let output = self.buffer[..frame_end].to_vec();
                self.buffer.drain(..frame_end);
                return Poll::Ready(Some(Ok(Bytes::from(output))));
            }

            if self.done {
                if !self.buffer.is_empty() {
                    let remaining = Bytes::from(std::mem::take(&mut self.buffer));
                    return Poll::Ready(Some(Ok(remaining)));
                }
                return Poll::Ready(None);
            }

            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    self.done = true;
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    self.done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// SSE frame-level stream wrapper that normalizes `message.model` in `message_start` events.
/// Buffers bytes until a complete SSE frame boundary (`\n\n` or `\r\n\r\n`) is found,
/// then optionally transforms the frame. All non-transformable frames pass through byte-for-byte.
struct SseModelNormalizationStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
    log_context: ModelIdentityLogContext,
    buffer: Vec<u8>,
    done: bool,
    normalized_once: bool,
    outcome_logged: bool,
    terminal_outcome: Option<StreamTerminalOutcome>,
    /// Token-cap diagnostic state (Anthropic SSE only)
    diag: TokenCapDiagnosticState,
    detect_failure: bool,
}

impl SseModelNormalizationStream {
    fn new(
        inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>>,
        log_context: ModelIdentityLogContext,
        detect_failure: bool,
    ) -> Self {
        Self {
            inner,
            log_context,
            buffer: Vec::with_capacity(8192),
            done: false,
            normalized_once: false,
            outcome_logged: false,
            terminal_outcome: None,
            diag: TokenCapDiagnosticState::new(),
            detect_failure,
        }
    }

    /// Log a terminal outcome for an unchanged stream. Idempotent — no-op if
    /// a prior outcome was already logged.
    fn log_unchanged_outcome(&mut self, outcome: StreamTerminalOutcome) {
        if self.normalized_once || self.outcome_logged {
            return;
        }
        let Some(reason) = outcome.as_skip_reason() else {
            return;
        };
        tracing::info!(
            request_id = self.log_context.request_id,
            request_model = %self.log_context.request_model,
            upstream_model = %self.log_context.upstream_model,
            canonical_gateway_model = %self.log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = reason,
            "response model identity"
        );
        self.outcome_logged = true;
        self.terminal_outcome = Some(outcome);
    }

    fn log_token_cap_warning(&self, kind: TokenCapFailureKind) {
        let reason = match kind {
            TokenCapFailureKind::AnthropicMaxTokens => "max_tokens",
        };
        tracing::warn!(
            request_id = self.log_context.request_id,
            upstream_model = %self.log_context.upstream_model,
            stop_reason = reason,
            "Reasoning-only response reached the per-turn token limit: \
             no non-empty text or tool_use block was produced. \
             The client may be unable to continue this conversation."
        );
    }
}

impl Drop for SseModelNormalizationStream {
    fn drop(&mut self) {
        self.log_unchanged_outcome(StreamTerminalOutcome::StreamCancelled);
    }
}

impl Stream for SseModelNormalizationStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            // Try to find a complete frame in the buffer
            if let Some(frame_end) = find_sse_frame_end(&self.buffer) {
                let frame = self.buffer[..frame_end].to_vec();
                self.buffer.drain(..frame_end);

                // Observe for token-cap failure diagnostics (before transform)
                if self.detect_failure && !self.diag.warning_emitted {
                    if let Some(data_range) = find_single_sse_data_value(&frame) {
                        let data_bytes = &frame[data_range];
                        if let Ok(trimmed) = std::str::from_utf8(data_bytes) {
                            let trimmed = trimmed.trim();
                            if !trimmed.is_empty() && trimmed != "[DONE]" {
                                if let Ok(event) = serde_json::from_str::<serde_json::Value>(trimmed) {
                                    if let Some(kind) = self.diag.observe(&event) {
                                        self.log_token_cap_warning(kind);
                                    }
                                }
                            }
                        }
                    }
                }

                // Try to transform; if not applicable, pass through unchanged
                let transformed =
                    transform_complete_sse_frame(&frame, &self.log_context.canonical_gateway_model);

                let output = match transformed {
                    Some(result) => {
                        if !self.normalized_once {
                            tracing::info!(
                                request_id = self.log_context.request_id,
                                request_model = %self.log_context.request_model,
                                upstream_model = %self.log_context.upstream_model,
                                response_model_before = %result.original_model,
                                response_model_after = %self.log_context.canonical_gateway_model,
                                response_model_path = "message_start.message.model",
                                stream = true,
                                normalized = true,
                                "response model identity"
                            );
                            self.normalized_once = true;
                            self.outcome_logged = true;
                            self.terminal_outcome = Some(StreamTerminalOutcome::Normalized);
                        }
                        result.frame
                    }
                    None => frame,
                };
                return Poll::Ready(Some(Ok(Bytes::from(output))));
            }

            // No complete frame yet — try pulling more data from upstream
            if self.done {
                // Flush remaining buffer as an incomplete frame (passthrough)
                if !self.buffer.is_empty() {
                    let remaining = Bytes::from(std::mem::take(&mut self.buffer));
                    return Poll::Ready(Some(Ok(remaining)));
                }
                // EOF: log terminal outcome if nothing was normalized
                self.log_unchanged_outcome(StreamTerminalOutcome::NoModelChangeObserved);
                return Poll::Ready(None);
            }

            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    self.buffer.extend_from_slice(&chunk);
                    // Continue loop to check for complete frames
                }
                Poll::Ready(Some(Err(e))) => {
                    self.done = true;
                    self.log_unchanged_outcome(StreamTerminalOutcome::StreamError);
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    self.done = true;
                    // Continue loop to flush remaining buffer
                }
                Poll::Pending => return Poll::Pending,
            }
        }
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
    block
        .get("type")
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

/// Count remaining image_url and image_base64 blocks after sanitization.
/// Used for per-provider pass-through verification in logs.
fn count_image_types_in_content(content: &Value) -> (usize, usize) {
    let (mut urls, mut b64s) = (0, 0);
    if let Value::Array(arr) = content {
        for item in arr {
            if let Some(source_type) = classify_image_source(item) {
                match source_type {
                    "url" => urls += 1,
                    "base64" => b64s += 1,
                    _ => {}
                }
            }
            if let Some(inner) = item.get("content") {
                let (u, b) = count_image_types_in_content(inner);
                urls += u;
                b64s += b;
            }
        }
    }
    (urls, b64s)
}

fn count_image_types(messages: &[Value]) -> (usize, usize) {
    let (mut urls, mut b64s) = (0, 0);
    for msg in messages {
        if let Some(content) = msg.get("content") {
            let (u, b) = count_image_types_in_content(content);
            urls += u;
            b64s += b;
        }
    }
    (urls, b64s)
}

/// Recursively sanitize unsupported media blocks in place.
/// Returns the count of sanitized blocks.
fn sanitize_content_blocks_granular(
    content: &mut Value,
    policy: &str,
    entry: &ModelRouteEntry,
) -> usize {
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
fn sanitize_body_images(body: &mut Value, entry: &ModelRouteEntry, policy: &str) -> (bool, usize) {
    // If model supports ALL image and video source types, skip entirely
    if entry.supports_image_url
        && entry.supports_image_base64
        && entry.supports_video_url
        && entry.supports_video_base64
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

    // OpenRouter Attribution Headers
    if route.provider_id == "openrouter" {
        headers.insert(
            "HTTP-Referer",
            "https://github.com/soheidon/anthro-bridge".parse().unwrap(),
        );
        headers.insert("X-OpenRouter-Title", "Anthro Bridge".parse().unwrap());
        headers.insert(
            "X-OpenRouter-Categories",
            "cli-agent,programming-app".parse().unwrap(),
        );
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

/// MiniMax-M3専用: thinking_mode_rawからMiniMax API形式のthinkingパラメータを設定する
/// 戻り値: true=M3専用処理を適用した（呼び出し元は汎用matchをスキップ）、false=M3ではない
fn apply_thinking_override_for_minimax_m3(
    entry: &ModelRouteEntry,
    body: &mut serde_json::Value,
) -> bool {
    if entry.provider_id != "minimax" || entry.upstream_model != "MiniMax-M3" {
        return false;
    }
    let Some(obj) = body.as_object_mut() else {
        return true;
    };
    match entry.thinking_mode_raw.as_deref() {
        Some("thinking") | Some("thinking_only") => {
            obj.insert("thinking".to_string(), json!({"type": "adaptive"}));
        }
        Some("normal") => {
            obj.insert("thinking".to_string(), json!({"type": "disabled"}));
        }
        Some("default") | None => {
            obj.remove("thinking");
        }
        Some(unknown) => {
            tracing::warn!("Unknown MiniMax-M3 thinking_mode: {}", unknown);
            obj.remove("thinking");
        }
    }
    true
}

async fn proxy_count_tokens(
    State(config): State<std::sync::Arc<ProxyConfig>>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let mut body: Value = serde_json::from_str(&body).map_err(|e| {
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
    let (was_sanitized, image_count) =
        sanitize_body_images(&mut body, entry, &config.non_vision_image_policy);

    // Check media support (rejects video always; rejects images only when policy == "reject")
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(
            messages,
            entry,
            &route.display_name,
            &config.non_vision_image_policy,
        )?;
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
    if apply_thinking_override_for_minimax_m3(&entry, &mut body) {
        // M3専用処理済み — 汎用matchをスキップ
    } else {
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
        tracing::warn!(
            status = status.as_u16(),
            response_body_bytes = resp_body.len(),
            "POST /v1/messages/count_tokens upstream error"
        );
    } else {
        tracing::info!(
            "POST /v1/messages/count_tokens | status={}",
            status.as_u16()
        );
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
    let mut body: Value = serde_json::from_str(&body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": {"type": "invalid_request_error", "message": e.to_string()}})),
        )
    })?;

    let model_in = body["model"].as_str().unwrap_or("").to_string();
    let (entry, route) = resolve_model(&model_in, &config)?;

    // Sanitize image blocks for non-vision models
    let (was_sanitized, image_count) =
        sanitize_body_images(&mut body, entry, &config.non_vision_image_policy);

    // Check media support (rejects video always; rejects images only when policy == "reject")
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        check_media_support(
            messages,
            entry,
            &route.display_name,
            &config.non_vision_image_policy,
        )?;
    }

    // Log sanitization info (no base64, no conversation text)
    if image_count > 0 {
        let (post_urls, post_b64s) = body
            .get("messages")
            .and_then(|v| v.as_array())
            .map(|msgs| count_image_types(msgs))
            .unwrap_or((0, 0));
        tracing::info!(
            "POST /v1/messages | model: {} -> {} | provider: {} | image_blocks={} (img_url={}, img_b64={} after sanitize) | image_policy={} | sanitized={}",
            model_in, entry.upstream_model, entry.provider_id,
            image_count, post_urls, post_b64s, config.non_vision_image_policy, was_sanitized
        );
    }

    // Apply thinking override + provider-specific reasoning effort injection.
    // Extracted to `apply_route_request_transforms()` for production/test sharing.
    apply_route_request_transforms(&mut body, entry);

    // OpenRouter Poolside S/XS: translate saved thinking_mode + reasoning_effort
    // into OpenRouter's "reasoning" format (NOT Anthropic "thinking" format).
    let uses_poolside_reasoning =
        entry.provider_id == "openrouter" && is_poolside_reasoning_model(&entry.upstream_model);

    if uses_poolside_reasoning {
        if let Some(obj) = body.as_object_mut() {
            match entry.thinking_mode_raw.as_deref() {
                Some("thinking") => {
                    obj.remove("thinking");
                    let reasoning = if entry.reasoning_effort.as_deref() == Some("max") {
                        json!({"effort": "max"})
                    } else {
                        json!({"enabled": true})
                    };
                    obj.insert("reasoning".to_string(), reasoning);
                }
                Some("normal") => {
                    obj.remove("thinking");
                    obj.insert("reasoning".to_string(), json!({"enabled": false}));
                }
                _ => {
                    // Default: no thinking_mode set.
                    // If the client sent thinking: {type: "disabled"}, translate to
                    // Poolside reasoning format so it's actually passed to the server.
                    let is_disabled = obj
                        .get("thinking")
                        .and_then(|t| t.get("type"))
                        .and_then(|t| t.as_str())
                        == Some("disabled");
                    if is_disabled {
                        obj.remove("thinking");
                        obj.insert(
                            "reasoning".to_string(),
                            json!({"enabled": false}),
                        );
                    }
                }
            }
        }
    }

    // OpenRouter Tencent Hy3: same shape as Poolside but `reasoning.effort` accepts
    // low/high/max (Hy3 has no throttling of `max`; we still translate it the same way).
    // Disabled shape is unchanged: {"reasoning": {"enabled": false}}.
    let uses_tencent_reasoning =
        entry.provider_id == "openrouter" && is_tencent_hy3(&entry.upstream_model);

    if uses_tencent_reasoning {
        if let Some(obj) = body.as_object_mut() {
            match entry.thinking_mode_raw.as_deref() {
                Some("thinking") => {
                    obj.remove("thinking");
                    let reasoning = match entry.reasoning_effort.as_deref() {
                        Some("max") => json!({"effort": "max"}),
                        Some("low") => json!({"effort": "low"}),
                        Some("high") => json!({"effort": "high"}),
                        _ => json!({"enabled": true}),
                    };
                    obj.insert("reasoning".to_string(), reasoning);
                }
                Some("normal") => {
                    obj.remove("thinking");
                    obj.insert("reasoning".to_string(), json!({"enabled": false}));
                }
                _ => {
                    // Default: no thinking_mode set.
                    // If the client sent thinking: {type: "disabled"}, translate to
                    // the same reasoning-disabled shape used by Poolside.
                    let is_disabled = obj
                        .get("thinking")
                        .and_then(|t| t.get("type"))
                        .and_then(|t| t.as_str())
                        == Some("disabled");
                    if is_disabled {
                        obj.remove("thinking");
                        obj.insert(
                            "reasoning".to_string(),
                            json!({"enabled": false}),
                        );
                    }
                }
            }
        }
    }

    // OpenRouter InclusionAI: translate saved thinking_mode + reasoning_effort
    // into OpenRouter's "reasoning" format.
    // Ring 2.6 1T: thinking forced (high / xhigh only). Normal → xhigh.
    // Ling 2.6 1T/Flash: NO thinking capability — remove BOTH thinking AND reasoning.
    // Ling 3.0 Flash Free: always normalize → reasoning.enabled = (thinking_mode == "thinking")
    let uses_inclusion_reasoning =
        entry.provider_id == "openrouter" && is_inclusionai_model(&entry.upstream_model);

    if uses_inclusion_reasoning {
        if let Some(obj) = body.as_object_mut() {
            if is_ling_non_thinking_model(&entry.upstream_model) {
                // Ling 2.6 1T / Ling 2.6 Flash: NO thinking capability.
                // Remove BOTH thinking AND reasoning from body.
                obj.remove("thinking");
                obj.remove("reasoning");
            } else if is_ling_free_model(&entry.upstream_model) {
                // Ling 3.0 Flash Free: thinking optional (off/on).
                // Always normalize — never passthrough unknown state.
                obj.remove("thinking");
                obj.remove("reasoning");
                let enabled = matches!(
                    entry.thinking_mode_raw.as_deref(),
                    Some("thinking")
                );
                obj.insert("reasoning".to_string(), json!({"enabled": enabled}));
            } else {
                // Ring 2.6 1T: thinking forced (high / xhigh only).
                // normal / invalid values → normalize to xhigh (model default).
                match entry.thinking_mode_raw.as_deref() {
                    Some("thinking") => {
                        obj.remove("thinking");
                        let reasoning = match entry.reasoning_effort.as_deref() {
                            Some("xhigh") => json!({"effort": "xhigh"}),
                            Some("high") => json!({"effort": "high"}),
                            _ => json!({"effort": "xhigh"}), // normalize invalid → xhigh
                        };
                        obj.insert("reasoning".to_string(), reasoning);
                    }
                    // normal or unset on a forced-thinking model → normalize to xhigh
                    _ => {
                        obj.remove("thinking");
                        obj.insert("reasoning".to_string(), json!({"effort": "xhigh"}));
                    }
                }
            }
        }
    }

    // OpenRouter StepFun: translate saved thinking_mode + reasoning_effort
    // into OpenRouter's "reasoning" format.
    // Step 3.7: low/medium/high effort. normal → medium.
    // Step 3.5: enabled:true always. normal → enabled:true.
    let uses_stepfun_reasoning =
        entry.provider_id == "openrouter" && is_stepfun_model(&entry.upstream_model);

    if uses_stepfun_reasoning {
        if let Some(obj) = body.as_object_mut() {
            let is_step35 = entry.upstream_model == "stepfun/step-3.5-flash";

            match entry.thinking_mode_raw.as_deref() {
                Some("thinking") => {
                    obj.remove("thinking");
                    if is_step35 {
                        // Step 3.5: thinking forced, no effort options
                        obj.insert("reasoning".to_string(), json!({"enabled": true}));
                    } else {
                        // Step 3.7: low / medium / high
                        let reasoning = match entry.reasoning_effort.as_deref() {
                            Some(effort @ ("low" | "medium" | "high")) => {
                                json!({"effort": effort})
                            }
                            _ => json!({"effort": "medium"}), // normalize invalid → medium
                        };
                        obj.insert("reasoning".to_string(), reasoning);
                    }
                }
                // normal or unset on forced-thinking models → normalize to default
                _ => {
                    obj.remove("thinking");
                    if is_step35 {
                        obj.insert("reasoning".to_string(), json!({"enabled": true}));
                    } else {
                        obj.insert("reasoning".to_string(), json!({"effort": "medium"}));
                    }
                }
            }
        }
    }

    // ── OpenAI GPT-5.6 reasoning translation ──────────────────────────────
    // Anthropic "thinking" JSON → OpenRouter "reasoning" JSON.
    //
    // normal → effort: "none" (OpenRouter standard for disabled reasoning)
    // thinking + effort value → effort: that value
    // unset → effort: "medium" (OpenAI default)
    let uses_openai_reasoning =
        entry.provider_id == "openrouter" && is_openai_gpt56_model(&entry.upstream_model);

    if uses_openai_reasoning {
        if let Some(obj) = body.as_object_mut() {
            apply_openai_reasoning(
                obj,
                entry.thinking_mode_raw.as_deref(),
                entry.reasoning_effort.as_deref(),
            );
        }
    }

    // ── Google Gemini via OpenRouter ────────────────────────────────
    // Uses the same OpenRouter reasoning envelope as other OpenRouter vendors.
    // Gemini 3.x is reasoning-mandatory in the UI; saved xhigh/max are
    // normalized to supported Gemini efforts rather than leaking upstream.
    let uses_gemini_reasoning =
        entry.provider_id == "openrouter" && is_gemini_model(&entry.upstream_model);

    if uses_gemini_reasoning {
        if let Some(obj) = body.as_object_mut() {
            apply_gemini_reasoning(
                obj,
                &entry.upstream_model,
                entry.thinking_mode_raw.as_deref(),
                entry.reasoning_effort.as_deref(),
            );
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

    let is_stream = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let upstream_headers = build_upstream_headers(&headers, route);
    let client = build_reqwest_client();
    let upstream_req = client
        .post(format!("{}/v1/messages", route.upstream_url))
        .headers(upstream_headers)
        .json(&body);

    let should_normalize = config
        .normalize_response_model_identity
        .load(std::sync::atomic::Ordering::Relaxed);

    let request_id = REQUEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let log_context = ModelIdentityLogContext {
        request_id,
        request_model: model_in.clone(),
        canonical_gateway_model: entry.gateway_model.clone(),
        upstream_model: entry.upstream_model.clone(),
    };

    tracing::debug!(
        request_id = log_context.request_id,
        request_model = %log_context.request_model,
        canonical_gateway_model = %log_context.canonical_gateway_model,
        upstream_model = %log_context.upstream_model,
        stream = is_stream,
        normalize_enabled = should_normalize,
        "model identity request"
    );

    let detect_failure =
        entry.provider_id == "openrouter" && is_poolside_reasoning_model(&entry.upstream_model);

    if is_stream {
        handle_stream(upstream_req, should_normalize, log_context, detect_failure).await
    } else {
        handle_nonstream(upstream_req, should_normalize, log_context, detect_failure).await
    }
}

/// Pure decision function: should non-stream normalization be attempted?
fn should_normalize_nonstream(
    status_success: bool,
    normalize_enabled: bool,
    encoding_transformable: bool,
) -> bool {
    status_success && normalize_enabled && encoding_transformable
}

/// Pure decision function: which skip reason applies for a non-stream request?
/// Returns `None` when normalization should proceed.
fn nonstream_skip_reason(
    status_success: bool,
    normalize_enabled: bool,
    encoding_transformable: bool,
) -> Option<&'static str> {
    if !status_success {
        Some("non_success_status")
    } else if !normalize_enabled {
        Some("disabled")
    } else if !encoding_transformable {
        Some("content_encoding_not_transformable")
    } else {
        None
    }
}

async fn handle_nonstream(
    req: reqwest::RequestBuilder,
    normalize: bool,
    log_context: ModelIdentityLogContext,
    detect_failure: bool,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let upstream_resp = req.send().await.map_err(|e| {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = false,
            normalized = false,
            skip_reason = "upstream_request_error",
            "response model identity"
        );
        tracing::warn!(
            request_id = log_context.request_id,
            error_kind = "request_send_failed",
            "upstream request failed"
        );
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();
    let resp_body = upstream_resp.bytes().await.map_err(|e| {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = false,
            normalized = false,
            skip_reason = "upstream_body_read_error",
            "response model identity"
        );
        tracing::warn!(
            request_id = log_context.request_id,
            error_kind = "body_read_failed",
            "upstream response body read failed"
        );
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    if !status.is_success() {
        tracing::warn!(
            request_id = log_context.request_id,
            status = status.as_u16(),
            response_body_bytes = resp_body.len(),
            "POST /v1/messages upstream error"
        );
    }

    let encoding_transformable = is_transformable_content_encoding(&resp_headers);
    let status_success = status.is_success();

    if let Some(reason) = nonstream_skip_reason(status_success, normalize, encoding_transformable) {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = false,
            normalized = false,
            skip_reason = reason,
            "response model identity"
        );
    }

    let final_body =
        if should_normalize_nonstream(status_success, normalize, encoding_transformable) {
            match normalize_nonstream_model(&resp_body, &log_context.canonical_gateway_model) {
                NonstreamNormalizationOutcome::Changed {
                    body,
                    original_model,
                } => {
                    tracing::info!(
                        request_id = log_context.request_id,
                        request_model = %log_context.request_model,
                        upstream_model = %log_context.upstream_model,
                        response_model_before = %original_model,
                        response_model_after = %log_context.canonical_gateway_model,
                        response_model_path = "model",
                        stream = false,
                        normalized = true,
                        "response model identity"
                    );
                    Bytes::from(body)
                }
                NonstreamNormalizationOutcome::AlreadyCanonical => {
                    tracing::info!(
                        request_id = log_context.request_id,
                        request_model = %log_context.request_model,
                        upstream_model = %log_context.upstream_model,
                        canonical_gateway_model = %log_context.canonical_gateway_model,
                        stream = false,
                        normalized = false,
                        skip_reason = "already_canonical",
                        "response model identity"
                    );
                    resp_body
                }
                NonstreamNormalizationOutcome::ModelFieldMissing => {
                    tracing::info!(
                        request_id = log_context.request_id,
                        request_model = %log_context.request_model,
                        upstream_model = %log_context.upstream_model,
                        canonical_gateway_model = %log_context.canonical_gateway_model,
                        stream = false,
                        normalized = false,
                        skip_reason = "model_field_missing",
                        "response model identity"
                    );
                    resp_body
                }
                NonstreamNormalizationOutcome::InvalidResponseShape => {
                    tracing::warn!(
                        "POST /v1/messages | request_id={} | response body is not a JSON object",
                        log_context.request_id,
                    );
                    tracing::info!(
                        request_id = log_context.request_id,
                        request_model = %log_context.request_model,
                        upstream_model = %log_context.upstream_model,
                        canonical_gateway_model = %log_context.canonical_gateway_model,
                        stream = false,
                        normalized = false,
                        skip_reason = "invalid_response_shape",
                        "response model identity"
                    );
                    resp_body
                }
            }
        } else {
            resp_body
        };

    // Token-cap failure detection for non-stream responses
    if detect_failure && status_success {
        if let Ok(response_json) = serde_json::from_slice::<serde_json::Value>(&final_body) {
            if let Some(kind) = detect_nonstream_token_cap_failure(&response_json) {
                let reason = match kind {
                    TokenCapFailureKind::AnthropicMaxTokens => "max_tokens",
                };
                tracing::warn!(
                    request_id = log_context.request_id,
                    upstream_model = %log_context.upstream_model,
                    stop_reason = reason,
                    "Reasoning-only response reached the per-turn token limit: \
                     no non-empty text or tool_use block was produced. \
                     The client may be unable to continue this conversation."
                );
            }
        }
    }

    let mut response = Response::new(Body::from(final_body));
    *response.status_mut() = status;
    copy_safe_response_headers(&resp_headers, response.headers_mut());

    if !response.headers().contains_key("content-type") {
        response
            .headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
    }

    Ok(response)
}

async fn handle_stream(
    req: reqwest::RequestBuilder,
    normalize: bool,
    log_context: ModelIdentityLogContext,
    detect_failure: bool,
) -> Result<Response, (StatusCode, Json<Value>)> {
    let upstream_resp = req.send().await.map_err(|e| {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = "upstream_request_error",
            "response model identity"
        );
        tracing::warn!(
            request_id = log_context.request_id,
            error_kind = "request_send_failed",
            "upstream stream request failed"
        );
        (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": {"type": "proxy_error", "message": e.to_string()}})),
        )
    })?;

    if !upstream_resp.status().is_success() {
        let status = upstream_resp.status();
        tracing::warn!(
            request_id = log_context.request_id,
            status = status.as_u16(),
            "POST /v1/messages upstream stream error"
        );
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = "non_success_status",
            "response model identity"
        );
        let body = upstream_resp.text().await.unwrap_or_default();
        let body_excerpt = truncate_chars(&body, 300);
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(json!({
                "error": {
                    "type": "proxy_error",
                    "message": format!("Upstream error {}: {}", status.as_u16(), body_excerpt)
                }
            })),
        ));
    }

    // Capture upstream status and headers BEFORE bytes_stream() consumes the response
    let status = upstream_resp.status();
    let resp_headers = upstream_resp.headers().clone();

    let encoding_transformable = is_transformable_content_encoding(&resp_headers);
    let is_sse = resp_headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|ct| {
            ct.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("text/event-stream")
        })
        .unwrap_or(false);

    // Log skip reasons before entering stream transform
    if !normalize {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = "disabled",
            "response model identity"
        );
    } else if !is_sse {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = "not_sse",
            "response model identity"
        );
    } else if !encoding_transformable {
        tracing::info!(
            request_id = log_context.request_id,
            request_model = %log_context.request_model,
            upstream_model = %log_context.upstream_model,
            canonical_gateway_model = %log_context.canonical_gateway_model,
            stream = true,
            normalized = false,
            skip_reason = "content_encoding_not_transformable",
            "response model identity"
        );
    }

    let stream = upstream_resp
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)));

    let body = if normalize && is_sse && encoding_transformable {
        let boxed: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(stream);
        let normalized = SseModelNormalizationStream::new(boxed, log_context, detect_failure);
        Body::from_stream(normalized)
    } else if detect_failure && is_sse && encoding_transformable {
        // Normalization disabled but token-cap diagnostics still needed
        let boxed: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(stream);
        let diag_stream = SseTokenCapDiagnosticStream::new(boxed, log_context);
        Body::from_stream(diag_stream)
    } else {
        if detect_failure && is_sse && !encoding_transformable {
            tracing::debug!(
                request_id = log_context.request_id,
                upstream_model = %log_context.upstream_model,
                "Skipping token-cap diagnostics for encoded SSE response"
            );
        }
        Body::from_stream(stream)
    };

    let mut response = Response::new(body);
    *response.status_mut() = status;
    copy_safe_response_headers(&resp_headers, response.headers_mut());

    // SSE headers: add defaults only when upstream didn't provide them
    if !response.headers().contains_key("content-type") {
        response
            .headers_mut()
            .insert("content-type", "text/event-stream".parse().unwrap());
    }
    if !response.headers().contains_key("cache-control") {
        response
            .headers_mut()
            .insert("cache-control", "no-cache".parse().unwrap());
    }
    // Not an SSE protocol requirement — preserves existing low-latency proxy behavior
    response
        .headers_mut()
        .insert("x-accel-buffering", "no".parse().unwrap());
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
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Cannot bind to {}: {}", addr, e))?;

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

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Normalize a saved/legacy Gemini effort to the supported OpenRouter values.
pub fn normalize_gemini_reasoning_effort(model: &str, effort: &str) -> &'static str {
    match effort {
        "minimal" if model == "google/gemini-3.5-flash-lite" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" | "max" | "minimal" => "high",
        _ => "high",
    }
}

/// Translate Anthropic thinking params to OpenRouter reasoning JSON for Gemini.
/// Removes the Anthropic `thinking` and `reasoning_effort` keys, inserts `reasoning`.
fn apply_gemini_reasoning(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    upstream_model: &str,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
) {
    obj.remove("thinking");
    obj.remove("reasoning_effort");

    let effort = match thinking_mode {
        Some("normal") => "high",
        Some("thinking") => reasoning_effort
            .map(|e| normalize_gemini_reasoning_effort(upstream_model, e))
            .unwrap_or("high"),
        _ => "high",
    };

    obj.insert("reasoning".to_string(), serde_json::json!({ "effort": effort }));
}
/// Pure function: translate Anthropic thinking params → OpenAI reasoning JSON.
/// Removes the Anthropic `thinking` and `reasoning_effort` keys, inserts `reasoning`.
fn apply_openai_reasoning(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
) {
    obj.remove("thinking");
    obj.remove("reasoning_effort");

    let effort = match thinking_mode {
        Some("normal") => "none",
        Some("thinking") => match reasoning_effort {
            Some(effort @ ("low" | "medium" | "high" | "xhigh" | "max")) => effort,
            _ => "medium",
        },
        _ => "medium",
    };

    obj.insert("reasoning".to_string(), serde_json::json!({ "effort": effort }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModelEntry;
    use futures::TryStreamExt;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    // ── validate_canonical_target tests ───────────────────────────────────────

    fn make_entry(upstream: &str, canonical: Option<&str>) -> ModelEntry {
        ModelEntry {
            upstream_model: upstream.to_string(),
            canonical: canonical.map(|s| s.to_string()),
            thinking: None,
            thinking_mode: None,
            reasoning_effort: None,
            supports_vision: None,
            supports_video: None,
            visible: true,
            force_thinking: None,
            supports_non_thinking: None,
            supports_image_url: Some(true),
            supports_image_base64: Some(true),
            supports_video_url: Some(false),
            supports_video_base64: Some(false),
        }
    }

    fn make_models(entries: Vec<(&str, &str, Option<&str>)>) -> HashMap<String, ModelEntry> {
        entries
            .into_iter()
            .map(|(key, upstream, canon)| (key.to_string(), make_entry(upstream, canon)))
            .collect()
    }

    // ── normalize_deepseek_reasoning_effort tests ───────────────────────────

    #[test]
    fn normalize_deepseek_effort_mapping() {
        // DeepSeek V4-Pro-0813 / V4-Flash-0731:
        // low → low, medium → high, high → high, xhigh → high, max → max
        assert_eq!(normalize_deepseek_reasoning_effort("low"), Some("low"));
        assert_eq!(normalize_deepseek_reasoning_effort("medium"), Some("high"));
        assert_eq!(normalize_deepseek_reasoning_effort("high"), Some("high"));
        assert_eq!(normalize_deepseek_reasoning_effort("xhigh"), Some("high"));
        assert_eq!(normalize_deepseek_reasoning_effort("max"), Some("max"));
        assert_eq!(normalize_deepseek_reasoning_effort("bogus"), None);
        assert_eq!(normalize_deepseek_reasoning_effort(""), None);
    }

    // ── apply_deepseek_reasoning_effort tests ──────────────────────────────

    #[test]
    fn deepseek_v4_pro_thinking_low_sets_output_config_effort_low() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_deepseek_reasoning_effort(&mut body, Some("low"));
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn deepseek_v4_pro_thinking_high_sets_output_config_effort_high() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_deepseek_reasoning_effort(&mut body, Some("high"));
        assert_eq!(body["output_config"]["effort"], "high");
    }

    #[test]
    fn deepseek_v4_pro_thinking_max_sets_output_config_effort_max() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_deepseek_reasoning_effort(&mut body, Some("max"));
        assert_eq!(body["output_config"]["effort"], "max");
    }

    #[test]
    fn deepseek_v4_pro_xhigh_normalized_to_high() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_deepseek_reasoning_effort(&mut body, Some("xhigh"));
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn deepseek_v4_pro_medium_normalized_to_high() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_deepseek_reasoning_effort(&mut body, Some("medium"));
        assert_eq!(body["output_config"]["effort"], "high");
    }

    #[test]
    fn deepseek_thinking_disabled_no_effort_injected() {
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_deepseek_reasoning_effort(&mut body, Some("high"));
        assert_eq!(body.get("output_config"), None);
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn deepseek_none_effort_cleans_stale_values() {
        // Stale flat reasoning_effort + stale output_config.effort both removed
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 4000},
            "reasoning_effort": "old_value",
            "output_config": {"effort": "old_effort", "other_key": "keep"}
        });
        apply_deepseek_reasoning_effort(&mut body, None);
        assert_eq!(body.get("reasoning_effort"), None);
        assert_eq!(body["output_config"]["other_key"], "keep");
        assert_eq!(body["output_config"].get("effort"), None);
    }

    #[test]
    fn deepseek_unknown_effort_no_effort_injected_stale_cleaned() {
        let mut body = json!({
            "thinking": {"type": "enabled", "budget_tokens": 4000},
            "reasoning_effort": "stale",
            "output_config": {"effort": "stale"}
        });
        apply_deepseek_reasoning_effort(&mut body, Some("unknown_effort"));
        assert_eq!(body.get("reasoning_effort"), None);
        assert_eq!(body["output_config"].get("effort"), None);
    }

    #[test]
    fn deepseek_empty_output_config_removed_after_cleanup() {
        let mut body = json!({
            "thinking": {"type": "disabled"},
            "output_config": {"effort": "high"}
        });
        apply_deepseek_reasoning_effort(&mut body, None);
        assert_eq!(body.get("output_config"), None);
    }

    #[test]
    fn minimax_thinking_enabled_sets_flat_reasoning_effort() {
        // MiniMax (non-DeepSeek, non-OpenRouter) keeps the flat key
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        // Simulate the MiniMax/Kimi/MiMo branch: flat reasoning_effort
        if body.get("thinking").and_then(|v| v.get("type")).and_then(|t| t.as_str()) == Some("enabled") {
            body["reasoning_effort"] = json!("high");
        }
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body.get("output_config"), None);
    }

    // ── Production-path payload tests ─────────────────────────────────────
    //
    // These call `apply_route_request_transforms()` — the same production
    // function invoked by proxy_messages(). Modifying the DeepSeek or
    // direct-provider branch in production will cause these tests to fail.

    fn make_route_entry(
        provider_id: &str,
        upstream_model: &str,
        thinking: ThinkingOverride,
        reasoning_effort: Option<&str>,
    ) -> ModelRouteEntry {
        ModelRouteEntry {
            gateway_model: "claude-sonnet-5".to_string(),
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            thinking,
            force_thinking: false,
            reasoning_effort: reasoning_effort.map(|s| s.to_string()),
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
            thinking_mode_raw: None,
        }
    }

    #[test]
    fn production_path_deepseek_v4_pro_thinking_max() {
        let entry = make_route_entry("deepseek", "deepseek-v4-pro", ThinkingOverride::Enabled, Some("max"));
        let mut body = json!({"model": "claude-opus-5", "messages": []});
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["output_config"]["effort"], "max");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn production_path_deepseek_v4_pro_thinking_low() {
        let entry = make_route_entry("deepseek", "deepseek-v4-pro", ThinkingOverride::Enabled, Some("low"));
        let mut body = json!({"model": "claude-opus-5", "messages": []});
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn production_path_deepseek_v4_pro_normal_no_effort() {
        let entry = make_route_entry("deepseek", "deepseek-v4-pro", ThinkingOverride::Disabled, Some("max"));
        let mut body = json!({"model": "claude-opus-5", "messages": []});
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body.get("output_config"), None);
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn production_path_deepseek_v4_pro_xhigh_normalized_to_high() {
        let entry = make_route_entry("deepseek", "deepseek-v4-pro", ThinkingOverride::Enabled, Some("xhigh"));
        let mut body = json!({"model": "claude-opus-5", "messages": []});
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["output_config"]["effort"], "high");
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn production_path_deepseek_stale_values_cleaned_in_normal() {
        // Simulate stale values from upstream that must be cleaned
        let entry = make_route_entry("deepseek", "deepseek-v4-pro", ThinkingOverride::Disabled, None);
        let mut body = json!({
            "model": "claude-opus-5",
            "messages": [],
            "reasoning_effort": "stale_old",
            "output_config": {"effort": "stale_old"}
        });
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["thinking"]["type"], "disabled");
        assert_eq!(body.get("reasoning_effort"), None);
        assert_eq!(body.get("output_config"), None);
    }

    #[test]
    fn production_path_minimax_thinking_high_flat_effort_no_output_config() {
        // MiniMax direct provider (not M3, which has its own thinking override).
        // Must use flat reasoning_effort, not output_config.effort.
        let entry = make_route_entry("minimax", "MiniMax-M2.5", ThinkingOverride::Enabled, Some("high"));
        let mut body = json!({"model": "claude-opus-5", "messages": []});
        apply_route_request_transforms(&mut body, &entry);
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body.get("output_config"), None);
    }

    // ── validate_canonical_target tests ───────────────────────────────────────

    #[test]
    fn canonical_no_field_returns_self() {
        let models = make_models(vec![("claude-opus-5", "deepseek-v4-pro", None)]);
        let entry = models.get("claude-opus-5").unwrap();
        assert_eq!(
            validate_canonical_target("claude-opus-5", entry, &models).unwrap(),
            "claude-opus-5"
        );
    }

    #[test]
    fn canonical_valid_alias() {
        let models = make_models(vec![
            ("claude-opus-5", "deepseek-v4-pro", None),
            ("claude-opus", "deepseek-v4-pro", Some("claude-opus-5")),
        ]);
        let entry = models.get("claude-opus").unwrap();
        assert_eq!(
            validate_canonical_target("claude-opus", entry, &models).unwrap(),
            "claude-opus-5"
        );
    }

    #[test]
    fn canonical_self_reference_rejected() {
        let models = make_models(vec![(
            "claude-opus",
            "deepseek-v4-pro",
            Some("claude-opus"),
        )]);
        assert!(validate_canonical_target(
            "claude-opus",
            models.get("claude-opus").unwrap(),
            &models
        )
        .unwrap_err()
        .contains("references itself"));
    }

    #[test]
    fn canonical_alias_chain_rejected() {
        let models = make_models(vec![
            ("claude-opus-5", "deepseek-v4-pro", None),
            ("claude-opus", "deepseek-v4-pro", Some("claude-opus-5")),
            ("claude-opus-alias", "deepseek-v4-pro", Some("claude-opus")),
        ]);
        assert!(validate_canonical_target(
            "claude-opus-alias",
            models.get("claude-opus-alias").unwrap(),
            &models
        )
        .unwrap_err()
        .contains("references another alias"));
    }

    #[test]
    fn canonical_missing_target_rejected() {
        let models = make_models(vec![(
            "claude-opus",
            "deepseek-v4-pro",
            Some("nonexistent"),
        )]);
        assert!(validate_canonical_target(
            "claude-opus",
            models.get("claude-opus").unwrap(),
            &models
        )
        .unwrap_err()
        .contains("references missing canonical"));
    }

    // ── normalize_nonstream_model tests ───────────────────────────────────────

    #[test]
    fn nonstream_normalizes_model_field() {
        let body = r#"{"id":"msg_123","model":"deepseek-v4-pro","content":"hi"}"#;
        match normalize_nonstream_model(body.as_bytes(), "claude-opus-5") {
            NonstreamNormalizationOutcome::Changed {
                body,
                original_model,
            } => {
                let parsed: Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(parsed["model"].as_str().unwrap(), "claude-opus-5");
                assert_eq!(parsed["id"].as_str().unwrap(), "msg_123");
                assert_eq!(original_model, "deepseek-v4-pro");
            }
            other => panic!("Expected Changed, got {:?}", other),
        }
    }

    #[test]
    fn nonstream_already_correct_returns_already_canonical() {
        assert!(matches!(
            normalize_nonstream_model(br#"{"model":"claude-opus-5"}"#, "claude-opus-5"),
            NonstreamNormalizationOutcome::AlreadyCanonical
        ));
    }

    #[test]
    fn nonstream_invalid_json_returns_invalid_response_shape() {
        assert!(matches!(
            normalize_nonstream_model(b"not json", "claude-opus-5"),
            NonstreamNormalizationOutcome::InvalidResponseShape
        ));
    }

    #[test]
    fn nonstream_missing_model_returns_model_field_missing() {
        assert!(matches!(
            normalize_nonstream_model(br#"{"id":"msg_123"}"#, "claude-opus-5"),
            NonstreamNormalizationOutcome::ModelFieldMissing
        ));
    }

    // ── find_single_sse_data_value tests ──────────────────────────────────────

    #[test]
    fn data_value_single_lf() {
        let frame = b"event: message_start\ndata: {\"model\":\"test\"}\n\n";
        let range = find_single_sse_data_value(frame).unwrap();
        assert_eq!(&frame[range], b"{\"model\":\"test\"}");
    }

    #[test]
    fn data_value_single_crlf() {
        let frame = b"event: message_start\r\ndata: {\"model\":\"test\"}\r\n\r\n";
        let range = find_single_sse_data_value(frame).unwrap();
        assert_eq!(&frame[range], b"{\"model\":\"test\"}");
    }

    #[test]
    fn data_value_space_after_colon() {
        let frame = b"data: {\"model\":\"test\"}\n\n";
        let range = find_single_sse_data_value(frame).unwrap();
        assert_eq!(&frame[range], b"{\"model\":\"test\"}");
    }

    #[test]
    fn data_value_no_space_after_colon() {
        let frame = b"data:{\"model\":\"test\"}\n\n";
        let range = find_single_sse_data_value(frame).unwrap();
        assert_eq!(&frame[range], b"{\"model\":\"test\"}");
    }

    #[test]
    fn data_value_multiple_data_passthrough() {
        assert!(find_single_sse_data_value(b"data: line1\ndata: line2\n\n").is_none());
    }

    #[test]
    fn data_value_no_data_line() {
        assert!(find_single_sse_data_value(b"event: ping\n\n").is_none());
    }

    // ── find_sse_frame_end tests ──────────────────────────────────────────────

    #[test]
    fn sse_frame_end_lf() {
        let buf = b"data: test\n\ndata: next\n\n";
        assert_eq!(find_sse_frame_end(buf), Some(b"data: test\n\n".len()));
    }

    #[test]
    fn sse_frame_end_crlf() {
        let buf = b"data: test\r\n\r\ndata: next\r\n\r\n";
        assert_eq!(find_sse_frame_end(buf), Some(b"data: test\r\n\r\n".len()));
    }

    #[test]
    fn sse_frame_end_lf_before_crlf() {
        let buf = b"data: test\n\ndata: test\r\n\r\n";
        assert_eq!(find_sse_frame_end(buf), Some(b"data: test\n\n".len()));
    }

    #[test]
    fn sse_frame_end_boundary_across_chunks() {
        assert!(find_sse_frame_end(b"data: test\n").is_none());
    }

    #[test]
    fn sse_boundary_not_double_consumed() {
        let mut buf = b"data: test\n\ndata: next\n\n".to_vec();
        let end1 = find_sse_frame_end(&buf).unwrap();
        buf.drain(..end1);
        let end2 = find_sse_frame_end(&buf).unwrap();
        assert_eq!(&buf[..end2], b"data: next\n\n");
    }

    // ── transform_complete_sse_frame tests ────────────────────────────────────

    #[test]
    fn sse_transform_lf_preserves_lf() {
        let frame = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let result = transform_complete_sse_frame(frame, "gateway-id").unwrap();
        assert!(!result.frame.windows(2).any(|w| w == b"\r\n"));
        assert!(std::str::from_utf8(&result.frame)
            .unwrap()
            .contains("\"gateway-id\""));
        assert_eq!(result.original_model, "upstream");
    }

    #[test]
    fn sse_transform_crlf_preserves_crlf() {
        let frame = b"event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\r\n\r\n";
        let result = transform_complete_sse_frame(frame, "gateway-id").unwrap();
        assert!(result.frame.windows(2).any(|w| w == b"\r\n"));
        assert_eq!(result.original_model, "upstream");
    }

    #[test]
    fn sse_transform_preserves_id_retry_comment() {
        let frame = b"event: message_start\nid: evt_1\nretry: 3000\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n: this is a comment\n\n";
        let result = transform_complete_sse_frame(frame, "gateway-id").unwrap();
        let s = std::str::from_utf8(&result.frame).unwrap();
        assert!(s.contains("id: evt_1"));
        assert!(s.contains("retry: 3000"));
        assert!(s.contains(": this is a comment"));
        assert!(s.contains("event: message_start"));
    }

    #[test]
    fn sse_transform_multiple_data_passthrough() {
        assert!(transform_complete_sse_frame(b"data: line1\ndata: line2\n\n", "gw").is_none());
    }

    #[test]
    fn sse_transform_data_done_passthrough() {
        assert!(transform_complete_sse_frame(b"data: [DONE]\n\n", "gw").is_none());
    }

    #[test]
    fn sse_transform_malformed_json_passthrough() {
        assert!(transform_complete_sse_frame(b"data: not-json\n\n", "gw").is_none());
    }

    // ── copy_safe_response_headers tests ──────────────────────────────────────

    #[test]
    fn headers_excludes_content_length() {
        let mut src = HeaderMap::new();
        src.insert("content-length", "1234".parse().unwrap());
        src.insert("x-request-id", "req_1".parse().unwrap());
        let mut dst = HeaderMap::new();
        copy_safe_response_headers(&src, &mut dst);
        assert!(!dst.contains_key("content-length"));
        assert_eq!(dst.get("x-request-id").unwrap(), "req_1");
    }

    #[test]
    fn headers_excludes_transfer_encoding() {
        let mut src = HeaderMap::new();
        src.insert("transfer-encoding", "chunked".parse().unwrap());
        let mut dst = HeaderMap::new();
        copy_safe_response_headers(&src, &mut dst);
        assert!(!dst.contains_key("transfer-encoding"));
    }

    #[test]
    fn headers_excludes_hop_by_hop() {
        let mut src = HeaderMap::new();
        src.insert("connection", "keep-alive".parse().unwrap());
        src.insert("upgrade", "h2c".parse().unwrap());
        let mut dst = HeaderMap::new();
        copy_safe_response_headers(&src, &mut dst);
        assert!(!dst.contains_key("connection"));
        assert!(!dst.contains_key("upgrade"));
    }

    #[test]
    fn headers_preserves_content_encoding_gzip() {
        let mut src = HeaderMap::new();
        src.insert("content-encoding", "gzip".parse().unwrap());
        let mut dst = HeaderMap::new();
        copy_safe_response_headers(&src, &mut dst);
        assert_eq!(dst.get("content-encoding").unwrap(), "gzip");
    }

    #[test]
    fn headers_preserves_content_encoding_br() {
        let mut src = HeaderMap::new();
        src.insert("content-encoding", "br".parse().unwrap());
        let mut dst = HeaderMap::new();
        copy_safe_response_headers(&src, &mut dst);
        assert_eq!(dst.get("content-encoding").unwrap(), "br");
    }

    // ── is_transformable_content_encoding tests ───────────────────────────────

    #[test]
    fn encoding_absent_is_transformable() {
        let headers = HeaderMap::new();
        assert!(is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_identity_is_transformable() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "identity".parse().unwrap());
        assert!(is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_identity_is_case_insensitive_and_trimmed() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", " Identity ".parse().unwrap());
        assert!(is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_gzip_is_not_transformable() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "gzip".parse().unwrap());
        assert!(!is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_br_is_not_transformable() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "br".parse().unwrap());
        assert!(!is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_deflate_is_not_transformable() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "deflate".parse().unwrap());
        assert!(!is_transformable_content_encoding(&headers));
    }

    #[test]
    fn encoding_zstd_is_not_transformable() {
        let mut headers = HeaderMap::new();
        headers.insert("content-encoding", "zstd".parse().unwrap());
        assert!(!is_transformable_content_encoding(&headers));
    }

    // ── error/edge case tests ─────────────────────────────────────────────────

    #[test]
    fn error_message_not_normalized() {
        assert!(transform_complete_sse_frame(
            b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded\"}}\n\n",
            "gateway-id"
        )
        .is_none());
    }

    // ── SseModelNormalizationStream integration tests ─────────────────────────

    fn make_log_context(
        request_model: &str,
        canonical: &str,
        upstream: &str,
    ) -> ModelIdentityLogContext {
        ModelIdentityLogContext {
            request_id: 999,
            request_model: request_model.to_string(),
            canonical_gateway_model: canonical.to_string(),
            upstream_model: upstream.to_string(),
        }
    }

    #[tokio::test]
    async fn stream_handles_frame_split_across_chunks() {
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from_static(b"data: {\"type\":\"message_start\",")),
            Ok(Bytes::from_static(
                b"\"message\":{\"model\":\"upstream\"}}\n\n",
            )),
        ];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap().concat();
        let text = std::str::from_utf8(&output).unwrap();
        assert!(text.contains("\"model\":\"gateway-id\""));
    }

    #[tokio::test]
    async fn stream_handles_multiple_frames_in_one_chunk() {
        let frame1 = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let frame2 = b"event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n";
        let mut combined = Vec::new();
        combined.extend_from_slice(frame1);
        combined.extend_from_slice(frame2);

        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(combined))];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap();

        // Should produce two separate frames
        assert_eq!(output.len(), 2);
        let first = std::str::from_utf8(&output[0]).unwrap();
        assert!(first.contains("\"model\":\"gateway-id\""));
        let second = std::str::from_utf8(&output[1]).unwrap();
        assert!(second.contains("\"type\":\"message_delta\""));
        // Second frame must pass through byte-for-byte (not message_start)
        assert_eq!(output[1].as_ref(), frame2);
    }

    #[tokio::test]
    async fn stream_flushes_incomplete_frame_on_eof() {
        // A frame without trailing \n\n — should be flushed as-is on stream end
        let chunks: Vec<Result<Bytes, std::io::Error>> =
            vec![Ok(Bytes::from_static(b"data: incomplete\n"))];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_ref(), b"data: incomplete\n");
    }

    #[tokio::test]
    async fn stream_multiple_data_lines_passthrough_unchanged() {
        let frame = b"data: line1\ndata: line2\n\n";
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from_static(frame))];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_ref(), frame);
    }

    #[tokio::test]
    async fn stream_non_message_start_unchanged() {
        let frame = b"event: ping\ndata: {\"type\":\"ping\"}\n\n";
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from_static(frame))];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_ref(), frame);
    }

    #[tokio::test]
    async fn stream_already_correct_model_unchanged() {
        let frame =
            b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"gateway-id\"}}\n\n";
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from_static(frame))];
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(chunks));
        let ctx = make_log_context("claude-opus-5", "gateway-id", "upstream");
        let stream = SseModelNormalizationStream::new(inner, ctx, false);
        let output = stream.try_collect::<Vec<Bytes>>().await.unwrap();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].as_ref(), frame);
    }

    // ── NonstreamNormalizationOutcome original_model test ──────────────────────

    #[test]
    fn nonstream_changed_captures_original_model() {
        let body = r#"{"model":"deepseek-v4-pro","content":"hi"}"#;
        match normalize_nonstream_model(body.as_bytes(), "claude-opus-5") {
            NonstreamNormalizationOutcome::Changed {
                body: _,
                original_model,
            } => {
                assert_eq!(original_model, "deepseek-v4-pro");
            }
            other => panic!("Expected Changed, got {:?}", other),
        }
    }

    // ── SseNormalizationResult original_model test ─────────────────────────────

    #[test]
    fn sse_transform_captures_original_model() {
        let frame = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream-model\"}}\n\n";
        let result = transform_complete_sse_frame(frame, "gateway-id").unwrap();
        assert_eq!(result.original_model, "upstream-model");
    }

    // ── Exact output regression tests ──────────────────────────────────────────

    #[test]
    fn sse_transform_exact_lf_output() {
        let input = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        let output = std::str::from_utf8(&result.frame).unwrap();
        assert!(!output.contains("data: data:"));
        assert!(output.starts_with("event: message_start\ndata: "));
        assert!(output.ends_with("\n\n"));
        assert_eq!(
            output
                .lines()
                .filter(|line| line.starts_with("data:"))
                .count(),
            1
        );
    }

    #[test]
    fn sse_transform_exact_crlf_output() {
        let input = b"event: message_start\r\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\r\n\r\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        let output = std::str::from_utf8(&result.frame).unwrap();
        assert!(!output.contains("data: data:"));
        assert!(output.starts_with("event: message_start\r\ndata: "));
        assert!(output.ends_with("\r\n\r\n"));
        assert!(!output.contains("\r\n\r\n\r\n"));
    }

    #[test]
    fn sse_transform_preserves_prefix_and_suffix() {
        let input = b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        let value_range = find_single_sse_data_value(input).unwrap();
        assert_eq!(
            &result.frame[..value_range.start],
            &input[..value_range.start]
        );
        assert!(result.frame.ends_with(&input[value_range.end..]));
    }

    #[test]
    fn sse_non_message_start_with_model_is_unchanged() {
        let frame = b"data: {\"type\":\"other\",\"message\":{\"model\":\"upstream\"}}\n\n";
        assert!(transform_complete_sse_frame(frame, "gateway-id").is_none());
    }

    // ── data: prefix whitespace preservation tests ─────────────────────────────

    #[test]
    fn sse_transform_preserves_data_prefix_without_space() {
        let input = b"data:{\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        let output = std::str::from_utf8(&result.frame).unwrap();
        assert!(output.starts_with("data:{"));
        assert!(!output.starts_with("data: {"));
        assert!(!output.contains("data:data:"));
    }

    #[test]
    fn sse_transform_preserves_data_prefix_with_space() {
        let input = b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        let output = std::str::from_utf8(&result.frame).unwrap();
        assert!(output.starts_with("data: {"));
        assert!(!output.contains("data: data:"));
    }

    #[test]
    fn sse_transform_preserves_trailing_value_whitespace() {
        let input =
            b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}   \n\n";
        let result = transform_complete_sse_frame(input, "gateway-id").unwrap();
        assert!(result.frame.ends_with(b"   \n\n"));
    }

    // ── StreamTerminalOutcome skip_reason mapping ──────────────────────────────

    #[test]
    fn stream_terminal_outcome_skip_reason_mapping() {
        assert_eq!(
            StreamTerminalOutcome::NoModelChangeObserved.as_skip_reason(),
            Some("no_model_change_observed")
        );
        assert_eq!(
            StreamTerminalOutcome::StreamError.as_skip_reason(),
            Some("stream_error")
        );
        assert_eq!(
            StreamTerminalOutcome::StreamCancelled.as_skip_reason(),
            Some("stream_cancelled")
        );
        assert_eq!(StreamTerminalOutcome::Normalized.as_skip_reason(), None);
    }

    // ── Stream state-transition tests ──────────────────────────────────────────

    fn empty_byte_stream() -> Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> {
        Box::pin(futures::stream::empty())
    }

    #[test]
    fn stream_outcome_logged_prevents_duplicate() {
        let mut stream = SseModelNormalizationStream {
            inner: empty_byte_stream(),
            log_context: make_log_context("req", "canon", "up"),
            buffer: vec![],
            done: true,
            normalized_once: false,
            outcome_logged: false,
            terminal_outcome: None,
            detect_failure: false,
            diag: TokenCapDiagnosticState::new(),
        };
        stream.log_unchanged_outcome(StreamTerminalOutcome::NoModelChangeObserved);
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::NoModelChangeObserved)
        );
        stream.log_unchanged_outcome(StreamTerminalOutcome::StreamError);
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::NoModelChangeObserved)
        );
    }

    #[test]
    fn stream_normalized_once_skips_outcome() {
        let mut stream = SseModelNormalizationStream {
            inner: empty_byte_stream(),
            log_context: make_log_context("req", "canon", "up"),
            buffer: vec![],
            done: true,
            normalized_once: true,
            outcome_logged: false,
            terminal_outcome: None,
            detect_failure: false,
            diag: TokenCapDiagnosticState::new(),
        };
        stream.log_unchanged_outcome(StreamTerminalOutcome::NoModelChangeObserved);
        assert_eq!(stream.terminal_outcome, None);
    }

    #[test]
    fn stream_cancelled_sets_terminal_outcome() {
        let mut stream = SseModelNormalizationStream {
            inner: empty_byte_stream(),
            log_context: make_log_context("req", "canon", "up"),
            buffer: vec![],
            done: false,
            normalized_once: false,
            outcome_logged: false,
            terminal_outcome: None,
            detect_failure: false,
            diag: TokenCapDiagnosticState::new(),
        };
        stream.log_unchanged_outcome(StreamTerminalOutcome::StreamCancelled);
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::StreamCancelled)
        );
        assert!(stream.outcome_logged);
    }

    #[tokio::test]
    async fn stream_error_marks_terminal_outcome() {
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(vec![Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "upstream failed",
            ))]));
        let mut stream =
            SseModelNormalizationStream::new(inner, make_log_context("req", "canon", "up"), false);
        let result = stream.next().await;
        assert!(result.unwrap().is_err());
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::StreamError)
        );
        assert!(!stream.normalized_once);
    }

    #[tokio::test]
    async fn stream_normalized_then_eof_has_normalized_outcome() {
        let frame = b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(vec![Ok(Bytes::from_static(frame))]));
        let mut stream =
            SseModelNormalizationStream::new(inner, make_log_context("req", "canon", "up"), false);
        while stream.next().await.is_some() {}
        assert!(stream.normalized_once);
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::Normalized)
        );
    }

    #[tokio::test]
    async fn normalized_outcome_is_not_overwritten() {
        let frame = b"data: {\"type\":\"message_start\",\"message\":{\"model\":\"upstream\"}}\n\n";
        let inner: Pin<Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send>> =
            Box::pin(futures::stream::iter(vec![Ok(Bytes::from_static(frame))]));
        let mut stream =
            SseModelNormalizationStream::new(inner, make_log_context("req", "canon", "up"), false);
        while stream.next().await.is_some() {}
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::Normalized)
        );
        stream.log_unchanged_outcome(StreamTerminalOutcome::StreamError);
        assert_eq!(
            stream.terminal_outcome,
            Some(StreamTerminalOutcome::Normalized)
        );
    }

    // ── truncate_chars tests ───────────────────────────────────────────────────

    #[test]
    fn truncate_chars_short_string_unchanged() {
        assert_eq!(truncate_chars("hello", 10), "hello");
    }

    #[test]
    fn truncate_chars_exact_boundary() {
        assert_eq!(truncate_chars("hello", 5), "hello");
    }

    #[test]
    fn truncate_chars_multibyte_safe() {
        assert_eq!(truncate_chars("日本語テスト", 3), "日本語");
    }

    #[test]
    fn truncate_chars_empty() {
        assert_eq!(truncate_chars("", 5), "");
    }

    // ── should_normalize_nonstream / nonstream_skip_reason tests ─────────────

    #[test]
    fn nonstream_does_not_normalize_nontransformable_encoding() {
        assert!(!should_normalize_nonstream(true, true, false));
    }

    #[test]
    fn nonstream_normalizes_when_all_conditions_met() {
        assert!(should_normalize_nonstream(true, true, true));
    }

    #[test]
    fn nonstream_does_not_normalize_on_failure() {
        assert!(!should_normalize_nonstream(false, true, true));
    }

    #[test]
    fn nonstream_does_not_normalize_when_disabled() {
        assert!(!should_normalize_nonstream(true, false, true));
    }

    #[test]
    fn nonstream_skip_reason_priority() {
        // non_success_status wins over everything
        assert_eq!(
            nonstream_skip_reason(false, false, false),
            Some("non_success_status")
        );
        assert_eq!(
            nonstream_skip_reason(false, true, true),
            Some("non_success_status")
        );
        // disabled wins over encoding
        assert_eq!(nonstream_skip_reason(true, false, false), Some("disabled"));
        assert_eq!(nonstream_skip_reason(true, false, true), Some("disabled"));
        // content_encoding_not_transformable
        assert_eq!(
            nonstream_skip_reason(true, true, false),
            Some("content_encoding_not_transformable")
        );
        // all conditions met → no skip
        assert_eq!(nonstream_skip_reason(true, true, true), None);
    }

    // ── TokenCapDiagnosticState tests (SSE observer) ──────────────────────────

    fn make_event(ty: &str, inner: serde_json::Value) -> serde_json::Value {
        json!({"type": ty, "content_block": inner})
    }

    fn make_delta_event(inner: serde_json::Value) -> serde_json::Value {
        json!({"type": "content_block_delta", "delta": inner})
    }

    fn make_message_delta(stop_reason: &str) -> serde_json::Value {
        json!({"type": "message_delta", "delta": {"stop_reason": stop_reason}})
    }

    #[test]
    fn diag_thinking_only_max_tokens_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(
            s.observe(&json!({"type": "message_stop"})),
            Some(TokenCapFailureKind::AnthropicMaxTokens)
        );
    }

    #[test]
    fn diag_thinking_plus_nonempty_text_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_delta_event(json!({"type": "text_delta", "text": "hello"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_thinking_plus_tool_use_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_event("content_block_start", json!({"type": "tool_use", "name": "read"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_thinking_only_end_turn_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_message_delta("end_turn"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_no_thinking_max_tokens_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "text", "text": "hello"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_thinking_no_message_stop_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_message_delta("max_tokens"));
        // No message_stop → finalize not called; observer returns None from non-message-stop event
        assert_eq!(s.finalize(), None);
    }

    #[test]
    fn diag_thinking_empty_text_start_plus_nonempty_delta_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_event("content_block_start", json!({"type": "text", "text": ""})));
        s.observe(&make_delta_event(json!({"type": "text_delta", "text": "hello"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_thinking_empty_text_start_empty_delta_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_event("content_block_start", json!({"type": "text", "text": ""})));
        s.observe(&make_delta_event(json!({"type": "text_delta", "text": "  "})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(
            s.observe(&json!({"type": "message_stop"})),
            Some(TokenCapFailureKind::AnthropicMaxTokens)
        );
    }

    #[test]
    fn diag_redacted_thinking_only_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "redacted_thinking"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(
            s.observe(&json!({"type": "message_stop"})),
            Some(TokenCapFailureKind::AnthropicMaxTokens)
        );
    }

    #[test]
    fn diag_thinking_tool_use_start_plus_max_tokens_not_detected() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_event("content_block_start", json!({"type": "tool_use", "name": "read"})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    #[test]
    fn diag_warning_emitted_only_once() {
        let mut s = TokenCapDiagnosticState::new();
        s.observe(&make_event("content_block_start", json!({"type": "thinking", "thinking": "..."})));
        s.observe(&make_message_delta("max_tokens"));
        assert_eq!(
            s.observe(&json!({"type": "message_stop"})),
            Some(TokenCapFailureKind::AnthropicMaxTokens)
        );
        // Second message_stop: already warned, returns None
        assert_eq!(s.observe(&json!({"type": "message_stop"})), None);
    }

    // ── detect_nonstream_token_cap_failure tests ──────────────────────────────

    #[test]
    fn nonstream_thinking_only_max_tokens_detected() {
        let body = json!({
            "stop_reason": "max_tokens",
            "content": [{"type": "thinking", "thinking": "..."}]
        });
        assert_eq!(
            detect_nonstream_token_cap_failure(&body),
            Some(TokenCapFailureKind::AnthropicMaxTokens)
        );
    }

    #[test]
    fn nonstream_thinking_plus_text_not_detected() {
        let body = json!({
            "stop_reason": "max_tokens",
            "content": [
                {"type": "thinking", "thinking": "..."},
                {"type": "text", "text": "result"}
            ]
        });
        assert_eq!(detect_nonstream_token_cap_failure(&body), None);
    }

    #[test]
    fn nonstream_thinking_end_turn_not_detected() {
        let body = json!({
            "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": "..."}]
        });
        assert_eq!(detect_nonstream_token_cap_failure(&body), None);
    }

    #[test]
    fn nonstream_text_only_max_tokens_not_detected() {
        let body = json!({
            "stop_reason": "max_tokens",
            "content": [{"type": "text", "text": "hello"}]
        });
        assert_eq!(detect_nonstream_token_cap_failure(&body), None);
    }

    #[test]
    fn nonstream_thinking_plus_tool_use_not_detected() {
        let body = json!({
            "stop_reason": "max_tokens",
            "content": [
                {"type": "thinking", "thinking": "..."},
                {"type": "tool_use", "name": "read"}
            ]
        });
        assert_eq!(detect_nonstream_token_cap_failure(&body), None);
    }

    #[test]
    fn nonstream_invalid_json_graceful() {
        let body = json!("not an object");
        assert_eq!(detect_nonstream_token_cap_failure(&body), None);
    }

    // ── Poolside thinking:disabled tests ──────────────────────────────────────

    fn make_route(
        provider_id: &str,
        upstream_model: &str,
        thinking_mode_raw: Option<&str>,
    ) -> ModelRouteEntry {
        make_route_with_effort(provider_id, upstream_model, thinking_mode_raw, None)
    }

    fn make_route_with_effort(
        provider_id: &str,
        upstream_model: &str,
        thinking_mode_raw: Option<&str>,
        reasoning_effort: Option<&str>,
    ) -> ModelRouteEntry {
        ModelRouteEntry {
            gateway_model: "claude-opus-5".to_string(),
            provider_id: provider_id.to_string(),
            upstream_model: upstream_model.to_string(),
            thinking: ThinkingOverride::Default,
            force_thinking: false,
            thinking_mode_raw: thinking_mode_raw.map(|s| s.to_string()),
            reasoning_effort: reasoning_effort.map(|s| s.to_string()),
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }
    }

    fn apply_tencent_reasoning(
        route: &ModelRouteEntry,
        body: &mut serde_json::Value,
    ) {
        let uses_tencent =
            route.provider_id == "openrouter" && is_tencent_hy3(&route.upstream_model);
        if !uses_tencent {
            return;
        }
        let obj = match body.as_object_mut() {
            Some(o) => o,
            None => return,
        };
        match route.thinking_mode_raw.as_deref() {
            Some("thinking") => {
                obj.remove("thinking");
                let reasoning = match route.reasoning_effort.as_deref() {
                    Some("max") => json!({"effort": "max"}),
                    Some("low") => json!({"effort": "low"}),
                    Some("high") => json!({"effort": "high"}),
                    _ => json!({"enabled": true}),
                };
                obj.insert("reasoning".to_string(), reasoning);
            }
            Some("normal") => {
                obj.remove("thinking");
                obj.insert("reasoning".to_string(), json!({"enabled": false}));
            }
            _ => {
                let is_disabled = obj
                    .get("thinking")
                    .and_then(|t| t.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("disabled");
                if is_disabled {
                    obj.remove("thinking");
                    obj.insert("reasoning".to_string(), json!({"enabled": false}));
                }
            }
        }
    }

    /// Build the request body JSON that proxy_messages would produce after
    /// the thinking-override and Poolside-reasoning pass, given a route with
    /// no saved thinking_mode (Unset). Simulates what the _ arm does.
    fn apply_poolside_passthrough(
        route: &ModelRouteEntry,
        body: &mut serde_json::Value,
    ) {
        let uses_poolside =
            route.provider_id == "openrouter" && is_poolside_reasoning_model(&route.upstream_model);
        if !uses_poolside {
            return;
        }
        let obj = match body.as_object_mut() {
            Some(o) => o,
            None => return,
        };
        match route.thinking_mode_raw.as_deref() {
            Some("thinking") => {
                obj.remove("thinking");
                obj.insert("reasoning".to_string(), json!({"enabled": true}));
            }
            Some("normal") => {
                obj.remove("thinking");
                obj.insert("reasoning".to_string(), json!({"enabled": false}));
            }
            _ => {
                let is_disabled = obj
                    .get("thinking")
                    .and_then(|t| t.get("type"))
                    .and_then(|t| t.as_str())
                    == Some("disabled");
                if is_disabled {
                    obj.remove("thinking");
                    obj.insert("reasoning".to_string(), json!({"enabled": false}));
                }
            }
        }
    }

    #[test]
    fn poolside_thinking_disabled_translates_to_reasoning_disabled() {
        let route = make_route("openrouter", "poolside/laguna-s-2.1", None);
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_poolside_passthrough(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"enabled": false}))
        );
    }

    #[test]
    fn poolside_thinking_disabled_overwrites_existing_reasoning() {
        let route = make_route("openrouter", "poolside/laguna-s-2.1", None);
        let mut body = json!({
            "thinking": {"type": "disabled"},
            "reasoning": {"effort": "high"}
        });
        apply_poolside_passthrough(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"enabled": false}))
        );
    }

    #[test]
    fn poolside_thinking_enabled_unchanged() {
        let route = make_route("openrouter", "poolside/laguna-s-2.1", None);
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_poolside_passthrough(&route, &mut body);
        // passthrough arm: not disabled, stays unchanged
        assert_eq!(body.get("thinking"), Some(&json!({"type": "enabled"})));
        assert_eq!(body.get("reasoning"), None);
    }

    #[test]
    fn non_poolside_thinking_disabled_unchanged() {
        let route = make_route("openrouter", "anthropic/claude-sonnet-latest", None);
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_poolside_passthrough(&route, &mut body);
        assert_eq!(body.get("thinking"), Some(&json!({"type": "disabled"})));
    }

    #[test]
    fn poolside_no_thinking_field_unchanged() {
        let route = make_route("openrouter", "poolside/laguna-xs-2.1", None);
        let mut body = json!({"model": "test"});
        apply_poolside_passthrough(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), None);
    }

    // ── Tencent Hy3 reasoning translation tests ────────────────────────────────

    #[test]
    fn tencent_thinking_low_sends_effort_low() {
        let route = make_route_with_effort(
            "openrouter", "tencent/hy3", Some("thinking"), Some("low"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"effort": "low"}))
        );
    }

    #[test]
    fn tencent_thinking_high_sends_effort_high() {
        let route = make_route_with_effort(
            "openrouter", "tencent/hy3", Some("thinking"), Some("high"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"effort": "high"}))
        );
    }

    #[test]
    fn tencent_thinking_off_sends_enabled_false() {
        let route = make_route("openrouter", "tencent/hy3", Some("normal"));
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"enabled": false}))
        );
    }

    #[test]
    fn tencent_client_disabled_translates_correctly() {
        let route = make_route("openrouter", "tencent/hy3:free", None);
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(
            body.get("reasoning"),
            Some(&json!({"enabled": false}))
        );
    }

    #[test]
    fn tencent_thinking_unset_no_reasoning() {
        let route = make_route("openrouter", "tencent/hy3", None);
        let mut body = json!({"model": "hi"});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), None);
    }

    #[test]
    fn tencent_hy3_capabilities_force_thinking_false() {
        let caps = resolve_model_capabilities("tencent/hy3");
        assert!(!caps.force_thinking);
        let caps_free = resolve_model_capabilities("tencent/hy3:free");
        assert!(!caps_free.force_thinking);
    }

    // ── Real-path regression tests for resolve_proxy_config ──────────────────
    //
    // These tests exercise the actual production code path that maps
    // (provider_id, upstream_model, openrouter cache) → ModelRouteEntry.force_thinking.
    // They guard against regressions in the Hy3 scope guard.

    fn make_openrouter_model(id: &str, modalities: Vec<&str>) -> openrouter::OpenRouterModel {
        openrouter::OpenRouterModel {
            id: id.to_string(),
            canonical_slug: None,
            display_name: id.to_string(),
            description: None,
            context_length: None,
            max_completion_tokens: None,
            input_modalities: modalities.iter().map(|s| s.to_string()).collect(),
            output_modalities: Vec::new(),
            supported_parameters: Vec::new(),
            pricing: openrouter::OpenRouterPricing::default(),
        }
    }

    fn make_openrouter_config(
        upstream: &str,
        thinking_mode: Option<&str>,
        reasoning_effort: Option<&str>,
    ) -> crate::GatewayConfigResponse {
        let mut entry = make_entry(upstream, None);
        entry.thinking_mode = thinking_mode.map(|s| s.to_string());
        entry.reasoning_effort = reasoning_effort.map(|s| s.to_string());
        let mut models = HashMap::new();
        models.insert("claude-opus-5".to_string(), entry);

        let provider = crate::ProviderConfig {
            display_name: "OpenRouter".to_string(),
            upstream_url: "https://openrouter.ai/api/v1".to_string(),
            api_key_env: "OPENROUTER_API_KEY".to_string(),
            default_model: "openrouter/auto".to_string(),
            force_anthropic_version: None,
            supports_count_tokens: false,
            supports_vision: true,
            supports_video: false,
            supports_thinking: true,
            model_map: HashMap::new(),
            visible_models: vec!["claude-opus-5".to_string()],
            models: Some(models),
            openrouter_profiles: vec![],
            claude_code: None,
            hidden: false,
        };
        let mut providers = indexmap::IndexMap::new();
        providers.insert("openrouter".to_string(), provider);
        crate::GatewayConfigResponse {
            config_version: "1.0".to_string(),
            active_provider: Some("openrouter".to_string()),
            active_openrouter_profile_id: None,
            providers,
            server: crate::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                enable_cors: false,
            },
            non_vision_image_policy: "replace".to_string(),
            normalize_response_model_identity: true,
            claude_code: None,
        }
    }

    fn route_force_thinking(
        cfg: &crate::GatewayConfigResponse,
        cache: &[openrouter::OpenRouterModel],
    ) -> bool {
        let atomic = Arc::new(AtomicBool::new(true));
        let proxy_cfg = resolve_proxy_config(cfg, cache, atomic).expect("resolve_proxy_config");
        proxy_cfg
            .model_route
            .get("claude-opus-5")
            .expect("route exists")
            .force_thinking
    }

    #[test]
    fn hy3_route_force_thinking_false_when_cache_says_thinking_supported() {
        // Even when the cache reports supports_thinking=true (the 3rd cache
        // tuple element), the Hy3 scope guard must force force_thinking=false.
        let cfg = make_openrouter_config("tencent/hy3", Some("thinking"), Some("low"));
        let cache = vec![make_openrouter_model("tencent/hy3", vec!["text"])];
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn hy3_route_force_thinking_false_when_cache_empty() {
        // No cache at all — Hy3 still resolves to force_thinking=false.
        let cfg = make_openrouter_config("tencent/hy3", Some("thinking"), Some("low"));
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn poolside_route_force_thinking_false() {
        // Poolside is not Hy3 — regression check that the scope guard
        // doesn't accidentally flip other OpenRouter models to true.
        let cfg = make_openrouter_config("poolside/laguna-s-2.1", Some("thinking"), Some("max"));
        let cache = vec![make_openrouter_model("poolside/laguna-s-2.1", vec!["text"])];
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn unknown_openrouter_model_force_thinking_false() {
        // An unknown OpenRouter model with no cache entry — must not be
        // force-marked as thinking.
        let cfg = make_openrouter_config("custom/private-model", None, None);
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn unknown_openrouter_model_with_cache_keeps_force_thinking_false() {
        // Cache present but model not in cache — should still be false.
        // (resolve_capabilities_from_cache returns None → the conservative
        // branch fires with force_thinking=false.)
        let cfg = make_openrouter_config("custom/private-model", None, None);
        let cache = vec![make_openrouter_model("another/model", vec!["text"])];
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn non_openrouter_provider_uses_static_resolver() {
        // Regression: providers other than OpenRouter (e.g. DeepSeek) still
        // get force_thinking directly from resolve_model_capabilities().
        let cfg = make_openrouter_config("deepseek-v4-pro", None, None);
        // Override provider id to deepseek via fresh config.
        let mut cfg = cfg;
        let deepseek = cfg.providers.remove("openrouter").unwrap();
        cfg.providers.insert("deepseek".to_string(), deepseek);
        cfg.active_provider = Some("deepseek".to_string());
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn is_tencent_hy3_matches_hy3_ids() {
        assert!(is_tencent_hy3("tencent/hy3"));
        assert!(is_tencent_hy3("tencent/hy3:free"));
        assert!(!is_tencent_hy3("tencent/other-model"));
        assert!(!is_tencent_hy3("poolside/laguna-s-2.1"));
        assert!(!is_tencent_hy3(""));
    }

    #[test]
    fn tencent_thinking_max_sends_effort_max() {
        // Hy3 has no Max UI option, but the proxy still handles the value
        // safely if the config somehow has it.
        let route = make_route_with_effort(
            "openrouter", "tencent/hy3", Some("thinking"), Some("max"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "max"})));
    }

    #[test]
    fn tencent_thinking_unset_effort_uses_enabled_true() {
        // When thinking_mode_raw="thinking" but reasoning_effort is unset,
        // fall back to {"enabled": true} — matches Poolside behavior.
        let route = make_route_with_effort(
            "openrouter", "tencent/hy3", Some("thinking"), None,
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_tencent_reasoning(&route, &mut body);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": true})));
    }

    // ── InclusionAI reasoning translation helpers ──────────────────────────

    fn apply_inclusion_reasoning(
        route: &ModelRouteEntry,
        body: &mut serde_json::Value,
    ) {
        let uses_inclusion =
            route.provider_id == "openrouter" && is_inclusionai_model(&route.upstream_model);
        if !uses_inclusion {
            return;
        }
        let obj = match body.as_object_mut() {
            Some(o) => o,
            None => return,
        };
        if is_ling_non_thinking_model(&route.upstream_model) {
            // Ling 2.6 1T / Ling 2.6 Flash: NO thinking capability.
            obj.remove("thinking");
            obj.remove("reasoning");
            return;
        }
        if is_ling_free_model(&route.upstream_model) {
            // Ling 3.0 Flash Free: always normalize.
            obj.remove("thinking");
            obj.remove("reasoning");
            let enabled = matches!(route.thinking_mode_raw.as_deref(), Some("thinking"));
            obj.insert("reasoning".to_string(), json!({"enabled": enabled}));
            return;
        }
        // Ring 2.6 1T: thinking forced (high / xhigh only).
        match route.thinking_mode_raw.as_deref() {
            Some("thinking") => {
                obj.remove("thinking");
                let reasoning = match route.reasoning_effort.as_deref() {
                    Some("xhigh") => json!({"effort": "xhigh"}),
                    Some("high") => json!({"effort": "high"}),
                    _ => json!({"effort": "xhigh"}),
                };
                obj.insert("reasoning".to_string(), reasoning);
            }
            _ => {
                obj.remove("thinking");
                obj.insert("reasoning".to_string(), json!({"effort": "xhigh"}));
            }
        }
    }

    fn apply_stepfun_reasoning(
        route: &ModelRouteEntry,
        body: &mut serde_json::Value,
    ) {
        let uses_stepfun =
            route.provider_id == "openrouter" && is_stepfun_model(&route.upstream_model);
        if !uses_stepfun {
            return;
        }
        let obj = match body.as_object_mut() {
            Some(o) => o,
            None => return,
        };
        let is_step35 = route.upstream_model == "stepfun/step-3.5-flash";
        match route.thinking_mode_raw.as_deref() {
            Some("thinking") => {
                obj.remove("thinking");
                if is_step35 {
                    obj.insert("reasoning".to_string(), json!({"enabled": true}));
                } else {
                    let reasoning = match route.reasoning_effort.as_deref() {
                        Some(effort @ ("low" | "medium" | "high")) => {
                            json!({"effort": effort})
                        }
                        _ => json!({"effort": "medium"}),
                    };
                    obj.insert("reasoning".to_string(), reasoning);
                }
            }
            _ => {
                obj.remove("thinking");
                if is_step35 {
                    obj.insert("reasoning".to_string(), json!({"enabled": true}));
                } else {
                    obj.insert("reasoning".to_string(), json!({"effort": "medium"}));
                }
            }
        }
    }

    // ── InclusionAI reasoning translation tests ────────────────────────────

    #[test]
    fn ring_xhigh_sends_effort_xhigh() {
        let route = make_route_with_effort(
            "openrouter", "inclusionai/ring-2.6-1t", Some("thinking"), Some("xhigh"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "xhigh"})));
    }

    #[test]
    fn ring_high_sends_effort_high() {
        let route = make_route_with_effort(
            "openrouter", "inclusionai/ring-2.6-1t", Some("thinking"), Some("high"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "high"})));
    }

    #[test]
    fn ring_normal_is_normalized_to_xhigh() {
        let route = make_route("openrouter", "inclusionai/ring-2.6-1t", Some("normal"));
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "xhigh"})));
    }

    #[test]
    fn ring_invalid_effort_normalized_to_xhigh() {
        let route = make_route_with_effort(
            "openrouter", "inclusionai/ring-2.6-1t", Some("thinking"), Some("invalid"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "xhigh"})));
    }

    #[test]
    fn ring_unset_normalized_to_xhigh() {
        let route = make_route("openrouter", "inclusionai/ring-2.6-1t", None);
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "xhigh"})));
    }

    #[test]
    fn ling26_removes_thinking_and_reasoning() {
        let route = make_route("openrouter", "inclusionai/ling-2.6-1t", None);
        let mut body = json!({"thinking": {"type": "enabled"}, "reasoning": {"effort": "high"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert!(body.get("thinking").is_none());
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn ling26_flash_removes_thinking_and_reasoning() {
        let route = make_route("openrouter", "inclusionai/ling-2.6-flash", Some("thinking"));
        let mut body = json!({"thinking": {"type": "enabled"}, "reasoning": {"enabled": true}});
        apply_inclusion_reasoning(&route, &mut body);
        assert!(body.get("thinking").is_none());
        assert!(body.get("reasoning").is_none());
    }

    #[test]
    fn ling_free_on_sends_enabled_true() {
        let route = make_route("openrouter", "inclusionai/ling-3.0-flash:free", Some("thinking"));
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": true})));
    }

    #[test]
    fn ling_free_off_sends_enabled_false() {
        let route = make_route("openrouter", "inclusionai/ling-3.0-flash:free", Some("normal"));
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": false})));
    }

    #[test]
    fn ling_free_unset_sends_enabled_false() {
        let route = make_route("openrouter", "inclusionai/ling-3.0-flash:free", None);
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_inclusion_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": false})));
    }

    // ── StepFun reasoning translation tests ────────────────────────────────

    #[test]
    fn step37_high_sends_effort_high() {
        let route = make_route_with_effort(
            "openrouter", "stepfun/step-3.7-flash", Some("thinking"), Some("high"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "high"})));
    }

    #[test]
    fn step37_medium_sends_effort_medium() {
        let route = make_route_with_effort(
            "openrouter", "stepfun/step-3.7-flash", Some("thinking"), Some("medium"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn step37_low_sends_effort_low() {
        let route = make_route_with_effort(
            "openrouter", "stepfun/step-3.7-flash", Some("thinking"), Some("low"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "low"})));
    }

    #[test]
    fn step37_normal_is_normalized_to_medium() {
        let route = make_route("openrouter", "stepfun/step-3.7-flash", Some("normal"));
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn step37_invalid_effort_normalized_to_medium() {
        let route = make_route_with_effort(
            "openrouter", "stepfun/step-3.7-flash", Some("thinking"), Some("max"),
        );
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn step37_unset_normalized_to_medium() {
        let route = make_route("openrouter", "stepfun/step-3.7-flash", None);
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn step35_thinking_sends_enabled_true() {
        let route = make_route("openrouter", "stepfun/step-3.5-flash", Some("thinking"));
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": true})));
    }

    #[test]
    fn step35_normal_is_normalized_to_enabled_true() {
        let route = make_route("openrouter", "stepfun/step-3.5-flash", Some("normal"));
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": true})));
    }

    #[test]
    fn step35_unset_normalized_to_enabled_true() {
        let route = make_route("openrouter", "stepfun/step-3.5-flash", None);
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_stepfun_reasoning(&route, &mut body);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"enabled": true})));
    }

    // ── Google Gemini reasoning tests ───────────────────────────────

    fn apply_gemini(obj: &mut serde_json::Value, model: &str, thinking_mode: Option<&str>, effort: Option<&str>) {
        apply_gemini_reasoning(obj.as_object_mut().unwrap(), model, thinking_mode, effort);
    }

    #[test]
    fn gemini_31_pro_high_sends_effort_high() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_gemini(&mut body, "google/gemini-3.1-pro-preview", Some("thinking"), Some("high"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "high"})));
    }

    #[test]
    fn gemini_37_flash_medium_sends_effort_medium() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_gemini(&mut body, "google/gemini-3.7-flash", Some("thinking"), Some("medium"));
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn gemini_35_lite_minimal_sends_effort_minimal() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_gemini(&mut body, "google/gemini-3.5-flash-lite", Some("thinking"), Some("minimal"));
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "minimal"})));
    }

    #[test]
    fn gemini_normalization_matches_requirements() {
        assert_eq!(normalize_gemini_reasoning_effort("google/gemini-3.1-pro-preview", "minimal"), "high");
        assert_eq!(normalize_gemini_reasoning_effort("google/gemini-3.7-flash", "xhigh"), "high");
        assert_eq!(normalize_gemini_reasoning_effort("google/gemini-3.5-flash-lite", "max"), "high");
        assert_eq!(normalize_gemini_reasoning_effort("google/gemini-3.5-flash-lite", "minimal"), "minimal");
    }

    #[test]
    fn gemini_normal_thinking_normalizes_to_high() {
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_gemini(&mut body, "google/gemini-3.1-pro-preview", Some("normal"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "high"})));
    }
    // ── OpenAI GPT-5.6 reasoning tests (pure function) ──────────────────

    fn apply_openai(obj: &mut serde_json::Value, thinking_mode: Option<&str>, effort: Option<&str>) {
        apply_openai_reasoning(obj.as_object_mut().unwrap(), thinking_mode, effort);
    }

    #[test]
    fn openai_thinking_medium_sends_effort_medium() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, Some("thinking"), Some("medium"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn openai_thinking_max_sends_effort_max() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, Some("thinking"), Some("max"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "max"})));
    }

    #[test]
    fn openai_normal_sends_effort_none() {
        let mut body = json!({"thinking": {"type": "disabled"}});
        apply_openai(&mut body, Some("normal"), None);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "none"})));
    }

    #[test]
    fn openai_unset_defaults_to_medium() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, None, None);
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn openai_xhigh_sends_effort_xhigh() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, Some("thinking"), Some("xhigh"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "xhigh"})));
    }

    #[test]
    fn openai_low_sends_effort_low() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, Some("thinking"), Some("low"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "low"})));
    }

    #[test]
    fn openai_invalid_effort_defaults_to_medium() {
        let mut body = json!({"thinking": {"type": "enabled"}});
        apply_openai(&mut body, Some("thinking"), Some("invalid"));
        assert_eq!(body.get("thinking"), None);
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "medium"})));
    }

    #[test]
    fn openai_removes_anthropic_thinking_key() {
        let mut body = json!({"thinking": {"type": "enabled", "budget_tokens": 4000}});
        apply_openai(&mut body, Some("thinking"), Some("high"));
        assert_eq!(body.get("thinking"), None);
    }

    #[test]
    fn openai_removes_reasoning_effort_key() {
        let mut body = json!({"thinking": {"type": "enabled"}, "reasoning_effort": "high"});
        apply_openai(&mut body, Some("thinking"), Some("high"));
        assert_eq!(body.get("reasoning_effort"), None);
    }

    #[test]
    fn openai_existing_reasoning_object_is_replaced() {
        let mut body = json!({"thinking": {"type": "enabled"}, "reasoning": {"effort": "low"}});
        apply_openai(&mut body, Some("thinking"), Some("high"));
        assert_eq!(body.get("reasoning"), Some(&json!({"effort": "high"})));
    }

    // ── OpenRouter multi-profile route tests ──────────────────────────────

    fn make_openrouter_profile_config(
        active_profile_id: &str,
        profiles: Vec<crate::OpenRouterProfile>,
    ) -> crate::GatewayConfigResponse {
        let provider = crate::ProviderConfig {
            display_name: "OpenRouter".to_string(),
            upstream_url: "https://openrouter.ai/api/v1".to_string(),
            api_key_env: "OPENROUTER_API_KEY".to_string(),
            default_model: "openrouter/auto".to_string(),
            force_anthropic_version: None,
            supports_count_tokens: false,
            supports_vision: true,
            supports_video: false,
            supports_thinking: true,
            model_map: HashMap::new(),
            visible_models: vec!["claude-opus-5".to_string()],
            models: None,
            openrouter_profiles: profiles,
            claude_code: None,
            hidden: false,
        };
        let mut providers = indexmap::IndexMap::new();
        providers.insert("openrouter".to_string(), provider);
        crate::GatewayConfigResponse {
            config_version: "1.0".to_string(),
            active_provider: Some("openrouter".to_string()),
            active_openrouter_profile_id: Some(active_profile_id.to_string()),
            providers,
            server: crate::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                enable_cors: false,
            },
            non_vision_image_policy: "replace".to_string(),
            normalize_response_model_identity: true,
            claude_code: None,
        }
    }

    fn make_profile(id: &str, display_name: &str, model_key: &str, upstream: &str) -> crate::OpenRouterProfile {
        let mut models = HashMap::new();
        models.insert(model_key.to_string(), make_entry(upstream, None));
        let mut model_map = HashMap::new();
        model_map.insert(model_key.to_string(), upstream.to_string());
        crate::OpenRouterProfile {
            id: id.to_string(),
            display_name: display_name.to_string(),
            model_map,
            visible_models: vec![model_key.to_string()],
            models,
            hidden: false,
            claude_code: None,
        }
    }

    #[test]
    fn resolve_proxy_config_uses_selected_openrouter_profile() {
        let p1 = make_profile("p1", "Profile One", "claude-opus-5", "poolside/laguna-s-2.1");
        let p2 = make_profile("p2", "Profile Two", "claude-opus-5", "tencent/hy3");
        let cfg = make_openrouter_profile_config("p2", vec![p1, p2]);
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        let atomic = Arc::new(AtomicBool::new(true));
        let proxy_cfg = resolve_proxy_config(&cfg, &cache, atomic).expect("resolve_proxy_config");
        let route = proxy_cfg.model_route.get("claude-opus-5").expect("route exists");
        // Active profile p2 has upstream tencent/hy3
        assert_eq!(route.upstream_model, "tencent/hy3");
    }

    #[test]
    fn resolve_proxy_config_changes_routes_after_profile_switch() {
        let p1 = make_profile("p1", "Profile One", "claude-opus-5", "poolside/laguna-s-2.1");
        let p2 = make_profile("p2", "Profile Two", "claude-opus-5", "deepseek-v4-pro");
        let mut cfg = make_openrouter_profile_config("p1", vec![p1, p2]);
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        let atomic = Arc::new(AtomicBool::new(true));
        let proxy_cfg = resolve_proxy_config(&cfg, &cache, atomic.clone()).expect("resolve");
        assert_eq!(
            proxy_cfg.model_route.get("claude-opus-5").unwrap().upstream_model,
            "poolside/laguna-s-2.1"
        );
        // Switch active profile
        cfg.active_openrouter_profile_id = Some("p2".to_string());
        let proxy_cfg2 = resolve_proxy_config(&cfg, &cache, atomic).expect("resolve after switch");
        assert_eq!(
            proxy_cfg2.model_route.get("claude-opus-5").unwrap().upstream_model,
            "deepseek-v4-pro"
        );
    }

    #[test]
    fn resolve_proxy_config_uses_kimi_static_capabilities() {
        // Kimi K3 in a non-OpenRouter provider uses static capabilities
        // (force_thinking=true, suppress_thinking_parameter=true, forced_reasoning_effort=None)
        std::env::set_var("_TEST_KIMI_KEY", "test-key");
        let entry = make_entry("kimi-k3", None);
        let mut models = HashMap::new();
        models.insert("claude-opus-5".to_string(), entry);
        let provider = crate::ProviderConfig {
            display_name: "Kimi".to_string(),
            upstream_url: "https://api.moonshot.cn/v1".to_string(),
            api_key_env: "_TEST_KIMI_KEY".to_string(),
            default_model: "kimi-k3".to_string(),
            force_anthropic_version: None,
            supports_count_tokens: false,
            supports_vision: true,
            supports_video: false,
            supports_thinking: true,
            model_map: HashMap::new(),
            visible_models: vec!["claude-opus-5".to_string()],
            models: Some(models),
            openrouter_profiles: vec![],
            claude_code: None,
            hidden: false,
        };
        let mut providers = indexmap::IndexMap::new();
        providers.insert("kimi".to_string(), provider);
        let cfg = crate::GatewayConfigResponse {
            config_version: "1.0".to_string(),
            active_provider: Some("kimi".to_string()),
            active_openrouter_profile_id: None,
            providers,
            server: crate::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                enable_cors: false,
            },
            non_vision_image_policy: "replace".to_string(),
            normalize_response_model_identity: true,
            claude_code: None,
        };
        let cache: Vec<openrouter::OpenRouterModel> = Vec::new();
        let atomic = Arc::new(AtomicBool::new(true));
        let proxy_cfg = resolve_proxy_config(&cfg, &cache, atomic).expect("resolve");
        let route = proxy_cfg.model_route.get("claude-opus-5").expect("route exists");
        assert!(route.force_thinking);
        assert!(route.suppress_thinking_parameter);
        // K3: reasoning_effort is config-driven, not forced by static caps
        assert_eq!(route.forced_reasoning_effort.as_deref(), None);
    }

    #[test]
    fn cached_reasoning_does_not_change_force_thinking_in_runtime_route() {
        // Regression: cache saying "supports thinking" must not leak into
        // force_thinking. DeepSeek V4 Pro has static force_thinking=false;
        // a cache entry with supported_parameters=["reasoning"] must not flip it.
        let cfg = make_openrouter_config("deepseek-v4-pro", None, None);
        let cache = vec![make_openrouter_model("deepseek-v4-pro", vec!["text"])];
        assert!(!route_force_thinking(&cfg, &cache));
    }

    #[test]
    fn resolve_proxy_config_distinguishes_cache_hit_miss_and_unavailable() {
        // Each of the 3 OpenRouterCacheLookup states must produce a
        // distinct, independently verifiable result.

        // --- Hit: model in cache with input_modalities=["video"] ---
        let cfg_hit = make_openrouter_config("custom/private-model", None, None);
        let cache_hit = vec![make_openrouter_model("custom/private-model", vec!["video"])];
        let atomic_hit = Arc::new(AtomicBool::new(true));
        let proxy_hit = resolve_proxy_config(&cfg_hit, &cache_hit, atomic_hit).expect("resolve");
        let route_hit = proxy_hit.model_route.get("claude-opus-5").expect("route hit");
        assert!(!route_hit.supports_image_url);
        assert!(!route_hit.supports_image_base64);
        assert!(route_hit.supports_video_url);
        assert!(route_hit.supports_video_base64);

        // --- Miss: cache non-empty but model absent → fallback image=true, video=false ---
        let cfg_miss = make_openrouter_config("custom/private-model", None, None);
        let cache_miss = vec![make_openrouter_model("other/model", vec!["image"])];
        let atomic_miss = Arc::new(AtomicBool::new(true));
        let proxy_miss = resolve_proxy_config(&cfg_miss, &cache_miss, atomic_miss).expect("resolve");
        let route_miss = proxy_miss.model_route.get("claude-opus-5").expect("route miss");
        assert!(route_miss.supports_image_url);
        assert!(route_miss.supports_image_base64);
        assert!(!route_miss.supports_video_url);
        assert!(!route_miss.supports_video_base64);

        // --- Unavailable: empty cache, provider.supports_vision=false → image=false, video=false ---
        let cfg_unavail = make_openrouter_config("custom/private-model", None, None);
        // Build config with supports_vision=false for the provider
        let mut cfg_unavail = cfg_unavail;
        cfg_unavail.providers.get_mut("openrouter").unwrap().supports_vision = false;
        let cache_unavail: Vec<openrouter::OpenRouterModel> = Vec::new();
        let atomic_unavail = Arc::new(AtomicBool::new(true));
        let proxy_unavail = resolve_proxy_config(&cfg_unavail, &cache_unavail, atomic_unavail).expect("resolve");
        let route_unavail = proxy_unavail.model_route.get("claude-opus-5").expect("route unavail");
        assert!(!route_unavail.supports_image_url);
        assert!(!route_unavail.supports_image_base64);
        assert!(!route_unavail.supports_video_url);
        assert!(!route_unavail.supports_video_base64);
    }

    // ── Shared route-resolution agreement with model_routing ─────────────

    fn make_direct_config_for_route_test() -> crate::GatewayConfigResponse {
        let mut models = HashMap::new();
        let mut opus = make_entry("deepseek-v4-pro", None);
        let mut flash = make_entry("deepseek-v4-flash", None);
        let mut opus_alias = make_entry("deepseek-v4-pro", Some("claude-opus-5"));
        opus.visible = true;
        flash.visible = true;
        opus_alias.visible = false;
        models.insert("claude-opus-5".to_string(), opus);
        models.insert("claude-sonnet-5".to_string(), flash.clone());
        models.insert("claude-haiku-4-5".to_string(), flash.clone());
        models.insert("claude-opus".to_string(), opus_alias);

        let provider = crate::ProviderConfig {
            display_name: "DeepSeek".to_string(),
            upstream_url: "https://api.deepseek.com".to_string(),
            api_key_env: "DEEPSEEK_API_KEY".to_string(),
            default_model: "deepseek-v4-flash".to_string(),
            force_anthropic_version: None,
            supports_count_tokens: false,
            supports_vision: false,
            supports_video: false,
            supports_thinking: true,
            model_map: HashMap::new(),
            visible_models: vec![
                "claude-opus-5".to_string(),
                "claude-sonnet-5".to_string(),
                "claude-haiku-4-5".to_string(),
            ],
            models: Some(models),
            openrouter_profiles: vec![],
            claude_code: None,
            hidden: false,
        };
        let mut providers = indexmap::IndexMap::new();
        providers.insert("deepseek".to_string(), provider);
        crate::GatewayConfigResponse {
            config_version: "1.0".to_string(),
            active_provider: Some("deepseek".to_string()),
            active_openrouter_profile_id: None,
            providers,
            server: crate::ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 4000,
                enable_cors: false,
            },
            non_vision_image_policy: "replace".to_string(),
            normalize_response_model_identity: true,
            claude_code: None,
        }
    }

    #[test]
    fn proxy_routes_agree_with_model_routing_resolution() {
        // The proxy's typed model_route and the shared raw-JSON resolver must
        // produce the same upstream model for every routed gateway model.
        let cfg = make_direct_config_for_route_test();
        let atomic = Arc::new(AtomicBool::new(true));
        let proxy_cfg = resolve_proxy_config(&cfg, &[], atomic).expect("resolve_proxy_config");
        let raw = serde_json::to_value(&cfg).expect("serialize config");

        assert!(!proxy_cfg.model_route.is_empty(), "expected routes");
        for (route, entry) in &proxy_cfg.model_route {
            let upstream = crate::model_routing::resolve_route_upstream_model(
                &raw,
                &entry.provider_id,
                None,
                route,
            );
            assert_eq!(
                upstream.as_deref(),
                Some(entry.upstream_model.as_str()),
                "route '{}' diverges between proxy and model_routing",
                route
            );
        }
    }
}
