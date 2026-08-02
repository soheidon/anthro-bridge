[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**目前版本：0.16.0**

Anthro Bridge 是一個本機閘道與桌面設定工具，讓 Claude Desktop 和 Claude Code 透過 Anthropic 相容的 API 使用多個第三方 LLM 提供者。

此應用程式包含：

- 以 Rust 撰寫的本機代理伺服器
- 使用 Tauri 2、React 和 TypeScript 建構的 Windows 原生圖形介面
- 從 Anthropic 模型名稱路由到提供者特定上游模型的模型路由
- 每條路由的模型、推理與功能設定

Anthro Bridge 是一個獨立專案。它不是 Moon Bridge 的分支、前端或附屬應用程式。

## 0.16.0 版重點功能

0.16.0 版新增了具模型感知能力的 Claude Code 上下文管理功能。

- Anthro Bridge 會解析指派給 Opus、Sonnet 和 Haiku 路由的上游模型之上下文容量。
- 在自動模式下，三條路由中已知的最小容量會用作安全的 Claude Code 上下文視窗。
- 僅在三條路由的容量皆已知時，才會套用上下文控制。
- 標題列提供了一個精簡的上下文管理開關；進階模式與閾值仍可透過 `config.json` 設定。
- 應用程式可以產生包含 Anthro Bridge 連線變數與 Claude Code 上下文控制變數的完整 PowerShell 啟動命令。
- 當上下文管理停用或不完整時，產生的命令會從目前的 PowerShell 工作階段移除過時的上下文控制變數。
- 內建的上下文中繼資料涵蓋標準的直接提供者模型與內建的 OpenRouter 模型。
- 產生的命令及其環境變數行為已由 Rust 單元測試、Windows PowerShell 整合測試與前端複製流程測試涵蓋。

## 支援的模型

Anthro Bridge 支援兩類上游模型。

### 原生整合

以下提供者透過各自相容於 Anthropic 的 API 獲得支援，不需要 OpenRouter 帳號。

| 提供者 | 支援的模型系列 | 連線方式 |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro 與 V4 Flash | 直接提供者 API |
| MiniMax | MiniMax M3 與 M2.7 變體 | 直接提供者 API |
| Kimi / Moonshot | Kimi K2.x 與 Kimi K3 | 直接提供者 API |
| MiMo / Xiaomi | MiMo V2.5 與 V2.5 Pro 變體 | 直接提供者 API |

### 透過 OpenRouter 支援的模型

以下模型透過 OpenRouter 設定檔存取。每個設定檔有各自的 API 金鑰、路由對應與推理設定。

| 供應商或模型系列 | 內建支援 | 推理控制 |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | 是 | 模型特定的思考控制項 |
| Tencent Hy3 | 是 | 低與高推理強度 |
| InclusionAI Ring | 是 | 模型特定的思考與推理控制項 |
| StepFun Step 3.5 / Step 3.7 | 是 | 支援之處可選低、中與高 |
| InclusionAI Ling 系列 | 是 | 模型特定的思考控制項 |
| OpenAI GPT-5.6 Sol / Terra / Luna | 是 | 模型特定的思考與推理控制項 |

其他 OpenRouter 模型也可以從即時 OpenRouter 模型列表中選取或手動輸入。內建支援表示 Anthro Bridge 已知道該模型系列、功能旗標、供應商分組與推理控制行為。

## 運作方式

Claude Desktop 和 Claude Code 使用 Anthropic 模型名稱發送請求，例如：

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge 將這些名稱視為穩定的路由識別碼。圖形介面決定每條路由使用哪個提供者與上游模型。

範例：

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

只有必須針對上游提供者調整的欄位才會被變更。除此之外，只要上游 API 支援，訊息、工具呼叫、工具結果、思考區塊與串流資料都會保持原樣。

## 主要功能

### 提供者路由

Anthro Bridge 支援兩種上游連線類型：

1. **直接提供者整合**，直接連線至提供者自己的 Anthropic 相容 API。
2. **OpenRouter 設定檔**，連線至 OpenRouter，並可透過單一 API 路由至多個供應商與模型系列。

#### 直接提供者整合

| 提供者 ID | 顯示名稱 | 預設端點 |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter 整合

| 連線類型 | 顯示名稱 | 端點 |
|---|---|---|
| 多設定檔模型閘道 | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter 不會被視為單一模型提供者。每個 OpenRouter 設定檔可以從支援的供應商群組（如 Poolside、Tencent、InclusionAI 和 StepFun）獨立選取模型，也可以從 OpenRouter API 發現的或手動輸入的其他模型中選取。

每條 Anthropic 路由都可以獨立對應至直接提供者模型或透過 OpenRouter 設定檔選取的模型。

### OpenRouter 多設定檔支援

可以建立並獨立管理多個 OpenRouter 設定檔。

每個設定檔皆有自己的：

- 設定檔名稱
- API 金鑰設定
- Opus、Sonnet 與 Haiku 路由對應
- 思考或推理設定
- 已快取的 OpenRouter 模型列表

設定檔可以在圖形介面中新增、重新命名、刪除、以拖放方式重新排序、隱藏及選取。儀表板會為每個可見的設定檔顯示一張卡片，並在重新整理後保留已儲存的順序。

目前內建的 OpenRouter 供應商群組包含 Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6 及其他辨識出的模型系列。未知模型仍可透過搜尋或自訂模型輸入使用。儀表板會將如 `poolside/laguna-s-2.1` 的供應商限定 ID 縮短為 `laguna-s-2.1` 以利閱讀，同時在路由時保留完整的 ID。

### OpenRouter 定價與模型詳細資訊

設定中的模型價格面板會顯示支援之 OpenRouter 模型的內建價格，包括輸入、輸出與快取輸入的定價。促銷價格可以與調整後的標準價格一同顯示，包括 GPT-5.6 Sol、Terra 與 Luna 變體及其 Pro 變體。定價備註在適用時可包含長上下文定價。

### 響應式儀表板尺寸調整

初始視窗高度是根據三欄儀表板中可見的提供者與 OpenRouter 卡片數量計算。額外的卡片列會增加視窗高度，同時尊重原生最小尺寸、螢幕工作區、DPI 縮放與標題列裝飾。當設定檔的可見性或數量改變時，會依新的列數重新計算高度；在列數不變的情況下，手動調整的尺寸會保留下來。

### 在地化的 Windows 安裝程式

Windows NSIS 安裝程式提供英文、日文、簡體中文、繁體中文、韓文、法文、德文與西班牙文的語言選擇。安裝程式使用 Anthro Bridge 應用程式圖示，並在升級期間保留穩定的使用者設定。

### 最新的 UI 可靠性改進

設定寫入會序列化處理，OpenRouter 儲存使用具過時請求防護的佇列更新路徑，設定檔重新排序操作在重新整理失敗後也能乾淨地復原。回歸測試涵蓋設定檔排序、儲存競態、模型價格、儀表板卡片計數與視窗尺寸。

### 模型與推理控制項

可用的控制項取決於所選的模型。

支援的控制項可能包含：

- 思考功能開啟或關閉
- 普通、低、中、高、極高或最大推理模式
- 提供者特定的推理強度
- 不允許使用者選擇的模型所採用的固定推理模式

切換模型時，Anthro Bridge 會嘗試保留最接近的相容推理設定。若先前確切的設定不可用，則會選取最接近的支援選項，並在兩個選項距離相同時偏好較弱的選項。

### 功能偵測

Anthro Bridge 結合了內建功能註冊表與即時 OpenRouter 中繼資料。

功能可能包含：

- 圖片輸入
- 影片輸入
- 思考支援
- 推理強度支援
- 已知的定價
- 提供者特定的請求轉譯規則

即時 OpenRouter 中繼資料會被快取以減少不必要的 API 呼叫。

### 回應模型正規化

上游 API 經常在回應中回傳自己的模型名稱。Anthro Bridge 可以將該欄位覆寫回用戶端預期的 Anthropic 路由名稱。

例如：

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

正規化適用於串流與非串流回應，並可以在設定中啟用或停用。

### 序列化設定寫入

設定變更會進行序列化處理，以防止並行寫入導致設定損毀或還原。

這涵蓋以下操作：

- 模型變更
- 思考模式變更
- 推理強度變更
- OpenRouter 設定檔變更
- 與 API 金鑰相關的設定變更

### OpenRouter 儲存佇列

OpenRouter 路由變更透過專用的儲存佇列處理。

該佇列提供：

- 序列化的儲存操作
- 廢棄過時請求
- 在提交請求時擷取路由識別碼
- 防範過時的 React 閉包
- 防範來自先前選取路由的回滾
- 儲存成功後的重新整理重試
- 聚合式閘道重啟處理
- 對儲存後工作期間新增的請求進行安全處理

這可防止快速模型變更、路由切換或延遲的 Tauri 回應還原成舊的 UI 值。

### Claude Code 上下文管理

Anthro Bridge 0.16.0 可以產生具模型感知上下文設定的 Claude Code 啟動命令。

解析器會執行以下步驟：

1. 解析指派給每條標準路由的上游模型：
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. 查詢每個上游模型的已知上下文容量。
3. 要求三條路由的容量皆為已知。
4. 使用最小的容量作為安全上下文視窗。
5. 套用已設定的觸發百分比。

例如，若三條路由解析出的容量分別為 1,000,000、262,144 與 1,000,000 個 Token，Anthro Bridge 會使用：

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

產生的 PowerShell 命令使用官方的 Claude Code 變數：

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

同時也包含 Anthro Bridge 閘道連線變數：

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

範例：

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

當上下文管理停用、設為 Claude Code 預設行為，或因某條路由的容量未知而不完整時，產生的命令會在啟動 Claude Code 前清除過時的上下文變數：

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

百分比覆寫會要求提早進行主動壓縮。Claude Code 可能會忽略會使壓縮延遲超過其自身預設行為的值。

Anthro Bridge 會驗證命令產生與 PowerShell 環境變數注入。這本身並不能證明特定的 Claude Code 版本已使用這些變數；最終確認需要 Claude Code 診斷或觀察壓縮行為。

### 閘道管理

圖形介面提供：

- 閘道啟動與停止控制項
- 提供者與設定檔選取
- 路由設定
- API 金鑰管理
- 日誌檢視
- 模型列表重新整理
- 儲存狀態與錯誤顯示

閘道監聽於：

```text
http://127.0.0.1:4000
```

## 需求

- Windows 10 或 Windows 11
- 開發需使用 Node.js 24 或更新版本
- 開發需使用穩定版 Rust 工具鏈
- 至少一個支援提供者的 API 金鑰

一個提供者的金鑰就足夠了。您不需要每個提供者都有金鑰。

## 安裝

從專案的 Releases 頁面下載最新的 Windows 安裝程式並執行。

安裝程式支援以下語言：

- 英文
- 日本語
- 中文（简体）
- 中文（繁體）
- 韓語
- Français
- Deutsch
- Español

若要更新 Anthro Bridge，執行較新的安裝程式即可。現有的使用者設定會保留下來。

穩定的使用者設定儲存於：

```text
%APPDATA%\Anthro Bridge\
```

開發版本使用獨立的應用程式識別碼與資料目錄：

```text
%APPDATA%\Anthro Bridge Dev\
```

這使得穩定版和開發版可以共存，而不會共享設定或快取檔案。

## 快速入門

### 1. 設定 API 金鑰

開啟：

```text
Settings > API Key
```

輸入您打算使用的提供者金鑰並儲存。

常見的環境變數名稱為：

| 提供者 | 環境變數 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter 設定檔可以使用透過圖形介面管理的設定檔特定金鑰設定。

### 2. 設定路由模型

開啟設定並為每條路由選取上游模型：

- Opus
- Sonnet
- Haiku

對於 OpenRouter，請先選取或建立設定檔，然後在該設定檔中設定每條路由。

### 3. 啟動閘道

點選 **啟動閘道**。

確認本機端點可用：

```text
GET http://127.0.0.1:4000/health
```

### 4. 透過 Anthro Bridge 啟動 Claude Code

開啟 Claude 設定面板並點選 **複製 Claude Code 啟動命令**。

將產生的命令貼到 PowerShell 中。該命令包含：

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- 套用上下文管理時的 `CLAUDE_CODE_AUTO_COMPACT_WINDOW`
- 套用上下文管理時的 `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- 未套用上下文管理時，清除過時上下文變數的命令

該命令以 Anthro Bridge 作為閘道啟動 Claude Code，同時保留已設定的模型感知上下文行為。

Claude Desktop 與其他第三方推論的詳細說明可於以下檔案取得：

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API 端點

| 方法 | 路徑 | 說明 |
|---|---|---|
| `GET` | `/health` | 閘道健康檢查 |
| `GET` | `/v1/models` | 公開路由模型列表 |
| `POST` | `/v1/messages` | 串流與非串流 Messages API |
| `POST` | `/v1/messages/count_tokens` | Token 計數（當所選提供者支援時） |

## 設定

主要的設定檔為 `config.json`。

大多數設定應透過圖形介面更改。手動編輯僅供進階使用。

重要的模型欄位包含：

| 鍵 | 說明 |
|---|---|
| `models.<route>.upstream_model` | 發送給提供者的上游模型名稱 |
| `models.<route>.thinking_mode` | 路由特定的思考模式 |
| `models.<route>.reasoning_effort` | 提供者特定的推理強度 |
| `models.<route>.supports_vision` | 圖片支援覆寫 |
| `models.<route>.supports_video` | 影片支援覆寫 |
| `models.<route>.visible` | 該路由是否對用戶端和儀表板公開 |
| `non_vision_image_policy` | 如何處理不支援的圖片輸入 |
| `normalize_response_model_identity` | 是否正規化回應模型名稱 |
| `claude_code.auto_compact.enabled` | 全域上下文管理開關 |
| `claude_code.auto_compact.trigger_percent` | 要求的主動壓縮百分比 |
| `claude_code.auto_compact.mode` | `auto`、`manual` 或 `claude_default` |
| `claude_code.auto_compact.window_tokens` | `manual` 模式下使用的手動上下文視窗 |

不支援的圖片可以透過以下任一政策處理：

- `replace`：以文字佔位符取代圖片
- `drop`：移除圖片內容
- `reject`：回傳錯誤

### 上下文管理設定

圖形介面僅提供全域的上下文管理開關。進階數值可以直接在 `config.json` 中編輯。

自動模式：

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

手動模式：

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

Claude Code 預設行為：

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "claude_default"
    }
  }
}
```

在 `auto` 模式下，Anthro Bridge 僅在三條標準路由皆有已知的上下文中繼資料時，才會套用上下文變數。未知的自訂 OpenRouter 模型仍是有效的路由目標，但在取得中繼資料或設定手動模式之前，上下文管理會回報為不完整狀態。

靜態模型容量儲存於：

```text
gui/src-tauri/resources/model_context_windows.json
```

該註冊表包含內建預設所使用的標準 DeepSeek、MiniMax、Kimi、MiMo、Poolside、Tencent、InclusionAI、StepFun 與 OpenAI GPT-5.6 模型。

## 提供者注意事項

### DeepSeek

`reasoning_effort`：

- `deepseek-v4-pro`
  - Normal：推理強度停用
  - Thinking：High / Max
- `deepseek-v4-flash`
  - Normal：推理強度停用
  - Thinking：Low / High / Max

啟動時，DeepSeek V4 Pro 路由中儲存的舊 `low` 或 `medium` 強度會遷移為 `high`（與 DeepSeek 的有效推理等級一致）。

新安裝和全新產生的設定之預設 DeepSeek 路由：

- Opus 5 → V4 Flash、Thinking、Max
- Sonnet 5 → V4 Flash、Thinking、High
- Haiku 4.5 → V4 Flash、Thinking、Low

現有已儲存的路由不會自動變更。

### MiniMax

MiniMax 模型的行為因模型世代而異。Anthro Bridge 會套用所選模型所需的請求格式，包括在支援時採用自適應或停用的思考功能。

### Kimi

Kimi 模型根據模型系列可能使用思考參數或固定推理強度模式。Anthro Bridge 會將圖形介面選擇轉譯為適當的上游請求格式。

### MiMo

MiMo 對支援的路由使用 `thinking_mode` 而非通用的 `thinking` 欄位。

視覺支援因模型而異。當路由無法接受圖片輸入時，Anthro Bridge 會套用已設定的不支援圖片政策。

### OpenRouter

OpenRouter 模型在已辨識時會按供應商分組。圖形介面提供：

- 模型搜尋
- 供應商分組
- 自訂模型輸入
- 功能標記
- 定價顯示
- 每個模型的推理控制項
- 統一的模型列表重新整理

OpenRouter 模型的功能和行為可能會隨時間改變。可用時會使用即時中繼資料，內建註冊表則為已知模型提供穩定的預設值。

內建的 OpenAI GPT-5.6 Balanced 設定檔在新安裝與全新產生的設定中，預設在所有路由上使用 Thinking High：

- Opus 5 → GPT-5.6 Sol、Thinking、High
- Sonnet 5 → GPT-5.6 Terra、Thinking、High
- Haiku 4.5 → GPT-5.6 Luna、Thinking、High

現有已儲存的路由不會自動變更。

## 使用者介面

設定介面包含：

- 可折疊的提供者區塊
- Opus、Sonnet 和 Haiku 路由設定
- OpenRouter 的模型搜尋和供應商分組
- 基於模型功能的思考與推理控制項
- 自訂上游模型輸入
- 自動路由儲存
- 明確的 API 金鑰儲存
- 儲存進度與錯誤訊息
- 模型定價與功能資訊
- 回應模型正規化開關
- 標題列中的 Claude Code 上下文管理開關
- Claude 設定面板中的 Claude Code 啟動命令複製操作

儀表板包含：

- 提供者或 OpenRouter 設定檔選取
- 閘道狀態
- 當前路由對應
- 功能指示器
- 定價資訊
- 提供者切換狀態

## 開發

### 專案結構

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   ├── model_routing.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   │       ├── config.json
│   │       └── model_context_windows.json
│   └── package.json
└── LICENSE
```

### 在開發模式下執行

```bash
cd gui
npm install
npm run tauri dev
```

### 建構開發版本

在 Windows 上，使用單一 Rust 建構工作以避免編譯器間歇性終止：

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

開發版本使用：

- 視窗標題：`Anthro Bridge (DEV)`
- 連接埠：`4000`
- 應用程式識別碼：`com.soheidon.anthro-bridge.dev`
- 獨立的設定與快取目錄

### 穩定版本

穩定版僅應在準備發布時建立。一般的實作與驗證工作應使用開發版本。

## 驗證

前端驗證：

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rust 驗證：

```bash
cd gui/src-tauri
cargo check
cargo test
```

上下文管理驗證涵蓋：

- 代理伺服器與上下文解析器之間共用的路由至上游解析
- 內建直接提供者與 OpenRouter 模型的完整模型上下文中繼資料
- 跨三條標準路由的自動最小視窗選擇
- 已套用、停用、不完整、手動與 Claude 預設模式
- 官方的 Claude Code 環境變數名稱
- PowerShell 命令的呈現與逸出處理
- 閘道連線變數
- 在真實的 Windows PowerShell 子處理程序中注入環境變數
- 未套用上下文管理時移除過時的上下文變數
- 前端複製產生的啟動命令

針對 OpenRouter 路由選擇器：

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

OpenRouter 選擇器測試涵蓋：

- 佇列儲存期間擷取的路由識別碼
- 跨路由回滾防護
- 過時回呼防護
- 重新整理重試行為
- 重新整理失敗後的閘道重啟
- 進行中請求的廢棄
- 世代型回滾抑制

未來可能會新增一個針對重啟聚合的專用多重儲存測試，以鎖定以下行為：

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## 手動驗證檢查清單

自動化測試無法重現每個 Tauri 和 React 的時序狀況。發布前，請在開發版本中驗證以下項目：

- 每個 OpenRouter 設定檔顯示正確的懸停詳細資訊
- 模型選取在變更後不會明顯回滾
- 思考與推理選擇在儲存後保持穩定
- 關閉並重新開啟設定畫面後設定仍然正確
- 重新啟動應用程式後設定仍然正確
- 在儲存期間切換設定檔不會損毀任一個設定檔
- 儲存失敗時僅回滾發起該儲存的路由
- 重新整理重試成功會清除先前的錯誤
- 重新整理重試失敗會顯示最新的錯誤
- 必要的閘道重啟在批次後僅執行一次
- 自訂模型能正確儲存並重新載入
- 內建與即時 OpenRouter 功能正確顯示
- 標題列的上下文管理開關使用視覺化切換，並會保留其狀態
- 每個內建的提供者或 OpenRouter 預設都能解析出三條路由的容量
- 產生的 Claude Code 命令包含閘道連線變數
- 啟用上下文管理時，產生的命令包含 `CLAUDE_CODE_AUTO_COMPACT_WINDOW` 與 `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- 停用上下文管理時，產生的命令會移除這兩個上下文變數
- 複製的命令會透過執行中的 Anthro Bridge 閘道啟動 Claude Code

## 疑難排解

### 連接埠 4000 已被使用

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 模型拒絕圖片或影片輸入

模型功能因提供者和路由而異。請檢查圖形介面中的功能標記，並選取相容的路由。

對於不支援的圖片輸入，Anthro Bridge 會遵循 `non_vision_image_policy`。

### 升級後設定還原

請先重新啟動應用程式，讓遷移程序執行。

若問題持續存在：

1. 備份使用者設定。
2. 將其與內建設定進行比對。
3. 移除過時欄位，或視需要重設使用者設定。

穩定版設定位置：

```text
%APPDATA%\Anthro Bridge\config.json
```

開發版設定位置：

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter 模型列表過時

使用設定中的統一模型重新整理控制項。Anthro Bridge 會快取模型中繼資料，因此在 OpenRouter 更改模型條目後可能需要手動重新整理。

### 上下文管理不完整

自動上下文管理需要三條標準路由皆為已知容量。

請檢查 Opus、Sonnet 與 Haiku 所設定的上游模型。自訂或新發布的模型可能尚未存在於 `model_context_windows.json` 中。

選項：

1. 選取具有已知中繼資料的內建模型。
2. 將經驗證的模型中繼資料新增至靜態註冊表。
3. 在 `config.json` 中使用手動模式。
4. 使用 `claude_default` 將壓縮完全交給 Claude Code 處理。

### Claude Code 未使用預期的上下文設定

請確認 Claude Code 是從產生的 PowerShell 命令啟動，而非從單獨的終端機命令啟動。

在相同的 PowerShell 工作階段中檢查：

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

這些數值可確認啟動環境已準備完成。但它們不能證明 Claude Code 已使用這些變數。請使用 Claude Code 診斷或觀察壓縮行為以進行最終確認。

## 翻譯

英文為原始 README。

翻譯後的 README 檔案存放於 `docs/` 目錄下。當英文 README 有變更時，請從英文原文重新產生或更新翻譯檔案，而非獨立編輯各語系版本。

應用程式介面的語言檔案存放於：

```text
gui/src/i18n/lang/
```

## 授權條款

MIT 授權條款。詳見 [LICENSE](../LICENSE)。
