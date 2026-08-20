[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Anthro Bridge README に戻る](README.ja.md)

# Google Antigravity で Anthro Bridge MCP を使用する

Anthro Bridge は独立した別個の MCP サーバー実行ファイルを必要としません。インストールされた単一の `anthro-bridge.exe` が、デスクトップ GUI アプリケーションと MCP サーバーの両方の機能を提供します。Antigravity は同じ実行ファイルを `--mcp-server` 引数付きで呼び出すことで MCP モードを開始します。

```text
通常起動
anthro-bridge.exe
→ Anthro Bridge デスクトップアプリ / 3P Gateway

MCP起動
anthro-bridge.exe --mcp-server
→ Antigravity 向けヘッドレス stdio MCP サーバー
```

これによって、Google Antigravity などのエージェント環境において、アーキテクチャ設計や実装計画の策定を外部 LLM（DeepSeek V4、MiMo、Kimi、MiniMax、OpenRouter モデル等）へ `anthro-bridge/plan` 経由で委託しながら、実際のコード編集、コマンド実行、ビルド、テストといったトークン消費の大きい作業は Antigravity のサブスクリプション枠で実行できます。

---

## 1. このワークフローの仕組み

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
設定された外部プランナーモデル
    ↓
構造化された実装計画が返却される
    ↓
Antigravity がサブスク枠で
ファイル編集・ビルド・テストを実行
```

- **外部 API**: リポジトリのコンテキストに基づく実装計画の生成のみを担当（各プロバイダーから従量課金）。
- **Antigravity サブスクリプション**: ファイルの読み書き、コード編集、ツール実行、テスト実行などの実装ループを担当。
- **責務の分離**: 高度な推論モデルによる計画の恩恵を受けつつ、通常の実装で外部 API のトークンを無駄に消費しません。

---

## 2. 前提条件

1. **Anthro Bridge** が Windows にインストールされていること。
2. プランナーとして利用したいプロバイダーの認証（Anthro Bridge 内設定またはシステム環境変数）が設定されていること。
3. **Google Antigravity** がインストールされ起動していること。

---

## 3. Antigravity に MCP サーバーを設定する

### 方法 1 — Anthro Bridge GUI からの設定（推奨）

1. Anthro Bridge を開き、最上部ナビの **設定**（`[設定]` タブ）を開き、左側のサブナビから **Antigravity** を選択します。
2. **Google Antigravity 連携設定** カード内を確認します:
   - **対象実行ファイル**: 通常は現在実行中の `anthro-bridge.exe` のパスが表示されています。ポータブル版や開発ビルドなど別のバイナリを使用したい場合は **変更** (`antigravity.btnChangeExe`) ボタンから実行ファイルを選択します。
   - **登録・更新**: **Antigravityの設定を更新** (`antigravity.btnUpdate`) をクリックすると、`%USERPROFILE%\.gemini\config\mcp_config.json` 内の他の MCP サーバー設定を維持したまま、`anthro-bridge` の設定を安全に追加・更新します。
   - **登録解除**: Antigravity から登録を削除したい場合は **設定を解除** (`antigravity.btnRemove`) をクリックします。
   - **設定フォルダ確認**: **設定フォルダを開く** (`antigravity.btnOpenFolder`) をクリックすると、設定ファイルが置かれているディレクトリを Windows Explorer で直接開くことができます。

---

### 方法 2 — 手動設定（上級者向け）

1. Anthro Bridge の **設定 > Antigravity** で **設定フォルダを開く** をクリックし、Windows Explorer で `%USERPROFILE%\.gemini\config\` を開きます。
2. `mcp_config.json` を開き、`mcpServers` オブジェクトに `anthro-bridge` の設定を追加します:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

開発版バイナリを使用する場合は、以下のように指定します:
```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

> [!TIP]
> Antigravity の `mcp_config.json` にプロバイダーの API キーを記述する必要はありません。MCP サーバーは Anthro Bridge 既存の認証解決（Windows ユーザー環境変数の `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` や Anthro Bridge の保存済み設定）を利用してキーを自動ロードします。

---

## 4. MCP 接続を確認する

Antigravity の **Installed MCP Servers** 画面で、`anthro-bridge` が認識されていることを確認します:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Anthro Bridge でプランナーモデルを設定する

Anthro Bridge では、プランナーの選択と詳細設定の役割が明確に分かれています:

1. **トップレベル `MCP` タブ (`MCP for Antigravity`)**:
   - 利用可能なプロバイダー（DeepSeek、OpenRouter、MiniMax、MiMo、Kimi）とプロファイルのカード一覧が表示されます。
   - カードをクリックすることで、アクティブなプランナー送信先を即座に切り替えます。
2. **`設定` > `Antigravity`**:
   - **MCP Plan 詳細設定** カード: プロバイダー/プロファイルごとに、使用モデル、Thinking モード、推論強度 (Reasoning Effort) を詳細に設定できます。
   - **Google Antigravity 連携設定** カード: MCP サーバー登録状態の管理や Antigravity Commands（Global Skills）の管理を行います。

> [!NOTE]
> Anthro Bridge MCP サーバーは、各 `plan()` ツールの呼び出し時に現在の設定を動的に読み込みます。GUI でプロバイダーやモデル設定を変更した場合でも、MCP サーバーや Antigravity を再起動する必要はありません。

---

## 6. Antigravity Commands (`/anthro-plan` & `/anthro-revise`) の利用（推奨）

**設定 > Antigravity** の **Google Antigravity 連携設定** カードから Antigravity Commands をインストールすると、すべてのワークスペースからスラッシュコマンドを利用できます:

- **すべてインストール** (`antigravity.btnInstallAll`) をクリックするか、各コマンド横の **インストール** (`antigravity.commandBtnInstall`) をクリックします。

### 新規実装計画の作成:
```text
/anthro-plan <実装したい課題や機能の説明>
```
*リポジトリのコンテキストを収集し、`anthro-bridge/plan` を呼び出し、ファイル編集やビルドを実行せずに安全に停止して実装計画を提示します。*

### 既存実装計画の修正・フィードバック反映:
```text
/anthro-revise <反映したいフィードバックや変更要件>
```
*アクティブコンテキストまたは `implementation_plan.md` から現在の実装計画を特定し、フィードバックとともに `anthro-bridge/plan` へ渡して計画を更新します。*

> [!IMPORTANT]
> `/anthro-plan` や `/anthro-revise` を明示実行している間は、コマンド自身が 1 回の planner 呼び出しを管理するため、Workspace Rule による二重の planner 呼び出しは発生しません。

---

## 7. Workspace Rule による計画作成の自動化

プロジェクトに [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) のような Workspace Rule を配置することで、複雑なコーディングタスク時に外部プランナーを自動呼び出しできます:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
   Note: "Exactly once" means duplicate planner calls are prohibited once a successful usable result is obtained. If the tool call itself fails or returns an unusable response (e.g. transport or decoding error), exactly 1 recovery retry is permitted.
5. Use the returned DeepSeek plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

### 呼び出しポリシー:
- **軽微・局所的なタスク (Trivial / localized tasks)**（単語修正、1行の微修正、文法調整など）: プランナーは呼び出されません。
- **非自明なタスク (Non-trivial tasks)**（設計変更、複数ファイルにまたがる機能実装、複雑なデバッグなど）: Antigravity がコードベースを調査し、`anthro-bridge/plan` を 1 回呼び出し、得られた計画に基づいて実装を進めます。

---

## 8. 一般的な自動化ワークフローの流れ

```text
ユーザー: 「機能 X をリファクタリングしてマルチプロファイルに対応させて」
    ↓
Antigravity がコードベースを調査しコンテキストを要約
    ↓
Antigravity が自動的に anthro-bridge/plan を呼び出し (1回)
    ↓
Anthro Bridge が選択された外部モデルへプロンプトを送信
    ↓
Antigravity が構造化された実装計画を受信
    ↓
ユーザーが計画を確認・承認
    ↓
Antigravity がファイルの編集、テストを実行し変更を検証
```

---

## 9. 重要な注意事項

- **独立した動作**: MCP サーバーは Anthro Bridge 3P Gateway とは完全に独立して動作します。MCP ツールの使用にあたって 3P Gateway を起動（ON）にしておく必要はありません。
- **料金の分離**: `anthro-bridge/plan` の呼び出しには、選択したプロバイダーの API 利用料金が発生します。その後のファイル編集やテストは Antigravity のサブスクリプション枠を使用します。
- **即時反映**: Anthro Bridge GUI でのプランナープロバイダ・モデル設定変更は、次回の `plan()` 呼び出しから即座に有効になります。
