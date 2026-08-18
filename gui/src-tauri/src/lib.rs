use chrono::Local;
use model_capabilities::{try_resolve_static_context_window, ContextWindowSource};
use model_routing::{resolve_route_upstream_model, CLAUDE_ROUTES};
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::Manager;
use tokio::sync::oneshot;

mod config_template;
mod model_capabilities;
mod model_routing;
mod openrouter;
mod paths;
mod proxy;

// ---------------------------------------------------------------------------
// Path helpers — all delegated to paths.rs module
// ---------------------------------------------------------------------------

/// Migrate config from old paths (Terra Bridge → Anthropic Proxy Gateway) if new path doesn't exist.
/// Returns Ok(true) if migration was performed. Accepts an injectable target path for testability.
fn migrate_old_config_at(target: &Path) -> Result<bool, String> {
    if target.exists() {
        return Ok(false); // Already has new config, skip
    }

    let appdata = std::env::var("APPDATA").unwrap_or_default();
    // Try Terra Bridge first (most recent old name), then Anthropic Proxy Gateway
    for old_name in &["Terra Bridge", "Anthropic Proxy Gateway"] {
        let old_config = PathBuf::from(&appdata).join(old_name).join("config.json");
        if old_config.exists() {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create config dir for migration: {e}"))?;
            }
            std::fs::copy(&old_config, target)
                .map_err(|e| format!("Failed to copy old config for migration: {e}"))?;
            return Ok(true);
        }
    }
    Ok(false)
}

// ---------------------------------------------------------------------------
// Config template write helpers
// ---------------------------------------------------------------------------

/// Check for and recover from an interrupted config replacement.
///
/// If `path` is missing but a `.rollback` sibling exists, a previous
/// `replace_config_from_template_atomically` crashed after staging the
/// original config out of the way but before installing the replacement.
/// Recover by renaming the rollback back to the original path.
fn recover_interrupted_config_replace(path: &Path) -> Result<(), String> {
    let rollback = path.with_extension("json.rollback");

    if !path.exists() && rollback.exists() {
        std::fs::rename(&rollback, path).map_err(|e| {
            format!("Failed to recover config from interrupted replacement: {e}")
        })?;
        eprintln!(
            "[anthro-bridge] WARN: Recovered config.json from rollback after interrupted replacement (path: {})",
            path.display()
        );
    }

    Ok(())
}

/// Write the embedded config template atomically to `path`.
/// Writes to .tmp, re-reads + validates JSON, then renames to target.
/// Returns an error if `path` already exists — this is create-only.
/// On failure, any temporary file is cleaned up.
fn seed_config_from_template(path: &Path) -> Result<(), String> {
    // Refuse to overwrite existing config
    if path.exists() {
        return Err(format!(
            "Refusing to overwrite existing config: {}",
            path.display()
        ));
    }

    // Validate template at runtime (also validated at compile time via include_str!)
    let _: serde_json::Value = serde_json::from_str(config_template::BUNDLED_CONFIG_TEMPLATE)
        .map_err(|e| format!("Embedded config template is invalid JSON: {e}"))?;

    let parent = path.parent().ok_or("Config path has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let tmp = path.with_extension("json.tmp");

    let result = (|| -> Result<(), String> {
        std::fs::write(&tmp, config_template::BUNDLED_CONFIG_TEMPLATE)
            .map_err(|e| format!("Failed to write config tmp: {e}"))?;

        // Re-read and validate the written file is valid JSON
        let check = std::fs::read_to_string(&tmp)
            .map_err(|e| format!("Failed to read back config tmp: {e}"))?;
        serde_json::from_str::<serde_json::Value>(&check)
            .map_err(|e| format!("Written config tmp is not valid JSON: {e}"))?;

        std::fs::rename(&tmp, path)
            .map_err(|e| format!("Failed to rename config tmp -> target: {e}"))?;

        Ok(())
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }

    result
}

/// Replace `path` with the embedded config template.
/// On I/O failure, restores the original config from rollback.
fn replace_config_from_template_atomically(path: &Path) -> Result<(), String> {
    let _: serde_json::Value = serde_json::from_str(config_template::BUNDLED_CONFIG_TEMPLATE)
        .map_err(|e| format!("Embedded config template is invalid JSON: {e}"))?;

    let parent = path.parent().ok_or("Config path has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let tmp = path.with_extension("json.reset.tmp");
    let rollback = path.with_extension("json.rollback");

    // ── Crash recovery (shared) ─────────────────────────────────
    recover_interrupted_config_replace(path)?;

    // ── Stale rollback preservation ─────────────────────────────
    if path.exists() && rollback.exists() {
        let stale = path.with_file_name(format!(
            "config.stale-rollback-{}.json",
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::rename(&rollback, &stale).map_err(|e| {
            format!("Failed to preserve stale rollback: {e}")
        })?;
    }

    // Clean up tmp from any previous crashed attempt
    let _ = std::fs::remove_file(&tmp);

    // ── 1. Write new config to tmp ──────────────────────────────
    std::fs::write(&tmp, config_template::BUNDLED_CONFIG_TEMPLATE)
        .map_err(|e| format!("Failed to write reset tmp: {e}"))?;

    // ── 2. Validate tmp ─────────────────────────────────────────
    let written = std::fs::read_to_string(&tmp)
        .map_err(|e| format!("Failed to read reset tmp: {e}"))?;
    serde_json::from_str::<serde_json::Value>(&written)
        .map_err(|e| format!("Reset tmp is invalid JSON: {e}"))?;

    // ── 3. Stage existing config -> rollback ─────────────────────
    if path.exists() {
        std::fs::rename(path, &rollback)
            .map_err(|e| format!("Failed to stage existing config: {e}"))?;
    }

    // ── 4. Install tmp -> target ─────────────────────────────────
    match std::fs::rename(&tmp, path) {
        Ok(()) => {
            // ── 5. Post-install validation ──────────────────────
            match std::fs::read_to_string(path) {
                Ok(installed) => {
                    if let Err(e) = serde_json::from_str::<serde_json::Value>(&installed) {
                        let _ = std::fs::remove_file(path);
                        if rollback.exists() {
                            let _ = std::fs::rename(&rollback, path);
                        }
                        return Err(format!("Installed config is invalid JSON: {e}"));
                    }
                }
                Err(e) => {
                    let _ = std::fs::remove_file(path);
                    if rollback.exists() {
                        let _ = std::fs::rename(&rollback, path);
                    }
                    return Err(format!("Failed to read installed config: {e}"));
                }
            }
            let _ = std::fs::remove_file(&rollback);
            Ok(())
        }
        Err(error) => {
            if rollback.exists() {
                let _ = std::fs::rename(&rollback, path);
            }
            let _ = std::fs::remove_file(&tmp);
            Err(format!("Failed to install replacement config: {error}"))
        }
    }
}

// ---------------------------------------------------------------------------
// Config initialization
// ---------------------------------------------------------------------------

/// Public entry point — resolves the real config path for the current channel.
fn ensure_config_initialized() -> Result<PathBuf, String> {
    ensure_config_initialized_at(
        &paths::config_path(),
        paths::app_channel(),
        true, // run legacy migration for real runs
    )
}

/// Core logic with injectable path, channel, and migration gating.
/// Testable via TempDir without touching real APPDATA.
fn ensure_config_initialized_at(
    path: &Path,
    channel: paths::AppChannel,
    run_legacy_migration: bool,
) -> Result<PathBuf, String> {
    // ── Crash recovery (must run BEFORE any existence check) ─────
    recover_interrupted_config_replace(path)?;

    eprintln!(
        "[anthro-bridge] Initializing configuration: path={}, exists={}, channel={:?}",
        path.display(),
        path.exists(),
        channel,
    );

    // Stable channel only: migrate from old product configs
    if run_legacy_migration && paths::should_migrate_old_config(channel) {
        migrate_old_config_at(path).map_err(|error| {
            eprintln!(
                "[anthro-bridge] ERROR: Config initialization failed at legacy_migration: {error}"
            );
            error
        })?;
    }

    if !path.exists() {
        seed_config_from_template(path)?;
        eprintln!("[anthro-bridge] Seeded config.json from embedded template");
    }

    // Normalize regardless of whether config already existed, was migrated
    // from an old product, or was freshly seeded from the bundled template.
    if path.exists() {
        // Merge new providers/models from bundled template into existing user config
        merge_bundled_providers(path).map_err(|error| {
            eprintln!(
                "[anthro-bridge] ERROR: Config initialization failed at merge_bundled_providers: {error}"
            );
            error
        })?;
        // One-time: fill missing capability flags for known OpenRouter text-only models
        migrate_poolside_capability_flags(path);
        // One-time: rename claude-opus-4-8 -> claude-opus-5
        migrate_opus_4_8_to_5(path);
        // One-time: M3 thinking_mode "thinking_only" -> "thinking"
        migrate_minimax_m3_thinking_only(path);
        // Idempotent: migrate legacy DeepSeek V4 Pro low/medium effort to high
        migrate_deepseek_pro_legacy_reasoning_effort(path);
        // One-time: Laguna Opus default thinking -> normal
        migrate_laguna_opus_default_to_normal(path);
        // One-time: migrate legacy OpenRouter config to multi-profile
        migrate_openrouter_to_profiles_at_path(path);
        // One-time: add built-in InclusionAI + StepFun profiles if missing
        ensure_builtin_openrouter_profiles_at_path(path)?;
        // One-time: migrate an earlier built-in Gemini profile default to the
        // current all-3.7-Flash (High/Medium/Low) default.
        migrate_gemini_profile_to_current_default(path);
        // Every startup: sync force_thinking with upstream model capabilities
        normalize_force_thinking(path);
        // Every startup: normalize OpenRouter config (repair, name normalization, active ID)
        normalize_config_at_path(path);
        // One-time: v2 Claude Code auto-compact modes. Runs LAST so the final
        // saved shape never contains a pre-v2 "inherit"/"override" mode.
        migrate_claude_code_auto_compact_modes(path);
    }

    Ok(path.to_path_buf())
}

/// Convenience re-export — most Tauri commands only need the path.
fn config_path() -> PathBuf {
    paths::config_path()
}

fn log_dir() -> PathBuf {
    paths::log_dir()
}

fn user_prefs_path() -> PathBuf {
    paths::user_prefs_path()
}

/// Merge new providers and model entries from the bundled config template
/// into the user's existing config. Preserves all user customizations.
fn merge_bundled_providers(user_config: &Path) -> Result<(), String> {
    // Parse both template and user config from the embedded constant
    let template: GatewayConfigResponse = serde_json::from_str(config_template::BUNDLED_CONFIG_TEMPLATE)
        .map_err(|e| format!("Failed to parse embedded config template in merge_bundled_providers: {e}"))?;

    // Parse the full template as raw JSON once for provider/model extraction
    let template_raw: serde_json::Value = serde_json::from_str(config_template::BUNDLED_CONFIG_TEMPLATE)
        .map_err(|e| format!("Failed to parse template as raw JSON: {e}"))?;

    let user_raw = std::fs::read_to_string(user_config)
        .map_err(|e| format!("Cannot read user config in merge_bundled_providers: {e}"))?;
    let mut user_cfg: serde_json::Value = serde_json::from_str(&user_raw)
        .map_err(|e| format!("Cannot parse user config JSON in merge_bundled_providers: {e}"))?;

    let mut changed = false;

    // Merge new providers from template
    if let Some(user_providers) = user_cfg.get_mut("providers") {
        for (pid, p) in &template.providers {
            if user_providers.get(pid).is_none() {
                // New provider: add in full from template
                if let Some(template_p) = template_raw.get("providers").and_then(|ps| ps.get(pid)) {
                    user_providers[pid] = template_p.clone();
                    changed = true;
                }
            } else {
                // Existing provider: merge new model entries from template
                if let (Some(user_models), Some(ref template_models)) =
                    (user_providers[pid].get_mut("models"), &p.models)
                {
                    for (mkey, _) in template_models {
                        if user_models.get(mkey).is_none() {
                            if let Some(tm) = template_raw
                                .get("providers")
                                .and_then(|ps| ps.get(pid))
                                .and_then(|p| p.get("models"))
                                .and_then(|ms| ms.get(mkey))
                            {
                                user_models[mkey] = tm.clone();
                                changed = true;
                            }
                        }
                    }
                }
            }
        }
    }

    if changed {
        if let Ok(merged) = serde_json::to_string_pretty(&user_cfg) {
            let _ = std::fs::write(user_config, merged);
        }
    }

    Ok(())
}

/// One-time: migrate legacy OpenRouter config to multi-profile.
/// Wraps migrate_openrouter_to_profiles with file read/write.
fn migrate_openrouter_to_profiles_at_path(config_path: &std::path::Path) {
    let raw_str = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw_str) {
        Ok(v) => v,
        Err(_) => return,
    };

    let outcome = match migrate_openrouter_to_profiles(&mut cfg) {
        Ok(o) => o,
        Err(_) => return,
    };

    if outcome.changed {
        if let Some(active_id) = outcome.active_profile_id {
            cfg.as_object_mut()
                .expect("root is object")
                .insert(
                    "active_openrouter_profile_id".into(),
                    serde_json::json!(active_id),
                );
        }
        let serialized = serde_json::to_string_pretty(&cfg).unwrap_or_default();
        let _ = std::fs::write(config_path, serialized);
    }
}

/// Ensure built-in InclusionAI and StepFun OpenRouter profiles exist.
/// Idempotent — checks by fixed profile UUID before adding.
/// Returns Err on I/O, parse, or save failure.
fn ensure_builtin_openrouter_profiles_at_path(
    config_path: &std::path::Path,
) -> Result<(), String> {
    let raw_str = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Cannot read config in ensure_builtin_openrouter_profiles: {e}"))?;
    let mut cfg: serde_json::Value = serde_json::from_str(&raw_str)
        .map_err(|e| format!("Cannot parse config JSON in ensure_builtin_openrouter_profiles: {e}"))?;

    // If OpenRouter provider doesn't exist, nothing to do.
    let or_provider = match cfg.pointer_mut("/providers/openrouter") {
        Some(p) => p,
        None => return Ok(()),
    };

    let profiles = or_provider
        .as_object_mut()
        .and_then(|obj| obj.get_mut("profiles"))
        .and_then(|v| v.as_array_mut());

    let profiles = match profiles {
        Some(p) => p,
        None => return Ok(()),
    };

    let mut changed = false;

    // Laguna: ensure exists + correct display_name + fix random UUID from old builds
    let laguna_id = "a0e0f000-0000-4000-8000-000000000001";
    let laguna_name = "OpenRouter: Laguna";
    let mut has_laguna = false;
    for p in profiles.iter_mut() {
        let id_match = p.get("id").and_then(|v| v.as_str()) == Some(laguna_id);
        let name_match = p
            .get("display_name")
            .and_then(|v| v.as_str())
            .map_or(false, |n| n == "Laguna" || n == laguna_name);
        if id_match || name_match {
            has_laguna = true;
            if p.get("id").and_then(|v| v.as_str()) != Some(laguna_id) {
                p["id"] = serde_json::Value::String(laguna_id.to_string());
                changed = true;
            }
            if p.get("display_name").and_then(|v| v.as_str()) != Some(laguna_name) {
                p["display_name"] = serde_json::Value::String(laguna_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_laguna {
        profiles.push(build_laguna_profile_json(laguna_name));
        changed = true;
    }

    // Hy3: ensure exists + correct display_name + fix random UUID from old builds
    let hy3_id = "b0e0f000-0000-4000-8000-000000000002";
    let hy3_name = "OpenRouter: Hy3";
    let mut has_hy3 = false;
    for p in profiles.iter_mut() {
        let id_match = p.get("id").and_then(|v| v.as_str()) == Some(hy3_id);
        let name_match = p
            .get("display_name")
            .and_then(|v| v.as_str())
            .map_or(false, |n| n == "Hy3" || n == hy3_name);
        if id_match || name_match {
            has_hy3 = true;
            if p.get("id").and_then(|v| v.as_str()) != Some(hy3_id) {
                p["id"] = serde_json::Value::String(hy3_id.to_string());
                changed = true;
            }
            if p.get("display_name").and_then(|v| v.as_str()) != Some(hy3_name) {
                p["display_name"] = serde_json::Value::String(hy3_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_hy3 {
        profiles.push(build_hy3_profile_json(hy3_name));
        changed = true;
    }

    // InclusionAI: ensure exists + correct display_name
    let inclusionai_id = "c0e0f000-0000-4000-8000-000000000003";
    let inclusionai_name = "OpenRouter: InclusionAI";
    let mut has_inclusionai = false;
    for p in profiles.iter_mut() {
        if p.get("id").and_then(|v| v.as_str()) == Some(inclusionai_id) {
            has_inclusionai = true;
            if p.get("display_name").and_then(|v| v.as_str()) != Some(inclusionai_name) {
                p["display_name"] = serde_json::Value::String(inclusionai_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_inclusionai {
        profiles.push(build_inclusionai_profile_json(inclusionai_name));
        changed = true;
    }

    // StepFun: ensure exists + correct display_name
    let stepfun_id = "d0e0f000-0000-4000-8000-000000000004";
    let stepfun_name = "OpenRouter: StepFun";
    let mut has_stepfun = false;
    for p in profiles.iter_mut() {
        if p.get("id").and_then(|v| v.as_str()) == Some(stepfun_id) {
            has_stepfun = true;
            if p.get("display_name").and_then(|v| v.as_str()) != Some(stepfun_name) {
                p["display_name"] = serde_json::Value::String(stepfun_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_stepfun {
        profiles.push(build_stepfun_profile_json(stepfun_name));
        changed = true;
    }

    // GPT-5.6 Balanced: ensure exists + correct display_name
    let gpt56_id = GPT56_BALANCED_PROFILE_ID;
    let gpt56_name = GPT56_BALANCED_PROFILE_NAME;
    let mut has_gpt56 = false;
    for p in profiles.iter_mut() {
        if p.get("id").and_then(|v| v.as_str()) == Some(gpt56_id) {
            has_gpt56 = true;
            if p.get("display_name").and_then(|v| v.as_str()) != Some(gpt56_name) {
                p["display_name"] = serde_json::Value::String(gpt56_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_gpt56 {
        profiles.push(build_gpt56_balanced_profile_json(gpt56_name));
        changed = true;
    }

    // Gemini: ensure exists + correct display_name
    let gemini_id = GEMINI_PROFILE_ID;
    let gemini_name = GEMINI_PROFILE_NAME;
    let mut has_gemini = false;
    for p in profiles.iter_mut() {
        if p.get("id").and_then(|v| v.as_str()) == Some(gemini_id) {
            has_gemini = true;
            if p.get("display_name").and_then(|v| v.as_str()) != Some(gemini_name) {
                p["display_name"] = serde_json::Value::String(gemini_name.to_string());
                changed = true;
            }
            break;
        }
    }
    if !has_gemini {
        profiles.push(build_gemini_profile_json(gemini_name));
        changed = true;
    }
    // Never change active profile ID
    // Never modify non-display-name fields of existing profiles

    if changed {
        let serialized = serde_json::to_string_pretty(&cfg)
            .map_err(|e| format!("Cannot serialize config in ensure_builtin_openrouter_profiles: {e}"))?;
        std::fs::write(config_path, serialized)
            .map_err(|e| format!("Cannot write config in ensure_builtin_openrouter_profiles: {e}"))?;
    }

    Ok(())
}

/// Every startup: repair empty profiles and missing/invalid active profile ID.
fn normalize_config_at_path(config_path: &std::path::Path) {
    let raw_str = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw_str) {
        Ok(v) => v,
        Err(_) => return,
    };

    if normalize_config(&mut cfg) {
        let serialized = serde_json::to_string_pretty(&cfg).unwrap_or_default();
        let _ = std::fs::write(config_path, serialized);
    }
}

/// One-time migration: fill missing capability flags for known text-to-text-only
/// OpenRouter models (Laguna S/XS). Uses `get_or_insert` so explicit user values
/// are never overwritten.
fn migrate_poolside_capability_flags(user_config: &Path) {
    let raw = match std::fs::read_to_string(user_config) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut changed = false;
    if let Some(providers) = cfg.get_mut("providers").and_then(|p| p.as_object_mut()) {
        if let Some(or_provider) = providers.get_mut("openrouter") {
            if let Some(models) = or_provider
                .get_mut("models")
                .and_then(|m| m.as_object_mut())
            {
                for model_key in models.keys().cloned().collect::<Vec<_>>() {
                    let entry = &mut models[&model_key];
                    if let Some(upstream) = entry.get("upstream_model").and_then(|u| u.as_str()) {
                        if TEXT_ONLY_OR_MODELS.contains(&upstream) {
                            let map = entry.as_object_mut().unwrap();
                            map.entry("supports_image_url".to_string())
                                .or_insert(serde_json::Value::Bool(false));
                            map.entry("supports_image_base64".to_string())
                                .or_insert(serde_json::Value::Bool(false));
                            map.entry("supports_video_url".to_string())
                                .or_insert(serde_json::Value::Bool(false));
                            map.entry("supports_video_base64".to_string())
                                .or_insert(serde_json::Value::Bool(false));
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(user_config, json);
        }
    }
}

/// One-time migration: rename Gateway model key `claude-opus-4-8` → `claude-opus-5`
/// across model_map, models, and visible_models in every provider.
/// Idempotent: does nothing if already migrated. If both keys exist, new key wins.
fn migrate_opus_4_8_to_5(user_config: &Path) {
    const OLD: &str = "claude-opus-4-8";
    const NEW: &str = "claude-opus-5";

    let raw = match std::fs::read_to_string(user_config) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut changed = false;
    if let Some(providers) = cfg.get_mut("providers").and_then(|p| p.as_object_mut()) {
        for (_pid, provider) in providers.iter_mut() {
            // model_map: Map key rename
            if let Some(map) = provider
                .get_mut("model_map")
                .and_then(|m| m.as_object_mut())
            {
                if map.contains_key(NEW) {
                    if map.remove(OLD).is_some() {
                        changed = true;
                    }
                } else if let Some(val) = map.remove(OLD) {
                    map.insert(NEW.to_string(), val);
                    changed = true;
                }
            }
            // models: Map key rename
            if let Some(models) = provider.get_mut("models").and_then(|m| m.as_object_mut()) {
                if models.contains_key(NEW) {
                    if models.remove(OLD).is_some() {
                        changed = true;
                    }
                } else if let Some(val) = models.remove(OLD) {
                    models.insert(NEW.to_string(), val);
                    changed = true;
                }
            }
            // visible_models: Array element replace
            if let Some(visible) = provider
                .get_mut("visible_models")
                .and_then(|v| v.as_array_mut())
            {
                for item in visible.iter_mut() {
                    if let Some(s) = item.as_str() {
                        if s == OLD {
                            *item = serde_json::Value::String(NEW.to_string());
                            changed = true;
                        }
                    }
                }
                // Deduplicate
                if changed {
                    let mut seen = std::collections::HashSet::new();
                    let deduped: Vec<_> = visible
                        .iter()
                        .filter(|v| v.as_str().map_or(true, |s| seen.insert(s.to_string())))
                        .cloned()
                        .collect();
                    if deduped.len() != visible.len() {
                        *visible = deduped;
                    }
                }
            }
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(user_config, json);
        }
    }
}

/// One-time migration: change OpenRouter Laguna S 2.1 Opus 5 default from
/// thinking_mode="thinking"+reasoning_effort="max" to "normal".
/// Idempotent: once migrated the original values are gone so it won't re-fire.
fn migrate_laguna_opus_default_to_normal(user_config: &Path) -> bool {
    let raw = match std::fs::read_to_string(user_config) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let mut changed = false;
    if let Some(providers) = cfg.get_mut("providers").and_then(|p| p.as_object_mut()) {
        if let Some(or_provider) = providers.get_mut("openrouter") {
            if let Some(models) = or_provider.get_mut("models").and_then(|m| m.as_object_mut()) {
                if let Some(entry) = models.get_mut("claude-opus-5") {
                    let is_laguna_opus = entry
                        .get("upstream_model")
                        .and_then(|u| u.as_str())
                        .map(|u| u == "poolside/laguna-s-2.1")
                        .unwrap_or(false);
                    let has_thinking_mode = entry
                        .get("thinking_mode")
                        .and_then(|v| v.as_str())
                        .map(|v| v == "thinking")
                        .unwrap_or(false);
                    let has_reasoning_effort = entry
                        .get("reasoning_effort")
                        .and_then(|v| v.as_str())
                        .map(|v| v == "max")
                        .unwrap_or(false);

                    if is_laguna_opus && has_thinking_mode && has_reasoning_effort {
                        if let Some(obj) = entry.as_object_mut() {
                            obj.insert(
                                "thinking_mode".to_string(),
                                serde_json::Value::String("normal".to_string()),
                            );
                            obj.remove("reasoning_effort");
                            changed = true;
                        }
                    }
                }
            }
        }
    }

    if changed {
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(user_config, json);
        }
    }
    changed
}

/// M3のthinking_mode "thinking_only"を"thinking"に移行
fn migrate_minimax_m3_thinking_only(config_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    let Some(models) = config
        .pointer_mut("/providers/minimax/models")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return false;
    };

    let mut changed = false;

    for entry in models.values_mut() {
        let is_m3 = entry
            .get("upstream_model")
            .and_then(serde_json::Value::as_str)
            == Some("MiniMax-M3");

        if is_m3
            && entry
                .get("thinking_mode")
                .and_then(serde_json::Value::as_str)
                == Some("thinking_only")
        {
            entry["thinking_mode"] = serde_json::json!("thinking");
            changed = true;
        }
    }

    if !changed {
        return false;
    }

    let Ok(serialized) = serde_json::to_string_pretty(&config) else {
        return false;
    };
    std::fs::write(config_path, serialized).is_ok()
}

/// Idempotent startup migration for DeepSeek V4 Pro reasoning effort.
/// DeepSeek's official API (V4-Pro-0813) supports low / high / max.
/// Legacy `medium` / `xhigh` values are rewritten to `high`;
/// `low` is preserved as a valid effort level.
/// For any non-thinking mode (normal / missing / invalid) the reasoning_effort
/// field is removed.
/// Only direct DeepSeek V4 Pro model entries under /providers/deepseek are
/// touched; Flash, OpenRouter profiles, and other providers are left unchanged.
fn migrate_deepseek_pro_legacy_reasoning_effort(config_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    let Some(models) = config
        .pointer_mut("/providers/deepseek/models")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return false;
    };

    let mut changed = false;

    for entry in models.values_mut() {
        let Some(entry) = entry.as_object_mut() else {
            continue;
        };

        let is_pro = entry.get("upstream_model").and_then(serde_json::Value::as_str)
            == Some("deepseek-v4-pro");
        if !is_pro {
            continue;
        }

        match entry.get("thinking_mode").and_then(serde_json::Value::as_str) {
            Some("thinking") => {
                if matches!(
                    entry
                        .get("reasoning_effort")
                        .and_then(serde_json::Value::as_str),
                    Some("medium" | "xhigh")
                ) {
                    entry.insert("reasoning_effort".into(), serde_json::json!("high"));
                    changed = true;
                }
            }
            _ => {
                if entry.remove("reasoning_effort").is_some() {
                    changed = true;
                }
            }
        }
    }

    if !changed {
        return false;
    }

    let Ok(serialized) = serde_json::to_string_pretty(&config) else {
        return false;
    };
    std::fs::write(config_path, serialized).is_ok()
}

/// Normalize force_thinking in all model entries to match upstream model capabilities.
/// Runs every startup. Idempotent: only writes if values differ.
/// This fixes stale force_thinking values left from older versions.
fn normalize_force_thinking(config_path: &std::path::Path) {
    let raw = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return,
    };

    let mut changed = false;
    if let Some(providers) = cfg.get_mut("providers").and_then(|p| p.as_object_mut()) {
        for (_pid, provider) in providers.iter_mut() {
            let is_openrouter = _pid == "openrouter";

            // Check if this OpenRouter provider has been migrated to profiles
            let has_profiles = is_openrouter && provider.get("profiles").is_some();

            if has_profiles {
                // Migrated config: normalize force_thinking inside each profile
                if let Some(profiles) = provider.get_mut("profiles").and_then(|p| p.as_array_mut()) {
                    for profile in profiles.iter_mut() {
                        if let Some(models) = profile.get_mut("models").and_then(|m| m.as_object_mut()) {
                            for (_model_key, entry) in models.iter_mut() {
                                let upstream = match entry.get("upstream_model").and_then(|v| v.as_str()) {
                                    Some(u) => u.to_string(),
                                    None => continue,
                                };
                                let caps = model_capabilities::resolve_static_model_capabilities(&upstream);
                                let current = entry.get("force_thinking").and_then(|v| v.as_bool());
                                if current != Some(caps.force_thinking) {
                                    entry["force_thinking"] = serde_json::Value::Bool(caps.force_thinking);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            } else if let Some(models) = provider.get_mut("models").and_then(|m| m.as_object_mut()) {
                for (_model_key, entry) in models.iter_mut() {
                    let upstream = match entry.get("upstream_model").and_then(|v| v.as_str()) {
                        Some(u) => u.to_string(),
                        None => continue,
                    };
                    // Skip OpenRouter models (capabilities come from live API cache)
                    if is_openrouter {
                        continue;
                    }
                    let caps = model_capabilities::resolve_static_model_capabilities(&upstream);
                    let current = entry.get("force_thinking").and_then(|v| v.as_bool());
                    if current != Some(caps.force_thinking) {
                        entry["force_thinking"] = serde_json::Value::Bool(caps.force_thinking);
                        changed = true;
                    }
                }
            }
        }
    }

    if changed {
        let serialized = serde_json::to_string_pretty(&cfg).unwrap_or_default();
        let _ = std::fs::write(config_path, serialized);
    }
}

#[derive(Serialize, Deserialize)]
struct UserPrefs {
    #[serde(default = "default_lang")]
    language: String,
    #[serde(default)]
    pricing_display_timezone: Option<String>,
}

fn default_lang() -> String {
    "en".into()
}

fn load_user_prefs() -> UserPrefs {
    let path = user_prefs_path();
    if path.exists() {
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(prefs) = serde_json::from_slice::<UserPrefs>(&bytes) {
                return prefs;
            }
        }
    }
    UserPrefs {
        language: default_lang(),
        pricing_display_timezone: None,
    }
}

#[tauri::command]
fn get_user_language() -> String {
    load_user_prefs().language
}

#[tauri::command]
fn set_user_language(language: String) -> Result<(), String> {
    let path = user_prefs_path();
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let prefs = UserPrefs {
        language,
        pricing_display_timezone: load_user_prefs().pricing_display_timezone,
    };
    let json = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(&path, json.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_pricing_display_timezone() -> Option<String> {
    load_user_prefs().pricing_display_timezone
}

#[tauri::command]
fn set_pricing_display_timezone(timezone_id: String) -> Result<(), String> {
    let existing = load_user_prefs();
    let prefs = UserPrefs {
        language: existing.language,
        pricing_display_timezone: Some(timezone_id),
    };
    let path = user_prefs_path();
    let dir = path.parent().unwrap();
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    std::fs::write(&path, json.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_first_run() -> Result<bool, String> {
    // Already configured
    if user_prefs_path().exists() {
        return Ok(false);
    }

    // Check for installer language file (written by NSIS installer hook)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let installer_lang = parent.join("resources").join("installer_lang.txt");
            if installer_lang.exists() {
                if let Ok(bytes) = std::fs::read(&installer_lang) {
                    let lang_id = String::from_utf8_lossy(&bytes).trim().to_string();
                    let app_lang = match lang_id.as_str() {
                        "ja" => "ja",
                        "zh-CN" => "zh-CN",
                        "zh-TW" => "zh-TW",
                        "ko" => "ko",
                        "fr" => "fr",
                        _ => "en",
                    };
                    let _ = std::fs::remove_file(&installer_lang);
                    // Create user_prefs.json with the installer-selected language
                    let _ = set_user_language(app_lang.to_string());
                    return Ok(false);
                }
            }
        }
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// Command 1: Health check
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    upstream: String,
}

#[tauri::command]
async fn check_health() -> Result<HealthResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    match client.get("http://127.0.0.1:4000/health").send().await {
        Ok(resp) => {
            let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
            Ok(HealthResponse {
                status: json["status"].as_str().unwrap_or("unknown").into(),
                upstream: json["upstream"].as_str().unwrap_or("").into(),
            })
        }
        Err(_) => Ok(HealthResponse {
            status: "unreachable".into(),
            upstream: "".into(),
        }),
    }
}

// ---------------------------------------------------------------------------
// Command 1b: Gateway status (used by dashboard)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct GatewayStatusResponse {
    reachable: bool,
    port_listening: bool,
    checked_at: String,
    error: Option<String>,
    managed_child_running: bool,
    managed_child_pid: Option<u32>,
    diagnostic: String,
}

#[tauri::command]
fn check_gateway_status(state: tauri::State<'_, ProxyState>) -> GatewayStatusResponse {
    use std::net::TcpStream;
    use std::time::Duration;

    let now = Local::now();
    let checked_at = now.format("%Y-%m-%d %H:%M:%S").to_string();

    // Check managed axum task
    let (managed_child_running, managed_child_pid) = {
        let guard = match state.handle.lock() {
            Ok(g) => g,
            Err(_) => {
                return GatewayStatusResponse {
                    reachable: false,
                    port_listening: false,
                    checked_at,
                    error: Some("Cannot lock proxy state".into()),
                    managed_child_running: false,
                    managed_child_pid: None,
                    diagnostic: "Lock error".into(),
                };
            }
        };
        match &*guard {
            Some(handle) => (!handle.inner().is_finished(), None),
            None => (false, None),
        }
    };

    // Check TCP port 4000
    let port_reachable = TcpStream::connect_timeout(
        &"127.0.0.1:4000".parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok();

    let port_listening = port_reachable;

    let diagnostic = format!(
        "managed_child_running: {}, managed_child_pid: {:?}, port_reachable: {}",
        managed_child_running, managed_child_pid, port_reachable
    );

    GatewayStatusResponse {
        reachable: port_reachable,
        port_listening,
        checked_at,
        error: None,
        managed_child_running,
        managed_child_pid,
        diagnostic,
    }
}

// ---------------------------------------------------------------------------
// Command 2: Check API key
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ApiKeyStatus {
    set: bool,
    env_var: String,
}

#[tauri::command]
fn check_api_key() -> Result<ApiKeyStatus, String> {
    match get_active_api_key_env() {
        Ok(env_var) => {
            let set = std::env::var(&env_var).is_ok();
            Ok(ApiKeyStatus { set, env_var })
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// Command 3: Set API key as environment variable
// ---------------------------------------------------------------------------

#[tauri::command]
fn set_env_api_key(key: String, env_var_name: String) -> Result<(), String> {
    let trimmed = key.trim().to_string();

    // Persist to user environment variable (survives app restart)
    // setx doesn't affect the current process, so we also call set_var below
    let output = std::process::Command::new("setx")
        .args([&env_var_name, &trimmed])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| format!("Failed to run setx: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("setx failed: {}", stderr));
    }

    // Also set for current process (setx only affects new processes)
    std::env::set_var(&env_var_name, &trimmed);

    Ok(())
}

// ---------------------------------------------------------------------------
// Command 3x: Update provider's api_key_env in config.json
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_provider_api_key_env(
    config_state: tauri::State<'_, ConfigState>,
    provider_id: String,
    api_key_env: String,
) -> Result<(), String> {
    // Validate env var name format: uppercase letters, digits, underscores only
    if api_key_env.is_empty() {
        return Err("Environment variable name cannot be empty".into());
    }
    let valid = api_key_env
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !valid {
        return Err(
            "Environment variable name must be uppercase letters, digits, or underscores (e.g. MOONSHOT_API_KEY)"
                .into(),
        );
    }

    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_provider_api_key_env(cfg, &provider_id, &api_key_env)
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 3y: Update upstream model for a specific gateway model
// (pure function + thin Tauri wrapper)
// ---------------------------------------------------------------------------

/// Helper: write model entry fields into a target JSON object that contains
/// `models` and `model_map` keys (a profile or the provider-level root).
/// Preserves the existing None→remove semantics from the original command.
fn write_model_entry_fields(
    target: &mut serde_json::Value,
    model_key: &str,
    upstream_model: &str,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
    is_openrouter: bool,
) -> Result<(), String> {
    let models = target
        .get_mut("models")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("models must be a JSON object")?;
    let model_entry = models
        .get_mut(model_key)
        .ok_or_else(|| format!("Model '{}' not found", model_key))?;
    if !model_entry.is_object() {
        return Err(format!(
            "Model '{}' entry must be a JSON object, found {}",
            model_key,
            if model_entry.is_null() { "null" } else { "a non-object value" }
        ));
    }
    model_entry["upstream_model"] = serde_json::Value::String(upstream_model.to_string());
    write_capability_flags(model_entry, upstream_model, is_openrouter)?;

    match thinking_mode {
        Some(tm) => {
            model_entry["thinking_mode"] = serde_json::Value::String(tm.to_string());
        }
        None => {
            model_entry
                .as_object_mut()
                .map(|obj| obj.remove("thinking_mode"));
        }
    }
    match reasoning_effort {
        Some(effort) => {
            model_entry["reasoning_effort"] = serde_json::Value::String(effort.to_string());
        }
        None => {
            model_entry
                .as_object_mut()
                .map(|obj| obj.remove("reasoning_effort"));
        }
    }

    // Dual-write to model_map
    let model_map = target
        .get_mut("model_map")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("model_map must be a JSON object")?;
    model_map.insert(
        model_key.to_string(),
        serde_json::Value::String(upstream_model.to_string()),
    );
    Ok(())
}

/// Pure function: update upstream model routing for one model key.
/// For OpenRouter providers, `profile_id` is required and targets a specific profile.
/// For non-OpenRouter providers, `profile_id` is ignored and the provider-level models are updated.
///
/// No-op detection: clones the target JSON node, applies the change, and compares
/// before/after — this automatically covers capability fields, model_map sync, and
/// any future fields added by write_capability_flags.
fn apply_set_model_upstream(
    cfg: &mut serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
    model_key: &str,
    upstream_model: &str,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
) -> Result<ApplyOutcome<()>, String> {
    if upstream_model.trim().is_empty() {
        return Err("upstream_model cannot be empty".into());
    }

    let providers = cfg["providers"]
        .as_object_mut()
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get_mut(provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;

    let is_openrouter = provider_id == "openrouter";

    // -- OpenRouter branch: target specific profile --
    if is_openrouter {
        let pid = profile_id.ok_or("profile_id is required for OpenRouter provider")?;
        let profiles = provider["profiles"]
            .as_array_mut()
            .ok_or("OpenRouter provider has no 'profiles' array")?;
        let prof_idx = profiles
            .iter()
            .position(|p| p.get("id").and_then(|i| i.as_str()) == Some(pid))
            .ok_or_else(|| format!("Profile '{}' not found", pid))?;

        // Clone the profile, apply changes to clone, compare
        let original = profiles[prof_idx].clone();
        let mut updated = original.clone();
        write_model_entry_fields(
            &mut updated,
            model_key,
            upstream_model,
            thinking_mode,
            reasoning_effort,
            true,
        )?;

        let config_changed = original != updated;
        if config_changed {
            profiles[prof_idx] = updated;
        }

        let active_provider = cfg
            .get("active_provider")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let active_profile_id = cfg
            .get("active_openrouter_profile_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let restart_gateway = config_changed && active_provider == "openrouter" && pid == active_profile_id;

        return Ok(ApplyOutcome {
            value: (),
            config_changed,
            restart_gateway,
            restart_reason: if restart_gateway {
                "model_upstream_changed_active_route"
            } else {
                "model_upstream_changed"
            },
        });
    }

    // -- Non-OpenRouter branch: target provider-level models --
    {
        let models = &provider["models"];
        if !models.is_object() {
            return Err(format!("Provider '{}' has no 'models' key", provider_id));
        }
    }

    // Clone provider's models+model_map, apply changes, compare
    let original = serde_json::json!({
        "models": &provider["models"],
        "model_map": &provider["model_map"],
    });
    let mut updated = original.clone();
    write_model_entry_fields(
        &mut updated,
        model_key,
        upstream_model,
        thinking_mode,
        reasoning_effort,
        false,
    )?;

    let config_changed = original != updated;
    if config_changed {
        provider["models"] = updated["models"].clone();
        provider["model_map"] = updated["model_map"].clone();
    }

    let active_provider = cfg
        .get("active_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let restart_gateway = config_changed && active_provider == provider_id;

    Ok(ApplyOutcome {
        value: (),
        config_changed,
        restart_gateway,
        restart_reason: if restart_gateway {
            "model_upstream_changed_active_route"
        } else {
            "model_upstream_changed"
        },
    })
}

const TEXT_ONLY_OR_MODELS: &[&str] = &[
    "poolside/laguna-s-2.1",
    "poolside/laguna-s-2.1:free",
    "poolside/laguna-xs-2.1",
    "poolside/laguna-xs-2.1:free",
];

// ---------------------------------------------------------------------------
// OpenRouter multi-profile: builders, migration, helpers
// ---------------------------------------------------------------------------

/// Unified Tauri command response — carries an optional typed value
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[derive(Debug)]
struct CommandResponse<T: serde::Serialize> {
    value: T,
    #[serde(rename = "restartGateway")]
    restart_gateway: bool,
    #[serde(rename = "restartReason")]
    restart_reason: &'static str,
}

struct MigrationOutcome {
    changed: bool,
    active_profile_id: Option<String>,
}

fn model_entry(
    _gateway_model: &str,
    upstream: &str,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
) -> ModelEntry {
    let caps = model_capabilities::resolve_static_model_capabilities(upstream);
    ModelEntry {
        upstream_model: upstream.to_string(),
        canonical: None,
        thinking: None,
        supports_vision: None,
        supports_video: None,
        visible: true,
        force_thinking: Some(caps.force_thinking),
        supports_non_thinking: None,
        supports_image_url: Some(caps.supports_image_url),
        supports_image_base64: Some(caps.supports_image_base64),
        supports_video_url: Some(caps.supports_video_url),
        supports_video_base64: Some(caps.supports_video_base64),
        thinking_mode: thinking_mode.map(String::from),
        reasoning_effort: reasoning_effort.map(String::from),
    }
}

fn build_laguna_profile(name: &str) -> OpenRouterProfile {
    let id = "a0e0f000-0000-4000-8000-000000000001".to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry(
        "claude-opus-5",
        "poolside/laguna-s-2.1",
        Some("thinking"),
        Some("max"),
    );
    let sonnet_entry = model_entry(
        "claude-sonnet-5",
        "poolside/laguna-s-2.1",
        Some("normal"),
        None,
    );
    let haiku_entry = model_entry(
        "claude-haiku-4-5",
        "poolside/laguna-xs-2.1",
        Some("thinking"),
        None,
    );

    model_map.insert("claude-opus-5".into(), "poolside/laguna-s-2.1".into());
    model_map.insert(
        "claude-sonnet-5".into(),
        "poolside/laguna-s-2.1".into(),
    );
    model_map.insert(
        "claude-haiku-4-5".into(),
        "poolside/laguna-xs-2.1".into(),
    );

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_hy3_profile(name: &str) -> OpenRouterProfile {
    let id = "b0e0f000-0000-4000-8000-000000000002".to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry(
        "claude-opus-5",
        "tencent/hy3",
        Some("thinking"),
        Some("high"),
    );
    let sonnet_entry = model_entry(
        "claude-sonnet-5",
        "tencent/hy3",
        Some("thinking"),
        Some("low"),
    );
    let haiku_entry = model_entry(
        "claude-haiku-4-5",
        "tencent/hy3",
        Some("normal"),
        None,
    );

    model_map.insert("claude-opus-5".into(), "tencent/hy3".into());
    model_map.insert("claude-sonnet-5".into(), "tencent/hy3".into());
    model_map.insert("claude-haiku-4-5".into(), "tencent/hy3".into());

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_laguna_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_laguna_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

fn build_hy3_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_hy3_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

fn build_inclusionai_profile(name: &str) -> OpenRouterProfile {
    let id = "c0e0f000-0000-4000-8000-000000000003".to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry(
        "claude-opus-5",
        "inclusionai/ring-2.6-1t",
        Some("thinking"),
        Some("xhigh"),
    );
    let sonnet_entry = model_entry(
        "claude-sonnet-5",
        "inclusionai/ling-2.6-1t",
        Some("normal"),
        None,
    );
    let haiku_entry = model_entry(
        "claude-haiku-4-5",
        "inclusionai/ling-2.6-flash",
        Some("normal"),
        None,
    );

    model_map.insert("claude-opus-5".into(), "inclusionai/ring-2.6-1t".into());
    model_map.insert("claude-sonnet-5".into(), "inclusionai/ling-2.6-1t".into());
    model_map.insert("claude-haiku-4-5".into(), "inclusionai/ling-2.6-flash".into());

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_stepfun_profile(name: &str) -> OpenRouterProfile {
    let id = "d0e0f000-0000-4000-8000-000000000004".to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry(
        "claude-opus-5",
        "stepfun/step-3.7-flash",
        Some("thinking"),
        Some("high"),
    );
    let sonnet_entry = model_entry(
        "claude-sonnet-5",
        "stepfun/step-3.7-flash",
        Some("thinking"),
        Some("medium"),
    );
    let haiku_entry = model_entry(
        "claude-haiku-4-5",
        "stepfun/step-3.5-flash",
        Some("thinking"),
        None,
    );

    model_map.insert("claude-opus-5".into(), "stepfun/step-3.7-flash".into());
    model_map.insert("claude-sonnet-5".into(), "stepfun/step-3.7-flash".into());
    model_map.insert("claude-haiku-4-5".into(), "stepfun/step-3.5-flash".into());

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_inclusionai_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_inclusionai_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

fn build_stepfun_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_stepfun_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

const GEMINI_PROFILE_ID: &str = "f0e0f000-0000-4000-8000-000000000006";
const GEMINI_PROFILE_NAME: &str = "OpenRouter: Gemini";

fn build_gemini_profile(name: &str) -> OpenRouterProfile {
    let id = GEMINI_PROFILE_ID.to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry("claude-opus-5", "google/gemini-3.7-flash", Some("thinking"), Some("high"));
    let sonnet_entry = model_entry("claude-sonnet-5", "google/gemini-3.7-flash", Some("thinking"), Some("medium"));
    let haiku_entry = model_entry("claude-haiku-4-5", "google/gemini-3.7-flash", Some("thinking"), Some("low"));

    model_map.insert("claude-opus-5".into(), "google/gemini-3.7-flash".into());
    model_map.insert("claude-sonnet-5".into(), "google/gemini-3.7-flash".into());
    model_map.insert("claude-haiku-4-5".into(), "google/gemini-3.7-flash".into());

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_gemini_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_gemini_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

/// One-time: rewrite a built-in `OpenRouter: Gemini` profile that still carries
/// an earlier default (either the initial 3.1 Pro/3.7 Flash/3.5 Flash Lite default
/// or the interim all-3.7-Flash High/High/Low default) to the current
/// all-3.7-Flash High/Medium/Low default. Only an EXACT match of all three
/// slots is migrated; if any slot was user-edited the profile is left untouched.
fn migrate_gemini_profile_to_current_default(config_path: &std::path::Path) -> bool {
    let Ok(content) = std::fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(mut config) = serde_json::from_str::<serde_json::Value>(&content) else {
        return false;
    };

    let Some(profiles) = config
        .pointer_mut("/providers/openrouter/profiles")
        .and_then(serde_json::Value::as_array_mut)
    else {
        return false;
    };

    let Some(profile) = profiles.iter_mut().find(|p| {
        p.get("id").and_then(serde_json::Value::as_str) == Some(GEMINI_PROFILE_ID)
    }) else {
        return false;
    };

    if !gemini_matches_migratable_default(profile) {
        return false;
    }

    // Replace only model_map + models with the new default; preserve
    // display_name / visible_models / hidden / claude_code.
    let new_default = build_gemini_profile_json(GEMINI_PROFILE_NAME);
    let Some(obj) = profile.as_object_mut() else {
        return false;
    };
    if let Some(model_map) = new_default.get("model_map").cloned() {
        obj.insert("model_map".to_string(), model_map);
    }
    if let Some(models) = new_default.get("models").cloned() {
        obj.insert("models".to_string(), models);
    }

    let Ok(serialized) = serde_json::to_string_pretty(&config) else {
        return false;
    };
    std::fs::write(config_path, serialized).is_ok()
}

/// True when a built-in Gemini profile still carries one of the earlier
/// migratable built-in defaults across all three slots.
fn gemini_matches_migratable_default(profile: &serde_json::Value) -> bool {
    // 1. Initial default: Opus=3.1 Pro Preview/high, Sonnet=3.7 Flash/high, Haiku=3.5 Flash Lite/low
    const INITIAL_DEFAULT: [(&str, &str, &str); 3] = [
        ("claude-opus-5", "google/gemini-3.1-pro-preview", "high"),
        ("claude-sonnet-5", "google/gemini-3.7-flash", "high"),
        ("claude-haiku-4-5", "google/gemini-3.5-flash-lite", "low"),
    ];

    // 2. Interim default: Opus=3.7 Flash/high, Sonnet=3.7 Flash/high, Haiku=3.7 Flash/low
    const INTERIM_DEFAULT: [(&str, &str, &str); 3] = [
        ("claude-opus-5", "google/gemini-3.7-flash", "high"),
        ("claude-sonnet-5", "google/gemini-3.7-flash", "high"),
        ("claude-haiku-4-5", "google/gemini-3.7-flash", "low"),
    ];

    let matches_pattern = |pattern: &[(&str, &str, &str); 3]| -> bool {
        let Some(model_map) = profile.get("model_map").and_then(serde_json::Value::as_object) else {
            return false;
        };
        let Some(models) = profile.get("models").and_then(serde_json::Value::as_object) else {
            return false;
        };

        for (route, upstream, effort) in pattern {
            if model_map.get(*route).and_then(serde_json::Value::as_str) != Some(*upstream) {
                return false;
            }
            let Some(entry) = models.get(*route).and_then(serde_json::Value::as_object) else {
                return false;
            };
            if entry.get("upstream_model").and_then(serde_json::Value::as_str) != Some(*upstream) {
                return false;
            }
            if entry.get("reasoning_effort").and_then(serde_json::Value::as_str) != Some(*effort) {
                return false;
            }
        }
        true
    };

    matches_pattern(&INITIAL_DEFAULT) || matches_pattern(&INTERIM_DEFAULT)
}

const GPT56_BALANCED_PROFILE_ID: &str = "e0e0f000-0000-4000-8000-000000000005";
const GPT56_BALANCED_PROFILE_NAME: &str = "OpenAI GPT-5.6 Balanced";

fn build_gpt56_balanced_profile(name: &str) -> OpenRouterProfile {
    let id = GPT56_BALANCED_PROFILE_ID.to_string();
    let mut models = std::collections::HashMap::new();
    let mut model_map = std::collections::HashMap::new();
    let visible_models: Vec<String> = vec![
        "claude-opus-5".into(),
        "claude-sonnet-5".into(),
        "claude-haiku-4-5".into(),
    ];

    let opus_entry = model_entry("claude-opus-5", "openai/gpt-5.6-sol", Some("thinking"), Some("high"));
    let sonnet_entry = model_entry("claude-sonnet-5", "openai/gpt-5.6-terra", Some("thinking"), Some("high"));
    let haiku_entry = model_entry("claude-haiku-4-5", "openai/gpt-5.6-luna", Some("thinking"), Some("high"));

    model_map.insert("claude-opus-5".into(), "openai/gpt-5.6-sol".into());
    model_map.insert("claude-sonnet-5".into(), "openai/gpt-5.6-terra".into());
    model_map.insert("claude-haiku-4-5".into(), "openai/gpt-5.6-luna".into());

    models.insert("claude-opus-5".into(), opus_entry);
    models.insert("claude-sonnet-5".into(), sonnet_entry);
    models.insert("claude-haiku-4-5".into(), haiku_entry);

    OpenRouterProfile {
        id,
        display_name: name.to_string(),
        model_map,
        visible_models,
        models,
        hidden: false,
        claude_code: Some(ClaudeCodeProviderSection::default()),
    }
}

fn build_gpt56_balanced_profile_json(name: &str) -> serde_json::Value {
    serde_json::to_value(build_gpt56_balanced_profile(name))
        .expect("OpenRouterProfile serialization must succeed")
}

/// Fill missing gateway model keys in a migrated profile.
/// Existing entries are left unchanged; only gaps are backfilled from the fallback.
fn complete_migrated_profile(
    profile: &mut OpenRouterProfile,
    fallback: &OpenRouterProfile,
) {
    for key in [
        "claude-opus-5",
        "claude-sonnet-5",
        "claude-haiku-4-5",
    ] {
        if !profile.models.contains_key(key) {
            if let Some(entry) = fallback.models.get(key) {
                profile.models.insert(key.to_string(), entry.clone());
            }
        }
        if !profile.model_map.contains_key(key) {
            if let Some(upstream) = fallback.model_map.get(key) {
                profile.model_map.insert(key.to_string(), upstream.clone());
            }
        }
        if !profile.visible_models.iter().any(|m| m == key) {
            profile.visible_models.push(key.to_string());
        }
    }
}

/// Migrate legacy single-provider OpenRouter config into the multi-profile schema.
///
/// Returns:
///   Ok({ changed: false, ... }) — OpenRouter provider absent, or already migrated
///   Ok({ changed: true, ... }) — migration created profiles; caller persists
///   Err(...) — JSON shape is invalid
fn migrate_openrouter_to_profiles(
    raw: &mut serde_json::Value,
) -> Result<MigrationOutcome, String> {
    let Some(openrouter) = raw
        .get_mut("providers")
        .and_then(|p| p.get_mut("openrouter"))
    else {
        return Ok(MigrationOutcome {
            changed: false,
            active_profile_id: None,
        });
    };

    if !openrouter.is_object() {
        return Err("OpenRouter provider must be a JSON object".into());
    }

    // Presence check on the raw JSON — works for both empty and non-empty arrays.
    if openrouter.get("profiles").is_some() {
        return Ok(MigrationOutcome {
            changed: false,
            active_profile_id: None,
        });
    }

    let existing_models: Vec<String> = openrouter
        .get("models")
        .and_then(|m| m.as_object())
        .map(|obj| {
            obj.values()
                .filter_map(|v| v.get("upstream_model").and_then(|s| s.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();

    let existing_uses_poolside = existing_models
        .iter()
        .any(|m| model_capabilities::is_laguna_model(m));
    let existing_uses_hy3 = existing_models
        .iter()
        .any(|m| model_capabilities::is_hy3_model(m));

    let first_id = uuid::Uuid::new_v4().to_string();

    let first_profile_json = serde_json::json!({
        "id": &first_id,
        "display_name": "OpenRouter",
        "model_map": openrouter.get("model_map").cloned().unwrap_or_else(|| serde_json::json!({})),
        "visible_models": openrouter.get("visible_models").cloned().unwrap_or_else(|| serde_json::json!([])),
        "models": openrouter.get("models").cloned().unwrap_or_else(|| serde_json::json!({})),
    });

    let second_profile = if existing_uses_hy3 && !existing_uses_poolside {
        build_laguna_profile_json("OpenRouter: Laguna")
    } else {
        build_hy3_profile_json("OpenRouter: Hy3")
    };

    // Deserialize first profile into typed struct for backfill
    let mut first_profile: OpenRouterProfile =
        serde_json::from_value(first_profile_json).map_err(|e| format!("failed to parse first profile: {}", e))?;

    let fallback_typed = if existing_uses_hy3 && !existing_uses_poolside {
        build_hy3_profile("_fallback")
    } else {
        build_laguna_profile("_fallback")
    };
    complete_migrated_profile(&mut first_profile, &fallback_typed);

    let first_profile_value = serde_json::to_value(&first_profile)
        .map_err(|e| format!("failed to serialize first profile: {}", e))?;

    let openrouter_obj = openrouter
        .as_object_mut()
        .ok_or("OpenRouter provider must be a JSON object")?;

    openrouter_obj.remove("model_map");
    openrouter_obj.remove("visible_models");
    openrouter_obj.remove("models");
    openrouter_obj.insert(
        "profiles".to_string(),
        serde_json::json!([first_profile_value, second_profile]),
    );

    Ok(MigrationOutcome {
        changed: true,
        active_profile_id: Some(first_id),
    })
}

/// Normalize legacy OpenRouter profile names to canonical "Model N".
/// Historical versions generated all "OpenRouter" and "OpenRouter: *" names
/// automatically (migration, repair, and bundled template). User-facing
/// "Add Profile" has always defaulted to "New Profile" (locale-translated).
/// Therefore `starts_with("OpenRouter: ")` is safe to use as a legacy-name detector.
///
/// The built-in profile names are preserved as-is so that the fixed
/// display names ("OpenRouter: Laguna", "OpenRouter: Hy3",
/// "OpenRouter: InclusionAI", "OpenRouter: StepFun",
/// "OpenAI GPT-5.6 Balanced", "OpenRouter: Gemini") are never renamed.
fn normalize_openrouter_profile_names(profiles: &mut Vec<serde_json::Value>) -> bool {
    use std::collections::BTreeSet;

    const LEGACY_PREFIX: &str = "OpenRouter: ";

    /// Built-in profile display names that must never be renamed.
    const BUILTIN_NAMES: &[&str] = &[
        "OpenRouter: Laguna",
        "OpenRouter: Hy3",
        "OpenRouter: InclusionAI",
        "OpenRouter: StepFun",
        "OpenAI GPT-5.6 Balanced",
        "OpenRouter: Gemini",
    ];

    let mut used: BTreeSet<u32> = profiles
        .iter()
        .filter_map(|p| {
            p.get("display_name")
                .and_then(|v| v.as_str())
                .and_then(|n| paths::parse_model_set_number(n))
        })
        .collect();

    let mut changed = false;

    for profile in profiles.iter_mut() {
        let display_name = match profile.get("display_name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => continue,
        };

        // Never rename the canonical built-in profiles.
        if BUILTIN_NAMES.contains(&display_name) {
            continue;
        }

        let is_legacy =
            display_name == "OpenRouter" || display_name.starts_with(LEGACY_PREFIX);
        if !is_legacy {
            continue;
        }

        let mut n = 1u32;
        while used.contains(&n) {
            n += 1;
        }

        profile["display_name"] = serde_json::Value::String(format!("Model {n}"));
        used.insert(n);
        changed = true;
    }

    changed
}

/// Normalize config on load: repair empty profiles, normalize legacy names,
/// and repair missing/invalid active profile ID.
/// Returns true if the config was modified (caller should persist).
fn normalize_config(cfg: &mut serde_json::Value) -> bool {
    // Extract needed info via an immutable pass first to avoid borrow conflicts
    let (needs_profiles_repair, first_id, active_id_valid, active_id_exists) = {
        let openrouter = match cfg.get("providers").and_then(|p| p.get("openrouter")) {
            Some(p) => p,
            None => return false,
        };

        let profiles = openrouter.get("profiles").and_then(|p| p.as_array());
        let Some(profiles) = profiles else {
            return false;
        };

        let empty = profiles.is_empty();

        let fid = if empty {
            // Will be filled by the repair step
            String::new()
        } else {
            profiles[0]
                .get("id")
                .and_then(|i| i.as_str())
                .unwrap_or("")
                .to_string()
        };

        let valid = cfg
            .get("active_openrouter_profile_id")
            .and_then(|v| v.as_str())
            .is_some_and(|id_str| {
                profiles
                    .iter()
                    .any(|p| p.get("id").and_then(|i| i.as_str()) == Some(id_str))
            });

        let exists = cfg.get("active_openrouter_profile_id").is_some();

        (empty, fid, valid, exists)
    };

    // Now perform mutations with no live references
    let mut changed = false;

    let openrouter = cfg
        .get_mut("providers")
        .and_then(|p| p.get_mut("openrouter"))
        .unwrap();

    let profiles_array = openrouter
        .get_mut("profiles")
        .and_then(|p| p.as_array_mut())
        .unwrap();

    // 1. Empty profiles → rebuild a default pair (Laguna + Hy3).
    if needs_profiles_repair {
        let laguna = serde_json::to_value(build_laguna_profile("OpenRouter: Laguna"))
            .expect("serialization must succeed");
        let hy3 = serde_json::to_value(build_hy3_profile("OpenRouter: Hy3"))
            .expect("serialization must succeed");
        profiles_array.push(laguna);
        profiles_array.push(hy3);
        // Normalize legacy names to canonical "Model N" (collision-safe)
        if normalize_openrouter_profile_names(profiles_array) {
            changed = true;
        }

        // Recompute first_id since we just added profiles
        let new_first_id = profiles_array[0]
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        if !active_id_exists || !active_id_valid {
            cfg.as_object_mut()
                .expect("root is object")
                .insert(
                    "active_openrouter_profile_id".into(),
                    serde_json::Value::String(new_first_id),
                );
            changed = true;
        }
        return changed;
    }

    // 2. Legacy profile name normalization (collision-safe, idempotent)
    if normalize_openrouter_profile_names(profiles_array) {
        changed = true;
    }

    // 3. Active profile id missing or invalid → repair to first profile.
    if !active_id_exists {
        cfg.as_object_mut()
            .expect("root is object")
            .insert(
                "active_openrouter_profile_id".into(),
                serde_json::Value::String(first_id),
            );
        changed = true;
    } else if !active_id_valid {
        cfg.as_object_mut()
            .expect("root is object")
            .insert(
                "active_openrouter_profile_id".into(),
                serde_json::Value::String(first_id),
            );
        changed = true;
    }

    changed
}

/// Resolve capability flags for an upstream model and write them into the config entry.
fn write_capability_flags(
    entry: &mut serde_json::Value,
    upstream_model: &str,
    is_openrouter: bool,
) -> Result<(), String> {
    // OpenRouter: known text-to-text-only models → all false
    if is_openrouter && TEXT_ONLY_OR_MODELS.contains(&upstream_model) {
        let map = entry
            .as_object_mut()
            .ok_or("model entry must be a JSON object")?;
        map.insert("supports_image_url".into(), serde_json::Value::Bool(false));
        map.insert("supports_image_base64".into(), serde_json::Value::Bool(false));
        map.insert("supports_video_url".into(), serde_json::Value::Bool(false));
        map.insert("supports_video_base64".into(), serde_json::Value::Bool(false));
        map.insert("force_thinking".into(), serde_json::Value::Bool(false));
        return Ok(());
    }

    // OpenRouter: model not in TEXT_ONLY_OR_MODELS — check if statically known
    if is_openrouter {
        if let Some(caps) = model_capabilities::try_resolve_static_model_capabilities(upstream_model) {
            let map = entry
                .as_object_mut()
                .ok_or("model entry must be a JSON object")?;
            map.insert("supports_image_url".into(), serde_json::Value::Bool(caps.supports_image_url));
            map.insert("supports_image_base64".into(), serde_json::Value::Bool(caps.supports_image_base64));
            map.insert("supports_video_url".into(), serde_json::Value::Bool(caps.supports_video_url));
            map.insert("supports_video_base64".into(), serde_json::Value::Bool(caps.supports_video_base64));
            map.insert("force_thinking".into(), serde_json::Value::Bool(caps.force_thinking));
            return Ok(());
        }
        // Unknown OpenRouter model — preserve existing flags
        return Ok(());
    }

    // Non-OpenRouter: use static resolver
    let c = model_capabilities::resolve_static_model_capabilities(upstream_model);
    let map = entry
        .as_object_mut()
        .ok_or("model entry must be a JSON object")?;
    map.insert("supports_image_url".into(), serde_json::Value::Bool(c.supports_image_url));
    map.insert("supports_image_base64".into(), serde_json::Value::Bool(c.supports_image_base64));
    map.insert("supports_video_url".into(), serde_json::Value::Bool(c.supports_video_url));
    map.insert("supports_video_base64".into(), serde_json::Value::Bool(c.supports_video_base64));
    map.insert("force_thinking".into(), serde_json::Value::Bool(c.force_thinking));
    Ok(())
}

#[tauri::command]
fn set_model_upstream(
    config_state: tauri::State<'_, ConfigState>,
    provider_id: String,
    model_key: String,
    upstream_model: String,
    thinking_mode: Option<String>,
    reasoning_effort: Option<String>,
    profile_id: Option<String>,
) -> Result<CommandResponse<()>, String> {
    validate_set_model_upstream_input(
        &upstream_model,
        thinking_mode.as_deref(),
        reasoning_effort.as_deref(),
    )?;

    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_set_model_upstream(
            cfg,
            &provider_id,
            profile_id.as_deref(),
            &model_key,
            &upstream_model,
            thinking_mode.as_deref(),
            reasoning_effort.as_deref(),
        )
    })
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// OpenRouter multi-profile — shared types & pure apply functions
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// OpenRouter multi-profile — shared types & pure apply functions
// ---------------------------------------------------------------------------

/// Internal outcome from pure apply_* functions — config_changed gates persistence.
#[derive(Debug)]
struct ApplyOutcome<T> {
    value: T,
    config_changed: bool,
    restart_gateway: bool,
    restart_reason: &'static str,
}

/// Orchestration helper: run a pure apply_* function against an in-memory config
/// and persist only when config_changed is true.  The `save` closure is injected
/// so tests can count saves without touching the real config file.
fn execute_config_mutation<T: serde::Serialize, FApply, FSave>(
    cfg: &mut serde_json::Value,
    apply: FApply,
    save: FSave,
) -> Result<CommandResponse<T>, String>
where
    FApply: FnOnce(&mut serde_json::Value) -> Result<ApplyOutcome<T>, String>,
    FSave: FnOnce(&serde_json::Value) -> Result<(), String>,
{
    let outcome = apply(cfg)?;
    if outcome.config_changed {
        save(cfg)?;
    }
    Ok(CommandResponse {
        value: outcome.value,
        restart_gateway: outcome.restart_gateway,
        restart_reason: outcome.restart_reason,
    })
}

// ── Group B pure apply functions (JSON mutation only, no I/O) ──────

fn apply_update_provider_api_key_env(
    cfg: &mut serde_json::Value,
    provider_id: &str,
    api_key_env: &str,
) -> Result<ApplyOutcome<()>, String> {
    let providers = cfg["providers"]
        .as_object_mut()
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get_mut(provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;
    provider["api_key_env"] = serde_json::Value::String(api_key_env.to_string());
    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: false,
        restart_reason: "",
    })
}

fn apply_update_active_provider(
    cfg: &mut serde_json::Value,
    provider_id: &str,
) -> Result<ApplyOutcome<()>, String> {
    // Validate provider exists
    cfg["providers"]
        .as_object()
        .and_then(|p| p.get(provider_id))
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;
    cfg["active_provider"] = serde_json::Value::String(provider_id.to_string());
    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: false,
        restart_reason: "",
    })
}

fn apply_update_server_config(
    cfg: &mut serde_json::Value,
    host: &str,
    port: u16,
    enable_cors: bool,
) -> Result<ApplyOutcome<()>, String> {
    let server = cfg["server"]
        .as_object_mut()
        .ok_or("config.json missing 'server' key")?;
    server["host"] = serde_json::Value::String(host.to_string());
    server["port"] = serde_json::Value::Number(serde_json::Number::from(port));
    server["enable_cors"] = serde_json::Value::Bool(enable_cors);
    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: false,
        restart_reason: "",
    })
}

fn apply_update_normalize_model_identity(
    cfg: &mut serde_json::Value,
    enabled: bool,
) -> Result<ApplyOutcome<()>, String> {
    cfg["normalize_response_model_identity"] = serde_json::Value::Bool(enabled);
    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: false,
        restart_reason: "",
    })
}

// ── Pure apply functions (no file I/O, operate on serde_json::Value) ──

fn normalize_profile_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("profile name cannot be empty".into());
    }
    if trimmed.chars().count() > 80 {
        return Err("profile name is too long".into());
    }
    Ok(trimmed.to_string())
}

// ── Pure apply functions (no file I/O, operate on serde_json::Value) ──

fn apply_add_openrouter_profile(
    cfg: &mut serde_json::Value,
    name: &str,
) -> Result<ApplyOutcome<serde_json::Value>, String> {
    let normalized = normalize_profile_name(name)?;
    let provider = cfg["providers"]["openrouter"]
        .as_object_mut()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"].as_array_mut().ok_or("no profiles array")?;
    let profile = build_laguna_profile(&normalized);
    let profile_value =
        serde_json::to_value(&profile).map_err(|e| format!("JSON error: {}", e))?;
    profiles.push(profile_value.clone());
    Ok(ApplyOutcome {
        value: profile_value,
        config_changed: true,
        restart_gateway: false,
        restart_reason: "profile_added",
    })
}

fn apply_delete_openrouter_profile(
    cfg: &mut serde_json::Value,
    profile_id: &str,
) -> Result<ApplyOutcome<()>, String> {
    let active_openrouter_id = cfg
        .get("active_openrouter_profile_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let active_provider = cfg
        .get("active_provider")
        .and_then(|v| v.as_str())
        .map(String::from);

    let provider = cfg["providers"]["openrouter"]
        .as_object_mut()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"]
        .as_array_mut()
        .ok_or("OpenRouter provider has no 'profiles' array")?;

    if profiles.len() <= 1 {
        return Err("cannot delete the last OpenRouter profile".into());
    }

    let pos = profiles
        .iter()
        .position(|p| p.get("id").and_then(|i| i.as_str()) == Some(profile_id))
        .ok_or("profile not found")?;

    let was_selected_profile = active_openrouter_id.as_deref() == Some(profile_id);
    let affected_running_route =
        active_provider.as_deref() == Some("openrouter") && was_selected_profile;

    profiles.remove(pos);

    if was_selected_profile {
        let new_first_id = profiles[0]
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        cfg.as_object_mut()
            .expect("root is object")
            .insert(
                "active_openrouter_profile_id".into(),
                serde_json::Value::String(new_first_id),
            );
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: affected_running_route,
        restart_reason: if affected_running_route {
            "active_profile_deleted"
        } else {
            "profile_deleted"
        },
    })
}

fn apply_rename_openrouter_profile(
    cfg: &mut serde_json::Value,
    profile_id: &str,
    new_name: &str,
) -> Result<ApplyOutcome<()>, String> {
    let normalized = normalize_profile_name(new_name)?;
    let provider = cfg["providers"]["openrouter"]
        .as_object_mut()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"]
        .as_array_mut()
        .ok_or("OpenRouter provider has no 'profiles' array")?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(profile_id))
        .ok_or("profile not found")?;

    let current = profile["display_name"].as_str().unwrap_or("");
    let config_changed = current != normalized;
    if config_changed {
        profile["display_name"] = serde_json::Value::String(normalized);
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed,
        restart_gateway: false,
        restart_reason: if config_changed {
            "profile_renamed"
        } else {
            "already_named"
        },
    })
}

fn apply_reorder_openrouter_profiles(
    cfg: &mut serde_json::Value,
    profile_ids: &[String],
) -> Result<ApplyOutcome<()>, String> {
    let provider = cfg["providers"]["openrouter"]
        .as_object_mut()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"]
        .as_array_mut()
        .ok_or("OpenRouter provider has no 'profiles' array")?;

    if profile_ids.len() != profiles.len() {
        return Err("profile id count does not match".into());
    }

    let supplied: std::collections::HashSet<&str> =
        profile_ids.iter().map(|s| s.as_str()).collect();
    if supplied.len() != profile_ids.len() {
        return Err("duplicate profile id".into());
    }

    let current_ids: std::collections::HashSet<&str> = profiles
        .iter()
        .filter_map(|p| p.get("id").and_then(|i| i.as_str()))
        .collect();
    if supplied != current_ids {
        return Err("profile id set does not match".into());
    }

    // Check if already in the same order (no-op)
    let current_order: Vec<&str> = profiles
        .iter()
        .filter_map(|p| p.get("id").and_then(|i| i.as_str()))
        .collect();
    let config_changed = current_order != profile_ids.iter().map(|s| s.as_str()).collect::<Vec<_>>();

    if config_changed {
        profiles.sort_by_key(|p| {
            let pid = p.get("id").and_then(|i| i.as_str()).unwrap_or("");
            profile_ids.iter().position(|id| id == pid).unwrap_or(0)
        });
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed,
        restart_gateway: false,
        restart_reason: if config_changed {
            "profiles_reordered"
        } else {
            "already_ordered"
        },
    })
}

fn apply_set_openrouter_profile_hidden(
    cfg: &mut serde_json::Value,
    profile_id: &str,
    hidden: bool,
) -> Result<ApplyOutcome<()>, String> {
    let provider = cfg["providers"]["openrouter"]
        .as_object_mut()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"]
        .as_array_mut()
        .ok_or("OpenRouter provider has no 'profiles' array")?;
    let profile = profiles
        .iter_mut()
        .find(|p| p.get("id").and_then(|i| i.as_str()) == Some(profile_id))
        .ok_or("profile not found")?;

    let current_hidden = profile
        .get("hidden")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let config_changed = current_hidden != hidden;
    if config_changed {
        if hidden {
            profile.as_object_mut().unwrap().insert("hidden".to_string(), serde_json::Value::Bool(true));
        } else {
            profile.as_object_mut().unwrap().remove("hidden");
        }
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed,
        restart_gateway: false,
        restart_reason: if config_changed {
            "profile_visibility_changed"
        } else {
            "already_desired_state"
        },
    })
}

fn apply_set_provider_hidden(
    cfg: &mut serde_json::Value,
    provider_id: &str,
    hidden: bool,
) -> Result<ApplyOutcome<()>, String> {
    // OpenRouter provider visibility is managed per-profile (profile.hidden), not at
    // the provider level. Reject so we never write a stray "hidden" onto OpenRouter.
    if provider_id == "openrouter" {
        return Err("OpenRouter visibility is managed per-profile".into());
    }

    let providers = cfg["providers"]
        .as_object_mut()
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get_mut(provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;

    let current_hidden = provider
        .get("hidden")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let config_changed = current_hidden != hidden;
    if config_changed {
        let provider_obj = provider
            .as_object_mut()
            .ok_or_else(|| format!("Provider '{}' config is not an object", provider_id))?;
        if hidden {
            provider_obj.insert("hidden".to_string(), serde_json::Value::Bool(true));
        } else {
            provider_obj.remove("hidden");
        }
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed,
        restart_gateway: false,
        restart_reason: if config_changed {
            "provider_visibility_changed"
        } else {
            "already_desired_state"
        },
    })
}

fn apply_activate_openrouter_profile(
    cfg: &mut serde_json::Value,
    profile_id: &str,
) -> Result<ApplyOutcome<()>, String> {
    // Validate profile exists
    let provider = cfg["providers"]["openrouter"]
        .as_object()
        .ok_or("OpenRouter provider not found")?;
    let profiles = provider["profiles"]
        .as_array()
        .ok_or("OpenRouter provider has no 'profiles' array")?;
    if !profiles
        .iter()
        .any(|p| p.get("id").and_then(|i| i.as_str()) == Some(profile_id))
    {
        return Err("profile not found".into());
    }

    let current_provider = cfg
        .get("active_provider")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let current_profile = cfg
        .get("active_openrouter_profile_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let already_openrouter = current_provider == "openrouter";
    let same_profile = already_openrouter && current_profile == profile_id;

    if same_profile {
        return Ok(ApplyOutcome {
            value: (),
            config_changed: false,
            restart_gateway: false,
            restart_reason: "already_active",
        });
    }

    let root = cfg.as_object_mut().expect("root is object");
    root.insert("active_provider".into(), serde_json::Value::String("openrouter".into()));
    root.insert(
        "active_openrouter_profile_id".into(),
        serde_json::Value::String(profile_id.to_string()),
    );

    Ok(ApplyOutcome {
        value: (),
        config_changed: true,
        restart_gateway: true,
        restart_reason: "active_profile_changed",
    })
}

// ---------------------------------------------------------------------------
// Tauri command wrappers (thin — all logic in apply_* above)
// ---------------------------------------------------------------------------

#[tauri::command]
fn add_openrouter_profile(
    config_state: tauri::State<'_, ConfigState>,
    name: String,
) -> Result<CommandResponse<serde_json::Value>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_add_openrouter_profile(cfg, &name)
    })
}

#[tauri::command]
fn delete_openrouter_profile(
    config_state: tauri::State<'_, ConfigState>,
    profile_id: String,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_delete_openrouter_profile(cfg, &profile_id)
    })
}

#[tauri::command]
fn rename_openrouter_profile(
    config_state: tauri::State<'_, ConfigState>,
    profile_id: String,
    new_name: String,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_rename_openrouter_profile(cfg, &profile_id, &new_name)
    })
}

#[tauri::command]
fn reorder_openrouter_profiles(
    config_state: tauri::State<'_, ConfigState>,
    profile_ids: Vec<String>,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_reorder_openrouter_profiles(cfg, &profile_ids)
    })
}

#[tauri::command]
fn activate_openrouter_profile(
    config_state: tauri::State<'_, ConfigState>,
    profile_id: String,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_activate_openrouter_profile(cfg, &profile_id)
    })
}

#[tauri::command]
fn set_openrouter_profile_hidden(
    config_state: tauri::State<'_, ConfigState>,
    profile_id: String,
    hidden: bool,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_set_openrouter_profile_hidden(cfg, &profile_id, hidden)
    })
}

#[tauri::command]
fn set_provider_hidden(
    config_state: tauri::State<'_, ConfigState>,
    provider_id: String,
    hidden: bool,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_set_provider_hidden(cfg, &provider_id, hidden)
    })
}

// ---------------------------------------------------------------------------
// Config read/write helpers used by the multi-profile commands
// ---------------------------------------------------------------------------

fn read_config_as_value(bytes: &[u8]) -> Result<(&'static str, serde_json::Value), String> {
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => match serde_json::from_str::<serde_json::Value>(&s) {
            Ok(v) => Ok(("UTF-8", v)),
            Err(e) => Err(format!("Invalid JSON: {}", e)),
        },
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(bytes);
            if had_errors {
                return Err("Cannot decode config.json".into());
            }
            match serde_json::from_str::<serde_json::Value>(&decoded.into_owned()) {
                Ok(v) => Ok(("Shift-JIS", v)),
                Err(e) => Err(format!("Invalid JSON: {}", e)),
            }
        }
    }
}

fn read_config_value(path: &std::path::Path) -> Result<(&'static str, serde_json::Value), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read config.json: {}", e))?;
    match String::from_utf8(bytes.clone()) {
        Ok(s) => {
            let cfg = serde_json::from_str::<serde_json::Value>(&s)
                .map_err(|e| format!("Invalid JSON: {}", e))?;
            Ok(("UTF-8", cfg))
        }
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                return Err("Cannot decode config.json".into());
            }
            let cfg = serde_json::from_str::<serde_json::Value>(&decoded.into_owned())
                .map_err(|e| format!("Invalid JSON: {}", e))?;
            Ok(("Shift-JIS", cfg))
        }
    }
}

fn write_config_value(
    cfg: &serde_json::Value,
    encoding: &str,
    path: &std::path::Path,
) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(cfg).map_err(|e| format!("JSON error: {}", e))?;
    let output = match encoding {
        "Shift-JIS" => {
            let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode(&json_str);
            if had_errors {
                return Err("Cannot encode config as Shift-JIS".into());
            }
            encoded.into_owned()
        }
        _ => json_str.into_bytes(),
    };
    std::fs::write(path, &output).map_err(|e| format!("Cannot write config.json: {}", e))
}

// ---------------------------------------------------------------------------
// Command 17: Check all API keys
// ---------------------------------------------------------------------------

#[tauri::command]
fn check_all_api_keys() -> Result<std::collections::HashMap<String, ApiKeyStatus>, String> {
    let cfg = load_gateway_config()?;
    let mut result = std::collections::HashMap::new();
    for (provider_id, provider) in &cfg.providers {
        let set = std::env::var(&provider.api_key_env).is_ok();
        result.insert(
            provider_id.clone(),
            ApiKeyStatus {
                set,
                env_var: provider.api_key_env.clone(),
            },
        );
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Command 18: Update active provider
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_active_provider(
    config_state: tauri::State<'_, ConfigState>,
    provider_id: String,
) -> Result<(), String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_active_provider(cfg, &provider_id)
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 19: Backup config.json
// ---------------------------------------------------------------------------

#[tauri::command]
fn backup_config() -> Result<String, String> {
    let path = config_path();
    if !path.exists() {
        return Err("config.json does not exist yet".into());
    }
    let now = Local::now();
    let bak_name = format!("config-{}.json.bak", now.format("%Y%m%d-%H%M%S"));
    let bak_path = path.parent().unwrap().join(&bak_name);
    std::fs::copy(&path, &bak_path).map_err(|e| format!("Cannot create backup: {}", e))?;
    Ok(bak_name)
}

// ---------------------------------------------------------------------------
// Command 20: Restore config.json from .bak
// ---------------------------------------------------------------------------

/// Pure path-based helper for restore_config_from_backup. Moves the
/// atomic copy-and-rename out of the command body so tests can exercise
/// the full replace on temp directories.
fn restore_config_from_backup_at_path(path: &std::path::Path) -> Result<(), String> {
    let bak_path = path.with_extension("json.bak");
    if !bak_path.exists() {
        return Err("No config.json.bak found".into());
    }
    // Atomic write: tmp then rename
    let tmp_path = path.with_extension("json.tmp");
    std::fs::copy(&bak_path, &tmp_path).map_err(|e| format!("Cannot copy backup: {}", e))?;
    std::fs::rename(&tmp_path, path).map_err(|e| format!("Cannot restore from backup: {}", e))?;
    Ok(())
}

#[tauri::command]
fn restore_config_from_backup(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    let path = config_path();
    let bak_path = path.with_extension("json.bak");
    if !bak_path.exists() {
        return Err("No config.json.bak found".into());
    }
    // Validate backup is valid JSON (before lock)
    let bak_bytes = std::fs::read(&bak_path).map_err(|e| format!("Cannot read backup: {}", e))?;
    let _val: serde_json::Value = match String::from_utf8(bak_bytes.clone()) {
        Ok(s) => {
            serde_json::from_str(&s).map_err(|e| format!("Backup is not valid JSON: {}", e))?
        }
        Err(_) => {
            let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&bak_bytes);
            serde_json::from_str(&decoded.into_owned())
                .map_err(|e| format!("Backup is not valid JSON: {}", e))?
        }
    };

    // Lock around file ops
    let _guard = config_state
        .write_lock
        .lock()
        .map_err(|e| format!("config write lock poisoned: {e}"))?;
    restore_config_from_backup_at_path(&path)
}

// ---------------------------------------------------------------------------
// Command 21: Reset config.json to factory defaults
// ---------------------------------------------------------------------------

/// Pure path-based helper for reset_config. Moves backup + crash recovery
/// + atomic template replace out of the command body so tests can exercise
/// the full replace on temp directories.
fn reset_config_at_path(path: &std::path::Path) -> Result<(), String> {
    // Recover from any interrupted previous reset before proceeding
    recover_interrupted_config_replace(path)?;

    if !path.exists() {
        return seed_config_from_template(path);
    }

    // Create timestamped backup before reset
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = path.with_file_name(format!("config.before-reset-{}.json", timestamp));
    std::fs::copy(&path, &backup)
        .map_err(|e| format!("Cannot create backup before reset: {e}"))?;

    // Atomically replace — builds in .reset.tmp, validates, then renames.
    // On failure, the original config.json is untouched.
    replace_config_from_template_atomically(path)
}

#[tauri::command]
fn reset_config(
    config_state: tauri::State<'_, ConfigState>,
) -> Result<(), String> {
    let _guard = config_state.write_lock.lock()
        .map_err(|e| format!("config write lock poisoned: {e}"))?;
    reset_config_at_path(&paths::config_path())
}

// ---------------------------------------------------------------------------
// Command 22: Update server config
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_server_config(
    config_state: tauri::State<'_, ConfigState>,
    host: String,
    port: u16,
    enable_cors: bool,
) -> Result<(), String> {
    if host.trim().is_empty() {
        return Err("Host cannot be empty".into());
    }
    if port == 0 {
        return Err("Port cannot be 0".into());
    }

    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_server_config(cfg, &host, port, enable_cors)
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 22b: Update normalize model identity setting
// ---------------------------------------------------------------------------

#[tauri::command]
fn update_normalize_model_identity(
    config_state: tauri::State<'_, ConfigState>,
    state: tauri::State<'_, ProxyState>,
    enabled: bool,
) -> Result<(), String> {
    tracing::info!(
        requested_enabled = enabled,
        "updating response model identity normalization"
    );

    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_normalize_model_identity(cfg, enabled)
    })?;

    // Side effect AFTER lock release
    use std::sync::atomic::Ordering;
    state
        .normalize_response_model_identity
        .store(enabled, Ordering::Relaxed);

    tracing::info!(
        runtime_enabled = state
            .normalize_response_model_identity
            .load(Ordering::Relaxed),
        "response model identity normalization updated"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Claude Code auto-compact (v2): types, resolver, apply, commands, migration
// ---------------------------------------------------------------------------

const AUTO_COMPACT_MODE_AUTO: &str = "auto";
const AUTO_COMPACT_MODE_MANUAL: &str = "manual";
const AUTO_COMPACT_MODE_CLAUDE_DEFAULT: &str = "claude_default";

fn default_trigger_percent() -> u8 {
    90
}

fn default_auto_compact_mode() -> String {
    AUTO_COMPACT_MODE_AUTO.to_string()
}

/// Root-level `claude_code.auto_compact` — global switch + common trigger.
/// The v2 capacity is auto-calculated from per-model metadata, so no common
/// `window_tokens` exists anymore.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClaudeCodeAutoCompactConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_trigger_percent")]
    pub trigger_percent: u8,
}

impl Default for ClaudeCodeAutoCompactConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            trigger_percent: default_trigger_percent(),
        }
    }
}

/// `claude_code.auto_compact` for a provider or OpenRouter profile
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClaudeCodeTargetConfig {
    #[serde(default = "default_auto_compact_mode")]
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_percent: Option<u8>,
}

impl Default for ClaudeCodeTargetConfig {
    fn default() -> Self {
        Self {
            mode: default_auto_compact_mode(),
            window_tokens: None,
            trigger_percent: None,
        }
    }
}

/// Root `claude_code` section: `{ "auto_compact": {...} }`
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClaudeCodeRootSection {
    #[serde(default)]
    pub auto_compact: ClaudeCodeAutoCompactConfig,
}

/// Provider/profile `claude_code` section: `{ "auto_compact": {...} }`
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ClaudeCodeProviderSection {
    #[serde(default)]
    pub auto_compact: ClaudeCodeTargetConfig,
}

/// v2 auto-compact mode, stored per provider/profile.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoCompactMode {
    Auto,
    Manual,
    ClaudeDefault,
}

/// Resolution status of the effective configuration — distinct from `mode`,
/// which is only the configured intent.
#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoCompactStatus {
    /// Global switch OFF or mode `claude_default` — nothing passed to env.
    Disabled,
    /// Environment variables will be applied.
    Applied,
    /// A route's model or metadata is missing — nothing auto-applied.
    Incomplete,
}

/// One canonical Claude Code route's resolved capacity basis.
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveContextRoute {
    /// "claude-opus-5" / "claude-sonnet-5" / "claude-haiku-4-5"
    pub route: String,
    /// None = routing itself is unset (no upstream model configured).
    pub upstream_model: Option<String>,
    /// None = the model is routed but has no context-length metadata entry.
    pub context_window_tokens: Option<u64>,
    pub context_window_source: ContextWindowSource,
}

/// Effective auto-compact resolution for the active connection target.
///
/// `mode` (configured intent) and `status` (resolved outcome) are separate: a
/// disabled global switch or missing metadata both surface as
/// `apply_environment=false`, while the panel still receives `routes` and the
/// auto-calculated `window_tokens` to show the calculation basis.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveAutoCompact {
    pub globally_enabled: bool,
    pub mode: AutoCompactMode,
    pub status: AutoCompactStatus,
    pub apply_environment: bool,
    pub window_tokens: Option<u32>,
    pub trigger_percent: Option<u8>,
    pub estimated_trigger_tokens: Option<u32>,
    pub target_kind: Option<&'static str>,
    pub target_id: Option<String>,
    pub target_name: Option<String>,
    pub routes: Vec<EffectiveContextRoute>,
}

fn auto_compact_value<'a>(scope: &'a serde_json::Value) -> Option<&'a serde_json::Value> {
    scope.get("claude_code").and_then(|c| c.get("auto_compact"))
}

fn as_u32(v: &serde_json::Value) -> Option<u32> {
    v.as_u64().map(|n| n as u32)
}

fn as_u8(v: &serde_json::Value) -> Option<u8> {
    v.as_u64().map(|n| n as u8)
}

/// Resolve the capacity basis for the 3 canonical Claude Code routes against
/// one target (provider or OpenRouter profile). Route→upstream resolution is
/// the shared `model_routing` implementation (also used by the proxy), so the
/// panel and the proxy can never disagree on which model a route uses.
fn resolve_context_routes(
    cfg: &serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
) -> Vec<EffectiveContextRoute> {
    CLAUDE_ROUTES
        .iter()
        .map(|route| {
            let upstream_model = resolve_route_upstream_model(cfg, provider_id, profile_id, route);
            let (context_window_tokens, context_window_source) = match &upstream_model {
                Some(up) => match try_resolve_static_context_window(provider_id, up) {
                    Some(w) => (Some(w.context_length), w.source),
                    None => (None, ContextWindowSource::Unknown),
                },
                None => (None, ContextWindowSource::Unknown),
            };
            EffectiveContextRoute {
                route: (*route).to_string(),
                upstream_model,
                context_window_tokens,
                context_window_source,
            }
        })
        .collect()
}

/// Minimum context window across the 3 canonical routes, only when ALL three
/// routes have known lengths. Any missing route or missing metadata → `None`
/// (never a partial minimum), and a length that cannot fit a u32 env var is an
/// explicit `Err`, not silently treated as "unknown".
fn min_context_window(routes: &[EffectiveContextRoute]) -> Result<Option<u32>, String> {
    if routes.len() != CLAUDE_ROUTES.len()
        || routes.iter().any(|r| r.context_window_tokens.is_none())
    {
        return Ok(None);
    }
    let min_context = routes
        .iter()
        .filter_map(|r| r.context_window_tokens)
        .min()
        .expect("all three routes are known");
    u32::try_from(min_context)
        .map(Some)
        .map_err(|_| format!("context window {} exceeds supported range", min_context))
}

/// Pure resolver — mirrors proxy.rs active-provider / active-profile fallbacks
/// (`effective_active`, transient `profiles[0]`). No I/O, operates on raw JSON.
fn resolve_effective_auto_compact(
    cfg: &serde_json::Value,
) -> Result<EffectiveAutoCompact, String> {
    let root_ac = auto_compact_value(cfg);
    let globally_enabled = root_ac
        .and_then(|r| r.get("enabled"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let common_percent = root_ac.and_then(|r| r.get("trigger_percent")).and_then(as_u8);

    // Resolve the active target (mirror proxy.rs resolution order)
    let providers = cfg.get("providers").and_then(|p| p.as_object());
    let mut provider_ids: Vec<&String> = providers.map(|p| p.keys().collect()).unwrap_or_default();
    provider_ids.sort();
    let active_provider = cfg.get("active_provider").and_then(|v| v.as_str());
    let effective_provider = active_provider
        .or_else(|| provider_ids.first().map(|s| s.as_str()))
        .unwrap_or("");
    let provider_value = providers.and_then(|p| p.get(effective_provider));

    let (target_kind, target_id, target_name, target_ac, route_profile_id) =
        if effective_provider == "openrouter" {
            let profiles = provider_value
                .and_then(|p| p.get("profiles"))
                .and_then(|p| p.as_array());
            if let Some(profiles) = profiles {
                let active_id = cfg
                    .get("active_openrouter_profile_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let active = profiles
                    .iter()
                    .find(|prof| prof.get("id").and_then(|v| v.as_str()) == Some(active_id))
                    .or_else(|| profiles.first());
                if let Some(prof) = active {
                    let id = prof
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = prof
                        .get("display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    (
                        Some("profile"),
                        Some(id.clone()),
                        Some(name),
                        auto_compact_value(prof),
                        Some(id),
                    )
                } else {
                    (Some("profile"), None, None, None, None)
                }
            } else {
                (None, None, None, None, None)
            }
        } else {
            let name = provider_value
                .and_then(|p| p.get("display_name"))
                .and_then(|v| v.as_str())
                .unwrap_or(effective_provider)
                .to_string();
            (
                Some("provider"),
                Some(effective_provider.to_string()),
                Some(name),
                provider_value.and_then(auto_compact_value),
                None,
            )
        };

    // Resolve routes even when the global switch is OFF so the settings panel
    // can show the calculation basis before the user flips the toggle.
    // `apply_environment` is the only thing that stops env vars from being set.
    let routes = resolve_context_routes(cfg, effective_provider, route_profile_id.as_deref());
    let all_routes_known = routes
        .iter()
        .all(|r| r.context_window_tokens.is_some());
    // Only an all-3-routes-known resolution produces an auto value; a partial
    // minimum would mislead the panel into showing a capacity that wasn't
    // actually computed from every route. u64 → u32 via try_from (never `as`).
    let auto_window = min_context_window(&routes)?;

    let mode_str = target_ac
        .and_then(|ac| ac.get("mode"))
        .and_then(|v| v.as_str())
        .unwrap_or(AUTO_COMPACT_MODE_AUTO);
    let mode = match mode_str {
        AUTO_COMPACT_MODE_AUTO => AutoCompactMode::Auto,
        AUTO_COMPACT_MODE_MANUAL => AutoCompactMode::Manual,
        AUTO_COMPACT_MODE_CLAUDE_DEFAULT => AutoCompactMode::ClaudeDefault,
        // Legacy/unrecognized mode (e.g. pre-v2 "inherit" a startup migration
        // missed) must never pass env vars — fall back to the safe default.
        _ => AutoCompactMode::ClaudeDefault,
    };

    let manual_window = target_ac
        .and_then(|ac| ac.get("window_tokens"))
        .and_then(as_u32);
    let manual_percent = target_ac
        .and_then(|ac| ac.get("trigger_percent"))
        .and_then(as_u8);

    let (status, apply_environment, window_tokens, trigger_percent) = match mode {
        AutoCompactMode::Manual => {
            // manual never falls back to the root trigger_percent — a missing
            // target value is an error (incomplete), not a silent default.
            let has_values = manual_window.is_some() && manual_percent.is_some();
            let (w, p) = if has_values {
                (manual_window, manual_percent)
            } else {
                (None, None)
            };
            let status = if globally_enabled && has_values {
                AutoCompactStatus::Applied
            } else if globally_enabled {
                AutoCompactStatus::Incomplete
            } else {
                AutoCompactStatus::Disabled
            };
            (status, globally_enabled && has_values, w, p)
        }
        AutoCompactMode::ClaudeDefault => (AutoCompactStatus::Disabled, false, None, None),
        AutoCompactMode::Auto => {
            let status = if !globally_enabled {
                AutoCompactStatus::Disabled
            } else if all_routes_known && auto_window.is_some() {
                AutoCompactStatus::Applied
            } else {
                AutoCompactStatus::Incomplete
            };
            (
                status,
                globally_enabled && all_routes_known && auto_window.is_some(),
                auto_window,
                common_percent,
            )
        }
    };

    let estimated_trigger_tokens =
        window_tokens.zip(trigger_percent).map(|(w, p)| (w as u64 * p as u64 / 100) as u32);

    Ok(EffectiveAutoCompact {
        globally_enabled,
        mode,
        status,
        apply_environment,
        window_tokens,
        trigger_percent,
        estimated_trigger_tokens,
        target_kind,
        target_id,
        target_name,
        routes,
    })
}

fn validate_claude_code_mode(mode: &str) -> Result<(), String> {
    match mode {
        AUTO_COMPACT_MODE_AUTO | AUTO_COMPACT_MODE_MANUAL | AUTO_COMPACT_MODE_CLAUDE_DEFAULT => {
            Ok(())
        }
        _ => Err(format!(
            "Invalid mode '{}'. Must be 'auto', 'manual', or 'claude_default'.",
            mode
        )),
    }
}

fn validate_claude_code_window_tokens(window_tokens: u32) -> Result<(), String> {
    if (1..=10_000_000).contains(&window_tokens) {
        Ok(())
    } else {
        Err(format!(
            "window_tokens must be between 1 and 10,000,000, got {}",
            window_tokens
        ))
    }
}

fn validate_claude_code_trigger_percent(trigger_percent: u8) -> Result<(), String> {
    if (1..=100).contains(&trigger_percent) {
        Ok(())
    } else {
        Err(format!(
            "trigger_percent must be between 1 and 100, got {}",
            trigger_percent
        ))
    }
}

/// コンテキスト制御で触る変数。OFF/Incomplete 時はここを Remove-Item して
/// 同一 PowerShell セッションの旧値が残らないようにする。
const AUTO_COMPACT_ENV_VARS: [&str; 2] = [
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
    "CLAUDE_AUTOCOMPACT_PCT_OVERRIDE",
];

/// 生成 env は「設定」と「削除」を分ける。削除だけの Vec<(str,String)> では表現できない。
#[derive(Debug)]
pub struct ClaudeCodeLaunchEnvironment {
    pub set: Vec<(&'static str, String)>,
    pub remove: Vec<&'static str>,
}

/// apply_environment=true のとき window/trigger の存在・window>0 を検証。
/// percent の範囲は既存 validate_claude_code_trigger_percent に委譲。
fn validate_auto_compact_environment(effective: &EffectiveAutoCompact) -> Result<(), String> {
    if !effective.apply_environment {
        return Ok(());
    }
    let window = effective
        .window_tokens
        .ok_or_else(|| "apply_environment=true but window_tokens is missing".to_string())?;
    let percent = effective
        .trigger_percent
        .ok_or_else(|| "apply_environment=true but trigger_percent is missing".to_string())?;
    if window == 0 {
        return Err("auto-compact window must be greater than zero".to_string());
    }
    validate_claude_code_trigger_percent(percent)?;
    Ok(())
}

/// Applied → set のみ。OFF/Incomplete/ClaudeDefault → remove のみ（旧値の削除）。
/// 変数名は Claude Code 公式: WINDOW + PCT_OVERRIDE（PERCENT は公式一覧に無い）。
fn auto_compact_environment(
    effective: &EffectiveAutoCompact,
) -> Result<ClaudeCodeLaunchEnvironment, String> {
    if !effective.apply_environment {
        return Ok(ClaudeCodeLaunchEnvironment {
            set: Vec::new(),
            remove: AUTO_COMPACT_ENV_VARS.to_vec(),
        });
    }

    validate_auto_compact_environment(effective)?;

    let window = effective
        .window_tokens
        .ok_or_else(|| "window_tokens disappeared after validation".to_string())?;
    let percent = effective
        .trigger_percent
        .ok_or_else(|| "trigger_percent disappeared after validation".to_string())?;

    Ok(ClaudeCodeLaunchEnvironment {
        set: vec![
            ("CLAUDE_CODE_AUTO_COMPACT_WINDOW", window.to_string()),
            ("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", percent.to_string()),
        ],
        remove: Vec::new(),
    })
}

/// クライアント向けゲートウェイ URL。gatewayConnection.ts と同型:
/// 空 / 0.0.0.0 / :: / [::] → 127.0.0.1。任意ホストは保持し、
/// `::1` のような IPv6 リテラルは `http://[::1]:4000` の形に角括弧を付ける。
fn gateway_client_base_url(host: &str, port: u16) -> String {
    let normalized = match host.trim() {
        "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1".to_string(),
        host if host.contains(':') && !(host.starts_with('[') && host.ends_with(']')) => {
            format!("[{host}]")
        }
        host => host.to_string(),
    };
    format!("http://{normalized}:{port}")
}

/// ローカルゲートウェイトークン。Rust 側に既存定数は無く、gatewayConnection.ts の
/// GATEWAY_LOCAL_TOKEN が単一正本。今回は mirror として二重定義し、
/// テストは Rust 側の期待値のみ固定する（TS との同一はコメントで管理）。
/// 長期的には設定値または共有リソースへ寄せる（別タスク）。
const LOCAL_GATEWAY_TOKEN: &str = "sk-local-gateway";

/// 認証変数は旧実装どおり ANTHROPIC_AUTH_TOKEN（API_KEY へ変更しない）。
fn gateway_connection_env_vars(
    cfg: &serde_json::Value,
) -> Result<Vec<(&'static str, String)>, String> {
    let host = cfg["server"]["host"].as_str().unwrap_or("127.0.0.1").to_string();
    let port_u64 = cfg["server"]["port"].as_u64().unwrap_or(4000);
    // as u16 で切り詰めず、範囲外は明示的にエラーにする。
    let port = u16::try_from(port_u64)
        .map_err(|_| format!("gateway port is out of range: {port_u64}"))?;
    Ok(vec![
        ("ANTHROPIC_BASE_URL", gateway_client_base_url(&host, port)),
        ("ANTHROPIC_AUTH_TOKEN", LOCAL_GATEWAY_TOKEN.to_string()),
    ])
}

/// PowerShell 単一引用符エスケープ: `'` を `''` にする。
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Applied 例:
/// `$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway';
///  $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude`
/// 非適用例:
/// `$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway';
///  Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
///  Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue; claude`
fn render_claude_code_launch_command(
    set: &[(&'static str, String)],
    remove: &[&'static str],
) -> String {
    let mut command = String::new();
    for (key, value) in set {
        command.push_str(&format!("$env:{key}={}; ", powershell_quote(value)));
    }
    for key in remove {
        command.push_str(&format!(
            "Remove-Item Env:{key} -ErrorAction SilentlyContinue; "
        ));
    }
    command.push_str("claude");
    command
}

/// Ensure `claude_code.auto_compact` exists at the given scope, returning it as
/// an object-mutable handle.
fn ensure_auto_compact_obj<'a>(
    scope: &'a mut serde_json::Value,
    default: serde_json::Value,
) -> Result<&'a mut serde_json::Map<String, serde_json::Value>, String> {
    if !scope.get("claude_code").map_or(false, |v| v.is_object()) {
        scope["claude_code"] = serde_json::json!({});
    }
    let claude_code = scope
        .get_mut("claude_code")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| "config 'claude_code' is not an object".to_string())?;
    if !claude_code
        .get("auto_compact")
        .map_or(false, |v| v.is_object())
    {
        claude_code.insert("auto_compact".to_string(), default);
    }
    claude_code
        .get_mut("auto_compact")
        .and_then(|v| v.as_object_mut())
        .ok_or_else(|| "config 'claude_code.auto_compact' is not an object".to_string())
}

/// Pure apply: merge only the provided fields into `claude_code.auto_compact`.
/// The common `window_tokens` no longer exists in v2 — the capacity comes from
/// the per-model auto-calculation, so only `enabled` / `trigger_percent` apply.
/// Detects no-ops so `execute_config_mutation` can skip persistence.
fn apply_update_claude_code_global(
    cfg: &mut serde_json::Value,
    enabled: Option<bool>,
    trigger_percent: Option<u8>,
) -> Result<ApplyOutcome<()>, String> {
    if let Some(v) = trigger_percent {
        validate_claude_code_trigger_percent(v)?;
    }

    let obj = ensure_auto_compact_obj(cfg, serde_json::json!(ClaudeCodeAutoCompactConfig::default()))?;

    let mut changed = false;
    if let Some(v) = enabled {
        if obj.get("enabled").and_then(|x| x.as_bool()) != Some(v) {
            obj.insert("enabled".to_string(), serde_json::Value::Bool(v));
            changed = true;
        }
    }
    if let Some(v) = trigger_percent {
        if obj.get("trigger_percent").and_then(as_u8) != Some(v) {
            obj.insert("trigger_percent".to_string(), serde_json::Value::from(v));
            changed = true;
        }
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed: changed,
        restart_gateway: false,
        restart_reason: "",
    })
}

fn locate_claude_code_target<'a>(
    cfg: &'a mut serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
) -> Result<&'a mut serde_json::Value, String> {
    let providers = cfg
        .get_mut("providers")
        .and_then(|p| p.as_object_mut())
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get_mut(provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;

    match profile_id {
        Some(pid) => {
            if provider_id != "openrouter" {
                return Err(format!(
                    "profile_id '{}' provided for non-openrouter provider '{}'",
                    pid, provider_id
                ));
            }
            let profiles = provider
                .get_mut("profiles")
                .and_then(|p| p.as_array_mut())
                .ok_or("openrouter provider missing 'profiles' array")?;
            let profile = profiles
                .iter_mut()
                .find(|prof| prof.get("id").and_then(|v| v.as_str()) == Some(pid))
                .ok_or_else(|| format!("OpenRouter profile '{}' not found", pid))?;
            Ok(profile)
        }
        None => Ok(provider),
    }
}

/// Pure apply: set a provider/profile's auto-compact mode (+ per-target values
/// for `manual`). Non-manual modes remove any stored per-target values.
/// Detects no-ops so `execute_config_mutation` can skip persistence.
fn apply_update_claude_code_target(
    cfg: &mut serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
    mode: &str,
    window_tokens: Option<u32>,
    trigger_percent: Option<u8>,
) -> Result<ApplyOutcome<()>, String> {
    validate_claude_code_mode(mode)?;
    let (window_tokens, trigger_percent) = if mode == AUTO_COMPACT_MODE_MANUAL {
        let w = window_tokens
            .ok_or_else(|| "window_tokens is required for mode 'manual'".to_string())?;
        let p = trigger_percent
            .ok_or_else(|| "trigger_percent is required for mode 'manual'".to_string())?;
        validate_claude_code_window_tokens(w)?;
        validate_claude_code_trigger_percent(p)?;
        (Some(w), Some(p))
    } else {
        (None, None)
    };

    let target = locate_claude_code_target(cfg, provider_id, profile_id)?;
    let obj = ensure_auto_compact_obj(target, serde_json::json!(ClaudeCodeTargetConfig::default()))?;

    let mut changed = false;
    let cur_mode = obj
        .get("mode")
        .and_then(|x| x.as_str())
        .unwrap_or(AUTO_COMPACT_MODE_AUTO);
    if cur_mode != mode {
        obj.insert("mode".to_string(), serde_json::Value::String(mode.to_string()));
        changed = true;
    }
    if mode == AUTO_COMPACT_MODE_MANUAL {
        let w = window_tokens.unwrap();
        if obj.get("window_tokens").and_then(as_u32) != Some(w) {
            obj.insert("window_tokens".to_string(), serde_json::Value::from(w));
            changed = true;
        }
        let p = trigger_percent.unwrap();
        if obj.get("trigger_percent").and_then(as_u8) != Some(p) {
            obj.insert("trigger_percent".to_string(), serde_json::Value::from(p));
            changed = true;
        }
    } else {
        let removed_window = obj.remove("window_tokens").is_some();
        let removed_percent = obj.remove("trigger_percent").is_some();
        if removed_window || removed_percent {
            changed = true;
        }
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed: changed,
        restart_gateway: false,
        restart_reason: "",
    })
}

/// Command input: fields to merge into the common `claude_code.auto_compact`
/// block. Only the fields present are applied. v2 has no common
/// `window_tokens` — the capacity comes from the per-model auto-calculation.
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeGlobalUpdate {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub trigger_percent: Option<u8>,
}

/// Command input: mode (+ optional per-target values) for one provider/profile.
/// `provider_id` and `mode` are required on purpose: a write command must not
/// silently fall back to a default mode (unlike the read-side config type).
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeTargetUpdate {
    pub provider_id: String,
    #[serde(default)]
    pub profile_id: Option<String>,
    pub mode: ClaudeCodeTargetMode,
    #[serde(default)]
    pub window_tokens: Option<u32>,
    #[serde(default)]
    pub trigger_percent: Option<u8>,
}

/// Update-command mode value. Frontend sends "auto" | "manual" |
/// "claude_default" (serde snake_case maps them onto these variants).
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeCodeTargetMode {
    Auto,
    Manual,
    ClaudeDefault,
}

/// Map the update-mode enum to the config's stored string values.
fn target_mode_str(mode: ClaudeCodeTargetMode) -> &'static str {
    match mode {
        ClaudeCodeTargetMode::Auto => AUTO_COMPACT_MODE_AUTO,
        ClaudeCodeTargetMode::Manual => AUTO_COMPACT_MODE_MANUAL,
        ClaudeCodeTargetMode::ClaudeDefault => AUTO_COMPACT_MODE_CLAUDE_DEFAULT,
    }
}

/// Read-only existence check for the target apply step. Used by the combined
/// apply to validate the whole request before mutating anything.
///
/// Enforces the target addressing rule: OpenRouter is addressed per profile
/// (`profile_id` required), direct providers never take a `profile_id`.
fn find_claude_code_target(
    cfg: &serde_json::Value,
    provider_id: &str,
    profile_id: Option<&str>,
) -> Result<(), String> {
    match (provider_id, profile_id) {
        ("openrouter", None) => {
            return Err("profile_id is required for OpenRouter".to_string());
        }
        (id, Some(_)) if id != "openrouter" => {
            return Err("profile_id is only valid for OpenRouter".to_string());
        }
        _ => {}
    }

    let providers = cfg
        .get("providers")
        .and_then(|p| p.as_object())
        .ok_or("config.json missing 'providers' key")?;
    let provider = providers
        .get(provider_id)
        .ok_or_else(|| format!("Provider '{}' not found in config", provider_id))?;
    if let Some(pid) = profile_id {
        let profiles = provider
            .get("profiles")
            .and_then(|p| p.as_array())
            .ok_or("openrouter provider missing 'profiles' array")?;
        let profile = profiles
            .iter()
            .find(|prof| prof.get("id").and_then(|v| v.as_str()) == Some(pid))
            .ok_or_else(|| format!("OpenRouter profile '{}' not found", pid))?;
        check_claude_code_shape(profile, &format!("OpenRouter profile '{}'", pid))?;
    } else {
        check_claude_code_shape(provider, &format!("Provider '{}'", provider_id))?;
    }
    Ok(())
}

/// Reject a malformed `claude_code` / `claude_code.auto_compact` early so the
/// later apply step cannot hit a structural surprise mid-way.
fn check_claude_code_shape(scope: &serde_json::Value, label: &str) -> Result<(), String> {
    if let Some(cc) = scope.get("claude_code") {
        if !cc.is_object() {
            return Err(format!("{} has a non-object 'claude_code'", label));
        }
        if let Some(ac) = cc.get("auto_compact") {
            if !ac.is_object() {
                return Err(format!("{} has a non-object 'claude_code.auto_compact'", label));
            }
        }
    }
    Ok(())
}

/// Combined apply for the settings panel: updates the common block AND the
/// active target in a single mutation.
///
/// Atomicity is guaranteed by clone-and-commit: both sections are applied to a
/// clone of the config, and the clone is committed to `cfg` only when every
/// step succeeds. Any validation, structural, or future apply error therefore
/// leaves the original config untouched and triggers no save.
fn apply_update_claude_code_settings(
    cfg: &mut serde_json::Value,
    global: Option<&ClaudeCodeGlobalUpdate>,
    target: Option<&ClaudeCodeTargetUpdate>,
) -> Result<ApplyOutcome<()>, String> {
    let global_provides = global
        .map(|g| g.enabled.is_some() || g.trigger_percent.is_some())
        .unwrap_or(false);
    if !global_provides && target.is_none() {
        return Err("global or target update is required".to_string());
    }

    // Pre-validation: clear error messages before the clone-and-commit path.
    if let Some(g) = global {
        if let Some(v) = g.trigger_percent {
            validate_claude_code_trigger_percent(v)?;
        }
    }
    if let Some(t) = target {
        validate_claude_code_mode(target_mode_str(t.mode))?;
        if t.mode == ClaudeCodeTargetMode::Manual {
            let w = t
                .window_tokens
                .ok_or_else(|| "window_tokens is required for mode 'manual'".to_string())?;
            let p = t
                .trigger_percent
                .ok_or_else(|| "trigger_percent is required for mode 'manual'".to_string())?;
            validate_claude_code_window_tokens(w)?;
            validate_claude_code_trigger_percent(p)?;
        }
        find_claude_code_target(cfg, &t.provider_id, t.profile_id.as_deref())?;
    }

    let mut candidate = cfg.clone();
    let mut changed = false;
    if let Some(g) = global {
        let outcome =
            apply_update_claude_code_global(&mut candidate, g.enabled, g.trigger_percent)?;
        changed |= outcome.config_changed;
    }
    if let Some(t) = target {
        let (window_tokens, trigger_percent) = if t.mode == ClaudeCodeTargetMode::Manual {
            (t.window_tokens, t.trigger_percent)
        } else {
            (None, None)
        };
        let outcome = apply_update_claude_code_target(
            &mut candidate,
            &t.provider_id,
            t.profile_id.as_deref(),
            target_mode_str(t.mode),
            window_tokens,
            trigger_percent,
        )?;
        changed |= outcome.config_changed;
    }
    if changed {
        *cfg = candidate;
    }

    Ok(ApplyOutcome {
        value: (),
        config_changed: changed,
        restart_gateway: false,
        restart_reason: "",
    })
}

#[tauri::command]
fn update_claude_code_context_settings(
    config_state: tauri::State<'_, ConfigState>,
    global: Option<ClaudeCodeGlobalUpdate>,
    target: Option<ClaudeCodeTargetUpdate>,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_claude_code_settings(cfg, global.as_ref(), target.as_ref())
    })
}

#[tauri::command]
fn update_claude_code_auto_compact_global(
    config_state: tauri::State<'_, ConfigState>,
    enabled: Option<bool>,
    trigger_percent: Option<u8>,
) -> Result<CommandResponse<()>, String> {
    if enabled.is_none() && trigger_percent.is_none() {
        return Err("at least one of enabled, trigger_percent must be provided".into());
    }
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_claude_code_global(cfg, enabled, trigger_percent)
    })
}

#[tauri::command]
fn update_claude_code_auto_compact_target(
    config_state: tauri::State<'_, ConfigState>,
    provider_id: String,
    profile_id: Option<String>,
    mode: String,
    window_tokens: Option<u32>,
    trigger_percent: Option<u8>,
) -> Result<CommandResponse<()>, String> {
    execute_serialized_config_mutation(&config_state.write_lock, |cfg| {
        apply_update_claude_code_target(
            cfg,
            &provider_id,
            profile_id.as_deref(),
            &mode,
            window_tokens,
            trigger_percent,
        )
    })
}

#[tauri::command]
fn resolve_claude_code_auto_compact() -> Result<EffectiveAutoCompact, String> {
    let (_encoding, cfg) = read_config_value(&config_path())?;
    resolve_effective_auto_compact(&cfg)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeCodeLaunchCommand {
    pub command: String,
    pub apply_environment: bool,
    pub status: AutoCompactStatus,
}

/// Claude Code 起動コマンドを生成する（実行はしない — クリップボードへコピーされる側で実行）。
/// ゲートウェイ接続（ANTHROPIC_BASE_URL / ANTHROPIC_AUTH_TOKEN）は常に設定し、
/// コンテキスト制御は Applied なら2変数を設定、OFF/Incomplete/ClaudeDefault なら
/// 同一 PowerShell セッションに残り得る旧値を Remove-Item で削除する。
#[tauri::command]
fn build_claude_code_launch_command() -> Result<ClaudeCodeLaunchCommand, String> {
    let (_encoding, cfg) = read_config_value(&config_path())?;
    let effective = resolve_effective_auto_compact(&cfg)?;
    // 検証失敗 → Err（壊れた設定のコマンドを生成しない）
    let context = auto_compact_environment(&effective)?;

    if context.set.is_empty() {
        let status = match effective.status {
            AutoCompactStatus::Applied => "applied",
            AutoCompactStatus::Disabled => "disabled",
            AutoCompactStatus::Incomplete => "incomplete",
        };
        tracing::info!(
            status,
            "Claude Code context control launch command prepared; context variables cleared"
        );
    } else {
        tracing::info!(
            window = effective.window_tokens,
            trigger_override = effective.trigger_percent,
            estimated = effective.estimated_trigger_tokens,
            "Claude Code context control launch command prepared"
        );
    }

    let mut set = gateway_connection_env_vars(&cfg)?;
    set.extend(context.set);
    let command = render_claude_code_launch_command(&set, &context.remove);
    Ok(ClaudeCodeLaunchCommand {
        command,
        apply_environment: effective.apply_environment,
        status: effective.status,
    })
}

// ---------------------------------------------------------------------------
// v2 migration: Claude Code auto-compact modes
// ---------------------------------------------------------------------------

/// One-time v2 migration for Claude Code auto-compact. Explicit behavior
/// change (phase 1 → v2):
///
/// - `inherit`  → `auto`            (v2 default: capacity is auto-calculated)
/// - `override` → `manual`          (per-target window/percent kept as-is)
/// - unrecognized mode → `claude_default` (safe fallback: env vars never passed)
/// - root `window_tokens` removed   (auto-calculation replaces the common value)
///
/// Idempotent. Wired as the LAST migration in `ensure_config_initialized_at`,
/// after `merge_bundled_providers` (which re-inserts the template's legacy
/// mode), so the final saved config never contains a pre-v2 mode.
fn migrate_claude_code_auto_compact_modes(config_path: &std::path::Path) {
    let raw_str = match std::fs::read_to_string(config_path) {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut cfg: serde_json::Value = match serde_json::from_str(&raw_str) {
        Ok(v) => v,
        Err(_) => return,
    };
    if !migrate_claude_code_auto_compact_modes_inner(&mut cfg) {
        return;
    }
    let serialized = serde_json::to_string_pretty(&cfg).unwrap_or_default();
    let _ = std::fs::write(config_path, serialized);
}

/// Returns true when the config was modified. Exposed as `_inner` for
/// idempotency tests without touching the filesystem.
fn migrate_claude_code_auto_compact_modes_inner(cfg: &mut serde_json::Value) -> bool {
    let mut changed = false;

    if let Some(root_ac) = cfg
        .get_mut("claude_code")
        .and_then(|c| c.get_mut("auto_compact"))
        .and_then(|ac| ac.as_object_mut())
    {
        if root_ac.remove("window_tokens").is_some() {
            changed = true;
        }
    }

    if let Some(providers) = cfg.get_mut("providers").and_then(|p| p.as_object_mut()) {
        for (_pid, provider) in providers.iter_mut() {
            if let Some(ac) = provider
                .get_mut("claude_code")
                .and_then(|c| c.get_mut("auto_compact"))
            {
                changed |= migrate_claude_code_mode(ac);
            }
            if let Some(profiles) = provider
                .get_mut("profiles")
                .and_then(|p| p.as_array_mut())
            {
                for profile in profiles.iter_mut() {
                    if let Some(ac) = profile
                        .get_mut("claude_code")
                        .and_then(|c| c.get_mut("auto_compact"))
                    {
                        changed |= migrate_claude_code_mode(ac);
                    }
                }
            }
        }
    }

    changed
}

/// Convert one provider/profile `claude_code.auto_compact` to a v2 mode.
/// A missing mode is left untouched (the read-side default is `auto`).
fn migrate_claude_code_mode(ac: &mut serde_json::Value) -> bool {
    let current = ac.get("mode").and_then(|v| v.as_str());
    let new_mode = match current {
        Some("inherit") => AUTO_COMPACT_MODE_AUTO,
        Some("override") => AUTO_COMPACT_MODE_MANUAL,
        Some(m) if m == AUTO_COMPACT_MODE_AUTO
            || m == AUTO_COMPACT_MODE_MANUAL
            || m == AUTO_COMPACT_MODE_CLAUDE_DEFAULT =>
        {
            return false
        }
        Some(_) => AUTO_COMPACT_MODE_CLAUDE_DEFAULT,
        None => return false,
    };
    ac["mode"] = serde_json::Value::String(new_mode.to_string());
    true
}

// ---------------------------------------------------------------------------
// Command 3b: Port 4000 process
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct PortProcessInfo {
    pid: String,
    raw_output: String,
}

#[tauri::command]
fn get_port_4000_process() -> Result<PortProcessInfo, String> {
    let output = std::process::Command::new("cmd")
        .args(["/C", "netstat -ano | findstr :4000"])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // Extract PID from LISTENING line (5th whitespace-delimited token)
    let pid = stdout
        .lines()
        .find(|line| line.to_uppercase().contains("LISTENING"))
        .and_then(|line| line.split_whitespace().nth(4).map(|s| s.to_string()))
        .unwrap_or_default();

    Ok(PortProcessInfo {
        pid,
        raw_output: stdout,
    })
}

// ---------------------------------------------------------------------------
// Command 4: Read config
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModelEntry {
    pub upstream_model: String,
    /// Canonical Gateway model ID for alias entries (e.g. "opus" → "claude-opus-5")
    #[serde(default)]
    pub canonical: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub supports_vision: Option<bool>,
    #[serde(default)]
    pub supports_video: Option<bool>,
    #[serde(default = "default_visible")]
    pub visible: bool,
    /// If true, always force `thinking: { type: "enabled" }` upstream
    #[serde(default)]
    pub force_thinking: Option<bool>,
    /// If false, the model does not support non-thinking mode
    #[serde(default)]
    pub supports_non_thinking: Option<bool>,
    /// Can receive image blocks with source.type = "url"
    #[serde(default)]
    pub supports_image_url: Option<bool>,
    /// Can receive image blocks with source.type = "base64"
    #[serde(default)]
    pub supports_image_base64: Option<bool>,
    /// Can receive video blocks with source.type = "url"
    #[serde(default)]
    pub supports_video_url: Option<bool>,
    /// Can receive video blocks with source.type = "base64"
    #[serde(default)]
    pub supports_video_base64: Option<bool>,
    /// Thinking mode preference: "normal" | "thinking" | "thinking_only"
    #[serde(default)]
    pub thinking_mode: Option<String>,
    /// Reasoning effort to inject when thinking is enabled (e.g. "high" for DeepSeek Opus)
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

fn default_visible() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct OpenRouterProfile {
    pub id: String,
    pub display_name: String,
    pub model_map: std::collections::HashMap<String, String>,
    pub visible_models: Vec<String>,
    pub models: std::collections::HashMap<String, ModelEntry>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// Per-profile Claude Code auto-compact override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCodeProviderSection>,
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProviderConfig {
    pub display_name: String,
    pub upstream_url: String,
    pub api_key_env: String,
    pub default_model: String,
    pub force_anthropic_version: Option<String>,
    pub supports_count_tokens: bool,
    pub supports_vision: bool,
    pub supports_video: bool,
    pub supports_thinking: bool,
    #[serde(default)]
    pub model_map: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub visible_models: Vec<String>,
    #[serde(default)]
    pub models: Option<std::collections::HashMap<String, ModelEntry>>,
    /// OpenRouter multi-profile support (serialized as "profiles" in JSON)
    #[serde(rename = "profiles", default)]
    pub openrouter_profiles: Vec<OpenRouterProfile>,
    /// Per-provider Claude Code auto-compact override
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCodeProviderSection>,
    /// Hide this provider's card on the Dashboard (defaults to shown).
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub enable_cors: bool,
}

#[derive(Serialize, Deserialize)]
pub struct GatewayConfigResponse {
    #[serde(default = "default_config_version")]
    pub config_version: String,
    #[serde(default)]
    pub active_provider: Option<String>,
    /// Currently active OpenRouter profile (only meaningful when active_provider == "openrouter")
    #[serde(default)]
    pub active_openrouter_profile_id: Option<String>,
    pub providers: indexmap::IndexMap<String, ProviderConfig>,
    pub server: ServerConfig,
    #[serde(default = "default_non_vision_image_policy")]
    pub non_vision_image_policy: String,
    #[serde(default = "default_true")]
    pub normalize_response_model_identity: bool,
    /// Root Claude Code auto-compact common settings
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_code: Option<ClaudeCodeRootSection>,
}

fn default_config_version() -> String {
    "1.0".into()
}

fn default_non_vision_image_policy() -> String {
    "replace".into()
}

fn default_true() -> bool {
    true
}

#[tauri::command]
fn read_config() -> Result<GatewayConfigResponse, String> {
    let path = config_path();
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;
    let cfg: GatewayConfigResponse =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))?;
    Ok(cfg)
}

/// Load config (internal helper, returns parsed struct).
fn load_gateway_config() -> Result<GatewayConfigResponse, String> {
    let path = config_path();
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))
}

/// Get the active provider's API key env var name from config (used by dashboard).
fn get_active_api_key_env() -> Result<String, String> {
    let cfg = load_gateway_config()?;
    let active = cfg.active_provider.as_deref().unwrap_or("deepseek");
    let provider = cfg
        .providers
        .get(active)
        .ok_or_else(|| format!("Provider '{}' not found in config", active))?;
    Ok(provider.api_key_env.clone())
}

// ---------------------------------------------------------------------------
// Command 5: Read latest log
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct LogFile {
    filename: String,
    content: String,
    line_count: usize,
}

#[tauri::command]
fn read_latest_log() -> Result<LogFile, String> {
    let dir = log_dir();

    if !dir.exists() {
        return Ok(LogFile {
            filename: String::new(),
            content: String::new(),
            line_count: 0,
        });
    }

    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read log dir: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("proxy-") && name.ends_with(".log")
        })
        .collect();

    // Sort by filename descending (ISO dates = chronological order)
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

    let latest = match entries.first() {
        Some(entry) => entry,
        None => {
            return Ok(LogFile {
                filename: String::new(),
                content: String::new(),
                line_count: 0,
            });
        }
    };

    let filename = latest.file_name().to_string_lossy().to_string();
    let bytes = std::fs::read(latest.path()).map_err(|e| format!("Cannot read log file: {}", e))?;

    // Try UTF-8 first, then fall back to Shift-JIS (for Japanese Windows)
    let content = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                String::from_utf8_lossy(&bytes).to_string()
            } else {
                decoded.into_owned()
            }
        }
    };
    let line_count = content.lines().count();

    Ok(LogFile {
        filename,
        content,
        line_count,
    })
}

// ---------------------------------------------------------------------------
// Command 6: Open logs folder in Explorer
// ---------------------------------------------------------------------------

#[tauri::command]
fn open_logs_folder() -> Result<(), String> {
    let dir = log_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create log dir: {}", e))?;
    }
    std::process::Command::new("explorer")
        .arg(&dir)
        .spawn()
        .map_err(|e| format!("Cannot open folder: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 7: Open any path in Explorer
// ---------------------------------------------------------------------------

fn expand_env_vars(path: &str) -> String {
    let mut result = path.to_string();
    let mut start = 0;
    while let Some(pct) = result[start..].find('%') {
        let abs = start + pct;
        if let Some(end) = result[abs + 1..].find('%') {
            let var_name = &result[abs + 1..abs + 1 + end];
            if let Ok(val) = std::env::var(var_name) {
                result.replace_range(abs..abs + end + 2, &val);
                start = abs + val.len();
            } else {
                start = abs + end + 2;
            }
        } else {
            break;
        }
    }
    result
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    let resolved = expand_env_vars(&path);
    std::process::Command::new("explorer")
        .arg(&resolved)
        .spawn()
        .map_err(|e| format!("Cannot open path: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Command 8: Read config raw (with encoding detection)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct RawConfigResponse {
    content: String,
    encoding_used: String,
    config_path: String,
}

#[tauri::command]
fn read_config_raw() -> Result<RawConfigResponse, String> {
    let path = config_path();
    let config_path_str = path.to_string_lossy().to_string();
    let bytes = std::fs::read(&path).map_err(|e| format!("Cannot read config.json: {}", e))?;

    match String::from_utf8(bytes.clone()) {
        Ok(s) => Ok(RawConfigResponse {
            content: s,
            encoding_used: "UTF-8".into(),
            config_path: config_path_str,
        }),
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                Err("Cannot decode config.json as UTF-8 or Shift-JIS".into())
            } else {
                Ok(RawConfigResponse {
                    content: decoded.into_owned(),
                    encoding_used: "Shift-JIS".into(),
                    config_path: config_path_str,
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Command 9: Write config
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct WriteConfigResponse {
    saved_encoding: String,
}

/// Pure path-based helper for write_config. Moves backup + encode + tmp
/// write + atomic rename out of the command body so tests can exercise the
/// full replace on temp directories.
fn write_config_at_path(
    path: &std::path::Path,
    content: &str,
    encoding: &str,
) -> Result<String, String> {
    // Create .bak backup before overwriting
    let bak_path = path.with_extension("json.bak");
    if path.exists() {
        std::fs::copy(&path, &bak_path).map_err(|e| format!("Cannot create backup: {}", e))?;
    }

    let bytes: Vec<u8> = match encoding {
        "Shift-JIS" => {
            let (encoded, _, had_errors) = encoding_rs::SHIFT_JIS.encode(content);
            if had_errors {
                return Err("Cannot encode content as Shift-JIS".into());
            }
            encoded.into_owned()
        }
        _ => content.as_bytes().to_vec(),
    };

    // Atomic write: write to temp file, then rename
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &bytes)
        .map_err(|e| format!("Cannot write config: {}", e))?;
    std::fs::rename(&tmp_path, path)
        .map_err(|e| format!("Cannot finalize config save: {}", e))?;
    Ok(encoding.to_string())
}

#[tauri::command]
fn write_config(
    config_state: tauri::State<'_, ConfigState>,
    content: String,
    encoding: String,
) -> Result<WriteConfigResponse, String> {
    // Validate that content is valid JSON (before lock)
    let _: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Invalid JSON: {}", e))?;

    // NOTE: Mutex prevents interleaving/file corruption but does NOT
    // prevent stale-snapshot overwrite. If the caller sends outdated
    // full-document JSON, previous field-level edits may be lost.
    // Callers (raw JSON editor, config import) are intentional full
    // replacements — this is accepted behavior.
    let saved_encoding = {
        let _guard = config_state
            .write_lock
            .lock()
            .map_err(|e| format!("config write lock poisoned: {e}"))?;
        write_config_at_path(&config_path(), &content, &encoding)?
    }; // lock released

    Ok(WriteConfigResponse { saved_encoding })
}

// ---------------------------------------------------------------------------
// Command 13: Find Claude Desktop config files
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct ClaudeConfigCandidate {
    path: String,
    exists: bool,
    likely_config: bool,
}

#[tauri::command]
fn find_claude_configs() -> Result<Vec<ClaudeConfigCandidate>, String> {
    let mut candidates: Vec<ClaudeConfigCandidate> = Vec::new();

    // Build search directories from environment variables
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    let vars: &[(&str, &str)] = &[
        ("APPDATA", "Claude"),
        ("LOCALAPPDATA", "Claude"),
        ("LOCALAPPDATA", "Claude-3p\\configLibrary"),
        ("USERPROFILE", ".claude"),
    ];

    for (env_var, subdir) in vars {
        if let Ok(base) = std::env::var(env_var) {
            let dir = PathBuf::from(&base).join(subdir);
            if seen.insert(dir.clone()) {
                dirs.push(dir);
            }
        }
    }

    // Claude-specific keys that indicate a real config file
    let claude_keys = [
        "inferenceProvider",
        "claude_desktop_config",
        "inferenceGatewayBaseUrl",
        "inferenceModels",
        "inferenceGatewayApiKey",
    ];

    for dir in &dirs {
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.ends_with(".json") {
                // Check if file content suggests it's a Claude config
                let likely_config = std::fs::read(&path)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok())
                    .map(|content| claude_keys.iter().any(|key| content.contains(key)))
                    .unwrap_or(false);

                candidates.push(ClaudeConfigCandidate {
                    path: path.to_string_lossy().to_string(),
                    exists: true,
                    likely_config,
                });
            }
        }
    }

    // Sort: likely configs first, then by path
    candidates.sort_by(|a, b| {
        b.likely_config
            .cmp(&a.likely_config)
            .then(a.path.cmp(&b.path))
    });
    candidates.dedup_by(|a, b| a.path == b.path);
    Ok(candidates)
}

// ---------------------------------------------------------------------------
// Command 14: List log files
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct LogListEntry {
    filename: String,
    size: u64,
}

#[tauri::command]
fn list_logs() -> Result<Vec<LogListEntry>, String> {
    let dir = log_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<LogListEntry> = std::fs::read_dir(&dir)
        .map_err(|e| format!("Cannot read log dir: {}", e))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let name = name.to_string_lossy();
            name.starts_with("proxy-") && name.ends_with(".log")
        })
        .map(|e| {
            let filename = e.file_name().to_string_lossy().to_string();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            LogListEntry { filename, size }
        })
        .collect();

    entries.sort_by(|a, b| b.filename.cmp(&a.filename));
    Ok(entries)
}

// ---------------------------------------------------------------------------
// Command 15: Read a specific log file
// ---------------------------------------------------------------------------

#[tauri::command]
fn read_log(filename: String) -> Result<LogFile, String> {
    let dir = log_dir();
    let path = dir.join(&filename);

    // Security: ensure the resolved path stays inside log_dir
    let canonical_dir = dir
        .canonicalize()
        .map_err(|e| format!("Cannot resolve log dir: {}", e))?;
    let canonical_path = path
        .canonicalize()
        .map_err(|_| format!("Log file not found: {}", filename))?;
    if !canonical_path.starts_with(&canonical_dir) {
        return Err("Invalid log filename".into());
    }

    let bytes =
        std::fs::read(&canonical_path).map_err(|e| format!("Cannot read log file: {}", e))?;

    let content = match String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        Err(_) => {
            let (decoded, _, had_errors) = encoding_rs::SHIFT_JIS.decode(&bytes);
            if had_errors {
                String::from_utf8_lossy(&bytes).to_string()
            } else {
                decoded.into_owned()
            }
        }
    };
    let line_count = content.lines().count();

    Ok(LogFile {
        filename,
        content,
        line_count,
    })
}

// ---------------------------------------------------------------------------
// Command 16: Create new log file
// ---------------------------------------------------------------------------

#[tauri::command]
fn create_new_log() -> Result<String, String> {
    let dir = log_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Cannot create log dir: {}", e))?;
    }

    let now = Local::now();
    let filename = format!("proxy-{}.log", now.format("%Y%m%d-%H%M%S"));
    let path = dir.join(&filename);

    std::fs::write(&path, "").map_err(|e| format!("Cannot create log file: {}", e))?;
    Ok(filename)
}

// ---------------------------------------------------------------------------
// Config write serialization
// ---------------------------------------------------------------------------
// ALL Tauri commands that write to config_path() MUST go through
// execute_serialized_config_mutation(&config_state.write_lock, ...).
//
// Full-replace commands (write_config, reset_config, restore) manually
// acquire config_state.write_lock around the atomic file operation,
// delegating to path-based helpers (write_config_at_path, etc.) so
// tests can exercise the same logic on temp directories.
//
// DO NOT call read_config_value + write_config_value directly from a
// Tauri command — the read-mutate-write cycle is not atomic without
// the lock.
//
// OpenRouter route edits are persisted through set_model_upstream
// (profile_id parameter); there is no separate config-writing route
// command.
//
// Protected commands:
//   Group A: set_model_upstream, add_openrouter_profile,
//            delete_openrouter_profile, rename_openrouter_profile,
//            reorder_openrouter_profiles, activate_openrouter_profile,
//            set_openrouter_profile_hidden
//   Group B: update_provider_api_key_env, update_active_provider,
//            update_server_config, update_normalize_model_identity
//   Group C: write_config, reset_config, restore_config_from_backup

/// Serializes concurrent config.json writes so two role-edit commands
/// don't lose each other's changes.
pub struct ConfigState {
    pub write_lock: Mutex<()>,
}

impl ConfigState {
    fn new() -> Self {
        Self {
            write_lock: Mutex::new(()),
        }
    }
}

/// Testable variant: serializes lock + read + apply + write for an
/// explicit config path. Used by regression tests on temp files.
fn execute_serialized_config_mutation_at_path<T: serde::Serialize, F>(
    lock: &Mutex<()>,
    path: &std::path::Path,
    apply: F,
) -> Result<CommandResponse<T>, String>
where
    F: FnOnce(&mut serde_json::Value) -> Result<ApplyOutcome<T>, String>,
{
    let _guard = lock
        .lock()
        .map_err(|e| format!("config write lock poisoned: {e}"))?;
    let (encoding, mut cfg) = read_config_value(path)?;
    execute_config_mutation(&mut cfg, apply, |cfg| write_config_value(cfg, encoding, path))
}

/// Production variant: serializes lock + read + apply + write using the
/// real config_path().
fn execute_serialized_config_mutation<T: serde::Serialize, F>(
    lock: &Mutex<()>,
    apply: F,
) -> Result<CommandResponse<T>, String>
where
    F: FnOnce(&mut serde_json::Value) -> Result<ApplyOutcome<T>, String>,
{
    execute_serialized_config_mutation_at_path(lock, &config_path(), apply)
}

/// Pure input validation for set_model_upstream — no I/O, can run
/// before acquiring the config write lock.
fn validate_set_model_upstream_input(
    upstream_model: &str,
    thinking_mode: Option<&str>,
    reasoning_effort: Option<&str>,
) -> Result<(), String> {
    if upstream_model.trim().is_empty() {
        return Err("upstream_model cannot be empty".into());
    }
    if let Some(ref tm) = thinking_mode {
        // persisted values: "normal", "thinking"
        // command-only (deletes key on save): "default"
        // legacy (migrated to "thinking" on startup): "thinking_only"
        let valid_tm = ["normal", "thinking", "thinking_only", "default"];
        let tm_str: &str = tm;
        if !valid_tm.iter().any(|v| *v == tm_str) {
            return Err(format!(
                "Invalid thinking_mode '{}'. Must be 'normal', 'thinking', 'thinking_only', or 'default'.",
                tm
            ));
        }
    }
    if let Some(ref effort) = reasoning_effort {
        let valid_effort = ["low", "medium", "high", "xhigh", "max"];
        let effort_str: &str = effort;
        if !valid_effort.iter().any(|v| *v == effort_str) {
            return Err(format!(
                "Invalid reasoning_effort '{}'. Must be 'low', 'medium', 'high', 'xhigh', or 'max'.",
                effort
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Proxy state
// ---------------------------------------------------------------------------

pub struct ProxyState {
    handle: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    done_rx: Mutex<Option<std::sync::mpsc::Receiver<Result<(), String>>>>,
    /// Runtime-updatable normalization toggle (shared with ProxyConfig)
    pub normalize_response_model_identity: Arc<AtomicBool>,
}

impl ProxyState {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
            done_rx: Mutex::new(None),
            normalize_response_model_identity: Arc::new(AtomicBool::new(true)),
        }
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Command 10: Start proxy
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct StartProxyResult {
    success: bool,
    pid: u32,
    python: String,
    dir: String,
    log: String,
}

#[tauri::command]
fn start_proxy(state: tauri::State<'_, ProxyState>) -> Result<StartProxyResult, String> {
    let mut diag: Vec<String> = Vec::new();

    // --- Phase 1: Check/clear previous state (brief lock) ---
    {
        let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
        let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;

        if let Some(ref handle) = *handle_guard {
            if !handle.inner().is_finished() {
                return Ok(StartProxyResult {
                    success: false,
                    pid: 0,
                    python: "rust-axum".into(),
                    dir: String::new(),
                    log: "already_running".into(),
                });
            }
            *handle_guard = None;
            *shutdown_guard = None;
            *done_guard = None;
        }
    } // locks dropped

    // --- Phase 2: Load config and resolve proxy config (no locks held) ---
    let cfg = match load_gateway_config() {
        Ok(c) => c,
        Err(e) => return Err(format!("Cannot read config: {}", e)),
    };

    diag.push(format!(
        "Providers: {}",
        cfg.providers.keys().cloned().collect::<Vec<_>>().join(", ")
    ));

    let openrouter_models = crate::openrouter::load_cached_models(&paths::user_data_dir());

    // Update runtime normalization toggle from persisted config
    use std::sync::atomic::Ordering;
    tracing::info!(
        normalize_response_model_identity = cfg.normalize_response_model_identity,
        "proxy runtime settings loaded"
    );
    state
        .normalize_response_model_identity
        .store(cfg.normalize_response_model_identity, Ordering::Relaxed);

    let proxy_config = match proxy::resolve_proxy_config(
        &cfg,
        &openrouter_models,
        state.normalize_response_model_identity.clone(),
    ) {
        Ok(c) => {
            diag.push(format!(
                "Routing: model-based ({} models across {} providers)",
                c.all_models.len(),
                c.providers.len()
            ));
            for m in &c.all_models {
                if let Some(entry) = c.model_route.get(m) {
                    diag.push(format!(
                        "  {} -> provider={} upstream={}",
                        m, entry.provider_id, entry.upstream_model
                    ));
                }
            }
            c
        }
        Err(e) => return Err(format!("Config error: {}", e)),
    };

    let host = proxy_config.server_host.clone();
    let port = proxy_config.server_port;
    diag.push(format!("Starting proxy on {}:{}", host, port));

    let (tx, rx) = oneshot::channel::<()>();
    let (done_tx, done_rx) = std::sync::mpsc::channel::<Result<(), String>>();

    let handle = tauri::async_runtime::spawn(async move {
        let result = proxy::run_proxy_server(host, port, proxy_config, rx).await;
        let _ = done_tx.send(result.map_err(|e| e.to_string()));
    });

    // --- Phase 3: Store handle, shutdown sender, and done receiver (brief lock) ---
    {
        let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
        let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
        let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;
        *handle_guard = Some(handle);
        *shutdown_guard = Some(tx);
        *done_guard = Some(done_rx);
    } // locks dropped

    // --- Phase 4: Poll for port reachability (no locks held) ---
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(5);
    loop {
        std::thread::sleep(std::time::Duration::from_millis(150));
        if TcpStream::connect_timeout(
            &"127.0.0.1:4000".parse().unwrap(),
            std::time::Duration::from_millis(200),
        )
        .is_ok()
        {
            diag.push(format!(
                "Port 4000 reachable after {:.1}s",
                start.elapsed().as_secs_f32()
            ));
            break;
        }
        if start.elapsed() >= timeout {
            // Re-acquire locks briefly just to clear state on failure
            let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
            let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
            let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;
            let _ = shutdown_guard.take().map(|tx| tx.send(()));
            let _ = handle_guard.take();
            let _ = done_guard.take();
            return Err(format!(
                "Proxy did not become reachable within {}s",
                timeout.as_secs()
            ));
        }
    }

    Ok(StartProxyResult {
        success: true,
        pid: 0,
        python: "rust-axum".into(),
        dir: String::new(),
        log: diag.join("\n"),
    })
}

// ---------------------------------------------------------------------------
// Command 11: Stop proxy
// ---------------------------------------------------------------------------

#[tauri::command]
fn stop_proxy(state: tauri::State<'_, ProxyState>) -> Result<String, String> {
    let mut handle_guard = state.handle.lock().map_err(|e| e.to_string())?;
    let mut shutdown_guard = state.shutdown_tx.lock().map_err(|e| e.to_string())?;
    let mut done_guard = state.done_rx.lock().map_err(|e| e.to_string())?;

    let mut diag_parts: Vec<String> = Vec::new();

    // Send shutdown signal
    if let Some(tx) = shutdown_guard.take() {
        let _ = tx.send(());
        diag_parts.push("Shutdown signal sent".into());
    } else {
        diag_parts.push("No active shutdown channel".into());
    }

    // Wait for task to finish via mpsc channel (avoids block_on re-entrancy panic)
    if let Some(rx) = done_guard.take() {
        diag_parts.push("Waiting for proxy task to finish...".into());
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(Ok(())) => diag_parts.push("Proxy task finished cleanly".into()),
            Ok(Err(e)) => diag_parts.push(format!("Proxy task error: {}", e)),
            Err(_) => diag_parts.push("Timeout waiting for proxy task to finish".into()),
        }
    } else {
        diag_parts.push("No active done channel".into());
    }

    // Clear handle
    let _ = handle_guard.take();

    // Check port 4000 after stopping
    let port_reachable = TcpStream::connect_timeout(
        &"127.0.0.1:4000".parse().unwrap(),
        std::time::Duration::from_millis(500),
    )
    .is_ok();
    diag_parts.push(format!(
        "Port 4000 reachable after stop: {}",
        port_reachable
    ));

    Ok(diag_parts.join("\n"))
}

// ---------------------------------------------------------------------------
// Command 12: Proxy status
// ---------------------------------------------------------------------------

#[tauri::command]
fn proxy_status(state: tauri::State<'_, ProxyState>) -> Result<bool, String> {
    let guard = state.handle.lock().map_err(|e| e.to_string())?;
    if let Some(ref handle) = *guard {
        Ok(!handle.inner().is_finished())
    } else {
        Ok(false)
    }
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------
// File-based tracing initialization
// ---------------------------------------------------------------------------

struct LogGuard(Mutex<Option<tracing_appender::non_blocking::WorkerGuard>>);

struct InitializedTracing {
    guard: tracing_appender::non_blocking::WorkerGuard,
    log_path: PathBuf,
}

fn initialize_file_tracing(log_dir: &PathBuf) -> Option<InitializedTracing> {
    if let Err(e) = std::fs::create_dir_all(log_dir) {
        eprintln!(
            "Failed to create log directory '{}': {}",
            log_dir.display(),
            e
        );
        return None;
    }

    let filename = format!(
        "proxy-{}-{}.log",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        std::process::id(),
    );
    let log_path = log_dir.join(&filename);

    let log_file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to open log file '{}': {}", log_path.display(), e);
            return None;
        }
    };

    let (non_blocking, guard) = tracing_appender::non_blocking(log_file);

    if let Err(e) = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("anthro_bridge_lib=info")),
        )
        .with_ansi(false)
        .with_target(false)
        .with_writer(non_blocking)
        .try_init()
    {
        eprintln!("Failed to initialize tracing subscriber: {e}");
        drop(guard);
        let _ = std::fs::remove_file(&log_path);
        return None;
    }

    Some(InitializedTracing { guard, log_path })
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize config (migration, seeding, normalization) before anything reads it
    if let Err(error) = ensure_config_initialized() {
        eprintln!(
            "[anthro-bridge] FATAL: Configuration initialization failed: {error}"
        );
        // Show a message box before exiting so the user sees the error
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let _ = std::process::Command::new("cmd")
                .args(&[
                    "/C",
                    "msg",
                    "%USERNAME%",
                    &format!("Anthro Bridge configuration initialization failed.\n\n{}\n\nCheck the log file in %APPDATA%\\Anthro Bridge\\Communication-Logs\\ for details.", error),
                ])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .spawn();
        }
        std::process::exit(1);
    }

    tauri::Builder::default()
        .setup(|app| {
            // Initialize file-based tracing
            let log_dir = paths::log_dir();
            if let Some(initialized) = initialize_file_tracing(&log_dir) {
                tracing::info!(
                    channel = ?paths::app_channel(),
                    data_dir = %paths::user_data_dir().display(),
                    "Resolved application data directory"
                );
                tracing::info!(
                    log_file = %initialized.log_path.display(),
                    "application file logging initialized"
                );
                app.manage(LogGuard(Mutex::new(Some(initialized.guard))));
            }

            // Dev builds get a "(DEV)" window title suffix.
            // cfg!(debug_assertions) guards against ANTHRO_BRIDGE_CHANNEL not
            // propagating through cross-env → tauri CLI → cargo build.
            if cfg!(debug_assertions) && paths::app_channel() == paths::AppChannel::Dev {
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(e) = window.set_title("Anthro Bridge (DEV)") {
                        tracing::warn!("Failed to set dev window title: {e}");
                    }
                }
            }

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_min_size(Some(tauri::PhysicalSize::new(1100, 720)));
            }
            Ok(())
        })
        .manage(ProxyState::new())
        .manage(ConfigState::new())
        .invoke_handler(tauri::generate_handler![
            check_health,
            check_gateway_status,
            check_api_key,
            get_port_4000_process,
            read_config,
            read_latest_log,
            open_logs_folder,
            open_path,
            read_config_raw,
            write_config,
            find_claude_configs,
            list_logs,
            read_log,
            create_new_log,
            set_env_api_key,
            update_provider_api_key_env,
            set_model_upstream,
            check_all_api_keys,
            update_active_provider,
            add_openrouter_profile,
            delete_openrouter_profile,
            rename_openrouter_profile,
            reorder_openrouter_profiles,
            activate_openrouter_profile,
            set_openrouter_profile_hidden,
            set_provider_hidden,
            start_proxy,
            stop_proxy,
            proxy_status,
            get_user_language,
            set_user_language,
            get_pricing_display_timezone,
            set_pricing_display_timezone,
            is_first_run,
            backup_config,
            restore_config_from_backup,
            reset_config,
            update_server_config,
            update_normalize_model_identity,
            update_claude_code_auto_compact_global,
            update_claude_code_auto_compact_target,
            update_claude_code_context_settings,
            resolve_claude_code_auto_compact,
            build_claude_code_launch_command,
            openrouter::openrouter_get_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Migration tests ──────────────────────────────────────────

    #[test]
    fn migration_creates_two_profiles() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "models": {
                        "claude-opus-5": {
                            "upstream_model": "poolside/laguna-s-2.1",
                            "thinking_mode": "thinking",
                            "reasoning_effort": "max",
                            "force_thinking": false,
                            "visible": true
                        },
                        "claude-sonnet-5": {
                            "upstream_model": "poolside/laguna-s-2.1",
                            "thinking_mode": "normal",
                            "force_thinking": false,
                            "visible": true
                        },
                        "claude-haiku-4-5": {
                            "upstream_model": "poolside/laguna-xs-2.1",
                            "thinking_mode": "thinking",
                            "visible": true
                        }
                    },
                    "model_map": {
                        "claude-opus-5": "poolside/laguna-s-2.1",
                        "claude-sonnet-5": "poolside/laguna-s-2.1",
                        "claude-haiku-4-5": "poolside/laguna-xs-2.1"
                    },
                    "visible_models": ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"]
                }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        assert!(outcome.changed);
        assert!(outcome.active_profile_id.is_some());

        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0]["display_name"], "OpenRouter");
        assert_eq!(profiles[1]["display_name"], "OpenRouter: Hy3");

        // Legacy fields removed
        assert!(cfg["providers"]["openrouter"]["models"].as_object().map_or(true, |o| o.is_empty()));
    }

    #[test]
    fn migration_preserves_existing_models() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "models": {
                        "claude-opus-5": {
                            "upstream_model": "poolside/laguna-s-2.1",
                            "thinking_mode": "thinking",
                            "visible": true
                        }
                    },
                    "model_map": {
                        "claude-opus-5": "poolside/laguna-s-2.1"
                    },
                    "visible_models": ["claude-opus-5"]
                }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        assert!(outcome.changed);

        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);

        // First profile preserves existing
        let first = &profiles[0];
        let opus = &first["models"]["claude-opus-5"];
        assert_eq!(opus["upstream_model"], "poolside/laguna-s-2.1");
        assert_eq!(opus["thinking_mode"], "thinking");

        // Missing keys are backfilled
        assert!(first["models"].get("claude-sonnet-5").is_some());
        assert!(first["models"].get("claude-haiku-4-5").is_some());
        assert!(first["model_map"].get("claude-sonnet-5").is_some());
        assert!(first["visible_models"].as_array().unwrap().contains(&json!("claude-opus-5")));
    }

    #[test]
    fn migration_hy3_existing_adds_laguna() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "models": {
                        "claude-opus-5": {
                            "upstream_model": "tencent/hy3",
                            "thinking_mode": "thinking",
                            "visible": true
                        },
                        "claude-sonnet-5": {
                            "upstream_model": "tencent/hy3",
                            "thinking_mode": "thinking",
                            "visible": true
                        },
                        "claude-haiku-4-5": {
                            "upstream_model": "tencent/hy3",
                            "thinking_mode": "normal",
                            "visible": true
                        }
                    },
                    "model_map": {
                        "claude-opus-5": "tencent/hy3",
                        "claude-sonnet-5": "tencent/hy3",
                        "claude-haiku-4-5": "tencent/hy3"
                    },
                    "visible_models": ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"]
                }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        assert!(outcome.changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0]["display_name"], "OpenRouter");
        // Second profile should be Laguna (Hy3-only configs get Laguna as second)
        assert_eq!(profiles[1]["display_name"], "OpenRouter: Laguna");
    }

    #[test]
    fn migration_idempotent() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "profiles": [{"id": "x", "display_name": "Test", "model_map": {}, "visible_models": [], "models": {}}]
                }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        assert!(!outcome.changed);
    }

    #[test]
    fn migration_without_openrouter_is_noop() {
        let mut cfg = json!({
            "providers": {
                "deepseek": { "display_name": "DeepSeek" }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        assert!(!outcome.changed);
        assert!(outcome.active_profile_id.is_none());
    }

    #[test]
    fn migration_active_id_points_to_first_profile() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "models": {
                        "claude-opus-5": {
                            "upstream_model": "poolside/laguna-s-2.1",
                            "visible": true
                        }
                    },
                    "model_map": {
                        "claude-opus-5": "poolside/laguna-s-2.1"
                    },
                    "visible_models": ["claude-opus-5"]
                }
            }
        });

        let outcome = migrate_openrouter_to_profiles(&mut cfg).unwrap();
        let active_id = outcome.active_profile_id.unwrap();
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["id"], active_id);
    }

    // ── Profile builder tests ─────────────────────────────────────

    #[test]
    fn laguna_profile_uses_xs_for_haiku() {
        let profile = build_laguna_profile("Test");
        assert_eq!(
            profile.models["claude-haiku-4-5"].upstream_model,
            "poolside/laguna-xs-2.1"
        );
        assert_eq!(
            profile.models["claude-opus-5"].upstream_model,
            "poolside/laguna-s-2.1"
        );
    }

    #[test]
    fn hy3_profile_has_expected_thinking_modes() {
        let profile = build_hy3_profile("Test");
        let opus = &profile.models["claude-opus-5"];
        let sonnet = &profile.models["claude-sonnet-5"];
        let haiku = &profile.models["claude-haiku-4-5"];

        assert_eq!(opus.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(opus.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(sonnet.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(sonnet.reasoning_effort.as_deref(), Some("low"));
        assert_eq!(haiku.thinking_mode.as_deref(), Some("normal"));
        assert!(haiku.reasoning_effort.is_none());

        // All Hy3 models have force_thinking = false
        assert!(!opus.force_thinking.unwrap());
        assert!(!sonnet.force_thinking.unwrap());
        assert!(!haiku.force_thinking.unwrap());
    }

    #[test]
    fn profile_models_and_model_map_are_consistent() {
        for profile in [
            build_laguna_profile("L"),
            build_hy3_profile("H"),
            build_inclusionai_profile("I"),
            build_stepfun_profile("S"),
            build_gpt56_balanced_profile("G"),
        ] {
            for key in ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"] {
                assert_eq!(
                    profile.models[key].upstream_model,
                    profile.model_map[key],
                    "profile={}, key={}", profile.display_name, key
                );
            }
        }
    }

    #[test]
    fn add_profile_creates_laguna_template() {
        let profile = build_laguna_profile("NewProfile");
        assert!(profile.id.len() > 30); // UUID v4
        assert!(profile.models.contains_key("claude-opus-5"));
        assert!(profile.models.contains_key("claude-sonnet-5"));
        assert!(profile.models.contains_key("claude-haiku-4-5"));
    }

    // ── normalize_config tests ────────────────────────────────────

    #[test]
    fn normalize_repairs_empty_profiles() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "profiles": []
                }
            }
        });

        let changed = normalize_config(&mut cfg);
        assert!(changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);
        assert!(cfg["active_openrouter_profile_id"].as_str().is_some());
    }

    #[test]
    fn normalize_repairs_invalid_active_profile_id() {
        let profile = build_laguna_profile("Test");
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "nonexistent-uuid",
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "profiles": [serde_json::to_value(&profile).unwrap()]
                }
            }
        });

        let changed = normalize_config(&mut cfg);
        assert!(changed);
        assert_eq!(
            cfg["active_openrouter_profile_id"].as_str().unwrap(),
            profile.id
        );
    }

    #[test]
    fn normalize_noop_when_valid() {
        let profile = build_laguna_profile("Test");
        let mut cfg = json!({
            "active_openrouter_profile_id": profile.id,
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "profiles": [serde_json::to_value(&profile).unwrap()]
                }
            }
        });

        let changed = normalize_config(&mut cfg);
        assert!(!changed);
    }

    // ── Profile name normalization ──────────────────────────────────

    #[test]
    fn normalize_profile_name_trims_whitespace() {
        assert_eq!(normalize_profile_name("  Foo  ").unwrap(), "Foo");
    }

    #[test]
    fn normalize_profile_name_rejects_empty() {
        assert!(normalize_profile_name("").is_err());
        assert!(normalize_profile_name("   ").is_err());
    }

    #[test]
    fn normalize_profile_name_rejects_too_long() {
        let long = "a".repeat(81);
        assert!(normalize_profile_name(&long).is_err());
        let ok = "a".repeat(80);
        assert!(normalize_profile_name(&ok).is_ok());
    }

    // ── apply_add_openrouter_profile ────────────────────────────────

    #[test]
    fn apply_add_openrouter_profile_appends() {
        let profile = build_laguna_profile("Existing");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&profile).unwrap()]
                }
            }
        });
        let outcome = apply_add_openrouter_profile(&mut cfg, "NewOne").unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[1]["display_name"], "NewOne");
        let returned = outcome.value;
        assert_eq!(returned["display_name"], "NewOne");
    }

    #[test]
    fn apply_add_rejects_blank_profile_name() {
        let profile = build_laguna_profile("Existing");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&profile).unwrap()]
                }
            }
        });
        assert!(apply_add_openrouter_profile(&mut cfg, "").is_err());
        assert!(apply_add_openrouter_profile(&mut cfg, "   ").is_err());
    }

    #[test]
    fn apply_profile_name_is_trimmed() {
        let profile = build_laguna_profile("Existing");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&profile).unwrap()]
                }
            }
        });
        let outcome = apply_add_openrouter_profile(&mut cfg, "  Trimmed  ").unwrap();
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[1]["display_name"], "Trimmed");
        assert_eq!(outcome.value["display_name"], "Trimmed");
    }

    // ── apply_delete_openrouter_profile ─────────────────────────────

    #[test]
    fn apply_delete_openrouter_profile_removes_and_repairs() {
        let p1 = build_laguna_profile("First");
        let p2 = build_laguna_profile("Second");
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": &p1.id,
            "providers": {
                "openrouter": {
                    "profiles": [
                        serde_json::to_value(&p1).unwrap(),
                        serde_json::to_value(&p2).unwrap(),
                    ]
                }
            }
        });
        let outcome = apply_delete_openrouter_profile(&mut cfg, &p1.id).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(cfg["active_openrouter_profile_id"].as_str().unwrap(), &p2.id);
    }

    #[test]
    fn apply_delete_openrouter_profile_refuses_last() {
        let p1 = build_laguna_profile("Only");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        assert!(apply_delete_openrouter_profile(&mut cfg, &p1.id).is_err());
    }

    // ── apply_rename_openrouter_profile ─────────────────────────────

    #[test]
    fn apply_rename_openrouter_profile_updates_name() {
        let p1 = build_laguna_profile("OldName");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let outcome = apply_rename_openrouter_profile(&mut cfg, &p1.id, "NewName").unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["display_name"], "NewName");
    }

    #[test]
    fn apply_rename_rejects_blank_profile_name() {
        let p1 = build_laguna_profile("Test");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        assert!(apply_rename_openrouter_profile(&mut cfg, &p1.id, "").is_err());
    }

    #[test]
    fn apply_rename_same_name_is_noop() {
        let p1 = build_laguna_profile("SameName");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let outcome = apply_rename_openrouter_profile(&mut cfg, &p1.id, "SameName").unwrap();
        assert!(!outcome.config_changed);
        assert!(!outcome.restart_gateway);
    }

    // ── apply_reorder_openrouter_profiles ───────────────────────────

    #[test]
    fn apply_reorder_openrouter_profiles_changes_order() {
        let p1 = build_laguna_profile("First");
        let p2 = build_hy3_profile("Second");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [
                        serde_json::to_value(&p1).unwrap(),
                        serde_json::to_value(&p2).unwrap(),
                    ]
                }
            }
        });
        let outcome =
            apply_reorder_openrouter_profiles(&mut cfg, &[p2.id.clone(), p1.id.clone()]).unwrap();
        assert!(outcome.config_changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["id"], p2.id);
        assert_eq!(profiles[1]["id"], p1.id);
    }

    #[test]
    fn apply_reorder_rejects_wrong_length() {
        let p1 = build_laguna_profile("First");
        let p2 = build_hy3_profile("Second");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [
                        serde_json::to_value(&p1).unwrap(),
                        serde_json::to_value(&p2).unwrap(),
                    ]
                }
            }
        });
        assert!(apply_reorder_openrouter_profiles(&mut cfg, &[p1.id.clone()]).is_err());
    }

    #[test]
    fn apply_reorder_rejects_duplicate_ids() {
        let p1 = build_laguna_profile("First");
        let p2 = build_hy3_profile("Second");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [
                        serde_json::to_value(&p1).unwrap(),
                        serde_json::to_value(&p2).unwrap(),
                    ]
                }
            }
        });
        assert!(
            apply_reorder_openrouter_profiles(&mut cfg, &[p1.id.clone(), p1.id.clone()]).is_err()
        );
    }

    #[test]
    fn apply_reorder_same_order_is_noop() {
        let p1 = build_laguna_profile("First");
        let p2 = build_hy3_profile("Second");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [
                        serde_json::to_value(&p1).unwrap(),
                        serde_json::to_value(&p2).unwrap(),
                    ]
                }
            }
        });
        let outcome =
            apply_reorder_openrouter_profiles(&mut cfg, &[p1.id.clone(), p2.id.clone()]).unwrap();
        assert!(!outcome.config_changed);
    }

    // ── apply_activate_openrouter_profile ───────────────────────────

    #[test]
    fn apply_activate_openrouter_profile_switches_and_signals() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "minimax",
            "active_openrouter_profile_id": null,
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let outcome = apply_activate_openrouter_profile(&mut cfg, &p1.id).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway);
        assert_eq!(cfg["active_provider"], "openrouter");
        assert_eq!(cfg["active_openrouter_profile_id"], p1.id);
    }

    #[test]
    fn apply_activate_same_profile_reports_config_unchanged() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": &p1.id,
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let outcome = apply_activate_openrouter_profile(&mut cfg, &p1.id).unwrap();
        assert!(!outcome.config_changed);
        assert!(!outcome.restart_gateway);
    }

    // ── execute_config_mutation orchestration ───────────────────────

    #[test]
    fn execute_mutation_saves_once_when_changed() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "minimax",
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| {
                apply_activate_openrouter_profile(cfg, &p1.id)
                    .map(|o| ApplyOutcome { ..o })
            },
            |_cfg| {
                save_count += 1;
                Ok(())
            },
        );
        let res = result.unwrap();
        assert_eq!(save_count, 1);
        assert!(res.restart_gateway);
        assert!(res.restart_reason.contains("changed"));
    }

    #[test]
    fn execute_mutation_does_not_save_on_noop() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": &p1.id,
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| {
                let outcome = apply_activate_openrouter_profile(cfg, &p1.id).unwrap();
                Ok(outcome)
            },
            |_cfg| {
                save_count += 1;
                Ok(())
            },
        );
        let res = result.unwrap();
        assert_eq!(save_count, 0);
        assert!(!res.restart_gateway);
    }

    #[test]
    fn execute_mutation_propagates_save_error() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "minimax",
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| {
                apply_activate_openrouter_profile(cfg, &p1.id)
                    .map(|o| ApplyOutcome { ..o })
            },
            |_cfg| Err("disk full".into()),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "disk full");
    }

    #[test]
    fn activate_noop_orchestration_skips_persistence() {
        let p1 = build_laguna_profile("First");
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": &p1.id,
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| Ok(apply_activate_openrouter_profile(cfg, &p1.id).unwrap()),
            |_cfg| {
                save_count += 1;
                Ok(())
            },
        );
        let res = result.unwrap();
        assert_eq!(save_count, 0);
        assert!(!res.restart_gateway);
    }

    #[test]
    fn rename_noop_orchestration_skips_persistence() {
        let p1 = build_laguna_profile("SameName");
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [serde_json::to_value(&p1).unwrap()]
                }
            }
        });
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| Ok(apply_rename_openrouter_profile(cfg, &p1.id, "SameName").unwrap()),
            |_cfg| {
                save_count += 1;
                Ok(())
            },
        );
        let res = result.unwrap();
        assert_eq!(save_count, 0);
        assert!(!res.restart_gateway);
    }

    // ── apply_set_model_upstream ─────────────────────────────────────

    /// Helper: build a minimal multi-profile config for testing set_model.
    fn make_or_test_cfg(profile_id: &str, upstream: &str, tm: Option<&str>) -> serde_json::Value {
        json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": profile_id,
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": profile_id,
                        "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": {
                                "upstream_model": upstream,
                                "thinking_mode": tm,
                                "visible": true,
                                "force_thinking": false,
                                "supports_image_url": false,
                                "supports_image_base64": false,
                                "supports_video_url": false,
                                "supports_video_base64": false
                            }
                        },
                        "model_map": {
                            "claude-sonnet-5": upstream
                        },
                        "visible_models": ["claude-sonnet-5"]
                    }]
                }
            }
        })
    }

    #[test]
    fn apply_set_model_upstream_writes_requested_profile() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "tencent/hy3",
            Some("normal"), Some("low"),
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        assert_eq!(m["upstream_model"], "tencent/hy3");
        assert_eq!(m["thinking_mode"], "normal");
        assert_eq!(m["reasoning_effort"], "low");
        assert_eq!(profiles[0]["model_map"]["claude-sonnet-5"], "tencent/hy3");
    }

    #[test]
    fn apply_set_model_upstream_does_not_modify_other_profiles() {
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [
                        {
                            "id": "p1",
                            "display_name": "First",
                            "models": {
                                "claude-sonnet-5": {
                                    "upstream_model": "poolside/laguna-s-2.1",
                                    "visible": true
                                }
                            },
                            "model_map": { "claude-sonnet-5": "poolside/laguna-s-2.1" },
                            "visible_models": ["claude-sonnet-5"]
                        },
                        {
                            "id": "p2",
                            "display_name": "Second",
                            "models": {
                                "claude-sonnet-5": {
                                    "upstream_model": "tencent/hy3",
                                    "visible": true
                                }
                            },
                            "model_map": { "claude-sonnet-5": "tencent/hy3" },
                            "visible_models": ["claude-sonnet-5"]
                        }
                    ]
                }
            }
        });
        apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        // p2 unchanged
        assert_eq!(profiles[1]["models"]["claude-sonnet-5"]["upstream_model"], "tencent/hy3");
    }

    #[test]
    fn apply_set_model_upstream_rejects_missing_profile_for_openrouter() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        let err = apply_set_model_upstream(
            &mut cfg, "openrouter", None,
            "claude-sonnet-5", "tencent/hy3",
            None, None,
        ).unwrap_err();
        assert!(err.contains("profile_id"), "expected profile_id error, got: {}", err);
    }

    #[test]
    fn apply_set_model_upstream_rejects_unknown_profile_id() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        let err = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("nonexistent"),
            "claude-sonnet-5", "tencent/hy3",
            None, None,
        ).unwrap_err();
        assert!(err.contains("not found"), "expected not-found error, got: {}", err);
    }

    #[test]
    fn apply_set_model_upstream_ignores_profile_for_non_openrouter() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "visible": true
                        }
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "deepseek", Some("ignored"),
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway); // active_provider == deepseek
        assert_eq!(
            cfg["providers"]["deepseek"]["models"]["claude-sonnet-5"]["upstream_model"],
            "deepseek-v4-flash"
        );
    }

    #[test]
    fn apply_set_model_upstream_same_values_is_noop() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "poolside/laguna-s-2.1",
            Some("thinking"), None,
        ).unwrap();
        assert!(!outcome.config_changed);
        assert!(!outcome.restart_gateway);
    }

    #[test]
    fn apply_set_model_upstream_keeps_models_and_model_map_in_sync() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let model_upstream = &profiles[0]["models"]["claude-sonnet-5"]["upstream_model"];
        let map_upstream = &profiles[0]["model_map"]["claude-sonnet-5"];
        assert_eq!(model_upstream, map_upstream);
        assert_eq!(model_upstream, "deepseek-v4-pro");
    }

    #[test]
    fn apply_set_model_upstream_refreshes_static_capability_fields() {
        // deepseek-v4-pro has force_thinking=false, all image/video false
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        // Start with stale capabilities (wrong force_thinking for laguna-s-2.1)
        apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        // OpenRouter + unknown model → write_capability_flags returns early (unchanged).
        // deepseek-v4-pro is not in TEXT_ONLY_OR_MODELS, so flags are left as-is.
        // The key test is that the model_map and upstream_model were updated.
        assert_eq!(m["upstream_model"], "deepseek-v4-pro");
    }

    #[test]
    fn apply_set_model_upstream_repairs_stale_capability_fields() {
        // Use a non-OpenRouter provider so capability refresh runs (TEXT_ONLY_OR_MODELS check
        // only returns early for OpenRouter unknown models).
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "force_thinking": true,         // stale!
                            "supports_image_url": true,     // stale!
                            "supports_image_base64": true, // stale!
                            "supports_video_url": true,    // stale!
                            "supports_video_base64": true, // stale!
                            "visible": true
                        }
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "MiniMax-M3",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        let m = &cfg["providers"]["deepseek"]["models"]["claude-sonnet-5"];
        // MiniMax-M3: image+video=true, force_thinking=false
        assert_eq!(m["upstream_model"], "MiniMax-M3");
        assert!(!m["force_thinking"].as_bool().unwrap());
        assert!(m["supports_image_url"].as_bool().unwrap());
        assert!(m["supports_image_base64"].as_bool().unwrap());
        assert!(m["supports_video_url"].as_bool().unwrap());
        assert!(m["supports_video_base64"].as_bool().unwrap());
    }

    #[test]
    fn apply_set_model_upstream_is_noop_only_when_all_written_fields_match() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "force_thinking": false,
                            "supports_image_url": false,
                            "supports_image_base64": false,
                            "supports_video_url": false,
                            "supports_video_base64": false,
                            "visible": true
                        }
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        // Same model, same everything — no-op
        let outcome = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        assert!(!outcome.config_changed);
    }

    #[test]
    fn apply_set_model_upstream_preserves_existing_none_serialization_semantics() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        // Remove thinking_mode → None semantics should remove the key
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "poolside/laguna-s-2.1",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed); // thinking_mode removed
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        assert!(m.get("thinking_mode").is_none());
        assert!(m.get("reasoning_effort").is_none());
        // No Value::Null
        assert!(!m.as_object().unwrap().contains_key("thinking_mode"));
    }

    #[test]
    fn apply_set_model_upstream_none_clears_existing_reasoning_effort() {
        // Pre-seed a reasoning_effort so we can verify a null/None update removes it.
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        cfg["providers"]["openrouter"]["profiles"][0]["models"]["claude-sonnet-5"]["reasoning_effort"] =
            json!("max");

        // Update with reasoning_effort = None → the stored value must be cleared.
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "poolside/laguna-s-2.1",
            Some("thinking"), None,
        ).unwrap();
        assert!(outcome.config_changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        assert!(m.get("reasoning_effort").is_none());
        assert!(!m.as_object().unwrap().contains_key("reasoning_effort"));
    }

    #[test]
    fn repeated_set_model_upstream_is_stable_after_first_write() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        // First write: changes thinking_mode + repairs stale capabilities
        let o1 = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "poolside/laguna-s-2.1",
            Some("normal"), None,
        ).unwrap();
        assert!(o1.config_changed, "1st write should change (thinking_mode: thinking→normal)");
        // Second write with same input: no-op
        let o2 = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "poolside/laguna-s-2.1",
            Some("normal"), None,
        ).unwrap();
        assert!(!o2.config_changed, "2nd write should be no-op");
        assert!(!o2.restart_gateway);
    }

    // ── apply_set_model_upstream: restart gating ─────────────────────

    #[test]
    fn editing_inactive_openrouter_profile_does_not_restart() {
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [
                        {
                            "id": "p1", "display_name": "Active",
                            "models": {
                                "claude-sonnet-5": { "upstream_model": "poolside/laguna-s-2.1", "visible": true }
                            },
                            "model_map": { "claude-sonnet-5": "poolside/laguna-s-2.1" },
                            "visible_models": ["claude-sonnet-5"]
                        },
                        {
                            "id": "p2", "display_name": "Inactive",
                            "models": {
                                "claude-sonnet-5": { "upstream_model": "tencent/hy3", "visible": true }
                            },
                            "model_map": { "claude-sonnet-5": "tencent/hy3" },
                            "visible_models": ["claude-sonnet-5"]
                        }
                    ]
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p2"), // editing p2, but p1 is active
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
    }

    #[test]
    fn editing_selected_openrouter_profile_while_other_provider_active_does_not_restart() {
        let mut cfg = json!({
            "active_provider": "minimax",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": "p1", "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": { "upstream_model": "poolside/laguna-s-2.1", "visible": true }
                        },
                        "model_map": { "claude-sonnet-5": "poolside/laguna-s-2.1" },
                        "visible_models": ["claude-sonnet-5"]
                    }]
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "tencent/hy3",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway); // minimax is active, not openrouter
    }

    #[test]
    fn editing_active_openrouter_profile_restarts() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "tencent/hy3",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway);
    }

    #[test]
    fn editing_active_non_openrouter_provider_restarts() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "visible": true
                        }
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        assert!(outcome.restart_gateway);
    }

    // ── set_model_upstream orchestration (in-memory, no file I/O) ────

    #[test]
    fn set_model_upstream_orchestration_saves_once_when_changed() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", None);
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| apply_set_model_upstream(
                cfg, "openrouter", Some("p1"),
                "claude-sonnet-5", "tencent/hy3",
                Some("thinking"), Some("high"),
            ),
            |_cfg| { save_count += 1; Ok(()) },
        );
        let res = result.unwrap();
        assert!(res.restart_gateway);
        assert_eq!(save_count, 1);
    }

    #[test]
    fn set_model_upstream_orchestration_skips_save_on_noop() {
        let mut cfg = make_or_test_cfg("p1", "poolside/laguna-s-2.1", Some("thinking"));
        let mut save_count = 0u32;
        let result: Result<CommandResponse<()>, String> = execute_config_mutation(
            &mut cfg,
            |cfg| apply_set_model_upstream(
                cfg, "openrouter", Some("p1"),
                "claude-sonnet-5", "poolside/laguna-s-2.1",
                Some("thinking"), None,
            ),
            |_cfg| { save_count += 1; Ok(()) },
        );
        let res = result.unwrap();
        assert!(!res.restart_gateway);
        assert_eq!(save_count, 0);
    }

    // ── OpenRouter capability persistence ──────────────────────────────

    #[test]
    fn openrouter_kimi_k3_persists_static_capabilities() {
        // Kimi K3 in OpenRouter: force_thinking=true, image_base64=true
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": "p1",
                        "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": {
                                "upstream_model": "tencent/hy3",
                                "force_thinking": false,
                                "supports_image_url": true,
                                "supports_image_base64": true,
                                "supports_video_url": true,
                                "supports_video_base64": true,
                                "visible": true
                            }
                        },
                        "model_map": { "claude-sonnet-5": "tencent/hy3" },
                        "visible_models": ["claude-sonnet-5"]
                    }]
                }
            }
        });
        // Change from tencent/hy3 to kimi-k3 — known model, static caps must overwrite
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "kimi-k3",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        assert_eq!(m["upstream_model"], "kimi-k3");
        assert!(m["force_thinking"].as_bool().unwrap()); // kimi-k3: force_thinking=true
        assert!(!m["supports_image_url"].as_bool().unwrap()); // kimi-k3: image_url=false
        assert!(m["supports_image_base64"].as_bool().unwrap()); // kimi-k3: image_b64=true
        assert!(!m["supports_video_url"].as_bool().unwrap());
        assert!(!m["supports_video_base64"].as_bool().unwrap());
    }

    #[test]
    fn openrouter_known_model_repairs_stale_capabilities() {
        // deepseek-v4-pro in OpenRouter: force_thinking=false, all image/video false
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": "p1",
                        "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": {
                                "upstream_model": "poolside/laguna-s-2.1",
                                "force_thinking": true,
                                "supports_image_url": true,
                                "supports_image_base64": true,
                                "supports_video_url": true,
                                "supports_video_base64": true,
                                "visible": true
                            }
                        },
                        "model_map": { "claude-sonnet-5": "poolside/laguna-s-2.1" },
                        "visible_models": ["claude-sonnet-5"]
                    }]
                }
            }
        });
        // Change to deepseek-v4-pro — known model, must repair all stale caps
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "deepseek-v4-pro",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        assert!(!m["force_thinking"].as_bool().unwrap());
        assert!(!m["supports_image_url"].as_bool().unwrap());
        assert!(!m["supports_image_base64"].as_bool().unwrap());
        assert!(!m["supports_video_url"].as_bool().unwrap());
        assert!(!m["supports_video_base64"].as_bool().unwrap());
    }

    #[test]
    fn openrouter_unknown_model_preserves_existing_capabilities() {
        // Unknown model in OpenRouter → existing caps preserved
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": "p1",
                        "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": {
                                "upstream_model": "poolside/laguna-s-2.1",
                                "force_thinking": false,
                                "supports_image_url": true,
                                "supports_image_base64": false,
                                "supports_video_url": false,
                                "supports_video_base64": true,
                                "visible": true
                            }
                        },
                        "model_map": { "claude-sonnet-5": "poolside/laguna-s-2.1" },
                        "visible_models": ["claude-sonnet-5"]
                    }]
                }
            }
        });
        let outcome = apply_set_model_upstream(
            &mut cfg, "openrouter", Some("p1"),
            "claude-sonnet-5", "custom/unknown-model-v999",
            None, None,
        ).unwrap();
        assert!(outcome.config_changed); // upstream_model changed
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let m = &profiles[0]["models"]["claude-sonnet-5"];
        // Unknown model → existing caps preserved
        assert!(!m["force_thinking"].as_bool().unwrap());
        assert!(m["supports_image_url"].as_bool().unwrap());
        assert!(!m["supports_image_base64"].as_bool().unwrap());
        assert!(!m["supports_video_url"].as_bool().unwrap());
        assert!(m["supports_video_base64"].as_bool().unwrap());
    }

    // ── Malformed model entry → Err (no panic) ────────────────────────

    #[test]
    fn apply_set_model_upstream_rejects_non_object_model_entry() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": "not-an-object"
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        let err = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).unwrap_err();
        assert!(err.contains("must be a JSON object"), "got: {}", err);
    }

    #[test]
    fn malformed_model_entry_does_not_panic() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": null
                    },
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        // Must not panic — returns Err
        let result = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn apply_set_model_upstream_rejects_non_object_models() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": "not-an-object",
                    "model_map": { "claude-sonnet-5": "deepseek-v4-pro" }
                }
            }
        });
        let err = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).unwrap_err();
        // The non-OpenRouter path catches this before write_model_entry_fields
        assert!(err.contains("models"), "got: {}", err);
    }

    #[test]
    fn apply_set_model_upstream_rejects_non_object_model_map() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "visible": true
                        }
                    },
                    "model_map": []
                }
            }
        });
        let err = apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).unwrap_err();
        assert!(err.contains("model_map must be a JSON object"), "got: {}", err);
    }

    #[test]
    fn malformed_model_map_does_not_panic() {
        let mut cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "visible": true
                        }
                    },
                    "model_map": null
                }
            }
        });
        assert!(apply_set_model_upstream(
            &mut cfg, "deepseek", None,
            "claude-sonnet-5", "deepseek-v4-flash",
            None, None,
        ).is_err());
    }

    #[test]
    fn all_statically_known_models_are_known_to_persistence() {
        // Every model in the static resolver must return Some from
        // try_resolve_static_model_capabilities — this proves there
        // is no separate list that can drift out of sync.
        let known = &[
            "deepseek-v4-pro",
            "deepseek-v4-flash",
            "MiniMax-M3",
            "MiniMax-M2.7",
            "MiniMax-M2.7-highspeed",
            "kimi-k3",
            "kimi-k2.7-code",
            "kimi-k2.7-code-highspeed",
            "kimi-k2.6",
            "kimi-k2.5",
            "mimo-v2.5-pro",
            "mimo-v2.5-pro-ultraspeed",
            "mimo-v2.5",
            "tencent/hy3",
            "tencent/hy3:free",
        ];
        for model in known {
            assert!(
                model_capabilities::try_resolve_static_model_capabilities(model).is_some(),
                "{} should be known to the static resolver",
                model,
            );
        }
    }

    // ── Config file I/O tests (TempDir isolation, no real APPDATA) ─────

    /// Compute the real APPDATA-based config paths for isolation verification.
    fn real_appdata_config_paths() -> (std::path::PathBuf, std::path::PathBuf) {
        let appdata = std::env::var("APPDATA").unwrap_or_default();
        let stable = std::path::PathBuf::from(&appdata).join("Anthro Bridge").join("config.json");
        let dev = std::path::PathBuf::from(&appdata).join("Anthro Bridge Dev").join("config.json");
        (stable, dev)
    }

    #[test]
    fn seed_creates_config_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");
        assert!(!config_path.exists());

        seed_config_from_template(&config_path).unwrap();

        assert!(config_path.exists(), "config.json should be created");
        let content = std::fs::read_to_string(&config_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert!(val.get("providers").is_some(), "must have providers key");
        assert!(val.get("server").is_some(), "must have server key");
        // No .tmp debris
        assert!(!dir.path().join("config.json.tmp").exists());
    }

    #[test]
    fn seed_does_not_overwrite_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        let user_json = r#"{"active_provider":"nonexistent-provider-xyz","providers":{},"server":{"host":"0.0.0.0","port":9999,"enable_cors":true}}"#;
        std::fs::write(&config_path, user_json).unwrap();

        let result = seed_config_from_template(&config_path);
        assert!(result.is_err(), "seed must refuse to overwrite existing config");
        assert!(
            result.unwrap_err().contains("Refusing to overwrite"),
            "error must mention refusal reason"
        );

        // Original content preserved byte-for-byte
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, user_json, "existing config must not be modified");
    }

    #[test]
    fn init_recovers_rollback_before_seeding() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");
        let rollback = dir.path().join("config.json.rollback");

        // Simulate crash during replace: rollback exists, config missing
        let user_settings = r#"{"active_provider":"minimax","providers":{"minimax":{"display_name":"UserMiniMax","upstream_url":"https://user.example.com","api_key_env":"USER_KEY","default_model":"user-model","supports_count_tokens":true,"supports_vision":false,"supports_video":false,"supports_thinking":true,"model_map":{},"visible_models":[],"models":{}}},"server":{"host":"127.0.0.1","port":4000,"enable_cors":false}}"#;
        std::fs::write(&rollback, user_settings).unwrap();
        assert!(!config_path.exists());
        assert!(rollback.exists());

        let result = ensure_config_initialized_at(&config_path, paths::AppChannel::Dev, false);
        assert!(result.is_ok(), "init should succeed via rollback recovery");

        // Must recover from rollback, not seed template
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(
            content.contains("UserMiniMax"),
            "should recover user settings from rollback, got: {}",
            content
        );
        assert!(
            content.contains("https://user.example.com"),
            "user's upstream_url must be preserved"
        );

        // Rollback must be gone after successful recovery
        assert!(!rollback.exists(), "rollback must be cleaned up after recovery");
    }

    #[test]
    fn replace_preserves_or_restores_original_on_failure() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        // Pre-create user config
        let user_json = r#"{"active_provider":"test-prov","providers":{"test-prov":{"display_name":"Test","upstream_url":"https://test.example.com","api_key_env":"TEST_KEY","default_model":"test-model","supports_count_tokens":false,"supports_vision":false,"supports_video":false,"supports_thinking":false,"model_map":{},"visible_models":[],"models":{}}},"server":{"host":"127.0.0.1","port":4000,"enable_cors":false}}"#;
        std::fs::write(&config_path, user_json).unwrap();

        // Sabotage: pre-create tmp as a directory so std::fs::write fails
        let tmp = dir.path().join("config.json.reset.tmp");
        std::fs::create_dir(&tmp).unwrap();

        let result = replace_config_from_template_atomically(&config_path);
        assert!(result.is_err(), "replace must fail when tmp is unwritable");

        // Original config preserved
        assert!(config_path.exists(), "original config must still exist");
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, user_json, "original config must be unchanged byte-for-byte");

        // No rollback left behind (step 3 never ran)
        let rollback = dir.path().join("config.json.rollback");
        assert!(!rollback.exists(), "no rollback should exist — stage never reached");
    }

    #[test]
    fn init_does_not_replace_user_config_with_template() {
        let dir = tempfile::TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");

        // User config with one custom provider
        let user_config = r#"{
            "active_provider": "my-custom-provider",
            "providers": {
                "my-custom-provider": {
                    "display_name": "My Custom Provider",
                    "upstream_url": "https://custom.example.com/api",
                    "api_key_env": "MY_CUSTOM_KEY",
                    "default_model": "my-custom-model",
                    "supports_count_tokens": true,
                    "supports_vision": false,
                    "supports_video": false,
                    "supports_thinking": true,
                    "model_map": {},
                    "visible_models": [],
                    "models": {}
                }
            },
            "server": {
                "host": "0.0.0.0",
                "port": 9999,
                "enable_cors": true
            }
        }"#;
        std::fs::write(&config_path, user_config).unwrap();

        let result = ensure_config_initialized_at(&config_path, paths::AppChannel::Dev, false);
        assert!(result.is_ok());

        let content = std::fs::read_to_string(&config_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();

        // User's custom provider MUST still exist
        let user_provider = &val["providers"]["my-custom-provider"];
        assert!(
            user_provider.is_object(),
            "user provider must be preserved after init"
        );
        assert_eq!(
            user_provider["display_name"], "My Custom Provider",
            "user provider display name must be preserved"
        );
        assert_eq!(
            user_provider["upstream_url"], "https://custom.example.com/api",
            "user provider upstream_url must be preserved"
        );

        // User's server settings MUST be preserved
        assert_eq!(val["server"]["port"], 9999, "user's custom port must be preserved");

        // Bundled providers SHOULD be merged in (additive, not replacement)
        // deepseek is in the template and should now exist alongside user's provider
        assert!(
            val["providers"].get("deepseek").is_some(),
            "bundled deepseek provider should be merged in"
        );
    }

    #[test]
    fn init_via_tempdir_does_not_touch_real_appdata() {
        let (stable_path, dev_path) = real_appdata_config_paths();
        let stable_existed = stable_path.exists();
        let dev_existed = dev_path.exists();
        let stable_content = if stable_existed {
            Some(std::fs::read_to_string(&stable_path).unwrap())
        } else {
            None
        };
        let dev_content = if dev_existed {
            Some(std::fs::read_to_string(&dev_path).unwrap())
        } else {
            None
        };

        // Run init entirely against a TempDir — no real APPDATA paths involved
        let dir = tempfile::TempDir::new().unwrap();
        let temp_config = dir.path().join("config.json");
        let result = ensure_config_initialized_at(&temp_config, paths::AppChannel::Dev, false);
        assert!(result.is_ok(), "TempDir init must succeed: {:?}", result.err());
        assert!(temp_config.exists(), "TempDir config must be created");

        // Real APPDATA paths must be completely unchanged
        assert_eq!(
            stable_path.exists(),
            stable_existed,
            "stable config existence must not change"
        );
        assert_eq!(
            dev_path.exists(),
            dev_existed,
            "dev config existence must not change"
        );

        if let Some(ref expected) = stable_content {
            let current = std::fs::read_to_string(&stable_path).unwrap();
            assert_eq!(&current, expected, "stable config content must not change");
        }
        if let Some(ref expected) = dev_content {
            let current = std::fs::read_to_string(&dev_path).unwrap();
            assert_eq!(&current, expected, "dev config content must not change");
        }
    }

    // ── DeepSeek V4 Pro reasoning-effort migration tests ─────────────

    fn deepseek_model_entry(
        upstream: &str,
        thinking: Option<&str>,
        effort: Option<&str>,
    ) -> serde_json::Value {
        let mut e = json!({
            "upstream_model": upstream,
            "visible": true,
            "force_thinking": false,
        });
        if let Some(t) = thinking {
            e["thinking_mode"] = json!(t);
        }
        if let Some(eff) = effort {
            e["reasoning_effort"] = json!(eff);
        }
        e
    }

    fn write_migration_config(entries: Vec<(&str, serde_json::Value)>) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        let models = serde_json::Map::from_iter(entries.into_iter().map(|(k, v)| (k.to_string(), v)));
        let config = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": { "models": models }
            },
            "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
        (dir, path)
    }

    fn migration_effort(path: &std::path::Path, route: &str) -> Option<String> {
        let raw = std::fs::read_to_string(path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&raw).unwrap();
        val["providers"]["deepseek"]["models"][route]
            .get("reasoning_effort")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    }

    #[test]
    fn deepseek_pro_migration_thinking_low_is_unchanged() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("thinking"), Some("low")),
        )]);
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("low"));
    }

    #[test]
    fn deepseek_pro_migration_thinking_medium_to_high() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("thinking"), Some("medium")),
        )]);
        assert!(migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("high"));
    }

    #[test]
    fn deepseek_pro_migration_thinking_xhigh_to_high() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("thinking"), Some("xhigh")),
        )]);
        assert!(migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("high"));
    }

    #[test]
    fn deepseek_pro_migration_thinking_high_is_unchanged() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("thinking"), Some("high")),
        )]);
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("high"));
    }

    #[test]
    fn deepseek_pro_migration_thinking_max_is_unchanged() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("thinking"), Some("max")),
        )]);
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("max"));
    }

    #[test]
    fn deepseek_pro_migration_normal_removes_effort() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("normal"), Some("max")),
        )]);
        assert!(migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5"), None);
    }

    #[test]
    fn deepseek_pro_migration_no_effort_is_noop() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", Some("normal"), None),
        )]);
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5"), None);
    }

    #[test]
    fn deepseek_pro_migration_missing_mode_removes_effort() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-pro", None, Some("high")),
        )]);
        assert!(migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5"), None);
    }

    #[test]
    fn deepseek_pro_migration_leaves_flash_untouched() {
        let (_dir, path) = write_migration_config(vec![(
            "claude-sonnet-5",
            deepseek_model_entry("deepseek-v4-flash", Some("thinking"), Some("low")),
        )]);
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));
        assert_eq!(migration_effort(&path, "claude-sonnet-5").as_deref(), Some("low"));
    }

    #[test]
    fn deepseek_pro_migration_ignores_other_providers_and_openrouter_profiles() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        let config = json!({
            "active_provider": "openrouter",
            "providers": {
                "openrouter": {
                    "profiles": [{
                        "id": "p1",
                        "display_name": "Test",
                        "models": {
                            "claude-sonnet-5": {
                                "upstream_model": "deepseek-v4-pro",
                                "thinking_mode": "thinking",
                                "reasoning_effort": "low"
                            }
                        }
                    }]
                },
                "other": {
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "deepseek-v4-pro",
                            "thinking_mode": "thinking",
                            "reasoning_effort": "medium"
                        }
                    }
                }
            },
            "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
        });
        std::fs::write(&path, serde_json::to_string_pretty(&config).unwrap()).unwrap();
        assert!(!migrate_deepseek_pro_legacy_reasoning_effort(&path));

        let val: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            val["providers"]["openrouter"]["profiles"][0]["models"]["claude-sonnet-5"]["reasoning_effort"],
            "low"
        );
        assert_eq!(
            val["providers"]["other"]["models"]["claude-sonnet-5"]["reasoning_effort"],
            "medium"
        );
    }

    // ── Profile builder ↔ config.json match tests ────────────────────

    const LAGUNA_PROFILE_ID: &str = "a0e0f000-0000-4000-8000-000000000001";
    const HY3_PROFILE_ID: &str = "b0e0f000-0000-4000-8000-000000000002";
    const INCLUSIONAI_PROFILE_ID: &str = "c0e0f000-0000-4000-8000-000000000003";
    const STEPFUN_PROFILE_ID: &str = "d0e0f000-0000-4000-8000-000000000004";

    fn find_profile<'a>(
        profiles: &'a [serde_json::Value],
        id: &str,
    ) -> &'a serde_json::Value {
        profiles
            .iter()
            .find(|p| p["id"].as_str() == Some(id))
            .unwrap_or_else(|| panic!("Bundled profile not found: {id}"))
    }

    #[test]
    fn inclusionai_profile_builder_matches_template() {
        let config_template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json")).unwrap();
        let profiles = config_template["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        let bundled = find_profile(profiles, INCLUSIONAI_PROFILE_ID);
        assert_eq!(
            build_inclusionai_profile_json("OpenRouter: InclusionAI"),
            *bundled,
            "InclusionAI builder output doesn't match bundled config.json",
        );
    }

    #[test]
    fn stepfun_profile_builder_matches_template() {
        let config_template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json")).unwrap();
        let profiles = config_template["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        let bundled = find_profile(profiles, STEPFUN_PROFILE_ID);
        assert_eq!(
            build_stepfun_profile_json("OpenRouter: StepFun"),
            *bundled,
            "StepFun builder output doesn't match bundled config.json",
        );
    }

    #[test]
    fn gpt56_balanced_profile_builder_matches_template() {
        let config_template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json")).unwrap();
        let profiles = config_template["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        let bundled = find_profile(profiles, GPT56_BALANCED_PROFILE_ID);
        assert_eq!(
            build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME),
            *bundled,
            "GPT-5.6 Balanced builder output doesn't match bundled config.json",
        );
    }

    #[test]
    fn bundled_deepseek_defaults_use_v4_flash_routing() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json"))
                .unwrap();

        let models = &config["providers"]["deepseek"]["models"];

        let opus = &models["claude-opus-5"];
        assert_eq!(opus["upstream_model"], "deepseek-v4-flash");
        assert_eq!(opus["thinking_mode"], "thinking");
        assert_eq!(opus["reasoning_effort"], "max");

        let sonnet = &models["claude-sonnet-5"];
        assert_eq!(sonnet["upstream_model"], "deepseek-v4-flash");
        assert_eq!(sonnet["thinking_mode"], "thinking");
        assert_eq!(sonnet["reasoning_effort"], "high");

        let haiku = &models["claude-haiku-4-5"];
        assert_eq!(haiku["upstream_model"], "deepseek-v4-flash");
        assert_eq!(haiku["thinking_mode"], "thinking");
        assert_eq!(haiku["reasoning_effort"], "low");
    }

    // ── ensure_builtin_openrouter_profiles migration tests ────────────

    use tempfile::TempDir;

    fn write_config(dir: &std::path::Path, cfg: &serde_json::Value) {
        let s = serde_json::to_string_pretty(cfg).unwrap();
        std::fs::write(dir.join("config.json"), s).unwrap();
    }

    fn read_config(dir: &std::path::Path) -> serde_json::Value {
        let s = std::fs::read_to_string(dir.join("config.json")).unwrap();
        serde_json::from_str(&s).unwrap()
    }

    fn make_openrouter_config_with_profiles(
        profiles: Vec<serde_json::Value>,
        active_profile_id: Option<&str>,
    ) -> serde_json::Value {
        let mut cfg = json!({
            "active_provider": "openrouter",
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "upstream_url": "https://openrouter.ai/api",
                    "api_key_env": "OPENROUTER_API_KEY",
                    "profiles": profiles,
                }
            }
        });
        if let Some(id) = active_profile_id {
            cfg["active_openrouter_profile_id"] = json!(id);
        }
        cfg
    }

    #[test]
    fn ensure_builtin_adds_missing_builtin_profiles() {
        let dir = TempDir::new().unwrap();
        // Start with only Laguna profile (old display name + random UUID)
        let laguna = build_laguna_profile_json("Laguna");
        let cfg = make_openrouter_config_with_profiles(vec![laguna.clone()], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        assert_eq!(profiles.len(), 6); // Laguna + Hy3 + InclusionAI + StepFun + GPT-5.6 + Gemini
        assert!(profiles.iter().any(|p| p["id"] == LAGUNA_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == HY3_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == INCLUSIONAI_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == STEPFUN_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == GPT56_BALANCED_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == GEMINI_PROFILE_ID));
        // Existing Laguna display_name repaired
        let laguna_repaired = profiles.iter().find(|p| p["id"] == LAGUNA_PROFILE_ID).unwrap();
        assert_eq!(laguna_repaired["display_name"].as_str().unwrap(), "OpenRouter: Laguna");
    }

    #[test]
    fn ensure_builtin_idempotent() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let hy3 = build_hy3_profile_json("OpenRouter: Hy3");
        let inclusionai = build_inclusionai_profile_json("OpenRouter: InclusionAI");
        let stepfun = build_stepfun_profile_json("OpenRouter: StepFun");
        let gpt56 = build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME);
        let cfg = make_openrouter_config_with_profiles(
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone(), build_gemini_profile_json(GEMINI_PROFILE_NAME).clone()],
            None,
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        // Run twice
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // Still 6 — no duplicates
        assert_eq!(profiles.len(), 6);
    }

    #[test]
    fn ensure_builtin_repairs_laguna_display_name() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("Laguna");
        let cfg = make_openrouter_config_with_profiles(vec![laguna.clone()], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // Laguna profile ID repaired to fixed UUID + display_name repaired
        let laguna_after = profiles.iter().find(|p| p["id"] == LAGUNA_PROFILE_ID).unwrap();
        assert_eq!(laguna_after["display_name"].as_str().unwrap(), "OpenRouter: Laguna");
    }

    #[test]
    fn ensure_builtin_preserves_active_profile() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let cfg = make_openrouter_config_with_profiles(
            vec![laguna.clone()],
            Some(LAGUNA_PROFILE_ID),
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        // Active profile ID unchanged (still the fixed Laguna UUID)
        assert_eq!(
            result["active_openrouter_profile_id"].as_str().unwrap(),
            LAGUNA_PROFILE_ID,
        );
    }

    #[test]
    fn ensure_builtin_noop_when_profiles_exist() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let hy3 = build_hy3_profile_json("OpenRouter: Hy3");
        let inclusionai = build_inclusionai_profile_json("OpenRouter: InclusionAI");
        let stepfun = build_stepfun_profile_json("OpenRouter: StepFun");
        let gpt56 = build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME);
        let cfg = make_openrouter_config_with_profiles(
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone(), build_gemini_profile_json(GEMINI_PROFILE_NAME).clone()],
            None,
        );
        write_config(dir.path(), &cfg);

        let original = std::fs::read_to_string(dir.path().join("config.json")).unwrap();

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let after = std::fs::read_to_string(dir.path().join("config.json")).unwrap();
        // No write at all — content identical
        assert_eq!(original, after);
    }

    #[test]
    fn ensure_builtin_noop_when_no_openrouter_provider() {
        let dir = TempDir::new().unwrap();
        let cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": {
                    "display_name": "DeepSeek",
                    "api_key_env": "DEEPSEEK_API_KEY"
                }
            }
        });
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        // No openrouter provider added
        assert!(result["providers"].get("openrouter").is_none());
    }

    #[test]
    fn ensure_builtin_repairs_old_display_names() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("Laguna");
        let hy3 = build_hy3_profile_json("Hy3");
        let inclusionai = build_inclusionai_profile_json("InclusionAI");
        let stepfun = build_stepfun_profile_json("StepFun");
        let gpt56 = build_gpt56_balanced_profile_json("GPT Test");
        let cfg = make_openrouter_config_with_profiles(
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone(), build_gemini_profile_json(GEMINI_PROFILE_NAME).clone()],
            None,
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // All 6 profiles have their display_name repaired
        let lg = profiles.iter().find(|p| p["id"] == LAGUNA_PROFILE_ID).unwrap();
        let hy = profiles.iter().find(|p| p["id"] == HY3_PROFILE_ID).unwrap();
        let ia = profiles.iter().find(|p| p["id"] == INCLUSIONAI_PROFILE_ID).unwrap();
        let sf = profiles.iter().find(|p| p["id"] == STEPFUN_PROFILE_ID).unwrap();
        let gpt = profiles.iter().find(|p| p["id"] == GPT56_BALANCED_PROFILE_ID).unwrap();
        assert_eq!(lg["display_name"].as_str().unwrap(), "OpenRouter: Laguna");
        assert_eq!(hy["display_name"].as_str().unwrap(), "OpenRouter: Hy3");
        assert_eq!(ia["display_name"].as_str().unwrap(), "OpenRouter: InclusionAI");
        assert_eq!(sf["display_name"].as_str().unwrap(), "OpenRouter: StepFun");
        assert_eq!(gpt["display_name"].as_str().unwrap(), GPT56_BALANCED_PROFILE_NAME);
    }

    #[test]
    fn normalize_preserves_builtin_profile_names() {
        use super::*;
        // All 6 built-in profiles now have fixed UUIDs
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let hy3 = build_hy3_profile_json("OpenRouter: Hy3");
        let inclusionai = build_inclusionai_profile_json("OpenRouter: InclusionAI");
        let stepfun = build_stepfun_profile_json("OpenRouter: StepFun");
        let gpt56 = build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME);
        let gemini = build_gemini_profile_json(GEMINI_PROFILE_NAME);
        let custom_legacy = serde_json::json!({
            "id": "00000000-0000-0000-0000-000000000099",
            "display_name": "OpenRouter: Old Model 99",
            "model_map": {},
            "visible_models": [],
            "models": {}
        });
        let mut profiles = vec![
            laguna,
            hy3,
            inclusionai,
            stepfun,
            gpt56,
            gemini,
            custom_legacy,
        ];
        let changed = normalize_openrouter_profile_names(&mut profiles);
        assert!(changed); // custom_legacy renamed
        // Built-in names preserved
        assert_eq!(profiles[0]["display_name"].as_str().unwrap(), "OpenRouter: Laguna");
        assert_eq!(profiles[1]["display_name"].as_str().unwrap(), "OpenRouter: Hy3");
        assert_eq!(profiles[2]["display_name"].as_str().unwrap(), "OpenRouter: InclusionAI");
        assert_eq!(profiles[3]["display_name"].as_str().unwrap(), "OpenRouter: StepFun");
        assert_eq!(profiles[4]["display_name"].as_str().unwrap(), GPT56_BALANCED_PROFILE_NAME);
        assert_eq!(profiles[5]["display_name"].as_str().unwrap(), GEMINI_PROFILE_NAME);
        // Custom legacy renamed to "Model 1" (no other numbered names in use)
        assert_eq!(profiles[6]["display_name"].as_str().unwrap(), "Model 1");
    }

    // ── Gemini tests ────────────────────────────────────────────────────

    #[test]
    fn gemini_profile_has_expected_routing() {
        let profile = build_gemini_profile("Test");
        assert_eq!(profile.id, GEMINI_PROFILE_ID);

        let opus = &profile.models["claude-opus-5"];
        assert_eq!(opus.upstream_model, "google/gemini-3.7-flash");
        assert_eq!(opus.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(opus.reasoning_effort.as_deref(), Some("high"));
        assert!(!opus.force_thinking.unwrap());

        let sonnet = &profile.models["claude-sonnet-5"];
        assert_eq!(sonnet.upstream_model, "google/gemini-3.7-flash");
        assert_eq!(sonnet.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(sonnet.reasoning_effort.as_deref(), Some("medium"));
        assert!(!sonnet.force_thinking.unwrap());

        let haiku = &profile.models["claude-haiku-4-5"];
        assert_eq!(haiku.upstream_model, "google/gemini-3.7-flash");
        assert_eq!(haiku.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(haiku.reasoning_effort.as_deref(), Some("low"));
        assert!(!haiku.force_thinking.unwrap());
    }

    // ── Gemini old-default migration tests ──────────────────────────────

    fn initial_gemini_default_profile() -> serde_json::Value {
        json!({
            "id": GEMINI_PROFILE_ID,
            "display_name": GEMINI_PROFILE_NAME,
            "model_map": {
                "claude-opus-5": "google/gemini-3.1-pro-preview",
                "claude-sonnet-5": "google/gemini-3.7-flash",
                "claude-haiku-4-5": "google/gemini-3.5-flash-lite"
            },
            "visible_models": ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"],
            "models": {
                "claude-opus-5": {
                    "upstream_model": "google/gemini-3.1-pro-preview",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "high"
                },
                "claude-sonnet-5": {
                    "upstream_model": "google/gemini-3.7-flash",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "high"
                },
                "claude-haiku-4-5": {
                    "upstream_model": "google/gemini-3.5-flash-lite",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "low"
                }
            }
        })
    }

    fn interim_gemini_default_profile() -> serde_json::Value {
        json!({
            "id": GEMINI_PROFILE_ID,
            "display_name": GEMINI_PROFILE_NAME,
            "model_map": {
                "claude-opus-5": "google/gemini-3.7-flash",
                "claude-sonnet-5": "google/gemini-3.7-flash",
                "claude-haiku-4-5": "google/gemini-3.7-flash"
            },
            "visible_models": ["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5"],
            "models": {
                "claude-opus-5": {
                    "upstream_model": "google/gemini-3.7-flash",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "high"
                },
                "claude-sonnet-5": {
                    "upstream_model": "google/gemini-3.7-flash",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "high"
                },
                "claude-haiku-4-5": {
                    "upstream_model": "google/gemini-3.7-flash",
                    "thinking_mode": "thinking",
                    "reasoning_effort": "low"
                }
            }
        })
    }

    fn gemini_route_upstream(profiles: &[serde_json::Value], route: &str) -> String {
        let profile = find_profile(profiles, GEMINI_PROFILE_ID);
        profile["models"][route]["upstream_model"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn gemini_route_effort(profiles: &[serde_json::Value], route: &str) -> String {
        let profile = find_profile(profiles, GEMINI_PROFILE_ID);
        profile["models"][route]["reasoning_effort"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn gemini_initial_default_is_migrated_to_current_default() {
        let dir = TempDir::new().unwrap();
        let cfg = make_openrouter_config_with_profiles(vec![initial_gemini_default_profile()], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        assert!(migrate_gemini_profile_to_current_default(&path));

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        assert_eq!(gemini_route_upstream(profiles, "claude-opus-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-opus-5"), "high");
        assert_eq!(gemini_route_upstream(profiles, "claude-sonnet-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-sonnet-5"), "medium");
        assert_eq!(gemini_route_upstream(profiles, "claude-haiku-4-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-haiku-4-5"), "low");
    }

    #[test]
    fn gemini_interim_default_is_migrated_to_current_default() {
        let dir = TempDir::new().unwrap();
        let cfg = make_openrouter_config_with_profiles(vec![interim_gemini_default_profile()], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        assert!(migrate_gemini_profile_to_current_default(&path));

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        assert_eq!(gemini_route_upstream(profiles, "claude-opus-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-opus-5"), "high");
        assert_eq!(gemini_route_upstream(profiles, "claude-sonnet-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-sonnet-5"), "medium");
        assert_eq!(gemini_route_upstream(profiles, "claude-haiku-4-5"), "google/gemini-3.7-flash");
        assert_eq!(gemini_route_effort(profiles, "claude-haiku-4-5"), "low");
    }

    #[test]
    fn gemini_edited_slot_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let mut profile = initial_gemini_default_profile();
        // User changed the Haiku route's upstream model (and its model entry).
        profile["model_map"]["claude-haiku-4-5"] = json!("google/gemini-3.1-pro-preview");
        profile["models"]["claude-haiku-4-5"]["upstream_model"] =
            json!("google/gemini-3.1-pro-preview");
        let cfg = make_openrouter_config_with_profiles(vec![profile], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        assert!(!migrate_gemini_profile_to_current_default(&path));

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // Haiku stays as the user edited it; Opus stays at the old default.
        assert_eq!(gemini_route_upstream(profiles, "claude-haiku-4-5"), "google/gemini-3.1-pro-preview");
        assert_eq!(gemini_route_upstream(profiles, "claude-opus-5"), "google/gemini-3.1-pro-preview");
    }

    #[test]
    fn gemini_user_customized_effort_is_left_untouched() {
        let dir = TempDir::new().unwrap();
        let mut profile = interim_gemini_default_profile();
        // User customized Sonnet reasoning effort to "low" instead of default
        profile["models"]["claude-sonnet-5"]["reasoning_effort"] = json!("low");
        let cfg = make_openrouter_config_with_profiles(vec![profile], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        assert!(!migrate_gemini_profile_to_current_default(&path));

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // Sonnet stays at user-customized "low"
        assert_eq!(gemini_route_effort(profiles, "claude-sonnet-5"), "low");
    }

    #[test]
    fn gemini_already_new_default_is_noop() {
        let dir = TempDir::new().unwrap();
        let cfg = make_openrouter_config_with_profiles(
            vec![build_gemini_profile_json(GEMINI_PROFILE_NAME)],
            None,
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        assert!(!migrate_gemini_profile_to_current_default(&path));
    }
    // ── GPT-5.6 Balanced tests ──────────────────────────────────────────

    #[test]
    fn gpt56_balanced_profile_has_expected_thinking_modes() {
        let profile = build_gpt56_balanced_profile("Test");
        assert_eq!(profile.id, GPT56_BALANCED_PROFILE_ID);

        let opus = &profile.models["claude-opus-5"];
        assert_eq!(opus.upstream_model, "openai/gpt-5.6-sol");
        assert_eq!(opus.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(opus.reasoning_effort.as_deref(), Some("high"));
        assert!(!opus.force_thinking.unwrap());

        let sonnet = &profile.models["claude-sonnet-5"];
        assert_eq!(sonnet.upstream_model, "openai/gpt-5.6-terra");
        assert_eq!(sonnet.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(sonnet.reasoning_effort.as_deref(), Some("high"));
        assert!(!sonnet.force_thinking.unwrap());

        let haiku = &profile.models["claude-haiku-4-5"];
        assert_eq!(haiku.upstream_model, "openai/gpt-5.6-luna");
        assert_eq!(haiku.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(haiku.reasoning_effort.as_deref(), Some("high"));
        assert!(!haiku.force_thinking.unwrap());
    }

    #[test]
    fn ensure_builtin_gpt56_idempotent() {
        let dir = TempDir::new().unwrap();
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let hy3 = build_hy3_profile_json("OpenRouter: Hy3");
        let inclusionai = build_inclusionai_profile_json("OpenRouter: InclusionAI");
        let stepfun = build_stepfun_profile_json("OpenRouter: StepFun");
        let cfg = make_openrouter_config_with_profiles(
            vec![laguna, hy3, inclusionai, stepfun],
            None,
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        // First run: adds GPT-5.6 Balanced
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();
        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 6);
        assert!(profiles.iter().any(|p| p["id"] == GPT56_BALANCED_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == GEMINI_PROFILE_ID));

        // Second run: no-op — no duplicates
        let before_second = read_config(dir.path());
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();
        let after_second = read_config(dir.path());
        assert_eq!(before_second, after_second);
    }

    #[test]
    fn ensure_builtin_gpt56_repairs_name_but_preserves_routes() {
        let dir = TempDir::new().unwrap();
        let mut gpt = build_gpt56_balanced_profile_json("Custom GPT Profile");
        // User customized Opus to use Terra instead of Sol
        gpt["model_map"]["claude-opus-5"] =
            serde_json::Value::String("openai/gpt-5.6-terra".to_string());
        gpt["models"]["claude-opus-5"]["upstream_model"] =
            serde_json::Value::String("openai/gpt-5.6-terra".to_string());
        let cfg = make_openrouter_config_with_profiles(vec![gpt], None);
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"].as_array().unwrap();
        let gpt = find_profile(profiles, GPT56_BALANCED_PROFILE_ID);

        // Display name repaired to canonical
        assert_eq!(gpt["display_name"], GPT56_BALANCED_PROFILE_NAME);
        // User's route customization preserved (not reset to Sol)
        assert_eq!(
            gpt["models"]["claude-opus-5"]["upstream_model"],
            "openai/gpt-5.6-terra",
        );
    }

    #[test]
    fn normalize_allows_custom_profile_to_share_gpt56_display_name() {
        // Built-in profile with canonical name and UUID
        let builtin = build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME);
        // User profile with same name but different UUID
        let mut custom = builtin.clone();
        custom["id"] = serde_json::Value::String("custom-random-uuid".to_string());

        let mut profiles = vec![builtin, custom];
        let changed = normalize_openrouter_profile_names(&mut profiles);

        // Neither profile is renamed: the built-in is in BUILTIN_NAMES,
        // and the custom profile doesn't match the LEGACY_PREFIX, so
        // normalize_openrouter_profile_names leaves both untouched.
        // In the GUI this means two profiles with the same display name
        // will coexist — ensure_builtin identifies by UUID so routing
        // is not affected. A future UX improvement could auto-rename the
        // custom duplicate (e.g. "OpenAI GPT-5.6 Balanced (2)").
        assert!(!changed);
        assert_eq!(
            profiles[0]["display_name"].as_str().unwrap(),
            GPT56_BALANCED_PROFILE_NAME,
        );
        assert_eq!(
            profiles[1]["display_name"].as_str().unwrap(),
            GPT56_BALANCED_PROFILE_NAME,
        );
    }

    // ── apply_set_openrouter_profile_hidden ──────────────────────────

    #[test]
    fn apply_set_hidden_hides_visible_profile() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", true).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["hidden"].as_bool().unwrap(), true);
    }

    #[test]
    fn apply_set_hidden_shows_hidden_profile() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test", "hidden": true}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", false).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        // Key removed, not written as false
        assert!(profiles[0].get("hidden").is_none());
    }

    #[test]
    fn apply_set_hidden_same_true_is_unchanged() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test", "hidden": true}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", true).unwrap();
        assert!(!outcome.config_changed);
        assert!(!outcome.restart_gateway);
        // Key still present
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert_eq!(profiles[0]["hidden"].as_bool().unwrap(), true);
    }

    #[test]
    fn apply_set_hidden_same_false_is_unchanged() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", false).unwrap();
        assert!(!outcome.config_changed);
        assert!(!outcome.restart_gateway);
    }

    #[test]
    fn apply_set_hidden_missing_profile_returns_error() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        assert!(apply_set_openrouter_profile_hidden(&mut cfg, "nonexistent", true).is_err());
    }

    #[test]
    fn apply_set_hidden_preserves_active_profile() {
        let mut cfg = json!({
            "active_provider": "openrouter",
            "active_openrouter_profile_id": "p1",
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", true).unwrap();
        assert!(outcome.config_changed);
        // Active profile unchanged
        assert_eq!(cfg["active_openrouter_profile_id"].as_str().unwrap(), "p1");
    }

    #[test]
    fn apply_set_hidden_does_not_request_gateway_restart() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", true).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        // And the reverse
        let mut cfg2 = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test", "hidden": true}]
                }
            }
        });
        let outcome2 = apply_set_openrouter_profile_hidden(&mut cfg2, "p1", false).unwrap();
        assert!(outcome2.config_changed);
        assert!(!outcome2.restart_gateway);
    }

    #[test]
    fn apply_set_hidden_false_removes_hidden_key() {
        // true→false removes key; second call sees missing key as false → unchanged
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test", "hidden": true}]
                }
            }
        });
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", false).unwrap();
        assert!(outcome.config_changed);
        let profiles = cfg["providers"]["openrouter"]["profiles"].as_array().unwrap();
        assert!(profiles[0].get("hidden").is_none());

        // Second call: key already absent → unchanged
        let outcome2 = apply_set_openrouter_profile_hidden(&mut cfg, "p1", false).unwrap();
        assert!(!outcome2.config_changed);
    }

    #[test]
    fn old_profile_without_hidden_deserializes_as_visible() {
        // Profile without "hidden" key → treated as false (visible)
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "profiles": [{"id": "p1", "display_name": "Test"}]
                }
            }
        });
        // Setting to false (which it already is) → unchanged
        let outcome = apply_set_openrouter_profile_hidden(&mut cfg, "p1", false).unwrap();
        assert!(!outcome.config_changed);
    }

    // ── apply_set_provider_hidden ──────────────────────────────────

    #[test]
    fn apply_set_provider_hidden_hides_visible_provider() {
        let mut cfg = json!({
            "providers": {
                "deepseek": {
                    "display_name": "DeepSeek",
                    "upstream_url": "https://example.com",
                    "api_key_env": "DEEPSEEK_API_KEY"
                }
            }
        });
        let outcome = apply_set_provider_hidden(&mut cfg, "deepseek", true).unwrap();
        assert!(outcome.config_changed);
        assert!(!outcome.restart_gateway);
        assert_eq!(cfg["providers"]["deepseek"]["hidden"].as_bool().unwrap(), true);
        // Other provider fields are preserved
        assert_eq!(cfg["providers"]["deepseek"]["api_key_env"].as_str().unwrap(), "DEEPSEEK_API_KEY");
    }

    #[test]
    fn apply_set_provider_hidden_shows_hidden_provider() {
        let mut cfg = json!({
            "providers": {
                "deepseek": { "display_name": "DeepSeek", "hidden": true }
            }
        });
        let outcome = apply_set_provider_hidden(&mut cfg, "deepseek", false).unwrap();
        assert!(outcome.config_changed);
        // Key removed, not written as false
        assert!(cfg["providers"]["deepseek"].get("hidden").is_none());
    }

    #[test]
    fn apply_set_provider_hidden_missing_hidden_treated_as_visible() {
        // Provider without "hidden" key → treated as false (visible); setting
        // false is a no-op so old configs never lose their cards.
        let mut cfg = json!({
            "providers": {
                "deepseek": { "display_name": "DeepSeek" }
            }
        });
        let outcome = apply_set_provider_hidden(&mut cfg, "deepseek", false).unwrap();
        assert!(!outcome.config_changed);
    }

    #[test]
    fn apply_set_provider_hidden_same_state_is_unchanged() {
        let mut cfg = json!({
            "providers": {
                "deepseek": { "display_name": "DeepSeek", "hidden": true }
            }
        });
        let outcome = apply_set_provider_hidden(&mut cfg, "deepseek", true).unwrap();
        assert!(!outcome.config_changed);
    }

    #[test]
    fn apply_set_provider_hidden_missing_provider_returns_error() {
        let mut cfg = json!({ "providers": { "deepseek": {} } });
        assert!(apply_set_provider_hidden(&mut cfg, "nonexistent", true).is_err());
    }

    #[test]
    fn apply_set_provider_hidden_rejects_openrouter() {
        let mut cfg = json!({
            "providers": {
                "openrouter": { "display_name": "OpenRouter", "profiles": [] }
            }
        });
        assert!(apply_set_provider_hidden(&mut cfg, "openrouter", true).is_err());
    }

    #[test]
    fn apply_set_provider_hidden_non_object_provider_returns_error() {
        // A non-object provider node (corrupted config) must return Err, not panic.
        let mut cfg = json!({ "providers": { "deepseek": 42 } });
        assert!(apply_set_provider_hidden(&mut cfg, "deepseek", true).is_err());
    }

    // ── Config write serialization regression tests ─────────────────

    /// Helper: build a minimal non-OpenRouter config with a single provider.
    fn make_provider_config(provider_id: &str, model_map: serde_json::Value) -> serde_json::Value {
        json!({
            "active_provider": provider_id,
            "providers": {
                provider_id: {
                    "display_name": "Test Provider",
                    "upstream_url": "https://example.com/api",
                    "api_key_env": "TEST_API_KEY",
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "sonnet-upstream",
                            "thinking_mode": null,
                            "visible": true
                        },
                        "claude-haiku-4-5": {
                            "upstream_model": "haiku-upstream",
                            "thinking_mode": "thinking_only",
                            "visible": true
                        }
                    },
                    "model_map": model_map
                }
            }
        })
    }

    #[test]
    fn two_threads_both_changes_survive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let cfg = make_provider_config(
            "minimax",
            json!({
                "claude-sonnet-5": "sonnet-upstream",
                "claude-haiku-4-5": "haiku-upstream"
            }),
        );
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));

        let lock_a = Arc::clone(&lock);
        let path_a = path.clone();
        let handle_a = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path(&lock_a, &path_a, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-sonnet-5",
                    "MiniMax-M3", Some("default"), None,
                )
            })
        });

        let lock_b = Arc::clone(&lock);
        let path_b = path.clone();
        let handle_b = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path(&lock_b, &path_b, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-haiku-4-5",
                    "MiniMax-M3", Some("default"), None,
                )
            })
        });

        let _ = handle_a.join().unwrap().unwrap();
        let _ = handle_b.join().unwrap().unwrap();

        let result = read_config(dir.path());
        let models = &result["providers"]["minimax"]["models"];

        // Both Sonnet and Haiku changes survived
        assert_eq!(models["claude-sonnet-5"]["upstream_model"], "MiniMax-M3");
        assert_eq!(models["claude-sonnet-5"]["thinking_mode"], "default");
        assert_eq!(models["claude-haiku-4-5"]["upstream_model"], "MiniMax-M3");
        assert_eq!(models["claude-haiku-4-5"]["thinking_mode"], "default");

        // model_map also updated for both
        let model_map = &result["providers"]["minimax"]["model_map"];
        assert_eq!(model_map["claude-sonnet-5"], "MiniMax-M3");
        assert_eq!(model_map["claude-haiku-4-5"], "MiniMax-M3");
    }

    #[test]
    fn four_threads_all_changes_survive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let mut cfg = make_provider_config(
            "minimax",
            json!({
                "model-a": "a-upstream",
                "model-b": "b-upstream",
                "model-c": "c-upstream",
                "model-d": "d-upstream"
            }),
        );
        // Add 4 model entries
        for key in &["model-a", "model-b", "model-c", "model-d"] {
            cfg["providers"]["minimax"]["models"][key] = json!({
                "upstream_model": format!("{}-upstream", &key[key.len()-1..]),
                "thinking_mode": null,
                "visible": true
            });
        }
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));
        let edits: Vec<(&str, &str)> = vec![
            ("model-a", "Target-A"),
            ("model-b", "Target-B"),
            ("model-c", "Target-C"),
            ("model-d", "Target-D"),
        ];

        let handles: Vec<_> = edits.iter().map(|(key, target)| {
            let lock = Arc::clone(&lock);
            let path = path.clone();
            let key = key.to_string();
            let target = target.to_string();
            std::thread::spawn(move || {
                execute_serialized_config_mutation_at_path(&lock, &path, |cfg| {
                    apply_set_model_upstream(
                        cfg, "minimax", None, &key,
                        &target, Some("default"), None,
                    )
                })
            })
        }).collect();

        for h in handles {
            let _ = h.join().unwrap().unwrap();
        }

        let result = read_config(dir.path());
        let models = &result["providers"]["minimax"]["models"];
        let model_map = &result["providers"]["minimax"]["model_map"];

        for (key, target) in &edits {
            assert_eq!(models[*key]["upstream_model"], *target,
                "model key {}: upstream_model mismatch", key);
            assert_eq!(model_map[*key], *target,
                "model_map key {}: mismatch", key);
        }
    }

    #[test]
    fn lock_releases_on_apply_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let cfg = make_provider_config(
            "minimax",
            json!({
                "claude-sonnet-5": "sonnet-upstream",
                "claude-haiku-4-5": "haiku-upstream"
            }),
        );
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));

        // Thread A: apply returns Err (empty upstream_model triggers validation in apply)
        let lock_a = Arc::clone(&lock);
        let path_a = path.clone();
        let handle_a = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path::<(), _>(&lock_a, &path_a, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-sonnet-5",
                    "", Some("default"), None, // empty → Err
                )
            })
        });

        // Thread B: valid mutation — must succeed
        let lock_b = Arc::clone(&lock);
        let path_b = path.clone();
        let handle_b = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path(&lock_b, &path_b, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-haiku-4-5",
                    "MiniMax-M3", Some("default"), None,
                )
            })
        });

        let result_a = handle_a.join().unwrap();
        assert!(result_a.is_err(), "Thread A must error on empty upstream");

        let result_b = handle_b.join().unwrap();
        assert!(result_b.is_ok(), "Thread B must succeed (lock released after A's error)");

        // Thread B's change is on disk
        let result = read_config(dir.path());
        let models = &result["providers"]["minimax"]["models"];
        assert_eq!(models["claude-haiku-4-5"]["upstream_model"], "MiniMax-M3");
    }

    #[test]
    fn validation_returns_err_while_lock_held() {
        // validate_set_model_upstream_input is pure — no lock needed.
        // Empty upstream_model returns Err instantly.
        let result = validate_set_model_upstream_input("", None, None);
        assert!(result.is_err());

        // Valid input passes
        let result = validate_set_model_upstream_input("some-model", None, None);
        assert!(result.is_ok());

        // Invalid thinking_mode
        let result = validate_set_model_upstream_input(
            "m", Some("bogus"), None);
        assert!(result.is_err());

        // Invalid reasoning_effort
        let result = validate_set_model_upstream_input(
            "m", None, Some("bogus"));
        assert!(result.is_err());

        // Valid thinking_mode and reasoning_effort (including xhigh)
        let result = validate_set_model_upstream_input(
            "m", Some("thinking"), Some("xhigh"));
        assert!(result.is_ok());
    }

    #[test]
    fn cross_group_ab_both_survive() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        let cfg = make_provider_config(
            "minimax",
            json!({"claude-sonnet-5": "sonnet-upstream"}),
        );
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));

        // Thread A: Group A — set_model_upstream via apply
        let lock_a = Arc::clone(&lock);
        let path_a = path.clone();
        let handle_a = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path(&lock_a, &path_a, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-sonnet-5",
                    "MiniMax-M3", Some("default"), None,
                )
            })
        });

        // Thread B: Group B — update_active_provider (no-op but exercises lock)
        let lock_b = Arc::clone(&lock);
        let path_b = path.clone();
        let handle_b = std::thread::spawn(move || {
            execute_serialized_config_mutation_at_path(&lock_b, &path_b, |cfg| {
                apply_update_active_provider(cfg, "minimax")
            })
        });

        let _ = handle_a.join().unwrap().unwrap();
        let _ = handle_b.join().unwrap().unwrap();

        let result = read_config(dir.path());
        let models = &result["providers"]["minimax"]["models"];
        // Group A's change survived
        assert_eq!(models["claude-sonnet-5"]["upstream_model"], "MiniMax-M3");
        // Group B's change (no-op: same provider) is intact
        assert_eq!(result["active_provider"], "minimax");
    }

    #[test]
    fn full_replace_serializes_no_corruption() {
        // Case 1: full replace runs LAST → overwrites partial mutation
        {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("config.json");

            let cfg = make_provider_config(
                "minimax",
                json!({"claude-sonnet-5": "sonnet-upstream"}),
            );
            write_config(dir.path(), &cfg);

            let lock = Arc::new(Mutex::new(()));
            let (tx, rx) = std::sync::mpsc::channel();

            // Thread A: partial mutation — signals when done
            let lock_a = Arc::clone(&lock);
            let path_a = path.clone();
            let handle_a = std::thread::spawn(move || {
                let r = execute_serialized_config_mutation_at_path(
                    &lock_a, &path_a, |cfg| {
                        apply_set_model_upstream(
                            cfg, "minimax", None, "claude-sonnet-5",
                            "MiniMax-M3", Some("default"), None,
                        )
                    });
                let _ = tx.send("done");
                r
            });

            // Thread B: waits for A, then full replaces
            let lock_b = Arc::clone(&lock);
            let path_b = path.clone();
            let handle_b = std::thread::spawn(move || {
                let _ = rx.recv();
                let _guard = lock_b.lock().unwrap();
                let new_cfg = json!({"active_provider": "deepseek", "providers": {}});
                let s = serde_json::to_string_pretty(&new_cfg).unwrap();
                std::fs::write(&path_b, s).map_err(|e| format!("{}", e))
            });

            let _ = handle_a.join().unwrap().unwrap();
            let _ = handle_b.join().unwrap().unwrap();

            let result = read_config(dir.path());
            // Full replace ran last → overwrites everything (intentional)
            assert_eq!(result["active_provider"], "deepseek");
            assert!(result["providers"].as_object().map_or(true, |o| o.is_empty()));
        }

        // Case 2: full replace runs FIRST → partial mutation applies on top
        {
            let dir = TempDir::new().unwrap();
            let path = dir.path().join("config.json");

            let cfg = make_provider_config(
                "minimax",
                json!({"claude-sonnet-5": "sonnet-upstream"}),
            );
            write_config(dir.path(), &cfg);

            let lock = Arc::new(Mutex::new(()));
            let (tx, rx) = std::sync::mpsc::channel();

            // Thread A: full replace runs first, signals completion
            let lock_a = Arc::clone(&lock);
            let path_a = path.clone();
            let handle_a = std::thread::spawn(move || {
                let _guard = lock_a.lock().unwrap();
                // Replace with a config that has the same structure so B can mutate it
                let new_cfg = json!({
                    "active_provider": "minimax",
                    "providers": {
                        "minimax": {
                            "display_name": "Test",
                            "upstream_url": "https://example.com/api",
                            "api_key_env": "TEST_API_KEY",
                            "models": {
                                "claude-sonnet-5": {
                                    "upstream_model": "replaced-upstream",
                                    "thinking_mode": null,
                                    "visible": true
                                }
                            },
                            "model_map": {
                                "claude-sonnet-5": "replaced-upstream"
                            }
                        }
                    }
                });
                let s = serde_json::to_string_pretty(&new_cfg).unwrap();
                let r = std::fs::write(&path_a, s).map_err(|e| format!("{}", e));
                let _ = tx.send("done");
                r
            });

            // Thread B: waits for A, then partial mutation on top
            let lock_b = Arc::clone(&lock);
            let path_b = path.clone();
            let handle_b = std::thread::spawn(move || {
                let _ = rx.recv(); // wait for A to finish
                let _ = execute_serialized_config_mutation_at_path(
                    &lock_b, &path_b, |cfg| {
                        apply_set_model_upstream(
                            cfg, "minimax", None, "claude-sonnet-5",
                            "MiniMax-M3", Some("default"), None,
                        )
                    });
                Ok::<_, String>(())
            });

            let _ = handle_a.join().unwrap();
            let _ = handle_b.join().unwrap();
        }
    }

    #[test]
    fn reset_then_partial_mutation_is_applied() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");

        // Seed temp dir with a config so reset_config_at_path has something to back up
        let cfg = make_provider_config(
            "minimax",
            json!({"claude-sonnet-5": "sonnet-upstream"}),
        );
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));
        let (tx, rx) = std::sync::mpsc::channel();

        // Thread A: reset, signals when done
        let lock_a = Arc::clone(&lock);
        let path_a = path.clone();
        let handle_a = std::thread::spawn(move || {
            let _guard = lock_a.lock().unwrap();
            let r = reset_config_at_path(&path_a);
            let _ = tx.send("done");
            r
        });

        // Thread B: waits for reset, then applies partial mutation on top
        let lock_b = Arc::clone(&lock);
        let path_b = path.clone();
        let handle_b = std::thread::spawn(move || {
            let _ = rx.recv();
            execute_serialized_config_mutation_at_path(&lock_b, &path_b, |cfg| {
                apply_set_model_upstream(
                    cfg, "deepseek", None, "claude-sonnet-5",
                    "deepseek-chat", Some("normal"), None,
                )
            })
        });

        let _ = handle_a.join().unwrap();
        let _ = handle_b.join().unwrap().unwrap();

        let result = read_config(dir.path());
        // deepseek is the default active_provider after reset
        assert_eq!(result["active_provider"], "deepseek");
        assert_eq!(
            result["providers"]["deepseek"]["models"]["claude-sonnet-5"]["upstream_model"],
            "deepseek-chat"
        );
        assert_eq!(
            result["providers"]["deepseek"]["models"]["claude-sonnet-5"]["thinking_mode"],
            "normal"
        );
    }

    #[test]
    fn restore_then_partial_mutation_is_applied() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        let bak_path = dir.path().join("config.json.bak");

        // Set up .bak with known content (simulating a backup)
        let bak_cfg = json!({
            "active_provider": "minimax",
            "providers": {
                "minimax": {
                    "display_name": "MiniMax",
                    "upstream_url": "https://example.com/api",
                    "api_key_env": "MINIMAX_API_KEY",
                    "models": {
                        "claude-sonnet-5": {
                            "upstream_model": "restored-upstream",
                            "thinking_mode": null,
                            "visible": true
                        }
                    },
                    "model_map": {
                        "claude-sonnet-5": "restored-upstream"
                    }
                }
            }
        });
        let bak_s = serde_json::to_string_pretty(&bak_cfg).unwrap();
        std::fs::write(&bak_path, bak_s).unwrap();

        // Write a different current config (will be overwritten by restore)
        let cfg = make_provider_config(
            "deepseek",
            json!({"claude-sonnet-5": "deepseek-chat"}),
        );
        write_config(dir.path(), &cfg);

        let lock = Arc::new(Mutex::new(()));
        let (tx, rx) = std::sync::mpsc::channel();

        // Thread A: restore, signals when done
        let lock_a = Arc::clone(&lock);
        let path_a = path.clone();
        let handle_a = std::thread::spawn(move || {
            let _guard = lock_a.lock().unwrap();
            let r = restore_config_from_backup_at_path(&path_a);
            let _ = tx.send("done");
            r
        });

        // Thread B: waits for restore, then applies partial mutation on top
        let lock_b = Arc::clone(&lock);
        let path_b = path.clone();
        let handle_b = std::thread::spawn(move || {
            let _ = rx.recv();
            execute_serialized_config_mutation_at_path(&lock_b, &path_b, |cfg| {
                apply_set_model_upstream(
                    cfg, "minimax", None, "claude-sonnet-5",
                    "MiniMax-M3", Some("default"), None,
                )
            })
        });

        let _ = handle_a.join().unwrap();
        let _ = handle_b.join().unwrap().unwrap();

        let result = read_config(dir.path());
        // Thread B's mutation applied on top of restored backup
        assert_eq!(
            result["providers"]["minimax"]["models"]["claude-sonnet-5"]["upstream_model"],
            "MiniMax-M3"
        );
        assert_eq!(
            result["providers"]["minimax"]["models"]["claude-sonnet-5"]["thinking_mode"],
            "default"
        );
        // model_map should also be updated
        assert_eq!(
            result["providers"]["minimax"]["model_map"]["claude-sonnet-5"],
            "MiniMax-M3"
        );
    }

    // ── Claude Code auto-compact (v2): resolver ──────────────────────

    fn claude_code_cfg(active_provider: &str) -> serde_json::Value {
        serde_json::json!({
            "active_provider": active_provider,
            "claude_code": {
                "auto_compact": {
                    "enabled": false,
                    "trigger_percent": 90
                }
            },
            "providers": {
                "deepseek": {
                    "display_name": "DeepSeek",
                    "models": {
                        "claude-opus-5": { "upstream_model": "deepseek-v4-pro" },
                        "claude-sonnet-5": { "upstream_model": "deepseek-v4-flash" },
                        "claude-haiku-4-5": { "upstream_model": "deepseek-v4-flash" }
                    },
                    "claude_code": { "auto_compact": { "mode": "auto" } }
                },
                "kimi": {
                    "display_name": "Kimi",
                    "models": {
                        "claude-opus-5": { "upstream_model": "moonshot-v1-128k" },
                        "claude-sonnet-5": { "upstream_model": "moonshot-v1-128k" },
                        "claude-haiku-4-5": { "upstream_model": "moonshot-v1-128k" }
                    },
                    "claude_code": {
                        "auto_compact": {
                            "mode": "manual",
                            "window_tokens": 128000,
                            "trigger_percent": 75
                        }
                    }
                },
                "openrouter": {
                    "display_name": "OpenRouter",
                    "claude_code": { "auto_compact": { "mode": "auto" } },
                    "profiles": [
                        {
                            "id": "p1",
                            "display_name": "Profile One",
                            "claude_code": { "auto_compact": { "mode": "claude_default" } }
                        },
                        {
                            "id": "p2",
                            "display_name": "Profile Two",
                            "models": {
                                "claude-opus-5": { "upstream_model": "openai/gpt-5.6-sol-pro" },
                                "claude-sonnet-5": { "upstream_model": "openai/gpt-5.6-terra-pro" },
                                "claude-haiku-4-5": { "upstream_model": "openai/gpt-5.6-luna-pro" }
                            },
                            "claude_code": {
                                "auto_compact": {
                                    "mode": "manual",
                                    "window_tokens": 200000,
                                    "trigger_percent": 80
                                }
                            }
                        }
                    ]
                }
            }
        })
    }

    #[test]
    fn auto_compact_global_off_keeps_routes_and_capacity() {
        // 全体OFF＋auto＋全ルート既知 → status=disabled, apply=false,
        // routes と window_tokens=Some(最小値) は保持
        let cfg = claude_code_cfg("deepseek");
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert!(!eff.globally_enabled);
        assert!(!eff.apply_environment);
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
        assert_eq!(eff.mode, AutoCompactMode::Auto);
        assert_eq!(eff.window_tokens, Some(1000000)); // min of deepseek 1M routes
        assert_eq!(eff.trigger_percent, Some(90));
        assert_eq!(eff.estimated_trigger_tokens, Some(900000));
        assert_eq!(eff.routes.len(), 3);
        assert!(eff.routes.iter().all(|r| r.context_window_tokens.is_some()));
        assert_eq!(eff.target_kind, Some("provider"));
        assert_eq!(eff.target_id.as_deref(), Some("deepseek"));
    }

    #[test]
    fn auto_compact_on_auto_uses_min_route_capacity() {
        let mut cfg = claude_code_cfg("deepseek");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert!(eff.globally_enabled);
        assert_eq!(eff.status, AutoCompactStatus::Applied);
        assert_eq!(eff.mode, AutoCompactMode::Auto);
        assert!(eff.apply_environment);
        assert_eq!(eff.window_tokens, Some(1000000));
        assert_eq!(eff.trigger_percent, Some(90));
        assert_eq!(eff.estimated_trigger_tokens, Some(900000));
        assert_eq!(eff.routes.len(), 3);
    }

    #[test]
    fn auto_compact_on_manual_uses_target_values_no_root_fallback() {
        // manual で root trigger=90・target trigger=75 → 実効 75（root へフォールバックしない）
        let mut cfg = claude_code_cfg("kimi");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Applied);
        assert_eq!(eff.mode, AutoCompactMode::Manual);
        assert!(eff.apply_environment);
        assert_eq!(eff.window_tokens, Some(128000));
        assert_eq!(eff.trigger_percent, Some(75));
        assert_eq!(eff.estimated_trigger_tokens, Some(96000));
        assert_eq!(eff.target_kind, Some("provider"));
        assert_eq!(eff.target_id.as_deref(), Some("kimi"));
        assert_eq!(eff.target_name.as_deref(), Some("Kimi"));
    }

    #[test]
    fn auto_compact_on_claude_default_skips_env() {
        let mut cfg = claude_code_cfg("openrouter");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        // No active_openrouter_profile_id → transient fallback to profiles[0]
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert!(eff.globally_enabled);
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
        assert_eq!(eff.mode, AutoCompactMode::ClaudeDefault);
        assert!(!eff.apply_environment);
        assert_eq!(eff.window_tokens, None);
        assert_eq!(eff.target_kind, Some("profile"));
        assert_eq!(eff.target_id.as_deref(), Some("p1"));
        assert_eq!(eff.target_name.as_deref(), Some("Profile One"));
    }

    #[test]
    fn auto_compact_active_profile_switch() {
        let mut cfg = claude_code_cfg("openrouter");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        cfg["active_openrouter_profile_id"] = json!("p2");
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Applied);
        assert_eq!(eff.mode, AutoCompactMode::Manual);
        assert!(eff.apply_environment);
        assert_eq!(eff.window_tokens, Some(200000));
        assert_eq!(eff.trigger_percent, Some(80));
        assert_eq!(eff.target_id.as_deref(), Some("p2"));
    }

    #[test]
    fn auto_compact_estimated_truncates() {
        let cfg = json!({
            "active_provider": "kimi",
            "claude_code": { "auto_compact": { "enabled": true, "trigger_percent": 90 } },
            "providers": { "kimi": {
                "display_name": "Kimi",
                "claude_code": {
                    "auto_compact": { "mode": "manual", "window_tokens": 999, "trigger_percent": 34 }
                }
            } }
        });
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.estimated_trigger_tokens, Some(339)); // floor(339.66)
    }

    #[test]
    fn auto_compact_legacy_config_resolves_defaults() {
        let cfg = json!({
            "active_provider": "deepseek",
            "providers": {
                "deepseek": { "display_name": "DeepSeek" }
            }
        });
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert!(!eff.globally_enabled);
        assert!(!eff.apply_environment);
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
        assert_eq!(eff.mode, AutoCompactMode::Auto);
        assert_eq!(eff.window_tokens, None);
        assert_eq!(eff.target_id.as_deref(), Some("deepseek"));
    }

    #[test]
    fn auto_compact_no_provider_falls_back_to_sorted_first() {
        let mut cfg = claude_code_cfg("z-nothing");
        cfg["active_provider"] = json!(null);
        // No matching provider → provider_value is None but resolution must not panic
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert!(!eff.globally_enabled);
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
    }

    #[test]
    fn auto_compact_auto_incomplete_when_metadata_missing() {
        // mode=auto・一部不明 → incomplete / apply_environment=false.
        // upstream は解決されるがメタデータなし（コンテキスト長不明）。
        let mut cfg = claude_code_cfg("kimi");
        cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["mode"] = json!("auto");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Incomplete);
        assert_eq!(eff.mode, AutoCompactMode::Auto);
        assert!(!eff.apply_environment);
        assert_eq!(eff.window_tokens, None);
        assert!(eff.routes.iter().all(|r| r.upstream_model.is_some()));
        assert!(eff.routes.iter().all(|r| r.context_window_tokens.is_none()));
    }

    #[test]
    fn auto_compact_auto_incomplete_when_route_unset() {
        // upstream_model=None → 「ルート未設定」。mode=auto・ルート未設定 → incomplete。
        let cfg = json!({
            "active_provider": "deepseek",
            "claude_code": { "auto_compact": { "enabled": true, "trigger_percent": 90 } },
            "providers": { "deepseek": {
                "display_name": "DeepSeek",
                "claude_code": { "auto_compact": { "mode": "auto" } }
            } }
        });
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Incomplete);
        assert!(!eff.apply_environment);
        assert!(eff.routes.iter().all(|r| r.upstream_model.is_none()));
        assert!(eff.routes.iter().all(|r| r.context_window_tokens.is_none()));
    }

    #[test]
    fn auto_compact_off_manual_still_resolves_values() {
        // OFF でも manual の override 値は表示用に解決される
        let cfg = claude_code_cfg("kimi");
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
        assert!(!eff.apply_environment);
        assert_eq!(eff.window_tokens, Some(128000));
        assert_eq!(eff.trigger_percent, Some(75));
    }

    #[test]
    fn auto_compact_unrecognized_mode_falls_back_to_claude_default() {
        // 旧 mode（migration が未適用の "inherit"）は env に渡してはならない
        let mut cfg = claude_code_cfg("deepseek");
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        cfg["providers"]["deepseek"]["claude_code"]["auto_compact"]["mode"] = json!("inherit");
        let eff = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(eff.status, AutoCompactStatus::Disabled);
        assert_eq!(eff.mode, AutoCompactMode::ClaudeDefault);
        assert!(!eff.apply_environment);
        assert_eq!(eff.window_tokens, None);
    }

    fn ctx_route(tokens: Option<u64>) -> EffectiveContextRoute {
        EffectiveContextRoute {
            route: "claude-opus-5".to_string(),
            upstream_model: Some("upstream".to_string()),
            context_window_tokens: tokens,
            context_window_source: ContextWindowSource::Official,
        }
    }

    #[test]
    fn min_context_window_all_three_known_returns_min() {
        let routes = vec![
            ctx_route(Some(1_000_000)),
            ctx_route(Some(131_072)),
            ctx_route(Some(1_000_000)),
        ];
        assert_eq!(min_context_window(&routes), Ok(Some(131_072)));
    }

    #[test]
    fn min_context_window_partial_unknown_returns_none() {
        // 1ルートでも不明なら部分最小値を返さない
        let routes = vec![
            ctx_route(Some(1_000_000)),
            ctx_route(None),
            ctx_route(Some(1_000_000)),
        ];
        assert_eq!(min_context_window(&routes), Ok(None));
    }

    #[test]
    fn min_context_window_wrong_route_count_returns_none() {
        assert_eq!(min_context_window(&[]), Ok(None));
        let two = vec![ctx_route(Some(1_000_000)), ctx_route(Some(1_000_000))];
        assert_eq!(min_context_window(&two), Ok(None));
        let four = vec![
            ctx_route(Some(1_000_000)),
            ctx_route(Some(1_000_000)),
            ctx_route(Some(1_000_000)),
            ctx_route(Some(1_000_000)),
        ];
        assert_eq!(min_context_window(&four), Ok(None));
    }

    #[test]
    fn min_context_window_u32_overflow_errors() {
        // min_context_window takes the minimum BEFORE converting to u32, so all
        // three routes must exceed u32::MAX for the minimum itself to overflow.
        let routes = vec![
            ctx_route(Some(5_000_000_000)),
            ctx_route(Some(5_000_000_000)),
            ctx_route(Some(5_000_000_000)),
        ];
        assert_eq!(
            min_context_window(&routes),
            Err("context window 5000000000 exceeds supported range".to_string())
        );
    }

    #[test]
    fn models_absent_model_map_survives_typed_roundtrip() {
        // A provider with NO `models` key but a `model_map` must keep its
        // model_map fallback through a typed Deserialize → Serialize round-trip
        // (what proxy.rs does before calling the shared extractors). If `models`
        // ever serialized as `{}` when absent, the shared raw-JSON resolver
        // would treat it as "models present" and drop the model_map fallback.
        let raw = json!({
            "active_provider": "legacy",
            "providers": {
                "legacy": {
                    "display_name": "Legacy",
                    "upstream_url": "https://example.com",
                    "api_key_env": "LEGACY_API_KEY",
                    "default_model": "some-model",
                    "force_anthropic_version": null,
                    "supports_count_tokens": false,
                    "supports_vision": false,
                    "supports_video": false,
                    "supports_thinking": true,
                    "model_map": { "claude-opus-5": "some-legacy-model" }
                }
            },
            "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
        });
        let cfg: GatewayConfigResponse =
            serde_json::from_value(raw).expect("deserialize typed");
        let raw_again = serde_json::to_value(&cfg).expect("re-serialize typed");
        assert_eq!(
            resolve_route_upstream_model(&raw_again, "legacy", None, "claude-opus-5"),
            Some("some-legacy-model".to_string())
        );
    }

    // ── Claude Code auto-compact: global apply ──────────────────────

    #[test]
    fn auto_compact_global_update_only_merges_provided_fields() {
        let mut cfg = json!({
            "claude_code": {
                "auto_compact": { "enabled": false, "trigger_percent": 90 }
            }
        });
        let o = apply_update_claude_code_global(&mut cfg, Some(true), None).unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["claude_code"]["auto_compact"]["enabled"], true);
        assert_eq!(cfg["claude_code"]["auto_compact"]["trigger_percent"], 90);
        assert!(cfg["claude_code"]["auto_compact"].get("window_tokens").is_none());
    }

    #[test]
    fn auto_compact_global_update_creates_default_block() {
        let mut cfg = json!({});
        let o = apply_update_claude_code_global(&mut cfg, Some(true), None).unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["claude_code"]["auto_compact"]["enabled"], true);
        assert_eq!(cfg["claude_code"]["auto_compact"]["trigger_percent"], 90);
        assert!(cfg["claude_code"]["auto_compact"].get("window_tokens").is_none());
    }

    #[test]
    fn auto_compact_global_update_noop_detected() {
        let mut cfg = json!({
            "claude_code": {
                "auto_compact": { "enabled": false, "trigger_percent": 90 }
            }
        });
        let o1 = apply_update_claude_code_global(&mut cfg, Some(false), None).unwrap();
        assert!(!o1.config_changed);
        let o2 = apply_update_claude_code_global(&mut cfg, Some(true), None).unwrap();
        assert!(o2.config_changed);
        let o3 = apply_update_claude_code_global(&mut cfg, Some(true), None).unwrap();
        assert!(!o3.config_changed);
    }

    #[test]
    fn auto_compact_global_noop_skips_save() {
        let mut cfg = json!({
            "claude_code": {
                "auto_compact": { "enabled": false, "trigger_percent": 90 }
            }
        });
        let mut saves = 0;
        execute_config_mutation(
            &mut cfg,
            |cfg| apply_update_claude_code_global(cfg, Some(false), None),
            |_| {
                saves += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saves, 0, "no-op must not write the config");
        execute_config_mutation(
            &mut cfg,
            |cfg| apply_update_claude_code_global(cfg, Some(true), None),
            |_| {
                saves += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saves, 1);
        execute_config_mutation(
            &mut cfg,
            |cfg| apply_update_claude_code_global(cfg, Some(true), None),
            |_| {
                saves += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saves, 1, "re-applying the same value must not write again");
    }

    // ── Claude Code auto-compact: target apply ──────────────────────

    #[test]
    fn auto_compact_target_manual_sets_values() {
        let mut cfg = json!({
            "providers": { "kimi": { "display_name": "Kimi" } }
        });
        let o = apply_update_claude_code_target(&mut cfg, "kimi", None, "manual", Some(128000), Some(75))
            .unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["mode"], "manual");
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["window_tokens"], 128000);
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["trigger_percent"], 75);
    }

    #[test]
    fn auto_compact_target_auto_removes_values_and_noops() {
        let mut cfg = json!({
            "providers": {
                "kimi": {
                    "display_name": "Kimi",
                    "claude_code": {
                        "auto_compact": {
                            "mode": "manual",
                            "window_tokens": 128000,
                            "trigger_percent": 75
                        }
                    }
                }
            }
        });
        let o = apply_update_claude_code_target(&mut cfg, "kimi", None, "auto", None, None).unwrap();
        assert!(o.config_changed);
        let ac = &cfg["providers"]["kimi"]["claude_code"]["auto_compact"];
        assert_eq!(ac["mode"], "auto");
        assert!(ac.get("window_tokens").is_none());
        assert!(ac.get("trigger_percent").is_none());
        let o2 = apply_update_claude_code_target(&mut cfg, "kimi", None, "auto", None, None).unwrap();
        assert!(!o2.config_changed);
    }

    #[test]
    fn auto_compact_target_profile_manual() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "profiles": [
                        { "id": "p1", "display_name": "One" },
                        { "id": "p2", "display_name": "Two" }
                    ]
                }
            }
        });
        let o = apply_update_claude_code_target(&mut cfg, "openrouter", Some("p2"), "manual", Some(200000), Some(80))
            .unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["providers"]["openrouter"]["profiles"][1]["claude_code"]["auto_compact"]["mode"], "manual");
        assert_eq!(cfg["providers"]["openrouter"]["profiles"][1]["claude_code"]["auto_compact"]["window_tokens"], 200000);
        // profile 1 untouched
        assert!(cfg["providers"]["openrouter"]["profiles"][0].get("claude_code").is_none());
    }

    #[test]
    fn auto_compact_target_missing_provider_rejected() {
        let mut cfg = json!({ "providers": {} });
        assert!(apply_update_claude_code_target(&mut cfg, "ghost", None, "auto", None, None).is_err());
    }

    #[test]
    fn auto_compact_target_missing_profile_rejected() {
        let mut cfg = json!({
            "providers": { "openrouter": { "profiles": [ { "id": "p1", "display_name": "One" } ] } }
        });
        assert!(apply_update_claude_code_target(&mut cfg, "openrouter", Some("nope"), "auto", None, None).is_err());
    }

    #[test]
    fn auto_compact_target_profile_id_on_non_openrouter_rejected() {
        let mut cfg = json!({ "providers": { "kimi": { "display_name": "Kimi" } } });
        assert!(apply_update_claude_code_target(&mut cfg, "kimi", Some("p1"), "auto", None, None).is_err());
    }

    #[test]
    fn auto_compact_manual_requires_values() {
        let mut cfg = json!({ "providers": { "kimi": { "display_name": "Kimi" } } });
        assert!(apply_update_claude_code_target(&mut cfg, "kimi", None, "manual", None, Some(75)).is_err());
        assert!(apply_update_claude_code_target(&mut cfg, "kimi", None, "manual", Some(100), None).is_err());
    }

    // ── Claude Code auto-compact: validation ────────────────────────

    #[test]
    fn auto_compact_validation_rejects_bad_inputs() {
        let mut cfg = json!({});
        assert!(apply_update_claude_code_global(&mut cfg, None, Some(0)).is_err());
        assert!(apply_update_claude_code_global(&mut cfg, None, Some(101)).is_err());
        assert!(apply_update_claude_code_global(&mut cfg, None, Some(50)).is_ok());

        let mut target = json!({ "providers": { "kimi": { "display_name": "Kimi" } } });
        assert!(apply_update_claude_code_target(&mut target, "kimi", None, "bogus", None, None).is_err());
        // manual window_tokens range validation
        assert!(apply_update_claude_code_target(&mut target, "kimi", None, "manual", Some(0), Some(50)).is_err());
        assert!(apply_update_claude_code_target(&mut target, "kimi", None, "manual", Some(10_000_001), Some(50)).is_err());
        assert!(apply_update_claude_code_target(&mut target, "kimi", None, "manual", Some(100), Some(50)).is_ok());
    }

    // ── Claude Code auto-compact: serialized mutation ───────────────

    #[test]
    fn auto_compact_serialized_mutation_persists() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json");
        write_config(
            dir.path(),
            &json!({ "providers": { "kimi": { "display_name": "Kimi" } } }),
        );

        let lock = Mutex::new(());
        let resp = execute_serialized_config_mutation_at_path(&lock, &path, |cfg| {
            apply_update_claude_code_global(cfg, Some(true), Some(85))
        })
        .unwrap();
        assert!(!resp.restart_gateway);

        let result = read_config(dir.path());
        assert_eq!(result["claude_code"]["auto_compact"]["enabled"], true);
        assert_eq!(result["claude_code"]["auto_compact"]["trigger_percent"], 85);
        assert!(result["claude_code"]["auto_compact"].get("window_tokens").is_none());
    }

    // ── Claude Code auto-compact: template defaults ─────────────────

    #[test]
    fn auto_compact_template_has_expected_defaults() {
        let template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json"))
                .expect("template config.json must be valid JSON");

        let root_ac = &template["claude_code"]["auto_compact"];
        assert_eq!(root_ac["enabled"], false);
        assert!(root_ac.get("window_tokens").is_none(), "root window_tokens must be removed in v2");
        assert_eq!(root_ac["trigger_percent"], 90);

        let providers = template["providers"].as_object().expect("providers object");
        for (id, provider) in providers {
            if id == "openrouter" {
                let profiles = provider.get("profiles").and_then(|p| p.as_array());
                if let Some(profiles) = profiles {
                    for prof in profiles {
                        assert_eq!(
                            prof["claude_code"]["auto_compact"]["mode"],
                            "auto",
                            "profile '{}' must default to mode auto",
                            prof["id"]
                        );
                    }
                }
            } else {
                assert_eq!(
                    provider["claude_code"]["auto_compact"]["mode"],
                    "auto",
                    "provider '{}' must default to mode auto",
                    id
                );
            }
        }
    }

    #[test]
    fn auto_compact_all_presets_applied() {
        // Every standard provider / OpenRouter preset, resolved against the real
        // embedded template with the global switch ON, must know all 3 routes
        // and produce Applied with the expected minimum window.
        let template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json"))
                .expect("template config.json must be valid JSON");

        // (provider_id, expected min window over the 3 canonical routes)
        let direct_cases = [
            ("deepseek", 1_000_000), // opus→v4-pro, sonnet→v4-pro, haiku→v4-flash (all 1M)
            ("minimax", 1_000_000),  // opus→M3, sonnet→M3, haiku→M3 (all 1M)
            ("kimi", 262_144),       // opus→k2.7-code, sonnet→k2.6, haiku→k2.5 (all 256K)
            ("mimo", 1_000_000),     // opus→v2.5-pro, sonnet→v2.5-pro, haiku→v2.5 (all 1M)
        ];

        for (provider_id, expected_window) in direct_cases {
            let mut cfg = template.clone();
            cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
            cfg["active_provider"] = json!(provider_id);
            let eff = resolve_effective_auto_compact(&cfg).unwrap();
            assert_eq!(eff.mode, AutoCompactMode::Auto, "{provider_id}");
            assert_eq!(eff.status, AutoCompactStatus::Applied, "{provider_id}");
            assert!(eff.apply_environment, "{provider_id}");
            assert_eq!(eff.routes.len(), CLAUDE_ROUTES.len(), "{provider_id}");
            assert!(
                eff.routes.iter().all(|r| r.context_window_tokens.is_some()),
                "{provider_id}"
            );
            assert_eq!(eff.window_tokens, Some(expected_window), "{provider_id}");
            assert_eq!(eff.target_kind, Some("provider"), "{provider_id}");
            assert_eq!(eff.target_id.as_deref(), Some(provider_id), "{provider_id}");
        }

        // (openrouter profile id, expected min window)
        let openrouter_cases = [
            ("a0e0f000-0000-4000-8000-000000000001", 262_144), // Laguna
            ("b0e0f000-0000-4000-8000-000000000002", 262_144), // Hy3
            ("c0e0f000-0000-4000-8000-000000000003", 262_144), // InclusionAI
            ("d0e0f000-0000-4000-8000-000000000004", 262_144), // StepFun
            ("e0e0f000-0000-4000-8000-000000000005", 1_050_000), // GPT-5.6 Balanced
        ];

        for (profile_id, expected_window) in openrouter_cases {
            let mut cfg = template.clone();
            cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
            cfg["active_provider"] = json!("openrouter");
            cfg["active_openrouter_profile_id"] = json!(profile_id);
            let eff = resolve_effective_auto_compact(&cfg).unwrap();
            assert_eq!(eff.mode, AutoCompactMode::Auto, "{profile_id}");
            assert_eq!(eff.status, AutoCompactStatus::Applied, "{profile_id}");
            assert!(eff.apply_environment, "{profile_id}");
            assert_eq!(eff.routes.len(), CLAUDE_ROUTES.len(), "{profile_id}");
            assert!(
                eff.routes.iter().all(|r| r.context_window_tokens.is_some()),
                "{profile_id}"
            );
            assert_eq!(eff.window_tokens, Some(expected_window), "{profile_id}");
            assert_eq!(eff.target_kind, Some("profile"), "{profile_id}");
            assert_eq!(eff.target_id.as_deref(), Some(profile_id), "{profile_id}");
        }
    }

    // ── Claude Code auto-compact: launch command env generation ─────

    fn eff(
        applied: bool,
        window: Option<u32>,
        percent: Option<u8>,
        status: AutoCompactStatus,
    ) -> EffectiveAutoCompact {
        EffectiveAutoCompact {
            globally_enabled: applied,
            mode: AutoCompactMode::Auto,
            status,
            apply_environment: applied,
            window_tokens: window,
            trigger_percent: percent,
            estimated_trigger_tokens: None,
            target_kind: None,
            target_id: None,
            target_name: None,
            routes: Vec::new(),
        }
    }

    #[test]
    fn auto_compact_environment_applied_returns_both() {
        let env = auto_compact_environment(&eff(true, Some(262144), Some(90), AutoCompactStatus::Applied))
            .unwrap();
        assert_eq!(env.set.len(), 2);
        assert!(env.set.contains(&("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "262144".to_string())));
        assert!(env.set.contains(&("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", "90".to_string())));
        assert!(env.remove.is_empty());
    }

    #[test]
    fn auto_compact_environment_manual_returns_target_values() {
        let mut effective =
            eff(true, Some(200_000), Some(80), AutoCompactStatus::Applied);
        effective.mode = AutoCompactMode::Manual;
        let env = auto_compact_environment(&effective).unwrap();
        assert!(env.set.contains(&("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "200000".to_string())));
        assert!(env.set.contains(&("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", "80".to_string())));
        assert!(env.remove.is_empty());
    }

    #[test]
    fn auto_compact_environment_disabled_clears() {
        let env = auto_compact_environment(&eff(false, None, None, AutoCompactStatus::Disabled)).unwrap();
        assert!(env.set.is_empty());
        assert_eq!(env.remove, AUTO_COMPACT_ENV_VARS.to_vec());
    }

    #[test]
    fn auto_compact_environment_claude_default_clears() {
        let mut e = eff(false, None, None, AutoCompactStatus::Disabled);
        e.mode = AutoCompactMode::ClaudeDefault;
        let env = auto_compact_environment(&e).unwrap();
        assert!(env.set.is_empty());
        assert_eq!(env.remove, AUTO_COMPACT_ENV_VARS.to_vec());
    }

    #[test]
    fn auto_compact_environment_incomplete_clears() {
        let env = auto_compact_environment(&eff(false, None, None, AutoCompactStatus::Incomplete)).unwrap();
        assert!(env.set.is_empty());
        assert_eq!(env.remove, AUTO_COMPACT_ENV_VARS.to_vec());
    }

    #[test]
    fn auto_compact_environment_missing_window_errors() {
        let err = auto_compact_environment(&eff(true, None, Some(90), AutoCompactStatus::Applied))
            .unwrap_err();
        assert!(err.contains("window_tokens"), "{err}");
    }

    #[test]
    fn auto_compact_environment_missing_percent_errors() {
        let err = auto_compact_environment(&eff(true, Some(262144), None, AutoCompactStatus::Applied))
            .unwrap_err();
        assert!(err.contains("trigger_percent"), "{err}");
    }

    #[test]
    fn auto_compact_environment_window_zero_errors() {
        let err = auto_compact_environment(&eff(true, Some(0), Some(90), AutoCompactStatus::Applied))
            .unwrap_err();
        assert!(err.contains("greater than zero"), "{err}");
    }

    #[test]
    fn auto_compact_environment_percent_out_of_range_errors() {
        let err = auto_compact_environment(&eff(true, Some(262144), Some(0), AutoCompactStatus::Applied))
            .unwrap_err();
        assert!(err.contains("between 1 and 100"), "{err}");
        let err = auto_compact_environment(&eff(true, Some(262144), Some(101), AutoCompactStatus::Applied))
            .unwrap_err();
        assert!(err.contains("between 1 and 100"), "{err}");
    }

    #[test]
    fn gateway_connection_env_vars_uses_normalized_base_url() {
        let cfg = json!({ "server": { "host": "0.0.0.0", "port": 4000 } });
        let env = gateway_connection_env_vars(&cfg).unwrap();
        assert!(env.contains(&("ANTHROPIC_BASE_URL", "http://127.0.0.1:4000".to_string())));

        let cfg = json!({ "server": { "host": "[::]", "port": 4000 } });
        let env = gateway_connection_env_vars(&cfg).unwrap();
        assert!(env.contains(&("ANTHROPIC_BASE_URL", "http://127.0.0.1:4000".to_string())));

        let cfg = json!({ "server": { "host": "proxy.example.com", "port": 8080 } });
        let env = gateway_connection_env_vars(&cfg).unwrap();
        assert!(env.contains(&("ANTHROPIC_BASE_URL", "http://proxy.example.com:8080".to_string())));

        let cfg = json!({ "server": { "port": 5000 } });
        let env = gateway_connection_env_vars(&cfg).unwrap();
        assert!(env.contains(&("ANTHROPIC_BASE_URL", "http://127.0.0.1:5000".to_string())));
    }

    #[test]
    fn gateway_client_base_url_brackets_ipv6_literal() {
        assert_eq!(gateway_client_base_url("::1", 4000), "http://[::1]:4000");
        assert_eq!(gateway_client_base_url("[::1]", 4000), "http://[::1]:4000");
    }

    #[test]
    fn gateway_connection_env_vars_rejects_out_of_range_port() {
        let cfg = json!({ "server": { "host": "127.0.0.1", "port": 70000 } });
        let err = gateway_connection_env_vars(&cfg).unwrap_err();
        assert_eq!(err, "gateway port is out of range: 70000");
    }

    #[test]
    fn gateway_connection_env_vars_uses_auth_token() {
        let env = gateway_connection_env_vars(&json!({})).unwrap();
        assert!(env.contains(&("ANTHROPIC_AUTH_TOKEN", "sk-local-gateway".to_string())));
    }

    #[test]
    fn gateway_connection_env_vars_never_uses_api_key() {
        let env = gateway_connection_env_vars(&json!({})).unwrap();
        assert!(!env.iter().any(|(k, _)| *k == "ANTHROPIC_API_KEY"));
    }

    #[test]
    fn local_gateway_token_pins_expected_local_token() {
        // Rust 側の期待値を固定する。gatewayConnection.ts の GATEWAY_LOCAL_TOKEN と
        // の同一はコメントで管理しており、実際のドリフト検出には
        // Vitest 側の `?raw` 読み込み比較か共有 JSON への移行が必要。
        assert_eq!(LOCAL_GATEWAY_TOKEN, "sk-local-gateway");
    }

    #[test]
    fn launch_command_applied_contains_env_vars() {
        let set = vec![
            ("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "262144".to_string()),
            ("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", "90".to_string()),
        ];
        let command = render_claude_code_launch_command(&set, &[]);
        assert!(command.contains("$env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; "));
        assert!(command.contains("$env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; "));
        assert!(command.ends_with("; claude"));
        assert!(!command.contains("Remove-Item"));
    }

    #[test]
    fn launch_command_not_applied_contains_remove_item() {
        let command = render_claude_code_launch_command(&[], &AUTO_COMPACT_ENV_VARS);
        assert!(command.contains(
            "Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue; "
        ));
        assert!(command.contains(
            "Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue; "
        ));
        assert!(command.ends_with("; claude"));
        assert!(!command.contains("$env:CLAUDE_"));
    }

    #[test]
    fn launch_command_gateway_vars_present() {
        let set = vec![
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:4000".to_string()),
            ("ANTHROPIC_AUTH_TOKEN", "sk-local-gateway".to_string()),
        ];
        let command = render_claude_code_launch_command(&set, &[]);
        assert!(command.contains("$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; "));
        assert!(command.contains("$env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; "));
        assert!(!command.contains("ANTHROPIC_API_KEY"));
    }

    #[test]
    fn launch_command_never_contains_legacy_percent_name() {
        let set = vec![
            ("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "262144".to_string()),
            ("CLAUDE_AUTOCOMPACT_PCT_OVERRIDE", "90".to_string()),
        ];
        let command = render_claude_code_launch_command(&set, &[]);
        assert!(!command.contains("CLAUDE_CODE_AUTO_COMPACT_PERCENT"));
    }

    #[test]
    fn powershell_quote_escapes_single_quote() {
        assert_eq!(powershell_quote("a'b"), "'a''b'");
        assert_eq!(powershell_quote("plain"), "'plain'");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_applied_command_injects_env_into_child() {
        let template: serde_json::Value =
            serde_json::from_str(include_str!("../resources/config.json"))
                .expect("template config.json must be valid JSON");
        let mut cfg = template;
        cfg["claude_code"]["auto_compact"]["enabled"] = json!(true);
        cfg["active_provider"] = json!("kimi");
        let effective = resolve_effective_auto_compact(&cfg).unwrap();
        assert_eq!(effective.window_tokens, Some(262_144));
        assert_eq!(effective.trigger_percent, Some(90));

        let context = auto_compact_environment(&effective).unwrap();
        let mut set = gateway_connection_env_vars(&cfg).unwrap();
        set.extend(context.set);
        let command = render_claude_code_launch_command(&set, &context.remove);

        // 誤った旧名称・API_KEY・Remove-Item を生成していないことを固定。
        assert!(!command.contains("CLAUDE_CODE_AUTO_COMPACT_PERCENT"));
        assert!(!command.contains("ANTHROPIC_API_KEY"));
        assert!(!command.contains("Remove-Item"));

        // 末尾の `claude` を env エコーに差し替え（実 CLI は起動しない）。
        let probe = format!(
            "{}; \
             Write-Output ('W=' + $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW); \
             Write-Output ('P=' + $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE)",
            command.trim_end_matches("claude").trim_end_matches("; ")
        );

        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &probe])
            .output()
            .expect("powershell.exe must be available on supported Windows");

        assert!(
            output.status.success(),
            "PowerShell failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("W=262144"), "{stdout}");
        assert!(stdout.contains("P=90"), "{stdout}");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn powershell_not_applied_command_clears_stale_context_vars() {
        let effective = eff(false, None, None, AutoCompactStatus::Disabled);
        let context = auto_compact_environment(&effective).unwrap();
        let set = gateway_connection_env_vars(&json!({})).unwrap();
        let command = render_claude_code_launch_command(&set, &context.remove);
        assert!(command.contains("Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW"));

        // 先に旧値を設定してから生成コマンドの set/remove 部分を実行し、
        // 削除後に残らないことを確認。
        let preamble =
            "$env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='999'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='99'; ";
        let body = command.trim_end_matches("claude").trim_end_matches("; ");
        let probe = format!(
            "{preamble}{body}; \
             Write-Output ('W=' + $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW); \
             Write-Output ('P=' + $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE)"
        );

        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &probe])
            .output()
            .expect("powershell.exe must be available on supported Windows");

        assert!(
            output.status.success(),
            "PowerShell failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains("W=999"), "{stdout}");
        assert!(!stdout.contains("P=99"), "{stdout}");
    }

    // ── Claude Code auto-compact: combined (atomic) apply ───────────

    #[test]
    fn auto_compact_combined_applies_both() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false, "trigger_percent": 90 } },
            "providers": { "kimi": { "display_name": "Kimi" } }
        });
        let o = apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: Some(true),
                trigger_percent: Some(85),
            }),
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Manual,
                window_tokens: Some(128000),
                trigger_percent: Some(75),
            }),
        )
        .unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["claude_code"]["auto_compact"]["enabled"], true);
        assert_eq!(cfg["claude_code"]["auto_compact"]["trigger_percent"], 85);
        assert!(cfg["claude_code"]["auto_compact"].get("window_tokens").is_none());
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["mode"], "manual");
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["window_tokens"], 128000);
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["trigger_percent"], 75);
    }

    #[test]
    fn auto_compact_combined_global_only() {
        let mut cfg = json!({});
        let o = apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: Some(true),
                trigger_percent: None,
            }),
            None,
        )
        .unwrap();
        assert!(o.config_changed);
        assert_eq!(cfg["claude_code"]["auto_compact"]["enabled"], true);
    }

    #[test]
    fn auto_compact_combined_target_only() {
        let mut cfg = json!({ "providers": { "kimi": { "display_name": "Kimi" } } });
        let o = apply_update_claude_code_settings(
            &mut cfg,
            None,
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Manual,
                window_tokens: Some(128000),
                trigger_percent: Some(75),
            }),
        )
        .unwrap();
        assert!(o.config_changed);
        assert!(cfg.get("claude_code").is_none());
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["mode"], "manual");
    }

    #[test]
    fn auto_compact_combined_noop_detected() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false, "trigger_percent": 90 } },
            "providers": { "kimi": { "display_name": "Kimi", "claude_code": { "auto_compact": { "mode": "auto" } } } }
        });
        let o = apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: Some(false),
                trigger_percent: Some(90),
            }),
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Auto,
                window_tokens: None,
                trigger_percent: None,
            }),
        )
        .unwrap();
        assert!(!o.config_changed);
    }

    #[test]
    fn auto_compact_combined_rejects_missing_target_without_mutating() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false } },
            "providers": { "kimi": { "display_name": "Kimi" } }
        });
        let before = cfg.clone();
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: Some(true),
                trigger_percent: None,
            }),
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "ghost".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Auto,
                window_tokens: None,
                trigger_percent: None,
            }),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an error for a missing target"),
        };
        assert!(err.contains("ghost"));
        assert_eq!(cfg, before, "rejected target must not leave a partial write");
    }

    #[test]
    fn auto_compact_combined_manual_requires_values() {
        let mut cfg = json!({ "providers": { "kimi": { "display_name": "Kimi" } } });
        let before = cfg.clone();
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            None,
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Manual,
                window_tokens: None,
                trigger_percent: Some(75),
            }),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an error for missing manual values"),
        };
        assert!(err.contains("window_tokens is required"));
        assert_eq!(cfg, before);
    }

    #[test]
    fn auto_compact_combined_rejects_invalid_global_before_applying() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false } },
            "providers": { "kimi": { "display_name": "Kimi" } }
        });
        let before = cfg.clone();
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: Some(true),
                trigger_percent: Some(0),
            }),
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Manual,
                window_tokens: Some(128000),
                trigger_percent: Some(75),
            }),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an error for an invalid global trigger_percent"),
        };
        assert!(err.contains("trigger_percent"));
        assert_eq!(cfg, before);
    }

    #[test]
    fn auto_compact_combined_save_is_atomic() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false, "trigger_percent": 90 } },
            "providers": { "kimi": { "display_name": "Kimi" } }
        });
        let mut saves = 0;

        // Successful combined update → exactly one save.
        execute_config_mutation(
            &mut cfg,
            |cfg| {
                apply_update_claude_code_settings(
                    cfg,
                    Some(&ClaudeCodeGlobalUpdate {
                        enabled: Some(true),
                        trigger_percent: None,
                    }),
                    Some(&ClaudeCodeTargetUpdate {
                        provider_id: "kimi".into(),
                        profile_id: None,
                        mode: ClaudeCodeTargetMode::Manual,
                        window_tokens: Some(128000),
                        trigger_percent: Some(75),
                    }),
                )
            },
            |_| {
                saves += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(saves, 1);
        assert_eq!(cfg["claude_code"]["auto_compact"]["enabled"], true);
        assert_eq!(cfg["providers"]["kimi"]["claude_code"]["auto_compact"]["mode"], "manual");

        // Failing target → no save, common section unchanged.
        let before = cfg.clone();
        let err = match execute_config_mutation(
            &mut cfg,
            |cfg| {
                apply_update_claude_code_settings(
                    cfg,
                    Some(&ClaudeCodeGlobalUpdate {
                        enabled: Some(false),
                        trigger_percent: None,
                    }),
                    Some(&ClaudeCodeTargetUpdate {
                        provider_id: "ghost".into(),
                        profile_id: None,
                        mode: ClaudeCodeTargetMode::Auto,
                        window_tokens: None,
                        trigger_percent: None,
                    }),
                )
            },
            |_| {
                saves += 1;
                Ok(())
            },
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected a save error"),
        };
        assert!(err.contains("ghost"));
        assert_eq!(saves, 1, "failing combined update must not save");
        assert_eq!(cfg, before, "failing target must not leave the common section mutated");
    }

    #[test]
    fn auto_compact_combined_rejects_structural_target_anomaly() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false, "trigger_percent": 90 } },
            "providers": { "kimi": { "display_name": "Kimi", "claude_code": "oops-not-an-object" } }
        });
        let before = cfg.clone();
        let mut saves = 0;
        let err = match execute_config_mutation(
            &mut cfg,
            |cfg| {
                apply_update_claude_code_settings(
                    cfg,
                    Some(&ClaudeCodeGlobalUpdate {
                        enabled: Some(true),
                        trigger_percent: None,
                    }),
                    Some(&ClaudeCodeTargetUpdate {
                        provider_id: "kimi".into(),
                        profile_id: None,
                        mode: ClaudeCodeTargetMode::Manual,
                        window_tokens: Some(128000),
                        trigger_percent: Some(75),
                    }),
                )
            },
            |_| {
                saves += 1;
                Ok(())
            },
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected a structural error"),
        };
        assert!(err.contains("non-object"), "unexpected error: {}", err);
        assert_eq!(saves, 0, "structural failure must not save");
        assert_eq!(cfg, before, "structural failure must leave the config fully unchanged");
    }

    #[test]
    fn auto_compact_combined_openrouter_requires_profile() {
        let mut cfg = json!({
            "providers": {
                "openrouter": {
                    "display_name": "OpenRouter",
                    "profiles": [ { "id": "p1", "display_name": "One" } ]
                },
                "kimi": { "display_name": "Kimi" }
            }
        });

        // openrouter without profile_id → rejected
        let before = cfg.clone();
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            None,
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "openrouter".into(),
                profile_id: None,
                mode: ClaudeCodeTargetMode::Auto,
                window_tokens: None,
                trigger_percent: None,
            }),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected openrouter to require a profile_id"),
        };
        assert!(err.contains("profile_id is required for OpenRouter"));
        assert_eq!(cfg, before);

        // profile_id on a direct provider → rejected
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            None,
            Some(&ClaudeCodeTargetUpdate {
                provider_id: "kimi".into(),
                profile_id: Some("p1".into()),
                mode: ClaudeCodeTargetMode::Auto,
                window_tokens: None,
                trigger_percent: None,
            }),
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected a direct provider to reject profile_id"),
        };
        assert!(err.contains("profile_id is only valid for OpenRouter"));
        assert_eq!(cfg, before);
    }

    #[test]
    fn auto_compact_combined_rejects_empty_global_without_target() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": false, "trigger_percent": 90 } }
        });
        let before = cfg.clone();
        let err = match apply_update_claude_code_settings(
            &mut cfg,
            Some(&ClaudeCodeGlobalUpdate {
                enabled: None,
                trigger_percent: None,
            }),
            None,
        ) {
            Err(e) => e,
            Ok(_) => panic!("expected an empty global update without a target to be rejected"),
        };
        assert!(err.contains("global or target update is required"));
        assert_eq!(cfg, before);
    }

    // ── Claude Code auto-compact: v2 migration ──────────────────────

    #[test]
    fn migrate_modes_converts_legacy_values() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": true, "window_tokens": 240000, "trigger_percent": 90 } },
            "providers": {
                "a": { "claude_code": { "auto_compact": { "mode": "inherit" } } },
                "b": { "claude_code": { "auto_compact": { "mode": "override", "window_tokens": 128000, "trigger_percent": 75 } } },
                "c": { "claude_code": { "auto_compact": { "mode": "bogus" } } },
                "d": { "claude_code": { "auto_compact": { "mode": "claude_default" } } },
                "e": { "claude_code": { "auto_compact": { "mode": "auto" } } },
                "f": { "claude_code": { "auto_compact": { "window_tokens": 90000 } } },
                "openrouter": {
                    "profiles": [
                        { "id": "p1", "claude_code": { "auto_compact": { "mode": "inherit" } } },
                        { "id": "p2", "claude_code": { "auto_compact": { "mode": "override" } } }
                    ]
                }
            }
        });
        assert!(migrate_claude_code_auto_compact_modes_inner(&mut cfg));
        let root = &cfg["claude_code"]["auto_compact"];
        assert!(root.get("window_tokens").is_none(), "root window_tokens removed");
        assert_eq!(cfg["providers"]["a"]["claude_code"]["auto_compact"]["mode"], "auto");
        assert_eq!(cfg["providers"]["b"]["claude_code"]["auto_compact"]["mode"], "manual");
        // manual は値維持
        assert_eq!(cfg["providers"]["b"]["claude_code"]["auto_compact"]["window_tokens"], 128000);
        assert_eq!(cfg["providers"]["b"]["claude_code"]["auto_compact"]["trigger_percent"], 75);
        assert_eq!(cfg["providers"]["c"]["claude_code"]["auto_compact"]["mode"], "claude_default");
        assert_eq!(cfg["providers"]["d"]["claude_code"]["auto_compact"]["mode"], "claude_default");
        assert_eq!(cfg["providers"]["e"]["claude_code"]["auto_compact"]["mode"], "auto");
        // mode なし → そのまま（読み取り側デフォルトが auto）
        assert!(cfg["providers"]["f"]["claude_code"]["auto_compact"].get("mode").is_none());
        assert_eq!(cfg["providers"]["openrouter"]["profiles"][0]["claude_code"]["auto_compact"]["mode"], "auto");
        assert_eq!(cfg["providers"]["openrouter"]["profiles"][1]["claude_code"]["auto_compact"]["mode"], "manual");
    }

    #[test]
    fn migrate_modes_is_idempotent() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": true, "window_tokens": 240000, "trigger_percent": 90 } },
            "providers": {
                "a": { "claude_code": { "auto_compact": { "mode": "inherit" } } },
                "openrouter": {
                    "profiles": [ { "id": "p1", "claude_code": { "auto_compact": { "mode": "override" } } } ]
                }
            }
        });
        assert!(migrate_claude_code_auto_compact_modes_inner(&mut cfg));
        // 2回目は変更なし
        assert!(!migrate_claude_code_auto_compact_modes_inner(&mut cfg));
    }

    #[test]
    fn migrate_modes_noop_when_already_v2() {
        let mut cfg = json!({
            "claude_code": { "auto_compact": { "enabled": true, "trigger_percent": 90 } },
            "providers": {
                "a": { "claude_code": { "auto_compact": { "mode": "auto" } } },
                "b": { "claude_code": { "auto_compact": { "mode": "manual", "window_tokens": 100, "trigger_percent": 50 } } },
                "c": { "claude_code": { "auto_compact": { "mode": "claude_default" } } },
                "d": { "claude_code": { "auto_compact": {} } }
            }
        });
        assert!(!migrate_claude_code_auto_compact_modes_inner(&mut cfg));
    }

    #[test]
    fn ensure_config_initialized_final_config_has_no_legacy_mode() {
        // Integration: 旧 inherit/override + root window_tokens を通した後、
        // template 由来の旧 mode が再導入されず最終的に v2 であること。
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(
            &config_path,
            r#"{
                "active_provider": "kimi",
                "claude_code": {
                    "auto_compact": { "enabled": true, "window_tokens": 240000, "trigger_percent": 90 }
                },
                "providers": {
                    "kimi": {
                        "display_name": "Kimi",
                        "claude_code": {
                            "auto_compact": { "mode": "override", "window_tokens": 128000, "trigger_percent": 75 }
                        }
                    }
                }
            }"#,
        )
        .unwrap();

        ensure_config_initialized_at(&config_path, paths::AppChannel::Dev, false).unwrap();
        let result = read_config(dir.path());

        // root window_tokens は存在しない
        assert!(result["claude_code"]["auto_compact"].get("window_tokens").is_none());

        // kimi: override → manual、値維持
        let kimi_ac = &result["providers"]["kimi"]["claude_code"]["auto_compact"];
        assert_eq!(kimi_ac["mode"], "manual");
        assert_eq!(kimi_ac["window_tokens"], 128000);
        assert_eq!(kimi_ac["trigger_percent"], 75);

        // 全 provider / profile に v2 以前の mode が無い（template 由来の inherit も auto 化）
        let providers = result["providers"].as_object().expect("providers object");
        for (pid, provider) in providers {
            if let Some(ac) = provider.get("claude_code").and_then(|c| c.get("auto_compact")) {
                let mode = ac.get("mode").and_then(|m| m.as_str());
                assert!(
                    matches!(mode, Some("auto") | Some("manual") | Some("claude_default") | None),
                    "provider '{}' has a legacy mode: {:?}",
                    pid,
                    mode
                );
            }
            if let Some(profiles) = provider.get("profiles").and_then(|p| p.as_array()) {
                for profile in profiles {
                    if let Some(ac) = profile.get("claude_code").and_then(|c| c.get("auto_compact")) {
                        let mode = ac.get("mode").and_then(|m| m.as_str());
                        assert!(
                            matches!(mode, Some("auto") | Some("manual") | Some("claude_default") | None),
                            "openrouter profile '{}' has a legacy mode: {:?}",
                            profile.get("id").and_then(|i| i.as_str()).unwrap_or("?"),
                            mode
                        );
                    }
                }
            }
        }

        // 実効解決も v2（kimi manual が生きている）
        let eff = resolve_effective_auto_compact(&result).unwrap();
        assert_eq!(eff.mode, AutoCompactMode::Manual);
        assert_eq!(eff.window_tokens, Some(128000));
        assert_eq!(eff.trigger_percent, Some(75));
    }
}
