[English](README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md)

# Terra Bridge

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
├── README.md                  英語
├── README.ja.md               日本語
├── README.zh-CN.md            中国語(簡体)
├── SPEC.md                    仕様書
├── SPEC.ja.md                 日本語
├── SPEC.zh-CN.md              中国語(簡体)
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
