[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md) | [中文(繁體)](THIRD_PARTY_INFERENCE.zh-TW.md) | [한국어](THIRD_PARTY_INFERENCE.ko.md) | [Français](THIRD_PARTY_INFERENCE.fr.md) | [Deutsch](THIRD_PARTY_INFERENCE.de.md) | [Español](THIRD_PARTY_INFERENCE.es.md)

# 使用 Anthro Bridge 搭配 Claude Desktop / Cowork on 3P

Anthro Bridge 可作為 Claude Desktop / Cowork on 3P 的本地 Anthropic 相容閘道使用。

Claude Desktop / Cowork on 3P 透過應用內配置視窗支援第三方推理。

官方文件：

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. 啟動 Anthro Bridge

先啟動 Anthro Bridge 並確保閘道正在運行。

Anthro Bridge 預設監聽：

```text
http://127.0.0.1:4000
```

使用 Claude Desktop / Cowork on 3P 時保持 Anthro Bridge 運行。

## 2. 在 Claude Desktop 中啟用開發者模式

開啟 Claude Desktop。

在 Windows 上，開啟左上角的應用選單。

然後選擇：

```text
Help → Troubleshooting → Enable Developer Mode
```

啟用開發者模式後，會出現新的 `Developer` 選單。

## 3. 開啟第三方推理設定

開啟：

```text
Developer → Configure third-party inference
```

這會開啟第三方推理配置視窗。

## 4. 配置連線

在 `Connection` 區段中選擇：

```text
Gateway
```

然後輸入以下值。

| 欄位                  | 值                                         |
| --------------------- | ------------------------------------------ |
| Gateway base URL      | `http://127.0.0.1:4000`                    |
| Gateway API key       | `sk-local-gateway`                         |
| Gateway auth scheme   | `bearer`                                   |
| Gateway extra headers | 留空，除非需要自訂標頭 |

`Gateway API key` 必須與 Anthro Bridge 中配置的本地 API 金鑰一致。

## 5. 配置身份與模型

在 `Identity & Models` 區段中，新增 Claude Desktop 應在模型選擇器中顯示的模型 ID。

範例：

```text
claude-sonnet-4-6
claude-haiku-4-5
```

您也可以為每個模型指定顯示標籤。

範例：

| 模型 ID             | 顯示標籤         |
| -------------------- | --------------- |
| `claude-sonnet-4-6`  | `Gateway Pro`   |
| `claude-haiku-4-5`   | `Gateway Flash` |

清單中的第一個模型將作為選擇器的預設項目。

對每個模型，展開列並確認 `Model ID` 正是您希望 Claude Desktop 發送給 Anthro Bridge 的模型名稱。

僅在您的上游提供者和選定模型確實支援擴展上下文視窗時，才啟用 `Offer 1M-context variant`。

## 6. 配置範例

以上設定對應以下第三方推理配置：

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

## 7. 套用並重新啟動 Claude Desktop

配置閘道和模型清單後，在本地套用設定。

如有提示，重新啟動 Claude Desktop。

Claude Desktop 重新啟動後，來自 Cowork on 3P 的請求將被發送到 Anthro Bridge。Anthro Bridge 然後將請求路由到在 Anthro Bridge 中配置的上游提供者。

## Xiaomi MiMo 供應商

當 Anthro Bridge 設定為使用 Direct MiMo 供應商時，來自 Claude Desktop 的推理請求將被路由至小米的 MiMo API。

### MiMo-V2.6 模型

| Model ID | 顯示名稱 | 多模態 |
| --- | --- | --- |
| `mimo-v2.6-flash` | MiMo-V2.6-Flash | Image + Video |
| `mimo-v2.6-pro` | MiMo-V2.6-Pro | Image + Video |
| `mimo-v2.6-pro-ultraspeed` | MiMo-V2.6-Pro-UltraSpeed | Image + Video |

所有 V2.6 模型支援 1,000,000 token 的上下文視窗。

### 普通 / 思考

MiMo 使用簡單的普通／思考切換開關，沒有推理努力等級（Low / Medium / High）。

| 設定 | 上游 API 參數 | 行為 |
| --- | --- | --- |
| 普通 | `thinking: {"type": "disabled"}` | 標準推理 |
| 思考 | `thinking: {"type": "enabled"}` | 延伸思考 |

### 預設路由

Direct MiMo 預設的內建路由：

| 閘道模型 | 上游模型 | 思考 |
| --- | --- | --- |
| `claude-opus-5` | `mimo-v2.6-pro` | 思考 |
| `claude-sonnet-5` | `mimo-v2.6-pro` | 普通 |
| `claude-haiku-4-5` | `mimo-v2.6-flash` | 思考 |

### V2.5 向後相容性

已儲存的 `mimo-v2.5`、`mimo-v2.5-pro` 和 `mimo-v2.5-pro-ultraspeed` 設定將被保留並繼續有效。啟動時僅自動遷移未經修改的出廠預設路由。自訂路由、思考模式設定以及自訂 `default_model` 值不會被覆寫。

## 備註

Anthro Bridge 是一個非官方的 Anthropic 相容本地閘道。

它與 Anthropic、Moon Bridge 或任何上游模型提供者沒有關聯。

Claude Desktop / Cowork on 3P 透過 Claude 的第三方推理設定進行配置。選單標籤和配置欄位可能會隨著 Anthropic 更新 Claude Desktop 而變更。


> [!NOTE]
> **Claude Desktop / Cowork on 3P 透過閘道使用雲端提供商**
>
> Claude Desktop 與 Cowork on 3P 將推論請求發送至 Anthro Bridge 閘道（`http://127.0.0.1:4000`），並路由至您設定的上游雲端提供商（DeepSeek、MiMo、Kimi、MiniMax、OpenRouter）。
>
> **Ollama Local** 後端專門為 **Claude Code CLI** 設計，與 Claude Desktop 及 MCP 完全隔離。有關 Claude Code 的本機 LLM 設定，請參閱 [Ollama Local 指南](OLLAMA_LOCAL.zh-TW.md)。
