[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**現在のリリース: 0.16.0**

Anthro Bridgeは、Claude DesktopとClaude CodeがAnthropic互換APIを通じて複数のサードパーティLLMプロバイダーを利用できるようにする、ローカルゲートウェイ兼デスクトップ設定ツールです。

このアプリケーションは以下で構成されています:

- Rustで書かれたローカルプロキシサーバー
- Tauri 2、React、TypeScriptで構築されたネイティブWindows GUI
- Anthropicモデル名からプロバイダー固有の上流モデルへのモデルベースルーティング
- ルートごとのモデル、推論、機能の設定

Anthro Bridgeは独立したプロジェクトです。Moon Bridgeのフォーク、フロントエンド、またはコンパニオンアプリケーションではありません。

## バージョン0.16.0のハイライト

バージョン0.16.0では、モデル認識型のClaude Codeコンテキスト管理が追加されました。

- Anthro Bridgeは、Opus、Sonnet、Haikuルートに割り当てられた上流モデルのコンテキスト容量を解決します。
- 自動モードでは、3つのルートのうち既知の最小容量が、安全なClaude Codeコンテキストウィンドウとして使用されます。
- コンテキスト制御は、3つすべてのルート容量が既知の場合にのみ適用されます。
- ヘッダーにはコンパクトなコンテキスト管理トグルが用意されています。詳細モードとしきい値は`config.json`から引き続き設定できます。
- アプリケーションは、Anthro Bridge接続変数とClaude Codeコンテキスト制御変数を含む完全なPowerShell起動コマンドを生成できます。
- コンテキスト管理が無効または不完全な場合、生成されたコマンドは現在のPowerShellセッションから古いコンテキスト制御変数を削除します。
- 組み込みのコンテキストメタデータは、標準のプロバイダー直接モデルと組み込みのOpenRouterモデルをカバーしています。
- 生成されたコマンドとその環境変数の動作は、Rustのユニットテスト、Windows PowerShell統合テスト、フロントエンドのコピーフローテストでカバーされています。

## 対応モデル

Anthro Bridgeは2つのカテゴリの上流モデルをサポートしています。

### ネイティブ統合

これらのプロバイダーは、独自のAnthropic互換APIを通じてサポートされています。OpenRouterアカウントは不要です。

| プロバイダー | 対応モデルファミリー | 接続方法 |
|---|---|---|
| DeepSeek | DeepSeek V4 ProおよびV4 Flash | プロバイダー直接API |
| MiniMax | MiniMax M3およびM2.7バリアント | プロバイダー直接API |
| Kimi / Moonshot | Kimi K2.xおよびKimi K3 | プロバイダー直接API |
| MiMo / Xiaomi | MiMo V2.5およびV2.5 Proバリアント | プロバイダー直接API |

### OpenRouter経由で対応するモデル

これらのモデルはOpenRouterモデルセットを通じてアクセスします。各モデルセットには独自のAPIキー、ルートマッピング、推論設定があります。

| ベンダーまたはモデルファミリー | 組み込みサポート | 推論制御 |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | あり | モデル固有のThinking制御 |
| Tencent Hy3 | あり | LowおよびHigh推論強度 |
| InclusionAI Ring | あり | モデル固有のThinkingおよび推論制御 |
| StepFun Step 3.5 / Step 3.7 | あり | Low、Medium、High（対応時） |
| InclusionAI Lingファミリー | あり | モデル固有のThinking制御 |
| OpenAI GPT-5.6 Sol / Terra / Luna | あり | モデル固有のThinkingおよび推論制御 |

その他のOpenRouterモデルも、OpenRouterのライブモデルリストから選択するか、手動で入力できます。組み込みサポートとは、Anthro Bridgeがモデルファミリー、機能フラグ、ベンダーグループ、推論制御の動作を既に認識していることを意味します。

## 仕組み

Claude DesktopとClaude Codeは、以下のようなAnthropicモデル名を使用してリクエストを送信します:

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridgeはこれらの名前を安定したルート識別子として扱います。各ルートがどのプロバイダーと上流モデルを使用するかはGUIが決定します。

例:

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

上流プロバイダー向けに適応が必要なフィールドのみが変更されます。メッセージ、ツール呼び出し、ツール結果、思考ブロック、ストリーミングデータは、上流APIが対応している限り、そのまま保持されます。

## 主な機能

### プロバイダールーティング

Anthro Bridgeは2種類の上流接続タイプをサポートしています:

1. **プロバイダー直接統合**: プロバイダー独自のAnthropic互換APIに接続します。
2. **OpenRouterモデルセット**: OpenRouterに接続し、単一のAPIを通じて複数のベンダーやモデルファミリーにルーティングできます。

#### プロバイダー直接統合

| プロバイダーID | 表示名 | デフォルトエンドポイント |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter統合

| 接続タイプ | 表示名 | エンドポイント |
|---|---|---|
| 複数モデルセット対応のモデルゲートウェイ | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouterは単一のモデルプロバイダーとして扱われません。各OpenRouterモデルセットは、Poolside、Tencent、InclusionAI、StepFunなどの対応ベンダーグループから独立してモデルを選択できるほか、OpenRouter APIから検出されたモデルや手動入力したモデルも使用できます。

各Anthropicルートは、プロバイダー直接モデルまたはOpenRouterモデルセットを通じて選択したモデルのいずれかに独立してマッピングできます。

### OpenRouter複数モデルセットサポート

複数のOpenRouterモデルセットを作成し、独立して管理できます。

各モデルセットには以下が含まれます:

- モデルセット名
- APIキー設定
- Opus、Sonnet、Haikuルートマッピング
- Thinkingまたは推論設定
- キャッシュされたOpenRouterモデルリスト

モデルセットはGUIから追加、名前変更、削除、ドラッグ＆ドロップによる並べ替え、非表示化、選択ができます。ダッシュボードには表示中のモデルセットごとに1枚のカードが表示され、リフレッシュ後も保存された順序が保持されます。

組み込みのOpenRouterベンダーグループには現在、Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6、その他の認識済みモデルファミリーが含まれます。認識されないモデルも、検索またはカスタムモデル入力から利用可能です。ダッシュボードは、`poolside/laguna-s-2.1`のようなベンダー修飾IDを可読性のために`laguna-s-2.1`に短縮表示しますが、ルーティングには完全なIDが保持されます。

### OpenRouterの価格とモデル詳細

Settingsのモデル料金パネルには、対応するOpenRouterモデルの組み込み価格が表示されます。これには入力、出力、キャッシュ入力の価格が含まれます。プロモーション価格は、改定後の標準価格と併せて表示できます。対象はGPT-5.6 Sol、Terra、LunaバリアントとそのProバリアントです。価格の備考には、該当する場合は長文コンテキスト価格を含められます。

### レスポンシブなダッシュボードサイズ

初期ウィンドウの高さは、3列のダッシュボードに表示されるプロバイダーカードとOpenRouterカードの枚数から計算されます。カードの行が増えるとウィンドウの高さも増えますが、ネイティブの最小サイズ、モニターの作業領域、DPIスケーリング、タイトルバーの装飾が考慮されます。モデルセットの表示状態や数が変わると、新しい行数に合わせて高さが再計算されます。行数が変わらない限り、手動でのサイズ変更は保持されます。

### ローカライズされたWindowsインストーラー

Windows NSISインストーラーは、英語、日本語、簡体字中国語、繁体字中国語、韓国語、フランス語、ドイツ語、スペイン語の言語選択に対応しています。インストーラーはAnthro Bridgeのアプリケーションアイコンを使用し、アップグレード時に安定版のユーザー設定を保持します。

### 最新のUI信頼性の改善

設定の書き込みは逐次化され、OpenRouterの保存は古いリクエストからの保護を備えたキューベースの更新パスを使用し、モデルセットの並べ替え操作はリフレッシュ失敗後もクリーンに復帰します。回帰テストは、モデルセットの順序、保存の競合、モデル価格、ダッシュボードのカード枚数、ウィンドウサイズをカバーしています。

### モデルと推論の制御

利用可能な制御は選択したモデルによって異なります。

対応する制御には以下が含まれます:

- Thinkingのオン/オフ
- Normal、low、medium、high、xhigh、maxの推論モード
- プロバイダー固有の推論強度
- ユーザー選択を許可しないモデルの固定推論モード

モデルを切り替える際、Anthro Bridgeは最も近い互換性のある推論設定を保持しようとします。以前の設定が正確に利用できない場合は、最も近い対応オプションを選択し、2つの選択肢が等距離の場合は弱い方を優先します。

### 機能検出

Anthro Bridgeは、組み込みの機能レジストリとOpenRouterのライブメタデータを組み合わせています。

機能には以下が含まれます:

- 画像入力
- 動画入力
- Thinkingサポート
- 推論強度サポート
- 既知の価格情報
- プロバイダー固有のリクエスト変換ルール

OpenRouterのライブメタデータは、不要なAPI呼び出しを減らすためにキャッシュされます。

### レスポンスモデル名の正規化

上流APIはしばしばレスポンスに独自のモデル名を返します。Anthro Bridgeはそのフィールドを、クライアントが期待するAnthropicルート名に書き換えることができます。

例:

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

正規化はストリーミングと非ストリーミングの両方のレスポンスに適用され、Settingsで有効/無効を切り替えられます。

### 逐次設定書き込み

設定の変更は逐次化され、同時書き込みによる設定の破損や巻き戻りを防ぎます。

以下の操作が対象です:

- モデル変更
- Thinkingモード変更
- 推論強度変更
- OpenRouterモデルセット変更
- APIキー関連の設定変更

### OpenRouter保存キュー

OpenRouterのルート変更は専用の保存キューを通じて処理されます。

キューは以下を提供します:

- 逐次化された保存操作
- 古いリクエストの置き換え
- リクエスト送信時のルートIDのキャプチャ
- 古いReactクロージャからの保護
- 以前に選択されたルートからのロールバック保護
- 保存成功後のリフレッシュ再試行
- 集約されたゲートウェイ再起動処理
- 保存後処理中に追加されたリクエストの安全な処理

これにより、素早いモデル変更、ルート切り替え、または遅延したTauriレスポンスが古いUI値を復元するのを防ぎます。

### Claude Codeコンテキスト管理

Anthro Bridge 0.16.0は、モデル認識型のコンテキスト設定を含むClaude Code起動コマンドを生成できます。

リゾルバーは以下の手順を実行します:

1. 各標準ルートに割り当てられた上流モデルを解決する:
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. 各上流モデルの既知のコンテキスト容量を検索する。
3. 3つすべてのルート容量が既知であることを要求する。
4. 最小容量を安全なコンテキストウィンドウとして使用する。
5. 設定されたトリガー割合を適用する。

たとえば、3つのルートの容量が1,000,000、262,144、1,000,000トークンに解決される場合、Anthro Bridgeは以下を使用します:

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

生成されたPowerShellコマンドは、公式のClaude Code変数を使用します:

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

また、Anthro Bridgeゲートウェイ接続変数も含まれます:

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

例:

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

コンテキスト管理が無効な場合、Claude Codeのデフォルト動作に設定されている場合、またはルート容量が不明なために不完全な場合、生成されたコマンドはClaude Codeを起動する前に古いコンテキスト変数をクリアします:

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

割合オーバーライドは、より早いプロアクティブ圧縮を要求します。Claude Codeは、独自のデフォルト動作を超えて圧縮を遅らせる値は無視する場合があります。

Anthro Bridgeは、コマンド生成とPowerShell環境注入を検証します。これ自体は、特定のClaude Codeリリースが変数を消費したことを証明するものではありません。最終的な確認には、Claude Codeの診断または圧縮動作の観察が必要です。

### ゲートウェイ管理

GUIは以下を提供します:

- ゲートウェイの起動・停止制御
- プロバイダーとモデルセットの選択
- ルート設定
- APIキー管理
- ログ表示
- モデルリストのリフレッシュ
- 保存状態とエラー表示

ゲートウェイは以下のアドレスで待ち受けます:

```text
http://127.0.0.1:4000
```

## 必要条件

- Windows 10またはWindows 11
- 開発にはNode.js 24以降
- 開発には安定版Rustツールチェーン
- 少なくとも1つの対応プロバイダーのAPIキー

1つのプロバイダーキーで十分です。すべてのプロバイダーのキーは必要ありません。

## インストール

プロジェクトのReleasesページから最新のWindowsインストーラーをダウンロードして実行してください。

インストーラーは以下の言語に対応しています:

- 英語
- 日本語
- 簡体字中国語
- 繁体字中国語
- 韓国語
- フランス語
- ドイツ語
- スペイン語

Anthro Bridgeを更新するには、新しいインストーラーを実行してください。既存のユーザー設定は保持されます。

安定版のユーザー設定は以下に保存されます:

```text
%APPDATA%\Anthro Bridge\
```

開発ビルドは別のアプリケーションIDとデータディレクトリを使用します:

```text
%APPDATA%\Anthro Bridge Dev\
```

これにより、安定版と開発版が設定やキャッシュファイルを共有せずに共存できます。

## クイックスタート

### 1. APIキーを設定する

以下を開きます:

```text
Settings > API Key
```

使用する予定のプロバイダーのキーを入力して保存します。

一般的な環境変数名は以下の通りです:

| プロバイダー | 環境変数 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouterモデルセットは、GUIから管理されるモデルセット固有のキー設定を使用できます。

### 2. ルートモデルを設定する

Settingsを開き、各ルートの上流モデルを選択します:

- Opus
- Sonnet
- Haiku

OpenRouterの場合は、まずモデルセットを選択または作成し、そのモデルセット内で各ルートを設定します。

### 3. ゲートウェイを起動する

**ゲートウェイ起動**をクリックします。

ローカルエンドポイントが利用可能であることを確認します:

```text
GET http://127.0.0.1:4000/health
```

### 4. Anthro Bridge経由でClaude Codeを起動する

Claude設定パネルを開き、**Claude Code起動コマンドをコピー**をクリックします。

生成されたコマンドをPowerShellに貼り付けます。このコマンドには以下が含まれます:

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- `CLAUDE_CODE_AUTO_COMPACT_WINDOW`（コンテキスト管理が適用される場合）
- `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`（コンテキスト管理が適用される場合）
- コンテキスト管理が適用されない場合の古いコンテキスト変数のクリーンアップコマンド

このコマンドは、Anthro BridgeをゲートウェイとしてClaude Codeを起動し、設定されたモデル認識型のコンテキスト動作を保持します。

Claude Desktopおよび追加のサードパーティ推論の手順については、以下を参照してください:

```text
docs/THIRD_PARTY_INFERENCE.md
```

## APIエンドポイント

| メソッド | パス | 説明 |
|---|---|---|
| `GET` | `/health` | ゲートウェイヘルスチェック |
| `GET` | `/v1/models` | 公開ルートモデルリスト |
| `POST` | `/v1/messages` | ストリーミングおよび非ストリーミングMessages API |
| `POST` | `/v1/messages/count_tokens` | 選択したプロバイダーが対応している場合のトークンカウント |

## 設定

メインの設定ファイルは`config.json`です。

ほとんどの設定はGUIから変更してください。手動編集は高度な用途向けです。

重要なモデルフィールドは以下の通りです:

| キー | 説明 |
|---|---|
| `models.<route>.upstream_model` | プロバイダーに送信される上流モデル名 |
| `models.<route>.thinking_mode` | ルート固有のThinkingモード |
| `models.<route>.reasoning_effort` | プロバイダー固有の推論強度 |
| `models.<route>.supports_vision` | 画像サポートの上書き |
| `models.<route>.supports_video` | 動画サポートの上書き |
| `models.<route>.visible` | ルートをクライアントとダッシュボードに公開するかどうか |
| `non_vision_image_policy` | 対応していない画像入力の処理方法 |
| `normalize_response_model_identity` | レスポンスモデル名を正規化するかどうか |
| `claude_code.auto_compact.enabled` | グローバルなコンテキスト管理トグル |
| `claude_code.auto_compact.trigger_percent` | 要求されるプロアクティブ圧縮の割合 |
| `claude_code.auto_compact.mode` | `auto`、`manual`、または`claude_default` |
| `claude_code.auto_compact.window_tokens` | `manual`モードで使用される手動コンテキストウィンドウ |

対応していない画像は、以下のいずれかのポリシーで処理できます:

- `replace`: 画像をテキストプレースホルダーに置き換える
- `drop`: 画像コンテンツを削除する
- `reject`: エラーを返す

### コンテキスト管理の設定

GUIで公開されるのはグローバルなコンテキスト管理トグルのみです。詳細な値は`config.json`で直接編集できます。

自動モード:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "auto",
      "trigger_percent": 90
    }
  }
}
```

手動モード:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "manual",
      "window_tokens": 240000,
      "trigger_percent": 90
    }
  }
}
```

Claude Codeのデフォルト動作:

```json
{
  "claude_code": {
    "auto_compact": {
      "enabled": true,
      "mode": "claude_default"
    }
  }
}
```

`auto`モードでは、Anthro Bridgeは3つすべての標準ルートが既知のコンテキストメタデータを持つ場合にのみコンテキスト変数を適用します。不明なカスタムOpenRouterモデルは有効なルーティング先のままですが、コンテキスト管理は、メタデータが利用可能になるかmanualモードが設定されるまで不完全な状態として報告されます。

静的モデル容量は以下に保存されます:

```text
gui/src-tauri/resources/model_context_windows.json
```

レジストリには、組み込みプリセットで使用される標準のDeepSeek、MiniMax、Kimi、MiMo、Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6モデルが含まれます。

## プロバイダー注意事項

### DeepSeek

`reasoning_effort`（推論強度）:

- `deepseek-v4-pro`
  - Normal: 推論強度無効
  - Thinking: High / Max
- `deepseek-v4-flash`
  - Normal: 推論強度無効
  - Thinking: Low / High / Max

起動時、DeepSeek V4 Proルートに保存されたレガシーの`low`または`medium`の推論強度は、`high`に移行されます（DeepSeekの実効推論レベルに一致）。

新規インストール時と新たに生成された設定ファイルの既定のDeepSeekルーティング:

- Opus 5 → V4 Flash、Thinking、Max
- Sonnet 5 → V4 Flash、Thinking、High
- Haiku 4.5 → V4 Flash、Thinking、Low

既存の保存済みルーティングは自動変更されません。

### MiniMax

MiniMaxモデルの動作はモデル世代によって異なります。Anthro Bridgeは選択されたモデルに必要なリクエスト形式を適用し、対応している場合は適応型または無効化されたThinkingを含みます。

### Kimi

Kimiモデルは、モデルファミリーに応じてThinkingパラメータまたは固定推論強度モードのいずれかを使用する場合があります。Anthro BridgeはGUIの選択を適切な上流リクエスト形式に変換します。

### MiMo

MiMoは対応ルートにおいて、一般的な`thinking`フィールドではなく`thinking_mode`を使用します。

Visionのサポートはモデルによって異なります。Anthro Bridgeは、ルートが画像入力を受け付けられない場合に設定された非対応画像ポリシーを適用します。

### OpenRouter

OpenRouterモデルは認識された場合ベンダーごとにグループ化されます。GUIは以下を提供します:

- モデル検索
- ベンダーグループ化
- カスタムモデル入力
- 機能バッジ
- 価格表示
- モデルごとの推論制御
- 統合モデルリストリフレッシュ

OpenRouterモデルの機能と動作は時間の経過とともに変更される可能性があります。利用可能な場合はライブメタデータが使用され、組み込みレジストリは既知のモデルに対して安定したデフォルトを提供します。

組み込みのOpenAI GPT-5.6 Balancedモデルセットは、新規インストール時と新たに生成された設定ファイルでは、全ルートがThinking Highに既定設定されます:

- Opus 5 → GPT-5.6 Sol、Thinking、High
- Sonnet 5 → GPT-5.6 Terra、Thinking、High
- Haiku 4.5 → GPT-5.6 Luna、Thinking、High

既存の保存済みルーティングは自動変更されません。

## ユーザーインターフェース

Settingsインターフェースには以下が含まれます:

- 折りたたみ可能なプロバイダーセクション
- Opus、Sonnet、Haikuルート設定
- OpenRouter向けモデル検索とベンダーグループ化
- モデル機能に基づくThinkingおよび推論制御
- カスタム上流モデル入力
- 自動ルート保存
- APIキーの明示的保存
- 保存の進行状況とエラーメッセージ
- モデルの価格と機能情報
- レスポンスモデル名正規化トグル
- ヘッダーのClaude Codeコンテキスト管理トグル
- Claude設定パネルのClaude Code起動コマンドコピー操作

Dashboardには以下が含まれます:

- プロバイダーまたはOpenRouterモデルセットの選択
- ゲートウェイステータス
- 現在のルートマッピング
- 機能インジケーター
- 価格情報
- プロバイダー切り替えステータス

## 開発

### プロジェクト構造

```text
anthro-bridge/
├── README.md
├── SPEC.md
├── config.json
├── docs/
│   ├── README.*.md
│   ├── SPEC.*.md
│   └── THIRD_PARTY_INFERENCE*.md
├── gui/
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   └── i18n/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   ├── proxy.rs
│   │   │   ├── openrouter.rs
│   │   │   ├── config_template.rs
│   │   │   ├── model_capabilities.rs
│   │   │   ├── model_routing.rs
│   │   │   └── paths.rs
│   │   └── resources/
│   │       ├── config.json
│   │       └── model_context_windows.json
│   └── package.json
└── LICENSE
```

### 開発モードで実行

```bash
cd gui
npm install
npm run tauri dev
```

### 開発バリアントのビルド

Windowsでは、断続的なコンパイラ終了を避けるために単一のRustビルドジョブを使用してください:

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

開発ビルドは以下を使用します:

- ウィンドウタイトル: `Anthro Bridge (DEV)`
- ポート: `4000`
- アプリケーションID: `com.soheidon.anthro-bridge.dev`
- 別個の設定およびキャッシュディレクトリ

### 安定版ビルド

安定版ビルドはリリース準備の場合にのみ作成してください。通常の実装および検証作業には開発バリアントを使用してください。

## 検証

フロントエンドの検証:

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rustの検証:

```bash
cd gui/src-tauri
cargo check
cargo test
```

コンテキスト管理の検証は以下をカバーしています:

- プロキシとコンテキストリゾルバーで共有されるルートから上流モデルへの解決
- 組み込みのプロバイダー直接モデルとOpenRouterモデルの完全なモデルコンテキストメタデータ
- 3つの標準ルートにわたる自動の最小ウィンドウ選択
- 適用、無効、不完全、manual、Claude-defaultの各モード
- 公式のClaude Code環境変数名
- PowerShellコマンドのレンダリングとエスケープ
- ゲートウェイ接続変数
- 実際のWindows PowerShell子プロセスでの環境注入
- コンテキスト管理が適用されない場合の古いコンテキスト変数の削除
- 生成された起動コマンドのフロントエンドでのコピー

OpenRouterルートセレクター固有の検証:

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

OpenRouterセレクターのテストは以下をカバーしています:

- キュー保存中のルートIDキャプチャ
- クロスルートロールバック保護
- 古いコールバック保護
- リフレッシュ再試行動作
- リフレッシュ失敗後のゲートウェイ再起動
- 処理中のリクエストの置き換え
- 世代ベースのロールバック抑制

再起動集約のための専用マルチ保存テストを追加して、以下の動作を確定することができます:

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## 手動検証チェックリスト

自動テストはすべてのTauriとReactのタイミング条件を再現するわけではありません。リリース前に、開発ビルドで以下を確認してください:

- 各OpenRouterモデルセットが正しいホバー詳細を表示すること
- モデル選択が変更後に視覚的に巻き戻らないこと
- Thinkingと推論の選択が保存後も安定していること
- 設定画面を閉じて再度開いた後も設定が正しいこと
- アプリケーション再起動後も設定が正しいこと
- 保存中にモデルセットを切り替えてもどちらのモデルセットも破損しないこと
- 保存失敗時はその保存を開始したルートのみがロールバックされること
- リフレッシュ再試行の成功が以前のエラーをクリアすること
- リフレッシュ再試行の失敗が最新のエラーを表示したままにすること
- 必要なゲートウェイ再起動がバッチ後に1回だけ発生すること
- カスタムモデルが正しく保存・再読み込みされること
- 組み込みおよびライブのOpenRouter機能が正しく表示されること
- ヘッダーのコンテキスト管理トグルがビジュアルスイッチを使用し、状態を保持すること
- すべての組み込みプロバイダーまたはOpenRouterプリセットが3つすべてのルート容量を解決すること
- 生成されたClaude Codeコマンドにゲートウェイ接続変数が含まれること
- コンテキスト管理が有効な場合、生成されたコマンドに`CLAUDE_CODE_AUTO_COMPACT_WINDOW`と`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`が含まれること
- コンテキスト管理が無効な場合、生成されたコマンドが両方のコンテキスト変数を削除すること
- コピーされたコマンドが、実行中のAnthro Bridgeゲートウェイを通じてClaude Codeを起動すること

## トラブルシューティング

### ポート4000が既に使用されている

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### モデルが画像または動画入力を拒否する

モデルの機能はプロバイダーとルートによって異なります。GUIで機能バッジを確認し、互換性のあるルートを選択してください。

対応していない画像入力の場合、Anthro Bridgeは`non_vision_image_policy`に従います。

### アップグレード後に設定が巻き戻る

マイグレーションが実行されるように、まずアプリケーションを再起動してください。

問題が解決しない場合:

1. ユーザー設定をバックアップします。
2. バンドルされた設定と比較します。
3. 古いフィールドを削除するか、必要に応じてユーザー設定をリセットします。

安定版の設定場所:

```text
%APPDATA%\Anthro Bridge\config.json
```

開発版の設定場所:

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouterモデルリストが古い

Settingsの統合モデルリフレッシュコントロールを使用してください。Anthro Bridgeはモデルメタデータをキャッシュするため、OpenRouterがモデルエントリを変更した後に手動リフレッシュが必要になる場合があります。

### コンテキスト管理が不完全

自動コンテキスト管理には、3つすべての標準ルートの既知の容量が必要です。

Opus、Sonnet、Haikuに設定されている上流モデルを確認してください。カスタムモデルまたは新しくリリースされたモデルは、`model_context_windows.json`にまだ存在しない場合があります。

選択肢:

1. 既知のメタデータを持つ組み込みモデルを選択します。
2. 検証済みのモデルメタデータを静的レジストリに追加します。
3. `config.json`でmanualモードを使用します。
4. 圧縮を完全にClaude Codeに任せるには`claude_default`を使用します。

### Claude Codeが期待したコンテキスト設定を使用しない

Claude Codeが、別のターミナルコマンドではなく、生成されたPowerShellコマンドから起動されたことを確認してください。

同じPowerShellセッションで、以下を確認します:

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

これらの値は、起動環境が準備されたことを確認します。Claude Codeが変数を消費したことを証明するものではありません。最終的な確認には、Claude Codeの診断を使用するか、圧縮動作を観察してください。

## 翻訳

英語がソースREADMEです。

翻訳されたREADMEファイルは`docs/`に保存されています。英語のREADMEが変更された場合は、各言語を個別に編集するのではなく、英語ソースから翻訳ファイルを再生成または更新してください。

アプリケーションUIの言語ファイルは以下に保存されています:

```text
gui/src/i18n/lang/
```

## ライセンス

MIT License。[LICENSE](../LICENSE)を参照してください。
