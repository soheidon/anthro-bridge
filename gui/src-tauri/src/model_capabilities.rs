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
}
