# Anthropic Proxy Gateway

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール。

Anthropic Messages API リクエストの `model` フィールドを読み取り、対応する upstream へ自動振り分け（モデルベースルーティング）。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

GUI 管理ツール（Tauri v2 + React + TypeScript）でプロキシの起動・停止、設定編集、ログ確認、API キー管理が可能です。

**v0.3.0 以降、Python は不要です。** プロキシサーバーは Rust (axum 0.7) で書き直され、Tauri アプリのバイナリに内蔵されています。

### なぜこのゲートウェイが必要か

Claude Desktop / Claude Code は、基本的にAnthropicのAPI形式とClaude系のモデル名を前提に動作します。そのため、DeepSeek、MiniMax、Kimi などがAnthropic互換APIを提供していても、Claude Desktop / Claude Code からそれらを直接指定して常に利用できるとは限りません。

特に現在配布されているClaude Desktop / Claude Code Desktopでは、モデル名の検証やモデルピッカーの制約により、`deepseek-v4-pro` や `kimi-k2.6` のようなサードパーティの実モデル名をそのまま使えない場合があります。

Anthropic Proxy Gateway は、この制約を吸収するために、Claude側には `claude-...` で始まるゲートウェイ用モデル名を提示し、内部で実際の上流モデルへ変換します。

```
claude-deepseek-v4       → deepseek-v4-pro
claude-deepseek-flash    → deepseek-v4-flash
claude-minimax-m3        → MiniMax-M3
claude-kimi-k2-6         → kimi-k2.6
```

つまり、このゲートウェイは、Claude Desktop / Claude Code からサードパーティ製LLMを利用するための互換レイヤーです。これを介することで、Claude側の制約に合わせつつ、DeepSeek / MiniMax / Kimi などのモデルを切り替えて利用できます。

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

#### 5. Claude Desktop / Claude Code 設定

GUI の **Claude Desktop Setup** タブで設定 JSON をコピーし、Claude Desktop の設定ファイルに貼り付けます。
自動検出された設定ファイルが一覧表示されるので、適切なファイルを開いて貼り付けてください。

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-deepseek-v4",              "labelOverride": "DeepSeek V4 Pro via Gateway" },
    { "name": "claude-deepseek-flash",           "labelOverride": "DeepSeek V4 Flash via Gateway" },
    { "name": "claude-minimax-m3",               "labelOverride": "MiniMax M3 via Gateway" },
    { "name": "claude-minimax-m3-thinking",      "labelOverride": "MiniMax M3 (Thinking) via Gateway" },
    { "name": "claude-minimax-m2-7-highspeed",   "labelOverride": "MiniMax M2.7 Highspeed via Gateway" },
    { "name": "claude-kimi-k2-6",                "labelOverride": "Kimi K2.6 via Gateway" },
    { "name": "claude-kimi-k2-6-thinking",       "labelOverride": "Kimi K2.6 (Thinking) via Gateway" }
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

```json
{
  "active_provider": "deepseek",
  "providers": {
    "deepseek": {
      "display_name": "DeepSeek",
      "upstream_url": "https://api.deepseek.com/anthropic",
      "api_key_env": "DEEPSEEK_API_KEY",
      "default_model": "deepseek-v4-pro",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "supports_thinking": true,
      "model_map": {
        "claude-sonnet-4-5": "deepseek-v4-pro"
      },
      "visible_models": ["claude-deepseek-v4", "claude-deepseek-flash"],
      "models": {
        "claude-deepseek-v4": { "upstream_model": "deepseek-v4-pro" },
        "claude-deepseek-flash": { "upstream_model": "deepseek-v4-flash" },
        "claude-sonnet-4-5": { "upstream_model": "deepseek-v4-pro", "visible": false }
      }
    },
    "minimax": {
      "display_name": "MiniMax",
      "upstream_url": "https://api.minimax.io/anthropic",
      "api_key_env": "MINIMAX_API_KEY",
      "default_model": "MiniMax-M3",
      "supports_count_tokens": true,
      "supports_vision": true,
      "supports_video": true,
      "models": {
        "claude-minimax-m3": {
          "upstream_model": "MiniMax-M3",
          "thinking": "disabled"
        },
        "claude-minimax-m3-thinking": { "upstream_model": "MiniMax-M3" },
        "claude-minimax-m2-7-highspeed": {
          "upstream_model": "MiniMax-M2.7-highspeed",
          "supports_vision": false,
          "supports_video": false
        }
      }
    },
    "kimi": {
      "display_name": "Kimi / Moonshot",
      "upstream_url": "https://api.moonshot.ai/anthropic",
      "api_key_env": "MOONSHOT_API_KEY",
      "default_model": "kimi-k2.6",
      "supports_vision": true,
      "supports_video": true,
      "models": {
        "claude-kimi-k2-6": {
          "upstream_model": "kimi-k2.6",
          "thinking": "disabled"
        },
        "claude-kimi-k2-6-thinking": { "upstream_model": "kimi-k2.6" }
      }
    }
  },
  "server": {
    "host": "127.0.0.1",
    "port": 4000,
    "enable_cors": false
  }
}
```

#### models セクションのキー

| キー | 説明 |
|-----|------|
| `upstream_model` | upstream へ送る実モデル名（必須） |
| `thinking` | `"disabled"` 時のみ thinking 抑制注入（省略可） |
| `supports_vision` | モデル単位の画像サポート（省略時はプロバイダー既定値） |
| `supports_video` | モデル単位の動画サポート（省略時はプロバイダー既定値） |
| `visible` | `/v1/models` とダッシュボードに表示するか（デフォルト `true`） |

`models` がない場合は `model_map` と `visible_models` にフォールバック（後方互換）。

> 日本語 Windows では `config.json` を **Shift-JIS** で保存する必要があります。GUI の Gateway Settings タブでエンコーディングを切り替えて編集できます。

### プロジェクト構成

```
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    仕様書（日英）
├── LICENSE                    MIT License
├── config.json                プロバイダー設定
├── .gitignore
├── icon/                      アイコンソース (SVG, PNG)
├── scripts/
│   ├── phase0_probe.py        事前検証スクリプト
│   └── proxy_e2e_test.py      E2E テスト
├── gui/
│   ├── src/                   React フロントエンド (TypeScript)
│   │   ├── components/        UI コンポーネント (7ファイル)
│   │   ├── hooks/             カスタムフック (7ファイル)
│   │   └── i18n/              日英翻訳
│   ├── src-tauri/             Tauri バックエンド (Rust)
│   │   ├── src/
│   │   │   ├── lib.rs         21 Tauri コマンド + プロキシライフサイクル
│   │   │   ├── main.rs        エントリーポイント
│   │   │   └── proxy.rs       axum プロキシサーバー本体
│   │   ├── resources/
│   │   │   └── config.json    バンドル設定
│   │   └── Cargo.toml
│   └── package.json
├── Communication-Logs/        プロキシ実行ログ
├── claude-log/                開発セッションログ
└── release/                   ビルド済み配布物
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

In particular, model name validation and model picker constraints in current releases of Claude Desktop / Claude Code Desktop may prevent third-party model names like `deepseek-v4-pro` or `kimi-k2.6` from being used as-is.

Anthropic Proxy Gateway absorbs this constraint by presenting gateway model names starting with `claude-...` to the Claude side, and internally translating them to actual upstream models.

```
claude-deepseek-v4       → deepseek-v4-pro
claude-deepseek-flash    → deepseek-v4-flash
claude-minimax-m3        → MiniMax-M3
claude-kimi-k2-6         → kimi-k2.6
```

In short, this gateway is a compatibility layer for using third-party LLMs from Claude Desktop / Claude Code. It lets you switch between DeepSeek, MiniMax, Kimi, and other models while respecting Claude-side constraints.

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

Go to the **Claude Desktop Setup** tab, copy the JSON config, and paste it into your Claude Desktop settings file.
Auto-detected config files are listed — open the appropriate one and paste.

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    { "name": "claude-deepseek-v4",              "labelOverride": "DeepSeek V4 Pro via Gateway" },
    { "name": "claude-deepseek-flash",           "labelOverride": "DeepSeek V4 Flash via Gateway" },
    { "name": "claude-minimax-m3",               "labelOverride": "MiniMax M3 via Gateway" },
    { "name": "claude-minimax-m3-thinking",      "labelOverride": "MiniMax M3 (Thinking) via Gateway" },
    { "name": "claude-minimax-m2-7-highspeed",   "labelOverride": "MiniMax M2.7 Highspeed via Gateway" },
    { "name": "claude-kimi-k2-6",                "labelOverride": "Kimi K2.6 via Gateway" },
    { "name": "claude-kimi-k2-6-thinking",       "labelOverride": "Kimi K2.6 (Thinking) via Gateway" }
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

```json
{
  "active_provider": "deepseek",
  "providers": {
    "deepseek": {
      "display_name": "DeepSeek",
      "upstream_url": "https://api.deepseek.com/anthropic",
      "api_key_env": "DEEPSEEK_API_KEY",
      "default_model": "deepseek-v4-pro",
      "supports_vision": false,
      "supports_video": false,
      "models": {
        "claude-deepseek-v4": { "upstream_model": "deepseek-v4-pro" },
        "claude-deepseek-flash": { "upstream_model": "deepseek-v4-flash" },
        "claude-sonnet-4-5": { "upstream_model": "deepseek-v4-pro", "visible": false }
      }
    },
    "minimax": {
      "display_name": "MiniMax",
      "upstream_url": "https://api.minimax.io/anthropic",
      "api_key_env": "MINIMAX_API_KEY",
      "default_model": "MiniMax-M3",
      "supports_count_tokens": true,
      "supports_vision": true,
      "supports_video": true,
      "models": {
        "claude-minimax-m3": { "upstream_model": "MiniMax-M3", "thinking": "disabled" },
        "claude-minimax-m3-thinking": { "upstream_model": "MiniMax-M3" },
        "claude-minimax-m2-7-highspeed": {
          "upstream_model": "MiniMax-M2.7-highspeed",
          "supports_vision": false,
          "supports_video": false
        }
      }
    },
    "kimi": {
      "display_name": "Kimi / Moonshot",
      "upstream_url": "https://api.moonshot.ai/anthropic",
      "api_key_env": "MOONSHOT_API_KEY",
      "default_model": "kimi-k2.6",
      "supports_vision": true,
      "supports_video": true,
      "models": {
        "claude-kimi-k2-6": { "upstream_model": "kimi-k2.6", "thinking": "disabled" },
        "claude-kimi-k2-6-thinking": { "upstream_model": "kimi-k2.6" }
      }
    }
  },
  "server": {
    "host": "127.0.0.1",
    "port": 4000,
    "enable_cors": false
  }
}
```

#### models section keys

| Key | Description |
|-----|-------------|
| `upstream_model` | Actual model name sent to upstream (required) |
| `thinking` | When `"disabled"`, injects thinking suppression (optional) |
| `supports_vision` | Per-model image support (falls back to provider default) |
| `supports_video` | Per-model video support (falls back to provider default) |
| `visible` | Whether to expose in `/v1/models` and dashboard (default `true`) |

When `models` is absent, falls back to `model_map` + `visible_models` (backward compatible).

> Japanese Windows requires saving `config.json` as **Shift-JIS**. Use the Gateway Settings tab in the GUI to toggle encoding.

### Project Structure

```
Anthropic-Proxy-Gateway/
├── README.md
├── SPEC.md                    Specification (JA/EN)
├── LICENSE                    MIT License
├── config.json                Provider configuration
├── .gitignore
├── icon/                      Icon source (SVG, PNG)
├── scripts/
│   ├── phase0_probe.py        Pre-implementation compatibility probe
│   └── proxy_e2e_test.py      End-to-end proxy tests
├── gui/
│   ├── src/                   React frontend (TypeScript)
│   │   ├── components/        UI components (7 files)
│   │   ├── hooks/             Custom hooks (7 files)
│   │   └── i18n/              Japanese/English translations
│   ├── src-tauri/             Tauri backend (Rust)
│   │   ├── src/
│   │   │   ├── lib.rs         21 Tauri commands + proxy lifecycle
│   │   │   ├── main.rs        Entry point
│   │   │   └── proxy.rs       axum proxy server
│   │   ├── resources/
│   │   │   └── config.json    Bundled configuration
│   │   └── Cargo.toml
│   └── package.json
├── Communication-Logs/        Proxy runtime logs
├── claude-log/                Development session logs
└── release/                   Built distributable
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
