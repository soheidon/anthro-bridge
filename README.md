# Terra Bridge

## English

### Overview

Terra Bridge is a proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

Terra Bridge reads the `model` field from each request and automatically routes to the correct upstream provider (model-based routing). Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

The GUI management tool (Tauri v2 + React 19 + TypeScript) provides start/stop control, config editing, log viewing, and API key management from a native Windows window.

### Why This Gateway Is Needed

Claude Desktop / Claude Code fundamentally expects Anthropic's API format and Claude-family model names. Even when providers like DeepSeek, MiniMax, and Kimi offer Anthropic-compatible APIs, Claude Desktop / Claude Code cannot always use them directly.

In particular, **Claude Desktop's `inferenceModels[].name` only accepts Anthropic official model names**. Gateway custom names like `claude-deepseek-v4` or `kimi-k2.6` are rejected as `"not an Anthropic model"`.

To work around this constraint, Terra Bridge **presents Anthropic official model names (`claude-sonnet-4-6` / `claude-haiku-4-5`) as "shells" to Claude Desktop, while the actual LLM (DeepSeek / MiniMax / Kimi) is selected in the GUI**.

```
Claude Desktop side (always fixed)
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

Gateway internal (based on GUI selection)
  DeepSeek:  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:   Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:      Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
```

This lets you pass Claude Desktop's model name validation while freely switching between DeepSeek, MiniMax, and Kimi.

### Prerequisites

- **Windows 10/11** (Japanese locale supported)
- API key for your chosen provider (DeepSeek / MiniMax / Kimi — **just one is enough**, since v0.5.0)

### Quick Start

#### 1. Install

Download the latest installer from [Releases](https://github.com/soheidon/terra-bridge/releases) and run it.

The installer shows a language selection screen on launch (choose from English, 日本語, 中文(简体), 中文(繁體), 한국어, Français).

#### 2. Set API Key

Settings (⚙) -> **API Key** tab, enter your provider's API key and click **Save**.
The key is persisted as a Windows user environment variable.

| Provider | Environment Variable |
|----------|---------------------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 3. Select Provider

On the Dashboard, click a provider tile (DeepSeek / MiniMax / Kimi) under **Select LLM Provider**.

#### 4. Start Gateway

Click **Start Gateway** in the header. The proxy starts on `http://127.0.0.1:4000`.

#### 5. Configure Claude Desktop

Settings (⚙) -> **Claude Desktop Setup** tab:

1. Click "Copy Claude Desktop Config"
2. In Claude Desktop, click "Open Config File"
3. Delete existing content, paste the copied settings

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-sonnet-4-6", "labelOverride": "Sonnet 4.6" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Haiku 4.5" }
  ]
}
```

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/models` | Public model list |
| POST | `/v1/messages` | Messages API (stream + non-stream). Model-based routing |
| POST | `/v1/messages/count_tokens` | Token counting (supported providers only) |

### Routing

Model-based routing: the `model` field in each request determines the target provider and upstream model.

### Languages

6 languages: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français.

To add a new translation, drop a language file (e.g., `es.ts`) into `gui/src/i18n/lang/` and rebuild.
See [CONTRIBUTING](CONTRIBUTING.md) for details.

### Configuration (config.json)

Provider settings define upstream model names and capability flags per model. Normally no editing is required.
Advanced users can edit via Settings (⚙) -> **Gateway Config**.

| Key | Description |
|-----|-------------|
| `models.<model>.upstream_model` | Actual model name sent to upstream (required) |
| `models.<model>.thinking` | When `"disabled"`, injects thinking suppression (optional) |
| `models.<model>.supports_vision` | Per-model image support (falls back to provider default) |
| `models.<model>.supports_video` | Per-model video support (falls back to provider default) |
| `models.<model>.visible` | Whether to expose in `/v1/models` and dashboard (default `true`) |
| `non_vision_image_policy` | Image handling for non-vision models: `replace` (placeholder) / `drop` / `reject` (error) |

### Project Structure

```
terra-bridge/
├── README.md
├── SPEC.md                    Specification (EN/JA/ZH-CN)
├── LICENSE                    MIT License
├── config.json                Provider configuration
├── .gitignore
└── gui/
    ├── src/                   React frontend (TypeScript)
    │   ├── components/        UI components
    │   ├── hooks/             Custom hooks
    │   └── i18n/              Multi-language support
    │       └── lang/          Language files (en, ja, zh-CN, zh-TW, ko, fr)
    ├── src-tauri/             Tauri backend (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 Tauri commands + proxy lifecycle
    │   │   ├── main.rs        Entry point
    │   │   └── proxy.rs       axum proxy server
    │   ├── resources/
    │   │   └── config.json    Bundled configuration
    │   └── Cargo.toml
    └── package.json
```

### Dev Build

```bash
cd gui
npm install
npm run tauri build    # Production build
npm run tauri dev      # Dev mode (HMR)
```

Requires [Rust](https://rustup.rs/) stable toolchain and Node.js 24+.

### Troubleshooting

#### Port 4000 in use

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Image/video rejected

DeepSeek does not support images or video. Images are automatically replaced with placeholder text (`non_vision_image_policy: "replace"`). To use images natively, switch to MiniMax or Kimi. Video is always rejected.

### License

MIT — see [LICENSE](LICENSE) for details.

---

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール「Terra Bridge」です。

Anthropic Messages API リクエストの `model` フィールドを読み取り、対応する upstream へ自動振り分け（モデルベースルーティング）。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

GUI 管理ツール（Tauri v2 + React 19 + TypeScript）でプロキシの起動・停止、設定編集、ログ確認、API キー管理が可能です。

### なぜこのゲートウェイが必要か

Claude Desktop / Claude Code は、基本的にAnthropicのAPI形式とClaude系のモデル名を前提に動作します。そのため、DeepSeek、MiniMax、Kimi などがAnthropic互換APIを提供していても、Claude Desktop / Claude Code からそれらを直接指定して常に利用できるとは限りません。

特に **Claude Desktop の `inferenceModels[].name` には Anthropic 公式モデル名しか指定できません**。`claude-deepseek-v4` や `kimi-k2.6` のようなゲートウェイ独自名は `"not an Anthropic model"` として弾かれます。

Terra Bridge はこの制約を回避するため、**Claude Desktop には常に Anthropic 公式モデル名（`claude-sonnet-4-6` / `claude-haiku-4-5`）を「器」として見せ、実際に使う LLM（DeepSeek / MiniMax / Kimi）は GUI で切り替える**設計を採用しています。

```
Claude Desktop 側（常に固定）
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

ゲートウェイ内部（GUI の選択による）
  DeepSeek 選択時:  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax 選択時:   Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi 選択時:      Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
```

これにより、Claude Desktop のモデル名検証を通過しつつ、DeepSeek / MiniMax / Kimi を自由に切り替えられます。

### 必要環境

- **Windows 10/11**（日本語環境対応）
- 利用するプロバイダーの API キー（DeepSeek / MiniMax / Kimi **いずれか1つでOK**、v0.5.0以降）

### クイックスタート

#### 1. インストール

[Releases](https://github.com/soheidon/terra-bridge/releases) から最新のインストーラーをダウンロードして実行。

インストーラー起動時に言語選択画面が表示されます（English, 日本語, 中文(简体), 中文(繁體), 한국어, Français から選択可）。

#### 2. API キー設定

設定（⚙）-> **API キー** タブで、使用するプロバイダーの API キーを入力し「保存」をクリック。
Windows ユーザー環境変数に永続保存されます。

| プロバイダー | 環境変数 |
|-------------|---------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 3. プロバイダ選択

ダッシュボードの **LLMプロバイダ選択** カードで使用するプロバイダ（DeepSeek / MiniMax / Kimi）をクリック。

#### 4. プロキシ起動

ヘッダーの **Start Gateway** ボタンをクリック。プロキシが `http://127.0.0.1:4000` で起動します。

#### 5. Claude Desktop 設定

設定（⚙）-> **Claude Desktop 設定** タブで：

1. 「Claude Desktop設定をコピー」をクリック
2. Claude Desktop の「設定ファイルを開く」をクリック
3. 既存の内容を削除し、コピーした設定を貼り付け

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-sonnet-4-6", "labelOverride": "Sonnet 4.6" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Haiku 4.5" }
  ]
}
```

### エンドポイント

| Method | Path | 説明 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | 公開モデル一覧 |
| POST | `/v1/messages` | Messages API（stream/non-stream）。モデルベースルーティング |
| POST | `/v1/messages/count_tokens` | トークン数カウント（対応プロバイダーのみ） |

### ルーティング

モデルベースルーティング: リクエストの `model` フィールドを読み取り、対応するプロバイダーと upstream モデルに自動振り分け。

### 言語

6言語対応: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français。

新しい翻訳を追加するには `gui/src/i18n/lang/` に言語ファイル（例: `es.ts`）を追加して再ビルドするだけです。
詳しくは [CONTRIBUTING](CONTRIBUTING.md) を参照。

### 設定 (config.json)

プロバイダー設定は各モデルの上流モデル名や機能フラグを定義します。通常は編集不要です。
上級者向けの詳細設定は GUI の設定（⚙）-> **Gateway Config** から行えます。

| キー | 説明 |
|-----|------|
| `models.<model>.upstream_model` | upstream へ送る実モデル名（必須） |
| `models.<model>.thinking` | `"disabled"` 時のみ thinking 抑制注入（省略可） |
| `models.<model>.supports_vision` | モデル単位の画像サポート（省略時はプロバイダー既定値） |
| `models.<model>.supports_video` | モデル単位の動画サポート（省略時はプロバイダー既定値） |
| `models.<model>.visible` | `/v1/models` とダッシュボードに表示するか（デフォルト `true`） |
| `non_vision_image_policy` | 非Visionモデルの画像処理: `replace`（プレースホルダ）/ `drop`（削除）/ `reject`（エラー） |

### プロジェクト構成

```
terra-bridge/
├── README.md
├── SPEC.md                    仕様書（EN/JA/ZH-CN）
├── LICENSE                    MIT License
├── config.json                プロバイダー設定
├── .gitignore
└── gui/
    ├── src/                   React フロントエンド (TypeScript)
    │   ├── components/        UI コンポーネント
    │   ├── hooks/             カスタムフック
    │   └── i18n/              多言語対応
    │       └── lang/          言語ファイル (en, ja, zh-CN, zh-TW, ko, fr)
    ├── src-tauri/             Tauri バックエンド (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 Tauri コマンド + プロキシライフサイクル
    │   │   ├── main.rs        エントリーポイント
    │   │   └── proxy.rs       axum プロキシサーバー本体
    │   ├── resources/
    │   │   └── config.json    バンドル設定
    │   └── Cargo.toml
    └── package.json
```

### 開発

```bash
cd gui
npm install
npm run tauri build    # プロダクションビルド
npm run tauri dev      # 開発モード (HMR)
```

[Rust](https://rustup.rs/) stable ツールチェーンと Node.js 24+ が必要です。

### トラブルシュート

#### ポート 4000 が使用中

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### 画像/動画が拒否される

DeepSeek は画像・動画に対応していません。画像が送信された場合は自動的にプレースホルダテキストに置換されます（`non_vision_image_policy: "replace"`）。画像をそのまま使いたい場合は MiniMax または Kimi を選択してください。動画は常に拒否されます。

### ライセンス

MIT — 詳細は [LICENSE](LICENSE) を参照。

---

## 中文(简体)

### 概述

Terra Bridge 是一个代理 + GUI 管理工具，可将 Claude Desktop / Claude Code 的 API 请求路由到多个提供商的 Anthropic 兼容端点。

Terra Bridge 读取每个请求中的 `model` 字段，并自动路由到正确的上游提供商（基于模型的路由）。仅重写 `model` 字段 — messages、thinking blocks、tool_use、tool_result 和 streaming SSE 均原样透传。

GUI 管理工具（Tauri v2 + React 19 + TypeScript）在原生 Windows 窗口中提供启动/停止控制、配置编辑、日志查看和 API 密钥管理功能。

### 为什么需要这个网关

Claude Desktop / Claude Code 从根本上依赖 Anthropic 的 API 格式和 Claude 系列的模型名称。即使 DeepSeek、MiniMax、Kimi 等提供商提供了 Anthropic 兼容的 API，Claude Desktop / Claude Code 也无法始终直接使用它们。

特别是 **Claude Desktop 的 `inferenceModels[].name` 只接受 Anthropic 官方模型名称**。像 `claude-deepseek-v4` 或 `kimi-k2.6` 这样的网关自定义名称会被拒绝，提示 `"not an Anthropic model"`。

为了解决这个限制，Terra Bridge **始终向 Claude Desktop 展示 Anthropic 官方模型名称（`claude-sonnet-4-6` / `claude-haiku-4-5`）作为"外壳"，而实际使用的 LLM（DeepSeek / MiniMax / Kimi）则在 GUI 中选择**。

```
Claude Desktop 侧（始终固定）
  Sonnet 4.6   = claude-sonnet-4-6
  Haiku 4.5 = claude-haiku-4-5

网关内部（根据 GUI 选择）
  DeepSeek:  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:   Sonnet -> MiniMax-M3,           Haiku -> MiniMax-M3
  Kimi:      Sonnet -> kimi-k2.7-code,      Haiku -> kimi-k2.6 (thinking disabled)
```

这样可以在通过 Claude Desktop 的模型名称验证的同时，自由切换 DeepSeek、MiniMax 和 Kimi。

### 运行环境

- **Windows 10/11**（支持日语环境）
- 所选提供商的 API 密钥（DeepSeek / MiniMax / Kimi — **只需一个即可**，自 v0.5.0 起）

### 快速开始

#### 1. 安装

从 [Releases](https://github.com/soheidon/terra-bridge/releases) 下载最新安装程序并运行。

安装程序启动时会显示语言选择界面（可选 English, 日本語, 中文(简体), 中文(繁體), 한국어, Français）。

#### 2. 设置 API 密钥

设置（⚙）-> **API 密钥** 选项卡，输入提供商的 API 密钥并点击 **保存**。
密钥将持久保存为 Windows 用户环境变量。

| 提供商 | 环境变量 |
|----------|---------------------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 3. 选择提供商

在仪表板的 **选择 LLM 提供商** 卡片中点击提供商磁贴（DeepSeek / MiniMax / Kimi）。

#### 4. 启动网关

点击标题栏中的 **Start Gateway** 按钮。代理将在 `http://127.0.0.1:4000` 上启动。

#### 5. 配置 Claude Desktop

设置（⚙）-> **Claude Desktop 设置** 选项卡：

1. 点击"复制 Claude Desktop 配置"
2. 在 Claude Desktop 中点击"打开配置文件"
3. 删除现有内容，粘贴复制的设置

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-sonnet-4-6", "labelOverride": "Sonnet 4.6" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Haiku 4.5" }
  ]
}
```

### 端点

| Method | Path | 说明 |
|--------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/v1/models` | 公开模型列表 |
| POST | `/v1/messages` | Messages API（stream + non-stream）。基于模型的路由 |
| POST | `/v1/messages/count_tokens` | Token 计数（仅支持的提供商） |

### 路由

基于模型的路由：每个请求中的 `model` 字段决定目标提供商和上游模型。

### 语言

支持 6 种语言：English, 日本語, 中文(简体), 中文(繁體), 한국어, Français。

要添加新翻译，只需将语言文件（如 `es.ts`）放入 `gui/src/i18n/lang/` 并重新构建。
详见 [CONTRIBUTING](CONTRIBUTING.md)。

### 配置 (config.json)

提供商设置定义了每个模型的上游模型名称和功能标志。通常无需编辑。
高级用户可通过设置（⚙）-> **Gateway Config** 进行编辑。

| 键 | 说明 |
|-----|------|
| `models.<model>.upstream_model` | 发送到上游的实际模型名称（必填） |
| `models.<model>.thinking` | 当设为 `"disabled"` 时注入 thinking 抑制（可选） |
| `models.<model>.supports_vision` | 按模型的图像支持（默认回退到提供商设置） |
| `models.<model>.supports_video` | 按模型的视频支持（默认回退到提供商设置） |
| `models.<model>.visible` | 是否在 `/v1/models` 和仪表板中显示（默认 `true`） |
| `non_vision_image_policy` | 非 Vision 模型的图像处理: `replace`（占位符）/ `drop`（删除）/ `reject`（错误） |

### 项目结构

```
terra-bridge/
├── README.md
├── SPEC.md                    规格说明 (EN/JA/ZH-CN)
├── LICENSE                    MIT 许可证
├── config.json                提供商配置
├── .gitignore
└── gui/
    ├── src/                   React 前端 (TypeScript)
    │   ├── components/        UI 组件
    │   ├── hooks/             自定义 Hooks
    │   └── i18n/              多语言支持
    │       └── lang/          语言文件 (en, ja, zh-CN, zh-TW, ko, fr)
    ├── src-tauri/             Tauri 后端 (Rust)
    │   ├── src/
    │   │   ├── lib.rs         24 个 Tauri 命令 + 代理生命周期
    │   │   ├── main.rs        入口点
    │   │   └── proxy.rs       axum 代理服务器
    │   ├── resources/
    │   │   └── config.json    打包配置
    │   └── Cargo.toml
    └── package.json
```

### 开发构建

```bash
cd gui
npm install
npm run tauri build    # 生产构建
npm run tauri dev      # 开发模式 (HMR)
```

需要 [Rust](https://rustup.rs/) stable 工具链和 Node.js 24+。

### 故障排除

#### 端口 4000 被占用

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### 图像/视频被拒绝

DeepSeek 不支持图像或视频。图像会自动替换为占位符文本（`non_vision_image_policy: "replace"`）。要原生使用图像，请切换到 MiniMax 或 Kimi。视频始终被拒绝。

### 许可证

MIT — 详见 [LICENSE](LICENSE)。
