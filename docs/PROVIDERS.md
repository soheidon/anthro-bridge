# Provider Details & Model Behaviors

This document details supported model providers, reasoning parameters, and capability translations in Anthro Bridge.

---

## 1. Supported Providers Overview

| Provider | Endpoint | Native / OpenRouter | Reasoning Control |
|---|---|---|---|
| **DeepSeek** | `https://api.deepseek.com/anthropic` | Native | Low / High / Max |
| **MiniMax** | `https://api.minimax.io/anthropic` | Native | Model-specific |
| **Kimi / Moonshot** | `https://api.moonshot.cn/anthropic` | Native | Thinking / Reasoning effort |
| **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | Native | Thinking mode (`thinking` / `normal`) |
| **OpenRouter** | `https://openrouter.ai/api/v1` | Multi-profile Gateway | Vendor / Model specific |

---

## 2. Direct Provider Details

### DeepSeek

- **Supported Models**: `deepseek-v4-pro` (V4-Pro-0813), `deepseek-v4-flash` (V4-Flash-0731).
- **Reasoning Levels**: `Normal` (effort disabled), `Low`, `High`, `Max`.
- **Effort Normalization**: Legacy `medium` and `xhigh` effort values are mapped to `high` using DeepSeek's `output_config.effort` payload format.

### MiniMax

- **Supported Models**: MiniMax M3 and M2.7 variants.
- **Thinking Mode**:
  | Setting | Upstream API parameter | Behavior |
  | ------- | --------------------- | -------- |
  | Thinking ON | `thinking: {"type": "adaptive"}` | Enables extended thinking |
  | Thinking OFF | `thinking: {"type": "disabled"}` | Disables extended thinking |
  | Default (unset) | *(omitted)* | API defaults to thinking disabled |
- MiniMax-M2.x models (`MiniMax-M2.7-highspeed`) permanently operate in Thinking-only mode.

### Kimi (Moonshot)

- **Supported Models**: Kimi K2.x and Kimi K3.
- Translates thinking parameters and fixed reasoning effort modes into the expected upstream payload structure.

### MiMo (Xiaomi)

- **Supported Models**: MiMo V2.5 and V2.5 Pro variants.
- Uses `thinking_mode` rather than the generic `thinking` boolean.
- Follows the configured `non_vision_image_policy` if an image is sent to a non-vision model.

---

## 3. OpenRouter Built-in Vendor Groups

OpenRouter profiles can access any model on OpenRouter, with specialized built-in handling for:

- **Poolside**: Laguna S 2.1 / Laguna XS 2.1 (Thinking controls).
  - Defaults to "Normal" (thinking off) on Opus routes to prevent excessive reasoning token consumption.
  - Detects reasoning token-cap limits and logs warnings when per-turn token ceilings are reached without text output.
- **Tencent**: Hy3 (Low / High reasoning effort).
- **InclusionAI**: Ring and Ling model families (Thinking and reasoning controls).
- **StepFun**: Step 3.5 and Step 3.7 (Low / Medium / High reasoning effort).
- **OpenAI**: GPT-5.6 Sol / Terra / Luna and Pro variants (Thinking and reasoning controls).
- **Google**: Gemini 3.1 Pro Preview and Gemini 3.7 Flash.

---

## 4. Response Model Normalization

Upstream APIs often return their native model name in JSON responses. When enabled, Anthro Bridge rewrites the response model field back to the Anthropic route requested by the client (`claude-opus-5`, `claude-sonnet-5`, `claude-haiku-4-5`). This ensures seamless compatibility with Claude Desktop and Claude Code.
