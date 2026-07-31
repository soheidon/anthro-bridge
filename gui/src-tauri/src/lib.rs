use chrono::Local;
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::Manager;
use tokio::sync::oneshot;

mod config_template;
mod model_capabilities;
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
        // One-time: Laguna Opus default thinking -> normal
        migrate_laguna_opus_default_to_normal(path);
        // One-time: migrate legacy OpenRouter config to multi-profile
        migrate_openrouter_to_profiles_at_path(path);
        // One-time: add built-in InclusionAI + StepFun profiles if missing
        ensure_builtin_openrouter_profiles_at_path(path)?;
        // Every startup: sync force_thinking with upstream model capabilities
        normalize_force_thinking(path);
        // Every startup: normalize OpenRouter config (repair, name normalization, active ID)
        normalize_config_at_path(path);
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
    let sonnet_entry = model_entry("claude-sonnet-5", "openai/gpt-5.6-terra", Some("thinking"), Some("medium"));
    let haiku_entry = model_entry("claude-haiku-4-5", "openai/gpt-5.6-luna", Some("thinking"), Some("low"));

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
/// The four built-in profile names are preserved as-is so that the fixed
/// display names ("OpenRouter: Laguna", "OpenRouter: Hy3",
/// "OpenRouter: InclusionAI", "OpenRouter: StepFun") are never renamed.
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

        // Never rename the four canonical built-in profiles.
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
        assert_eq!(profiles.len(), 5); // Laguna + Hy3 + InclusionAI + StepFun + GPT-5.6
        assert!(profiles.iter().any(|p| p["id"] == LAGUNA_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == HY3_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == INCLUSIONAI_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == STEPFUN_PROFILE_ID));
        assert!(profiles.iter().any(|p| p["id"] == GPT56_BALANCED_PROFILE_ID));
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
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone()],
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
        // Still 5 — no duplicates
        assert_eq!(profiles.len(), 5);
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
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone()],
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
            vec![laguna.clone(), hy3.clone(), inclusionai.clone(), stepfun.clone(), gpt56.clone()],
            None,
        );
        write_config(dir.path(), &cfg);

        let path = dir.path().join("config.json");
        ensure_builtin_openrouter_profiles_at_path(&path).unwrap();

        let result = read_config(dir.path());
        let profiles = result["providers"]["openrouter"]["profiles"]
            .as_array()
            .unwrap();
        // All 5 profiles have their display_name repaired
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
        // All 5 built-in profiles now have fixed UUIDs
        let laguna = build_laguna_profile_json("OpenRouter: Laguna");
        let hy3 = build_hy3_profile_json("OpenRouter: Hy3");
        let inclusionai = build_inclusionai_profile_json("OpenRouter: InclusionAI");
        let stepfun = build_stepfun_profile_json("OpenRouter: StepFun");
        let gpt56 = build_gpt56_balanced_profile_json(GPT56_BALANCED_PROFILE_NAME);
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
        // Custom legacy renamed to "Model 1" (no other numbered names in use)
        assert_eq!(profiles[5]["display_name"].as_str().unwrap(), "Model 1");
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
        assert_eq!(sonnet.reasoning_effort.as_deref(), Some("medium"));
        assert!(!sonnet.force_thinking.unwrap());

        let haiku = &profile.models["claude-haiku-4-5"];
        assert_eq!(haiku.upstream_model, "openai/gpt-5.6-luna");
        assert_eq!(haiku.thinking_mode.as_deref(), Some("thinking"));
        assert_eq!(haiku.reasoning_effort.as_deref(), Some("low"));
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
        assert_eq!(profiles.len(), 5);
        assert!(profiles.iter().any(|p| p["id"] == GPT56_BALANCED_PROFILE_ID));

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
}
