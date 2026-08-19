[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← 返回 Anthro Bridge README](../README.zh-TW.md)

# 在 Google Antigravity 中使用 Anthro Bridge MCP

Anthro Bridge 不需要單獨的 MCP 伺服器執行檔。安裝的單一 `anthro-bridge.exe` 同時提供桌面 GUI 應用程式與 MCP 伺服器功能。Antigravity 透過附加 `--mcp-server` 參數啟動同一執行檔來進入 MCP 模式。

```text
一般啟動
anthro-bridge.exe
→ Anthro Bridge 桌面應用程式 / 3P Gateway

MCP啟動
anthro-bridge.exe --mcp-server
→ 面向 Antigravity 的無前端 stdio MCP 伺服器
```

這使得 Google Antigravity 等代理型編碼環境能夠將架構設計與實作規劃委託給外部大型語言模型（如 DeepSeek V4、MiMo、Kimi、MiniMax 或 OpenRouter 模型）透過 `anthro-bridge/plan` 完成，而實際的高 Token 消耗程式碼編輯、指令執行、建置與測試則使用 Antigravity 訂閱所包含的模型額度。

---

## 1. 該工作流程的運作方式

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
設定的外部規劃器模型
    ↓
返回結構化實作計劃
    ↓
Antigravity 使用訂閱額度
執行編輯、建置與測試
```

- **外部 API**：僅負責根據相關程式庫情境產生實作計劃（由相應提供者按量計費）。
- **Antigravity 訂閱**：負責繁重的檔案讀寫、程式碼編輯、工具呼叫與測試執行循環。
- **職責分離**：享受高智能外部模型規劃帶來的優勢，而不會在常規程式碼編寫中消耗昂貴的外部 API Token。

---

## 2. 先決條件

1. 在 Windows 上安裝 **Anthro Bridge**。
2. 為想要用於規劃的提供者設定驗證資訊（在 Anthro Bridge 內設定或設定系統環境變數）。
3. 安裝並執行 **Google Antigravity**。

---

## 3. 在 Antigravity 中設定 MCP 伺服器

### 方法 1 — 透過 Anthro Bridge GUI 設定（推薦）

1. 開啟 Anthro Bridge，進入頂部導覽的 **設定**（`[設定]` 標籤頁）> 左側子導覽的 **Antigravity**。
2. 檢視 **Google Antigravity 整合** 卡片：
   - **目標執行檔**：預設顯示目前正在執行的 `anthro-bridge.exe` 路徑。若想使用其他二進位檔（如可攜版或自訂建置版），點擊 **變更** (`antigravity.btnChangeExe`) 選擇執行檔。
   - **註冊 / 更新**：點擊 **更新 Antigravity 設定** (`antigravity.btnUpdate`)，即可在保留 `%USERPROFILE%\.gemini\config\mcp_config.json` 中其他 MCP 伺服器設定的前提下，安全地註冊或更新 `anthro-bridge`。
   - **移除註冊**：若需從 Antigravity 中移除，點擊 **移除設定** (`antigravity.btnRemove`)。
   - **檢視設定資料夾**：點擊 **開啟設定資料夾** (`antigravity.btnOpenFolder`) 可直接在 Windows 檔案總管中開啟該目錄。

---

### 方法 2 — 手動設定（進階）

1. 在 Anthro Bridge **設定 > Antigravity** 中點擊 **開啟設定資料夾**，在 Windows 檔案總管中開啟 `%USERPROFILE%\.gemini\config\`。
2. 開啟 `mcp_config.json`，在 `mcpServers` 物件中新增 `anthro-bridge` 設定：

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

開發建置版本可直接指向 Release 路徑：
```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

> [!TIP]
> 無需在 Antigravity 的 `mcp_config.json` 中寫入提供者 API 金鑰。MCP 伺服器會利用 Anthro Bridge 現有的憑證解析機制（從 Windows 使用者環境變數如 `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`、`MOONSHOT_API_KEY`、`MINIMAX_API_KEY`、`XIAOMI_API_KEY` 或已儲存的應用程式設定中自動讀取）。

---

## 4. 驗證 MCP 連線

在 Antigravity 的 **Installed MCP Servers** 介面中確認 `anthro-bridge` 已被辨識：

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. 在 Anthro Bridge 中設定規劃器模型

Anthro Bridge 明確劃分了規劃器選擇與詳細參數管理的職責：

1. **頂層 `MCP` 標籤頁 (`MCP for Antigravity`)**：
   - 顯示可用提供者（DeepSeek、OpenRouter、MiniMax、MiMo、Kimi）與設定檔的卡片清單。
   - 點擊卡片即可立即切換生效的規劃器目標。
2. **`設定` > `Antigravity`**：
   - **MCP Plan 詳細設定** 卡片：按提供者/設定檔詳細設定所選模型、思考模式 (Thinking Mode) 及推論強度 (Reasoning Effort)。
   - **Google Antigravity 整合** 卡片：管理 MCP 伺服器註冊狀態以及 Antigravity Commands（全域技能）。

> [!NOTE]
> Anthro Bridge MCP 伺服器在每次呼叫 `plan()` 工具時都會動態讀取目前設定。在 GUI 中變更規劃器提供者或模型參數時，**無需**重啟 MCP 伺服器或 Antigravity。

---

## 6. 使用 Antigravity Commands (`/anthro-plan` & `/anthro-revise`)（推薦）

在 **設定 > Antigravity** 的 **Google Antigravity 整合** 卡片中安裝全域技能後，可在所有 Antigravity 工作區中使用斜線指令：

- 點擊 **全部安裝** (`antigravity.btnInstallAll`) 或點擊指令旁的 **安裝** (`antigravity.commandBtnInstall`)。

### 建立新實作計畫:
```text
/anthro-plan <要實作的課題或功能描述>
```
*收集程式庫情境，呼叫 `anthro-bridge/plan`，在展示計畫後安全停止，不進行檔案修改或執行建置指令。*

### 修訂現有實作計畫:
```text
/anthro-revise <要反映的回饋或變更需求>
```
*從作用中情境或 `implementation_plan.md` 中確定目前實作計畫，連同回饋一起傳遞給 `anthro-bridge/plan` 進行修訂，同時保留未受影響的部分。*

> [!IMPORTANT]
> 透過 `/anthro-plan` 或 `/anthro-revise` 執行時，由指令自身管理單次 planner 呼叫，Workspace Rule 不會觸發額外的重複 planner 呼叫。

---

## 7. 透過 Workspace Rule 自動化計畫流程

可以在專案中放置如 [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) 的 Workspace Rule，在複雜編碼任務時自動化呼叫外部規劃器：

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
   Note: "Exactly once" means duplicate planner calls are prohibited once a successful usable result is obtained. If the tool call itself fails or returns an unusable response (e.g. transport or decoding error), exactly 1 recovery retry is permitted.
5. Use the returned DeepSeek plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

### 觸發策略:
- **微小 / 局部任務 (Trivial / localized tasks)**（如修改錯字、單行微調、語法調整）：不會觸發規劃器。
- **非普通任務 (Non-trivial tasks)**（架構設計變更、跨多檔案功能實作、複雜除錯等）：Antigravity 會調查程式庫情境，呼叫 1 次 `anthro-bridge/plan`，並根據返回的計畫進行實作。

---

## 8. 典型自動化工作流程

```text
使用者：「重構功能 X 以支援多設定檔。」
    ↓
Antigravity 探索程式碼並總結情境
    ↓
Antigravity 自動觸發 anthro-bridge/plan 工具呼叫 (僅1次)
    ↓
Anthro Bridge 向選定的外部模型發送請求
    ↓
Antigravity 接收結構化實作計劃
    ↓
使用者審閱並批准計劃
    ↓
Antigravity 執行程式碼編輯、執行測試並驗證修改
```

---

## 9. 重要說明

- **獨立運作**：MCP 伺服器完全獨立於 Anthro Bridge 3P Gateway 運作。無需啟動（開啟）3P Gateway 即可使用 MCP 工具。
- **帳單分開**：呼叫 `anthro-bridge/plan` 會產生由相應提供者收取的 API 費用。隨後的檔案編輯與測試消耗 Antigravity 自身的訂閱額度。
- **即時生效**：在 Anthro Bridge GUI 中變更規劃器提供者或模型參數將在下一次 `plan()` 呼叫時立即生效。
