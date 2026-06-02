# SPEC: Anthropic Proxy Gateway / 仕様書

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code Desktop から利用するための薄型プロキシ + GUI 管理ツール。

### 背景

Claude Desktop / Claude Code Desktop は Anthropic Messages API (`/v1/messages`) に直接リクエストを送る。これを各プロバイダーの Anthropic 互換エンドポイントに振り向けることで、複数プロバイダーのモデルを Anthropic クライアントから透過的に利用可能にする。

#### 解決する問題

- Claude Desktop 側のモデル名バリデーション
- LiteLLM の Anthropic→OpenAI 変換による情報ロス
- `claude-haiku-4-5-20251001` などの未登録モデル名問題
- 単一プロバイダー依存（DeepSeek, MiniMax, Kimi を切り替え可能）

#### 既知の制限

- DeepSeek Anthropic 互換 API が thinking block を完全に扱えない場合がある
- `tool_use` / `tool_result` / streaming SSE の互換性が不十分な場合がある

### アーキテクチャ

```
Claude Desktop / Claude Code
       │
       ▼
proxy.rs (127.0.0.1:4000)  ← Tauri アプリに内蔵 (axum 0.7 + reqwest)
       │
       │ model フィールドのみ書き換え
       │ 他は完全透過転送（messages / thinking / tool_use / tool_result / SSE）
       │ 画像/動画のプロバイダー非対応チェック
       ▼
選択中プロバイダーの Anthropic-compatible API
(DeepSeek / MiniMax / Kimi)
```

#### 設計方針

- **薄型プロキシ**: model フィールドの書換え以外は一切手を加えない。SSE もパースせずバイト単位で透過転送。
- **ロスレス転送**: メッセージ本文やツール呼び出し、thinking block を一切加工しない。
- **マルチプロバイダー**: `config.json` の `active_provider` でプロバイダーを切り替え可能。プロバイダーごとに upstream URL、API キー環境変数、model_map、機能フラグを設定。
- **Windows ネイティブ GUI**: Tauri v2 + React + TypeScript。バックエンドは Rust、フロントエンドは Vite + React 19。
- **ゼロ外部依存**: v0.3.0 以降、プロキシは Rust に移植され Tauri バイナリに内蔵。Python 不要。

### GUI 管理ツール

Tauri v2 + React + TypeScript 製。4タブ構成。

```
┌──────────────────────────────────────────┐
│  Anthropic Proxy Gateway Manager          │
│  [Gateway: Running] [起動/停止] [EN|JA]  │
├──────────────────────────────────────────┤
│  Dashboard │ Gateway設定 │ Claude設定 │ APIキー │
├──────────────────────────────────────────┤
│  Status      │  最新ログ                 │
│  - Port 4000 │  - ログ切替              │
│  - APIキー   │  - 新規ログ              │
│  - URL       │  - 使用回数              │
└──────────────────────────────────────────┘
```

| タブ | 機能 |
|------|------|
| Dashboard | Port 4000 状態、APIキー設定状態、Gateway URL、最新ログ表示、使用回数集計 |
| Gateway Settings | config.json の直接編集、UTF-8/Shift-JIS エンコード切替、プロバイダー切替、保存/再読込 |
| Claude Desktop Setup | 設定JSONの表示とクリップボードコピー、設定ファイル自動検出、手動フォルダ参照 |
| API Key | 各プロバイダー API キーの設定（Windows ユーザー環境変数に setx で永続保存） |

#### プロキシプロセス管理

- **起動**: `start_proxy` コマンドが `proxy::resolve_proxy_config()` で設定を解決し、`tokio::spawn` で axum サーバーを非同期タスクとして起動。起動後ポート 4000 を最大 5 秒間 150ms 間隔でポーリングし、listen を確認。API キーが未設定の場合はエラーを返す。
- **停止**: `stop_proxy` コマンドが oneshot チャネルで graceful shutdown を送信し、タスクの終了を待機。停止後ポート 4000 の開放を確認。
- **状態監視**: 3 秒間隔で `proxy_status` をポーリングし、`JoinHandle` の完了状態で予期せぬタスク終了を検知。

### Tauri コマンド一覧

| # | コマンド名 | 種別 | 説明 |
|---|-----------|------|------|
| 1 | `check_health` | async | `GET http://127.0.0.1:4000/health` でプロキシ死活確認 |
| 2 | `check_gateway_status` | sync | ポート 4000 の listen 状態 + tokio task の生存確認 |
| 3 | `check_api_key` | sync | 現在の active_provider の API キー環境変数の設定有無を返す |
| 4 | `set_env_api_key` | sync | `setx` コマンドで API キーをユーザー環境変数に永続保存 |
| 5 | `get_port_4000_process` | sync | `netstat` でポート 4000 を listen しているプロセスの PID を取得 |
| 6 | `read_config` | sync | `config.json` をパースして返す（旧形式の自動正規化あり） |
| 7 | `read_config_raw` | sync | `config.json` を生テキストで読み取り、エンコーディング自動判定 |
| 8 | `write_config` | sync | `config.json` を指定エンコーディング（UTF-8 / Shift-JIS）で保存 |
| 9 | `read_latest_log` | sync | `Communication-Logs/` 内の最新ログファイルを読み取り |
| 10 | `read_log` | sync | 指定ログファイルを読み取り（パストラバーサル対策あり） |
| 11 | `list_logs` | sync | `Communication-Logs/` 内のログファイル一覧を返す |
| 12 | `create_new_log` | sync | 新しい空ログファイルを作成 |
| 13 | `open_logs_folder` | sync | `Communication-Logs/` をエクスプローラで開く |
| 14 | `open_path` | sync | 任意パスをエクスプローラで開く（`%ENV_VAR%` 展開対応） |
| 15 | `find_claude_configs` | sync | Claude Desktop 設定ファイルを既知のパスから自動検出 |
| 16 | `start_proxy` | sync | API キー検証 → 設定解決 → axum を tokio::spawn → ポート確認 |
| 17 | `stop_proxy` | sync | oneshot シグナル送信 → graceful shutdown → ポート開放確認 |
| 18 | `proxy_status` | sync | JoinHandle の完了状態でタスクの生存確認 |

### プロキシサーバー (proxy.rs)

v0.3.0 で Python (FastAPI/httpx) から Rust (axum 0.7/reqwest) に完全移植。

#### エンドポイント

| Method | Path | 動作 |
|--------|------|------|
| GET | `/health` | 死活確認、`{"status": "ok", "upstream": "...", "provider": "..."}` を返す |
| GET | `/v1/models` | active_provider の `visible_models` に列挙されたモデル名のみ返す |
| POST | `/v1/messages` | メディアチェック → `model` を書換え後 upstream へ転送、stream / non-stream 両対応 |
| POST | `/v1/messages/count_tokens` | `supports_count_tokens=false` 時は 501。対応時は `model` を書換え後 upstream へ転送 |

#### モデル名書換え

`config.json` の active_provider の `model_map` に従い、リクエストの `model` フィールドを実モデル名に変換。マップにない場合は `default_model` にフォールバック。

#### メディアチェック

リクエストの messages 配列を走査し、image/video ブロックの有無を検出。プロバイダーの `supports_vision`/`supports_video` が false の場合、400 エラーを返す。

#### SSE 透過転送

reqwest の `bytes_stream()` で upstream から SSE イベントをバイト単位で受信し、`axum::body::Body::from_stream()` でそのまま返す。パース・再構築は行わない。

#### HTTP クライアント

`reqwest::Client`（プロセス共有）:
- 接続タイムアウト: 30 秒
- 全体タイムアウト: 300 秒

#### CORS

`tower-http::cors::CorsLayer` を使用。`server.enable_cors` が true の場合のみ有効。

### プロバイダー別機能マトリクス

| プロバイダー | count_tokens | vision | video |
|-------------|-------------|--------|-------|
| DeepSeek | ✗ | ✗ | ✗ |
| MiniMax | ✓ | ✓ | ✓ |
| Kimi | ✗ | ✓ | ✓ |

### config.json リファレンス

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "表示名",
      "upstream_url": "Anthropic互換APIのベースURL",
      "api_key_env": "APIキー環境変数名",
      "default_model": "フォールバックモデル名",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": {
        "claude-sonnet-4-5": "実モデル名"
      },
      "visible_models": [
        "claude-表示用-モデル名"
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

| キー | 型 | 説明 |
|------|-----|------|
| `active_provider` | string | 現在有効なプロバイダー ID（`providers` のキーと一致） |
| `providers.<id>.display_name` | string | GUI 表示用のプロバイダー名 |
| `providers.<id>.upstream_url` | string | Anthropic 互換 API のベース URL |
| `providers.<id>.api_key_env` | string | API キーを保持する Windows ユーザー環境変数名 |
| `providers.<id>.default_model` | string | model_map にないモデル名のフォールバック先 |
| `providers.<id>.force_anthropic_version` | string\|null | null 時はリクエストの `anthropic-version` ヘッダをそのまま転送。設定時は強制上書き |
| `providers.<id>.supports_count_tokens` | boolean | count_tokens エンドポイントのサポート有無 |
| `providers.<id>.supports_vision` | boolean | 画像入力のサポート有無 |
| `providers.<id>.supports_video` | boolean | 動画入力のサポート有無 |
| `providers.<id>.model_map` | object | Claude モデル名 → 実モデル名のマッピング |
| `providers.<id>.visible_models` | string[] | `GET /v1/models` で公開するモデル名一覧 |
| `server.host` | string | プロキシの listen アドレス |
| `server.port` | number | プロキシの listen ポート |
| `server.enable_cors` | boolean | CORS ミドルウェアの有効/無効 |

#### 後方互換性

v0.2.0 以前の単一プロバイダー形式（`model_map`, `visible_models`, `upstream_url` がルートにある形式）も `read_config` で自動的に新形式へ正規化されます。

### Claude Desktop 設定

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-deepseek-v4",
      "labelOverride": "DeepSeek V4 Pro via Gateway"
    },
    {
      "name": "claude-deepseek-flash",
      "labelOverride": "DeepSeek V4 Flash via Gateway"
    }
  ]
}
```

設定ファイルの場所（Windows）:
- `%APPDATA%\Claude\claude_desktop_config.json`
- `%USERPROFILE%\.claude\settings.json`
- `%LOCALAPPDATA%\Claude-3p\configLibrary\`

GUI の Claude Desktop Setup タブで自動検出・クリップボードコピーが可能。

### 実地テスト結果

| 経路 | モデル | stream | tools | msgs | 結果 |
|------|--------|--------|-------|------|------|
| Pro | claude-sonnet-4-5 → deepseek-v4-pro | ✓ | ✓ | 43 | PASS |
| Flash | claude-haiku-4-5-20251001 → deepseek-v4-flash | ✓ | ✓ | 17 | PASS |

両経路ともツール利用を含む長めの会話が最後まで完了。`reasoning_content` エラー・`Invalid model name` エラーは発生していない。

### 事前検証

DeepSeek Anthropic 互換 API の互換性を実装前に検証。全項目 PASS。

| # | 項目 | 結果 | 詳細 |
|---|------|------|------|
| 1 | non-stream `/v1/messages` | PASS | 200, "hello" |
| 2 | stream=true SSE 形式 | PASS | Anthropic SSE 形式, 全 7 種 event type |
| 3 | thinking block | PASS | ['thinking', 'text'], reasoning_content 混入なし |
| 4 | 2nd turn pass-back | PASS | reasoning_content エラーなし |
| 5 | tool_use block | PASS | ['thinking', 'tool_use'], stop_reason=tool_use |
| 6 | tool_result 2nd turn | PASS | tool_result 使用応答成功 |
| 7 | count_tokens | PASS | input_tokens=10 |
| 8 | header handling | PASS | anthropic-beta 未知値も 200 |

---

## English

### Overview

A thin proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

### Background

Claude Desktop / Claude Code sends requests directly to the Anthropic Messages API (`/v1/messages`). By routing these through each provider's Anthropic-compatible endpoint, models from multiple providers can be used transparently from Anthropic clients.

#### Problems Solved

- Claude Desktop model name validation
- Information loss from LiteLLM Anthropic→OpenAI conversion
- Unregistered model names such as `claude-haiku-4-5-20251001`
- Single-provider lock-in (switchable between DeepSeek, MiniMax, Kimi)

#### Known Limitations

- DeepSeek Anthropic-compatible API may not fully support thinking blocks in all cases
- `tool_use` / `tool_result` / streaming SSE compatibility may be incomplete in edge cases

### Architecture

```
Claude Desktop / Claude Code
       │
       ▼
proxy.rs (127.0.0.1:4000)  ← Embedded in Tauri app (axum 0.7 + reqwest)
       │
       │ Rewrites only the model field
       │ Everything else passes through untouched
       │ (messages / thinking / tool_use / tool_result / SSE)
       │ Media support checking per provider
       ▼
Selected provider's Anthropic-compatible API
(DeepSeek / MiniMax / Kimi)
```

#### Design Principles

- **Thin proxy**: Nothing is modified except the `model` field. SSE events are forwarded byte-for-byte without parsing or reconstruction.
- **Lossless forwarding**: Message bodies, tool calls, and thinking blocks pass through unmodified.
- **Multi-provider**: Switch providers via `active_provider` in `config.json`. Each provider has its own upstream URL, API key env var, model_map, and capability flags.
- **Windows-native GUI**: Tauri v2 + React + TypeScript. Rust backend, Vite + React 19 frontend.
- **Zero external dependencies**: As of v0.3.0, the proxy is written in Rust and embedded in the Tauri binary. Python is not required.

### GUI Manager

Built with Tauri v2 + React + TypeScript. Four-tab layout.

```
┌──────────────────────────────────────────┐
│  Anthropic Proxy Gateway Manager          │
│  [Gateway: Running] [Start/Stop] [EN|JA] │
├──────────────────────────────────────────┤
│  Dashboard │ Settings │ ClaudeSetup │ API Key │
├──────────────────────────────────────────┤
│  Status      │  Latest Log               │
│  - Port 4000 │  - Log switcher           │
│  - API Key   │  - New log                │
│  - URL       │  - Usage counters         │
└──────────────────────────────────────────┘
```

| Tab | Function |
|-----|----------|
| Dashboard | Port 4000 status, API key status, Gateway URL, latest log viewer, usage counters |
| Gateway Settings | Raw config.json editor, UTF-8/Shift-JIS encoding toggle, provider switching, save/reload |
| Claude Desktop Setup | Config JSON display + clipboard copy, auto-detect config files, manual folder browse |
| API Key | Set API keys per provider (persisted via `setx` to Windows user environment variable) |

#### Proxy Process Management

- **Start**: The `start_proxy` command resolves the config via `proxy::resolve_proxy_config()`, validates the API key, then spawns the axum server as an async task via `tokio::spawn`. After spawning, port 4000 is polled every 150ms for up to 5 seconds to confirm the proxy is listening. Returns an error if the API key is not set.
- **Stop**: The `stop_proxy` command sends a graceful shutdown signal via a oneshot channel and awaits task completion. Port 4000 release is verified.
- **Status monitoring**: The frontend polls `proxy_status` every 3 seconds, checking `JoinHandle::is_finished()` to detect unexpected task termination.

### Tauri Commands

| # | Command | Type | Description |
|---|---------|------|-------------|
| 1 | `check_health` | async | Proxies `GET http://127.0.0.1:4000/health` |
| 2 | `check_gateway_status` | sync | Checks port 4000 listen state + tokio task liveness |
| 3 | `check_api_key` | sync | Returns whether the active provider's API key env var is set |
| 4 | `set_env_api_key` | sync | Persists API key via `setx` to user environment variable |
| 5 | `get_port_4000_process` | sync | Gets PID of process listening on port 4000 via `netstat` |
| 6 | `read_config` | sync | Reads and parses `config.json` (auto-normalizes legacy format) |
| 7 | `read_config_raw` | sync | Reads `config.json` as raw text with auto encoding detection |
| 8 | `write_config` | sync | Saves `config.json` with specified encoding (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | Reads the latest log file from `Communication-Logs/` |
| 10 | `read_log` | sync | Reads a specific log file by name (path traversal safe) |
| 11 | `list_logs` | sync | Lists all log files in `Communication-Logs/` |
| 12 | `create_new_log` | sync | Creates a new empty log file |
| 13 | `open_logs_folder` | sync | Opens `Communication-Logs/` in Explorer |
| 14 | `open_path` | sync | Opens an arbitrary path in Explorer (supports `%ENV_VAR%` expansion) |
| 15 | `find_claude_configs` | sync | Auto-discovers Claude Desktop config files from known paths |
| 16 | `start_proxy` | sync | Validates API key → resolves config → spawns axum via tokio::spawn → confirms port |
| 17 | `stop_proxy` | sync | Sends oneshot signal → graceful shutdown → confirms port release |
| 18 | `proxy_status` | sync | Returns task liveness via JoinHandle status |

### Proxy Server (proxy.rs)

Fully ported from Python (FastAPI/httpx) to Rust (axum 0.7/reqwest) in v0.3.0.

#### Endpoints

| Method | Path | Behavior |
|--------|------|----------|
| GET | `/health` | Health check, returns `{"status": "ok", "upstream": "...", "provider": "..."}` |
| GET | `/v1/models` | Returns only models listed in active provider's `visible_models` |
| POST | `/v1/messages` | Media check → rewrites `model` field → forwards to upstream (stream + non-stream) |
| POST | `/v1/messages/count_tokens` | Returns 501 if `supports_count_tokens=false`. Otherwise rewrites model and forwards |

#### Model Name Rewriting

The `model` field in requests is rewritten according to the active provider's `model_map`. Unmapped models fall back to `default_model`.

#### Media Support Checking

Scans the messages array for image/video content blocks. Returns 400 if the provider's `supports_vision` or `supports_video` flag is false.

#### SSE Transparent Forwarding

SSE events are received byte-by-byte from upstream via `reqwest::bytes_stream()` and returned directly via `axum::body::Body::from_stream()` without parsing or reconstruction.

#### HTTP Client

`reqwest::Client` (process-shared):
- Connect timeout: 30s
- Overall timeout: 300s

#### CORS

Uses `tower-http::cors::CorsLayer`. Enabled only when `server.enable_cors` is true.

### Provider Capability Matrix

| Provider | count_tokens | vision | video |
|----------|-------------|--------|-------|
| DeepSeek | ✗ | ✗ | ✗ |
| MiniMax | ✓ | ✓ | ✓ |
| Kimi | ✗ | ✓ | ✓ |

### config.json Reference

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display Name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key environment variable name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": {
        "claude-sonnet-4-5": "actual-model-name"
      },
      "visible_models": [
        "claude-display-model-name"
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

| Key | Type | Description |
|-----|------|-------------|
| `active_provider` | string | Currently active provider ID (matches a key in `providers`) |
| `providers.<id>.display_name` | string | Provider display name for the GUI |
| `providers.<id>.upstream_url` | string | Anthropic-compatible API base URL |
| `providers.<id>.api_key_env` | string | Windows user environment variable holding the API key |
| `providers.<id>.default_model` | string | Fallback when model not in map |
| `providers.<id>.force_anthropic_version` | string\|null | `null` = forward request header; set to override |
| `providers.<id>.supports_count_tokens` | boolean | Whether count_tokens endpoint is supported |
| `providers.<id>.supports_vision` | boolean | Whether image input is supported |
| `providers.<id>.supports_video` | boolean | Whether video input is supported |
| `providers.<id>.model_map` | object | Claude model name → actual model name mapping |
| `providers.<id>.visible_models` | string[] | Models exposed via `GET /v1/models` |
| `server.host` | string | Proxy listen address |
| `server.port` | number | Proxy listen port |
| `server.enable_cors` | boolean | Enable/disable CORS middleware |

#### Backward Compatibility

Pre-v0.2.0 single-provider configs (with `model_map`, `visible_models`, `upstream_url` at the root) are automatically normalized to the new format by `read_config`.

### Claude Desktop Configuration

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-deepseek-v4",
      "labelOverride": "DeepSeek V4 Pro via Gateway"
    },
    {
      "name": "claude-deepseek-flash",
      "labelOverride": "DeepSeek V4 Flash via Gateway"
    }
  ]
}
```

Config file locations (Windows):
- `%APPDATA%\Claude\claude_desktop_config.json`
- `%USERPROFILE%\.claude\settings.json`
- `%LOCALAPPDATA%\Claude-3p\configLibrary\`

The GUI's Claude Desktop Setup tab provides auto-detection and clipboard copy.

### Field Test Results

| Route | Model | stream | tools | msgs | Result |
|-------|-------|--------|-------|------|--------|
| Pro | claude-sonnet-4-5 → deepseek-v4-pro | ✓ | ✓ | 43 | PASS |
| Flash | claude-haiku-4-5-20251001 → deepseek-v4-flash | ✓ | ✓ | 17 | PASS |

Both routes completed multi-turn tool-use conversations with no `reasoning_content` or `Invalid model name` errors.

### Pre-Implementation Verification

All compatibility probes against DeepSeek's Anthropic-compatible API passed before implementation.

| # | Test | Result | Detail |
|---|------|--------|--------|
| 1 | non-stream `/v1/messages` | PASS | 200, "hello" |
| 2 | stream=true SSE format | PASS | Anthropic SSE format, all 7 event types |
| 3 | thinking block | PASS | ['thinking', 'text'], no reasoning_content leakage |
| 4 | 2nd turn pass-back | PASS | No reasoning_content error |
| 5 | tool_use block | PASS | ['thinking', 'tool_use'], stop_reason=tool_use |
| 6 | tool_result 2nd turn | PASS | tool_result response successful |
| 7 | count_tokens | PASS | input_tokens=10 |
| 8 | header handling | PASS | Unknown anthropic-beta values return 200 |
