[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**以 Claude Code / Claude Desktop 作為程式碼執行框架，將推論請求路由至第三方 LLM API，並使用外部模型作為 Google Antigravity 的規劃器與審查器。**

Anthro Bridge 是一款專為 AI 輔助軟體開發設計的 Windows 配套應用程式，支援兩種互補的工作流程：

1. **Claude Code / Claude Desktop 的第三方閘道（3P Gateway）** — 保留 Claude 的程式庫探索、工具使用、檔案編輯與測試執行功能，同時將推論請求路由至第三方供應商。
2. **Google Antigravity 的 MCP 規劃器與審查器** — 透過 `anthro-bridge/plan` 和 `anthro-bridge/review` MCP 工具，將實作規劃與實作後審查委派給外部模型處理。

---

## 兩大主要工作流程

### 1. Claude Code / Claude Desktop 搭配第三方閘道

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **框架與模型分離**：保留 Claude 的智能代理工具，同時將推論請求路由至第三方供應商。
- **動態多配置檔路由**：可從 GUI 切換使用中的供應商、OpenRouter 配置檔及模型路由。
- **設定指南**：[Claude Desktop / Cowork 第三方閘道設定](THIRD_PARTY_INFERENCE.md)

### 2. Claude Code 專用本機 LLM（Ollama）

```text
Claude Code CLI
      ↓ (Loopback ANTHROPIC_BASE_URL)
Ollama Local (127.0.0.1:11434/v1)
      ↓
Gemma 4 / Qwen / Llama 3 / DeepSeek-R1（本機裝置端）
```

- **完全免費、離線與隱私**: 無需 API 金鑰，無用量限制，無雲端資料傳輸。
- **獨立路由與模型探索**: 獨立的 Claude Code 路由（`active_route: "ollama"`），支援 `/api/tags` 本機模型自動探索。
- **思考模式與上下文微調**: 支援為每個別名設定思考預算（1024 標記）與上下文長度（`num_ctx`）。
- **完全隔離**: Claude Desktop、Cowork on 3P 及 Google Antigravity MCP 繼續獨立使用雲端模型。
- **設定指南**: [Ollama Local 指南](OLLAMA_LOCAL.zh-TW.md)

### 3. Antigravity + MCP 規劃器 / 審查器 搭配 MCP 規劃器與審查器

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
已設定的外部模型（規劃器 / 審查器）
    ↓
實作計畫 / 審查結論
    ↓
Antigravity 使用訂閱容量
執行實作與測試
```

- **規劃與執行分工**：外部模型負責生成高階計畫或審查結論；Antigravity 訂閱容量負責執行耗費大量 Token 的程式碼編輯。
- **即時 GUI 設定**：切換規劃器或審查器的供應商、模型或推論強度，下次呼叫時立即生效。
- **設定指南**：[Google Antigravity + Anthro Bridge MCP 設定](ANTIGRAVITY_MCP.zh-TW.md)

**Antigravity 全域指令：**

- **`/anthro-plan`** — 將實作規劃委派給已設定的外部模型。
- **`/anthro-revise`** — 根據新的回饋或限制條件修改現有計畫。
- **`/anthro-review`** — 在提交前，對照已核准的計畫審查已完成的實作，並給出明確的 READY / NOT READY 結論。

**建議工作流程：**

```text
/anthro-plan → 實作與測試 → /anthro-review → 提交
```

---

## 支援的供應商

| 供應商 | 連線方式 | 支援系列 | 推論控制 |
|---|---|---|---|
| **DeepSeek** | 直接 API | DeepSeek V4.1 Flash、V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | 直接 API | kimi-for-coding、kimi-for-coding-highspeed | 思考模式 |
| **MiniMax** | 直接 API | MiniMax M3、M2.7 | 依模型而定 |
| **Kimi / Moonshot** | 直接 API | Kimi K2.x、Kimi K3 | 思考 / 推論強度 |
| **MiMo / Xiaomi** | 直接 API | MiMo V2.6 Flash、Pro、Pro-UltraSpeed（相容 V2.5） | Normal / Thinking |
| **OpenRouter** | 多配置檔閘道 | 請參閱下方 OpenRouter 章節 | 依模型 / 配置檔而定 |

### DeepSeek（直接連線）

內建的 **Direct DeepSeek** 預設路由：Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low。

- `deepseek-v4.1-flash` — 目前的旗艦推理模型（輸入 $0.27 / 1M · 輸出 $1.10 / 1M）。
- `deepseek-v4-pro-0813` — 不含擴充推理的高品質基礎模型（輸入 $0.27 / 1M · 輸出 $1.10 / 1M）。

### Kimi Code（直接連線）

專屬的程式碼專家 API（`KIMI_CODE_API_KEY`），與 Moonshot Kimi 分開：

- `kimi-for-coding` — 完整品質的程式碼模型。
- `kimi-for-coding-highspeed` — 低延遲變體。

### MiMo / Xiaomi（直接連線）

內建的 **Direct MiMo** 預設路由：
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- 預設模型：`mimo-v2.6-flash`

模型：`mimo-v2.6-flash`、`mimo-v2.6-pro`、`mimo-v2.6-pro-ultraspeed`（可選）。三者均支援 100 萬 Token 的上下文視窗及原生多模態能力（文字、圖像、影片）。

**Normal / Thinking**：MiMo 採用簡單的 Normal/Thinking 切換 —— 不支援推論強度等級。

**V2.5 向下相容**：已儲存的 `mimo-v2.5`、`mimo-v2.5-pro`、`mimo-v2.5-pro-ultraspeed` 路由將持續正常運作。未修改的舊版預設設定將於啟動時自動遷移至 V2.6。

### OpenRouter

支援多個命名配置檔。完整 OpenAI 模型目錄（單一下拉選單）：

| 模型 ID | 顯示名稱 |
|---|---|
| `openai/gpt-6-astra` | GPT-6 Astra |
| `openai/gpt-6-astra-pro` | GPT-6 Astra Pro |
| `openai/gpt-astra-latest` | GPT Astra Latest |
| `openai/gpt-5.6-sol` | GPT-5.6 Sol |
| `openai/gpt-5.6-sol-pro` | GPT-5.6 Sol Pro |
| `openai/gpt-5.6-terra` | GPT-5.6 Terra |
| `openai/gpt-5.6-terra-pro` | GPT-5.6 Terra Pro |
| `openai/gpt-5.6-luna` | GPT-5.6 Luna |
| `openai/gpt-5.6-luna-pro` | GPT-5.6 Luna Pro |

**GPT-6 Astra**：1.05M 上下文 · 推論強度：`low / medium / high / xhigh / max`。
**GPT-6 Astra Pro**：1.05M 上下文 · 永久啟用 Pro 推論（`reasoning.mode = pro`），無法由使用者選擇強度。
**GPT Astra Latest**：追蹤最新 Astra 系列模型的別名。

內建 **OpenRouter: chatGPT** 預設路由：Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium。

此外亦提供：**OpenRouter: Gemini**（Gemini 3.8 Flash · 推論強度 `low / medium / high`）、**OpenRouter: Poolside**、**OpenRouter: Tencent**、**OpenRouter: InclusionAI**、**OpenRouter: StepFun**。

---

## 模型定價（截至 v0.23.0）

| 模型 | 輸入 | 輸出 |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash（OpenRouter） | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## 安裝

從 [Releases](https://github.com/soheidon/anthro-bridge/releases) 頁面下載最新的 Windows 安裝程式（`Anthro Bridge_0.23.0_x64-setup.exe`）並執行。

安裝程式支援 8 種語言，升級時會保留現有的使用者設定。

---

## 快速開始

### 工作流程 1：Claude Code / Claude Desktop 的第三方閘道

1. 開啟 Anthro Bridge，前往 **Settings > API Key**，為所需的供應商設定 API 金鑰。
2. 在儀表板上選擇您的供應商或 OpenRouter 配置檔。
3. 點擊 **Start Gateway**（執行於 `http://127.0.0.1:4000`）。
4. 連接 Claude Code 或 Claude Desktop：
   - **Claude Code**：在 Settings 中點擊 **Copy Claude Code launch command**，並貼入 PowerShell。
   - **Claude Desktop / Cowork**：請參閱 [Claude Desktop 第三方設定指南](THIRD_PARTY_INFERENCE.md)。

### 工作流程 2：Google Antigravity 的 MCP 規劃器與審查器

1. 在 Anthro Bridge 中為您選擇的規劃器 / 審查器模型設定 API 金鑰。
2. 選擇 **MCP** 標籤，並在 **Settings > Antigravity > MCP Plan Settings** 中設定您的模型。
3. 在 Antigravity 的 MCP 設定中以 `["--mcp-server"]` 參數註冊 `anthro-bridge.exe`（或在 Anthro Bridge 中點擊 **Configure Automatically**）。
4. 使用 `/anthro-plan` 設計計畫、`/anthro-revise` 更新計畫，以及 `/anthro-review` 在提交前審查實作成果。
5. 請參閱完整的 [Antigravity MCP 設定指南](ANTIGRAVITY_MCP.zh-TW.md)。

---

## API 金鑰

| 供應商 | 環境變數 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## 文件

- [Claude Desktop / Cowork 第三方閘道設定](THIRD_PARTY_INFERENCE.md)
- [Google Antigravity + Anthro Bridge MCP 設定](ANTIGRAVITY_MCP.zh-TW.md)
- [設定參考（`config.json`）](CONFIGURATION.md)
- [供應商詳情與推論控制](PROVIDERS.md)
- [開發與驗證指南](DEVELOPMENT.md)

---

## 疑難排解

### 連接埠 4000 已被佔用
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 升級後設定還原
重新啟動應用程式以執行資料庫遷移。設定儲存於 `%APPDATA%\Anthro Bridge\config.json`。

### MCP 規劃器呼叫失敗
請確認在 **MCP** 標籤中所選供應商已設定 API 金鑰，或已在 Windows 使用者環境變數中匯出（例如 `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`）。MCP 執行時不需要啟動第三方閘道。

---

## 授權條款

MIT 授權。請參閱 [LICENSE](../LICENSE)。
