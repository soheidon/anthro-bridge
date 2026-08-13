[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**当前版本：0.16.0**

Anthro Bridge 是一个本地网关和桌面配置工具，让 Claude Desktop 和 Claude Code 能够通过兼容 Anthropic 的 API 使用多个第三方大模型提供商。

本应用程序包含：

- 一个用 Rust 编写的本地代理服务器
- 一个基于 Tauri 2、React 和 TypeScript 构建的原生 Windows 图形界面
- 从 Anthropic 模型名称映射到各提供商上游模型的基于模型的路由
- 针对每条路由的模型、推理和功能配置

Anthro Bridge 是一个独立项目，不是 Moon Bridge 的分支、前端或配套应用。

## 0.16.0 版本亮点

0.16.0 版本新增了模型感知的 Claude Code 上下文管理功能。

- Anthro Bridge 会解析分配给 Opus、Sonnet 和 Haiku 路由的上游模型的上下文容量。
- 在自动模式下，三条路由中已知的最小容量被用作安全的 Claude Code 上下文窗口。
- 只有三条路由的容量都已知时，才会应用上下文控制。
- 标题栏提供了一个紧凑的上下文管理开关；高级模式和阈值仍可通过 `config.json` 设置。
- 该应用可以生成一条完整的 PowerShell 启动命令，其中包含 Anthro Bridge 连接变量和 Claude Code 上下文控制变量。
- 当上下文管理被禁用或不完整时，生成的命令会从当前 PowerShell 会话中清除过时的上下文控制变量。
- 内置上下文元数据覆盖标准的直接提供商模型和内置 OpenRouter 模型。
- 生成的命令及其环境变量行为由 Rust 单元测试、Windows PowerShell 集成测试和前端复制流程测试覆盖。

## 支持的模型

Anthro Bridge 支持两类上游模型。

### 原生集成

以下提供商通过其自身的 Anthropic 兼容 API 提供支持，无需 OpenRouter 账户。

| 提供商 | 支持的模型系列 | 连接方式 |
|---|---|---|
| DeepSeek | DeepSeek V4 Pro 和 V4 Flash | 直接连接提供商 API |
| MiniMax | MiniMax M3 和 M2.7 变体 | 直接连接提供商 API |
| Kimi / Moonshot | Kimi K2.x 和 Kimi K3 | 直接连接提供商 API |
| MiMo / Xiaomi | MiMo V2.5 和 V2.5 Pro 变体 | 直接连接提供商 API |

### 通过 OpenRouter 支持的模型

以下模型通过 OpenRouter 配置文件访问。每个配置文件都有独立的 API 密钥、路由映射和推理设置。

| 供应商或模型系列 | 内置支持 | 推理控制 |
|---|---|---|
| Poolside Laguna S 2.1 / Laguna XS 2.1 | 是 | 模型专属的 Thinking 控制 |
| Tencent Hy3 | 是 | Low 和 High 推理强度 |
| InclusionAI Ring | 是 | 模型专属的 Thinking 和推理控制 |
| StepFun Step 3.5 / Step 3.7 | 是 | 支持 Low、Medium 和 High（视具体模型） |
| InclusionAI Ling 系列 | 是 | 模型专属的 Thinking 控制 |
| OpenAI GPT-5.6 Sol / Terra / Luna | 是 | 模型专属的 Thinking 和推理控制 |

其他 OpenRouter 模型也可从实时 OpenRouter 模型列表中选取或手动输入。内置支持意味着 Anthro Bridge 已预先了解该模型系列、功能标志、供应商分组和推理控制行为。

## 工作原理

Claude Desktop 和 Claude Code 使用 Anthropic 模型名称发送请求，例如：

- `claude-opus-5`
- `claude-sonnet-5`
- `claude-haiku-4-5`

Anthro Bridge 将这些名称视为稳定的路由标识符。图形界面决定每条路由使用哪个提供商和上游模型。

示例：

```text
Claude Code request
  model: claude-sonnet-5

Anthro Bridge route
  provider: OpenRouter profile "Hy3"
  upstream model: tencent/hunyuan-a13b-instruct
  reasoning mode: high
```

只有必须为上游提供商适配的字段才会被修改。只要上游 API 支持，消息、工具调用、工具结果、thinking 块和流式数据都将按原样保留。

## 主要功能

### 提供商路由

Anthro Bridge 支持两种上游连接类型：

1. **直接提供商集成**，连接到提供商的 Anthropic 兼容 API。
2. **OpenRouter 配置文件**，连接到 OpenRouter，可通过单个 API 路由到多个供应商和模型系列。

#### 直接提供商集成

| 提供商 ID | 显示名称 | 默认端点 |
|---|---|---|
| `deepseek` | DeepSeek | `https://api.deepseek.com/anthropic` |
| `minimax` | MiniMax | `https://api.minimax.io/anthropic` |
| `kimi` | Kimi / Moonshot | `https://api.moonshot.cn/anthropic` |
| `mimo` | MiMo / Xiaomi | `https://api.xiaomimimo.com/anthropic` |

#### OpenRouter 集成

| 连接类型 | 显示名称 | 端点 |
|---|---|---|
| 多配置文件模型网关 | OpenRouter | `https://openrouter.ai/api/v1` |

OpenRouter 不被视为单个模型提供商。每个 OpenRouter 配置文件可以独立从支持的供应商组（如 Poolside、Tencent、InclusionAI 和 StepFun）中选择模型，也可以从 OpenRouter API 中搜索到的或手动输入的其他模型中选择。

每条 Anthropic 路由都可以独立映射到直接提供商模型或通过 OpenRouter 配置文件选择的模型。

### OpenRouter 多配置文件支持

可以创建并独立管理多个 OpenRouter 配置文件。

每个配置文件都有独立的：

- 配置文件名称
- API 密钥配置
- Opus、Sonnet 和 Haiku 路由映射
- Thinking 或推理设置
- 缓存的 OpenRouter 模型列表

配置文件可以在图形界面中添加、重命名、删除、通过拖拽重新排序、隐藏和选择。仪表板为每个可见配置文件显示一张卡片，并在刷新后保持已保存的顺序。

内置的 OpenRouter 供应商组目前包括 Poolside、Tencent、InclusionAI、StepFun、OpenAI GPT-5.6 以及其他已识别的模型系列。未知模型仍可通过搜索或自定义模型输入使用。仪表板会将 `poolside/laguna-s-2.1` 之类的供应商限定 ID 缩短为 `laguna-s-2.1` 以便阅读，同时保留完整 ID 用于路由。

### OpenRouter 定价和模型详情

设置中的模型价格面板会显示受支持 OpenRouter 模型的内置价格，包括提示词、输出和缓存输入价格。促销价格可与调整后的标准价格一同显示，包括 GPT-5.6 Sol、Terra 和 Luna 变体及其 Pro 变体。定价备注可在适用时包含长上下文价格。

### 自适应仪表板尺寸

初始窗口高度根据三列仪表板中可见的提供商和 OpenRouter 卡片数量计算。额外的卡片行会增加窗口高度，同时遵循系统的原生最小尺寸、显示器工作区、DPI 缩放和标题栏装饰。当配置文件可见性或数量变化时，会按新的行数重新计算高度；只要行数不变，手动调整的尺寸会被保留。

### 多语言 Windows 安装程序

Windows NSIS 安装程序支持选择英语、日语、简体中文、繁体中文、韩语、法语、德语和西班牙语。安装程序使用 Anthro Bridge 应用图标，并在升级期间保留稳定的用户配置。

### 最新的界面可靠性改进

配置写入已序列化，OpenRouter 保存使用带过时请求保护的队列更新路径，配置文件重新排序操作在刷新失败后可干净地恢复。回归测试覆盖配置文件排序、保存竞争、模型定价、仪表板卡片计数和窗口尺寸。

### 模型和推理控制

可用控制项取决于所选模型。

支持的控制项可能包括：

- Thinking 开启或关闭
- Normal、low、medium、high、xhigh 或 max 推理模式
- 提供商专属的推理强度
- 对于不允许用户选择的模型，使用固定推理模式

切换模型时，Anthro Bridge 会尝试保留最接近的兼容推理设置。如果之前的确切设置不可用，则选择最近的受支持选项；当两个选项距离相同时，优先选择较弱的选项。

### 功能检测

Anthro Bridge 结合了内置功能注册表和实时 OpenRouter 元数据。

功能可能包括：

- 图像输入
- 视频输入
- Thinking 支持
- 推理强度支持
- 已知定价
- 提供商专属的请求翻译规则

实时 OpenRouter 元数据会被缓存，以减少不必要的 API 调用。

### 响应模型名称规范化

上游 API 通常在响应中返回自己的模型名称。Anthro Bridge 可以将该字段重写回客户端所期望的 Anthropic 路由名称。

例如：

```text
Upstream response model: deepseek-v4-pro
Client-visible model:    claude-sonnet-5
```

规范化同时适用于流式和非流式响应，并可在设置中启用或禁用。

### 序列化配置写入

配置变更操作已序列化，以防止并发写入损坏或回滚设置。

这涵盖以下操作：

- 模型变更
- Thinking 模式变更
- 推理强度变更
- OpenRouter 配置文件变更
- 与 API 密钥相关的配置变更

### OpenRouter 保存队列

OpenRouter 路由变更通过专用保存队列处理。

该队列提供：

- 序列化的保存操作
- 淘汰过时的请求
- 请求提交时捕获路由标识
- 防止 React 闭包过时
- 防止之前选择的路由回滚
- 保存成功后自动重试刷新
- 聚合网关重启处理
- 安全处理保存后工作中追加的请求

这可以防止快速模型切换、路由变更或延迟的 Tauri 响应恢复旧的界面值。

### Claude Code 上下文管理

Anthro Bridge 0.16.0 可以生成带模型感知上下文设置的 Claude Code 启动命令。

解析器执行以下步骤：

1. 解析分配给每个规范路由的上游模型：
   - `claude-opus-5`
   - `claude-sonnet-5`
   - `claude-haiku-4-5`
2. 查找每个上游模型的已知上下文容量。
3. 要求三条路由的容量都必须已知。
4. 使用最小容量作为安全的上下文窗口。
5. 应用配置的触发百分比。

例如，如果三条路由解析出的容量分别为 1,000,000、262,144 和 1,000,000 个 token，Anthro Bridge 使用：

```text
window: 262144
trigger override: 90%
estimated trigger point: 235929 tokens
```

生成的 PowerShell 命令使用官方的 Claude Code 变量：

```text
CLAUDE_CODE_AUTO_COMPACT_WINDOW
CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
```

它还包含 Anthro Bridge 网关连接变量：

```text
ANTHROPIC_BASE_URL
ANTHROPIC_AUTH_TOKEN
```

示例：

```powershell
$env:ANTHROPIC_BASE_URL='http://127.0.0.1:4000'; $env:ANTHROPIC_AUTH_TOKEN='sk-local-gateway'; $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW='262144'; $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE='90'; claude
```

当上下文管理被禁用、设置为 Claude Code 默认行为，或因路由容量未知而不完整时，生成的命令会在启动 Claude Code 之前清除过时的上下文变量：

```powershell
Remove-Item Env:CLAUDE_CODE_AUTO_COMPACT_WINDOW -ErrorAction SilentlyContinue;
Remove-Item Env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE -ErrorAction SilentlyContinue;
```

百分比覆盖值会请求更早的主动压缩。Claude Code 可能会忽略那些会导致压缩晚于其自身默认行为的数值。

Anthro Bridge 会验证命令生成和 PowerShell 环境注入。这本身并不能证明某个特定版本的 Claude Code 消费了这些变量；最终确认需要 Claude Code 诊断或观察压缩行为。

### 网关管理

图形界面提供：

- 网关启动和停止控制
- 提供商和配置文件选择
- 路由配置
- API 密钥管理
- 日志查看
- 模型列表刷新
- 保存状态和错误显示

网关监听地址：

```text
http://127.0.0.1:4000
```

## 环境要求

- Windows 10 或 Windows 11
- 开发需要 Node.js 24 或更高版本
- 开发需要稳定的 Rust 工具链
- 至少一个受支持提供商的 API 密钥

一个提供商的密钥即可，不需要每个提供商都拥有密钥。

## 安装

从项目的 Releases 页面下载最新的 Windows 安装程序并运行。

安装程序支持以下语言：

- 英语
- 日语
- 简体中文
- 繁体中文
- 韩语
- 法语
- 德语
- 西班牙语

要更新 Anthro Bridge，运行新版本的安装程序即可。现有用户设置将被保留。

稳定版用户配置存储在：

```text
%APPDATA%\Anthro Bridge\
```

开发版本使用独立的应用标识符和数据目录：

```text
%APPDATA%\Anthro Bridge Dev\
```

这使得稳定版和开发版可以共存，不会共享配置或缓存文件。

## 快速开始

### 1. 配置 API 密钥

打开：

```text
Settings > API Key
```

输入你计划使用的提供商的密钥并保存。

常用环境变量名如下：

| 提供商 | 环境变量 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

OpenRouter 配置文件可以通过图形界面使用配置文件专属的密钥设置。

### 2. 配置路由模型

打开设置，为每条路由选择上游模型：

- Opus
- Sonnet
- Haiku

对于 OpenRouter，先选择或创建一个配置文件，然后在该配置文件中配置每条路由。

### 3. 启动网关

点击 **启动网关**。

验证本地端点是否可用：

```text
GET http://127.0.0.1:4000/health
```

### 4. 通过 Anthro Bridge 启动 Claude Code

打开 Claude 配置面板，点击 **复制 Claude Code 启动命令**。

将生成的命令粘贴到 PowerShell 中。该命令包含：

- `ANTHROPIC_BASE_URL`
- `ANTHROPIC_AUTH_TOKEN`
- 应用上下文管理时的 `CLAUDE_CODE_AUTO_COMPACT_WINDOW`
- 应用上下文管理时的 `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- 未应用上下文管理时清除过时上下文变量的清理命令

该命令会以 Anthro Bridge 作为网关启动 Claude Code，同时保留已配置的模型感知上下文行为。

有关 Claude Desktop 和更多的第三方推理说明，请参阅：

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API 端点

| 方法 | 路径 | 说明 |
|---|---|---|
| `GET` | `/health` | 网关健康检查 |
| `GET` | `/v1/models` | 公开路由模型列表 |
| `POST` | `/v1/messages` | 流式和非流式 Messages API |
| `POST` | `/v1/messages/count_tokens` | Token 计数（所选提供商支持时） |

## 配置

主配置文件为 `config.json`。

大多数设置应通过图形界面更改。手动编辑适用于高级用户。

重要的模型字段包括：

| 键 | 说明 |
|---|---|
| `models.<route>.upstream_model` | 发送给提供商的上游模型名称 |
| `models.<route>.thinking_mode` | 路由专属的 thinking 模式 |
| `models.<route>.reasoning_effort` | 提供商专属的推理强度 |
| `models.<route>.supports_vision` | 图像支持覆盖 |
| `models.<route>.supports_video` | 视频支持覆盖 |
| `models.<route>.visible` | 该路由是否向客户端和仪表板公开 |
| `non_vision_image_policy` | 不支持的图像输入如何处理 |
| `normalize_response_model_identity` | 是否规范化响应模型名称 |
| `claude_code.auto_compact.enabled` | 全局上下文管理开关 |
| `claude_code.auto_compact.trigger_percent` | 请求的主动压缩百分比 |
| `claude_code.auto_compact.mode` | `auto`、`manual` 或 `claude_default` |
| `claude_code.auto_compact.window_tokens` | `manual` 模式下使用的手动上下文窗口 |

不支持的图像可采用以下策略之一处理：

- `replace`：将图像替换为文本占位符
- `drop`：移除图像内容
- `reject`：返回错误

### 上下文管理配置

图形界面只提供全局上下文管理开关。高级数值可以直接在 `config.json` 中编辑。

自动模式：

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

手动模式：

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

Claude Code 默认行为：

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

在 `auto` 模式下，只有当三条规范路由都具有已知的上下文元数据时，Anthro Bridge 才会应用上下文变量。未知的自定义 OpenRouter 模型仍然是有效的路由目标，但在元数据可用或配置手动模式之前，上下文管理会报告不完整状态。

静态模型容量存储在：

```text
gui/src-tauri/resources/model_context_windows.json
```

注册表包含内置预设使用的标准 DeepSeek、MiniMax、Kimi、MiMo、Poolside、Tencent、InclusionAI、StepFun 和 OpenAI GPT-5.6 模型。

## 提供商说明

### DeepSeek

`reasoning_effort`：

- `deepseek-v4-pro`（V4-Pro-0813）
  - Normal: 推理强度禁用
  - Thinking: Low / High / Max
- `deepseek-v4-flash`（V4-Flash-0731）
  - Normal: 推理强度禁用
  - Thinking: Low / High / Max

启动时，DeepSeek V4 Pro 路由中保存的旧 `medium` 或 `xhigh` 强度会迁移为 `high`（与官方有效级别一致）。代理在发送前会规范化强度值（`medium`/`xhigh` → `high`），并使用 `output_config.effort` 格式。

新安装和全新生成的配置的默认 DeepSeek 路由：

- Opus 5 → V4 Flash、Thinking、Max
- Sonnet 5 → V4 Flash、Thinking、High
- Haiku 4.5 → V4 Flash、Thinking、Low

现有已保存的路由不会自动更改。

### MiniMax

MiniMax 模型行为因模型代际而异。Anthro Bridge 应用所选模型所需的请求格式，包括受支持时的自适应或禁用的 thinking。

### Kimi

Kimi 模型根据所属模型系列，可能使用 thinking 参数或固定推理强度模式。Anthro Bridge 将图形界面中的选择翻译为适当的上游请求格式。

### MiMo

MiMo 对受支持的路由使用 `thinking_mode` 而非通用 `thinking` 字段。

各模型对视觉的支持不同。当路由无法接受图像输入时，Anthro Bridge 应用配置的不支持图像策略。

### OpenRouter

OpenRouter 模型在可识别时按供应商分组。图形界面提供：

- 模型搜索
- 供应商分组
- 自定义模型输入
- 功能徽章
- 定价显示
- 每个模型的推理控制
- 统一模型列表刷新

OpenRouter 模型的功能和行为可能随时间变化。实时元数据在可用时优先使用，而内置注册表为已知模型提供稳定的默认值。

内置的 OpenAI GPT-5.6 Balanced 配置文件在新安装和全新生成的配置中，默认在所有路由上使用 Thinking High：

- Opus 5 → GPT-5.6 Sol、Thinking、High
- Sonnet 5 → GPT-5.6 Terra、Thinking、High
- Haiku 4.5 → GPT-5.6 Luna、Thinking、High

现有已保存的路由不会自动更改。

## 用户界面

设置界面包括：

- 可折叠的提供商区域
- Opus、Sonnet 和 Haiku 路由配置
- OpenRouter 的模型搜索和供应商分组
- 基于模型功能的 Thinking 和推理控制
- 自定义上游模型输入
- 自动路由保存
- 显式 API 密钥保存
- 保存进度和错误消息
- 模型定价和功能信息
- 响应模型名称规范化开关
- 标题栏中的 Claude Code 上下文管理开关
- Claude 配置面板中的 Claude Code 启动命令复制操作

仪表板包括：

- 提供商或 OpenRouter 配置文件选择
- 网关状态
- 当前路由映射
- 功能指示器
- 定价信息
- 提供商切换状态

## 开发

### 项目结构

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

### 在开发模式下运行

```bash
cd gui
npm install
npm run tauri dev
```

### 构建开发版本

在 Windows 上，使用单个 Rust 构建任务以避免编译器间歇性终止：

```powershell
cd gui
$env:CARGO_BUILD_JOBS = "1"
npm run tauri:build:dev
Remove-Item Env:CARGO_BUILD_JOBS
```

开发版本使用：

- 窗口标题：`Anthro Bridge (DEV)`
- 端口：`4000`
- 应用标识符：`com.soheidon.anthro-bridge.dev`
- 独立的配置和缓存目录

### 稳定版构建

稳定版构建应仅用于发布准备。常规的实现和验证工作应使用开发版本。

## 验证

前端验证：

```bash
cd gui
npx vitest run
npx tsc --noEmit
```

Rust 验证：

```bash
cd gui/src-tauri
cargo check
cargo test
```

上下文管理验证覆盖：

- 代理与上下文解析器之间共享的路由到上游解析
- 内置直接提供商和 OpenRouter 模型的完整模型上下文元数据
- 三条规范路由间的自动最小窗口选择
- 已应用、禁用、不完整、手动和 Claude 默认模式
- 官方 Claude Code 环境变量名
- PowerShell 命令的渲染和转义
- 网关连接变量
- 在真实的 Windows PowerShell 子进程中注入环境变量
- 未应用上下文管理时移除过时上下文变量
- 前端复制生成的启动命令

针对 OpenRouter 路由选择器的专项测试：

```bash
cd gui
npx vitest run src/components/OpenRouterModelSelector.test.tsx
```

OpenRouter 选择器测试覆盖：

- 队列保存期间捕获的路由标识
- 跨路由回滚保护
- 过时回调保护
- 刷新重试行为
- 刷新失败后的网关重启
- 进行中请求的淘汰
- 基于代际的回滚抑制

可能会添加一个专门针对重启聚合的多保存测试，以锁定以下行为：

```text
save 1 requests restart
save 2 does not request restart
result: restart once after the batch
```

## 手动验证检查表

自动化测试无法再现所有 Tauri 和 React 的时序条件。在发布前，请在开发版本中验证以下内容：

- 每个 OpenRouter 配置文件显示正确的悬停详情
- 模型选择在更改后不会明显回弹
- Thinking 和推理选择在保存后保持稳定
- 关闭并重新打开设置屏幕后设置保持正确
- 重启应用程序后设置保持正确
- 保存期间切换配置文件不会损坏任一配置文件
- 保存失败只回滚发起该操作的路由
- 刷新重试成功会清除之前的错误
- 刷新重试失败会保留最新的错误提示
- 所需的网关重启动在批量操作后只发生一次
- 自定义模型正确保存和重新加载
- 内置和实时 OpenRouter 功能正确显示
- 标题栏的上下文管理开关使用视觉开关并保持其状态
- 每个内置提供商或 OpenRouter 预设都能解析全部三条路由的容量
- 生成的 Claude Code 命令包含网关连接变量
- 启用上下文管理时，生成的命令包含 `CLAUDE_CODE_AUTO_COMPACT_WINDOW` 和 `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE`
- 禁用上下文管理时，生成的命令会移除两个上下文变量
- 复制的命令通过正在运行的 Anthro Bridge 网关启动 Claude Code

## 故障排除

### 端口 4000 已被占用

```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 模型拒绝图像或视频输入

模型功能因提供商和路由而异。请检查图形界面中的功能徽章并选择兼容的路由。

对于不支持的图像输入，Anthro Bridge 遵循 `non_vision_image_policy` 处理。

### 升级后设置被回滚

首先重启应用程序，以便迁移可以运行。

如果问题仍然存在：

1. 备份用户配置。
2. 与捆绑的配置进行对比。
3. 移除过时的字段，或必要时重置用户配置。

稳定版配置位置：

```text
%APPDATA%\Anthro Bridge\config.json
```

开发版配置位置：

```text
%APPDATA%\Anthro Bridge Dev\config.json
```

### OpenRouter 模型列表已过时

使用设置中的统一模型刷新控件。Anthro Bridge 会缓存模型元数据，因此当 OpenRouter 更改了模型条目后，可能需要手动刷新。

### 上下文管理不完整

自动上下文管理要求三条规范路由的容量都已知。

请检查 Opus、Sonnet 和 Haiku 的已配置上游模型。自定义或新发布的模型可能尚未存在于 `model_context_windows.json` 中。

选项：

1. 选择具有已知元数据的内置模型。
2. 向静态注册表添加已验证的模型元数据。
3. 在 `config.json` 中使用手动模式。
4. 使用 `claude_default` 将压缩完全交由 Claude Code 处理。

### Claude Code 未使用预期的上下文设置

请确认 Claude Code 是从生成的 PowerShell 命令启动的，而不是从单独的终端命令启动。

在同一 PowerShell 会话中检查：

```powershell
echo $env:CLAUDE_CODE_AUTO_COMPACT_WINDOW
echo $env:CLAUDE_AUTOCOMPACT_PCT_OVERRIDE
echo $env:ANTHROPIC_BASE_URL
echo $env:ANTHROPIC_AUTH_TOKEN
```

这些值可以确认启动环境已经准备就绪。它们不能证明 Claude Code 消费了这些变量。最终确认请使用 Claude Code 诊断或观察压缩行为。

## 翻译

英文源文件为源 README。

翻译后的 README 文件存储在 `docs/` 目录下。当英文 README 发生更改时，应从英文源文件重新生成或更新翻译文件，而不是独立编辑每种语言。

应用程序界面的语言文件存储在：

```text
gui/src/i18n/lang/
```

## 许可证

MIT 许可证。详见 [LICENSE](../LICENSE)。
