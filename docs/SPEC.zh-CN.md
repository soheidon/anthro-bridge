[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

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
- **多语言支持**: 8 种语言（en, ja, zh-CN, zh-TW, ko, fr, de, es）。向 `lang/` 文件夹添加文件即可支持新语言。首次启动时显示语言选择界面。
- **峰谷定价感知**: DeepSeek 峰值时间段在本地时区显示，并用颜色编码的 PEAK 徽章（粉色）区分。
- **UTC 偏移显示**: 时区选择器在每个选项旁显示动态 UTC 偏移（如 UTC+09:00）。

### GUI 管理工具

Tauri v2 + React 19 + TypeScript 构建。仪表板 + 设置双面板布局。

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [Start/Stop Gateway] [Status]    [=]   |
+------------------------------------------+
|  Dashboard                                |
|  +- Select LLM Provider ----------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]          ||
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

默认路由 (v0.11.0):

| 网关模型 | DeepSeek | MiMo | MiniMax | Kimi |
|---|---|---|---|---|
| claude-opus-4-8 | deepseek-chat | mimo-v2-pro | MiniMax-M3 + Thinking | kimi-k2.7-code + Thinking |
| claude-sonnet-5 | deepseek-chat | mimo-v2-flash | MiniMax-M3 + Thinking | kimi-k2.6 + Thinking |
| claude-haiku-4-5 | deepseek-chat | mimo-v2-flash | MiniMax-M3 + Thinking | kimi-k2.6 + Normal |

#### API 密钥验证（自 v0.5.0）

第 1 步: 构建模型路由表（无需 API 密钥）
第 2 步: 仅检查路由表引用的提供商的 API 密钥

#### Thinking 注入 (v0.9.0〜)

根据 `models` 条目的 `thinking_mode` 控制四种行为模式:
- `disabled`: 始终注入 `{"type": "disabled"}`
- `thinking`: 用户未指定时注入 `{"type": "enabled", "budget_tokens": 10000}`
- `thinking_only`: 无 `thinking` 参数则返回 400 错误，必须自动注入
- `normal`: 不干预

#### K3 特殊处理 (v0.11.0)

Kimi K3 不支持 `thinking` 参数。通过 `suppressThinkingParameter: true` + `forcedReasoningEffort: "max"` 实现:
- 从请求中移除 `thinking` 参数
- 注入 `reasoning_effort: "max"`
- 移除 temperature/top_p 等固定参数

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
  de.ts      德语
  es.ts      西班牙语
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
          "thinking_mode": "normal",
          "reasoning_effort": "high",
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
