[English](README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md)

# Terra Bridge

## Overview

Terra Bridge is a proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

Terra Bridge reads the `model` field from each request and automatically routes to the correct upstream provider (model-based routing). Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

The GUI management tool (Tauri v2 + React 19 + TypeScript) provides start/stop control, config editing, log viewing, and API key management from a native Windows window.

### Why This Gateway Is Needed

Claude Desktop / Claude Code fundamentally expects Anthropic's API format and Claude-family model names. Even when providers like DeepSeek, MiniMax, and Kimi offer Anthropic-compatible APIs, Claude Desktop / Claude Code cannot always use them directly.

In particular, **Claude Desktop's `inferenceModels[].name` only accepts Anthropic official model names**. Gateway custom names like `claude-deepseek-v4` or `kimi-k2.6` are rejected as `"not an Anthropic model"`.

To work around this constraint, Terra Bridge **presents Anthropic official model names (`claude-sonnet-4-6` / `claude-haiku-4-5`) as "shells" to Claude Desktop, while the actual LLM (DeepSeek / MiniMax / Kimi) is selected in the GUI**.

```
Claude Desktop side (always fixed)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Gateway internal (based on GUI selection)
  DeepSeek:  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:   Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:      Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
```

This lets you pass Claude Desktop's model name validation while freely switching between DeepSeek, MiniMax, and Kimi.

### Prerequisites

- **Windows 10/11** (Japanese locale supported)
- API key for your chosen provider (DeepSeek / MiniMax / Kimi — **just one is enough**, since v0.5.0)

### Quick Start

#### 1. Install

Download the latest installer from [Releases](https://github.com/soheidon/terra-bridge/releases) and run it.

The installer shows a language selection screen on launch (choose from English, 日本語, 中文(简体), 中文(繁體), 한국어, Français).

#### 2. Set API Key

Settings (⚙) -> **API Key** tab, enter your provider's API key and click **Save**.
The key is persisted as a Windows user environment variable.

| Provider | Environment Variable |
|----------|---------------------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 3. Select Provider

On the Dashboard, click a provider tile (DeepSeek / MiniMax / Kimi) under **Select LLM Provider**.

#### 4. Start Gateway

Click **Start Gateway** in the header. The proxy starts on `http://127.0.0.1:4000`.

#### 5. Configure Claude Desktop

Settings (⚙) -> **Claude Desktop Setup** tab:

1. Click "Copy Claude Desktop Config"
2. In Claude Desktop, click "Open Config File"
3. Delete existing content, paste the copied settings

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-sonnet-4-6", "labelOverride": "Sonnet 4.6" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Haiku 4.5" }
  ]
}
```

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/models` | Public model list |
| POST | `/v1/messages` | Messages API (stream + non-stream). Model-based routing |
| POST | `/v1/messages/count_tokens` | Token counting (supported providers only) |

### Routing

Model-based routing: the `model` field in each request determines the target provider and upstream model.

### Languages

6 languages: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français.

To add a new translation, drop a language file (e.g., `es.ts`) into `gui/src/i18n/lang/` and rebuild.
See [CONTRIBUTING](CONTRIBUTING.md) for details.

### Configuration (config.json)

Provider settings define upstream model names and capability flags per model. Normally no editing is required.
Advanced users can edit via Settings (⚙) -> **Gateway Config**.

| Key | Description |
|-----|-------------|
| `models.<model>.upstream_model` | Actual model name sent to upstream (required) |
| `models.<model>.thinking` | When `"disabled"`, injects thinking suppression (optional) |
| `models.<model>.supports_vision` | Per-model image support (falls back to provider default) |
| `models.<model>.supports_video` | Per-model video support (falls back to provider default) |
| `models.<model>.visible` | Whether to expose in `/v1/models` and dashboard (default `true`) |
| `non_vision_image_policy` | Image handling for non-vision models: `replace` (placeholder) / `drop` / `reject` (error) |

### Project Structure

```
terra-bridge/
├── README.md
├── README.ja.md               Japanese
├── README.zh-CN.md            Chinese Simplified
├── SPEC.md                    Specification
├── SPEC.ja.md                 Japanese
├── SPEC.zh-CN.md              Chinese Simplified
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

DeepSeek does not support images or video. Images are automatically replaced with placeholder text (`non_vision_image_policy: "replace"`). To use images natively, switch to MiniMax or Kimi. Video is always rejected.

### License

MIT — see [LICENSE](LICENSE) for details.
