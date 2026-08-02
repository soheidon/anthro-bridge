// ---------------------------------------------------------------------------
// Shared route→upstream model resolution.
//
// The proxy (proxy.rs `resolve_proxy_config`) and the auto-compact resolver
// (lib.rs) must agree on which upstream model a route maps to, so both use the
// extractors in this module. The caller decides WHICH map is authoritative —
// the proxy via its typed `models`/`model_map` branch, this module's raw-JSON
// resolver via the `models`-object presence check — while `resolve_from_models`
// / `resolve_from_model_map` perform the actual lookup. Do NOT impose a new
// precedence here; it must stay faithful to what the proxy does, or route
// resolution will diverge.
// ---------------------------------------------------------------------------

/// The three canonical Gateway model routes used by Claude Code.
pub(crate) const CLAUDE_ROUTES: [&str; 3] =
    ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"];

/// Extract `models[route].upstream_model` from a scope (provider or profile).
///
/// Does NOT decide whether `models` is authoritative — the caller has already
/// selected it. Returns `None` when the scope has no `models` object or the
/// route is absent.
pub(crate) fn resolve_from_models(scope: &serde_json::Value, route: &str) -> Option<String> {
    scope
        .get("models")
        .and_then(|m| m.get(route))
        .and_then(|entry| entry.get("upstream_model"))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Extract `model_map[route]` from a scope (provider or profile).
///
/// Does NOT decide whether `model_map` is authoritative — the caller has
/// already selected it. Returns `None` when the scope has no `model_map` or
/// the route is absent.
pub(crate) fn resolve_from_model_map(scope: &serde_json::Value, route: &str) -> Option<String> {
    scope
        .get("model_map")
        .and_then(|m| m.get(route))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Resolve the upstream model a route maps to for one provider/profile.
///
/// Mirrors proxy.rs `resolve_proxy_config` exactly:
/// - `openrouter`: the active profile's `models[route].upstream_model` ONLY —
///   the proxy skips provider/profile `model_map` entirely when profiles exist.
///   When `profile_id` is `None`, the active profile (by
///   `active_openrouter_profile_id`) is used, falling back to the first profile
///   — the proxy's transient fallback.
/// - direct provider: exclusive selection — if the provider has a `models`
///   object, only `models[route].upstream_model` is consulted and `model_map`
///   is ignored (the proxy's `if let Some(models)` branch); otherwise the
///   legacy `model_map[route]` is used.
///
/// Returns `None` when the route is not routed by that target at all
/// ("route unset" — the proxy would have no entry either).
pub(crate) fn resolve_route_upstream_model(
    cfg: &serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
    route: &str,
) -> Option<String> {
    let providers = cfg.get("providers").and_then(|p| p.as_object())?;
    let provider = providers.get(provider_id)?;

    if provider_id == "openrouter" {
        let profiles = provider.get("profiles").and_then(|p| p.as_array())?;
        if profiles.is_empty() {
            return None;
        }
        let active_id = cfg
            .get("active_openrouter_profile_id")
            .and_then(|v| v.as_str());
        let profile = match profile_id {
            Some(pid) => profiles
                .iter()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(pid)),
            None => profiles
                .iter()
                .find(|p| p.get("id").and_then(|v| v.as_str()) == active_id)
                .or_else(|| profiles.first()),
        }?;
        return resolve_from_models(profile, route);
    }

    // Exclusive selection, matching the proxy: `models` present (as an object)
    // ⇒ models only; otherwise legacy model_map. `is_object` (not `is_some`)
    // treats `"models": null` as absent — the same as typed `Option` → `None`
    // taking the legacy model_map branch.
    if provider.get("models").map_or(false, serde_json::Value::is_object) {
        resolve_from_models(provider, route)
    } else {
        resolve_from_model_map(provider, route)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base_config() -> serde_json::Value {
        json!({
            "active_provider": "deepseek",
            "active_openrouter_profile_id": "prof-a",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-opus-5": { "upstream_model": "deepseek-v4-pro" },
                        "claude-sonnet-5": { "upstream_model": "deepseek-v4-flash" },
                        "claude-haiku-4-5": { "upstream_model": "deepseek-v4-flash" }
                    }
                },
                "openrouter": {
                    "profiles": [
                        {
                            "id": "prof-a",
                            "display_name": "OpenRouter: A",
                            "models": {
                                "claude-opus-5": { "upstream_model": "openai/gpt-5.6-sol" },
                                "claude-sonnet-5": { "upstream_model": "openai/gpt-5.6-terra" },
                                "claude-haiku-4-5": { "upstream_model": "openai/gpt-5.6-luna" }
                            }
                        },
                        {
                            "id": "prof-b",
                            "display_name": "OpenRouter: B",
                            "models": {
                                "claude-opus-5": { "upstream_model": "tencent/hy3" }
                            }
                        }
                    ]
                },
                "legacy": {
                    "model_map": {
                        "claude-opus-5": "some-legacy-model"
                    }
                }
            }
        })
    }

    #[test]
    fn direct_provider_models_explicit_used() {
        let cfg = base_config();
        assert_eq!(
            resolve_route_upstream_model(&cfg, "deepseek", None, "claude-opus-5"),
            Some("deepseek-v4-pro".to_string())
        );
        assert_eq!(
            resolve_route_upstream_model(&cfg, "deepseek", None, "claude-haiku-4-5"),
            Some("deepseek-v4-flash".to_string())
        );
    }

    #[test]
    fn direct_provider_model_map_fallback() {
        let cfg = base_config();
        assert_eq!(
            resolve_route_upstream_model(&cfg, "legacy", None, "claude-opus-5"),
            Some("some-legacy-model".to_string())
        );
    }

    #[test]
    fn openrouter_active_profile_by_id() {
        let cfg = base_config();
        assert_eq!(
            resolve_route_upstream_model(&cfg, "openrouter", None, "claude-opus-5"),
            Some("openai/gpt-5.6-sol".to_string())
        );
    }

    #[test]
    fn openrouter_explicit_profile_id() {
        let cfg = base_config();
        assert_eq!(
            resolve_route_upstream_model(&cfg, "openrouter", Some("prof-b"), "claude-opus-5"),
            Some("tencent/hy3".to_string())
        );
        assert_eq!(
            resolve_route_upstream_model(&cfg, "openrouter", Some("prof-b"), "claude-sonnet-5"),
            None
        );
    }

    #[test]
    fn openrouter_missing_profile_falls_back_to_first() {
        let mut cfg = base_config();
        cfg["active_openrouter_profile_id"] = json!("no-such-profile");
        assert_eq!(
            resolve_route_upstream_model(&cfg, "openrouter", None, "claude-opus-5"),
            Some("openai/gpt-5.6-sol".to_string())
        );
    }

    #[test]
    fn openrouter_no_profiles_returns_none() {
        let cfg = json!({
            "providers": { "openrouter": { "profiles": [] } }
        });
        assert_eq!(resolve_route_upstream_model(&cfg, "openrouter", None, "claude-opus-5"), None);
    }

    #[test]
    fn unknown_provider_or_route_returns_none() {
        let cfg = base_config();
        assert_eq!(resolve_route_upstream_model(&cfg, "no-such-provider", None, "claude-opus-5"), None);
        assert_eq!(resolve_route_upstream_model(&cfg, "deepseek", None, "claude-unknown-route"), None);
    }

    #[test]
    fn real_claude_route_names_resolve() {
        // The 3 canonical routes used by the resolver.
        let cfg = base_config();
        for route in CLAUDE_ROUTES {
            assert!(
                resolve_route_upstream_model(&cfg, "deepseek", None, route).is_some(),
                "route {} must resolve",
                route
            );
        }
    }

    #[test]
    fn direct_models_present_ignores_model_map() {
        // `models` present but lacking the route + `model_map` has it → None.
        // This is the exclusive selection (proxy behavior): once `models`
        // exists, `model_map` is never consulted per-key.
        let cfg = json!({
            "providers": {
                "mixed": {
                    "models": {
                        "claude-sonnet-5": { "upstream_model": "model-a" },
                        "claude-haiku-4-5": { "upstream_model": "model-b" }
                    },
                    "model_map": { "claude-opus-5": "model-c" }
                }
            }
        });
        assert_eq!(
            resolve_route_upstream_model(&cfg, "mixed", None, "claude-opus-5"),
            None
        );
        assert_eq!(
            resolve_route_upstream_model(&cfg, "mixed", None, "claude-sonnet-5"),
            Some("model-a".to_string())
        );
    }

    #[test]
    fn direct_empty_models_present_ignores_model_map() {
        // `models: {}` is "present" — the proxy enters the models branch and
        // iterates nothing, so model_map must NOT be used as a fallback.
        let cfg = json!({
            "providers": {
                "mixed": {
                    "models": {},
                    "model_map": { "claude-opus-5": "model-c" }
                }
            }
        });
        assert_eq!(
            resolve_route_upstream_model(&cfg, "mixed", None, "claude-opus-5"),
            None
        );
    }

    #[test]
    fn direct_models_null_uses_model_map() {
        // `"models": null` is treated as absent (typed `Option` → `None`),
        // so the legacy model_map branch is used.
        let cfg = json!({
            "providers": {
                "mixed": {
                    "models": null,
                    "model_map": { "claude-opus-5": "model-c" }
                }
            }
        });
        assert_eq!(
            resolve_route_upstream_model(&cfg, "mixed", None, "claude-opus-5"),
            Some("model-c".to_string())
        );
    }

    #[test]
    fn openrouter_profile_models_only_ignores_model_map() {
        // A profile without `models` must NOT fall back to its own `model_map`
        // (the proxy routes only from the active profile's `models`).
        let cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [
                        {
                            "id": "prof-x",
                            "display_name": "X",
                            "model_map": { "claude-opus-5": "some-model" }
                        }
                    ]
                }
            }
        });
        assert_eq!(
            resolve_route_upstream_model(&cfg, "openrouter", None, "claude-opus-5"),
            None
        );
    }

    #[test]
    fn resolve_from_models_and_model_map_extract_only() {
        let scope = json!({
            "models": { "claude-opus-5": { "upstream_model": "model-a" } },
            "model_map": { "claude-opus-5": "model-b", "claude-sonnet-5": "model-c" }
        });
        // resolve_from_models never consults model_map.
        assert_eq!(resolve_from_models(&scope, "claude-opus-5"), Some("model-a".to_string()));
        assert_eq!(resolve_from_models(&scope, "claude-sonnet-5"), None);
        // resolve_from_model_map never consults models.
        assert_eq!(resolve_from_model_map(&scope, "claude-opus-5"), Some("model-b".to_string()));
        assert_eq!(resolve_from_model_map(&scope, "claude-sonnet-5"), Some("model-c".to_string()));
        // Missing maps / routes → None.
        assert_eq!(resolve_from_models(&scope, "claude-unknown"), None);
        let no_maps = json!({});
        assert_eq!(resolve_from_models(&no_maps, "claude-opus-5"), None);
        assert_eq!(resolve_from_model_map(&no_maps, "claude-opus-5"), None);
    }
}
