# CLAUDE.md — Anthro Bridge

## Project Identity
- **Name**: Anthro Bridge
- **Repository**: https://github.com/soheidon/anthro-bridge
- **Identifier**: `com.soheidon.anthro-bridge`
- **Author**: @soheidon
- **License**: MIT
- **Current version**: 0.8.0

## What This Is
Anthro Bridge is a proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints (DeepSeek, MiniMax, Kimi/Moonshot).

It is **NOT** a fork, GUI, or companion app for Moon Bridge — it is an independent Anthropic-compatible gateway.

## Architecture
```
gui/                        Tauri v2 + React 19 + TypeScript
├── src/                    React frontend
│   ├── components/         UI components
│   ├── hooks/              Custom hooks
│   └── i18n/lang/          en, ja, zh-CN, zh-TW, ko, fr
├── src-tauri/              Rust backend
│   ├── src/lib.rs          24 Tauri commands + proxy lifecycle
│   ├── src/main.rs         Entry point
│   ├── src/proxy.rs        axum proxy server (0.7 + reqwest)
│   └── Cargo.toml          Package: anthro-bridge, lib: anthro_bridge_lib
├── index.html              <title>Anthro Bridge</title>
└── package.json            "name": "anthro-bridge"
```

## Build
```bash
cd gui
npm install
npx tsc --noEmit          # TypeScript check
cargo check                # from gui/src-tauri/
npm run tauri build        # Production build → NSIS installer
npm run tauri dev          # Dev mode with HMR
```

## Release Process
1. Bump version in: `gui/package.json`, `gui/src-tauri/Cargo.toml`, `gui/src-tauri/tauri.conf.json`, `gui/src-tauri/Cargo.lock`, `gui/src/components/Header.tsx`
2. Verify: `npx tsc --noEmit` && `cargo check` — both must pass
3. Build: `npm run tauri build` → produces `Anthro Bridge_<version>_x64-setup.exe`
4. Commit with clean message (NO `Co-Authored-By` lines)
5. Tag: `git tag -a v<version> -m "v<version>: <summary>"`
6. Push: `git push origin main && git push origin v<version>`
7. Release: `gh release create v<version> --title "Anthro Bridge v<version>" --notes "<notes>" "gui/src-tauri/target/release/bundle/nsis/Anthro Bridge_<version>_x64-setup.exe"`

## Git Conventions
- **Default branch**: `main`
- **Remote**: `https://github.com/soheidon/anthro-bridge.git`
- **Do NOT add Co-Authored-By lines** to commits
- Commit messages in English, concise

## AppData Migration
`lib.rs` has `migrate_old_config()` that auto-migrates on first run:
1. `%APPDATA%\Terra Bridge\config.json` → `%APPDATA%\Anthro Bridge\config.json`
2. `%APPDATA%\Anthropic Proxy Gateway\config.json` → `%APPDATA%\Anthro Bridge\config.json`

## Key Files
| File | Purpose |
|------|---------|
| `README.md` / `docs/README.ja.md` / `docs/README.zh-CN.md` | Per-language README with switcher links |
| `SPEC.md` / `docs/SPEC.ja.md` / `docs/SPEC.zh-CN.md` | Per-language specification |
| `docs/THIRD_PARTY_INFERENCE.md` / `.ja.md` / `.zh-CN.md` | Third-party inference setup guide |
| `config.json` | Provider configuration |
| `gui/src-tauri/src/lib.rs` | 24 Tauri commands + AppData + proxy lifecycle |
| `gui/src-tauri/src/proxy.rs` | axum HTTP proxy, model routing, media checks |
| `gui/src-tauri/tauri.conf.json` | productName, identifier, version, NSIS config |
| `gui/src/i18n/lang/*.ts` | 6-language translations |

## i18n
6 languages: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français
Add new language: copy `en.ts` → translate → rebuild. No code changes needed.

## Old Names (for reference, do not reintroduce)
- Anthropic Proxy Gateway (original name)
- Terra Bridge (intermediate name, v0.7.x)

