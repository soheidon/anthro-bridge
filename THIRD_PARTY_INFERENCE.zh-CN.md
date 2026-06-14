[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md)

# 在 Claude Desktop / Cowork on 3P 中使用 Anthro Bridge

Anthro Bridge 可作为 Claude Desktop / Cowork on 3P 的本地
Anthropic 兼容网关使用。

Claude Desktop / Cowork on 3P 通过应用内配置窗口支持第三方推理。

官方文档：

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. 启动 Anthro Bridge

首先启动 Anthro Bridge，确保网关正在运行。

默认情况下，Anthro Bridge 监听：

```text
http://127.0.0.1:4000
```

在使用 Claude Desktop / Cowork on 3P 期间，请保持 Anthro Bridge 运行。

## 2. 在 Claude Desktop 中启用开发者模式

打开 Claude Desktop。

在 Windows 上，点击左上角的应用菜单。

然后选择：

```text
Help → Troubleshooting → Enable Developer Mode
```

启用开发者模式后，会出现一个新的 `Developer` 菜单。

## 3. 打开第三方推理设置

打开：

```text
Developer → Configure third-party inference
```

这将打开第三方推理配置窗口。

## 4. 配置 Connection

在 `Connection` 部分中，选择：

```text
Gateway
```

然后输入以下值。

| 字段                    | 值                        |
| --------------------- | ------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000` |
| Gateway API key       | `sk-local-gateway`       |
| Gateway auth scheme   | `bearer`                 |
| Gateway extra headers | 通常留空                      |

`Gateway API key` 必须与 Anthro Bridge 中配置的本地 API 密钥一致。

## 5. 配置 Identity & Models

在 `Identity & Models` 部分中，添加要在模型选择器中显示的模型 ID。

示例：

```text
claude-sonnet-4-6
claude-haiku-4-5
```

您还可以为每个模型设置显示名称。

示例：

| Model ID            | 显示名称          |
| ------------------- | ------------- |
| `claude-sonnet-4-6` | `Gateway Pro` |
| `claude-haiku-4-5`  | `Gateway Flash` |

列表中的第一个模型将作为默认选择项。

展开每个模型行，确认 `Model ID` 与您希望发送到 Anthro Bridge 的模型名称完全一致。

仅当上游提供商和所选模型确实支持扩展上下文窗口时，才启用 `Offer 1M-context variant`。

## 6. 配置示例

以上设置对应以下第三方推理配置：

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

## 7. 应用并重启 Claude Desktop

配置好网关和模型列表后，在本地应用设置。

如有提示，请重启 Claude Desktop。

重启后，Cowork on 3P 的请求将发送到 Anthro Bridge。Anthro Bridge 随后将请求路由到您在 Anthro Bridge 中配置的上游提供商。

## 注意事项

Anthro Bridge 是一个非官方的 Anthropic 兼容本地网关。

与 Anthropic、Moon Bridge 或任何上游模型提供商无关联。

Claude Desktop / Cowork on 3P 通过 Claude 的第三方推理设置进行配置。菜单标签和配置字段可能随 Anthropic 更新 Claude Desktop 而发生变化。
