# Anthropic Proxy Gateway

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール。

Anthropic Messages API リクエストを選択中のプロバイダーに透過転送します。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

GUI 管理ツール（Tauri v2 + React + TypeScript）でプロキシの起動・停止、プロバイダー切替、設定編集、ログ確認、API キー管理が可能です。

**v0.3.0 以降、Python は不要です。** プロキシサーバーは Rust (axum 0.7) で書き直され、Tauri アプリのバイナリに内蔵されています。

### 対応プロバイダー

| プロバイダー | モデル | Vision | Video | Count Tokens |
|-------------|--------|--------|-------|-------------|
| DeepSeek | deepseek-v4-pro / deepseek-v4-flash | ✗ | ✗ | ✗ |
| MiniMax | MiniMax-M3 | ✓ | ✓ | ✓ |
| Kimi / Moonshot | kimi-k2.6 | ✓ | ✓ | ✗ |

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

### エンドポイント

| Method | Path | 説明 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | モデル一覧（active_provider の visible_models） |
| POST | `/v1/messages` | Messages API（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | トークン数カウント（対応プロバイダーのみ） |

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
      "model_map": {
        "claude-sonnet-4-5": "deepseek-v4-pro",
        "claude-haiku-4-5-20251001": "deepseek-v4-flash"
      },
      "visible_models": [
        "claude-deepseek-v4",
        "claude-deepseek-flash"
      ]
    }
  },
  "server": {
    "host": "127.0.0.1",
    "port": 4000,
    "enable_cors": false
  }
}
```

| キー | 説明 |
|-----|------|
| `active_provider` | 現在有効なプロバイダー ID |
| `providers.<id>.upstream_url` | Anthropic 互換 API のベース URL |
| `providers.<id>.api_key_env` | API キーを保持する環境変数名 |
| `providers.<id>.model_map` | Claude モデル名 → 実モデル名のマッピング |
| `providers.<id>.visible_models` | `GET /v1/models` で公開するモデル名 |
| `providers.<id>.default_model` | マップにない場合のフォールバック |
| `providers.<id>.force_anthropic_version` | null 時は受信ヘッダを転送、設定時は強制上書き |
| `providers.<id>.supports_vision` / `supports_video` | 画像/動画非対応プロバイダーでは 400 で拒否 |
| `providers.<id>.supports_count_tokens` | false 時は count_tokens エンドポイントが 501 を返す |
| `server.enable_cors` | CORS 有効/無効 |

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
│   │   │   ├── lib.rs         18 Tauri コマンド + プロキシライフサイクル
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

`config.json` の `model_map` に対象モデル名を追加してください。

#### 画像/動画が拒否される

DeepSeek プロバイダーは画像・動画に対応していません。MiniMax または Kimi に切り替えてください。

### ライセンス

MIT — 詳細は [LICENSE](LICENSE) を参照。

---

## English

### Overview

A proxy + GUI manager that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

Anthropic Messages API requests are transparently forwarded to the selected provider. Only the `model` field is rewritten — messages, thinking blocks, tool_use, tool_result, and streaming SSE pass through untouched.

The GUI management tool (Tauri v2 + React + TypeScript) provides start/stop control, provider switching, config editing, log viewing, and API key management from a native Windows window.

**As of v0.3.0, Python is no longer required.** The proxy server has been rewritten in Rust (axum 0.7) and is embedded directly in the Tauri app binary.

### Supported Providers

| Provider | Model | Vision | Video | Count Tokens |
|----------|-------|--------|-------|-------------|
| DeepSeek | deepseek-v4-pro / deepseek-v4-flash | ✗ | ✗ | ✗ |
| MiniMax | MiniMax-M3 | ✓ | ✓ | ✓ |
| Kimi / Moonshot | kimi-k2.6 | ✓ | ✓ | ✗ |

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

### Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/v1/models` | List visible models for active provider |
| POST | `/v1/messages` | Messages API (stream + non-stream) |
| POST | `/v1/messages/count_tokens` | Token counting (supported providers only) |

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
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": {
        "claude-sonnet-4-5": "deepseek-v4-pro",
        "claude-haiku-4-5-20251001": "deepseek-v4-flash"
      },
      "visible_models": [
        "claude-deepseek-v4",
        "claude-deepseek-flash"
      ]
    }
  },
  "server": {
    "host": "127.0.0.1",
    "port": 4000,
    "enable_cors": false
  }
}
```

| Key | Description |
|-----|-------------|
| `active_provider` | Currently active provider ID |
| `providers.<id>.upstream_url` | Anthropic-compatible API base URL |
| `providers.<id>.api_key_env` | Environment variable holding the API key |
| `providers.<id>.model_map` | Claude model name → actual model name mapping |
| `providers.<id>.visible_models` | Models exposed via `GET /v1/models` |
| `providers.<id>.default_model` | Fallback when model not in map |
| `providers.<id>.force_anthropic_version` | `null` = passthrough; set to override |
| `providers.<id>.supports_vision` / `supports_video` | Returns 400 for media on incapable providers |
| `providers.<id>.supports_count_tokens` | Returns 501 on count_tokens for unsupported providers |
| `server.enable_cors` | Enable/disable CORS middleware |

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
│   │   │   ├── lib.rs         18 Tauri commands + proxy lifecycle
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

Add the model name to `model_map` in `config.json`.

#### Image/video rejected

The DeepSeek provider does not support images or video. Switch to MiniMax or Kimi.

### License

MIT — see [LICENSE](LICENSE) for details.
