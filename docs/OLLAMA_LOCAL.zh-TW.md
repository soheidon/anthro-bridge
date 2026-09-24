[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | 中文(繁體) | [한국어](OLLAMA_LOCAL.ko.md) | [Français](OLLAMA_LOCAL.fr.md) | [Deutsch](OLLAMA_LOCAL.de.md) | [Español](OLLAMA_LOCAL.es.md)

[← 返回 Anthro Bridge README](../README.md)

# Claude Code + Ollama Local

Anthro Bridge 能夠將 Claude Code 路由至本機執行的 Ollama 模型，同時保持 Claude Desktop 和 MCP 使用的雲端提供者不變。

---

## 架構

```text
Claude Code
     │
     │ Anthro Bridge Claude Code 識別標記 (X-Anthro-Bridge-Client: claude-code)
     ▼
Anthro Bridge Gateway
     │
     ├─ Gateway 路由 (`active_route = "gateway"`) ──→ 全域雲端提供者 (DeepSeek / MiMo / OpenRouter 等)
     │
     └─ Ollama 路由  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                      本機模型
```

Ollama Local 專為 **Claude Code** 設計。它不會取代 Claude Desktop 或 Anthro Bridge MCP 工具使用的全域雲端提供者。

---

## 環境需求

1. **Anthro Bridge** 已安裝並啟動。
2. **Ollama** 已在本機電腦上安裝並啟動。
3. Ollama 中已下載至少一個模型（如 `gemma4:latest`、`llama3.3:70b`、`qwen2.5-coder:32b`），或具備有效的自訂模型標籤。
4. 使用 Anthro Bridge 產生的啟動指令啟動 Claude Code。

---

## 設定 Ollama Local

開啟 Anthro Bridge：
1. 前往 **設定 > API 金鑰**。
2. 在 API 金鑰表格下方找到 **Ollama Local** 設定卡片。

### 主要設定列

- **模型**: 選擇已探索的 Ollama 模型或自訂模型。
- **重新整理 (`🔄 重新整理`)**: 查詢本機 Ollama 的 `/api/tags` 端點以載入已安裝的模型。
- **Thinking**: 選擇 **一般 (停用)** 或 **Thinking (啟用)**。
- **在儀表板顯示**: 控制 Ollama Local 是否作為可選卡片顯示在儀表板上。

### 進階設定 (`▸ 進階設定`)

- **端點**: Ollama 的基礎 URL。預設值為 `http://127.0.0.1:11434`。
- **視覺 (Base64)**: 啟用或停用多模態本機模型的影像輸入（預設：停用）。
- **上下文視窗**: 選填的明確上下文 Token 容量（例如 `131072`）。
- **無需 API 金鑰**: 本機 loopback Ollama 實例無需 API 金鑰。

---

## 模型探索與重新整理

點擊 **重新整理 (`🔄 重新整理`)** 會向以下位址發送請求：

```http
GET http://127.0.0.1:11434/api/tags
```

### 安全與設定保留規則

- **僅限本機 loopback**: 僅查詢本機 loopback 位址（`127.0.0.1`、`localhost`、`[::1]`）。
- **非阻塞錯誤處理**: 若 Ollama 未啟動或請求逾時（2 秒上限），介面會顯示提示，已儲存的模型設定**絕不會被清除**。
- **自訂模型保留**: 如果已儲存的模型不在回傳清單中，下拉選單中仍會保持其選取狀態。
- **自訂模型標籤**: 選擇 `+ 自訂模型標籤...` 可手動輸入任意模型識別碼。

---

## 在儀表板上選取 Ollama

1. 在設定中啟用 **在儀表板顯示**。
2. 前往 **儀表板**。
3. 點擊 **Ollama Local** 卡片：
   - 設定 `claude_code.active_route = "ollama"`。
   - 全域 `active_provider`（如 `deepseek`）**保持不變**。
   - Ollama 卡片醒目提示為 **Claude Code 使用中**。
4. 若要切回雲端提供者路由：
   - 點擊儀表板上的任意雲端提供者卡片（如 DeepSeek 或 MiMo）。
   - Claude Code 自動重設為 `claude_code.active_route = "gateway"`。

---

## Thinking 行為規範

Anthro Bridge 將 Thinking 選取轉換為 Ollama 所需的 Anthropic 相容格式：

- **一般 (Normal)**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking (啟用)**:
  ```json
  "thinking": { "type": "enabled" }
  ```

*注意：Ollama Local 不提供 reasoning effort 等級選擇。*

---

## 上下文視窗與自動壓縮

- `context_window` 為**選填**參數。
- 指定後，Anthro Bridge 將其用於 Claude Code 上下文管理與自動壓縮（auto-compact）計算。
- 省略（`null`）時，Anthro Bridge 不會擅自推測上下文大小，而是採用預設規則。

---

## Claude Code 識別標記

Anthro Bridge 產生的 Claude Code 啟動指令透過 `ANTHROPIC_CUSTOM_HEADERS` 注入內部識別標記：

```text
X-Anthro-Bridge-Client: claude-code
```

- **請求標頭正規化**: 不分大小寫正規化標頭，合併或取代舊標記，並保留使用者的其他自訂請求標頭。
- **向上游轉發前移除**: Anthro Bridge 僅在本機將其用於路由判斷，在向上游提供者或 Ollama 轉發請求前會將其完全移除。

---

## 客戶端完全隔離

| 客戶端 | 路由選取器 | 目標 |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` 或 `"ollama"`) | 儀表板選取的提供者 / Ollama Local |
| **Claude Desktop** | `active_provider` (全域雲端提供者) | 雲端閘道 (DeepSeek, MiMo, OpenRouter 等) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | 專用的 MCP 規劃與審查提供者 |

此設計確保將 Claude Code 切換至 Ollama Local 時，絕不會影響 Claude Desktop 或 Antigravity MCP 的工作流程。

---

## 疑難排解

### 重新整理無法找到 Ollama
1. 確認 Ollama 正在背景或終端機中執行：
   ```powershell
   ollama list
   ```
2. 確認進階設定中的端點為 `http://127.0.0.1:11434`。

### 已儲存的模型未在清單中列出
即使 `/api/tags` 未回傳該模型，Anthro Bridge 也會保留已儲存的設定。您也可以透過 `+ 自訂模型標籤...` 手動輸入。

### Claude Code 仍在使用雲端提供者
1. 檢查儀表板，確認 **Ollama Local** 卡片處於啟用狀態。
2. 確保使用 Anthro Bridge 產生的啟動指令（`複製 Claude Code 啟動指令`）啟動 Claude Code。

### Claude Desktop 仍在使用雲端提供者
此為正常行為。Ollama Local 僅供 Claude Code 使用。
