[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# Using Anthro Bridge with Claude Desktop / Cowork on 3P

Anthro Bridge can be used as a local Anthropic-compatible gateway for
Claude Desktop / Cowork on 3P.

Claude Desktop / Cowork on 3P supports third-party inference through the
in-app configuration window.

Official documentation:

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Start Anthro Bridge

Start Anthro Bridge first and make sure the gateway is running.

By default, Anthro Bridge listens on:

```text
http://127.0.0.1:4000
```

Keep Anthro Bridge running while using Claude Desktop / Cowork on 3P.

## 2. Enable Developer Mode in Claude Desktop

Open Claude Desktop.

On Windows, open the application menu in the upper-left corner.

Then select:

```text
Help → Troubleshooting → Enable Developer Mode
```

After Developer Mode is enabled, a new `Developer` menu will appear.

## 3. Open third-party inference settings

Open:

```text
Developer → Configure third-party inference
```

This opens the third-party inference configuration window.

## 4. Configure Connection

In the `Connection` section, select:

```text
Gateway
```

Then enter the following values.

| Field                 | Value                                      |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | Leave blank unless you need custom headers |

The `Gateway API key` must match the local API key configured in Anthro Bridge.

## 5. Configure Identity & Models

In the `Identity & Models` section, add the model IDs that Claude Desktop should show in the model picker.

Example:

```text
claude-opus-5
claude-sonnet-5
claude-haiku-4-5
```

You can also give each model a display label.

Example:

| Model ID            | Display label   |
| ------------------- | --------------- |
| `claude-opus-5`  | `Gateway Opus`  |
| `claude-sonnet-5`  | `Gateway Pro`   |
| `claude-haiku-4-5` | `Gateway Flash` |

The first model in the list is used as the default picker entry.

For each model, expand the row and confirm that `Model ID` is exactly the model name you want Claude Desktop to send to Anthro Bridge.

Only enable `Offer 1M-context variant` if your upstream provider and selected model actually support the extended context window.

## 6. Example configuration

The above settings correspond to the following third-party inference configuration:

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-opus-5",
      "labelOverride": "Gateway Opus"
    },
    {
      "name": "claude-sonnet-5",
      "labelOverride": "Gateway Pro"
    },
    {
      "name": "claude-haiku-4-5",
      "labelOverride": "Gateway Flash"
    }
  ]
}
```

## 7. Apply and restart Claude Desktop

After configuring the gateway and model list, apply the settings locally.

Restart Claude Desktop if prompted.

Once Claude Desktop restarts, requests from Cowork on 3P should be sent to Anthro Bridge. Anthro Bridge then routes the requests to the upstream provider configured in Anthro Bridge.

## MiniMax-M3 Thinking Mode

MiniMax-M3 supports toggling the thinking (extended thinking) feature on and off through the Anthropic-compatible API.

| Setting | Upstream API parameter | Behavior |
| ------- | --------------------- | -------- |
| Thinking ON | `thinking: {"type": "adaptive"}` | Enables extended thinking |
| Thinking OFF | `thinking: {"type": "disabled"}` | Disables extended thinking |
| Default (unset) | *(omitted)* | API defaults to thinking disabled |

In the Anthro Bridge Settings UI, MiniMax-M3 rows display a Thinking / Normal toggle.

MiniMax-M2.x models (`MiniMax-M2.7-highspeed`) do not support disabling thinking and are permanently set to "Thinking-only" mode.

## Laguna S/XS 2.1 (OpenRouter) Configuration

Anthro Bridge routes OpenRouter's `claude-opus-5` and `claude-sonnet-5` to `poolside/laguna-s-2.1` and `claude-haiku-4-5` to `poolside/laguna-xs-2.1` by default.

### Thinking behavior

Third-party testing has observed that thinking behavior with Laguna S 2.1 can be sensitive to system prompt structure. A clear, professional persona with explicit acceptance criteria has been shown in those tests to reduce unnecessary reasoning substantially.

Laguna Opus (`claude-opus-5`) now defaults to "Normal" (thinking off) mode based on these findings. You can re-enable thinking in the Settings UI if desired.

### Known limitation: token-cap silence

Under certain conditions — particularly when the per-turn token limit is reached while the model is still in its reasoning phase — Laguna S 2.1 may return a response that contains only reasoning content with no text or tool-use output and a `stop_reason` of `max_tokens`. Clients receiving such a response may be unable to continue the conversation.

Anthro Bridge detects this condition and logs a warning to the log panel. If you encounter repeated "Reasoning-only response reached the per-turn token limit" warnings, consider raising the max output tokens or switching thinking off for the affected model.

These observations come from third-party community testing. Results may vary by provider configuration and model revision.

## Notes

Anthro Bridge is an unofficial Anthropic-compatible local gateway.

It is not affiliated with Anthropic, Moon Bridge, or any upstream model provider.

Claude Desktop / Cowork on 3P is configured through Claude's third-party inference settings. Menu labels and configuration fields may change as Anthropic updates Claude Desktop.
