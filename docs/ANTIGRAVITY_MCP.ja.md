[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Anthro Bridge README に戻る](README.ja.md)

# Google Antigravity で Anthro Bridge MCP を使用する

Anthro Bridge には、専用の `plan` ツール（`anthro-bridge/plan`）を提供する Model Context Protocol (MCP) サーバーが組み込まれています。これにより、Google Antigravity などのエージェント環境において、アーキテクチャ設計や実装計画の策定を外部 LLM（DeepSeek V4、MiMo、Kimi、MiniMax、OpenRouter モデル等）へ委託しながら、実際のコード編集、コマンド実行、ビルド、テストといったトークン消費の大きい作業は Antigravity のサブスクリプション枠で実行できます。

---

## 1. このワークフローの仕組み

```text
Antigravity
    ↓
リポジトリ探索 (関連ファイル・コードを収集)
    ↓
anthro-bridge / plan (タスク・コンテキスト・制約を渡してMCP呼び出し)
    ↓
Anthro Bridge MCP サーバー
    ↓
外部プランナーモデル (Anthro Bridge GUIで設定)
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
2. **`anthro-bridge-mcp-server.exe`** がビルドまたは配置されていること（例: `mcp-server/target/release/anthro-bridge-mcp-server.exe`）。
3. プランナーとして利用したいプロバイダーの **API キー** が設定されていること。
4. **Google Antigravity** がインストールされ起動していること。

---

## 3. Antigravity に MCP サーバーを設定する

1. Google Antigravity を開きます。
2. 以下を開きます:
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. `mcpServers` オブジェクトに `anthro-bridge` の設定を追加します:

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> MCP 設定ファイル内に API キーを平文で記述する必要はありません。MCP サーバーは、Windows のユーザー環境変数（`DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` 等）または Anthro Bridge の保存済み設定から自動的にキーを読み込みます。

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

1. **Anthro Bridge** デスクトップアプリを開きます。
2. 最上部の **MCP** タブを選択します。
3. 有効にするプランナー **プロバイダー** または **プロファイル**（DeepSeek、MiMo、OpenRouter 等）を選択します。
4. **設定**（または MCP Plan 詳細設定）を開き、以下を構成します:
   - **モデル (Model)**
   - **Thinking モード**
   - **推論強度 (Reasoning Effort)**
5. 設定を保存します。

> [!NOTE]
> Anthro Bridge MCP サーバーは、各 `plan()` ツールの呼び出し時に現在の設定を動的に読み込みます。GUI でプロバイダーやモデルを変更した場合でも、MCP サーバーや Antigravity を再起動する必要はありません。

---

## 6. 手動で plan ツールを呼び出す

Antigravity のチャットで直接プランナーの呼び出しを指示できます:

```text
このプロジェクトを調査したうえで、anthro-bridge/plan MCP ツールを使って実装計画を立ててください。まだ実装はしないでください。
```

Antigravity が関連ファイルを調査してコンテキストをまとめ、`anthro-bridge/plan` を呼び出して実装計画を提示します。

---

## 7. Workspace Rule による計画作成の自動化

[`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) のような Workspace Rule を配置することで、複雑なコーディングタスク時にプランナーの呼び出しを自動化できます:

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. 一般的な自動化ワークフローの流れ

```text
ユーザー: 「機能 X をリファクタリングしてマルチプロファイルに対応させて」
    ↓
Antigravity がコードベースを調査しコンテキストを要約
    ↓
Antigravity が自動的に anthro-bridge/plan を呼び出し
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

- **独立した動作**: MCP サーバーは Anthro Bridge 3P Gateway とは独立して動作します。MCP ツールの使用にあたって 3P Gateway を起動しておく必要はありません。
- **料金の分離**: `anthro-bridge/plan` の呼び出しには、選択したプロバイダーの API 利用料金が発生します。その後のファイル編集やテストは Antigravity のサブスクリプション枠を使用します。
- **即時反映**: Anthro Bridge GUI でのプランナー設定変更は、次回の `plan()` 呼び出しから即座に有効になります。
