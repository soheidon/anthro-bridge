[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**Claude Code Desktop をコーディングハーネスとして使用し、実装をサードパーティ API へルーティング。外部モデルを Antigravity のプランナーとして活用。**

Anthro Bridge は、AI 支援ソフトウェア開発のための Windows 向けコンパニオンアプリケーションです。主に以下の 2 つのワークフローをサポートしています。

1. **Claude Code / Claude Desktop + 3P Gateway**: Claude Code Desktop などの優れたコーディングハーネスをそのまま使いながら、ローカルの Anthropic 互換 3P Gateway を介してサードパーティ LLM API（DeepSeek、MiMo、MiniMax、Kimi、OpenRouter）へモデルリクエストをルーティングします。
2. **Antigravity + MCP Planner**: Anthro Bridge MCP の `plan` ツール（`anthro-bridge/plan`）を通じて、アーキテクチャ設計や実装計画の策定を外部モデルへ委託し、実際のコード編集やテストは Antigravity のサブスクリプション枠で実行します。

---

## 2 つの主要ワークフロー

### 1. Claude Code / Claude Desktop + 3P Gateway

Claude Code Desktop や Claude Desktop をエージェント型コーディングハーネスとして利用しつつ、本来直接利用できないサードパーティ LLM API へリクエストをルーティングします。

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **ハーネスとモデルの分離**: Claude 側のリポジトリ探索、ツール使用、ファイル編集、ビルド・テスト実行をそのまま維持しながら、推論のみをサードパーティプロバイダーへ送信。
- **動的なマルチプロファイルルーティング**: GUI ダッシュボードからアクティブなプロバイダーや OpenRouter プロファイルを即座に切り替え、Opus、Sonnet、Haiku のルートを個別にカスタマイズ可能。
- **セットアップガイド**: [Claude Desktop / Cowork 3P Gateway 設定手順](THIRD_PARTY_INFERENCE.ja.md)

### 2. Antigravity + MCP Planner

実装計画や設計を Anthro Bridge MCP の `plan` ツール（`anthro-bridge/plan`）経由で外部モデルに委託し、トークン消費の多いコード編集やコマンド実行は Antigravity のサブスクリプション枠で行います。

```text
Antigravity
    ↓
リポジトリ探索 (コンテキスト収集)
    ↓
anthro-bridge / plan (MCP)
    ↓
Anthro Bridge MCP Server
    ↓
GUIで選択した外部LLM
    ↓
構造化された実装計画
    ↓
Antigravity がサブスク枠で
ファイル編集・ビルド・テストを実行
```

- **計画と実装の役割分担**: 外部モデルが高レベルな実装計画を作成し、Antigravity のサブスクリプション枠で実際のファイル変更やテストループを実行。
- **リアルタイム GUI 設定**: Anthro Bridge の GUI でプランナープロバイダー、モデル、Thinking/推論強度を変更すると、次回の `plan()` 呼び出しから即座に反映。
- **セットアップガイド**: [Google Antigravity + Anthro Bridge MCP 設定手順](ANTIGRAVITY_MCP.ja.md)

---

## 対応プロバイダー

| プロバイダー | 接続タイプ | 対応モデルファミリー | 推論制御 |
|---|---|---|---|
| **DeepSeek** | 直接 API | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | 直接 API | MiniMax M3, M2.7 | モデル固有 |
| **Kimi / Moonshot** | 直接 API | Kimi K2.x, Kimi K3 | Thinking / 推論強度 |
| **MiMo / Xiaomi** | 直接 API | MiMo V2.5, V2.5 Pro | Thinking モード |
| **OpenRouter** | マルチプロファイル Gateway | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini 等 | モデル固有 / プロファイル固有 |

---

## インストール

[Releases](https://github.com/soheidon/anthro-bridge/releases) ページから最新の Windows インストーラー（`Anthro Bridge_x.x.x_x64-setup.exe`）をダウンロードして実行してください。

インストーラーは 8 言語（英語、日本語、簡体字中国語、繁体字中国語、韓国語、フランス語、ドイツ語、スペイン語）に対応し、アップグレード時も既存の設定を保持します。

---

## クイックスタート

### ワークフロー 1: Claude Code / Claude Desktop 向け 3P Gateway

1. Anthro Bridge の **設定 > APIキー** を開き、利用したいプロバイダーの API キーを設定します。
2. ダッシュボードで使用するプロバイダーまたは OpenRouter プロファイルを選択します。
3. **Gateway 起動** をクリックします（`http://127.0.0.1:4000` で待受開始）。
4. Claude Code または Claude Desktop を接続します:
   - **Claude Code**: 設定画面の **Claude Code 起動コマンドをコピー** をクリックし、PowerShell に貼り付けて実行。
   - **Claude Desktop / Cowork**: [Claude Desktop 3P 設定手順](THIRD_PARTY_INFERENCE.ja.md) に従って設定。

### ワークフロー 2: Google Antigravity 向け MCP Planner

1. Anthro Bridge でプランナーとして使用したいプロバイダーの API キーを設定します。
2. **MCP** タブを選択し、**設定 > MCP Plan 詳細設定** でモデルと推論設定を構成します。
3. Antigravity の MCP 設定に `anthro-bridge.exe` を引数 `["--mcp-server"]` とともに登録します。
4. Antigravity チャットから `anthro-bridge/plan` を呼び出すか、Workspace Rule で自動化します。
5. 詳しい手順は [Google Antigravity + Anthro Bridge MCP 設定手順](ANTIGRAVITY_MCP.ja.md) を参照してください。

---

## ドキュメント

- [Claude Desktop / Cowork 3P Gateway 設定手順](THIRD_PARTY_INFERENCE.ja.md)
- [Google Antigravity + Anthro Bridge MCP 設定手順](ANTIGRAVITY_MCP.ja.md)
- [設定リファレンス (`config.json`)](CONFIGURATION.ja.md)
- [プロバイダー詳細・モデル仕様](PROVIDERS.ja.md)
- [開発・検証ガイド](DEVELOPMENT.ja.md)

---

## トラブルシューティング

### ポート 4000 が既に使用されている
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### アップグレード後に設定が戻る
マイグレーションを実行するためにアプリを再起動してください。設定ファイルは `%APPDATA%\Anthro Bridge\config.json` に保存されています。

### MCP Planner の呼び出しに失敗する
Anthro Bridge の **MCP** タブで選択されているプロバイダーの API キーが設定されているか、または Windows のユーザー環境変数（`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY` 等）に設定されているか確認してください。MCP の使用に 3P Gateway の起動は不要です。

---

## ライセンス

MIT License。詳細は [LICENSE](../LICENSE) を参照してください。
