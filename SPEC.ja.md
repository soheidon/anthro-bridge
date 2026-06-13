[English](SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md)

# SPEC: Terra Bridge

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code Desktop から利用するための薄型プロキシ + GUI 管理ツール。

### 背景

Claude Desktop / Claude Code Desktop は Anthropic Messages API (`/v1/messages`) に直接リクエストを送る。これを各プロバイダーの Anthropic 互換エンドポイントに振り向けることで、複数プロバイダーのモデルを Anthropic クライアントから透過的に利用可能にする。

#### 解決する問題

- Claude Desktop 側のモデル名バリデーション（`inferenceModels[].name` に Anthropic 公式名しか使えない）
- LiteLLM の Anthropic->OpenAI 変換による情報ロス
- 複数プロバイダー・複数モデルを単一エンドポイントから利用可能

### アーキテクチャ

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Tauri アプリに内蔵 (axum 0.7 + reqwest)
       |
       | リクエストの model フィールドをもとにプロバイダーを自動判別
       | model を upstream 名に書換え、他は完全透過転送
       | thinking 注入（thinking=disabled のバリアント向け）
       | モデル単位の画像/動画非対応チェック
       v
各プロバイダーの Anthropic-compatible API
(DeepSeek / MiniMax / Kimi)
```

#### 設計方針

- **シェルモデル + プロバイダ選択**: Claude Desktop には常に `claude-sonnet-4-6` / `claude-haiku-4-5` の2モデルを表示。実際の LLM は GUI で選択（DeepSeek / MiniMax / Kimi）。選択されたプロバイダのモデルマッピングがルーティングに使われる。
- **アクティブプロバイダのみ API キー必須**: v0.5.0 以降、起動時にチェックされる API キーはルートテーブルで参照されるプロバイダのみ。非アクティブプロバイダのキーは不要。
- **薄型プロキシ**: model フィールドの書換え以外は一切手を加えない。SSE もパースせずバイト単位で透過転送。
- **ロスレス転送**: メッセージ本文やツール呼び出し、thinking block を一切加工しない。
- **Windows ネイティブ GUI**: Tauri v2 + React 19 + TypeScript。バックエンドは Rust、フロントエンドは Vite + React 19。
- **ゼロ外部依存**: v0.3.0 以降、プロキシは Rust に移植され Tauri バイナリに内蔵。Python 不要。
- **多言語対応**: v0.5.0 から6言語（en, ja, zh-CN, zh-TW, ko, fr）。`lang/` フォルダにファイル追加で新言語対応可。初回起動時に言語選択画面。

### GUI 管理ツール

Tauri v2 + React 19 + TypeScript 製。ダッシュボード + 設定画面の2画面構成。

```
+------------------------------------------+
|  Terra Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiniMax] [Kimi]          ||
|  +- Status ------------------------------+
|  | Port 4000 | API Key | Gateway URL    ||
|  | Model routing table                  ||
|  +- Latest Log --------------------------+
|  | Log viewer with Pro/Flash counters   ||
|  +---------------------------------------+
+------------------------------------------+

設定画面 (=):
  +- Language ----------------------------+
  | ドロップダウンで即時切替              |
  +- API Key -----------------------------+
  | プロバイダごとのAPIキー管理           |
  +- Claude Desktop Setup ----------------+
  | 設定JSON生成・コピー・ファイル検出    |
  +- Gateway Config ----------------------+
  | config.json 編集（上級者向け）        |
  +---------------------------------------+
```

### Tauri コマンド一覧

| # | コマンド名 | 種別 | 説明 |
|---|-----------|------|------|
| 1 | `check_health` | async | プロキシ死活確認 |
| 2 | `check_gateway_status` | sync | ポート 4000 + tokio task の生存確認 |
| 3 | `check_api_key` | sync | アクティブプロバイダの API キー状態 |
| 4 | `set_env_api_key` | sync | setx で API キー永続保存 |
| 5 | `get_port_4000_process` | sync | netstat でポート 4000 の PID 取得 |
| 6 | `read_config` | sync | config.json 読込 |
| 7 | `read_config_raw` | sync | config.json 生テキスト + エンコーディング判定 |
| 8 | `write_config` | sync | config.json 保存（UTF-8 / Shift-JIS） |
| 9 | `read_latest_log` | sync | 最新ログ読込 |
| 10 | `read_log` | sync | 指定ログファイル読込 |
| 11 | `list_logs` | sync | ログファイル一覧 |
| 12 | `create_new_log` | sync | 新規ログファイル作成 |
| 13 | `open_logs_folder` | sync | ログフォルダを開く |
| 14 | `open_path` | sync | 任意パスを開く |
| 15 | `find_claude_configs` | sync | Claude Desktop 設定ファイル自動検出 |
| 16 | `start_proxy` | sync | プロキシ起動（設定解決 -> spawn -> ポート確認） |
| 17 | `stop_proxy` | sync | プロキシ停止（graceful shutdown） |
| 18 | `proxy_status` | sync | タスク生存確認 |
| 19 | `check_all_api_keys` | sync | 全プロバイダの API キー状態 |
| 20 | `update_active_provider` | sync | active_provider 保存 |
| 21 | `update_provider_api_key_env` | sync | プロバイダの api_key_env 保存 |
| 22 | `get_user_language` | sync | 保存された言語設定の取得 |
| 23 | `set_user_language` | sync | 言語設定の保存 |
| 24 | `is_first_run` | sync | 初回起動判定（user_prefs.json の有無） |

### プロキシサーバー (proxy.rs)

v0.3.0 で Python から Rust (axum 0.7/reqwest) に移植。

#### エンドポイント

| Method | Path | 動作 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | 公開モデル一覧（`visible: true` のみ） |
| POST | `/v1/messages` | モデル解決 -> thinking 注入 -> メディアチェック -> 転送（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 対応時のみ upstream へ転送 |

#### モデルルーティング

各プロバイダの `models` セクションから gateway model -> (provider, upstream model) の逆引きテーブルを構築。全プロバイダが同じ gateway model 名を使うため、`active_provider` が衝突時に優先される。結果としてアクティブプロバイダのモデルのみがルートテーブルに登録される。

#### API キー検証（v0.5.0〜）

Pass 1: モデルルートテーブル構築（APIキー不要）
Pass 2: ルートテーブルで参照されるプロバイダのみ API キーをチェック

#### Thinking 注入

`models` エントリに `thinking: "disabled"` が指定されているモデルに対し、ユーザーが明示的に thinking を設定していない場合のみ `{"type": "disabled"}` を注入。

#### メディアチェック / 画像サニタイズ

モデル単位の `supports_vision` / `supports_video` フラグで判定。非対応モデルに画像が送られた場合、`non_vision_image_policy` に従う:
- `replace`（デフォルト）: 画像ブロックをプレースホルダテキストに置換
- `drop`: 画像ブロックを削除（空になった場合はプレースホルダ挿入）
- `reject`: 400 エラーを返す

動画ブロックは常に 400 エラー。`non_vision_image_policy` は `/health` エンドポイントで確認可能。

### 多言語対応

`gui/src/i18n/lang/` に1言語1ファイル。`import.meta.glob` で自動検出。

```
gui/src/i18n/lang/
  en.ts      英語（正規 — TranslationKey 型を定義）
  ja.ts      日本語
  zh-CN.ts   中国語(簡体)
  zh-TW.ts   中国語(繁体)
  ko.ts      韓国語
  fr.ts      フランス語
```

新規言語追加: `en.ts` をコピー -> 翻訳 -> 再ビルド。コード変更不要。

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
      "model_map": { "claude-sonnet-4-5": "実モデル名" },
      "visible_models": ["claude-公開モデル名"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "実モデル名",
          "thinking": "disabled",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
}
```
