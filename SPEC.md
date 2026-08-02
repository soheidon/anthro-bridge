[English](SPEC.md) | [日本語](docs/SPEC.ja.md) | [中文(简体)](docs/SPEC.zh-CN.md) | [中文(繁體)](docs/SPEC.zh-TW.md) | [한국어](docs/SPEC.ko.md) | [Français](docs/SPEC.fr.md) | [Deutsch](docs/SPEC.de.md) | [Español](docs/SPEC.es.md)

# SPEC: Anthro Bridge

## Overview

A thin proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

### Architecture

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Embedded in Tauri app (axum 0.7 + reqwest)
       |
       | Routes by model field -> resolves correct upstream provider
       | Rewrites only model to upstream name
       | Injects thinking disabled for non-thinking variants
       | Per-model media support checking
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### Design Principles

- **Shell model + provider selection**: Claude Desktop always sees `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`. The actual LLM is selected in the GUI (DeepSeek / MiniMax / Kimi / MiMo / OpenRouter). The active provider's model mapping is used for routing.
- **OpenRouter support**: Routes to OpenRouter's Anthropic-compatible endpoint with Poolside Laguna S/XS defaults. Dedicated thinking mode controls (Max/On/Off) translated to OpenRouter's `reasoning` format at request time.
- **Only active provider needs API key**: Since v0.5.0, only providers referenced by the route table are checked at startup. Non-active provider keys are not required.
- **Thin proxy**: Nothing modified except the `model` field. SSE forwarded byte-for-byte.
- **Lossless forwarding**: Message bodies, tool calls, thinking blocks pass through unmodified.
- **Windows-native GUI**: Tauri v2 + React 19 + TypeScript. Rust backend, Vite + React 19 frontend.
- **Zero external dependencies**: Proxy embedded in Tauri binary since v0.3.0. Python not required.
- **Multi-language**: 8 languages (en, ja, zh-CN, zh-TW, ko, fr, de, es). Add new languages by dropping files into `lang/`. First-run language picker.
- **Reasoning effort**: DeepSeek V4 Pro supports reasoning effort High / Max in Thinking mode; V4 Flash supports Low / High / Max. Reasoning effort is disabled in Normal mode. A legacy `low`/`medium` effort stored for a V4 Pro route is migrated to `high` on startup.
- **Capability detection**: Live capability flags (supports_image_url, supports_image_base64, supports_video_url, supports_video_base64) fetched from OpenRouter API and persisted to config.json.
- **Peak/valley pricing awareness**: DeepSeek and OpenRouter peak time ranges shown in local timezone.
- **MiniMax-M3 thinking toggle**: MiniMax-M3 supports Thinking ON/OFF via Anthropic-compatible API (`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`). M2.x models remain thinking-only. Startup migration converts legacy `thinking_only` → `thinking` for existing users.
- **Response Model Identity Normalization**: Rewrites upstream model names in API responses (both SSE streaming and non-streaming) back to Anthropic official model names. Controlled by `normalize_response_model_identity` in config.json and a runtime `AtomicBool`. Independent save command (`update_normalize_model_identity`) to avoid cross-contamination with server config saves.
- **Structured communication logging**: `tracing` + `tracing-appender` writes structured logs to `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`. Each request gets a correlation ID from an `AtomicU64` counter. Log entries include request model, gateway model, upstream model, normalization outcome, and skip reasons. No sensitive data (prompts, bodies, API keys) is logged.
- **PEAK badge**: Color-coded pink badge in the dashboard for peak-priced models.
- **UTC offset display**: Timezone selector shows dynamic UTC offsets (e.g. UTC+09:00) next to each option.
- **Laguna S/XS 2.1 token-cap failure detection**: Detects reasoning-only responses with `stop_reason: "max_tokens"` in both SSE streams and non-stream responses. Logs a warning when the per-turn token limit is hit without producing usable text or tool calls. Available for all Poolside Laguna models via OpenRouter.
- **Poolside thinking:disabled passthrough**: Translates client-sent `thinking: { type: "disabled" }` to OpenRouter's `reasoning: { enabled: false }` format for Poolside models, ensuring disabled thinking is correctly forwarded even without a saved config setting.
- **Laguna Opus default migration**: One-time idempotent migration changes `claude-opus-5` default from thinking-on to normal mode for `poolside/laguna-s-2.1` OpenRouter users. New install template reflects the updated default.
- **OpenRouter multi-profile**: Multiple OpenRouter profiles per user, each with its own API key and model configuration. Profile CRUD via Tauri commands. Active profile switching from dashboard or settings. Profiles can be reordered by drag and drop, hidden, and persisted in the configured order.
- **OpenRouter dashboard cards**: The dashboard creates one card per visible OpenRouter profile, with a fallback card when profiles are absent. Model summaries hide the vendor namespace before the first `/` for OpenRouter display only; full upstream IDs remain unchanged for routing.
- **OpenRouter model registry**: Local built-in registry of known OpenRouter models (`model_capabilities.rs`, `builtinOpenRouter.ts`) with pre-configured capabilities (vision, video, thinking policy, reasoning effort), vendor grouping, and pricing data. Used for model classification without live API calls.
- **OpenRouter pricing details**: Built-in pricing supports current and revised-standard values for prompt, output, and cached-input rates, including GPT-5.6 Sol, Terra, Luna, and Pro variants. The GUI displays promotional and standard rates together when both are available.
- **GPT-5.6 model support**: OpenRouter profiles can use Sol, Terra, and Luna model variants, with capability-aware thinking controls and pricing notes for long-context rates where applicable. The built-in OpenAI GPT-5.6 Balanced profile routes Opus 5 → GPT-5.6 Sol, Sonnet 5 → GPT-5.6 Terra, and Haiku 4.5 → GPT-5.6 Luna with Thinking High reasoning effort on all three routes for new installations; existing saved routing is not changed automatically.
- **Dashboard-driven window sizing**: Initial and row-count changes calculate the window height from visible dashboard cards in a three-column grid. The calculation accounts for card height, grid gaps, native minimum size, monitor work area, DPI scaling, and window decorations while preserving manual resizing when the row count is unchanged.
- **Localized NSIS installer**: The Windows installer exposes English, Japanese, Simplified Chinese, Traditional Chinese, Korean, French, German, and Spanish language choices and bundles the Anthro Bridge application icon.
- **Regression coverage**: Vitest coverage includes OpenRouter profile ordering and save races, production pricing data, dashboard card-count semantics, and monitor-aware window sizing.
- **New providers via OpenRouter**: InclusionAI and StepFun added as OpenRouter model providers with dedicated capability flags, thinking mode controls, and vendor grouping.
- **Tencent Hy3 thinking modes**: Low/High reasoning effort support for Tencent's Hunyuan model. Thinking mode translation in proxy.rs maps `thinking_mode` to OpenRouter's `reasoning` format. UI displays Low/High as dropdown options.
- **Kimi K3 fixes**: Removed hard-coded `forced_reasoning_effort` from capability definitions. Replaced fixed "Max" display with configurable dropdown selector. Default values from saved config, falling back to "max".
- **Config write serialization**: All config-writing Tauri commands serialize through `execute_serialized_config_mutation` with a `Mutex` guard. `ConfigState` struct provides `applied_config`, `in_flight_config`, and `pending_ops` tracking with validation. Prevents race conditions when multiple settings changes are saved concurrently.
- **OpenRouter UI race fixes**: (1) `syncUiFromSavedRouteRef` latest-callback ref prevents stale closure from overwriting new route's UI. (2) `rollbackRouteId` guard prevents cross-route Phase 2 rollback. (3) `useRouteSaveGeneration` hook provides `begin()`/`isCurrent()` generation guards for all handlers. (4) Save queue hook (`useOpenRouterSaveQueue`) with drain loop, supersede detection, and restart OR-aggregation.
- **Dev/stable app identity isolation**: `AppChannel` enum (`Stable`/`Dev`) in `paths.rs` selects separate identifier (`com.soheidon.anthro-bridge` vs `.dev`), config directory (`Anthro Bridge` vs `Anthro Bridge Dev`), and cache paths. Dev channel uses `tauri.dev.conf.json`. NPM scripts: `npm run dev` (dev), `npm run dev:stable` (stable).
- **Config template embedding**: `include_str!()` embeds `config_template.rs` at compile time, removing runtime dependency on bundled `config.json`. `merge_bundled_providers` returns `Result` with typed error handling.
- **Frontend regression tests**: 7 vitest regression tests for OpenRouter save race conditions using `QueueHarness` and `GenerationHandlerHarness`. Tests cover: latest-callback ref, cross-route rollback guard, identity capture, refresh retry (fail + success paths), in-flight supersede, and generation guard.
- **Claude Code context management**: Model-aware auto-compaction for Claude Code. `resolve_effective_auto_compact` resolves each standard route (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) to its upstream model, looks up each model's context capacity in the static `model_context_windows.json` registry, and in Auto mode uses the smallest known capacity as a safe context window. Context control applies only when all three capacities are known (otherwise status is Incomplete). A header toggle switches context management on/off; advanced modes and thresholds are set in `config.json` under `claude_code.auto_compact`. Modes: `auto`, `manual` (`window_tokens`), `claude_default`.
- **Claude Code launch command generation**: `build_claude_code_launch_command` renders a complete PowerShell command combining gateway connection variables (`ANTHROPIC_BASE_URL` pointing at the local gateway, `ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`) with Claude Code context control variables (`CLAUDE_CODE_AUTO_COMPACT_WINDOW`, `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`). When context management is disabled, incomplete, or set to Claude default, the command removes stale context variables with `Remove-Item Env:... -ErrorAction SilentlyContinue` so previously set session values do not leak into a new launch. The "Copy Claude Code launch command" button in the Claude settings panel copies the command to the clipboard. Anthro Bridge only generates and copies the command — it never executes it.
- **Shared model routing module**: `model_routing.rs` extracts route-to-upstream resolution into pure functions shared by `proxy.rs` and the context resolver, guaranteeing context windows resolve the same upstream models the proxy actually forwards to.
- **Context capacity registry**: `model_context_windows.json` is a static registry of known context capacities covering built-in direct-provider models (DeepSeek, MiniMax, Kimi, MiMo) and built-in OpenRouter models (Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6). Unknown custom OpenRouter models remain valid route targets but report context management as Incomplete until metadata is added or manual mode is configured.

### GUI Management Tool

Tauri v2 + React 19 + TypeScript. Two-panel layout: Dashboard + Settings.

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]          ||
|  +- Status ------------------------------+
|  | Port 4000 | API Key | Gateway URL    ||
|  | Model routing table                  ||
|  +- Latest Log --------------------------+
|  | Log viewer with Pro/Flash counters   ||
|  +---------------------------------------+
+------------------------------------------+

Settings (=):
  +- Language ----------------------------+
  | Dropdown for instant switching        |
  +- API Key -----------------------------+
  | Per-provider API key management       |
  +- Claude Desktop Setup ----------------+
  | Config JSON generation, copy,         |
  | config file detection                 |
  +- Gateway Config ----------------------+
  | config.json editor (advanced)         |
  +---------------------------------------+
```

### Tauri Commands

| # | Command | Type | Description |
|---|---------|------|-------------|
| 1 | `check_health` | async | Proxy health check |
| 2 | `check_gateway_status` | sync | Port 4000 + tokio task liveness |
| 3 | `check_api_key` | sync | Active provider API key status |
| 4 | `set_env_api_key` | sync | Persist API key via setx |
| 5 | `get_port_4000_process` | sync | Get PID of port 4000 via netstat |
| 6 | `read_config` | sync | Read config.json |
| 7 | `read_config_raw` | sync | Raw config.json text + encoding detect |
| 8 | `write_config` | sync | Save config.json (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | Read latest log |
| 10 | `read_log` | sync | Read specified log file |
| 11 | `list_logs` | sync | List log files |
| 12 | `create_new_log` | sync | Create new log file |
| 13 | `open_logs_folder` | sync | Open logs folder |
| 14 | `open_path` | sync | Open arbitrary path |
| 15 | `find_claude_configs` | sync | Auto-detect Claude Desktop config files |
| 16 | `start_proxy` | sync | Start proxy (resolve config -> spawn -> verify port) |
| 17 | `stop_proxy` | sync | Stop proxy (graceful shutdown) |
| 18 | `proxy_status` | sync | Check task liveness |
| 19 | `check_all_api_keys` | sync | All provider API key status |
| 20 | `update_active_provider` | sync | Save active_provider |
| 21 | `update_provider_api_key_env` | sync | Save provider api_key_env |
| 22 | `get_user_language` | sync | Get saved language preference |
| 23 | `set_user_language` | sync | Save language preference |
| 24 | `is_first_run` | sync | Determine first run (user_prefs.json existence) |
| 25 | `openrouter_get_models` | async | Fetch/cache OpenRouter model catalog |
| 26 | `set_model_upstream` | sync | Save upstream model + thinking config + capability flags for a gateway model |
| 27 | `update_server_config` | sync | Save server host/port/CORS settings |
| 28 | `update_normalize_model_identity` | sync | Save response model identity normalization toggle (updates config + runtime AtomicBool) |
| 29 | `update_claude_code_auto_compact_global` | sync | Toggle global Claude Code context management (enabled + trigger percent) |
| 30 | `update_claude_code_auto_compact_target` | sync | Set per-provider/profile context mode (auto / manual / claude_default) + manual window tokens |
| 31 | `update_claude_code_context_settings` | sync | Combined atomic update of global + target context settings |
| 32 | `resolve_claude_code_auto_compact` | sync | Resolve effective context settings (mode, window tokens, trigger percent, status) |
| 33 | `build_claude_code_launch_command` | sync | Generate complete PowerShell Claude Code launch command (gateway + context env vars) |

### Proxy Server (proxy.rs)

Ported from Python to Rust (axum 0.7/reqwest) in v0.3.0.

#### Endpoints

| Method | Path | Behavior |
|--------|------|----------|
| GET | `/health` | Health check |
| GET | `/v1/models` | Public model list (`visible: true` only) |
| POST | `/v1/messages` | Model resolve -> thinking injection -> media check -> forward (stream/non-stream) |
| POST | `/v1/messages/count_tokens` | Forward to upstream if supported |

#### Model Routing

Builds a reverse lookup table from gateway model -> (provider, upstream model) using each provider's `models` section. Since all providers use the same gateway model names, `active_provider` wins on collision. Effectively, only the active provider's models end up in the route table.

#### API Key Validation (since v0.5.0)

Pass 1: Build model route table (no API keys needed)
Pass 2: Only check API keys for providers referenced by the route table

#### Thinking Injection

For models with `thinking: "disabled"` in their config entry, injects `{"type": "disabled"}` only when the user has not explicitly set thinking.

#### Response Model Normalization

When `normalize_response_model_identity` is enabled, the proxy rewrites the `model` field in upstream responses:

- **Non-streaming**: Parses JSON response, rewrites `model` to the Anthropic canonical name, re-serializes
- **Streaming (SSE)**: Intercepts `message_start` event frames, rewrites `model` in-place using byte-range replacement to preserve SSE formatting and whitespace
- **Skip reasons**: `disabled` (toggle off), `non_success_status` (non-200 response), `content_encoding_not_transformable` (gzip/brotli), `stream_error`, `stream_cancelled`
- **Decision logic**: Pure functions (`should_normalize_nonstream`, `nonstream_skip_reason`) used by both production code and tests

#### Media Check / Image Sanitization

Per-model `supports_vision` / `supports_video` flags determine behavior. For non-vision models receiving images, `non_vision_image_policy` applies:
- `replace` (default): Replace image blocks with placeholder text
- `drop`: Remove image blocks (insert placeholder if content becomes empty)
- `reject`: Return 400 error

Video blocks always return 400. `non_vision_image_policy` is visible via `/health`.

#### Claude Code Context Management

Claude Code context control uses two official environment variables:

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

Resolver pipeline:

1. Resolve each standard route (claude-opus-5, claude-sonnet-5, claude-haiku-4-5) to its upstream model
2. Look up each upstream model's context capacity in `model_context_windows.json`
3. Require all three capacities to be known
4. Use the smallest known capacity as the safe context window
5. Apply the configured trigger percent

Modes: `auto` (smallest known capacity), `manual` (`window_tokens`), `claude_default` (Claude Code's own default; no variables set). Effective status is `applied`, `disabled`, or `incomplete`.

The launch command combines gateway connection variables with the context variables:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

When context control is not applied, the command removes stale variables first:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

The percent override only pushes compaction earlier; values that would delay compaction past Claude Code's default may be ignored. Anthro Bridge generates and copies the command only — it never executes it, and this does not prove that a specific Claude Code version honors the variables (final confirmation requires Claude Code diagnostics or observed compaction behavior).

### Multi-language

File-per-language architecture with `import.meta.glob` auto-discovery:

```
gui/src/i18n/lang/
  en.ts      English (canonical — defines TranslationKey type)
  ja.ts      Japanese
  zh-CN.ts   Chinese Simplified
  zh-TW.ts   Chinese Traditional
  ko.ts      Korean
  fr.ts      French
  de.ts      German
  es.ts      Spanish
```

To add a language: copy `en.ts`, translate, rebuild. No code changes needed.

### config.json Reference

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key env var name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "real-model-name" },
      "visible_models": ["claude-public-model-name"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "real-model-name",
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

Each provider or OpenRouter profile may also set a default context mode via `claude_code: { "auto_compact": { "mode": "auto" } }`. The effective mode for a route is the provider/profile value, falling back to the global block; `resolve_claude_code_auto_compact` returns the resolved result.
