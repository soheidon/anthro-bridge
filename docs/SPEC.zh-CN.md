[English](../SPEC.md) | [日本語](SPEC.ja.md) | [中文(简体)](SPEC.zh-CN.md) | [中文(繁體)](SPEC.zh-TW.md) | [한국어](SPEC.ko.md) | [Français](SPEC.fr.md) | [Deutsch](SPEC.de.md) | [Español](SPEC.es.md)

# SPEC: Anthro Bridge

## 概述

一个薄型代理 + GUI 管理工具，可将 Claude Desktop / Claude Code 的 API 请求路由到多个提供商的 Anthropic 兼容端点。

### 架构

```
Claude Desktop / Claude Code
       |
       v
proxy.rs (127.0.0.1:4000)  <- 嵌入 Tauri 应用 (axum 0.7 + reqwest)
       |
       | 按 model 字段路由 -> 解析正确的上游提供商
       | 仅将 model 重写为上游名称
       | 为不支持 thinking 的变体注入 thinking 禁用
       | 按模型进行媒体支持检查
       v
各提供商的 Anthropic-compatible API
(DeepSeek / MiniMax / Kimi / MiMo / OpenRouter)
```

#### 设计原则

- **外壳模型 + 提供商选择**: Claude Desktop 始终显示 `claude-opus-5` / `claude-sonnet-5` / `claude-haiku-4-5`。实际的 LLM 在 GUI 中选择（DeepSeek / MiniMax / Kimi / MiMo / OpenRouter）。活跃提供商的模型映射用于路由。
- **OpenRouter 支持**: 路由到 OpenRouter 的 Anthropic 兼容端点，默认使用 Poolside Laguna S/XS。专用 thinking 模式控制（Max/On/Off）在请求时转换为 OpenRouter 的 `reasoning` 格式。
- **仅活跃提供商需要 API 密钥**: 自 v0.5.0 起，启动时仅检查路由表引用的提供商的 API 密钥。非活跃提供商的密钥无需设置。
- **薄型代理**: 除 `model` 字段外不做任何修改。SSE 逐字节透传。
- **无损转发**: 消息正文、工具调用、thinking 块完全不加修改地透传。
- **Windows 原生 GUI**: Tauri v2 + React 19 + TypeScript。后端 Rust，前端 Vite + React 19。
- **零外部依赖**: 自 v0.3.0 起代理已嵌入 Tauri 二进制文件。无需 Python。
- **多语言**: 8 种语言（en, ja, zh-CN, zh-TW, ko, fr, de, es）。向 `lang/` 文件夹添加文件即可支持新语言。首次启动时显示语言选择器。
- **推理强度**: DeepSeek V4 Pro 在 Thinking 模式下支持推理强度 High / Max；V4 Flash 支持 Low / High / Max。Normal 模式下禁用推理强度。为 V4 Pro 路由保存的旧 `low`/`medium` 强度会在启动时迁移为 `high`。
- **能力检测**: 实时能力标志（supports_image_url, supports_image_base64, supports_video_url, supports_video_base64）从 OpenRouter API 获取并持久化到 config.json。
- **峰谷定价感知**: DeepSeek 和 OpenRouter 的峰值时段在本地时区显示。
- **MiniMax-M3 thinking 切换**: MiniMax-M3 通过 Anthropic 兼容 API 支持 Thinking ON/OFF（`thinking: {"type":"adaptive"}` / `{"type":"disabled"}`）。M2.x 系列模型仍为仅思考模式。启动时迁移将现有用户的旧 `thinking_only` 转换为 `thinking`。
- **响应模型标识规范化**: 将 API 响应（SSE 流式和非流式）中的上游模型名称重写回 Anthropic 官方模型名称。由 config.json 中的 `normalize_response_model_identity` 和运行时 `AtomicBool` 控制。独立的保存命令（`update_normalize_model_identity`）以避免与服务器配置保存相互污染。
- **结构化通信日志**: `tracing` + `tracing-appender` 将结构化日志写入 `%APPDATA%\Anthro Bridge\Communication-Logs\proxy-*.log`。每个请求通过 `AtomicU64` 计数器获得关联 ID。日志条目包含请求模型、网关模型、上游模型、规范化结果和跳过原因。不记录任何敏感数据（提示词、正文、API 密钥）。
- **PEAK 徽章**: 仪表板中为峰值价格模型显示彩色粉色徽章。
- **UTC 偏移显示**: 时区选择器在每个选项旁显示动态 UTC 偏移（如 UTC+09:00）。
- **Laguna S/XS 2.1 令牌上限失败检测**: 在 SSE 流和非流响应中检测带有 `stop_reason: "max_tokens"` 的纯推理响应。当达到每轮令牌上限而未产生可用文本或工具调用时记录警告。适用于通过 OpenRouter 使用的所有 Poolside Laguna 模型。
- **Poolside thinking:disabled 透传**: 将客户端发送的 `thinking: { type: "disabled" }` 转换为 OpenRouter 的 `reasoning: { enabled: false }` 格式以用于 Poolside 模型，确保即使未保存配置设置也能正确转发禁用的 thinking。
- **Laguna Opus 默认迁移**: 一次性幂等迁移将 `poolside/laguna-s-2.1` OpenRouter 用户的 `claude-opus-5` 默认值从 thinking 开启改为普通模式。新安装模板反映更新后的默认值。
- **OpenRouter 多模型集**: 每个用户可有多个 OpenRouter 模型集，每个模型集有自己的 API 密钥和模型配置。通过 Tauri 命令进行模型集的增删改查（CRUD）。可从仪表板或设置切换活跃模型集。模型集可通过拖拽重新排序、隐藏，并按配置的顺序持久化。
- **OpenRouter 仪表板卡片**: 仪表板为每个可见的 OpenRouter 模型集创建一个卡片，模型集不存在时显示回退卡片。模型摘要仅在 OpenRouter 显示时隐藏第一个 `/` 之前的供应商命名空间；完整的上游 ID 保持不变以用于路由。
- **OpenRouter 模型注册表**: 内置的已知 OpenRouter 模型注册表（`model_capabilities.rs`、`builtinOpenRouter.ts`），带有预配置的能力（视觉、视频、thinking 策略、推理强度）、供应商分组和价格数据。用于无需实时 API 调用即可进行模型分类。
- **OpenRouter 价格详情**: 内置价格支持当前值和修订后的标准值，涵盖提示词、输出和缓存输入费率，包括 GPT-5.6 Sol、Terra、Luna 和 Pro 变体。GUI 在两者同时可用时同时显示促销价和标准价。
- **GPT-5.6 模型支持**: OpenRouter 模型集可使用 Sol、Terra 和 Luna 模型变体，具有能力感知的 thinking 控制以及适用的长上下文费率价格说明。内置的 OpenAI GPT-5.6 Balanced 模型集将 Opus 5 → GPT-5.6 Sol、Sonnet 5 → GPT-5.6 Terra、Haiku 4.5 → GPT-5.6 Luna 进行路由，三条路由均采用 Thinking High 推理强度（适用于新安装）；已保存的路由不会自动更改。
- **仪表板驱动的窗口尺寸**: 初始及行数变化时，根据三列网格中可见的仪表板卡片计算窗口高度。计算会考虑卡片高度、网格间距、原生最小尺寸、监视器工作区、DPI 缩放和窗口装饰，同时在行数不变时保留手动调整大小。
- **本地化 NSIS 安装程序**: Windows 安装程序提供英语、日语、简体中文、繁体中文、韩语、法语、德语和西班牙语语言选项，并捆绑 Anthro Bridge 应用图标。
- **回归测试覆盖**: Vitest 覆盖包括 OpenRouter 模型集排序和保存竞争、生产价格数据、仪表板卡片数量语义和监视器感知的窗口尺寸。
- **通过 OpenRouter 新增提供商**: InclusionAI 和 StepFun 作为 OpenRouter 模型提供商加入，具有专用能力标志、thinking 模式控制和供应商分组。
- **Tencent Hy3 thinking 模式**: 支持腾讯混元（Hunyuan）模型的 Low/High 推理强度。proxy.rs 中的 thinking 模式转换将 `thinking_mode` 映射为 OpenRouter 的 `reasoning` 格式。UI 将 Low/High 显示为下拉选项。
- **Kimi K3 修复**: 从能力定义中移除了硬编码的 `forced_reasoning_effort`。将固定的 "Max" 显示替换为可配置的下拉选择器。默认值来自已保存的配置，回退到 "max"。
- **配置写入序列化**: 所有写入配置的 Tauri 命令都通过 `execute_serialized_config_mutation` 配合 `Mutex` 守卫进行序列化。`ConfigState` 结构体提供 `applied_config`、`in_flight_config` 和 `pending_ops` 跟踪及验证。防止多个设置更改同时保存时出现竞争条件。
- **OpenRouter UI 竞争修复**: (1) `syncUiFromSavedRouteRef` 最新回调 ref 防止陈旧闭包覆盖新路由的 UI。(2) `rollbackRouteId` 守卫防止跨路由的 Phase 2 回滚。(3) `useRouteSaveGeneration` hook 为所有处理器提供 `begin()`/`isCurrent()` 代际守卫。(4) 保存队列 hook（`useOpenRouterSaveQueue`）具有排空循环、抢占检测和 OpenRouter 聚合重启。
- **开发/正式版应用标识隔离**: `paths.rs` 中的 `AppChannel` 枚举（`Stable`/`Dev`）选择不同的标识符（`com.soheidon.anthro-bridge` vs `.dev`）、配置目录（`Anthro Bridge` vs `Anthro Bridge Dev`）和缓存路径。开发通道使用 `tauri.dev.conf.json`。NPM 脚本：`npm run dev`（开发）、`npm run dev:stable`（正式版）。
- **配置模板嵌入**: `include_str!()` 在编译时嵌入 `config_template.rs`，消除了对捆绑的 `config.json` 的运行时依赖。`merge_bundled_providers` 返回带类型化错误处理的 `Result`。
- **前端回归测试**: 7 个针对 OpenRouter 保存竞争条件的 vitest 回归测试，使用 `QueueHarness` 和 `GenerationHandlerHarness`。测试覆盖：最新回调 ref、跨路由回滚守卫、身份捕获、刷新重试（失败 + 成功路径）、进行中的抢占和代际守卫。
- **Claude Code 上下文管理**: 针对 Claude Code 的模型感知自动压缩。`resolve_effective_auto_compact` 将每条标准路由（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）解析为其上游模型，在静态的 `model_context_windows.json` 注册表中查找每个模型的上下文容量，在 Auto 模式下使用最小已知容量作为安全上下文窗口。上下文控制仅在所有三个容量都已知时应用（否则状态为 Incomplete）。标题栏切换开关可开启/关闭上下文管理；高级模式和阈值在 `config.json` 的 `claude_code.auto_compact` 下设置。模式：`auto`、`manual`（`window_tokens`）、`claude_default`。
- **Claude Code 启动命令生成**: `build_claude_code_launch_command` 生成一条完整的 PowerShell 命令，结合网关连接变量（`ANTHROPIC_BASE_URL` 指向本地网关，`ANTHROPIC_AUTH_TOKEN` = `sk-local-gateway`）与 Claude Code 上下文控制变量（`CLAUDE_CODE_AUTO_COMPACT_WINDOW`、`CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`）。当上下文管理被禁用、不完整或设置为 Claude 默认值时，该命令使用 `Remove-Item Env:... -ErrorAction SilentlyContinue` 移除过期的上下文变量，以避免先前设置的会话值泄漏到新的启动中。Claude 设置面板中的"复制 Claude Code 启动命令"按钮将命令复制到剪贴板。Anthro Bridge 仅生成并复制该命令——它从不执行该命令。
- **共享模型路由模块**: `model_routing.rs` 将路由到上游的解析提取为纯函数，由 `proxy.rs` 和上下文解析器共享，确保上下文窗口解析出的上游模型与代理实际转发的模型一致。
- **上下文容量注册表**: `model_context_windows.json` 是已知上下文容量的静态注册表，涵盖内置直连提供商模型（DeepSeek、MiniMax、Kimi、MiMo）和内置 OpenRouter 模型（Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6）。未知的自定义 OpenRouter 模型仍是有效的路由目标，但会报告上下文管理为 Incomplete，直到添加元数据或配置手动模式。

### GUI 管理工具

Tauri v2 + React 19 + TypeScript。仪表板 + 设置双面板布局。

```
+------------------------------------------+
|  Anthro Bridge                   |
|  [启动/停止网关] [状态]    [=]   |
+------------------------------------------+
|  仪表板                                   |
|  +- 选择 LLM 提供商 -------------------+|
|  | [DeepSeek] [MiMo] [MiniMax] [Kimi]          ||
|  +- 状态 -------------------------------+
|  | 端口 4000 | API 密钥 | 网关 URL     ||
|  | 模型路由表                            ||
|  +- 最新日志 ----------------------------+
|  | 带 Pro/Flash 计数器的日志查看器      ||
|  +---------------------------------------+
+------------------------------------------+

设置 (=):
  +- 语言 --------------------------------+
  | 下拉菜单即时切换                      |
  +- API 密钥 ----------------------------+
  | 按提供商管理 API 密钥                 |
  +- Claude Desktop 设置 -----------------+
  | 配置 JSON 生成、复制、                 |
  | 配置文件检测                          |
  +- 网关配置 ----------------------------+
  | config.json 编辑器（高级）            |
  +---------------------------------------+
```

### Tauri 命令

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
| 25 | `openrouter_get_models` | async | 获取/缓存 OpenRouter 模型目录 |
| 26 | `set_model_upstream` | sync | 保存网关模型的上游模型 + thinking 配置 + 能力标志 |
| 27 | `update_server_config` | sync | 保存服务器 host/port/CORS 设置 |
| 28 | `update_normalize_model_identity` | sync | 保存响应模型标识规范化开关（更新配置 + 运行时 AtomicBool） |
| 29 | `update_claude_code_auto_compact_global` | sync | 切换全局 Claude Code 上下文管理（enabled + trigger percent） |
| 30 | `update_claude_code_auto_compact_target` | sync | 设置每提供商/模型集的上下文模式（auto / manual / claude_default）+ 手动 window tokens |
| 31 | `update_claude_code_context_settings` | sync | 全局 + 目标上下文设置的组合原子更新 |
| 32 | `resolve_claude_code_auto_compact` | sync | 解析有效上下文设置（模式、window tokens、trigger percent、状态） |
| 33 | `build_claude_code_launch_command` | sync | 生成完整的 PowerShell Claude Code 启动命令（网关 + 上下文环境变量） |

### 代理服务器 (proxy.rs)

v0.3.0 中从 Python 移植到 Rust (axum 0.7/reqwest)。

#### 端点

| 方法 | 路径 | 行为 |
|--------|------|------|
| GET | `/health` | 健康检查 |
| GET | `/v1/models` | 公开模型列表（仅 `visible: true`） |
| POST | `/v1/messages` | 模型解析 -> thinking 注入 -> 媒体检查 -> 转发（stream/non-stream） |
| POST | `/v1/messages/count_tokens` | 如果支持则转发到上游 |

#### 模型路由

使用各提供商的 `models` 部分构建 gateway model -> (provider, upstream model) 反向查找表。由于所有提供商使用相同的网关模型名称，冲突时 `active_provider` 优先。实际上，只有活跃提供商的模型会进入路由表。

#### API 密钥验证（自 v0.5.0）

第 1 步: 构建模型路由表（无需 API 密钥）
第 2 步: 仅检查路由表引用的提供商的 API 密钥

#### Thinking 注入

对于配置条目中 `thinking: "disabled"` 的模型，仅当用户未显式设置 thinking 时注入 `{"type": "disabled"}`。

#### 响应模型规范化

当启用 `normalize_response_model_identity` 时，代理会重写上游响应中的 `model` 字段：

- **非流式**: 解析 JSON 响应，将 `model` 重写为 Anthropic 规范名称，重新序列化
- **流式（SSE）**: 拦截 `message_start` 事件帧，使用字节范围替换就地重写 `model`，以保留 SSE 格式和空白
- **跳过原因**: `disabled`（开关关闭）、`non_success_status`（非 200 响应）、`content_encoding_not_transformable`（gzip/brotli）、`stream_error`、`stream_cancelled`
- **决策逻辑**: 纯函数（`should_normalize_nonstream`、`nonstream_skip_reason`），生产代码和测试共用

#### 媒体检查 / 图像清理

按模型的 `supports_vision` / `supports_video` 标志判断行为。对于收到图像但不支持视觉的模型，适用 `non_vision_image_policy`：
- `replace`（默认）: 将图像块替换为占位符文本
- `drop`: 删除图像块（内容为空时插入占位符）
- `reject`: 返回 400 错误

视频块始终返回 400。`non_vision_image_policy` 可通过 `/health` 查看。

#### Claude Code 上下文管理

Claude Code 上下文控制使用两个官方环境变量：

```
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

解析管线：

1. 将每条标准路由（claude-opus-5、claude-sonnet-5、claude-haiku-4-5）解析为其上游模型
2. 在 `model_context_windows.json` 中查找每个上游模型的上下文容量
3. 要求三个容量全部已知
4. 使用最小已知容量作为安全上下文窗口
5. 应用配置的触发百分比

模式：`auto`（最小已知容量）、`manual`（`window_tokens`）、`claude_default`（Claude Code 自身的默认值；不设置变量）。有效状态为 `applied`、`disabled` 或 `incomplete`。

启动命令结合网关连接变量与上下文变量：

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

当上下文控制未应用时，命令会先移除过期的变量：

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

百分比覆盖只会提前触发压缩；延迟压缩超过 Claude Code 默认值的值可能被忽略。Anthro Bridge 仅生成并复制该命令——它从不执行该命令，这也不证明特定 Claude Code 版本会遵守这些变量（最终确认需要 Claude Code 诊断或观察到的压缩行为）。

### 多语言

按文件的语言架构，通过 `import.meta.glob` 自动发现：

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

添加语言: 复制 `en.ts`，翻译，重新构建。无需修改代码。

### config.json 参考

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
          "thinking_mode": "normal",
          "reasoning_effort": "high",
          "supports_vision": true,
          "supports_video": true,
          "visible": true
        }
      }
    },
    "openrouter": {
      "display_name": "OpenRouter",
      "upstream_url": "https://openrouter.ai/api/v1",
      "api_key_env": "OPENROUTER_API_KEY",
      "default_model": "openrouter/auto",
      "models": {
        "claude-opus-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "thinking",
          "reasoning_effort": "max",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-sonnet-5": {
          "upstream_model": "poolside/laguna-s-2.1",
          "thinking_mode": "normal",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        },
        "claude-haiku-4-5": {
          "upstream_model": "poolside/laguna-xs-2.1",
          "thinking_mode": "thinking",
          "supports_image_url": false,
          "supports_image_base64": false,
          "supports_video_url": false,
          "supports_video_base64": false
        }
      }
    }
  },
  "non_vision_image_policy": "replace",
  "normalize_response_model_identity": true,
  "server": { "host": "127.0.0.1", "port": 4000, "enable_cors": false },
  "claude_code": {
    "auto_compact": {
      "enabled": false,
      "trigger_percent": 90
    }
  }
}
```

每个提供商或 OpenRouter 模型集也可以通过 `claude_code: { "auto_compact": { "mode": "auto" } }` 设置默认上下文模式。路由的有效模式取提供商/模型集的值，回退到全局块；`resolve_claude_code_auto_compact` 返回解析结果。
