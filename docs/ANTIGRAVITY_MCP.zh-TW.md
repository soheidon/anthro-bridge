[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← 返回 Anthro Bridge README](README.zh-TW.md)

# 在 Google Antigravity 中使用 Anthro Bridge MCP

Anthro Bridge 內建了 Model Context Protocol (MCP) 伺服器，提供專門的 `plan` 規劃工具（`anthro-bridge/plan`）。這使得像 Google Antigravity 這樣的代理型編碼環境能夠將架構設計與實作規劃委託給外部大型語言模型（如 DeepSeek V4、MiMo、Kimi、MiniMax 或 OpenRouter 上的模型），同時使用 Antigravity 訂閱所包含的模型額度執行高 Token 消耗的檔案編輯、指令執行、建置與測試。

---

## 1. 該工作流程的運作方式

```text
Antigravity
    ↓
程式碼庫探索 (檢查相關檔案並擷取情境)
    ↓
anthro-bridge / plan (攜帶任務、情境與限制發起 MCP 呼叫)
    ↓
Anthro Bridge MCP 伺服器
    ↓
外部規劃器模型 (在 Anthro Bridge GUI 中設定)
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

1. 在 Windows 上安裝了 **Anthro Bridge**。
2. 已建置或可取得 **`anthro-bridge-mcp-server.exe`**（例如位於 `mcp-server/target/release/anthro-bridge-mcp-server.exe`）。
3. 已為計劃使用的規劃器模型設定了 **API 金鑰**。
4. **Google Antigravity** 已安裝並執行。

---

## 3. 在 Antigravity 中設定 MCP 伺服器

1. 開啟 Google Antigravity。
2. 導覽至：
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. 將 `anthro-bridge` 伺服器設定新增至 `mcpServers` 物件中：

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> 無需在 MCP 設定檔中以純文字寫入 API 金鑰。MCP 伺服器會自動讀取 Windows 使用者環境變數（如 `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`、`MOONSHOT_API_KEY`、`MINIMAX_API_KEY`、`XIAOMI_API_KEY`）或 Anthro Bridge 中儲存的設定。

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

1. 開啟 **Anthro Bridge** 桌面用戶端。
2. 選擇頂部的 **MCP** 標籤頁。
3. 選擇目前生效的規劃器 **提供者 (Provider)** 或 **設定檔 (Profile)**（如 DeepSeek、MiMo、OpenRouter 等）。
4. 開啟 **設定 (Settings)**（或 MCP Plan 詳細設定），設定以下參數：
   - **模型 (Model)**
   - **思考模式 (Thinking Mode)**
   - **推論強度 (Reasoning Effort)**
5. 儲存設定。

> [!NOTE]
> Anthro Bridge MCP 伺服器在每次呼叫 `plan()` 工具時都會動態讀取目前設定。在 GUI 中更改規劃器提供者或模型時，**無需**重啟 MCP 伺服器或 Antigravity。

---

## 6. 手動呼叫 plan 工具

您可以在 Antigravity 聊天中直接要求智慧代理呼叫規劃器：

```text
請調查這個專案，然後使用 anthro-bridge/plan MCP 工具制定實作計劃。先不要開始實作。
```

Antigravity 將探索相關檔案、總結關鍵架構情境、呼叫 `anthro-bridge/plan` 並呈現產生的實作計劃供您審閱。

---

## 7. 使用 Workspace Rule 自動化規劃呼叫

您可以透過在 [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) 建立工作區規則檔案，在進行複雜程式碼變更時自動觸發規劃器：

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. 典型自動化工作流程

```text
使用者：「重構功能 X 以支援多設定檔。」
    ↓
Antigravity 探索程式碼並總結情境
    ↓
Antigravity 自動觸發 anthro-bridge/plan 工具呼叫
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

- **獨立運作**：MCP 伺服器獨立於 Anthro Bridge 3P Gateway 運作。無需啟動 3P Gateway 即可使用 MCP 工具。
- **帳單分開**：呼叫 `anthro-bridge/plan` 會產生由相應提供者收取的 API 費用。隨後的檔案編輯與測試消耗 Antigravity 自身的訂閱額度。
- **即時生效**：在 Anthro Bridge GUI 中變更規劃器設定將在下一次 `plan()` 呼叫時立即生效。
