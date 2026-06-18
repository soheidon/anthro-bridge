[English](README.md) | [日本語](docs/README.ja.md) | [中文(简体)](docs/README.zh-CN.md) | [中文(繁體)](docs/README.zh-TW.md) | [한국어](docs/README.ko.md) | [Français](docs/README.fr.md) | [Deutsch](docs/README.de.md) | [Español](docs/README.es.md)

# Anthro Bridge

## Overview

Anthro Bridge is a proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

Anthro Bridge reads the `model` field from each request and automatically routes to the correct upstream provider (model-based routing). Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

Anthro Bridge is not a fork, GUI, or companion app for Moon Bridge; it is an independent Anthropic-compatible gateway.

### Supported Providers

| Provider ID | Display Name | Upstream Endpoint | Default Model |
|-------------|--------------|-------------------|---------------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

The GUI management tool (Tauri v2 + React 19 + TypeScript) provides start/stop control, config editing, log viewing, and API key management from a native Windows window.

### Why This Gateway Is Needed

Claude Desktop / Claude Code fundamentally expects Anthropic's API format and Claude-family model names. Even when providers like DeepSeek, MiniMax, Kimi, and MiMo offer Anthropic-compatible APIs, Claude Desktop / Claude Code cannot always use them directly.

In particular, **Claude Desktop's `inferenceModels[].name` only accepts Anthropic official model names**. Gateway custom names like `claude-deepseek-v4` or `kimi-k2.6` are rejected as `"not an Anthropic model"`.

To work around this constraint, Anthro Bridge **presents Anthropic official model names (`claude-sonnet-4-6` / `claude-haiku-4-5`) as "shells" to Claude Desktop, while the actual LLM (DeepSeek / MiniMax / Kimi / MiMo) is selected in the GUI**.

```
Claude Desktop side (always fixed)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Gateway internal (based on GUI selection)
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

This lets you pass Claude Desktop's model name validation while freely switching between DeepSeek, MiniMax, Kimi, and MiMo.

### Prerequisites

- **Windows 10/11** (Japanese locale supported)
- API key for your chosen provider (DeepSeek / MiniMax / Kimi / MiMo — **just one is enough**, since v0.5.0)

### Quick Start

#### 1. Install

Download the latest installer from [Releases](https://github.com/soheidon/anthro-bridge/releases) and run it.

The installer shows a language selection screen on launch (choose from English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español).

#### Updating

Simply run the new `setup.exe` — it automatically detects and replaces the previous version. Manual uninstallation is not required. Your settings (`%APPDATA%\Anthro Bridge\config.json`) are preserved across updates.

#### 2. Set API Key

Settings (⚙) -> **API Key** tab, enter your provider's API key and click **Save**.
The key is persisted as a Windows user environment variable.

| Provider | Environment Variable | Notes |
|----------|---------------------|-------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. Select Provider

On the Dashboard, the provider tiles are laid out in a 2×2 grid:

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

Click a tile to select the active provider under **Select LLM Provider**.

#### 4. Start Gateway

Click **Start Gateway** in the header. The proxy starts on `http://127.0.0.1:4000`.

#### 5. Configure Claude Desktop / Cowork on 3P

See [docs/THIRD_PARTY_INFERENCE.md](docs/THIRD_PARTY_INFERENCE.md) for detailed step-by-step instructions.

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/models` | Public model list |
| POST | `/v1/messages` | Messages API (stream + non-stream). Model-based routing |
| POST | `/v1/messages/count_tokens` | Token counting (supported providers only) |

### Routing

Model-based routing: the `model` field in each request determines the target provider and upstream model.

| Anthropic Model | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|-----------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking on) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking off) | `mimo-v2.5` |

#### MiMo routing details

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`**: Thinking is **enabled by default** (`thinking_mode: "thinking"`). The `thinking_mode` key (not `thinking`) controls MiMo's thinking behavior. Set to `"default"` for standard mode.
- **`claude-haiku-4-5` → `mimo-v2.5`**: Supports image pass-through (image URL and base64). Audio/video input is not supported by Anthro Bridge on MiMo.
- **`claude-sonnet-4-6` route does NOT support images.** When images are sent to this route, they are replaced with text placeholders (`non_vision_image_policy: "replace"`).
- **Upstream endpoint**: Requests are sent to `https://api.xiaomimimo.com/anthropic/v1/messages`.

### Languages

8 languages: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español.

To add a new translation, drop a language file (e.g., `es.ts`) into `gui/src/i18n/lang/` and rebuild.
See [CONTRIBUTING](CONTRIBUTING.md) for details.

### Configuration (config.json)

Provider settings define upstream model names and capability flags per model. Normally no editing is required.
Advanced users can edit via Settings (⚙) -> **Gateway Config**.

| Key | Description |
|-----|-------------|
| `models.<model>.upstream_model` | Actual model name sent to upstream (required) |
| `models.<model>.thinking` | When `"disabled"`, injects thinking suppression (optional). For MiMo, use `thinking_mode` instead |
| `models.<model>.thinking_mode` | MiMo-specific: `"thinking"` (enabled) or `"default"` (standard). Only used by the MiMo provider |
| `models.<model>.supports_vision` | Per-model image support (falls back to provider default) |
| `models.<model>.supports_video` | Per-model video support (falls back to provider default) |
| `models.<model>.visible` | Whether to expose in `/v1/models` and dashboard (default `true`) |
| `non_vision_image_policy` | Image handling for non-vision models: `replace` (placeholder) / `drop` / `reject` (error) |

### Project Structure

```
anthro-bridge/
├── README.md
├── SPEC.md                    Specification
├── docs/
│   ├── README.ja.md           Japanese
│   ├── README.zh-CN.md        Chinese Simplified
│   ├── SPEC.ja.md             Japanese
│   ├── SPEC.zh-CN.md          Chinese Simplified
│   ├── THIRD_PARTY_INFERENCE.md   Third-party inference guide
│   ├── THIRD_PARTY_INFERENCE.ja.md
│   └── THIRD_PARTY_INFERENCE.zh-CN.md
├── LICENSE                    MIT License
├── config.json                Provider configuration
├── .gitignore
└── gui/
    ├── src/                   React frontend (TypeScript)
    │   ├── components/        UI components
    │   ├── hooks/             Custom hooks
    │   └── i18n/              Multi-language support
    │       └── lang/          Language files (en, ja, zh-CN, zh-TW, ko, fr)
    ├── src-tauri/             Tauri backend (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 Tauri commands + proxy lifecycle
    │   │   ├── main.rs        Entry point
    │   │   └── proxy.rs       axum proxy server
    │   ├── resources/
    │   │   └── config.json    Bundled configuration
    │   └── Cargo.toml
    └── package.json
```

### Dev Build

```bash
cd gui
npm install
npm run tauri build    # Production build
npm run tauri dev      # Dev mode (HMR)
```

Requires [Rust](https://rustup.rs/) stable toolchain and Node.js 24+.

### Troubleshooting

#### Port 4000 in use

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Image/video rejected

DeepSeek does not support images or video. Images are automatically replaced with placeholder text (`non_vision_image_policy: "replace"`). To use images natively, switch to MiniMax, Kimi, or MiMo (`claude-haiku-4-5` route).

MiMo's `claude-sonnet-4-6` route also does not support images — use `claude-haiku-4-5` for image tasks. Video is always rejected.

#### MiMo: existing user config not reflecting changes

If you upgraded from a version before v0.9.0, your saved user config may still have the old `"display_name": "MiMo"` or `"thinking": "default"` values. v0.9.0 auto-migrates these on first launch, but if you experience issues:

1. **Restart the app** — the auto-migration runs at startup.
2. **Reset config**: Delete `%APPDATA%\Anthro Bridge\config.json` and restart. The bundled config with the correct MiMo settings will be used.
3. **Manual check**: Open `%APPDATA%\Anthro Bridge\config.json` and verify `providers.mimo` has `"display_name": "MiMo / Xiaomi"`, `"api_key_env": "XIAOMI_API_KEY"`, and `thinking_mode` (not `thinking`) on model entries.

### Manual Test — MiMo / Xiaomi

#### Text-only (claude-sonnet-4-6 → mimo-v2.5-pro)

1. Set `XIAOMI_API_KEY` in Settings → API Key tab → Save.
2. Select **MiMo / Xiaomi** on the dashboard.
3. Start Gateway.
4. Send a message via Claude Desktop. Verify the response arrives with thinking blocks.

#### Image test (claude-haiku-4-5 → mimo-v2.5)

1. Select **MiMo / Xiaomi** on the dashboard.
2. In Claude Desktop, attach an image to a message and send.
3. Verify the image is received and described correctly.
4. Sending an image to `claude-sonnet-4-6` should result in a placeholder text replacement.

#### Verification

Check the Log panel in the GUI — requests should show the `model` field rewritten to `mimo-v2.5-pro` or `mimo-v2.5` depending on the route.

### License

MIT — see [LICENSE](LICENSE) for details.
