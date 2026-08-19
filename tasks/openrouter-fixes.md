# Plan: OpenRouter Release Fixes

## Context

v0.12.0 was built and tagged, but the release has not been published yet (installer created, GitHub release draft exists). The user identified 8 issues that must be fixed before the release is finalized. Issues #1 and #2 are **release-blocking**; the rest are quality improvements.

## Issues Overview

| # | Severity | Issue | Root Cause |
|---|----------|-------|------------|
| 1 | **BLOCKER** | Thinking override overwrites Claude Code's settings | `thinking_mode: "thinking"` in config.json → `ThinkingOverride::Enabled` → Anthro Bridge injects `{"type":"enabled"}` instead of passing through |
| 2 | **BLOCKER** | Cache save fails on Windows (2nd refresh) | `std::fs::rename` fails when destination file exists on Windows |
| 3 | HIGH | Cache directory may not exist on first run | `save_cache()` doesn't call `create_dir_all` |
| 4 | MEDIUM | Cache never expires (stale forever) | No TTL check; `force_refresh=false` always returns cache |
| 5 | LOW | Proxy capabilities don't refresh mid-run | `ModelRouteEntry` capabilities baked at proxy start |
| 6 | LOW | Video support falsely reported without cache | `supports_video: true` at provider level for all OpenRouter models |
| 7 | N/A | File path notation error in summary | Cosmetic only — actual file is correct |
| 8 | LOW | `reqwest::blocking` blocks command thread | Works but non-ideal; async is better |

## Fix Plan

### Fix 1: Thinking Override (BLOCKER)

**File**: `gui/src-tauri/src/proxy.rs` + `gui/src-tauri/resources/config.json`

**Root cause**: OpenRouter models have `"thinking_mode": "thinking"` in config.json. This causes `ThinkingOverride::Enabled` which injects `{"type": "enabled"}` into the body, overwriting any thinking/budget settings from Claude Code.

**Solution**: For OpenRouter provider, always use `ThinkingOverride::Default` (pass-through). Change config.json thinking_mode from `"thinking"` to a new value that maps to `Default`.

**Implementation**:
1. In `proxy.rs` `resolve_proxy_config()`, after computing `thinking` from `entry.thinking_mode`, override to `Default` when provider is `"openrouter"`:

```rust
let thinking = if *provider_id == "openrouter" {
    ThinkingOverride::Default  // Always pass through for OpenRouter
} else {
    match entry.thinking_mode.as_deref() {
        Some("normal") => ThinkingOverride::Disabled,
        Some("thinking") => ThinkingOverride::Enabled,
        Some("thinking_only") => ThinkingOverride::Forced,
        _ => { /* backward compat */ }
    }
};
```

2. Update config.json OpenRouter models to `"thinking_mode": "default"` (cosmetic — the code override makes this irrelevant, but makes intent clear).

**Files modified**:
- `gui/src-tauri/src/proxy.rs` (lines ~227-241)
- `gui/src-tauri/resources/config.json` (lines 473, 478, 483)

### Fix 2: Windows Cache Rename (BLOCKER)

**File**: `gui/src-tauri/src/openrouter.rs`

**Root cause**: Windows `std::fs::rename` fails when destination file already exists (unlike Unix which overwrites atomically).

**Solution**: Delete existing file before rename. Use `std::fs::remove_file` + `std::fs::rename`.

**Implementation** (in `save_cache()`):

```rust
fn save_cache(app_data_dir: &std::path::Path, cache: &OpenRouterModelCache) -> Result<(), String> {
    let path = cache_path(app_data_dir);
    let tmp_path = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(cache).map_err(|e| e.to_string())?;
    std::fs::write(&tmp_path, &json).map_err(|e| format!("Failed to write cache: {}", e))?;

    // Windows: rename fails if destination exists, so remove first
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to remove old cache: {}", e))?;
    }

    std::fs::rename(&tmp_path, &path)
        .map_err(|e| format!("Failed to rename cache file: {}", e))?;
    Ok(())
}
```

**Files modified**: `gui/src-tauri/src/openrouter.rs` (lines ~191-198)

### Fix 3: Cache Directory Creation

**File**: `gui/src-tauri/src/openrouter.rs`

**Root cause**: `save_cache()` assumes `app_data_dir` exists, but first run may not have created it yet.

**Solution**: Add `create_dir_all` at the start of `save_cache()`.

**Implementation**:

```rust
fn save_cache(app_data_dir: &std::path::Path, cache: &OpenRouterModelCache) -> Result<(), String> {
    let path = cache_path(app_data_dir);
    std::fs::create_dir_all(app_data_dir)
        .map_err(|e| format!("Failed to create app data directory: {}", e))?;
    // ... rest of save logic
}
```

**Files modified**: `gui/src-tauri/src/openrouter.rs`

### Fix 4: Cache TTL (24h)

**File**: `gui/src-tauri/src/openrouter.rs`

**Root cause**: Cache is returned unconditionally when `force_refresh=false`. Stale data persists indefinitely.

**Solution**: Add 24-hour TTL check. If cache is >24h old, attempt network fetch (fall back to stale cache on failure).

**Implementation**:

```rust
const CACHE_TTL_HOURS: i64 = 24;

fn is_cache_fresh(cache: &OpenRouterModelCache) -> bool {
    // Parse fetched_at timestamp (format: "2024-01-15T10:30:00+09:00")
    chrono::DateTime::parse_from_str(&cache.fetched_at, "%Y-%m-%dT%H:%M:%S%:z")
        .ok()
        .map(|dt| {
            let elapsed = chrono::Local::now().signed_duration_since(dt);
            elapsed.num_hours() < CACHE_TTL_HOURS
        })
        .unwrap_or(false)  // If parse fails, treat as stale
}

// In openrouter_get_models():
if !force_refresh {
    if let Some(cache) = load_cache(&app_data_dir) {
        if !cache.models.is_empty() && is_cache_fresh(&cache) {
            return Ok(OpenRouterModelsResult { ... source: "cache" ... });
        }
        // Cache exists but stale — try refresh, fall back to stale cache
    }
}
```

**Files modified**: `gui/src-tauri/src/openrouter.rs`

### Fix 5: Video Support False Positive

**File**: `gui/src-tauri/src/proxy.rs`

**Root cause**: When no cache exists, OpenRouter models use `p.supports_video` (true) for all models, but Claude family aliases may not support video.

**Solution**: Default to `false` for video when no cache available.

**Implementation** (in the capability resolution block):

```rust
} else if *provider_id == "openrouter" {
    // No cache yet — conservative defaults
    (false, p.supports_vision, p.supports_vision,
     false, false,  // video unknown without cache
     false, None)
}
```

**Files modified**: `gui/src-tauri/src/proxy.rs` (lines ~256-260)

### Fix 5b: Custom Model Capabilities

**Current code** returns `(false, true, true, false, false, false, None)` for custom models. This is reasonable (unknown = assume vision capable but no video). No change needed.

### Fix 8: Async `openrouter_get_models` (DEFERRED)

**Rationale**: Converting to async is a larger refactor that affects the Tauri command signature and Cargo.toml features. The blocking approach works correctly. Defer to a future release.

**Decision**: Keep `reqwest::blocking` for v0.12.0. The 30-second timeout is acceptable for a model list fetch that only happens on-demand.

## Implementation Order

1. Fix 1 (Thinking override) — BLOCKER
2. Fix 2 (Windows cache rename) — BLOCKER
3. Fix 3 (Directory creation) — HIGH
4. Fix 4 (Cache TTL) — MEDIUM
5. Fix 5 (Video false positive) — LOW
6. Verify: `cargo check` + `npx tsc --noEmit`
7. Rebuild: `CARGO_BUILD_JOBS=1 npm run tauri build`
8. Delete old v0.12.0 tag + GitHub release
9. Re-commit, re-tag, re-push, re-release

## Files Modified

| File | Fixes |
|------|-------|
| `gui/src-tauri/src/proxy.rs` | #1 (thinking override), #5 (video default) |
| `gui/src-tauri/src/openrouter.rs` | #2 (rename), #3 (mkdir), #4 (TTL) |
| `gui/src-tauri/resources/config.json` | #1 (thinking_mode values) |

## Verification Checklist

After fixes:
1. `cargo check` — clean
2. `npx tsc --noEmit` — clean
3. `CARGO_BUILD_JOBS=1 npm run tauri build` — success
4. Release installer size ~3.1 MB (not debug)
5. Code review: OpenRouter thinking passes through untouched
6. Code review: Cache save works on Windows (remove + rename)
7. Code review: Cache expires after 24h
