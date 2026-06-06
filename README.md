# Anthropic Proxy Gateway

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール。

Anthropic Messages API リクエストの `model` フィールドを読み取り、対応する upstream へ自動振り分け（モデルベースルーティング）。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

GUI 管理ツール（Tauri v2 + React 19 + TypeScript）でプロキシの起動・停止、設定編集、ログ確認、API キー管理が可能です。

### なぜこのゲートウェイが必要か

Claude Desktop / Claude Code は、基本的にAnthropicのAPI形式とClaude系のモデル名を前提に動作します。そのため、DeepSeek、MiniMax、Kimi などがAnthropic互換APIを提供していても、Claude Desktop / Claude Code からそれらを直接指定して常に利用できるとは限りません。

特に **Claude Desktop の `inferenceModels[].name` には Anthropic 公式モデル名しか指定できません**。`claude-deepseek-v4` や `kimi-k2.6` のようなゲートウェイ独自名は `"not an Anthropic model"` として弾かれます。

Anthropic Proxy Gateway はこの制約を回避するため、**Claude Desktop には常に Anthropic 公式モデル名（`claude-sonnet-4-6` / `claude-haiku-4-5`）を「器」として見せ、実際に使う LLM（DeepSeek / MiniMax / Kimi）は GUI で切り替える**設計を採用しています。

```
Claude Desktop 側（常に固定）
  Gateway Pro   = claude-sonnet-4-6
  Gateway Flash = claude-haiku-4-5

ゲートウェイ内部（GUI の選択による）
  DeepSeek 選択時:  Sonnet → deepseek-v4-pro,     Haiku → deepseek-v4-flash
  MiniMax 選択時:   Sonnet → MiniMax-M3,           Haiku → MiniMax-M3
  Kimi 選択時:      Sonnet → kimi-k2.6,            Haiku → kimi-k2.6 (thinking disabled)
```

これにより、Claude Desktop のモデル名検証を通過しつつ、DeepSeek / MiniMax / Kimi を自由に切り替えられます。

### 必要環境

- **Windows 10/11**（日本語環境対応）
- 利用するプロバイダーの API キー（DeepSeek / MiniMax / Kimi **いずれか1つでOK**、v0.5.0以降）

### クイックスタート

#### 1. インストール

[Releases](https://github.com/soheidon/Anthropic-Proxy-Gateway/releases) から最新のインストーラーをダウンロードして実行。

インストーラー起動時に言語選択画面が表示されます（English, 日本語, 中文(简体), 中文(繁體), 한국어, Français から選択可）。

#### 2. API キー設定

設定（⚙）→ **API キー** タブで、使用するプロバイダーの API キーを入力し「保存」をクリック。
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

設定（⚙）→ **Claude Desktop 設定** タブで：

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
    { "name": "claude-sonnet-4-6", "labelOverride": "Gateway Pro" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Gateway Flash" }
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
上級者向けの詳細設定は GUI の設定（⚙）→ **Gateway Config** から行えます。

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
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    仕様書（日英）
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
    │   │   ├── lib.rs         22 Tauri コマンド + プロキシライフサイクル
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

## English

### Overview

A proxy + GUI manager that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

The proxy reads the `model` field from each request and automatically routes to the correct upstream provider (model-based routing). Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

The GUI management tool (Tauri v2 + React 19 + TypeScript) provides start/stop control, config editing, log viewing, and API key management from a native Windows window.

### Why This Gateway Is Needed

Claude Desktop / Claude Code fundamentally expects Anthropic's API format and Claude-family model names. Even when providers like DeepSeek, MiniMax, and Kimi offer Anthropic-compatible APIs, Claude Desktop / Claude Code cannot always use them directly.

In particular, **Claude Desktop's `inferenceModels[].name` only accepts Anthropic official model names**. Gateway custom names like `claude-deepseek-v4` or `kimi-k2.6` are rejected as `"not an Anthropic model"`.

To work around this constraint, Anthropic Proxy Gateway **presents Anthropic official model names (`claude-sonnet-4-6` / `claude-haiku-4-5`) as "shells" to Claude Desktop, while the actual LLM (DeepSeek / MiniMax / Kimi) is selected in the GUI**.

```
Claude Desktop side (always fixed)
  Gateway Pro   = claude-sonnet-4-6
  Gateway Flash = claude-haiku-4-5

Gateway internal (based on GUI selection)
  DeepSeek:  Sonnet → deepseek-v4-pro,     Haiku → deepseek-v4-flash
  MiniMax:   Sonnet → MiniMax-M3,           Haiku → MiniMax-M3
  Kimi:      Sonnet → kimi-k2.6,            Haiku → kimi-k2.6 (thinking disabled)
```

This lets you pass Claude Desktop's model name validation while freely switching between DeepSeek, MiniMax, and Kimi.

### Prerequisites

- **Windows 10/11** (Japanese locale supported)
- API key for your chosen provider (DeepSeek / MiniMax / Kimi — **just one is enough**, since v0.5.0)

### Quick Start

#### 1. Install

Download the latest installer from [Releases](https://github.com/soheidon/Anthropic-Proxy-Gateway/releases) and run it.

The installer shows a language selection screen on launch (choose from English, 日本語, 中文(简体), 中文(繁體), 한국어, Français).

#### 2. Set API Key

Settings (⚙) → **API Key** tab, enter your provider's API key and click **Save**.
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

Settings (⚙) → **Claude Desktop Setup** tab:

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
    { "name": "claude-sonnet-4-6", "labelOverride": "Gateway Pro" },
    { "name": "claude-haiku-4-5",  "labelOverride": "Gateway Flash" }
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
Advanced users can edit via Settings (⚙) → **Gateway Config**.

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
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    Specification (JA/EN)
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
    │   │   ├── lib.rs         22 Tauri commands + proxy lifecycle
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
