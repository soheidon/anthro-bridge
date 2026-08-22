use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::openrouter::OpenRouterModel;

// ---------------------------------------------------------------------------
// Shared model capability types — used by both proxy.rs (runtime routing)
// and lib.rs (template generation, config migration, normalization).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    pub supports_image_url: bool,
    pub supports_image_base64: bool,
    pub supports_video_url: bool,
    pub supports_video_base64: bool,
    pub force_thinking: bool,
    /// If true, do NOT send `thinking` parameter upstream (K3 uses reasoning_effort instead)
    pub suppress_thinking_parameter: bool,
    /// If set, inject this reasoning_effort value (e.g. "max" for K3)
    pub forced_reasoning_effort: Option<&'static str>,
}

impl ModelCapabilities {
    pub fn all_false() -> Self {
        ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }
    }
}

// ---------------------------------------------------------------------------
// OpenRouter cache lookup state — 3 distinct states map to the 3 paths
// in proxy.rs's legacy inline capability resolution.
// ---------------------------------------------------------------------------

pub enum OpenRouterCacheLookup<'a> {
    /// openrouter_models list is empty — no cache data at all
    Unavailable,
    /// Model found in the cached OpenRouter model list
    Hit(&'a OpenRouterModel),
    /// Cache exists but this model is not in it
    Miss,
}

// ---------------------------------------------------------------------------
// Static capability lookup — based purely on the upstream model ID.
// Used during template generation, migration, and normalization — places
// where an OpenRouter API cache is not available.
//
// **Moved verbatim from proxy.rs's existing resolve_model_capabilities**
// — do NOT re-implement as a simplified two-way branch.
// ---------------------------------------------------------------------------

/// Single source of truth for statically known model capabilities.
/// Returns `Some(caps)` for every model with an explicit entry in the static
/// resolver. Returns `None` for unknown / custom models — callers decide
/// whether to fall back to all-false defaults or preserve existing values.
pub fn try_resolve_static_model_capabilities(upstream_model: &str) -> Option<ModelCapabilities> {
    match upstream_model {
        // ── DeepSeek ──
        "deepseek-v4-pro" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "deepseek-v4-flash" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "deepseek-v4-flash-vision-exp" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── MiniMax ──
        "MiniMax-M3" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: true,
            supports_video_base64: true,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "MiniMax-M2.7" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: true,
            supports_video_base64: true,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "MiniMax-M2.7-highspeed" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── Kimi / Moonshot ──
        "kimi-k3" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false, // ms:// only, no proxy conversion
            force_thinking: true,
            suppress_thinking_parameter: true,
            forced_reasoning_effort: None,
        }),
        "kimi-k2.7-code" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: true,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "kimi-k2.7-code-highspeed" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: true,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "kimi-k2.6" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: true,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "kimi-k2.5" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: true,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── MiMo ──
        "mimo-v2.5-pro" | "mimo-v2.5-pro-ultraspeed" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "mimo-v2.5" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── Poolside Laguna (OpenRouter) ──
        "poolside/laguna-s-2.1" | "poolside/laguna-s-2.1:free"
        | "poolside/laguna-xs-2.1" | "poolside/laguna-xs-2.1:free" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── Tencent (OpenRouter) ──
        "tencent/hy3" | "tencent/hy3:free" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── InclusionAI (OpenRouter) ──
        "inclusionai/ring-2.6-1t" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // Ling 2.6: NO thinking capability. suppress_thinking_parameter signals
        // the proxy layer to strip any thinking/reasoning fields.
        "inclusionai/ling-2.6-1t" | "inclusionai/ling-2.6-flash" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: true,
            forced_reasoning_effort: None,
        }),
        // Ling 3.0 Free: thinking optional (off/on) — SEPARATE from Ling 2.6
        "inclusionai/ling-3.0-flash:free" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── Google Gemini (OpenRouter) ──
        // Initial static capabilities: image-capable, reasoning mandatory in UI.
        "google/gemini-3.1-pro-preview" | "google/gemini-3.7-flash"
        | "google/gemini-3.5-flash-lite" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),        // ── OpenAI GPT-5.6 (OpenRouter) ──
        "openai/gpt-5.6-sol" | "openai/gpt-5.6-sol-pro"
        | "openai/gpt-5.6-terra" | "openai/gpt-5.6-terra-pro"
        | "openai/gpt-5.6-luna" | "openai/gpt-5.6-luna-pro" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: false,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── StepFun (OpenRouter) ──
        // ⚠️ Step 3.7 video flags: start as false until verified with real
        // OpenRouter requests. Image flags are true (confirmed via metadata).
        "stepfun/step-3.7-flash" => Some(ModelCapabilities {
            supports_image_url: true,
            supports_image_base64: true,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        "stepfun/step-3.5-flash" => Some(ModelCapabilities {
            supports_image_url: false,
            supports_image_base64: false,
            supports_video_url: false,
            supports_video_base64: false,
            force_thinking: true,
            suppress_thinking_parameter: false,
            forced_reasoning_effort: None,
        }),
        // ── Unknown / custom ──
        _ => None,
    }
}

/// Configへ保存するcapability → モデルIDから決まる静的・永続的な基準値.
/// Falls back to all-false for unknown models.
pub fn resolve_static_model_capabilities(upstream_model: &str) -> ModelCapabilities {
    try_resolve_static_model_capabilities(upstream_model).unwrap_or_else(ModelCapabilities::all_false)
}

// ---------------------------------------------------------------------------
// Context-window metadata — single source of truth is the embedded resource
// JSON (`gui/src-tauri/resources/model_context_windows.json`). There is no
// TS-side re-implementation; the frontend only renders what Rust resolves.
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextWindowSource {
    Official,
    ProviderApi,
    Builtin,
    User,
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StaticContextWindow {
    pub context_length: u64,
    pub source: ContextWindowSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
}

#[derive(Deserialize)]
pub struct ModelContextWindowsFile {
    pub schema_version: u32,
    pub models: HashMap<String, StaticContextWindow>,
}

fn context_window_file() -> &'static ModelContextWindowsFile {
    static FILE: OnceLock<ModelContextWindowsFile> = OnceLock::new();
    FILE.get_or_init(|| {
        let parsed: ModelContextWindowsFile = serde_json::from_str(
            include_str!("../resources/model_context_windows.json"),
        )
        .expect("embedded model_context_windows.json must be valid JSON");
        assert_eq!(
            parsed.schema_version, 1,
            "embedded model_context_windows.json schema_version must be 1"
        );
        parsed
    })
}

/// Lookup inside a parsed context-window file. Keys are either
/// `provider_id:upstream_model` (provider-specific) or a bare `upstream_model`
/// (generic builtin). Provider-specific entries win over generic ones.
pub fn lookup_static_context_window<'a>(
    file: &'a ModelContextWindowsFile,
    provider_id: &str,
    upstream_model: &str,
) -> Option<&'a StaticContextWindow> {
    let provider_key = format!("{}:{}", provider_id, upstream_model);
    file.models
        .get(&provider_key)
        .or_else(|| file.models.get(upstream_model))
}

/// Static context-window lookup against the embedded resource JSON.
/// Returns `None` when the model is unknown (callers treat that as
/// `ContextWindowSource::Unknown`).
pub fn try_resolve_static_context_window(
    provider_id: &str,
    upstream_model: &str,
) -> Option<StaticContextWindow> {
    lookup_static_context_window(context_window_file(), provider_id, upstream_model).cloned()
}

// ---------------------------------------------------------------------------
// Effective capability lookup — for actual gateway route building.
//
// This is the RUNTIME path. Its result may differ from the static value
// stored in config (from resolve_static_model_capabilities) when OpenRouter
// cache data is present. Dynamic cache values are never persisted — config
// always stores the static baseline.
//
// Priority (matches legacy proxy.rs inline behavior):
//   1. OpenRouter cached modality (image/video) from cache Hit variant
//   2. Cache Miss / Unavailable fallback values
//   3. Static resolve_static_model_capabilities for force_thinking,
//      suppress_thinking_parameter, forced_reasoning_effort
//   4. Hard overrides applied last (e.g. Hy3 force_thinking=false)
//
// Called by resolve_proxy_config. NOT called by set_model_upstream capability
// sync — that path uses resolve_static_model_capabilities to persist the
// stable baseline.
//
// Gatewayで使うcapability → 静的基準値にOpenRouterキャッシュとhard overrideを統合した実効値
// ---------------------------------------------------------------------------

pub fn resolve_effective_model_capabilities(
    upstream_model: &str,
    provider_supports_vision: bool,
    cache: OpenRouterCacheLookup<'_>,
) -> ModelCapabilities {
    let static_caps = resolve_static_model_capabilities(upstream_model);

    let (image_mod, video_mod) = match cache {
        OpenRouterCacheLookup::Hit(model) => (
            model.input_modalities.iter().any(|m| m == "image"),
            model.input_modalities.iter().any(|m| m == "video"),
        ),
        OpenRouterCacheLookup::Miss => (true, false),
        OpenRouterCacheLookup::Unavailable => (provider_supports_vision, false),
    };

    let mut caps = ModelCapabilities {
        supports_image_url: image_mod,
        supports_image_base64: image_mod,
        supports_video_url: video_mod,
        supports_video_base64: video_mod,
        force_thinking: static_caps.force_thinking,
        suppress_thinking_parameter: static_caps.suppress_thinking_parameter,
        forced_reasoning_effort: static_caps.forced_reasoning_effort,
    };

    apply_hard_overrides(upstream_model, &mut caps);
    caps
}

// ---------------------------------------------------------------------------
// Hard overrides — model-specific overrides that always win, regardless of
// cache or static values.
// ---------------------------------------------------------------------------

fn apply_hard_overrides(upstream_model: &str, caps: &mut ModelCapabilities) {
    if is_hy3_model(upstream_model) {
        caps.force_thinking = false;
    }
}

// ---------------------------------------------------------------------------
// Model-ID classification helpers — shared between lib.rs and proxy.rs
// ---------------------------------------------------------------------------

/// Check if a model ID is a known Poolside Laguna variant.
pub fn is_laguna_model(model: &str) -> bool {
    matches!(
        model,
        "poolside/laguna-s-2.1"
            | "poolside/laguna-s-2.1:free"
            | "poolside/laguna-xs-2.1"
            | "poolside/laguna-xs-2.1:free"
    )
}

/// Check if a model ID is a known Tencent Hy3 variant.
pub fn is_hy3_model(model: &str) -> bool {
    matches!(model, "tencent/hy3" | "tencent/hy3:free")
}

/// Check if a model ID is a known InclusionAI model.
pub fn is_inclusionai_model(model: &str) -> bool {
    matches!(
        model,
        "inclusionai/ring-2.6-1t"
            | "inclusionai/ling-2.6-1t"
            | "inclusionai/ling-2.6-flash"
            | "inclusionai/ling-3.0-flash:free"
    )
}

/// Check if a model ID is a known Google Gemini model on OpenRouter.
pub fn is_gemini_model(model: &str) -> bool {
    matches!(
        model,
        "google/gemini-3.1-pro-preview"
            | "google/gemini-3.7-flash"
            | "google/gemini-3.5-flash-lite"
    )
}

/// Check if a model ID is a known StepFun model.
pub fn is_stepfun_model(model: &str) -> bool {
    matches!(model, "stepfun/step-3.7-flash" | "stepfun/step-3.5-flash")
}

/// Check if a model ID is a Ling non-thinking model (Ling 2.6 family).
pub fn is_ling_non_thinking_model(model: &str) -> bool {
    matches!(model, "inclusionai/ling-2.6-1t" | "inclusionai/ling-2.6-flash")
}

/// Check if a model ID is the Ling 3.0 Free model.
pub fn is_ling_free_model(model: &str) -> bool {
    matches!(model, "inclusionai/ling-3.0-flash:free")
}

/// Check if an OpenRouter upstream model uses Poolside-specific reasoning format.
pub fn is_poolside_reasoning_model(model: &str) -> bool {
    is_laguna_model(model)
}

/// Check if an OpenRouter upstream model is the Tencent Hy3 family.
pub fn is_tencent_hy3(model: &str) -> bool {
    is_hy3_model(model)
}

/// Check if an OpenRouter upstream model is the OpenAI GPT-5.6 family.
pub fn is_openai_gpt56_model(model: &str) -> bool {
    matches!(
        model,
        "openai/gpt-5.6-sol" | "openai/gpt-5.6-sol-pro"
        | "openai/gpt-5.6-terra" | "openai/gpt-5.6-terra-pro"
        | "openai/gpt-5.6-luna" | "openai/gpt-5.6-luna-pro"
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Static capability tests ──────────────────────────────────────

    #[test]
    fn static_caps_unknown_model_returns_all_false() {
        let caps = resolve_static_model_capabilities("some/unknown-model");
        assert!(!caps.force_thinking);
        assert!(!caps.supports_image_url);
        assert!(!caps.supports_image_base64);
    }

    #[test]
    fn static_caps_hy3_force_thinking_is_false() {
        let caps = resolve_static_model_capabilities("tencent/hy3");
        assert!(!caps.force_thinking);
        let caps_free = resolve_static_model_capabilities("tencent/hy3:free");
        assert!(!caps_free.force_thinking);
    }

    #[test]
    fn static_caps_kimi_k3_reasoning_effort_is_config_driven() {
        let caps = resolve_static_model_capabilities("kimi-k3");
        assert!(caps.force_thinking);
        assert!(caps.suppress_thinking_parameter);
        // No longer forced — reasoning_effort comes from model config (low/high/max)
        assert_eq!(caps.forced_reasoning_effort, None);
    }

    #[test]
    fn static_caps_deepseek_v4_pro() {
        let caps = resolve_static_model_capabilities("deepseek-v4-pro");
        assert!(!caps.force_thinking);
        assert!(!caps.supports_image_url);
    }

    #[test]
    fn is_laguna_model_known_variants() {
        assert!(is_laguna_model("poolside/laguna-s-2.1"));
        assert!(is_laguna_model("poolside/laguna-s-2.1:free"));
        assert!(is_laguna_model("poolside/laguna-xs-2.1"));
        assert!(!is_laguna_model("poolside/something-else"));
        assert!(!is_laguna_model("tencent/hy3"));
    }

    #[test]
    fn is_hy3_model_known_variants() {
        assert!(is_hy3_model("tencent/hy3"));
        assert!(is_hy3_model("tencent/hy3:free"));
        assert!(!is_hy3_model("tencent/something-else"));
        assert!(!is_hy3_model("poolside/laguna-s-2.1"));
    }

    // ── Effective capability tests (3-state cache) ───────────────────

    fn make_cache_model(image: bool, video: bool) -> OpenRouterModel {
        let mut modalities = Vec::new();
        if image { modalities.push("image".to_string()); }
        if video { modalities.push("video".to_string()); }
        OpenRouterModel {
            id: "test/model".to_string(),
            canonical_slug: None,
            display_name: "Test Model".to_string(),
            description: None,
            context_length: None,
            max_completion_tokens: None,
            input_modalities: modalities,
            output_modalities: vec![],
            supported_parameters: vec![],
            pricing: Default::default(),
        }
    }

    #[test]
    fn effective_caps_cache_hit_matches_previous_inline() {
        // Cache Hit: image/video from cache, force_thinking from static
        let model = make_cache_model(true, true); // image+video in cache
        let caps = resolve_effective_model_capabilities(
            "poolside/laguna-s-2.1",
            false, // provider_supports_vision irrelevant for Hit
            OpenRouterCacheLookup::Hit(&model),
        );
        // Laguna S: static all-false (falls through wildcard), so force_thinking=false
        assert!(!caps.force_thinking);
        // Hit path: image/video from cache model modalities
        assert!(caps.supports_image_url);
        assert!(caps.supports_image_base64);
        assert!(caps.supports_video_url);
        assert!(caps.supports_video_base64);
    }

    #[test]
    fn effective_caps_cache_miss_matches_previous_inline() {
        // Cache Miss: image=true, video=false
        let caps = resolve_effective_model_capabilities(
            "poolside/laguna-s-2.1",
            true,
            OpenRouterCacheLookup::Miss,
        );
        assert!(!caps.force_thinking);
        assert!(caps.supports_image_url);
        assert!(caps.supports_image_base64);
        assert!(!caps.supports_video_url);
        assert!(!caps.supports_video_base64);
    }

    #[test]
    fn effective_caps_cache_unavailable_matches_previous_inline() {
        // Unavailable: image=provider_supports_vision, video=false
        let caps = resolve_effective_model_capabilities(
            "poolside/laguna-s-2.1",
            true,
            OpenRouterCacheLookup::Unavailable,
        );
        assert!(!caps.force_thinking);
        assert!(caps.supports_image_url);
        assert!(!caps.supports_video_url);
    }

    #[test]
    fn cached_reasoning_does_not_change_force_thinking() {
        // cache supported_parameters is never consulted for force_thinking
        let caps = resolve_effective_model_capabilities(
            "deepseek-v4-pro",
            true,
            OpenRouterCacheLookup::Miss, // no reasoning param in Miss path
        );
        assert!(!caps.force_thinking);
    }

    #[test]
    fn openrouter_route_uses_static_force_thinking() {
        // Kimi K3's static force_thinking=true flows through to effective caps
        let caps = resolve_effective_model_capabilities(
            "kimi-k3",
            true,
            OpenRouterCacheLookup::Unavailable,
        );
        assert!(caps.force_thinking);
        assert!(caps.suppress_thinking_parameter);
        // Kimi K3: reasoning_effort is config-driven, not forced
        assert_eq!(caps.forced_reasoning_effort, None);
    }

    #[test]
    fn hy3_hard_override_keeps_force_thinking_false() {
        // Hy3 always force_thinking=false, even if static somehow changed
        let caps = resolve_effective_model_capabilities(
            "tencent/hy3",
            true,
            OpenRouterCacheLookup::Miss,
        );
        assert!(!caps.force_thinking);

        let caps_ua = resolve_effective_model_capabilities(
            "tencent/hy3",
            true,
            OpenRouterCacheLookup::Unavailable,
        );
        assert!(!caps_ua.force_thinking);
    }

    // ── InclusionAI / StepFun tests ─────────────────────────────────

    #[test]
    fn static_caps_ring_force_thinking_true() {
        let caps = resolve_static_model_capabilities("inclusionai/ring-2.6-1t");
        assert!(caps.force_thinking);
        assert!(!caps.suppress_thinking_parameter);
        assert!(!caps.supports_image_url);
        assert!(!caps.supports_image_base64);
    }

    #[test]
    fn static_caps_step37_has_image_but_video_unverified() {
        let caps = resolve_static_model_capabilities("stepfun/step-3.7-flash");
        assert!(caps.force_thinking);
        assert!(caps.supports_image_url);
        assert!(caps.supports_image_base64);
        // Video: false until real-request verification passes
        assert!(!caps.supports_video_url);
        assert!(!caps.supports_video_base64);
    }

    #[test]
    fn static_caps_step35_no_vision() {
        let caps = resolve_static_model_capabilities("stepfun/step-3.5-flash");
        assert!(caps.force_thinking);
        assert!(!caps.supports_image_url);
        assert!(!caps.supports_image_base64);
        assert!(!caps.supports_video_url);
        assert!(!caps.supports_video_base64);
    }

    #[test]
    fn static_caps_ling_no_force_thinking() {
        let caps_1t = resolve_static_model_capabilities("inclusionai/ling-2.6-1t");
        assert!(!caps_1t.force_thinking);
        assert!(caps_1t.suppress_thinking_parameter);

        let caps_flash = resolve_static_model_capabilities("inclusionai/ling-2.6-flash");
        assert!(!caps_flash.force_thinking);
        assert!(caps_flash.suppress_thinking_parameter);

        let caps_free = resolve_static_model_capabilities("inclusionai/ling-3.0-flash:free");
        assert!(!caps_free.force_thinking);
        assert!(!caps_free.suppress_thinking_parameter);
    }

    #[test]
    fn is_inclusionai_model_known_variants() {
        assert!(is_inclusionai_model("inclusionai/ring-2.6-1t"));
        assert!(is_inclusionai_model("inclusionai/ling-2.6-1t"));
        assert!(is_inclusionai_model("inclusionai/ling-2.6-flash"));
        assert!(is_inclusionai_model("inclusionai/ling-3.0-flash:free"));
        assert!(!is_inclusionai_model("inclusionai/something-else"));
        assert!(!is_inclusionai_model("tencent/hy3"));
    }

    #[test]
    fn is_stepfun_model_known_variants() {
        assert!(is_stepfun_model("stepfun/step-3.7-flash"));
        assert!(is_stepfun_model("stepfun/step-3.5-flash"));
        assert!(!is_stepfun_model("stepfun/something-else"));
        assert!(!is_stepfun_model("inclusionai/ring-2.6-1t"));
    }

    #[test]
    fn is_ling_non_thinking_model_variants() {
        assert!(is_ling_non_thinking_model("inclusionai/ling-2.6-1t"));
        assert!(is_ling_non_thinking_model("inclusionai/ling-2.6-flash"));
        assert!(!is_ling_non_thinking_model("inclusionai/ling-3.0-flash:free"));
        assert!(!is_ling_non_thinking_model("inclusionai/ring-2.6-1t"));
    }

    #[test]
    fn is_ling_free_model_variants() {
        assert!(is_ling_free_model("inclusionai/ling-3.0-flash:free"));
        assert!(!is_ling_free_model("inclusionai/ling-2.6-1t"));
        assert!(!is_ling_free_model("inclusionai/ring-2.6-1t"));
    }

    // ── Google Gemini tests ──────────────────────────────────────────

    #[test]
    fn is_gemini_model_returns_true_for_supported_ids() {
        assert!(is_gemini_model("google/gemini-3.1-pro-preview"));
        assert!(is_gemini_model("google/gemini-3.7-flash"));
        assert!(is_gemini_model("google/gemini-3.5-flash-lite"));
    }

    #[test]
    fn is_gemini_model_returns_false_for_other_models() {
        assert!(!is_gemini_model("google/gemini-3.8-flash"));
        assert!(!is_gemini_model("openai/gpt-5.6-sol"));
        assert!(!is_gemini_model(""));
    }

    #[test]
    fn static_caps_gemini_supports_images_and_not_video() {
        for id in &[
            "google/gemini-3.1-pro-preview",
            "google/gemini-3.7-flash",
            "google/gemini-3.5-flash-lite",
        ] {
            let caps = resolve_static_model_capabilities(id);
            assert!(!caps.force_thinking, "force_thinking should be false for {}", id);
            assert!(!caps.suppress_thinking_parameter, "suppress_thinking should be false for {}", id);
            assert!(caps.supports_image_url, "supports_image_url should be true for {}", id);
            assert!(caps.supports_image_base64, "supports_image_base64 should be true for {}", id);
            assert!(!caps.supports_video_url, "supports_video_url should be false for {}", id);
            assert!(!caps.supports_video_base64, "supports_video_base64 should be false for {}", id);
        }
    }
    // ── OpenAI GPT-5.6 tests ────────────────────────────────────────

    #[test]
    fn is_openai_gpt56_model_returns_true_for_all_six_ids() {
        assert!(is_openai_gpt56_model("openai/gpt-5.6-sol"));
        assert!(is_openai_gpt56_model("openai/gpt-5.6-sol-pro"));
        assert!(is_openai_gpt56_model("openai/gpt-5.6-terra"));
        assert!(is_openai_gpt56_model("openai/gpt-5.6-terra-pro"));
        assert!(is_openai_gpt56_model("openai/gpt-5.6-luna"));
        assert!(is_openai_gpt56_model("openai/gpt-5.6-luna-pro"));
    }

    #[test]
    fn is_openai_gpt56_model_returns_false_for_other_models() {
        assert!(!is_openai_gpt56_model("openai/gpt-4o"));
        assert!(!is_openai_gpt56_model("poolside/laguna-s-2.1"));
        assert!(!is_openai_gpt56_model("tencent/hy3"));
        assert!(!is_openai_gpt56_model(""));
    }

    #[test]
    fn static_caps_openai_gpt56_all_variants() {
        for id in &[
            "openai/gpt-5.6-sol", "openai/gpt-5.6-sol-pro",
            "openai/gpt-5.6-terra", "openai/gpt-5.6-terra-pro",
            "openai/gpt-5.6-luna", "openai/gpt-5.6-luna-pro",
        ] {
            let caps = resolve_static_model_capabilities(id);
            assert!(!caps.force_thinking, "force_thinking should be false for {}", id);
            assert!(!caps.suppress_thinking_parameter, "suppress_thinking should be false for {}", id);
            assert!(caps.supports_image_url, "supports_image_url should be true for {}", id);
            assert!(caps.supports_image_base64, "supports_image_base64 should be true for {}", id);
            assert!(!caps.supports_video_url, "supports_video_url should be false for {}", id);
            assert!(!caps.supports_video_base64, "supports_video_base64 should be false for {}", id);
        }
    }

    #[test]
    fn openrouter_builtin_static_capabilities_match_expected() {
        struct ExpectedCaps {
            force_thinking: bool,
            supports_image_url: bool,
            supports_image_base64: bool,
            supports_video_url: bool,
            supports_video_base64: bool,
        }
        let cases: &[(&str, ExpectedCaps)] = &[
            ("poolside/laguna-s-2.1", ExpectedCaps {
                force_thinking: false, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("tencent/hy3", ExpectedCaps {
                force_thinking: false, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("inclusionai/ring-2.6-1t", ExpectedCaps {
                force_thinking: true, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("inclusionai/ling-2.6-1t", ExpectedCaps {
                force_thinking: false, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("inclusionai/ling-2.6-flash", ExpectedCaps {
                force_thinking: false, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("inclusionai/ling-3.0-flash:free", ExpectedCaps {
                force_thinking: false, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("stepfun/step-3.7-flash", ExpectedCaps {
                force_thinking: true, supports_image_url: true,
                supports_image_base64: true,
                supports_video_url: false, supports_video_base64: false,
            }),
            ("stepfun/step-3.5-flash", ExpectedCaps {
                force_thinking: true, supports_image_url: false,
                supports_image_base64: false,
                supports_video_url: false, supports_video_base64: false,
            }),
        ];
        for (model_id, expected) in cases {
            let caps = try_resolve_static_model_capabilities(model_id)
                .unwrap_or_else(|| panic!("No static caps for {}", model_id));
            assert_eq!(caps.force_thinking, expected.force_thinking,
                "force_thinking mismatch for {}", model_id);
            assert_eq!(caps.supports_image_url, expected.supports_image_url,
                "supports_image_url mismatch for {}", model_id);
            assert_eq!(caps.supports_image_base64, expected.supports_image_base64,
                "supports_image_base64 mismatch for {}", model_id);
            assert_eq!(caps.supports_video_url, expected.supports_video_url,
                "supports_video_url mismatch for {}", model_id);
            assert_eq!(caps.supports_video_base64, expected.supports_video_base64,
                "supports_video_base64 mismatch for {}", model_id);
        }
    }

    #[test]
    fn static_caps_deepseek_all_variants() {
        let pro = try_resolve_static_model_capabilities("deepseek-v4-pro").unwrap();
        assert!(!pro.supports_image_url);
        assert!(!pro.supports_image_base64);
        assert!(!pro.supports_video_url);
        assert!(!pro.supports_video_base64);
        assert!(!pro.force_thinking);

        let flash = try_resolve_static_model_capabilities("deepseek-v4-flash").unwrap();
        assert!(!flash.supports_image_url);
        assert!(!flash.supports_image_base64);
        assert!(!flash.supports_video_url);
        assert!(!flash.supports_video_base64);
        assert!(!flash.force_thinking);

        let vision = try_resolve_static_model_capabilities("deepseek-v4-flash-vision-exp").unwrap();
        assert!(vision.supports_image_url);
        assert!(vision.supports_image_base64);
        assert!(!vision.supports_video_url);
        assert!(!vision.supports_video_base64);
        assert!(!vision.force_thinking);
    }

    // ── Context-window metadata tests ─────────────────────────────────

    fn synthetic_file(entries: &[(&str, u64, ContextWindowSource)]) -> ModelContextWindowsFile {
        ModelContextWindowsFile {
            schema_version: 1,
            models: entries
                .iter()
                .map(|(key, len, source)| {
                    (
                        key.to_string(),
                        StaticContextWindow {
                            context_length: *len,
                            source: *source,
                            verified_at: None,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn context_window_provider_specific_beats_generic() {
        let file = synthetic_file(&[
            ("model-x", 1_000_000, ContextWindowSource::Official),
            ("openrouter:model-x", 200_000, ContextWindowSource::Builtin),
        ]);
        let hit = lookup_static_context_window(&file, "openrouter", "model-x").unwrap();
        assert_eq!(hit.context_length, 200_000);
        assert_eq!(hit.source, ContextWindowSource::Builtin);
    }

    #[test]
    fn context_window_generic_fallback_when_no_provider_key() {
        let file = synthetic_file(&[("model-x", 1_000_000, ContextWindowSource::Official)]);
        let hit = lookup_static_context_window(&file, "openrouter", "model-x").unwrap();
        assert_eq!(hit.context_length, 1_000_000);
        assert_eq!(hit.source, ContextWindowSource::Official);
    }

    #[test]
    fn context_window_unknown_model_returns_none() {
        let file = synthetic_file(&[("model-x", 1_000_000, ContextWindowSource::Official)]);
        assert!(lookup_static_context_window(&file, "openrouter", "no-such-model").is_none());
        // Generic key "model-x" resolves for ANY provider — an absent provider key
        // must fall through to the generic entry rather than resolve to None.
        assert_eq!(
            lookup_static_context_window(&file, "deepseek", "model-x")
                .map(|w| w.context_length),
            Some(1_000_000)
        );
    }

    #[test]
    fn embedded_json_deserializes_all_entries() {
        let file = context_window_file();
        assert_eq!(file.schema_version, 1);
        assert!(!file.models.is_empty(), "resource JSON must not be empty");
        for (key, entry) in &file.models {
            assert!(!key.is_empty(), "model key must not be empty");
            assert!(
                entry.context_length > 0,
                "context_length must be > 0 for {}",
                key
            );
            // Unknown is expressed by the entry being absent, never by an
            // explicit `source: "unknown"` entry in the embedded JSON.
            assert_ne!(
                entry.source,
                ContextWindowSource::Unknown,
                "embedded JSON must not register '{}' with source 'unknown'",
                key
            );
        }
    }

    #[test]
    fn embedded_json_known_model_values() {
        // DeepSeek official 1M
        let deepseek = try_resolve_static_context_window("deepseek", "deepseek-v4-flash").unwrap();
        assert_eq!(deepseek.context_length, 1_000_000);
        assert_eq!(deepseek.source, ContextWindowSource::Official);
        assert!(deepseek.verified_at.is_some());
        let ds_vision = try_resolve_static_context_window("deepseek", "deepseek-v4-flash-vision-exp").unwrap();
        assert_eq!(ds_vision.context_length, 1_000_000);
        assert_eq!(ds_vision.source, ContextWindowSource::Official);
        assert!(ds_vision.verified_at.is_some());

        // Generic openai/gpt-5.6-sol: official via generic-key fallback
        let gpt_sol = try_resolve_static_context_window("openrouter", "openai/gpt-5.6-sol").unwrap();
        assert_eq!(gpt_sol.context_length, 1_050_000);
        assert_eq!(gpt_sol.source, ContextWindowSource::Official);

        // OpenRouter-derived -pro: provider-specific builtin key
        let gpt_pro = try_resolve_static_context_window("openrouter", "openai/gpt-5.6-sol-pro").unwrap();
        assert_eq!(gpt_pro.context_length, 1_050_000);
        assert_eq!(gpt_pro.source, ContextWindowSource::Builtin);

        // MiniMax official 1M / 204.8K (provider-specific keys)
        let m3 = try_resolve_static_context_window("minimax", "MiniMax-M3").unwrap();
        assert_eq!(m3.context_length, 1_000_000);
        assert_eq!(m3.source, ContextWindowSource::Official);
        assert!(m3.verified_at.is_some());
        // MiniMax-M2.7 is not in the dropdown's PROVIDER_MODELS but is
        // referenced by the config model_map — lock it against removal.
        let m27 = try_resolve_static_context_window("minimax", "MiniMax-M2.7").unwrap();
        assert_eq!(m27.context_length, 204_800);
        assert_eq!(m27.source, ContextWindowSource::Official);
        assert!(m27.verified_at.is_some());
        assert_eq!(
            try_resolve_static_context_window("minimax", "MiniMax-M2.7-highspeed")
                .unwrap()
                .context_length,
            204_800
        );

        // Kimi official 1M (k3) and 262K (k2.7-code etc.)
        let k3 = try_resolve_static_context_window("kimi", "kimi-k3").unwrap();
        assert_eq!(k3.context_length, 1_048_576);
        assert_eq!(k3.source, ContextWindowSource::Official);
        assert!(k3.verified_at.is_some());
        assert_eq!(
            try_resolve_static_context_window("kimi", "kimi-k2.7-code-highspeed")
                .unwrap()
                .context_length,
            262_144
        );

        // MiMo official 1M for all three IDs
        let mimo = try_resolve_static_context_window("mimo", "mimo-v2.5-pro").unwrap();
        assert_eq!(mimo.context_length, 1_000_000);
        assert_eq!(mimo.source, ContextWindowSource::Official);
        assert!(mimo.verified_at.is_some());

        // Laguna corrected to 262K (provider-specific builtin key)
        let laguna = try_resolve_static_context_window("openrouter", "poolside/laguna-s-2.1").unwrap();
        assert_eq!(laguna.context_length, 262_144);
        assert_eq!(laguna.source, ContextWindowSource::Builtin);
    }

    #[test]
    fn legacy_openrouter_gemini_35_flash_lite_has_context_metadata() {
        // google/gemini-3.5-flash-lite is no longer offered as a new selection
        // (removed from the built-in UI model list), but its context metadata is
        // intentionally kept for backward compatibility: legacy saved profiles
        // may still route a slot to it, and context-management Auto must still
        // resolve its context length.
        let flash_lite =
            try_resolve_static_context_window("openrouter", "google/gemini-3.5-flash-lite")
                .unwrap();
        assert_eq!(flash_lite.context_length, 1_000_000);
        assert_eq!(flash_lite.source, ContextWindowSource::Builtin);
    }
}
