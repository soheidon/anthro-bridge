# Configuration Reference

This document describes the structure and settings available in Anthro Bridge's configuration (`config.json`).

Configuration files are stored under:
- **Stable**: `%APPDATA%\Anthro Bridge\config.json`
- **Development**: `%APPDATA%\Anthro Bridge Dev\config.json`

---

## 1. Top-Level Structure

```json
{
  "active_provider": "deepseek",
  "active_openrouter_profile": "openrouter_default",
  "providers": {
    "deepseek": { ... },
    "minimax": { ... },
    "kimi": { ... },
    "mimo": { ... },
    "openrouter": { ... }
  },
  "claude_code": {
    "auto_compact": { ... }
  },
  "mcp": {
    "provider": "deepseek",
    "profile_id": "",
    "model": "deepseek-v4-pro",
    "thinking_mode": "thinking",
    "reasoning_effort": "high",
    "targets": { ... }
  },
  "timezone": "Asia/Tokyo",
  "normalize_response_model": true,
  "non_vision_image_policy": "fallback_to_text"
}
```

---

## 2. Provider and Route Configuration

Each provider configuration maps Anthropic route names (`claude-opus-5`, `claude-sonnet-5`, `claude-haiku-4-5`) to provider-specific upstream models and reasoning parameters.

### Canonical Routes

- `opus`: Mapped to heavy reasoning / high-intelligence upstream models.
- `sonnet`: Mapped to balanced general-purpose coding models.
- `haiku`: Mapped to fast, lightweight auxiliary models.

### Reasoning and Thinking Fields

- `thinking`: Boolean flag or provider-specific parameter.
- `thinking_mode`: Used by providers like MiMo (`thinking`, `normal`).
- `reasoning_effort`: Controls effort levels (`low`, `medium`, `high`, `xhigh`, `max`).

---

## 3. OpenRouter Profiles

OpenRouter supports multiple profiles, each with its own independent configuration:

```json
{
  "profiles": [
    {
      "id": "openrouter_default",
      "display_name": "GPT-5.6 Balanced",
      "models": {
        "opus": "openai/gpt-5.6-sol",
        "sonnet": "openai/gpt-5.6-terra",
        "haiku": "openai/gpt-5.6-luna"
      },
      "thinking_modes": {
        "opus": "thinking",
        "sonnet": "thinking",
        "haiku": "thinking"
      },
      "reasoning_efforts": {
        "opus": "high",
        "sonnet": "high",
        "haiku": "high"
      },
      "hidden": false
    }
  ]
}
```

---

## 4. Claude Code Context Management

Anthro Bridge can manage context capacity to trigger proactive compaction in Claude Code.

### Modes

1. **Auto (`mode: "auto"`)**: Automatically detects the minimum context window among the three canonical routes and applies an override percentage.
```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "auto",
      "trigger_percent": 90
    }
  }
}
```

2. **Manual (`mode: "manual"`)**: Explicit token window and trigger percentage.
```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "manual",
      "window_tokens": 240000,
      "trigger_percent": 90
    }
  }
}
```

3. **Claude Default (`mode: "claude_default"`)**: Disables context variable injection and leaves compaction to Claude Code.

---

## 5. MCP Planning Configuration

The `mcp` section configures the model used when external agents call `anthro-bridge/plan`:

```json
{
  "mcp": {
    "provider": "deepseek",
    "profile_id": "",
    "model": "deepseek-v4-pro",
    "thinking_mode": "thinking",
    "reasoning_effort": "high",
    "targets": {
      "deepseek": {
        "model": "deepseek-v4-pro",
        "thinking_mode": "thinking",
        "reasoning_effort": "high"
      },
      "openrouter:gemini": {
        "model": "google/gemini-3.7-flash",
        "thinking_mode": "normal",
        "reasoning_effort": "high"
      }
    }
  }
}
```

- Target configurations are stored per-provider/profile in `targets`.
- Switching the active target in the GUI automatically mirrors its values to top-level fields for backwards compatibility.
