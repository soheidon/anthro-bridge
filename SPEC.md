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
(DeepSeek / MiniMax / Kimi / MiMo)
```

#### Design Principles

- **Shell model + provider selection**: Claude Desktop always sees `claude-opus-4-8` / `claude-sonnet-5` / `claude-haiku-4-5`. The actual LLM is selected in the GUI (DeepSeek / MiniMax / Kimi / MiMo). The active provider's model mapping is used for routing.
- **Only active provider needs API key**: Since v0.5.0, only providers referenced by the route table are checked at startup. Non-active provider keys are not required.
- **Thin proxy**: Nothing modified except the `model` field. SSE forwarded byte-for-byte.
- **Lossless forwarding**: Message bodies, tool calls, thinking blocks pass through unmodified.
- **Windows-native GUI**: Tauri v2 + React 19 + TypeScript. Rust backend, Vite + React 19 frontend.
- **Zero external dependencies**: Proxy embedded in Tauri binary since v0.3.0. Python not required.
- **Multi-language**: 8 languages (en, ja, zh-CN, zh-TW, ko, fr, de, es). Add new languages by dropping files into `lang/`. First-run language picker.
- **Reasoning effort**: DeepSeek Pro models support configurable reasoning effort (high / medium / low / max). Flash models automatically disable reasoning effort in the GUI.
- **Peak/valley pricing awareness**: DeepSeek peak time ranges shown in local timezone with color-coded PEAK badge (pink).
- **UTC offset display**: Timezone selector shows dynamic UTC offsets (e.g. UTC+09:00) next to each option.

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

#### Media Check / Image Sanitization

Per-model `supports_vision` / `supports_video` flags determine behavior. For non-vision models receiving images, `non_vision_image_policy` applies:
- `replace` (default): Replace image blocks with placeholder text
- `drop`: Remove image blocks (insert placeholder if content becomes empty)
- `reject`: Return 400 error

Video blocks always return 400. `non_vision_image_policy` is visible via `/health`.

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
    }
  },
  "non_vision_image_policy": "replace",
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
}
```
