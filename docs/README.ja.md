[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Claude Code / Claude Desktop をコーディングハーネスとして使用し、推論をサードパーティ LLM API にルーティングしながら、外部モデルを Google Antigravity のプランナー・レビュアーとして活用します。**

Anthro Bridge は、AI 支援ソフトウェア開発のための Windows コンパニオンアプリケーションです。2 つの補完的なワークフローをサポートしています。

1. **Claude Code / Claude Desktop 向け 3P ゲートウェイ** — Claude のリポジトリ探索、ツール使用、ファイル編集、テスト実行はそのままに、推論をサードパーティプロバイダーにルーティングします。
2. **Google Antigravity 向け MCP プランナー & レビュアー** — `anthro-bridge/plan` および `anthro-bridge/review` MCP ツールを通じて、実装計画と実装後レビューを外部モデルに委任します。

---

## 2 つのメインワークフロー

### 1. Claude Code / Claude Desktop と 3P ゲートウェイ

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **ハーネスとモデルの分離**: Claude のエージェント型ツールを活かしながら、推論をサードパーティプロバイダーにルーティングします。
- **動的マルチプロファイルルーティング**: GUI からアクティブなプロバイダー、OpenRouter プロファイル、モデルルートを切り替えられます。
- **セットアップガイド**: [Claude Desktop / Cowork 3P ゲートウェイ セットアップ](THIRD_PARTY_INFERENCE.ja.md)

### 2. Antigravity と MCP プランナー & レビュアー

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
設定済み外部モデル (プランナー / レビュアー)
    ↓
実装計画 / レビュー判定
    ↓
Antigravity がサブスクリプションの処理能力を使って
実装・テストを実行
```

- **計画と実行の分離**: 外部モデルが高レベルの計画またはレビュー判定を生成し、Antigravity のサブスクリプション処理能力がトークン集約的なコード編集を実行します。
- **ライブ GUI 設定**: プランナーまたはレビュアーのプロバイダー、モデル、推論努力度を切り替えると、次回の呼び出し時から即座に反映されます。
- **セットアップガイド**: [Google Antigravity + Anthro Bridge MCP セットアップ](ANTIGRAVITY_MCP.ja.md)

**Antigravity グローバルコマンド:**

- **`/anthro-plan`** — 設定済み外部モデルに実装計画を委任します。
- **`/anthro-revise`** — 新しいフィードバックや制約に基づいて既存の計画を修正します。
- **`/anthro-review`** — コミット前に承認済み計画に対して完成した実装をレビューし、明示的に READY / NOT READY の判定を行います。

**推奨ワークフロー:**

```text
/anthro-plan → 実装 & テスト → /anthro-review → コミット
```

---

## 対応プロバイダー

| プロバイダー | 接続方式 | 対応ファミリー | 推論コントロール |
|---|---|---|---|
| **DeepSeek** | Direct API | DeepSeek V4.1 Flash, V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | Direct API | kimi-for-coding, kimi-for-coding-highspeed | Thinking モード |
| **MiniMax** | Direct API | MiniMax M3, M2.7 | モデル固有 |
| **Kimi / Moonshot** | Direct API | Kimi K2.x, Kimi K3 | Thinking / 推論努力度 |
| **MiMo / Xiaomi** | Direct API | MiMo V2.6 Flash, Pro, Pro-UltraSpeed（V2.5 後方互換あり） | Normal / Thinking |
| **OpenRouter** | マルチプロファイルゲートウェイ | 下記 OpenRouter セクション参照 | モデル固有 / プロファイル固有 |

### DeepSeek（ダイレクト）

組み込みの **Direct DeepSeek** プリセット: Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low。

- `deepseek-v4.1-flash` — 現行フラッグシップ推論モデル（$0.27 / 1M 入力 · $1.10 / 1M 出力）。
- `deepseek-v4-pro-0813` — 拡張推論なしの高品質ベースラインモデル（$0.27 / 1M 入力 · $1.10 / 1M 出力）。

### Kimi Code（ダイレクト）

Moonshot Kimi とは別の、コーディング専用 API（`KIMI_CODE_API_KEY`）:

- `kimi-for-coding` — フルクオリティのコーディングモデル。
- `kimi-for-coding-highspeed` — 低レイテンシ版。

### MiMo / Xiaomi（ダイレクト）

組み込みの **Direct MiMo** プリセットルート:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- デフォルトモデル: `mimo-v2.6-flash`

モデル: `mimo-v2.6-flash`、`mimo-v2.6-pro`、`mimo-v2.6-pro-ultraspeed`（選択可能）。3 つすべてが 100 万トークンのコンテキストウィンドウとネイティブマルチモーダル機能（テキスト、画像、動画）に対応。

**Normal / Thinking**: MiMo はシンプルな Normal/Thinking の切り替えのみ — 推論努力度レベルはありません。

**V2.5 後方互換性**: 保存済みの `mimo-v2.5`、`mimo-v2.5-pro`、`mimo-v2.5-pro-ultraspeed` ルートはそのまま機能し続けます。変更されていないレガシーデフォルトは起動時に自動的に V2.6 へ移行します。

### OpenRouter

複数の名前付きプロファイルをサポート。OpenAI の全モデルカタログ（単一ドロップダウン）:

| モデル ID | 表示名 |
|---|---|
| `openai/gpt-6-astra` | GPT-6 Astra |
| `openai/gpt-6-astra-pro` | GPT-6 Astra Pro |
| `openai/gpt-astra-latest` | GPT Astra Latest |
| `openai/gpt-5.6-sol` | GPT-5.6 Sol |
| `openai/gpt-5.6-sol-pro` | GPT-5.6 Sol Pro |
| `openai/gpt-5.6-terra` | GPT-5.6 Terra |
| `openai/gpt-5.6-terra-pro` | GPT-5.6 Terra Pro |
| `openai/gpt-5.6-luna` | GPT-5.6 Luna |
| `openai/gpt-5.6-luna-pro` | GPT-5.6 Luna Pro |

**GPT-6 Astra**: コンテキスト 1.05M · 推論努力度: `low / medium / high / xhigh / max`。  
**GPT-6 Astra Pro**: コンテキスト 1.05M · 常時 Pro 推論（`reasoning.mode = pro`）、ユーザーによる努力度選択不可。  
**GPT Astra Latest**: 最新の Astra ファミリーモデルを追跡するエイリアス。

組み込みの **OpenRouter: chatGPT** プリセット: Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium。

その他利用可能: **OpenRouter: Gemini**（Gemini 3.8 Flash · 推論努力度 `low / medium / high`）、**OpenRouter: Poolside**、**OpenRouter: Tencent**、**OpenRouter: InclusionAI**、**OpenRouter: StepFun**。

---

## モデル料金（v0.22.1 時点）

| モデル | 入力 | 出力 |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash (OpenRouter) | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## インストール

[Releases](https://github.com/soheidon/anthro-bridge/releases) ページから最新の Windows インストーラー（`Anthro Bridge_x.x.x_x64-setup.exe`）をダウンロードして実行してください。

インストーラーは 8 言語に対応しており、アップグレード時に既存のユーザー設定を保持します。

---

## クイックスタート

### ワークフロー 1: Claude Code / Claude Desktop 向け 3P ゲートウェイ

1. Anthro Bridge の **Settings > API Key** を開き、使用するプロバイダーの API キーを設定します。
2. ダッシュボードでプロバイダーまたは OpenRouter プロファイルを選択します。
3. **Start Gateway** をクリックします（`http://127.0.0.1:4000` で起動）。
4. Claude Code または Claude Desktop を接続します。
   - **Claude Code**: Settings の **Copy Claude Code launch command** をクリックし、コマンドを PowerShell に貼り付けます。
   - **Claude Desktop / Cowork**: [Claude Desktop 3P セットアップガイド](THIRD_PARTY_INFERENCE.ja.md) に従ってください。

### ワークフロー 2: Google Antigravity 向け MCP プランナー & レビュアー

1. Anthro Bridge で、選択したプランナー / レビュアーモデルの API キーを設定します。
2. **MCP** タブを選択し、**Settings > Antigravity > MCP Plan Settings** でモデルを設定します。
3. Antigravity の MCP 設定で `anthro-bridge.exe` を `["--mcp-server"]` 付きで登録します（または Anthro Bridge の **Configure Automatically** をクリックします）。
4. `/anthro-plan` で計画を作成し、`/anthro-revise` で計画を更新し、`/anthro-review` でコミット前に実装をレビューします。
5. 完全な [Antigravity MCP セットアップガイド](ANTIGRAVITY_MCP.ja.md) に従ってください。

---

## API キー

| プロバイダー | 環境変数 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## ドキュメント

- [Claude Desktop / Cowork 3P ゲートウェイ セットアップ](THIRD_PARTY_INFERENCE.ja.md)
- [Google Antigravity + Anthro Bridge MCP セットアップ](ANTIGRAVITY_MCP.ja.md)
- [設定リファレンス（`config.json`）](CONFIGURATION.md)
- [プロバイダー詳細 & 推論コントロール](PROVIDERS.md)
- [開発 & 検証ガイド](DEVELOPMENT.md)

---

## トラブルシューティング

### ポート 4000 が使用中の場合
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### アップグレード後に設定が元に戻る場合
マイグレーションが実行されるよう、アプリケーションを再起動してください。設定は `%APPDATA%\Anthro Bridge\config.json` に保存されています。

### MCP プランナーの呼び出しが失敗する場合
**MCP** タブで選択したプロバイダーの API キーが設定されているか、または Windows ユーザー環境変数（例: `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`）にエクスポートされていることを確認してください。MCP の利用に 3P ゲートウェイの起動は不要です。

---

## ライセンス

MIT ライセンス。[LICENSE](../LICENSE) を参照してください。
