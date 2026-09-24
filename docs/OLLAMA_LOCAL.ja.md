[English](OLLAMA_LOCAL.md) | 日本語 | [中文(简体)](OLLAMA_LOCAL.zh-CN.md) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | [한국어](OLLAMA_LOCAL.ko.md) | [Français](OLLAMA_LOCAL.fr.md) | [Deutsch](OLLAMA_LOCAL.de.md) | [Español](OLLAMA_LOCAL.es.md)

[← Anthro Bridge README に戻る](../README.md)

# Claude Code + Ollama Local

Anthro Bridge では、Claude Desktop や MCP が使用しているクラウドプロバイダーの設定を変更することなく、Claude Code だけをローカルで動作する Ollama モデルへルーティングできます。

---

## 構成

```text
Claude Code
     │
     │ Anthro Bridge Claude Code 識別マーカー (X-Anthro-Bridge-Client: claude-code)
     ▼
Anthro Bridge Gateway
     │
     ├─ Gateway ルート (`active_route = "gateway"`) ──→ グローバルクラウドプロバイダー (DeepSeek / MiMo / OpenRouter など)
     │
     └─ Ollama ルート  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                     ローカルモデル
```

Ollama Local は **Claude Code 専用** のルートです。Claude Desktop や Anthro Bridge MCP ツールが使用するグローバルなクラウドプロバイダー設定は変更されません。

---

## 必要なもの

1. **Anthro Bridge** が起動していること。
2. **Ollama** がローカルPCでインストールされ起動していること。
3. Ollama に少なくとも1つのモデル（`gemma4:latest`, `llama3.3:70b`, `qwen2.5-coder:32b` など）がダウンロードされているか、有効なカスタムモデルタグがあること。
4. Anthro Bridge が生成した起動コマンドから Claude Code が起動されていること。

---

## Ollama Local の設定

Anthro Bridge の **設定 > APIキー** を開きます。APIキータスクの下にある **Ollama Local** 設定カードで設定します。

### メイン設定行

- **モデル**: 取得済みモデルおよびカスタムモデルから選択するドロップダウン。
- **更新 (`🔄 更新`)**: ローカル Ollama の `/api/tags` エンドポイントに問い合わせてインストール済みモデル一覧を取得。
- **Thinking**: **通常（無効）** または **Thinking（有効）** を選択。
- **ダッシュボードに表示**: ダッシュボード上に Ollama Local タイルを表示するかどうかの表示切り替えチェックボックス。

### 詳細設定 (`▸ 詳細設定`)

- **エンドポイント**: Ollama のベースURL。既定値は `http://127.0.0.1:11434` です。
- **ビジョン (Base64)**: マルチモーダルモデル用の画像入力を有効/無効化（既定値: 無効）。
- **コンテキストウィンドウ**: 任意の明示的なトークン容量（例: `131072`）。
- **APIキー不要**: ローカル loopback の Ollama は APIキーを必要としません。

---

## インストール済みモデルの取得

**更新 (`🔄 更新`)** をクリックすると、Anthro Bridge は次へリクエストを送信します：

```http
GET http://127.0.0.1:11434/api/tags
```

### セキュリティと設定値の保持

- **loopback 限定**: モデル取得はローカルループバックアドレス（`127.0.0.1`, `localhost`, `[::1]`）に限定されます。
- **エラー時の安全な保持**: Ollama が停止している場合やタイムアウト（2秒上限）した場合でも、通知が表示されるのみで、保存済みのモデル設定がクリアされることはありません。
- **カスタムモデルの保持**: 保存済みモデルが取得結果に含まれていない場合でも、ドロップダウン内で選択状態が維持されます。
- **カスタムタグの手動入力**: `+ カスタムモデルタグ...` を選択して任意のモデルタグを直接入力できます。

---

## ダッシュボードから Ollama へ切り替える

1. 設定で **ダッシュボードに表示** を有効にします。
2. **ダッシュボード** を開きます。
3. **Ollama Local** タイルをクリックします：
   - `claude_code.active_route = "ollama"` に設定されます。
   - グローバルな `active_provider`（例: `deepseek`）は **変更されません**。
   - Ollama タイルが **Claude Code 使用中** として強調表示されます。
4. クラウドプロバイダーに戻す場合：
   - ダッシュボード上のクラウドプロバイダー（DeepSeek, MiMo 等）をクリックします。
   - Claude Code は自動的に `claude_code.active_route = "gateway"` に戻ります。

---

## Thinking（思考モード）の仕様

Anthro Bridge は Thinking の選択状態を Ollama が要求する Anthropic 互換フォーマットに変換します：

- **通常 (Normal)**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking (有効)**:
  ```json
  "thinking": { "type": "enabled" }
  ```

*※ Ollama Local では reasoning effort の段階指定（Low/High/Max 等）は行いません。*

---

## コンテキストウィンドウと自動圧縮

- `context_window` の設定は **任意** です。
- 値を指定した場合、Claude Code のコンテキスト管理および自動圧縮（auto-compact）計算に使用されます。
- 未指定（`null`）の場合、Anthro Bridge がコンテキスト長を勝手に推測することはなく、既定の自動圧縮ルールに従います。

---

## Claude Code 識別マーカー

Anthro Bridge が生成する Claude Code 起動コマンドは、`ANTHROPIC_CUSTOM_HEADERS` を通じて内部識別マーカーを注入します：

```text
X-Anthro-Bridge-Client: claude-code
```

- **ヘッダーの正規化**: 大文字小文字を問わず重複や古いマーカーを1つの正規マーカーに統一し、ユーザー独自のカスタムヘッダーを保持します。
- **上流転送前の除去**: このマーカーは Anthro Bridge 内部のルーティング判定にのみ使用され、上流のプロバイダーや Ollama に転送される前に確実に除去されます。

---

## クライアントの完全分離

| クライアント | ルート選択元 | 接続先 |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` または `"ollama"`) | 選択中のクラウドプロバイダー / Ollama Local |
| **Claude Desktop** | `active_provider` (グローバルクラウドプロバイダー) | クラウド Gateway (DeepSeek, MiMo, OpenRouter 等) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | MCP 専用の計画・レビュー用プロバイダー |

この分離設計により、Claude Code をローカルモデルで実行しながら、Claude Desktop や Antigravity MCP を安定したクラウド API で継続利用できます。

---

## トラブルシューティング

### 更新してもモデルが表示されない
1. Ollama がバックグラウンドまたは端末で動作しているか確認します：
   ```powershell
   ollama list
   ```
2. 詳細設定のエンドポイントが `http://127.0.0.1:11434` であることを確認します。

### 保存したモデルが一覧にない
取得結果に含まれないモデルであっても、Anthro Bridge は設定を保持します。また `+ カスタムモデルタグ...` から手動入力も可能です。

### Claude Code がクラウド側へ接続される
1. ダッシュボードで **Ollama Local** がアクティブになっていることを確認してください。
2. Anthro Bridge で生成された起動コマンド（`Claude Code起動コマンドをコピー`）から起動したことを確認してください。

### Claude Desktop がクラウド側を使い続ける
正常な動作です。Ollama Local は Claude Code 専用です。
