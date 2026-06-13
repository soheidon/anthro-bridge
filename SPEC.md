# SPEC: Terra Bridge

## English

### Overview

A thin proxy + GUI management tool that routes Claude Desktop / Claude Code API requests through multiple providers' Anthropic-compatible endpoints.

### Architecture

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Embedded in Tauri app (axum 0.7 + reqwest)
       |
       | Routes by model field -> resolves correct upstream provider
       | Rewrites only model to upstream name
       | Injects thinking disabled for non-thinking variants
       | Per-model media support checking
       v
Provider Anthropic-compatible APIs
(DeepSeek / MiniMax / Kimi)
```

#### Design Principles

- **Shell model + provider selection**: Claude Desktop always sees `claude-sonnet-4-6` / `claude-haiku-4-5`. The actual LLM is selected in the GUI (DeepSeek / MiniMax / Kimi). The active provider's model mapping is used for routing.
- **Only active provider needs API key**: Since v0.5.0, only providers referenced by the route table are checked at startup. Non-active provider keys are not required.
- **Thin proxy**: Nothing modified except the `model` field. SSE forwarded byte-for-byte.
- **Lossless forwarding**: Message bodies, tool calls, thinking blocks pass through unmodified.
- **Windows-native GUI**: Tauri v2 + React 19 + TypeScript. Rust backend, Vite + React 19 frontend.
- **Zero external dependencies**: Proxy embedded in Tauri binary since v0.3.0. Python not required.
- **Multi-language**: 6 languages since v0.5.0 (en, ja, zh-CN, zh-TW, ko, fr). Add new languages by dropping files into `lang/`. First-run language picker.

### GUI Management Tool

Tauri v2 + React 19 + TypeScript. Two-panel layout: Dashboard + Settings.

```
+------------------------------------------+
|  Terra Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiniMax] [Kimi]          ||
|  +- Status ------------------------------+
|  | Port 4000 | API Key | Gateway URL    ||
|  | Model routing table                  ||
|  +- Latest Log --------------------------+
|  | Log viewer with Pro/Flash counters   ||
|  +---------------------------------------+
+------------------------------------------+

Settings (=):
  +- Language ----------------------------+
  | Dropdown for instant switching        |
  +- API Key -----------------------------+
  | Per-provider API key management       |
  +- Claude Desktop Setup ----------------+
  | Config JSON generation, copy,         |
  | config file detection                 |
  +- Gateway Config ----------------------+
  | config.json editor (advanced)         |
  +---------------------------------------+
```

### Tauri Commands

| # | Command | Type | Description |
|---|---------|------|-------------|
| 1 | `check_health` | async | Proxy health check |
| 2 | `check_gateway_status` | sync | Port 4000 + tokio task liveness |
| 3 | `check_api_key` | sync | Active provider API key status |
| 4 | `set_env_api_key` | sync | Persist API key via setx |
| 5 | `get_port_4000_process` | sync | Get PID of port 4000 via netstat |
| 6 | `read_config` | sync | Read config.json |
| 7 | `read_config_raw` | sync | Raw config.json text + encoding detect |
| 8 | `write_config` | sync | Save config.json (UTF-8 / Shift-JIS) |
| 9 | `read_latest_log` | sync | Read latest log |
| 10 | `read_log` | sync | Read specified log file |
| 11 | `list_logs` | sync | List log files |
| 12 | `create_new_log` | sync | Create new log file |
| 13 | `open_logs_folder` | sync | Open logs folder |
| 14 | `open_path` | sync | Open arbitrary path |
| 15 | `find_claude_configs` | sync | Auto-detect Claude Desktop config files |
| 16 | `start_proxy` | sync | Start proxy (resolve config -> spawn -> verify port) |
| 17 | `stop_proxy` | sync | Stop proxy (graceful shutdown) |
| 18 | `proxy_status` | sync | Check task liveness |
| 19 | `check_all_api_keys` | sync | All provider API key status |
| 20 | `update_active_provider` | sync | Save active_provider |
| 21 | `update_provider_api_key_env` | sync | Save provider api_key_env |
| 22 | `get_user_language` | sync | Get saved language preference |
| 23 | `set_user_language` | sync | Save language preference |
| 24 | `is_first_run` | sync | Determine first run (user_prefs.json existence) |

### Proxy Server (proxy.rs)

Ported from Python to Rust (axum 0.7/reqwest) in v0.3.0.

#### Endpoints

| Method | Path | Behavior |
|--------|------|----------|
| GET | `/health` | Health check |
| GET | `/v1/models` | Public model list (`visible: true` only) |
| POST | `/v1/messages` | Model resolve -> thinking injection -> media check -> forward (stream/non-stream) |
| POST | `/v1/messages/count_tokens` | Forward to upstream if supported |

#### Model Routing

Builds a reverse lookup table from gateway model -> (provider, upstream model) using each provider's `models` section. Since all providers use the same gateway model names, `active_provider` wins on collision. Effectively, only the active provider's models end up in the route table.

#### API Key Validation (since v0.5.0)

Pass 1: Build model route table (no API keys needed)
Pass 2: Only check API keys for providers referenced by the route table

#### Thinking Injection

For models with `thinking: "disabled"` in their config entry, injects `{"type": "disabled"}` only when the user has not explicitly set thinking.

#### Media Check / Image Sanitization

Per-model `supports_vision` / `supports_video` flags determine behavior. For non-vision models receiving images, `non_vision_image_policy` applies:
- `replace` (default): Replace image blocks with placeholder text
- `drop`: Remove image blocks (insert placeholder if content becomes empty)
- `reject`: Return 400 error

Video blocks always return 400. `non_vision_image_policy` is visible via `/health`.

### Multi-language

File-per-language architecture with `import.meta.glob` auto-discovery:

```
gui/src/i18n/lang/
  en.ts      English (canonical — defines TranslationKey type)
  ja.ts      Japanese
  zh-CN.ts   Chinese Simplified
  zh-TW.ts   Chinese Traditional
  ko.ts      Korean
  fr.ts      French
```

To add a language: copy `en.ts`, translate, rebuild. No code changes needed.

### config.json Reference

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
          "thinking": "disabled",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
}
```

---

## 日本語

### 概要

複数プロバイダーの Anthropic 互換 API を Claude Desktop / Claude Code Desktop から利用するための薄型プロキシ + GUI 管理ツール。

### 背景

Claude Desktop / Claude Code Desktop は Anthropic Messages API (`/v1/messages`) に直接リクエストを送る。これを各プロバイダーの Anthropic 互換エンドポイントに振り向けることで、複数プロバイダーのモデルを Anthropic クライアントから透過的に利用可能にする。

#### 解決する問題

- Claude Desktop 側のモデル名バリデーション（`inferenceModels[].name` に Anthropic 公式名しか使えない）
- LiteLLM の Anthropic->OpenAI 変換による情報ロス
- 複数プロバイダー・複数モデルを単一エンドポイントから利用可能

### アーキテクチャ

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- Tauri アプリに内蔵 (axum 0.7 + reqwest)
       |
       | リクエストの model フィールドをもとにプロバイダーを自動判別
       | model を upstream 名に書換え、他は完全透過転送
       | thinking 注入（thinking=disabled のバリアント向け）
       | モデル単位の画像/動画非対応チェック
       v
各プロバイダーの Anthropic-compatible API
(DeepSeek / MiniMax / Kimi)
```

#### 設計方針

- **シェルモデル + プロバイダ選択**: Claude Desktop には常に `claude-sonnet-4-6` / `claude-haiku-4-5` の2モデルを表示。実際の LLM は GUI で選択（DeepSeek / MiniMax / Kimi）。選択されたプロバイダのモデルマッピングがルーティングに使われる。
- **アクティブプロバイダのみ API キー必須**: v0.5.0 以降、起動時にチェックされる API キーはルートテーブルで参照されるプロバイダのみ。非アクティブプロバイダのキーは不要。
- **薄型プロキシ**: model フィールドの書換え以外は一切手を加えない。SSE もパースせずバイト単位で透過転送。
- **ロスレス転送**: メッセージ本文やツール呼び出し、thinking block を一切加工しない。
- **Windows ネイティブ GUI**: Tauri v2 + React 19 + TypeScript。バックエンドは Rust、フロントエンドは Vite + React 19。
- **ゼロ外部依存**: v0.3.0 以降、プロキシは Rust に移植され Tauri バイナリに内蔵。Python 不要。
- **多言語対応**: v0.5.0 から6言語（en, ja, zh-CN, zh-TW, ko, fr）。`lang/` フォルダにファイル追加で新言語対応可。初回起動時に言語選択画面。

### GUI 管理ツール

Tauri v2 + React 19 + TypeScript 製。ダッシュボード + 設定画面の2画面構成。

```
+------------------------------------------+
|  Terra Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiniMax] [Kimi]          ||
|  +- Status ------------------------------+
|  | Port 4000 | API Key | Gateway URL    ||
|  | Model routing table                  ||
|  +- Latest Log --------------------------+
|  | Log viewer with Pro/Flash counters   ||
|  +---------------------------------------+
+------------------------------------------+

設定画面 (=):
  +- Language ----------------------------+
  | ドロップダウンで即時切替              |
  +- API Key -----------------------------+
  | プロバイダごとのAPIキー管理           |
  +- Claude Desktop Setup ----------------+
  | 設定JSON生成・コピー・ファイル検出    |
  +- Gateway Config ----------------------+
  | config.json 編集（上級者向け）        |
  +---------------------------------------+
```

### Tauri コマンド一覧

| # | コマンド名 | 種別 | 説明 |
|---|-----------|------|------|
| 1 | `check_health` | async | プロキシ死活確認 |
| 2 | `check_gateway_status` | sync | ポート 4000 + tokio task の生存確認 |
| 3 | `check_api_key` | sync | アクティブプロバイダの API キー状態 |
| 4 | `set_env_api_key` | sync | setx で API キー永続保存 |
| 5 | `get_port_4000_process` | sync | netstat でポート 4000 の PID 取得 |
| 6 | `read_config` | sync | config.json 読込 |
| 7 | `read_config_raw` | sync | config.json 生テキスト + エンコーディング判定 |
| 8 | `write_config` | sync | config.json 保存（UTF-8 / Shift-JIS） |
| 9 | `read_latest_log` | sync | 最新ログ読込 |
| 10 | `read_log` | sync | 指定ログファイル読込 |
| 11 | `list_logs` | sync | ログファイル一覧 |
| 12 | `create_new_log` | sync | 新規ログファイル作成 |
| 13 | `open_logs_folder` | sync | ログフォルダを開く |
| 14 | `open_path` | sync | 任意パスを開く |
| 15 | `find_claude_configs` | sync | Claude Desktop 設定ファイル自動検出 |
| 16 | `start_proxy` | sync | プロキシ起動（設定解決 -> spawn -> ポート確認） |
| 17 | `stop_proxy` | sync | プロキシ停止（graceful shutdown） |
| 18 | `proxy_status` | sync | タスク生存確認 |
| 19 | `check_all_api_keys` | sync | 全プロバイダの API キー状態 |
| 20 | `update_active_provider` | sync | active_provider 保存 |
| 21 | `update_provider_api_key_env` | sync | プロバイダの api_key_env 保存 |
| 22 | `get_user_language` | sync | 保存された言語設定の取得 |
| 23 | `set_user_language` | sync | 言語設定の保存 |
| 24 | `is_first_run` | sync | 初回起動判定（user_prefs.json の有無） |

### プロキシサーバー (proxy.rs)

v0.3.0 で Python から Rust (axum 0.7/reqwest) に移植。

#### エンドポイント

| Method | Path | 動作 |
|--------|------|------|
| GET | `/health` | 死活確認 |
| GET | `/v1/models` | 公開モデル一覧（`visible: true` のみ） |
| POST | `/v1/messages` | モデル解決 -> thinking 注入 -> メディアチェック -> 転送（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 対応時のみ upstream へ転送 |

#### モデルルーティング

各プロバイダの `models` セクションから gateway model -> (provider, upstream model) の逆引きテーブルを構築。全プロバイダが同じ gateway model 名を使うため、`active_provider` が衝突時に優先される。結果としてアクティブプロバイダのモデルのみがルートテーブルに登録される。

#### API キー検証（v0.5.0〜）

Pass 1: モデルルートテーブル構築（APIキー不要）
Pass 2: ルートテーブルで参照されるプロバイダのみ API キーをチェック

#### Thinking 注入

`models` エントリに `thinking: "disabled"` が指定されているモデルに対し、ユーザーが明示的に thinking を設定していない場合のみ `{"type": "disabled"}` を注入。

#### メディアチェック / 画像サニタイズ

モデル単位の `supports_vision` / `supports_video` フラグで判定。非対応モデルに画像が送られた場合、`non_vision_image_policy` に従う:
- `replace`（デフォルト）: 画像ブロックをプレースホルダテキストに置換
- `drop`: 画像ブロックを削除（空になった場合はプレースホルダ挿入）
- `reject`: 400 エラーを返す

動画ブロックは常に 400 エラー。`non_vision_image_policy` は `/health` エンドポイントで確認可能。

### 多言語対応

`gui/src/i18n/lang/` に1言語1ファイル。`import.meta.glob` で自動検出。

```
gui/src/i18n/lang/
  en.ts      英語（正規 — TranslationKey 型を定義）
  ja.ts      日本語
  zh-CN.ts   中国語(簡体)
  zh-TW.ts   中国語(繁体)
  ko.ts      韓国語
  fr.ts      フランス語
```

新規言語追加: `en.ts` をコピー -> 翻訳 -> 再ビルド。コード変更不要。

### config.json リファレンス

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "表示名",
      "upstream_url": "Anthropic互換APIのベースURL",
      "api_key_env": "APIキー環境変数名",
      "default_model": "フォールバックモデル名",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "実モデル名" },
      "visible_models": ["claude-公開モデル名"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "実モデル名",
          "thinking": "disabled",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
}
```

---

## 中文(简体)

### 概述

一个薄型代理 + GUI 管理工具，可将 Claude Desktop / Claude Code 的 API 请求路由到多个提供商的 Anthropic 兼容端点。

### 背景

Claude Desktop / Claude Code Desktop 直接向 Anthropic Messages API (`/v1/messages`) 发送请求。将其路由到各提供商的 Anthropic 兼容端点，使 Anthropic 客户端能够透明地使用多个提供商的模型。

#### 解决的问题

- Claude Desktop 的模型名称验证（`inferenceModels[].name` 只能使用 Anthropic 官方名称）
- LiteLLM 的 Anthropic->OpenAI 转换导致的信息丢失
- 从单一端点使用多个提供商和多个模型

### 架构

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- 嵌入 Tauri 应用 (axum 0.7 + reqwest)
       |
       | 根据请求的 model 字段自动判断提供商
       | 仅将 model 重写为上游名称，其他完全透传
       | thinking 注入（针对 thinking=disabled 的变体）
       | 按模型检查图像/视频兼容性
       v
各提供商的 Anthropic-compatible API
(DeepSeek / MiniMax / Kimi)
```

#### 设计原则

- **外壳模型 + 提供商选择**: Claude Desktop 始终显示 `claude-sonnet-4-6` / `claude-haiku-4-5` 两个模型。实际的 LLM 在 GUI 中选择（DeepSeek / MiniMax / Kimi）。所选提供商的模型映射用于路由。
- **仅活跃提供商需要 API 密钥**: 自 v0.5.0 起，仅检查路由表引用的提供商的 API 密钥。非活跃提供商的密钥无需设置。
- **薄型代理**: 除 `model` 字段外不做任何修改。SSE 也不解析，逐字节透传。
- **无损转发**: 消息正文、工具调用、thinking block 完全不加修改。
- **Windows 原生 GUI**: Tauri v2 + React 19 + TypeScript。后端 Rust，前端 Vite + React 19。
- **零外部依赖**: 自 v0.3.0 起代理已移植到 Rust 并嵌入 Tauri 二进制文件。无需 Python。
- **多语言支持**: 自 v0.5.0 起支持 6 种语言（en, ja, zh-CN, zh-TW, ko, fr）。向 `lang/` 文件夹添加文件即可支持新语言。首次启动时显示语言选择界面。

### GUI 管理工具

Tauri v2 + React 19 + TypeScript 构建。仪表板 + 设置双面板布局。

```
+------------------------------------------+
|  Terra Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiniMax] [Kimi]          ||
|  +- Status ------------------------------+
|  | Port 4000 | API Key | Gateway URL    ||
|  | Model routing table                  ||
|  +- Latest Log --------------------------+
|  | Log viewer with Pro/Flash counters   ||
|  +---------------------------------------+
+------------------------------------------+

设置界面 (=):
  +- Language ----------------------------+
  | 下拉菜单即时切换                      |
  +- API Key -----------------------------+
  | 按提供商管理 API 密钥                 |
  +- Claude Desktop Setup ----------------+
  | 配置 JSON 生成、复制、文件检测        |
  +- Gateway Config ----------------------+
  | config.json 编辑器（高级用户）        |
  +---------------------------------------+
```

### Tauri 命令列表

| # | 命令名 | 类型 | 说明 |
|---|--------|------|------|
| 1 | `check_health` | async | 代理健康检查 |
| 2 | `check_gateway_status` | sync | 端口 4000 + tokio 任务存活检查 |
| 3 | `check_api_key` | sync | 活跃提供商 API 密钥状态 |
| 4 | `set_env_api_key` | sync | 通过 setx 持久保存 API 密钥 |
| 5 | `get_port_4000_process` | sync | 通过 netstat 获取端口 4000 的 PID |
| 6 | `read_config` | sync | 读取 config.json |
| 7 | `read_config_raw` | sync | config.json 原始文本 + 编码检测 |
| 8 | `write_config` | sync | 保存 config.json（UTF-8 / Shift-JIS） |
| 9 | `read_latest_log` | sync | 读取最新日志 |
| 10 | `read_log` | sync | 读取指定日志文件 |
| 11 | `list_logs` | sync | 日志文件列表 |
| 12 | `create_new_log` | sync | 创建新日志文件 |
| 13 | `open_logs_folder` | sync | 打开日志文件夹 |
| 14 | `open_path` | sync | 打开任意路径 |
| 15 | `find_claude_configs` | sync | 自动检测 Claude Desktop 配置文件 |
| 16 | `start_proxy` | sync | 启动代理（解析配置 -> 生成 -> 验证端口） |
| 17 | `stop_proxy` | sync | 停止代理（优雅关闭） |
| 18 | `proxy_status` | sync | 检查任务存活 |
| 19 | `check_all_api_keys` | sync | 所有提供商的 API 密钥状态 |
| 20 | `update_active_provider` | sync | 保存 active_provider |
| 21 | `update_provider_api_key_env` | sync | 保存提供商 api_key_env |
| 22 | `get_user_language` | sync | 获取已保存的语言偏好 |
| 23 | `set_user_language` | sync | 保存语言偏好 |
| 24 | `is_first_run` | sync | 判定首次运行（user_prefs.json 是否存在） |

### 代理服务器 (proxy.rs)

v0.3.0 中从 Python 移植到 Rust (axum 0.7/reqwest)。

#### 端点

| Method | Path | 行为 |
|--------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/v1/models` | 公开模型列表（仅 `visible: true`） |
| POST | `/v1/messages` | 模型解析 -> thinking 注入 -> 媒体检查 -> 转发（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 如果支持则转发到上游 |

#### 模型路由

从各提供商的 `models` 部分构建 gateway model -> (provider, upstream model) 反向查找表。由于所有提供商使用相同的 gateway model 名称，冲突时 `active_provider` 优先。最终只有活跃提供商的模型会进入路由表。

#### API 密钥验证（自 v0.5.0）

第 1 步: 构建模型路由表（无需 API 密钥）
第 2 步: 仅检查路由表引用的提供商的 API 密钥

#### Thinking 注入

对于配置中 `thinking: "disabled"` 的模型，仅当用户未显式设置 thinking 时注入 `{"type": "disabled"}`。

#### 媒体检查 / 图像清理

按模型的 `supports_vision` / `supports_video` 标志判断。对于不支持 Vision 的模型收到图像时，根据 `non_vision_image_policy` 处理:
- `replace`（默认）: 将图像块替换为占位符文本
- `drop`: 删除图像块（内容为空时插入占位符）
- `reject`: 返回 400 错误

视频块始终返回 400。`non_vision_image_policy` 可通过 `/health` 端点查看。

### 多语言支持

按文件的语言架构，通过 `import.meta.glob` 自动发现:

```
gui/src/i18n/lang/
  en.ts      英语（规范 — 定义 TranslationKey 类型）
  ja.ts      日语
  zh-CN.ts   中文(简体)
  zh-TW.ts   中文(繁体)
  ko.ts      韩语
  fr.ts      法语
```

添加语言: 复制 `en.ts` -> 翻译 -> 重新构建。无需修改代码。

### config.json 参考

```json
{
  "active_provider": "deepseek",
  "providers": {
    "<provider_id>": {
      "display_name": "显示名称",
      "upstream_url": "Anthropic 兼容 API 的基础 URL",
      "api_key_env": "API 密钥环境变量名",
      "default_model": "回退模型名称",
      "force_anthropic_version": null,
      "supports_count_tokens": false,
      "supports_vision": false,
      "supports_video": false,
      "model_map": { "claude-sonnet-4-5": "实际模型名" },
      "visible_models": ["claude-公开模型名"],
      "models": {
        "claude-sonnet-4-6": {
          "upstream_model": "实际模型名",
          "thinking": "disabled",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false }
}
```
