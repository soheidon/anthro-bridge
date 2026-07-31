[English](README.md) | [日本語](docs/README.ja.md) | [中文(简体)](docs/README.zh-CN.md) | [中文(繁體)](docs/README.zh-TW.md) | [한국어](docs/README.ko.md) | [Français](docs/README.fr.md) | [Deutsch](docs/README.de.md) | [Español](docs/README.es.md)

# Anthro Bridge

Anthro Bridge is a local gateway and desktop configuration tool that lets Claude Desktop and Claude Code use multiple third-party LLM providers through an Anthropic-compatible API.

The application consists of:

- A local proxy server written in Rust
- A native Windows GUI built with Tauri 2, React, and TypeScript
- Model-based routing from Anthropic model names to provider-specific upstream models
- Per-route model, reasoning, and capability configuration

Anthro Bridge is an independent project. It is not a fork, frontend, or companion application for Moon Bridge.

## Supported Models

Anthro Bridge supports two categories of upstream models.

### Native Integrations

These providers are supported through their own Anthropic-compatible APIs. No OpenRouter account is required.

| Provider | Supported model families | Connection |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro and V4 Flash | Direct provider API |
| MiniMax | MiniMax M3 and M2.7 variants | Direct provider API |
| Kimi / Moonshot | Kimi K2.x and Kimi K3 | Direct provider API |
| MiMo / Xiaomi | MiMo V2.5 and V2.5 Pro variants | Direct provider API |

### Models Supported Through OpenRouter

These models are accessed through an OpenRouter profile. Each profile has its own API key, route mappings, and reasoning settings.

| Vendor or model family | Built-in support | Reasoning controls |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | Yes | Model-specific Thinking controls |
| Tencent Hy3 | Yes | Low and High reasoning effort |
| InclusionAI Ring | Yes | Model-specific Thinking and reasoning controls |
| StepFun Step 3.5 / Step 3.7 | Yes | Low, Medium, and High where supported |
| InclusionAI Ling family | Yes | Model-specific Thinking controls |

Other OpenRouter models can also be selected from the live OpenRouter model list or entered manually. Built-in support means Anthro Bridge already knows the model family, capability flags, vendor grouping, and reasoning-control behavior.

## How It Works

Claude Desktop and Claude Code send requests using Anthropic model names such as:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge treats these names as stable route identifiers. The GUI determines which provider and upstream model each route uses.

Example:

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

Only fields that must be adapted for the upstream provider are changed. Messages, tool calls, tool results, thinking blocks, and streaming data are otherwise preserved whenever the upstream API supports them.

## Main Features

### Provider Routing

Anthro Bridge supports two upstream connection types:

1. **Direct provider integrations**, which connect to a provider's own Anthropic-compatible API.
2. **OpenRouter profiles**, which connect to OpenRouter and can route to multiple vendors and model families through a single API.

#### Direct Provider Integrations

| Provider ID | Display Name | Default Endpoint |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter Integration

| Connection Type | Display Name | Endpoint |
|---|---|---|
| Multi-profile model gateway | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter is not treated as a single model provider. Each OpenRouter profile can independently select models from supported vendor groups such as Poolside, Tencent, InclusionAI, and StepFun, as well as other models discovered from the OpenRouter API or entered manually.

Each Anthropic route can be mapped independently to either a direct-provider model or a model selected through an OpenRouter profile.

### OpenRouter Multi-Profile Support

Multiple OpenRouter profiles can be created and managed independently.

Each profile has its own:

- Profile name
- API key configuration
- Opus, Sonnet, and Haiku route mappings
- Thinking or reasoning settings
- Cached OpenRouter model list

Profiles can be added, renamed, deleted, reordered by drag and drop, hidden, and selected from the GUI. The dashboard displays one card per visible profile and keeps the saved order after refresh.

Built-in OpenRouter vendor groups currently include Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, and other recognized model families. Unknown models remain available through search or custom model entry. The dashboard shortens vendor-qualified IDs such as `poolside/laguna-s-2.1` to `laguna-s-2.1` for readability while retaining the full ID for routing.

### OpenRouter Pricing and Model Details

The Settings model pricing panel shows built-in prices for supported OpenRouter models, including prompt, output, and cached-input pricing. Promotional prices can be displayed together with revised standard prices, including the GPT-5.6 Sol, Terra, and Luna variants and their Pro variants. Pricing notes can include long-context pricing where applicable.

### Responsive Dashboard Sizing

The initial window height is calculated from the number of visible provider and OpenRouter cards in the three-column dashboard. Additional card rows increase the window height while respecting the native minimum size, monitor work area, DPI scaling, and title-bar decorations. When profile visibility or count changes, the height is recalculated for the new row count; manual resizing is preserved while the row count remains unchanged.

### Localized Windows Installer

The Windows NSIS installer provides language selection for English, Japanese, Simplified Chinese, Traditional Chinese, Korean, French, German, and Spanish. The installer uses the Anthro Bridge application icon and preserves stable user configuration during upgrades.

### Latest UI Reliability Improvements

Configuration writes are serialized, OpenRouter saves use a queued update path with stale-request protection, and profile reorder operations recover cleanly after refresh failures. Regression tests cover profile ordering, save races, model pricing, dashboard card counting, and window sizing.

### Model and Reasoning Controls

The available controls depend on the selected model.

Supported controls may include:

- Thinking on or off
- Normal, low, medium, high, xhigh, or max reasoning modes
- Provider-specific reasoning effort
- Fixed reasoning modes for models that do not allow user selection

When switching models, Anthro Bridge attempts to preserve the closest compatible reasoning setting. If the exact previous setting is unavailable, it selects the nearest supported option, preferring the weaker option when two choices are equally close.

### Capability Detection

Anthro Bridge combines a built-in capability registry with live OpenRouter metadata.

Capabilities may include:

- Image input
- Video input
- Thinking support
- Reasoning-effort support
- Known pricing
- Provider-specific request translation rules

Live OpenRouter metadata is cached to reduce unnecessary API calls.

### Response Model Normalization

Upstream APIs often return their own model name in responses. Anthro Bridge can rewrite that field back to the Anthropic route name expected by the client.

For example:

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

Normalization applies to both streaming and non-streaming responses and can be enabled or disabled in Settings.

### Serialized Configuration Writes

Configuration mutations are serialized to prevent concurrent writes from corrupting or reverting settings.

This covers operations such as:

- Model changes
- Thinking-mode changes
- Reasoning-effort changes
- OpenRouter profile changes
- API-key-related configuration changes

### OpenRouter Save Queue

OpenRouter route changes are processed through a dedicated save queue.

The queue provides:

- Serialized save operations
- Superseding of obsolete requests
- Route identity captured when a request is submitted
- Protection against stale React closures
- Protection against rollback from a previously selected route
- Refresh retry after a successful save
- Aggregated gateway restart handling
- Safe processing of requests added during post-save work

This prevents rapid model changes, route switching, or delayed Tauri responses from restoring old UI values.

### Gateway Management

The GUI provides:

- Gateway start and stop controls
- Provider and profile selection
- Route configuration
- API key management
- Log viewing
- Model list refresh
- Save status and error display

The gateway listens on:

```text
http://127.0.0.1:4000
```

## Requirements

- Windows 10 or Windows 11
- Node.js 24 or later for development
- Stable Rust toolchain for development
- An API key for at least one supported provider

A single provider key is sufficient. You do not need keys for every provider.

## Installation

Download the latest Windows installer from the project Releases page and run it.

The installer supports:

- English
- Japanese
- Simplified Chinese
- Traditional Chinese
- Korean
- French
- German
- Spanish

To update Anthro Bridge, run the newer installer. Existing user settings are preserved.

Stable user configuration is stored under:

```text
%APPDATA%\Anthro Bridge\
```

Development builds use a separate application identity and data directory:

```text
%APPDATA%\Anthro Bridge Dev\
```

This allows stable and development versions to coexist without sharing configuration or cache files.

## Quick Start

### 1. Configure an API Key

Open:

```text
Settings > API Key
```

Enter the key for the provider you plan to use and save it.

Common environment variable names are:

| Provider | Environment Variable |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter profiles can use profile-specific key settings managed through the GUI.

### 2. Configure Route Models

Open Settings and select the upstream model for each route:

- Opus
- Sonnet
- Haiku

For OpenRouter, select or create a profile first, then configure each route inside that profile.

### 3. Start the Gateway

Click **Start Gateway**.

Verify that the local endpoint is available:

```text
GET http://127.0.0.1:4000/health
```

### 4. Configure Claude Desktop or Claude Code

Point the client to the Anthro Bridge endpoint while continuing to use Anthropic model names.

Detailed third-party inference instructions are available in:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `GET` | `/health` | Gateway health check |
| `GET` | `/v1/models` | Public route model list |
| `POST` | `/v1/messages` | Streaming and non-streaming Messages API |
| `POST` | `/v1/messages/count_tokens` | Token counting when supported by the selected provider |

## Configuration

The main configuration file is `config.json`.

Most settings should be changed through the GUI. Manual editing is intended for advanced use.

Important model fields include:

| Key | Description |
|---|---|
| `models.<route>.upstream_model` | Upstream model name sent to the provider |
| `models.<route>.thinking_mode` | Route-specific thinking mode |
| `models.<route>.reasoning_effort` | Provider-specific reasoning effort |
| `models.<route>.supports_vision` | Image support override |
| `models.<route>.supports_video` | Video support override |
| `models.<route>.visible` | Whether the route is exposed to clients and the dashboard |
| `non_vision_image_policy` | How unsupported image input is handled |
| `normalize_response_model_identity` | Whether response model names are normalized |

Unsupported images can be handled by one of the following policies:

- `replace`: replace the image with a text placeholder
- `drop`: remove the image content
- `reject`: return an error

## Provider Notes

### DeepSeek

DeepSeek Pro models can use configurable reasoning effort. Flash models do not expose the same reasoning-effort control, so unavailable options are disabled automatically.

### MiniMax

MiniMax model behavior differs by model generation. Anthro Bridge applies the request format required by the selected model, including adaptive or disabled thinking when supported.

### Kimi

Kimi models may use either a thinking parameter or a fixed reasoning-effort mode depending on the model family. Anthro Bridge translates the GUI selection into the appropriate upstream request format.

### MiMo

MiMo uses `thinking_mode` rather than the generic `thinking` field for supported routes.

Vision support varies by model. Anthro Bridge applies the configured unsupported-image policy when a route cannot accept image input.

### OpenRouter

OpenRouter models are grouped by vendor when recognized. The GUI provides:

- Model search
- Vendor grouping
- Custom model entry
- Capability badges
- Pricing display
- Per-model reasoning controls
- Unified model list refresh

OpenRouter model capabilities and behavior can change over time. Live metadata is used where available, while the built-in registry provides stable defaults for known models.

### Poolside Laguna

Laguna S and Laguna XS use OpenRouter reasoning translation rules.

Anthro Bridge also detects a failure pattern in which a response reaches the output token limit after producing only reasoning content and no usable text or tool call. When detected, the event is logged so the user can adjust output limits, disable thinking, or choose another model.

## User Interface

The Settings interface includes:

- Collapsible provider sections
- Opus, Sonnet, and Haiku route configuration
- Model search and vendor grouping for OpenRouter
- Thinking and reasoning controls based on model capability
- Custom upstream model entry
- Automatic route saving
- Explicit API key saving
- Save progress and error messages
- Model pricing and capability information
- Response model normalization toggle

The Dashboard includes:

- Provider or OpenRouter profile selection
- Gateway status
- Current route mappings
- Capability indicators
- Pricing information
- Provider switch status

## Development

### Project Structure

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   └── package.json
└── LICENSE
```

### Run in Development Mode

```bash
cd gui
npm install
npm run tauri dev
```

### Build the Development Variant

On Windows, use a single Rust build job to avoid intermittent compiler termination:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

Development builds use:

- Window title: `Anthro Bridge (DEV)`
- Port: `4000`
- Application identity: `com.soheidon.anthro-bridge.dev`
- Separate configuration and cache directories

### Stable Builds

Stable builds should be created only for release preparation. Normal implementation and verification work should use the development variant.

## Verification

Frontend verification:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rust verification:

```bash
cd gui/src-tauri
cargo check
```

For the OpenRouter route selector specifically:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

The OpenRouter selector tests cover:

- Captured route identity during queued saves
- Cross-route rollback protection
- Stale callback protection
- Refresh retry behavior
- Gateway restart after refresh failure
- In-flight request superseding
- Generation-based rollback suppression

A dedicated multi-save test for restart aggregation may be added to lock down the following behavior:

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## Manual Verification Checklist

Automated tests do not reproduce every Tauri and React timing condition. Before release, verify the following in the development build:

- Each OpenRouter profile shows the correct hover details
- Model selection does not visibly revert after a change
- Thinking and reasoning selections remain stable after saving
- Settings remain correct after closing and reopening the settings screen
- Settings remain correct after restarting the application
- Switching profiles during a save does not corrupt either profile
- A failed save rolls back only the route that initiated it
- Refresh retry success clears the previous error
- Refresh retry failure leaves the latest error visible
- Required gateway restart occurs once after the batch
- Custom models save and reload correctly
- Built-in and live OpenRouter capabilities are displayed correctly

## Troubleshooting

### Port 4000 Is Already in Use

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### A Model Rejects Image or Video Input

Model capabilities vary by provider and route. Check the capability badges in the GUI and select a compatible route.

For unsupported image input, Anthro Bridge follows `non_vision_image_policy`.

### Settings Revert After an Upgrade

Restart the application first so migrations can run.

If the problem persists:

1. Back up the user configuration.
2. Compare it with the bundled configuration.
3. Remove obsolete fields or reset the user configuration if necessary.

Stable configuration location:

```text
%APPDATA%\Anthro Bridge\config.json
```

Development configuration location:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter Model List Is Outdated

Use the unified model refresh control in Settings. Anthro Bridge caches model metadata, so a manual refresh may be needed after OpenRouter changes a model entry.

## Translation

English is the source README.

Translated README files are stored under `docs/`. When the English README changes, regenerate or update the translated files from the English source rather than editing each language independently.

Language files for the application UI are stored under:

```text
gui/src/i18n/lang/
```

## License

MIT License. See [LICENSE](LICENSE).
