[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md)

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
claude-sonnet-4-6
claude-haiku-4-5
```

You can also give each model a display label.

Example:

| Model ID            | Display label   |
| ------------------- | --------------- |
| `claude-sonnet-4-6` | `Gateway Pro`   |
| `claude-haiku-4-5`  | `Gateway Flash` |

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
      "name": "claude-sonnet-4-6",
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

## Notes

Anthro Bridge is an unofficial Anthropic-compatible local gateway.

It is not affiliated with Anthropic, Moon Bridge, or any upstream model provider.

Claude Desktop / Cowork on 3P is configured through Claude's third-party inference settings. Menu labels and configuration fields may change as Anthropic updates Claude Desktop.
