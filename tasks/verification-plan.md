# Verification & Fix Plan — OpenRouter UI + Dev Path Separation

**Date**: 2026-07-31  
**Status**: Code written, 2 bugs found, build/UI verification pending

---

## Phase 0: Bug Fixes (must fix before verification)

### Bug 0-1: `startRename` uses canonical name instead of locale display name

**File**: `gui/src/components/OpenRouterModelSetCard.tsx:110`

**Current (wrong)**:
```typescript
const startRename = useCallback(() => {
    setRenamingId(profile.id);
    setRenameText(profile.display_name);  // "Model 2" — canonical English
    ...
}, [profile.id, profile.display_name]);
```

**Fix**:
```typescript
const startRename = useCallback(() => {
    setRenamingId(profile.id);
    setRenameText(displayName);  // "モデル 2" in Japanese locale
    ...
}, [profile.id, displayName]);
```

**Rationale**: The card displays `displayName` (locale-aware, e.g. "モデル 2" in Japanese). The rename input should start with what the user sees, not the canonical stored name. This prevents jarring flicker from "モデル 2" → "Model 2" on double-click.

**Dependency fix**: Add `displayName` to the `useCallback` dependency array instead of `profile.display_name`.

---

### Bug 0-2: `handleAddProfile` passes translated prefix as name (no numbering, wrong language)

**File**: `gui/src/components/ApiKeyPanel.tsx:898-922`

**Current (wrong)**:
```typescript
const handleAddProfile = useCallback(async () => {
    ...
    res = await invoke<CommandResponse>("add_openrouter_profile", {
      name: t("openRouterProfile.defaultNewName"),  // "モデル" in JA, "Modèle" in FR — NO NUMBER!
    });
    ...
}, [t, refreshConfig, gatewayRunning, restartGateway]);
```

**Problems**:
1. No auto-numbering — sends bare prefix (e.g. "モデル") without " 3"
2. Sends locale-translated string instead of canonical "Model"

**Fix**: Import `parseAutoModelSetNumber` from OpenRouterProviderSection (or extract to shared util), compute next number, send canonical `Model ${number}`:
```typescript
const handleAddProfile = useCallback(async () => {
    setAddError(null);
    // Compute next available Model N number from existing profiles
    const openRouterProfiles = config.providers["openrouter"]?.profiles ?? [];
    const used = new Set<number>();
    for (const p of openRouterProfiles) {
      const n = parseAutoModelSetNumber(p.display_name);
      if (n !== null) used.add(n);
    }
    let next = 1;
    while (used.has(next)) next++;
    const name = `Model ${next}`;

    let res: CommandResponse;
    try {
      res = await invoke<CommandResponse>("add_openrouter_profile", { name });
    } catch (e) {
      setAddError(String(e));
      return;
    }
    ...
}, [config, refreshConfig, gatewayRunning, restartGateway]);
```

**Note**: `parseAutoModelSetNumber` must be extracted from `OpenRouterProviderSection.tsx` into a shared location, or imported via named export.

---

## Phase 1: Dev/Stable Build Verification (Task B)

### 1-1: Build dev installer
```bash
cd gui
npm run tauri:build:dev
```
**Expected**: Produces `Anthro Bridge Dev_<ver>_x64-setup.exe`

### 1-2: Build stable installer
```bash
cd gui
npm run tauri:build:stable
```
**Expected**: Produces `Anthro Bridge_<ver>_x64-setup.exe`

### 1-3: Startup log verification (dev)
Launch the dev binary. Check startup log for:
```
channel=Dev
data_dir=C:\Users\Sohei\AppData\Roaming\Anthro Bridge Dev
```

### 1-4: Startup log verification (stable)
Launch the stable binary. Check startup log for:
```
channel=Stable
data_dir=C:\Users\Sohei\AppData\Roaming\Anthro Bridge
```

### 1-5: Stable config isolation (SHA256)
```powershell
$stable = "$env:APPDATA\Anthro Bridge\config.json"
$before = (Get-FileHash $stable -Algorithm SHA256).Hash

# Launch Dev build, change settings, refresh model list, route a request through gateway

$after = (Get-FileHash $stable -Algorithm SHA256).Hash

[PSCustomObject]@{
  Before    = $before
  After     = $after
  Unchanged = $before -eq $after
}
```
**Expected**:
```
Unchanged : True
```

### 1-6: Dev installer isolation
- Dev installer uses `productName: "Anthro Bridge Dev"`, `identifier: "com.soheidon.anthro-bridge.dev"`
- Dev exe is `Anthro Bridge Dev.exe`, stable is `Anthro Bridge.exe`
- Both appear in Start Menu independently
- Dev window title shows "Anthro Bridge (DEV)"

### 1-7: Data path staging (dev first run)
After dev first run + model refresh + gateway request:
```powershell
Get-ChildItem "$env:APPDATA\Anthro Bridge Dev" -Recurse
```
**Expected files**:
- `config.json`
- `user_prefs.json`
- `openrouter_models.json`
- `Communication-Logs\` (directory with log files)

---

## Phase 2: UI Manual Verification (Task A)

### 2-1: OpenRouter API key section appears exactly once
- [ ] No duplicate API key inputs
- [ ] Env var name input + save works
- [ ] API key input + save works
- [ ] "Refresh model list" button works, spinner starts/stops correctly

### 2-2: Model set cards match profile count
- [ ] Card count = `profiles.length`
- [ ] Each card has expand/collapse toggle

### 2-3: Legacy name normalization
- [ ] Existing profiles show as "モデル 1", "モデル 2" (Japanese) or "Model 1", "Model 2" (English)
- [ ] Stored name in config.json is canonical "Model 1", "Model 2"

### 2-4: New model set auto-numbering (after fix 0-2)
- [ ] "Add Model Set" → creates "Model 3" (if 2 exist)
- [ ] Displayed as locale-aware: "モデル 3" in Japanese
- [ ] Stored as "Model 3" in config.json

### 2-5: Model set actions
- [ ] "Use this" button switches active model set
- [ ] Double-click name → rename input appears with locale display name (not canonical)
- [ ] Rename: Enter saves, Escape cancels, blur saves
- [ ] Delete: non-active shows simple confirm, active shows "will switch" confirm
- [ ] Last model set: delete button disabled with tooltip

### 2-6: Delete confirmation messages
- [ ] Active model set: "このモデルセットを削除しますか？アクティブなモデルセットは次のモデルセットに切り替わります。" (JA)
- [ ] Inactive model set: "このモデルセットを削除しますか？" (JA)

### 2-7: Model refresh spinner guarantee
- [ ] Click "Refresh model list" → spinner appears
- [ ] After successful fetch → spinner stops
- [ ] After failed fetch → spinner still stops (finally block)
- [ ] No permanent spinner

### 2-8: Rapid model changes
- [ ] Change model in selector rapidly 5+ times → last selection saved
- [ ] No stale UI state from superseded saves

### 2-9: Gateway integration
- [ ] Start gateway → routing uses active model set's models
- [ ] Switch active model set → gateway restarts with new routing

---

## Phase 3: Code Quality Checks

### 3-1: TypeScript
```bash
cd gui && npx tsc --noEmit
```
**Expected**: Clean, no errors.

### 3-2: Rust
```bash
cd gui/src-tauri && cargo check
```
**Expected**: Only pre-existing LogGuard warning.

### 3-3: Rust tests
```bash
cd gui/src-tauri && cargo test
```
**Expected**: All tests pass, report actual count.

### 3-4: Git diff
```bash
git diff --stat
```

---

## Files Summary

### New files
| File | Purpose |
|------|---------|
| `gui/src/components/OpenRouterProviderSection.tsx` | OpenRouter auth + model set cards container |
| `gui/src/components/OpenRouterModelSetCard.tsx` | Single model set card (rename/delete/activate + 3 selectors) |
| `gui/src-tauri/src/paths.rs` | AppChannel enum + all data path functions + tests |
| `gui/src-tauri/tauri.dev.conf.json` | Dev build identity overrides |

### Modified files
| File | Changes |
|------|---------|
| `gui/src/components/ApiKeyPanel.tsx` | Extract OR code → new components; simplified ProviderRow |
| `gui/src/i18n/lang/{en,ja,zh-CN,zh-TW,ko,fr,de,es}.ts` | 10 keys changed, 1 key added — "profile" → "model set" |
| `gui/src-tauri/src/lib.rs` | `mod paths`, `ensure_config_initialized()`, startup log, gated migration |
| `gui/src-tauri/src/openrouter.rs` | `cache_path()` → `paths::openrouter_models_cache_path()` |
| `gui/src-tauri/build.rs` | `cargo:rerun-if-env-changed=ANTHRO_BRIDGE_CHANNEL` |
| `gui/package.json` | `tauri`, `tauri:dev`, `tauri:build:dev`, `tauri:build:stable` scripts |

---

## Bug Fix Checklist

| # | Bug | File | Severity | Status |
|---|-----|------|----------|--------|
| 0-1 | `startRename` uses canonical name, not locale display | `OpenRouterModelSetCard.tsx:110` | Medium | **TO FIX** |
| 0-2 | `handleAddProfile` sends translated prefix, no number | `ApiKeyPanel.tsx:902-903` | High | **TO FIX** |

---

## Verification Checklist

| # | Check | Phase | Status |
|---|-------|-------|--------|
| 1 | `npx tsc --noEmit` clean | 3 | ☐ |
| 2 | `cargo check` clean (LogGuard only) | 3 | ☐ |
| 3 | `cargo test` all pass | 3 | ☐ |
| 4 | dev build succeeds | 1 | ☐ |
| 5 | stable build succeeds | 1 | ☐ |
| 6 | dev startup log: channel=Dev | 1 | ☐ |
| 7 | dev startup log: data_dir=...\Anthro Bridge Dev | 1 | ☐ |
| 8 | stable startup log: channel=Stable | 1 | ☐ |
| 9 | stable startup log: data_dir=...\Anthro Bridge | 1 | ☐ |
| 10 | stable config SHA256 unchanged after dev ops | 1 | ☐ |
| 11 | dev installer doesn't overwrite stable | 1 | ☐ |
| 12 | dev window title: "Anthro Bridge (DEV)" | 1 | ☐ |
| 13 | dev data path staging (all 4 items) | 1 | ☐ |
| 14 | OR API key section appears once | 2 | ☐ |
| 15 | Card count = profiles.length | 2 | ☐ |
| 16 | Legacy names → "Model 1", "Model 2" | 2 | ☐ |
| 17 | New model set → auto-numbered "Model N" | 2 | ☐ |
| 18 | "Use this" switches active | 2 | ☐ |
| 19 | Rename starts with locale display name | 2 | ☐ |
| 20 | Last model set cannot be deleted | 2 | ☐ |
| 21 | Delete confirm messages correct | 2 | ☐ |
| 22 | Refresh spinner always stops | 2 | ☐ |
| 23 | Rapid model changes → last wins | 2 | ☐ |
| 24 | Gateway routes via active model set | 2 | ☐ |
