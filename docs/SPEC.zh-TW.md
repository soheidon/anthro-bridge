[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## 概述

一個輕量級的代理 + GUI 管理工具，將 Claude Desktop / Claude Code API 請求路由到多個提供者的 Anthropic 相容端點。

### 架構

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- 嵌入在 Tauri 應用中 (axum 0.7 + reqwest)
       |
       | 根據 model 欄位路由 -> 解析正確的上游提供者
       | 僅重寫 model 為上游名稱
       | 為非 thinking 變體注入 thinking disabled
       | 逐模型媒體支援檢查
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi / MiMo)
```

#### 設計原則

- **外殼模型 + 提供者選擇**：Claude Desktop 始終看到 `claude-sonnet-4-6` / `claude-haiku-4-5`。實際的 LLM 在 GUI 中選擇（DeepSeek / MiniMax / Kimi / MiMo）。使用活動提供者的模型映射進行路由。
- **僅活動提供者需要 API 金鑰**：自 v0.5.0 起，僅檢查路由表引用的提供者的 API 金鑰。非活動提供者的金鑰不需要。
- **輕量代理**：除 `model` 欄位外不修改任何內容。SSE 逐位元組轉發。
- **無損轉發**：消息主體、工具呼叫、thinking 塊未經修改地傳遞。
- **Windows 原生 GUI**：Tauri v2 + React 19 + TypeScript。Rust 後端，Vite + React 19 前端。
- **零外部依賴**：自 v0.3.0 起代理嵌入 Tauri 二進制檔。不需要 Python。
- **多語言**：自 v0.9.1 起支援 8 種語言（en, ja, zh-CN, zh-TW, ko, fr, de, es）。將語言檔案放入 `lang/` 即可新增語言。首次啟動語言選擇器。

### GUI 管理工具

Tauri v2 + React 19 + TypeScript。雙面板佈局：儀表板 + 設定。

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [啟動/停止閘道] [狀態]         [=]       |
+------------------------------------------+
|  儀表板                                   |
|  +- 選擇 LLM 提供者 ------------------+|
|  | [DeepSeek] [MiniMax] [Kimi] [MiMo]   ||
|  +- 狀態 --------------------------------+
|  | 埠 4000 | API 金鑰 | 閘道 URL        ||
|  | 模型路由表                            ||
|  +- 最新日誌 ----------------------------+
|  | 日誌檢視器與 Pro/Flash 計數器         ||
|  +---------------------------------------+
+------------------------------------------+

設定 (=):
  +- 語言 ------------------------------+
  | 下拉選單即時切換                      |
  +- API 金鑰 ---------------------------+
  | 逐提供者 API 金鑰管理                 |
  +- Claude Desktop 設定 ----------------+
  | 設定 JSON 產生、複製、                 |
  | 設定檔偵測                            |
  +- 閘道設定 ---------------------------+
  | config.json 編輯器（進階）            |
  +---------------------------------------+
```

### Tauri 指令

| # | 指令 | 類型 | 說明 |
|---|------|------|------|
| 1 | `check_health` | async | 代理健康檢查 |
| 2 | `check_gateway_status` | sync | 埠 4000 + tokio 任務存活檢查 |
| 3 | `check_api_key` | sync | 活動提供者 API 金鑰狀態 |
| 4 | `set_env_api_key` | sync | 透過 setx 持久化 API 金鑰 |
| 5 | `get_port_4000_process` | sync | 透過 netstat 取得埠 4000 的 PID |
| 6 | `read_config` | sync | 讀取 config.json |
| 7 | `read_config_raw` | sync | 原始 config.json 文字 + 編碼偵測 |
| 8 | `write_config` | sync | 儲存 config.json (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | 讀取最新日誌 |
| 10 | `read_log` | sync | 讀取指定的日誌檔 |
| 11 | `list_logs` | sync | 列出日誌檔 |
| 12 | `create_new_log` | sync | 建立新日誌檔 |
| 13 | `open_logs_folder` | sync | 開啟日誌資料夾 |
| 14 | `open_path` | sync | 開啟任意路徑 |
| 15 | `find_claude_configs` | sync | 自動偵測 Claude Desktop 設定檔 |
| 16 | `start_proxy` | sync | 啟動代理（解析設定 -> 啟動 -> 驗證埠） |
| 17 | `stop_proxy` | sync | 停止代理（優雅關閉） |
| 18 | `proxy_status` | sync | 檢查任務存活狀態 |
| 19 | `check_all_api_keys` | sync | 所有提供者 API 金鑰狀態 |
| 20 | `update_active_provider` | sync | 儲存 active_provider |
| 21 | `update_provider_api_key_env` | sync | 儲存 provider api_key_env |
| 22 | `get_user_language` | sync | 取得已儲存的語言偏好設定 |
| 23 | `set_user_language` | sync | 儲存語言偏好設定 |
| 24 | `is_first_run` | sync | 判斷是否首次啟動（user_prefs.json 是否存在） |

### 代理伺服器 (proxy.rs)

自 v0.3.0 起從 Python 移植到 Rust (axum 0.7/reqwest)。

#### 端點

| 方法 | 路徑 | 行為 |
|------|------|------|
| GET | `/health` | 健康檢查 |
| GET | `/v1/models` | 公開模型清單（僅 `visible: true`） |
| POST | `/v1/messages` | 模型解析 -> thinking 注入 -> 媒體檢查 -> 轉發（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 若支援則轉發至上游 |

#### 模型路由

從 Gateway 模型 -> (提供者, 上游模型) 建立反向查詢表，使用每個提供者的 `models` 區段。由於所有提供者使用相同的 Gateway 模型名稱，衝突時 `active_provider` 勝出。實際上，只有活動提供者的模型會進入路由表。

預設路由 (v0.11.0):

| 閘道模型 | DeepSeek | MiMo | MiniMax | Kimi |
|---|---|---|---|---|
| claude-opus-4-8 | deepseek-chat | mimo-v2-pro | MiniMax-M3 + Thinking | kimi-k2.7-code + Thinking |
| claude-sonnet-5 | deepseek-chat | mimo-v2-flash | MiniMax-M3 + Thinking | kimi-k2.6 + Thinking |
| claude-haiku-4-5 | deepseek-chat | mimo-v2-flash | MiniMax-M3 + Thinking | kimi-k2.6 + Normal |

#### API 金鑰驗證（自 v0.5.0 起）

步驟 1：建立模型路由表（不需要 API 金鑰）
步驟 2：僅檢查路由表引用的提供者的 API 金鑰

#### Thinking 注入 (v0.9.0〜)

根據 `models` 條目的 `thinking_mode` 控制四種行為:
- `disabled`: 始終注入 `{"type": "disabled"}`
- `thinking`: 使用者未指定時注入 `{"type": "enabled", "budget_tokens": 10000}`
- `thinking_only`: 無 `thinking` 參數則返回 400 錯誤，必須自動注入
- `normal`: 不干預

#### K3 特殊處理 (v0.11.0)

Kimi K3 不支援 `thinking` 參數。透過 `suppressThinkingParameter: true` + `forcedReasoningEffort: "max"` 實現:
- 從請求中移除 `thinking` 參數
- 注入 `reasoning_effort: "max"`
- 移除 temperature/top_p 等固定參數

#### 媒體檢查 / 影像淨化

逐模型的 `supports_vision` / `supports_video` 標誌決定行為。對於接收影像的非視覺模型，套用 `non_vision_image_policy`：
- `replace`（預設）：將影像塊替換為佔位符文字
- `drop`：移除影像塊（若內容為空則插入佔位符）
- `reject`：返回 400 錯誤

視訊塊始終返回 400。`non_vision_image_policy` 可透過 `/health` 查看。

### 多語言

每語言一個檔案的架構，搭配 `import.meta.glob` 自動探索：

```
gui/src/i18n/lang/
  en.ts      英語（規範——定義 TranslationKey 類型）
  ja.ts      日語
  zh-CN.ts   中文（簡體）
  zh-TW.ts   中文（繁體）
  ko.ts      韓語
  fr.ts      法語
  de.ts      德語
  es.ts      西班牙語
```

要新增語言：複製 `en.ts`、翻譯、重新建置。不需要程式碼變更。

### config.json 參考

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "顯示名稱",
      "upstream_url": "Anthropic 相容 API 基礎 URL",
      "api_key_env": "API 金鑰環境變數名稱",
      "default_model": "回退模型名稱",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-6": "實際模型名稱" },
      "visible_models": ["claude 公開模型名稱"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "實際模型名稱",
          "thinking_mode": "thinking",
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
