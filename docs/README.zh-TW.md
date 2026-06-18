[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## 中文(繁體)

### 概述

Anthro Bridge 是一個代理 + GUI 管理工具，可將 Claude Desktop / Claude Code 的 API 請求路由到多個提供者的 Anthropic 相容端點。

Anthro Bridge 讀取每個請求中的 `model` 欄位，並自動路由到正確的上游提供者（基於模型的路由）。僅重寫 `model` 欄位 — messages、thinking blocks、tool_use、tool_result 和 streaming SSE 均原樣透傳。

Anthro Bridge 不是 Moon Bridge 的分支、GUI 版本或配套應用，而是一個獨立的 Anthropic 相容閘道。

### 支援的提供者

| 提供者 ID | 顯示名稱 | 上游端點 | 預設模型 |
|-----------|----------|----------|----------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

GUI 管理工具（Tauri v2 + React 19 + TypeScript）在原生 Windows 視窗中提供啟動/停止控制、配置編輯、日誌查看和 API 金鑰管理功能。

### 為什麼需要這個閘道

Claude Desktop / Claude Code 從根本上依賴 Anthropic 的 API 格式和 Claude 系列的模型名稱。即使 DeepSeek、MiniMax、Kimi、MiMo 等提供者提供了 Anthropic 相容的 API，Claude Desktop / Claude Code 也無法始終直接使用它們。

特別是 **Claude Desktop 的 `inferenceModels[].name` 只接受 Anthropic 官方模型名稱**。像 `claude-deepseek-v4` 或 `kimi-k2.6` 這樣的閘道自訂名稱會被拒絕，提示 `"not an Anthropic model"`。

為了解決這個限制，Anthro Bridge **始終向 Claude Desktop 展示 Anthropic 官方模型名稱（`claude-sonnet-4-6` / `claude-haiku-4-5`）作為"外殼"，而實際使用的 LLM（DeepSeek / MiniMax / Kimi / MiMo）則在 GUI 中選擇**。

```
Claude Desktop 側（始終固定）
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

閘道內部（根據 GUI 選擇）
  DeepSeek:      Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:          Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
  MiMo / Xiaomi: Sonnet -> mimo-v2.5-pro,      Haiku -> mimo-v2.5
```

這樣可以在通過 Claude Desktop 的模型名稱驗證的同時，自由切換 DeepSeek、MiniMax、Kimi 和 MiMo。

### 先決條件

- **Windows 10/11**（支援日語環境）
- 所選提供者的 API 金鑰（DeepSeek / MiniMax / Kimi / MiMo — **只需一個即可**，自 v0.5.0 起）

### 快速開始

#### 1. 安裝

從 [Releases](https://github.com/soheidon/anthro-bridge/releases) 下載最新安裝程式並執行。

安裝程式啟動時會顯示語言選擇介面（可選 English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español）。

#### 更新

直接執行新的 `setup.exe` 即可 — 安裝程式會自動偵測並替換舊版本，無需手動解除安裝。您的設定（`%APPDATA%\Anthro Bridge\config.json`）在更新後會被保留。

#### 2. 設定 API 金鑰

設定（⚙）-> **API 金鑰** 分頁，輸入提供者的 API 金鑰並點擊 **儲存**。
金鑰將持久保存為 Windows 使用者環境變數。

| 提供者 | 環境變數 | 備註 |
|----------|---------------------|------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. 選擇提供者

儀表板中的 **選擇 LLM 提供者** 卡片為 2×2 網格佈局：

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

點擊磁貼選擇要使用的提供者。

#### 4. 啟動閘道

點擊標題欄中的 **Start Gateway** 按鈕。代理將在 `http://127.0.0.1:4000` 上啟動。

#### 5. 配置 Claude Desktop / Cowork on 3P

詳細步驟請參閱 [THIRD_PARTY_INFERENCE.zh-TW.md](THIRD_PARTY_INFERENCE.zh-TW.md)。

### 端點

| 方法 | 路徑 | 說明 |
|--------|------|------|
| GET | `/health` | 健康檢查 |
| GET | `/v1/models` | 公開模型列表 |
| POST | `/v1/messages` | Messages API（stream + non-stream）。基於模型的路由 |
| POST | `/v1/messages/count_tokens` | Token 計數（僅支援的提供者） |

### 路由

基於模型的路由：每個請求中的 `model` 欄位決定目標提供者和上游模型。

| Anthropic 模型 | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|-----------------|----------|---------|------|---------------|
| `claude-sonnet-4-6` | `deepseek-v4-pro` | `MiniMax-M3` | `kimi-k2.7-code` | `mimo-v2.5-pro` (Thinking 開啟) |
| `claude-haiku-4-5` | `deepseek-v4-flash` | `MiniMax-M3` | `kimi-k2.6` (Thinking 關閉) | `mimo-v2.5` |

#### MiMo 路由詳情

- **`claude-sonnet-4-6` → `mimo-v2.5-pro`**: Thinking **預設開啟**（`thinking_mode: "thinking"`）。MiMo 的 thinking 控制使用 `thinking_mode` 鍵（非 `thinking`）。設為 `"default"` 可切換為標準模式。
- **`claude-haiku-4-5` → `mimo-v2.5`**: 支援影像 URL 和 base64 影像透傳。Anthro Bridge 不支援 MiMo 的音訊/視訊輸入。
- **`claude-sonnet-4-6` 路由不支援影像。** 發送到此路由的影像將被替換為文字佔位符（`non_vision_image_policy: "replace"`）。
- **上游端點**: 請求發送至 `https://api.xiaomimimo.com/anthropic/v1/messages`。

### 語言

8 種語言：English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español。

要新增翻譯，將語言檔案（如 `es.ts`）放入 `gui/src/i18n/lang/` 並重新構建。
詳見 [CONTRIBUTING](CONTRIBUTING.md)。

### 配置 (config.json)

提供者設定定義了每個模型的上游模型名稱和功能標誌。通常無需編輯。
進階使用者可透過設定（⚙）-> **閘道配置** 進行編輯。

| 鍵 | 說明 |
|-----|------|
| `models.<model>.upstream_model` | 發送到上游的實際模型名稱（必填） |
| `models.<model>.thinking` | 當設為 `"disabled"` 時注入 thinking 抑制（可選）。對於 MiMo 請使用 `thinking_mode` |
| `models.<model>.thinking_mode` | MiMo 專用：`"thinking"`（開啟）或 `"default"`（標準）。仅供 MiMo 提供者使用 |
| `models.<model>.supports_vision` | 按模型的影像支援（預設回退到提供者設定） |
| `models.<model>.supports_video` | 按模型的視訊支援（預設回退到提供者設定） |
| `models.<model>.visible` | 是否在 `/v1/models` 和儀表板中顯示（預設 `true`） |
| `non_vision_image_policy` | 非 Vision 模型的影像處理: `replace`（佔位符）/ `drop`（刪除）/ `reject`（錯誤） |

### 專案結構

```
anthro-bridge/
├── README.md
├── SPEC.md                    規格說明
├── docs/
│   ├── README.ja.md           日語
│   ├── README.zh-CN.md        中文(简体)
│   ├── README.zh-TW.md        中文(繁體)
│   ├── README.ko.md           韓語
│   ├── README.fr.md           法語
│   ├── README.de.md           德語
│   ├── README.es.md           西班牙語
│   ├── THIRD_PARTY_INFERENCE.md   第三方推理指南
│   └── THIRD_PARTY_INFERENCE.*.md
├── LICENSE                    MIT 許可證
├── config.json                提供者配置
├── .gitignore
└── gui/
    ├── src/                   React 前端 (TypeScript)
    │   ├── components/        UI 組件
    │   ├── hooks/             自訂 Hooks
    │   └── i18n/              多語言支援
    │       └── lang/          語言檔案 (en, ja, zh-CN, zh-TW, ko, fr, de, es)
    ├── src-tauri/             Tauri 後端 (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 個 Tauri 命令 + 代理生命週期
    │   │   ├── main.rs        入口點
    │   │   └── proxy.rs       axum 代理伺服器
    │   ├── resources/
    │   │   └── config.json    打包配置
    │   └── Cargo.toml
    └── package.json
```

### 開發構建

```bash
cd gui
npm install
npm run tauri build    # 生產構建
npm run tauri dev      # 開發模式 (HMR)
```

需要 [Rust](https://rustup.rs/) stable 工具鏈和 Node.js 24+。

### 故障排除

#### 端口 4000 被佔用

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### 影像/視訊被拒絕

DeepSeek 不支援影像或視訊。影像會自動替換為佔位符文字（`non_vision_image_policy: "replace"`）。要原生使用影像，請切換到 MiniMax、Kimi 或 MiMo（`claude-haiku-4-5` 路由）。

MiMo 的 `claude-sonnet-4-6` 路由同樣不支援影像 — 影像任務請使用 `claude-haiku-4-5` 路由。視訊始終被拒絕。

#### MiMo：現有用戶配置未生效

如果從 v0.9.0 之前的版本升級，保存的用戶配置可能仍保留舊的 `"display_name": "MiMo"` 或 `"thinking": "default"` 值。v0.9.0 會在首次啟動時自動遷移這些設定，但如果遇到問題：

1. **重啟應用** — 自動遷移在啟動時運行。
2. **重置配置**：刪除 `%APPDATA%\Anthro Bridge\config.json` 並重啟。將使用包含正確 MiMo 設定的捆綁配置。
3. **手動檢查**：打開 `%APPDATA%\Anthro Bridge\config.json`，確認 `providers.mimo` 中有 `"display_name": "MiMo / Xiaomi"`、`"api_key_env": "XIAOMI_API_KEY"`，以及模型條目中使用 `thinking_mode`（而非 `thinking`）。

### 手動測試 — MiMo / Xiaomi

#### 純文字測試（claude-sonnet-4-6 → mimo-v2.5-pro）

1. 在設定 → API 金鑰分頁中設定 `XIAOMI_API_KEY` 並儲存。
2. 在儀表板上選擇 **MiMo / Xiaomi**。
3. 啟動閘道。
4. 透過 Claude Desktop 發送消息。驗證回應是否包含 thinking blocks。

#### 影像測試（claude-haiku-4-5 → mimo-v2.5）

1. 在儀表板上選擇 **MiMo / Xiaomi**。
2. 在 Claude Desktop 中附加影像並發送消息。
3. 驗證影像是否被正確識別和描述。
4. 發送影像到 `claude-sonnet-4-6` 時應被替換為文字佔位符。

#### 驗證

檢查 GUI 的 Log 面板 — 請求的 `model` 欄位應根據路由被重寫為 `mimo-v2.5-pro` 或 `mimo-v2.5`。

### 許可證

MIT — 詳見 [LICENSE](LICENSE)。
