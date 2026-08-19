# Development & Verification Guide

This document contains instructions for building, developing, testing, and verifying Anthro Bridge.

---

## 1. Project Structure

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── docs/
│   ├── ANTIGRAVITY_MCP.md
│   ├── CONFIGURATION.md
│   ├── DEVELOPMENT.md
│   ├── PROVIDERS.md
│   └── THIRD_PARTY_INFERENCE.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── config/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   └── model_capabilities.rs
│   │   └── resources/
│   └── package.json
├── mcp-server/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── main.rs
│   │   └── provider/
│   └── Cargo.toml
└── LICENSE
```

---

## 2. Running in Development Mode

```bash
cd gui
npm install
npm run tauri dev
```

### Building the Development Variant

On Windows, use a single Rust build job to avoid intermittent compiler memory termination:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Development builds use:
- Window title: `Anthro Bridge (DEV)`
- Application identity: `com.soheidon.anthro-bridge.dev`
- Port: `4000`
- Data directory: `%APPDATA%\Anthro Bridge Dev\`

---

## 3. Automated Verification

### Frontend Typecheck & Tests

```bash
cd gui
npm run build
npx tsc --noEmit
npx vitest run
```

### Rust Tests

```bash
# GUI / Tauri backend tests
cd gui/src-tauri
cargo check
cargo test

# MCP Server tests
cd mcp-server
cargo test
```

---

## 4. Manual Verification Checklist

Before creating a release, verify the following:

- [ ] Window controls (minimize, maximize/restore, close) operate smoothly on the custom titlebar.
- [ ] Titlebar workspace tabs (`Anthro Bridge` and `MCP`) switch views correctly and cleanly fuse with the header.
- [ ] Dragging works from the title and central spacer; tabs and buttons do not drag the window.
- [ ] Opening Settings and clicking a titlebar tab closes Settings and navigates to the target workspace.
- [ ] Every built-in provider and OpenRouter profile saves and loads routes reliably.
- [ ] Gateway start/stop functions reliably and reports health on `http://127.0.0.1:4000/health`.
- [ ] Copied Claude Code launch commands contain correct base URL and context window parameters.
- [ ] Anthro Bridge MCP server connects cleanly to Google Antigravity and executes `anthro-bridge/plan`.
