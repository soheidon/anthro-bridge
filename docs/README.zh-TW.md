[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**將 Claude Code Desktop 用作編碼框架，將實作路由至第三方 API，並將外部模型用作 Antigravity 的規劃器。**

Anthro Bridge 是一套專為 AI 輔助軟體開發打造的 Windows 附屬應用程式，圍繞兩個核心工作流程建構：

1. **Claude Code / Claude Desktop + 第三方閘道 (3P Gateway)**：繼續使用 Claude Code Desktop 作為編碼框架，同時透過本機 Anthropic 相容閘道將模型請求路由至第三方 LLM API（DeepSeek、MiMo、MiniMax、Kimi 與 OpenRouter）。
2. **Antigravity + MCP 規劃器 (MCP Planner)**：透過 Anthro Bridge MCP `plan` 工具（`anthro-bridge/plan`）將架構設計與實作規劃委託給外部模型，同時使用 Antigravity 訂閱內含的模型額度執行檔案編輯與測試。

---

## 兩個主要工作流程

### 1. Claude Code / Claude Desktop with 3P Gateway

繼續使用 Claude Code Desktop 與 Claude Desktop 作為代理型編碼框架，同時將底層模型請求路由至 Anthropic 用戶端原生不支援的第三方 LLM API。

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **框架與模型分離**：保留 Claude 的程式庫探索、工具使用、檔案編輯與測試執行能力，同時將推論路由至第三方提供者。
- **動態多設定檔路由**：在 GUI 儀表板中隨時切換當前提供者或 OpenRouter 設定檔，並在設定中自訂 Opus、Sonnet 與 Haiku 路線。
- **設定指南**：[Claude Desktop / Cowork 3P Gateway 設定指南](THIRD_PARTY_INFERENCE.zh-TW.md)

### 2. Antigravity with MCP Planner

透過 Anthro Bridge MCP `plan` 工具（`anthro-bridge/plan`）將實作規劃與架構設計委託給外部模型，同時使用 Antigravity 的訂閱模型額度執行實際的檔案編輯與終端機指令。

```text
Antigravity
    ↓
程式碼庫探索 (收集情境)
    ↓
anthro-bridge / plan (MCP)
    ↓
Anthro Bridge MCP 伺服器
    ↓
設定的外部大型語言模型
    ↓
結構化實作計劃
    ↓
Antigravity 使用訂閱額度
執行編輯、建置與測試
```

- **規劃與執行分工**：外部模型產生高階計劃；Antigravity 訂閱額度執行高 Token 消耗的程式碼編輯與測試循環。
- **即時 GUI 設定**：在 Anthro Bridge 中切換規劃器提供者、模型或推論強度時，會在下一次 `plan()` 呼叫時立即生效，無需重啟 Antigravity。
- **設定指南**：[Google Antigravity + Anthro Bridge MCP 設定指南](ANTIGRAVITY_MCP.zh-TW.md)

---

## 支援的提供者

| 提供者 | 連線類型 | 支援的模型系列 | 推論控制 |
|---|---|---|---|
| **DeepSeek** | 直接 API | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | 直接 API | MiniMax M3, M2.7 | 特定模型支援 |
| **Kimi / Moonshot** | 直接 API | Kimi K2.x, Kimi K3 | 思考 / 推論強度 |
| **MiMo / Xiaomi** | 直接 API | MiMo V2.5, V2.5 Pro | 思考模式 |
| **OpenRouter** | 多設定檔閘道 | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini 等 | 特定模型 / 特定設定檔 |

---

## 安裝

從 [Releases](https://github.com/soheidon/anthro-bridge/releases) 頁面下載最新的 Windows 安裝程式（`Anthro Bridge_x.x.x_x64-setup.exe`）並執行。

安裝程式支援 8 種語言（英文、日文、簡體中文、繁體中文、韓文、法文、德文、西班牙文），並在升級時保留現有的使用者設定。

---

## 快速上手

### 工作流程 1：適用於 Claude Code / Claude Desktop 的 3P Gateway

1. 開啟 Anthro Bridge **設定 > API Key** 並設定所需提供者的 API 金鑰。
2. 在儀表板上選擇提供者或 OpenRouter 設定檔。
3. 點擊 **啟動閘道 (Start Gateway)**（監聽 `http://127.0.0.1:4000`）。
4. 連線 Claude Code 或 Claude Desktop：
   - **Claude Code**：在設定中點擊 **複製 Claude Code 啟動指令** 並貼至 PowerShell 中執行。
   - **Claude Desktop / Cowork**：參考 [Claude Desktop 3P 設定指南](THIRD_PARTY_INFERENCE.zh-TW.md)。

### 工作流程 2：適用於 Google Antigravity 的 MCP Planner

1. 在 Anthro Bridge 中為您選擇的規劃器模型設定 API 金鑰。
2. 選擇 Anthro Bridge 的 **MCP** 標籤頁，並在 **設定 > MCP Plan 詳細設定** 中設定規劃器模型與推論參數。
3. 在 Antigravity 的 MCP 設定中註冊 `anthro-bridge-mcp-server.exe`。
4. 在 Antigravity 中呼叫 `anthro-bridge/plan`（或透過 Workspace Rule 自動化）。
5. 詳細步驟請參閱 [Google Antigravity + Anthro Bridge MCP 設定指南](ANTIGRAVITY_MCP.zh-TW.md)。

---

## 文件

- [Claude Desktop / Cowork 3P Gateway 設定指南](THIRD_PARTY_INFERENCE.zh-TW.md)
- [Google Antigravity + Anthro Bridge MCP 設定指南](ANTIGRAVITY_MCP.zh-TW.md)
- [設定參考 (`config.json`)](CONFIGURATION.md)
- [提供者詳情與模型行為](PROVIDERS.md)
- [開發與驗證指南](DEVELOPMENT.md)

---

## 疑難排解

### 連接埠 4000 已被佔用
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 升級後設定還原
重啟應用程式以執行設定遷移。設定檔儲存於 `%APPDATA%\Anthro Bridge\config.json`。

### MCP Planner 呼叫失敗
請確保 Anthro Bridge 的 **MCP** 標籤頁中選擇的提供者已設定 API 金鑰，或已在 Windows 使用者環境變數中設定（例如 `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`）。MCP 不需要執行 3P Gateway。

---

## 授權條款

MIT License。詳見 [LICENSE](../LICENSE)。
