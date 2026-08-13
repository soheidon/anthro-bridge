[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## 概述

一個輕量級代理 + GUI 管理工具，將 Claude Desktop / Claude Code API 請求路由到多個提供者的 Anthropic 相容端點。

### 架構

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- 嵌入於 Tauri 應用 (axum 0.7 + reqwest)
       |
       | 依 model 欄位路由 -> 解析正確的上游提供者
       | 僅將 model 重寫為上游名稱
       | 為非推理變體注入 thinking disabled
       | 逐模型媒體支援檢查
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### 設計原則

- **外殼模型 + 提供者選擇**：Claude Desktop 始終看到 `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`。實際的 LLM 在 GUI 中選擇（DeepSeek / MiniMax / Kimi / MiMo / OpenRouter）。使用活動提供者的模型映射進行路由。
- **OpenRouter 支援**：路由到 OpenRouter 的 Anthropic 相容端點，預設使用 Poolside Laguna S/XS。專用的 thinking 模式控制（Max/On/Off）在請求時轉換為 OpenRouter 的 `reasoning` 格式。
- **僅活動提供者需要 API 金鑰**：自 v0.5.0 起，僅檢查路由表引用的提供者的 API 金鑰。非活動提供者的金鑰不需要。
- **輕量代理**：除 `model` 欄位外不修改任何內容。SSE 逐位元組轉發。
- **無損轉發**：訊息主體、工具呼叫、thinking 區塊未經修改地傳遞。
- **Windows 原生 GUI**：Tauri v2 + React 19 + TypeScript。Rust 後端，Vite + React 19 前端。
- **零外部依賴**：自 v0.3.0 起代理嵌入於 Tauri 二進位檔。不需要 Python。
- **多語言**：支援 8 種語言（en, ja, zh-CN, zh-TW, ko, fr, de, es）。將語言檔案放入 `lang/` 即可新增語言。首次啟動語言選擇器。
- **推理強度**：DeepSeek V4 Pro（V4-Pro-0813）和 V4 Flash（V4-Flash-0731）在 Thinking 模式下均支援推理強度 Low / High / Max。推理強度在普通模式下停用。為 V4 Pro 路由儲存的舊版 `medium`/`xhigh` 強度會在啟動時遷移為 `high`。代理在傳送至 DeepSeek 前會正規化強度值（`medium`/`xhigh` → `high`），並使用 `output_config.effort` 格式。
- **能力偵測**：從 OpenRouter API 取得即時能力旗標（`supports_image_url`、`supports_image_base64`、`supports_video_url`、`supports_video_base64`）並持久化到 config.json。
- **峰谷定價感知**：DeepSeek 與 OpenRouter 的峰值時段在本地時區顯示。
- **MiniMax-M3 thinking 切換**：MiniMax-M3 透過 Anthropic 相容 API 支援 Thinking ON/OFF（`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`）。M2.x 模型仍僅支援 thinking。啟動遷移會將現有用戶的舊版 `thinking_only` → `thinking` 轉換。
- **回應模型識別規範化**：將上游回應（SSE 串流與非串流）中的 `model` 名稱重寫回 Anthropic 官方模型名稱。由 config.json 中的 `normalize_response_model_identity` 與執行期的 `AtomicBool` 控制。提供獨立的儲存指令（`update_normalize_model_identity`），以避免與伺服器設定的儲存互相污染。
- **結構化通訊日誌**：`tracing` + `tracing-appender` 將結構化日誌寫入 `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`。每個請求從 `AtomicU64` 計數器取得關聯 ID。日誌條目包含請求模型、閘道模型、上游模型、規範化結果與跳過原因。不記錄敏感資料（提示詞、主體、API 金鑰）。
- **PEAK 徽章**：儀表板中以粉紅色徽章標示峰值定價模型。
- **UTC 偏移顯示**：時區選擇器在每個選項旁顯示動態 UTC 偏移（如 UTC+09:00）。
- **Laguna S/XS 2.1 權杖上限失敗偵測**：在 SSE 串流與非串流回應中偵測 `stop_reason: "max_tokens"` 的僅推理回應。當達到每回合權杖上限而未產生可用文字或工具呼叫時記錄警告。適用於透過 OpenRouter 提供的所有 Poolside Laguna 模型。
- **Poolside thinking:disabled 傳遞**：將客戶端傳送的 `thinking: { type: "disabled" }` 轉換為 OpenRouter 的 `reasoning: { enabled: false }` 格式，用於 Poolside 模型，確保即使未儲存設定也能正確轉發停用的 thinking。
- **Laguna Opus 預設遷移**：一次性冪等遷移將 `poolside/laguna-s-2.1` OpenRouter 使用者的 `claude-opus-5` 預設從 thinking 開啟變更為普通模式。新安裝範本反映更新的預設值。
- **OpenRouter 多模型集**：每個使用者可擁有多個 OpenRouter 模型集，各有自己的 API 金鑰與模型設定。透過 Tauri 指令進行模型集的新增、讀取、更新、刪除。可從儀表板或設定切換活動模型集。模型集可透過拖曳重新排序、隱藏，並依設定的順序持久化。
- **OpenRouter 儀表板卡片**：儀表板為每個可見的 OpenRouter 模型集建立一張卡片，當沒有模型集時則提供備用卡片。模型摘要僅在 OpenRouter 顯示時隱藏第一個 `/` 之前的供應商命名空間；完整的上游 ID 在路由時保持不變。
- **OpenRouter 模型登錄**：內建的本機已知 OpenRouter 模型登錄（`model_capabilities.rs`、`builtinOpenRouter.ts`），含預先設定的能力（視覺、影片、thinking 策略、推理強度）、供應商分組與定價資料。用於無即時 API 呼叫的模型分類。
- **OpenRouter 定價詳情**：內建定價支援提示詞、輸出與快取輸入費率的目前值及調整後標準值，包括 GPT-5.6 Sol、Terra、Luna 與 Pro 變體。當兩者同時可用時，GUI 會一起顯示促銷與標準費率。
- **GPT-5.6 模型支援**：OpenRouter 模型集可使用 Sol、Terra 與 Luna 模型變體，並具備能力感知的 thinking 控制與長文本費率定價備註（如適用）。內建的 OpenAI GPT-5.6 Balanced 模型集為新安裝將 Opus 5 → GPT-5.6 Sol、Sonnet 5 → GPT-5.6 Terra、Haiku 4.5 → GPT-5.6 Luna 路由，三條路由皆以 Thinking High 推理強度；既有已儲存的路由不會自動變更。
- **儀表板驅動的視窗尺寸**：初始與列數變更時，依三欄網格中的可見儀表板卡片計算視窗高度。計算考量卡片高度、網格間距、原生最小尺寸、螢幕工作區、DPI 縮放與視窗裝飾，同時在列數不變時保留手動調整大小。
- **本地化 NSIS 安裝程式**：Windows 安裝程式提供英文、日文、簡體中文、繁體中文、韓文、法文、德文與西班牙文語言選擇，並內含 Anthro Bridge 應用程式圖示。
- **回歸測試涵蓋**：Vitest 涵蓋 OpenRouter 模型集排序與儲存競態、正式定價資料、儀表板卡片計數語意與螢幕感知視窗尺寸。
- **透過 OpenRouter 新增提供者**：InclusionAI 與 StepFun 新增為 OpenRouter 模型提供者，具備專用能力旗標、thinking 模式控制與供應商分組。
- **Tencent Hy3 thinking 模式**：支援 Tencent Hunyuan 模型的 Low/High 推理強度。proxy.rs 中的 thinking 模式轉換將 `thinking_mode` 對應到 OpenRouter 的 `reasoning` 格式。UI 以下拉選項顯示 Low/High。
- **Kimi K3 修正**：從能力定義中移除硬編碼的 `forced_reasoning_effort`。將固定的 "Max" 顯示替換為可設定的下拉選擇器。預設值取自已儲存的設定，若無則回退為 "max"。
- **設定寫入序列化**：所有寫入設定的 Tauri 指令皆透過 `execute_serialized_config_mutation` 搭配 `Mutex` 鎖進行序列化。`ConfigState` 結構提供 `applied_config`、`in_flight_config` 與 `pending_ops` 追蹤並附帶驗證。防止多個設定變更同時儲存時的競態條件。
- **OpenRouter UI 競態修正**：(1) `syncUiFromSavedRouteRef` 最新的 callback ref 防止過時的閉包覆寫新路由的 UI。(2) `rollbackRouteId` 守衛防止跨路由的 Phase 2 回滾。(3) `useRouteSaveGeneration` hook 為所有處理程式提供 `begin()`/`isCurrent()` 世代守衛。(4) 儲存佇列 hook（`useOpenRouterSaveQueue`）搭配清空迴圈、取代偵測與重新啟動 OR 彙總。
- **開發/穩定版應用程式識別隔離**：`paths.rs` 中的 `AppChannel` 列舉（`Stable`/`Dev`）選擇不同的識別碼（`com.soheidon.anthro-bridge` 對比 `.dev`）、設定目錄（`Anthro Bridge` 對比 `Anthro Bridge Dev`）與快取路徑。開發通道使用 `tauri.dev.conf.json`。NPM 腳本：`npm run dev`（開發）、`npm run dev:stable`（穩定）。
- **設定範本內嵌**：`include_str!()` 在編譯時期嵌入 `config_template.rs`，移除對內含 `config.json` 的執行期依賴。`merge_bundled_providers` 回傳帶型別錯誤處理的 `Result`。
- **前端回歸測試**：使用 `QueueHarness` 與 `GenerationHandlerHarness` 的 7 個 OpenRouter 儲存競態 Vitest 回歸測試。測試涵蓋：最新 callback ref、跨路由回滾守衛、識別擷取、重新整理重試（失敗與成功路徑）、進行中取代與世代守衛。
- **Claude Code 上下文管理**：針對 Claude Code 的模型感知自動壓縮。`resolve_effective_auto_compact` 將每條標準路由（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）解析為其上游模型，在靜態 `model_context_windows.json` 登錄中查詢每個模型的上下文容量，並在 Auto 模式下使用已知的最小容量作為安全上下文視窗。上下文控制僅在三種容量皆已知時套用（否則狀態為 Incomplete）。標題列切換可開啟/關閉上下文管理；進階模式與閾值在 `config.json` 的 `claude_code.auto_compact` 下設定。模式：`auto`、`manual`（`window_tokens`）、`claude_default`。
- **Claude Code 啟動命令產生**：`build_claude_code_launch_command` 產生完整的 PowerShell 命令，結合閘道連線變數（指向本機閘道的 `ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`）與 Claude Code 上下文控制變數（`CLAUDE_CODE_AUTO_COMPACT_WINDOW`、`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`）。當上下文管理停用、不完整或設為 Claude 預設時，命令使用 `Remove-Item Env:... -ErrorAction SilentlyContinue` 移除過時的上下文變數，以避免先前設定的工作階段值洩漏到新的啟動中。Claude 設定面板中的「複製 Claude Code 啟動命令」按鈕會將命令複製到剪貼簿。Anthro Bridge 僅產生並複製命令——絕不會執行它。
- **共享模型路由模組**：`model_routing.rs` 將路由到上游的解析抽取為純函式，由 `proxy.rs` 與上下文解析器共享，確保上下文視窗解析出的上游模型與代理實際轉發的模型相同。
- **上下文容量登錄**：`model_context_windows.json` 是已知上下文容量的靜態登錄，涵蓋內建的直接提供者模型（DeepSeek、MiniMax、Kimi、MiMo）與內建的 OpenRouter 模型（Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6）。未知的自訂 OpenRouter 模型仍可作為有效的路由目標，但在加入元資料或設定手動模式之前，會將上下文管理回報為 Incomplete。

### GUI 管理工具

Tauri v2 + React 19 + TypeScript。雙面板佈局：儀表板 + 設定。

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [啟動/停止閘道] [狀態]        [=]     |
+------------------------------------------+
|  儀表板                                   |
|  +- 選擇 LLM 提供者 ------------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]  ||
|  +- 狀態 -------------------------------+
|  | 埠 4000 | API 金鑰 | 閘道 URL        ||
|  | 模型路由表                            ||
|  +- 最新日誌 ---------------------------+
|  | 日誌檢視器與 Pro/Flash 計數器         ||
|  +---------------------------------------+
+------------------------------------------+

設定 (=):
  +- 語言 -------------------------------+
  | 下拉選單即時切換                       |
  +- API 金鑰 ----------------------------+
  | 逐提供者 API 金鑰管理                  |
  +- Claude Desktop 設定 -----------------+
  | 設定 JSON 產生、複製、                  |
  | 設定檔偵測                             |
  +- 閘道設定 ----------------------------+
  | config.json 編輯器（進階）             |
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
| 25 | `openrouter_get_models` | async | 獲取/快取 OpenRouter 模型目錄 |
| 26 | `set_model_upstream` | sync | 儲存閘道模型的 upstream 模型 + thinking 設定 + 能力旗標 |
| 27 | `update_server_config` | sync | 儲存伺服器主機/埠/CORS 設定 |
| 28 | `update_normalize_model_identity` | sync | 儲存回應模型識別規範化切換（更新設定 + 執行期 AtomicBool） |
| 29 | `update_claude_code_auto_compact_global` | sync | 切換全域 Claude Code 上下文管理（啟用 + 觸發百分比） |
| 30 | `update_claude_code_auto_compact_target` | sync | 設定逐提供者/模型集的上下文模式（auto / manual / claude_default）+ 手動視窗權杖數 |
| 31 | `update_claude_code_context_settings` | sync | 全域 + 目標上下文設定的組合原子更新 |
| 32 | `resolve_claude_code_auto_compact` | sync | 解析有效的上下文設定（模式、視窗權杖數、觸發百分比、狀態） |
| 33 | `build_claude_code_launch_command` | sync | 產生完整的 PowerShell Claude Code 啟動命令（閘道 + 上下文環境變數） |

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

使用每個提供者的 `models` 區段，從閘道模型 -> (提供者, 上游模型) 建立反向查詢表。由於所有提供者使用相同的閘道模型名稱，衝突時 `active_provider` 勝出。實際上，只有活動提供者的模型會進入路由表。

#### API 金鑰驗證（自 v0.5.0 起）

步驟 1：建立模型路由表（不需要 API 金鑰）
步驟 2：僅檢查路由表引用的提供者的 API 金鑰

#### Thinking 注入

對設定條目中 `thinking: "disabled"` 的模型，僅在使用者未明確設定 thinking 時注入 `{"type": "disabled"}`。

#### 回應模型規範化

當啟用 `normalize_response_model_identity` 時，代理會重寫上游回應中的 `model` 欄位：

- **非串流**：解析 JSON 回應，將 `model` 重寫為 Anthropic 標準名稱並重新序列化
- **串流 (SSE)**：攔截 `message_start` 事件幀，使用位元組範圍取代就地重寫 `model`，以保留 SSE 格式與空白字元
- **跳過原因**：`disabled`（切換關閉）、`non_success_status`（非 200 回應）、`content_encoding_not_transformable`（gzip/brotli）、`stream_error`、`stream_cancelled`
- **決策邏輯**：純函式（`should_normalize_nonstream`、`nonstream_skip_reason`）同時用於正式程式碼與測試

#### 媒體檢查 / 影像淨化

逐模型的 `supports_vision` / `supports_video` 旗標決定行為。對於接收影像的非視覺模型，套用 `non_vision_image_policy`：
- `replace`（預設）：將影像塊替換為佔位符文字
- `drop`：移除影像塊（若內容為空則插入佔位符）
- `reject`：返回 400 錯誤

視訊塊始終返回 400。`non_vision_image_policy` 可透過 `/health` 查看。

#### Claude Code 上下文管理

Claude Code 上下文控制使用兩個官方環境變數：

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

解析管線：

1. 將每條標準路由（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）解析為其上游模型
2. 在 `model_context_windows.json` 中查詢每個上游模型的上下文容量
3. 要求三種容量皆為已知
4. 使用已知的最小容量作為安全上下文視窗
5. 套用設定的觸發百分比

模式：`auto`（已知的最小容量）、`manual`（`window_tokens`）、`claude_default`（Claude Code 自己的預設；不設定任何變數）。有效狀態為 `applied`、`disabled` 或 `incomplete`。

啟動命令結合閘道連線變數與上下文變數：

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

當未套用上下文控制時，命令會先移除過時的變數：

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

百分比覆寫只會讓壓縮提前；會延遲壓縮超過 Claude Code 預設的值可能被忽略。Anthro Bridge 僅產生並複製命令——它絕不會執行命令，這也無法證明特定 Claude Code 版本會採用這些變數（最終確認需要 Claude Code 診斷或觀察到的壓縮行為）。

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

每個提供者或 OpenRouter 模型集也可以透過 `claude_code: { "auto_compact": { "mode": "auto" } }` 設定預設的上下文模式。路由的有效模式是提供者/模型集的值，若無則回退到全域區塊；`resolve_claude_code_auto_compact` 回傳解析後的結果。
