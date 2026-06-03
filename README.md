# Anthropic Proxy Gateway

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール。

Anthropic Messages API リクエストの `model` フィールドを読み取り、対応する upstream へ自動振り分け（モデルベースルーティング）。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

GUI 管理ツール（Tauri v2 + React + TypeScript）でプロキシの起動・停止、設定編集、ログ確認、API キー管理が可能です。

**v0.3.0 以降、Python は不要です。** プロキシサーバーは Rust (axum 0.7) で書き直され、Tauri アプリのバイナリに内蔵されています。

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
  MiniMax 選択時:   Sonnet → MiniMax-M3,           Haiku → MiniMax-M2.7-highspeed
  Kimi 選択時:      Sonnet → kimi-k2.6,            Haiku → kimi-k2.6 (thinking disabled)
```

これにより、Claude Desktop のモデル名検証を通過しつつ、DeepSeek / MiniMax / Kimi を自由に切り替えられます。

### 公開モデル

`/v1/models` が返す7モデル（全プロバイダーの全公開モデル）:

| Gateway model | Upstream | Provider | Thinking | Vision/Video |
|---|---|---|---|---|
| `claude-deepseek-v4` | `deepseek-v4-pro` | DeepSeek | default | no |
| `claude-deepseek-flash` | `deepseek-v4-flash` | DeepSeek | default | no |
| `claude-minimax-m3` | `MiniMax-M3` | MiniMax | disabled | yes |
| `claude-minimax-m3-thinking` | `MiniMax-M3` | MiniMax | default | yes |
| `claude-minimax-m2-7-highspeed` | `MiniMax-M2.7-highspeed` | MiniMax | default | no |
| `claude-kimi-k2-6` | `kimi-k2.6` | Kimi | disabled | yes |
| `claude-kimi-k2-6-thinking` | `kimi-k2.6` | Kimi | default | yes |

### プロバイダー機能マトリクス

| プロバイダー | モデル | Vision | Video | Count Tokens | Thinking |
|-------------|--------|--------|-------|-------------|----------|
| DeepSeek | deepseek-v4-pro / deepseek-v4-flash | ✗ | ✗ | ✗ | default |
| MiniMax | MiniMax-M3 | ✓ | ✓ | ✓ | default / disabled |
| MiniMax | MiniMax-M2.7-highspeed | ✗ | ✗ | ✗ | default |
| Kimi | kimi-k2.6 | ✓ | ✓ | ✗ | default / disabled |

### 必要環境

- **Windows 10/11**（日本語環境対応）
- 利用するプロバイダーの API キー（DeepSeek / MiniMax / Kimi いずれか）

### クイックスタート

#### 1. インストール

[Releases](https://github.com/soheidon/Anthropic-Proxy-Gateway/releases) から最新の MSI インストーラーをダウンロードして実行。

#### 2. 起動

デスクトップのショートカットから `Anthropic Provider Gateway Manager` を起動します。

#### 3. API キー設定

GUI の **API キー** タブで、使用するプロバイダーの API キーを入力し「保存」をクリック。
Windows ユーザー環境変数に永続保存されます。

| プロバイダー | 環境変数 |
|-------------|---------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 4. プロキシ起動

ヘッダーの **Start Gateway** ボタンをクリック。プロキシが `http://127.0.0.1:4000` で起動します（コンソールウィンドウは表示されません）。

#### 5. Claude Desktop 設定

GUI の **Claude Desktop 設定** タブで以下を行います：

1. 使用したい LLM プロバイダ（DeepSeek / MiniMax / Kimi）を選択
2. 自動生成された設定 JSON をコピー
3. 自動検出された Claude Desktop 設定ファイルに貼り付け

Claude Desktop に貼り付ける JSON は常に以下2モデルのみです。ゲートウェイがプロバイダ選択に応じて内部変換します。

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
| GET | `/v1/models` | 全プロバイダーの公開モデル一覧（7モデル） |
| POST | `/v1/messages` | Messages API（stream/non-stream）。モデルベースルーティング |
| POST | `/v1/messages/count_tokens` | トークン数カウント（対応プロバイダーのみ） |

### ルーティング

モデルベースルーティング（v0.4.0〜）: リクエストの `model` フィールドを読み取り、対応するプロバイダーと upstream モデルに自動振り分け。`active_provider` の手動切替は不要。

Thinking バリアント:
- `claude-kimi-k2-6` / `claude-minimax-m3` → `thinking: {"type": "disabled"}` を注入
- `*-thinking` バリアント → thinking はデフォルト動作（注入なし）

### 設定 (config.json)

プロバイダー設定は各モデルの上流モデル名や機能フラグを定義します。通常は編集不要です（インストール時に適切なデフォルトが同梱されます）。上級者向けの詳細設定は GUI の **詳細設定** タブから行えます。

| キー | 説明 |
|-----|------|
| `models.<model>.upstream_model` | upstream へ送る実モデル名（必須） |
| `models.<model>.thinking` | `"disabled"` 時のみ thinking 抑制注入（省略可） |
| `models.<model>.supports_vision` | モデル単位の画像サポート（省略時はプロバイダー既定値） |
| `models.<model>.supports_video` | モデル単位の動画サポート（省略時はプロバイダー既定値） |
| `models.<model>.visible` | `/v1/models` とダッシュボードに表示するか（デフォルト `true`） |

> 日本語 Windows では `config.json` を **Shift-JIS** で保存する必要があります。GUI の詳細設定タブでエンコーディングを切り替えて編集できます。

### プロジェクト構成

```
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    仕様書（日英）
├── LICENSE                    MIT License
├── config.json                プロバイダー設定
├── .gitignore
├── icon/                      アイコンソース (SVG, PNG)
└── gui/
    ├── src/                   React フロントエンド (TypeScript)
    │   ├── components/        UI コンポーネント (6ファイル)
    │   ├── hooks/             カスタムフック (5ファイル)
    │   └── i18n/              日英翻訳
    ├── src-tauri/             Tauri バックエンド (Rust)
    │   ├── src/
    │   │   ├── lib.rs         21 Tauri コマンド + プロキシライフサイクル
    │   │   ├── main.rs        エントリーポイント
    │   │   └── proxy.rs       axum プロキシサーバー本体
    │   ├── resources/
    │   │   └── config.json    バンドル設定
    │   └── Cargo.toml
    └── package.json
```

### 開発

#### GUI のビルド

```bash
cd gui
npm install
npm run tauri build
```

[Rust](https://rustup.rs/) stable ツールチェーンと Node.js 24+ が必要です。

#### 開発モード

```bash
cd gui
npm run tauri dev
```

GUI 開発モードでは Vite dev server (`localhost:1420`) と Tauri ウィンドウが起動します。

### トラブルシュート

#### ポート 4000 が使用中

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Invalid model name

`config.json` の `models` セクションまたは `model_map` に対象モデル名を追加してください。

#### 画像/動画が拒否される

DeepSeek および MiniMax-M2.7-highspeed は画像・動画に対応していません。MiniMax-M3 または Kimi K2.6 を選択してください。

### ライセンス

MIT — 詳細は [LICENSE](LICENSE) を参照。

---

## English

### Overview

A proxy + GUI manager that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

The proxy reads the `model` field from each request and automatically routes to the correct upstream provider (model-based routing). Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

The GUI management tool (Tauri v2 + React + TypeScript) provides start/stop control, config editing, log viewing, and API key management from a native Windows window.

**As of v0.3.0, Python is no longer required.** The proxy server has been rewritten in Rust (axum 0.7) and is embedded directly in the Tauri app binary.

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
  MiniMax:   Sonnet → MiniMax-M3,           Haiku → MiniMax-M2.7-highspeed
  Kimi:      Sonnet → kimi-k2.6,            Haiku → kimi-k2.6 (thinking disabled)
```

This lets you pass Claude Desktop's model name validation while freely switching between DeepSeek, MiniMax, and Kimi.

### Public Models

7 models returned by `/v1/models` (all public models from all providers):

| Gateway model | Upstream | Provider | Thinking | Vision/Video |
|---|---|---|---|---|
| `claude-deepseek-v4` | `deepseek-v4-pro` | DeepSeek | default | no |
| `claude-deepseek-flash` | `deepseek-v4-flash` | DeepSeek | default | no |
| `claude-minimax-m3` | `MiniMax-M3` | MiniMax | disabled | yes |
| `claude-minimax-m3-thinking` | `MiniMax-M3` | MiniMax | default | yes |
| `claude-minimax-m2-7-highspeed` | `MiniMax-M2.7-highspeed` | MiniMax | default | no |
| `claude-kimi-k2-6` | `kimi-k2.6` | Kimi | disabled | yes |
| `claude-kimi-k2-6-thinking` | `kimi-k2.6` | Kimi | default | yes |

### Provider Capability Matrix

| Provider | Model | Vision | Video | Count Tokens | Thinking |
|----------|-------|--------|-------|-------------|----------|
| DeepSeek | deepseek-v4-pro / deepseek-v4-flash | ✗ | ✗ | ✗ | default |
| MiniMax | MiniMax-M3 | ✓ | ✓ | ✓ | default / disabled |
| MiniMax | MiniMax-M2.7-highspeed | ✗ | ✗ | ✗ | default |
| Kimi | kimi-k2.6 | ✓ | ✓ | ✗ | default / disabled |

### Prerequisites

- **Windows 10/11** (Japanese locale supported)
- API key for your chosen provider (DeepSeek / MiniMax / Kimi)

### Quick Start

#### 1. Install

Download the latest MSI installer from [Releases](https://github.com/soheidon/Anthropic-Proxy-Gateway/releases) and run it.

#### 2. Launch

Launch `Anthropic Provider Gateway Manager` from the desktop shortcut.

#### 3. Set API Key

Go to the **API Key** tab, enter your provider's API key, and click **Save**.
The key is persisted as a Windows user environment variable.

| Provider | Environment Variable |
|----------|---------------------|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |

#### 4. Start Gateway

Click **Start Gateway** in the header. The proxy starts on `http://127.0.0.1:4000` as a background process (no console window).

#### 5. Configure Claude Desktop / Claude Code

Go to the **Claude Desktop Setup** tab:

1. Select the LLM provider you want to use (DeepSeek / MiniMax / Kimi)
2. Copy the auto-generated config JSON
3. Paste it into your auto-detected Claude Desktop settings file

The JSON you paste always uses these 2 model names. The gateway translates them based on your provider selection.

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
| GET | `/v1/models` | All public models from all providers (7 models) |
| POST | `/v1/messages` | Messages API (stream + non-stream). Model-based routing |
| POST | `/v1/messages/count_tokens` | Token counting (supported providers only) |

### Routing

Model-based routing (since v0.4.0): the `model` field in each request determines the target provider and upstream model. No manual `active_provider` switching.

Thinking variants:
- `claude-kimi-k2-6` / `claude-minimax-m3` → injects `thinking: {"type": "disabled"}`
- `*-thinking` variants → default behavior (no injection)

### Configuration (config.json)

Provider settings define upstream model names and capability flags per model. Normally no editing is required — sensible defaults ship with the installer. Advanced users can edit via the **Advanced** tab.

| Key | Description |
|-----|-------------|
| `models.<model>.upstream_model` | Actual model name sent to upstream (required) |
| `models.<model>.thinking` | When `"disabled"`, injects thinking suppression (optional) |
| `models.<model>.supports_vision` | Per-model image support (falls back to provider default) |
| `models.<model>.supports_video` | Per-model video support (falls back to provider default) |
| `models.<model>.visible` | Whether to expose in `/v1/models` and dashboard (default `true`) |

> Japanese Windows requires saving `config.json` as **Shift-JIS**. Use the Advanced tab in the GUI to toggle encoding.

### Project Structure

```
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    Specification (JA/EN)
├── LICENSE                    MIT License
├── config.json                Provider configuration
├── .gitignore
├── icon/                      Icon source (SVG, PNG)
└── gui/
    ├── src/                   React frontend (TypeScript)
    │   ├── components/        UI components (6 files)
    │   ├── hooks/             Custom hooks (5 files)
    │   └── i18n/              Japanese/English translations
    ├── src-tauri/             Tauri backend (Rust)
    │   ├── src/
    │   │   ├── lib.rs         21 Tauri commands + proxy lifecycle
    │   │   ├── main.rs        Entry point
    │   │   └── proxy.rs       axum proxy server
    │   ├── resources/
    │   │   └── config.json    Bundled configuration
    │   └── Cargo.toml
    └── package.json
```

### Dev Build

#### GUI

```bash
cd gui
npm install
npm run tauri build
```

Requires [Rust](https://rustup.rs/) stable toolchain and Node.js 24+.

#### Dev Mode

```bash
cd gui
npm run tauri dev
```

This starts the Vite dev server (`localhost:1420`) and a Tauri window.

### Troubleshooting

#### Port 4000 in use

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

#### Invalid model name

Add the model name to the `models` section or `model_map` in `config.json`.

#### Image/video rejected

DeepSeek and MiniMax-M2.7-highspeed do not support images or video. Switch to MiniMax-M3 or Kimi K2.6.

### License

MIT — see [LICENSE](LICENSE) for details.
