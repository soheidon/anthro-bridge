[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code から利用するためのプロキシ + GUI 管理ツール「Anthro Bridge」です。

Anthropic Messages API リクエストの `model` フィールドを読み取り、対応する upstream へ自動振り分け（モデルベースルーティング）。変更するのは `model` フィールドのみで、messages / thinking / tool_use / tool_result / streaming SSE は一切改変しません。

Anthro Bridge は Moon Bridge のフォーク、GUI版、補助アプリではなく、独立した Anthropic互換ゲートウェイです。

### 対応プロバイダー

| プロバイダーID | 表示名 | Upstream エンドポイント | デフォルトモデル |
|---------------|--------|------------------------|-----------------|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` | `deepseek-v4-pro` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` | `MiniMax-M3` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` | `kimi-k2.7-code` |
| `mimo` | **MiMo / Xiaomi** | `https://api.xiaomimimo.com/anthropic` | `mimo-v2.5-pro` |

GUI 管理ツール（Tauri v2 + React 19 + TypeScript）でプロキシの起動・停止、設定編集、ログ確認、API キー管理が可能です。

### なぜこのゲートウェイが必要か

Claude Desktop / Claude Code は、基本的にAnthropicのAPI形式とClaude系のモデル名を前提に動作します。そのため、DeepSeek、MiniMax、Kimi、MiMo などがAnthropic互換APIを提供していても、Claude Desktop / Claude Code からそれらを直接指定して常に利用できるとは限りません。

特に **Claude Desktop の `inferenceModels[].name` には Anthropic 公式モデル名しか指定できません**。`claude-deepseek-v4` や `kimi-k2.6` のようなゲートウェイ独自名は `"not an Anthropic model"` として弾かれます。

Anthro Bridge はこの制約を回避するため、**Claude Desktop には常に Anthropic 公式モデル名（`claude-opus-4-8` / `claude-sonnet-5` / `claude-haiku-4-5`）を「器」として見せ、実際に使う LLM（DeepSeek / MiniMax / Kimi / MiMo）は GUI で切り替える**設計を採用しています。

```
Claude Desktop 側（常に固定）
  Opus 4.8   = claude-opus-4-8
  Sonnet 5   = claude-sonnet-5
  Haiku 4.5  = claude-haiku-4-5

ゲートウェイ内部（GUI の選択による）
  DeepSeek:      Opus -> deepseek-v4-pro (thinking+high),  Sonnet -> deepseek-v4-pro,     Haiku -> deepseek-v4-flash
  MiniMax:       Opus -> MiniMax-M3,                       Sonnet -> MiniMax-M2.7,         Haiku -> MiniMax-M2.7-highspeed
  Kimi:          Opus -> kimi-k2.7-code,                   Sonnet -> kimi-k2.6,           Haiku -> kimi-k2.5
  MiMo / Xiaomi: Opus -> mimo-v2.5-pro (thinking),        Sonnet -> mimo-v2.5-pro,       Haiku -> mimo-v2.5
```

これにより、Claude Desktop のモデル名検名検証を通過しつつ、DeepSeek / MiniMax / Kimi / MiMo を自由に切り替えられます。

### 必要環境

- **Windows 10/11**（日本語環境対応）
- 利用するプロバイダーの API キー（DeepSeek / MiniMax / Kimi / MiMo **いずれか1つでOK**、v0.5.0以降）

### クイックスタート

#### 1. インストール

[Releases](https://github.com/soheidon/anthro-bridge/releases) から最新のインストーラーをダウンロードして実行。

インストーラー起動時に言語選択画面が表示されます（English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español から選択可）。

#### アップデート

新しい `setup.exe` を実行するだけで、旧バージョンを自動検出して上書き更新されます。手動アンインストールは不要です。設定ファイル（`%APPDATA%\Anthro Bridge\config.json`）は更新後も保持されます。

#### 2. API キー設定

設定（⚙）-> **API キー** タブで、使用するプロバイダーの API キーを入力し「保存」をクリック。
Windows ユーザー環境変数に永続保存されます。

| プロバイダー | 環境変数 | 備考 |
|-------------|---------|------|
| DeepSeek | `DEEPSEEK_API_KEY` | |
| MiniMax | `MINIMAX_API_KEY` | |
| Kimi / Moonshot | `MOONSHOT_API_KEY` | |
| MiMo / Xiaomi | `XIAOMI_API_KEY` | |

#### 3. プロバイダ選択

ダッシュボードの **LLMプロバイダ選択** カードは2×2グリッドで表示されます：

```
[ DeepSeek       ] [ MiMo / Xiaomi  ]
[ MiniMax        ] [ Kimi / Moonshot]
```

使用するプロバイダをクリックして選択してください。

#### 4. プロキシ起動

ヘッダーの **Start Gateway** ボタンをクリック。プロキシが `http://127.0.0.1:4000` で起動します。

#### 5. Claude Desktop / Cowork on 3P の設定

詳しい手順は [THIRD_PARTY_INFERENCE.ja.md](THIRD_PARTY_INFERENCE.ja.md) を参照してください。

### エンドポイント

| Method | Path | 説明 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | 公開モデル一覧 |
| POST | `/v1/messages` | Messages API（stream/non-stream）。モデルベースルーティング |
| POST | `/v1/messages/count_tokens` | トークン数カウント（対応プロバイダーのみ） |

### ルーティング

モデルベースルーティング: リクエストの `model` フィールドを読み取り、対応するプロバイダーと upstream モデルに自動振り分け。

| Anthropic モデル | DeepSeek | MiniMax | Kimi | MiMo / Xiaomi |
|------------------|----------|---------|------|---------------|
| `claude-opus-4-8` | `deepseek-v4-pro` (+ thinking + high effort) | `MiniMax-M3` (+ thinking) | `kimi-k2.7-code` (+ thinking) | `mimo-v2.5-pro` (+ thinking) |
| `claude-sonnet-5` | `deepseek-v4-pro` (+ medium effort) | `MiniMax-M3` (+ thinking) | `kimi-k2.6` (+ thinking) | `mimo-v2.5-pro` |
| `claude-haiku-4-5` | `deepseek-v4-flash` (+ thinking) | `MiniMax-M3` (+ thinking) | `kimi-k2.6` | `mimo-v2.5` |

**Kimi K3**: `kimi-k3` は手動選択肢として利用可能（デフォルトではない）。K2.x の `thinking` パラメータではなく、常時推論の `reasoning_effort: "max"` を使用。動画入力はプロキシが未対応の ms:// ファイル ID 経由のみ。

**MiniMax M3**: 3 tier 全てが Thinking 専用モードの `MiniMax-M3` をデフォルトとして使用。`MiniMax-M2.7` と `MiniMax-M2.7-highspeed` は手動選択肢として維持。

#### MiMo ルーティング詳細

- **`claude-opus-4-8` → `mimo-v2.5-pro`**: Thinking が**デフォルトで有効**（`thinking_mode: "thinking"`）。MiMo の thinking 制御には `thinking_mode` キーを使います（`thinking` ではありません）。標準モードにするには `"default"` を指定。
- **`claude-haiku-4-5` → `mimo-v2.5`**: 画像URL・base64画像の pass-through に対応。音声・動画入力は Anthro Bridge では未対応。
- **`claude-opus-4-8` ルートは画像非対応。** 画像が送信された場合はテキストプレースホルダに置換されます（`non_vision_image_policy: "replace"`）。
- **Upstream エンドポイント**: `https://api.xiaomimimo.com/anthropic/v1/messages` にリクエストを送信します。

### 言語

8言語対応: English, 日本語, 中文(简体), 中文(繁體), 한국어, Français, Deutsch, Español。

新しい翻訳を追加するには `gui/src/i18n/lang/` に言語ファイル（例: `es.ts`）を追加して再ビルドするだけです。
詳しくは [CONTRIBUTING](CONTRIBUTING.md) を参照。

### 設定 UI (v0.11.0)

- **プロバイダー行の折りたたみ**: クリック、Enter、Spaceで展開・折りたたみ
- **3-tier モデルマッピング**: 各プロバイダーごとに Opus / Sonnet / Haiku のターゲットモデルを個別設定
- **Thinking Mode セレクタ**: モデルが対応している場合、thinking / normal を切替
- **Reasoning Effort セレクタ**: DeepSeek Pro で high / medium / low を設定。K3 は固定「Max」表示
- **カスタム upstream モデル**: 「Custom」オプションで任意のモデル名を入力
- **自動保存**: model、thinking mode、reasoning effort は変更時に即保存
- **環境変数名**: blur または Enter で保存
- **API キー**: 明示的な「保存」ボタンで保存（blur では自動保存しない）
- **保存状態表示**: 「保存中…」「保存済」「保存失敗」をインライン表示
- **起動時ウィンドウサイズ**: 1100×720 に安定化（余計なスクロールバーなし）
- **モデル料金表示**: 折りたたみ式の料金テーブルで各モデルの入力/出力/キャッシュコストを表示（USD/1Mトークン）
- **ダッシュボード料金列**: 利用可能モデルテーブルに「入力/1M」「出力/1M」列を追加
- **ライブダッシュボード同期**: 設定画面でのモデル変更がダッシュボードに即時反映（再起動不要）

### 設定 (config.json)

プロバイダー設定は各モデルの上流モデル名や機能フラグを定義します。通常は編集不要です。
上級者向けの詳細設定は GUI の設定（⚙）-> **Gateway Config** から行えます。

| キー | 説明 |
|-----|------|
| `models.<model>.upstream_model` | upstream へ送る実モデル名（必須） |
| `models.<model>.thinking_mode` | `"thinking"`（有効）または `"normal"`（標準）。Thinking 注入を制御 |
| `models.<model>.reasoning_effort` | `"high"` / `"medium"` / `"low"` / `"max"`。DeepSeek Pro モデルのみ。Thinking 有効時に送信 |
| `models.<model>.supports_vision` | モデル単位の画像サポート（省略時はプロバイダー既定値） |
| `models.<model>.supports_video` | モデル単位の動画サポート（省略時はプロバイダー既定値） |
| `models.<model>.visible` | `/v1/models` とダッシュボードに表示するか（デフォルト `true`） |
| `non_vision_image_policy` | 非Visionモデルの画像処理: `replace`（プレースホルダ）/ `drop`（削除）/ `reject`（エラー） |

### プロジェクト構成

```
anthro-bridge/
├── README.md                  英語
├── SPEC.md                    仕様書
├── docs/
│   ├── README.ja.md           日本語
│   ├── README.zh-CN.md        中国語(簡体)
│   ├── SPEC.ja.md             日本語
│   ├── SPEC.zh-CN.md          中国語(簡体)
│   ├── THIRD_PARTY_INFERENCE.md   サードパーティ推論ガイド
│   ├── THIRD_PARTY_INFERENCE.ja.md
│   └── THIRD_PARTY_INFERENCE.zh-CN.md
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

DeepSeek は画像・動画に対応していません。画像が送信された場合は自動的にプレースホルダテキストに置換されます（`non_vision_image_policy: "replace"`）。画像をそのまま使いたい場合は MiniMax、Kimi、または MiMo（`claude-haiku-4-5` ルート）を選択してください。

MiMo の `claude-opus-4-8` ルートも画像非対応です。画像を使う場合は `claude-sonnet-5` または `claude-haiku-4-5` ルートを使用してください。動画は常に拒否されます。

#### MiMo: 既存ユーザー設定が反映されない場合

v0.9.0 より前からアップグレードした場合、保存済みのユーザー設定に古い `"display_name": "MiMo"` または `"thinking": "default"` が残っている可能性があります。v0.9.0 は初回起動時に自動移行を行いますが、問題がある場合は以下を試してください：

1. **アプリを再起動** — 自動移行は起動時に実行されます。
2. **設定をリセット**: `%APPDATA%\Anthro Bridge\config.json` を削除して再起動すると、正しい設定が適用された同梱configが使用されます。
3. **手動確認**: `%APPDATA%\Anthro Bridge\config.json` を開き、`providers.mimo` に `"display_name": "MiMo / Xiaomi"`、`"api_key_env": "XIAOMI_API_KEY"`、およびモデルエントリに `thinking_mode`（`thinking` ではない）があることを確認してください。

### 動作確認 — MiMo / Xiaomi

#### テキストテスト（claude-opus-4-8 → mimo-v2.5-pro）

1. 設定 → API キー タブで `XIAOMI_API_KEY` を設定し保存。
2. ダッシュボードで **MiMo / Xiaomi** を選択。
3. プロキシを起動。
4. Claude Desktop からメッセージを送信し、thinking blocks 付きの応答が返ることを確認。

#### 画像テスト（claude-haiku-4-5 → mimo-v2.5）

1. ダッシュボードで **MiMo / Xiaomi** を選択。
2. Claude Desktop で画像を添付してメッセージを送信。
3. 画像が正しく認識・説明されることを確認。
4. `claude-opus-4-8` に画像を送信した場合、テキストプレースホルダに置換されることを確認。

#### 検証

GUI の Log パネルで、リクエストの `model` フィールドがルートに応じて `mimo-v2.5-pro` または `mimo-v2.5` に書き換えられていることを確認してください。

### ライセンス

MIT — 詳細は [LICENSE](../LICENSE) を参照。
