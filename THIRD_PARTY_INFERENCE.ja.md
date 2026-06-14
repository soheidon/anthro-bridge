[English](THIRD_PARTY_INFERENCE.md) | [日本語](THIRD_PARTY_INFERENCE.ja.md) | [中文(简体)](THIRD_PARTY_INFERENCE.zh-CN.md)

# Claude Desktop / Cowork on 3P で Anthro Bridge を使う

Anthro Bridge は、Claude Desktop / Cowork on 3P から利用できる
ローカルの Anthropic互換ゲートウェイとして動作します。

Claude Desktop / Cowork on 3P では、開発者モードを有効にすると、
アプリ内の設定画面からサードパーティ推論を設定できます。

公式ドキュメント:

- [https://claude.com/docs/cowork/3p/installation](https://claude.com/docs/cowork/3p/installation)
- [https://claude.com/docs/cowork/3p/configuration](https://claude.com/docs/cowork/3p/configuration)

## 1. Anthro Bridge を起動する

まず Anthro Bridge を起動し、ゲートウェイを開始します。

デフォルトでは、Anthro Bridge は次のURLで待ち受けます。

```text
http://127.0.0.1:4000
```

Claude Desktop / Cowork on 3P から利用する間は、Anthro Bridge を起動したままにしてください。

## 2. Claude Desktop で開発者モードを有効にする

Claude Desktop を開きます。

Windowsでは、ログイン画面左上のアプリケーションメニューから操作します。

次を選択します。

```text
Help → Troubleshooting → Enable Developer Mode
```

開発者モードを有効にすると、`Developer` メニューが表示されます。

## 3. サードパーティ推論の設定画面を開く

次のメニューを開きます。

```text
Developer → Configure third-party inference
```

Claude Desktop のサードパーティ推論設定画面が開きます。

## 4. Connection を設定する

`Connection` セクションで、次を選択します。

```text
Gateway
```

続いて、以下の値を入力します。

| 項目                    | 値                       |
| --------------------- | ----------------------- |
| Gateway base URL      | `http://127.0.0.1:4000` |
| Gateway API key       | `sk-local-gateway`      |
| Gateway auth scheme   | `bearer`                |
| Gateway extra headers | 通常は空欄                   |

`Gateway API key` には、Anthro Bridge 側で設定したローカルAPIキーと同じ値を入力します。

## 5. Identity & Models を設定する

`Identity & Models` セクションで、Claude Desktop のモデル選択欄に表示したいモデルIDを追加します。

例:

```text
claude-sonnet-4-6
claude-haiku-4-5
```

モデルごとに表示名を付けることもできます。

例:

| Model ID            | 表示名             |
| ------------------- | --------------- |
| `claude-sonnet-4-6` | `Gateway Pro`   |
| `claude-haiku-4-5`  | `Gateway Flash` |

最初に登録したモデルが、モデル選択欄のデフォルトになります。

各モデル行を展開し、`Model ID` が Anthro Bridge に送信したいモデル名と完全に一致していることを確認してください。

`Offer 1M-context variant` は、上流プロバイダと選択モデルが拡張コンテキストに実際に対応している場合だけ有効にしてください。

## 6. 設定例

上記の設定は、内部的には次のようなサードパーティ推論設定に対応します。

```json
{
  "inferenceProvider": "gateway",
  "inferenceGatewayBaseUrl": "http://127.0.0.1:4000",
  "inferenceGatewayApiKey": "sk-local-gateway",
  "inferenceGatewayAuthScheme": "bearer",
  "inferenceModels": [
    {
      "name": "claude-sonnet-4-6",
      "labelOverride": "Gateway Pro"
    },
    {
      "name": "claude-haiku-4-5",
      "labelOverride": "Gateway Flash"
    }
  ]
}
```

## 7. 適用して Claude Desktop を再起動する

ゲートウェイとモデルリストを設定したら、設定をローカルに適用します。

必要に応じて Claude Desktop を再起動してください。

再起動後、Claude Desktop / Cowork on 3P からのリクエストは Anthro Bridge に送られます。Anthro Bridge は、そのリクエストを設定済みの上流プロバイダへ中継します。

## 注意

Anthro Bridge は、Anthropic互換APIのための非公式ローカルゲートウェイです。

Anthropic、Moon Bridge、および各モデル提供元とは提携・関係ありません。

Claude Desktop / Cowork on 3P のサードパーティ推論設定画面やメニュー名は、Claude Desktop のアップデートにより変更される可能性があります。
