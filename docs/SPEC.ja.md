[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## 概要

Claude Desktop / Claude Code の API リクエストを、複数プロバイダーの Anthropic 互換エンドポイントへルーティングする薄型プロキシ + GUI 管理ツール。

### アーキテクチャ

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Tauri アプリに内蔵 (axum 0.7 + reqwest)
       |
       | model フィールドでルーティング -> 正しい upstream プロバイダーを解決
       | model のみを upstream 名に書換え
       | 非思考バリアント向けに thinking disabled を注入
       | モデル単位のメディア対応チェック
       v
プロバイダーの Anthropic 互換 API
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### 設計方針

- **シェルモデル + プロバイダー選択**: Claude Desktop には常に `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5` の3モデルが表示される。実際の LLM は GUI で選択する（DeepSeek / MiniMax / Kimi / MiMo / OpenRouter）。アクティブプロバイダーのモデルマッピングがルーティングに使われる。
- **OpenRouter 対応**: Poolside Laguna S/XS をデフォルトとして、OpenRouter の Anthropic 互換エンドポイントへルーティングする。専用の thinking モード制御（Max/On/Off）は、リクエスト時に OpenRouter の `reasoning` フォーマットへ変換される。
- **API キーが必須なのはアクティブプロバイダーのみ**: v0.5.0 以降、起動時にチェックされるのはルートテーブルで参照されるプロバイダーのみ。非アクティブプロバイダーのキーは不要。
- **薄型プロキシ**: `model` フィールド以外は一切変更しない。SSE はバイト単位で透過転送。
- **ロスレス転送**: メッセージ本文、ツール呼び出し、thinking ブロックは加工されずにそのまま通過する。
- **Windows ネイティブ GUI**: Tauri v2 + React 19 + TypeScript。バックエンドは Rust、フロントエンドは Vite + React 19。
- **ゼロ外部依存**: v0.3.0 以降、プロキシは Tauri バイナリに内蔵。Python は不要。
- **多言語対応**: 8言語（en, ja, zh-CN, zh-TW, ko, fr, de, es）。`lang/` にファイルを置くだけで新言語を追加できる。初回起動時に言語選択画面を表示。
- **推論強度**: DeepSeek V4 Pro は Thinking モードで推論強度 High / Max、V4 Flash は Low / High / Max に対応。推論強度は Normal モードでは無効化される。V4 Pro ルートに保存されたレガシーな `low`/`medium` は、起動時に `high` へ移行される。
- **機能検出**: OpenRouter API から取得したライブの機能フラグ（supports_image_url、supports_image_base64、supports_video_url、supports_video_base64）を config.json に永続化する。
- **ピーク/バレー料金の認識**: DeepSeek と OpenRouter のピーク時間帯をローカルタイムゾーンで表示する。
- **MiniMax-M3 thinking トグル**: MiniMax-M3 は Anthropic 互換 API で Thinking ON/OFF に対応する（`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`）。M2.x 系モデルは引き続き thinking 専用。起動時マイグレーションが既存ユーザーのレガシー `thinking_only` を `thinking` へ変換する。
- **レスポンスモデル ID 正規化**: API レスポンス（SSE ストリーミング / ノンストリーミングの両方）内の upstream モデル名を、Anthropic 公式モデル名へ書き戻す。config.json の `normalize_response_model_identity` と実行時の `AtomicBool` で制御する。サーバー設定の保存との相互汚染を避けるため、独立した保存コマンド（`update_normalize_model_identity`）を使用する。
- **構造化通信ログ**: `tracing` + `tracing-appender` が構造化ログを `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log` に書き込む。各リクエストには `AtomicU64` カウンターから相関 ID が付与される。ログエントリにはリクエストモデル、ゲートウェイモデル、upstream モデル、正規化の結果、スキップ理由が含まれる。機密データ（プロンプト、本文、API キー）は記録されない。
- **PEAK バッジ**: ピーク価格のモデルに対して、ダッシュボードに色分けされたピンクのバッジを表示する。
- **UTC オフセット表示**: タイムゾーンセレクターに、各オプションの横へ動的な UTC オフセット（例: UTC+09:00）を表示する。
- **Laguna S/XS 2.1 トークン上限失敗の検出**: SSE ストリームとノンストリームレスポンスの両方で、`stop_reason: "max_tokens"` を伴う reasoning のみのレスポンスを検出する。利用可能なテキストやツール呼び出しを生成せずに毎ターンのトークン上限に達した場合、警告をログに記録する。OpenRouter 経由のすべての Poolside Laguna モデルで利用可能。
- **Poolside thinking:disabled パススルー**: Poolside モデル向けに、クライアントが送信した `thinking: { type: "disabled" }` を OpenRouter の `reasoning: { enabled: false }` フォーマットへ変換し、保存済みの設定がなくても disabled thinking が正しく転送されるようにする。
- **Laguna Opus デフォルト移行**: `poolside/laguna-s-2.1` の OpenRouter ユーザー向けに、`claude-opus-5` のデフォルトを thinking-on から通常モードへ変更する、1回限りの冪等なマイグレーション。新規インストール用テンプレートは更新後のデフォルトを反映する。
- **OpenRouter マルチモデルセット**: ユーザーごとに複数の OpenRouter モデルセットを持つことができ、各モデルセットは独自の API キーとモデル設定を持つ。モデルセットの CRUD は Tauri コマンド経由で行う。ダッシュボードまたは設定からアクティブなモデルセットを切り替える。モデルセットはドラッグ & ドロップで並べ替え、非表示化、設定した順序で永続化が可能。
- **OpenRouter ダッシュボードカード**: ダッシュボードは、表示中の OpenRouter モデルセットごとに1枚のカードを作成し、モデルセットが無い場合はフォールバックカードを表示する。モデル概要では OpenRouter 表示専用に、最初の `/` より前のベンダーネームスペースを非表示にする。ルーティング用の完全な upstream ID は変更されない。
- **OpenRouter モデルレジストリ**: 既知の OpenRouter モデルのローカル組み込みレジストリ（`model_capabilities.rs`、`builtinOpenRouter.ts`）。事前設定済みの機能（vision、video、thinking ポリシー、推論強度）、ベンダーグルーピング、価格データを含む。ライブ API 呼び出しなしでモデル分類に使用する。
- **OpenRouter 価格詳細**: 組み込み価格は、入力、出力、キャッシュ入力レートの現在価格と改定後標準価格に対応する。GPT-5.6 の Sol、Terra、Luna、Pro 各バリアントを含む。GUI はプロモーション価格と標準価格の両方が利用可能な場合、両方を併記して表示する。
- **GPT-5.6 モデル対応**: OpenRouter モデルセットは Sol、Terra、Luna のモデルバリアントを使用でき、機能を考慮した thinking 制御と、該当する場合の長文コンテキスト料金に関する価格メモを備える。組み込みの OpenAI GPT-5.6 Balanced モデルセットは、新規インストールでは Opus 5 → GPT-5.6 Sol、Sonnet 5 → GPT-5.6 Terra、Haiku 4.5 → GPT-5.6 Luna へ、3ルートすべてで Thinking High の推論強度でルーティングする。既存の保存済みルーティングは自動では変更されない。
- **ダッシュボード駆動のウィンドウサイズ調整**: 初期表示時と行数変更時に、3列グリッド内の表示中のダッシュボードカードからウィンドウの高さを計算する。この計算はカードの高さ、グリッドの隙間、ネイティブの最小サイズ、モニターのワークエリア、DPI スケーリング、ウィンドウ装飾を考慮しつつ、行数が変わらない場合は手動リサイズを保持する。
- **ローカライズされた NSIS インストーラー**: Windows インストーラーは英語、日本語、中国語（簡体）、中国語（繁体）、韓国語、フランス語、ドイツ語、スペイン語の言語選択を提供し、Anthro Bridge のアプリケーションアイコンを同梱する。
- **リグレッションカバレッジ**: Vitest のカバレッジには、OpenRouter モデルセットの並び順と保存レース、本番価格データ、ダッシュボードのカード数セマンティクス、モニター考慮のウィンドウサイズ調整が含まれる。
- **OpenRouter 経由の新プロバイダー**: InclusionAI と StepFun を OpenRouter モデルプロバイダーとして追加する。専用の機能フラグ、thinking モード制御、ベンダーグルーピングを備える。
- **Tencent Hy3 thinking モード**: Tencent の Hunyuan モデルで推論強度 Low/High に対応する。proxy.rs の thinking モード変換は `thinking_mode` を OpenRouter の `reasoning` フォーマットへマッピングする。UI は Low/High をドロップダウンオプションとして表示する。
- **Kimi K3 修正**: 機能定義からハードコードされた `forced_reasoning_effort` を削除した。固定の「Max」表示を設定可能なドロップダウンセレクターに置き換えた。デフォルト値は保存済み設定から取得し、フォールバックとして "max" を使用する。
- **設定書き込みの直列化**: config を書き込むすべての Tauri コマンドは、`Mutex` ガード付きの `execute_serialized_config_mutation` を介して直列化される。`ConfigState` 構造体は検証付きで `applied_config`、`in_flight_config`、`pending_ops` を追跡する。複数の設定変更が同時に保存される際のレースコンディションを防ぐ。
- **OpenRouter UI レース修正**: (1) `syncUiFromSavedRouteRef` の最新コールバック ref が、古いクロージャによる新ルート UI の上書きを防ぐ。(2) `rollbackRouteId` ガードがルート間の Phase 2 ロールバックを防ぐ。(3) `useRouteSaveGeneration` フックが全ハンドラーに `begin()`/`isCurrent()` の世代ガードを提供する。(4) 保存キューフック（`useOpenRouterSaveQueue`）がドレインループ、supersede 検出、再起動時の OR 再集計を提供する。
- **Dev/Stable アプリ ID の分離**: `paths.rs` の `AppChannel` enum（`Stable`/`Dev`）が、それぞれ別の識別子（`com.soheidon.anthro-bridge` vs `.dev`）、設定ディレクトリ（`Anthro Bridge` vs `Anthro Bridge Dev`）、キャッシュパスを選択する。Dev チャンネルは `tauri.dev.conf.json` を使用する。NPM スクリプト: `npm run dev`（dev）、`npm run dev:stable`（stable）。
- **設定テンプレートの埋め込み**: `include_str!()` が `config_template.rs` をコンパイル時に埋め込み、同梱の `config.json` への実行時依存を排除する。`merge_bundled_providers` は型付きエラーハンドリング付きで `Result` を返す。
- **フロントエンドのリグレッションテスト**: `QueueHarness` と `GenerationHandlerHarness` を使用した、OpenRouter 保存レース条件に対する vitest リグレッションテスト 7 件。テスト対象: 最新コールバック ref、ルート間ロールバックガード、ID キャプチャ、リフレッシュ再試行（失敗 + 成功パス）、実行中 supersede、世代ガード。
- **Claude Code コンテキスト管理**: Claude Code 向けのモデル認識型自動圧縮。`resolve_effective_auto_compact` は各標準ルート（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）をその upstream モデルへ解決し、各モデルのコンテキスト容量を静的レジストリ `model_context_windows.json` で参照し、Auto モードでは既知の最小容量を安全なコンテキストウィンドウとして使用する。コンテキスト制御は3つすべての容量が既知の場合のみ適用される（それ以外はステータスが Incomplete）。ヘッダーのトグルでコンテキスト管理のオン/オフを切り替える。高度なモードとしきい値は `config.json` の `claude_code.auto_compact` 配下で設定する。モード: `auto`、`manual`（`window_tokens`）、`claude_default`。
- **Claude Code 起動コマンド生成**: `build_claude_code_launch_command` は、ゲートウェイ接続変数（ローカルゲートウェイを指す `ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`）と Claude Code コンテキスト制御変数（`CLAUDE_CODE_AUTO_COMPACT_WINDOW`、`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`）を組み合わせた完全な PowerShell コマンドを生成する。コンテキスト管理が無効、Incomplete、または Claude デフォルト設定の場合、コマンドは `Remove-Item Env:... -ErrorAction SilentlyContinue` で古いコンテキスト変数を削除し、以前設定されたセッション値が新しい起動に漏れないようにする。Claude 設定パネルの「Claude Code起動コマンドをコピー」ボタンがコマンドをクリップボードにコピーする。Anthro Bridge はコマンドの生成とコピーのみを行い、実行は一切しない。
- **共有モデルルーティングモジュール**: `model_routing.rs` は、ルートから upstream への解決を `proxy.rs` とコンテキストリゾルバーが共有する純粋関数に抽出し、コンテキストウィンドウがプロキシが実際に転送する upstream モデルと同一のモデルを解決することを保証する。
- **コンテキスト容量レジストリ**: `model_context_windows.json` は、既知のコンテキスト容量の静的レジストリ。組み込みの直接プロバイダーモデル（DeepSeek、MiniMax、Kimi、MiMo）と組み込みの OpenRouter モデル（Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6）をカバーする。不明なカスタム OpenRouter モデルは有効なルートターゲットのままであるが、メタデータが追加されるか手動モードが設定されるまで、コンテキスト管理は Incomplete として報告される。

### GUI 管理ツール

Tauri v2 + React 19 + TypeScript。ダッシュボード + 設定の2画面構成。

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  ダッシュボード                            |
|  +- LLMプロバイダ選択 -------------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]          ||
|  +- ステータス ----------------------------+
|  | Port 4000 | APIキー | Gateway URL    ||
|  | モデルルーティングテーブル            ||
|  +- 最新ログ -----------------------------+
|  | Pro/Flashカウンター付きログビューア   ||
|  +---------------------------------------+
+------------------------------------------+

設定 (=):
  +- 言語 ---------------------------------+
  | 即時切り替え用ドロップダウン           |
  +- APIキー -------------------------------+
  | プロバイダーごとのAPIキー管理          |
  +- Claude Desktop セットアップ ----------+
  | 設定JSONの生成、コピー、                |
  | 設定ファイルの検出                      |
  +- ゲートウェイ設定 ---------------------+
  | config.json エディタ（上級者向け）     |
  +---------------------------------------+
```

### Tauri コマンド一覧

| # | コマンド | 種別 | 説明 |
|---|---------|------|------|
| 1 | `check_health` | async | プロキシの死活確認 |
| 2 | `check_gateway_status` | sync | ポート 4000 + tokio タスクの生存確認 |
| 3 | `check_api_key` | sync | アクティブプロバイダーの API キー状態 |
| 4 | `set_env_api_key` | sync | setx で API キーを永続保存 |
| 5 | `get_port_4000_process` | sync | netstat でポート 4000 の PID を取得 |
| 6 | `read_config` | sync | config.json を読み込む |
| 7 | `read_config_raw` | sync | config.json の生テキスト + エンコーディング判定 |
| 8 | `write_config` | sync | config.json を保存（UTF-8 / Shift-JIS） |
| 9 | `read_latest_log` | sync | 最新ログを読み込む |
| 10 | `read_log` | sync | 指定したログファイルを読み込む |
| 11 | `list_logs` | sync | ログファイルの一覧 |
| 12 | `create_new_log` | sync | 新しいログファイルを作成 |
| 13 | `open_logs_folder` | sync | ログフォルダを開く |
| 14 | `open_path` | sync | 任意のパスを開く |
| 15 | `find_claude_configs` | sync | Claude Desktop 設定ファイルを自動検出 |
| 16 | `start_proxy` | sync | プロキシを起動（設定解決 -> spawn -> ポート確認） |
| 17 | `stop_proxy` | sync | プロキシを停止（グレースフルシャットダウン） |
| 18 | `proxy_status` | sync | タスクの生存確認 |
| 19 | `check_all_api_keys` | sync | 全プロバイダーの API キー状態 |
| 20 | `update_active_provider` | sync | active_provider を保存 |
| 21 | `update_provider_api_key_env` | sync | プロバイダーの api_key_env を保存 |
| 22 | `get_user_language` | sync | 保存済みの言語設定を取得 |
| 23 | `set_user_language` | sync | 言語設定を保存 |
| 24 | `is_first_run` | sync | 初回起動かどうかを判定（user_prefs.json の有無） |
| 25 | `openrouter_get_models` | async | OpenRouter モデルカタログを取得/キャッシュ |
| 26 | `set_model_upstream` | sync | ゲートウェイモデルの upstream モデル + thinking 設定 + 機能フラグを保存 |
| 27 | `update_server_config` | sync | サーバーのホスト/ポート/CORS 設定を保存 |
| 28 | `update_normalize_model_identity` | sync | レスポンスモデル ID 正規化トグルを保存（config + 実行時 AtomicBool を更新） |
| 29 | `update_claude_code_auto_compact_global` | sync | グローバルの Claude Code コンテキスト管理をトグル（有効化 + トリガー割合） |
| 30 | `update_claude_code_auto_compact_target` | sync | プロバイダー/モデルセットごとのコンテキストモード（auto / manual / claude_default）+ 手動ウィンドウトークンを設定 |
| 31 | `update_claude_code_context_settings` | sync | グローバル + ターゲットのコンテキスト設定をアトミックにまとめて更新 |
| 32 | `resolve_claude_code_auto_compact` | sync | 有効なコンテキスト設定（モード、ウィンドウトークン、トリガー割合、ステータス）を解決 |
| 33 | `build_claude_code_launch_command` | sync | 完全な PowerShell の Claude Code 起動コマンドを生成（ゲートウェイ + コンテキスト環境変数） |

### プロキシサーバー (proxy.rs)

v0.3.0 で Python から Rust（axum 0.7/reqwest）に移植。

#### エンドポイント

| Method | Path | 動作 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | 公開モデル一覧（`visible: true` のみ） |
| POST | `/v1/messages` | モデル解決 -> thinking 注入 -> メディアチェック -> 転送（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 対応時のみ upstream へ転送 |

#### モデルルーティング

各プロバイダーの `models` セクションを使用して、gateway model -> (provider, upstream model) の逆引きテーブルを構築する。全プロバイダーが同じ gateway モデル名を使用するため、衝突時は `active_provider` が優先される。結果として、アクティブプロバイダーのモデルのみがルートテーブルに残る。

#### API キー検証（v0.5.0〜）

Pass 1: モデルルートテーブルを構築（API キー不要）
Pass 2: ルートテーブルで参照されるプロバイダーの API キーのみチェック

#### Thinking 注入

設定エントリに `thinking: "disabled"` が含まれるモデルに対して、ユーザーが明示的に thinking を設定していない場合のみ `{"type": "disabled"}` を注入する。

#### レスポンスモデル正規化

`normalize_response_model_identity` が有効な場合、プロキシは upstream レスポンス内の `model` フィールドを書き換える:

- **ノンストリーミング**: JSON レスポンスをパースし、`model` を Anthropic 標準名へ書き換えて再シリアライズ
- **ストリーミング (SSE)**: `message_start` イベントフレームをインターセプトし、SSE のフォーマットと空白を保つためバイト範囲置換で `model` をその場で書き換え
- **スキップ理由**: `disabled`（トグルオフ）、`non_success_status`（200 以外のレスポンス）、`content_encoding_not_transformable`（gzip/brotli）、`stream_error`、`stream_cancelled`
- **判定ロジック**: 本番コードとテストの両方で使用される純粋関数（`should_normalize_nonstream`、`nonstream_skip_reason`）

#### メディアチェック / 画像サニタイズ

モデル単位の `supports_vision` / `supports_video` フラグが動作を決定する。画像を受け取った vision 非対応モデルには `non_vision_image_policy` が適用される:
- `replace`（デフォルト）: 画像ブロックをプレースホルダーテキストに置換
- `drop`: 画像ブロックを削除（コンテンツが空になった場合はプレースホルダーを挿入）
- `reject`: 400 エラーを返す

動画ブロックは常に 400 エラーを返す。`non_vision_image_policy` は `/health` で確認できる。

#### Claude Code コンテキスト管理

Claude Code のコンテキスト制御は、公式の環境変数 2 つを使用する:

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

リゾルバーパイプライン:

1. 各標準ルート（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）をその upstream モデルへ解決
2. `model_context_windows.json` で各 upstream モデルのコンテキスト容量を参照
3. 3 つすべての容量が既知であることを要求
4. 既知の最小容量を安全なコンテキストウィンドウとして使用
5. 設定されたトリガー割合を適用

モード: `auto`（既知の最小容量）、`manual`（`window_tokens`）、`claude_default`（Claude Code 独自のデフォルト。変数を設定しない）。実効ステータスは `applied`、`disabled`、`incomplete` のいずれか。

起動コマンドは、ゲートウェイ接続変数とコンテキスト変数を組み合わせる:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

コンテキスト制御が適用されない場合、コマンドはまず古い変数を削除する:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

割合オーバーライドは圧縮を早める方向にのみ作用する。Claude Code のデフォルトよりも圧縮を遅らせる値は無視される場合がある。Anthro Bridge はコマンドの生成とコピーのみを行い、実行は一切しない。また、特定の Claude Code バージョンがこれらの変数を尊重することを保証するものではない（最終確認には Claude Code の診断情報または実際に観測された圧縮動作が必要）。

### 多言語対応

1 言語 1 ファイルの構成。`import.meta.glob` で自動検出:

```
gui/src/i18n/lang/
  en.ts      English (canonical — defines TranslationKey type)
  ja.ts      Japanese
  zh-CN.ts   Chinese Simplified
  zh-TW.ts   Chinese Traditional
  ko.ts      Korean
  fr.ts      French
  de.ts      German
  es.ts      Spanish
```

言語を追加するには: `en.ts` をコピーし、翻訳して、再ビルドする。コード変更は不要。

### config.json リファレンス

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "Display name",
      "upstream_url": "Anthropic-compatible API base URL",
      "api_key_env": "API key env var name",
      "default_model": "Fallback model name",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "real-model-name" },
      "visible_models": ["claude-public-model-name"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "real-model-name",
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

各プロバイダーまたは OpenRouter モデルセットは、`claude_code: { "auto_compact": { "mode": "auto" } }` でデフォルトのコンテキストモードを設定することもできる。ルートの実効モードはプロバイダー/モデルセットの値で、グローバルブロックへフォールバックする。`resolve_claude_code_auto_compact` が解決結果を返す。
