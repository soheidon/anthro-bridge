[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

Anthro Bridge 是一个本地网关和桌面配置工具，使 Claude Desktop 和 Claude Code 能够通过兼容 Anthropic 的 API 使用多个第三方大模型提供商。

本应用程序包含：

- 一个用 Rust 编写的本地代理服务器
- 一个基于 Tauri 2、React 和 TypeScript 构建的原生 Windows 图形界面
- 从 Anthropic 模型名称映射到各提供商上游模型的基于模型的路由
- 针对每条路由的模型、推理和功能配置

Anthro Bridge 是一个独立项目，不是 Moon Bridge 的分支、前端或配套应用。

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
| Tencent Hy3 | 是 | Low 和 High 推理力度 |
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
Claude Code 请求
  model: claude-sonnet-5

Anthro Bridge 路由
  提供商: OpenRouter 配置文件 "Hy3"
  上游模型: tencent/hunyuan-a13b-instruct
  推理模式: high
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

配置文件可以在图形界面中添加、重命名、删除和选择。

内置的 OpenRouter 供应商组目前包括 Poolside、Tencent、InclusionAI、StepFun 以及其他已识别的模型系列。未识别的模型仍可通过搜索或自定义模型输入使用。

### 模型和推理控制

可用控制项取决于所选模型。

支持的控制项可能包括：

- Thinking 开启或关闭
- Normal、low、medium、high、xhigh 或 max 推理模式
- 提供商专属的推理力度
- 对于不允许用户选择的模型，使用固定推理模式

切换模型时，Anthro Bridge 会尝试保留最接近的兼容推理设置。如果之前的确切设置不可用，则选择最近的受支持选项，当两个选项距离相同时优先选择较弱的选项。

### 功能检测

Anthro Bridge 结合了内置功能注册表和实时 OpenRouter 元数据。

功能可能包括：

- 图像输入
- 视频输入
- Thinking 支持
- 推理力度支持
- 已知定价
- 提供商专属的请求翻译规则

实时 OpenRouter 元数据会被缓存，以减少不必要的 API 调用。

### 响应模型名称规范化

上游 API 通常在响应中返回自己的模型名称。Anthro Bridge 可以将该字段重写回客户端所期望的 Anthropic 路由名称。

例如：

```text
上游响应模型:   deepseek-v4-pro
客户端可见模型:  claude-sonnet-5
```

规范化同时适用于流式和非流式响应，并可在设置中启用或禁用。

### 序列化配置写入

配置变更操作已序列化，以防止并发写入损坏或回滚设置。

这涵盖以下操作：

- 模型变更
- Thinking 模式变更
- 推理力度变更
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

- English
- 日本語
- 中文(简体)
- 中文(繁體)
- 한국어
- Français
- Deutsch
- Español

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

点击 **Start Gateway**。

验证本地端点是否可用：

```text
GET http://127.0.0.1:4000/health
```

### 4. 配置 Claude Desktop 或 Claude Code

将客户端指向 Anthro Bridge 端点，同时继续使用 Anthropic 模型名称。

详细的第三方推理说明请参阅：

```text
docs/THIRD_PARTY_INFERENCE.md
```

## API 端点

| Method | Path | 说明 |
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
| `models.<route>.reasoning_effort` | 提供商专属的推理力度 |
| `models.<route>.supports_vision` | 图像支持覆盖 |
| `models.<route>.supports_video` | 视频支持覆盖 |
| `models.<route>.visible` | 该路由是否向客户端和仪表板公开 |
| `non_vision_image_policy` | 不支持的图像输入如何处理 |
| `normalize_response_model_identity` | 是否规范化响应模型名称 |

不支持的图像可采用以下策略之一处理：

- `replace`：将图像替换为文本占位符
- `drop`：移除图像内容
- `reject`：返回错误

## 提供商说明

### DeepSeek

DeepSeek Pro 模型可使用可配置的推理力度。Flash 模型不开放相同的推理力度控制，因此不可用选项会被自动禁用。

### MiniMax

MiniMax 模型行为因模型代际而异。Anthro Bridge 应用所选模型所需的请求格式，包括受支持时的自适应或禁用的 thinking。

### Kimi

Kimi 模型根据所属模型系列，可能使用 thinking 参数或固定推理力度模式。Anthro Bridge 将图形界面中的选择翻译为适当的上游请求格式。

### MiMo

MiMo 对受支持的路由使用 `thinking_mode` 而非通用 `thinking` 字段。

各模型对视觉的支持不同。当路由无法接受图像输入时，Anthro Bridge 应用配置的「不支持图像的策略」。

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
│   │   │   └── paths.rs
│   │   └── resources/
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
```

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
save 1 请求重启
save 2 不请求重启
结果: 批量完成后重启一次
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
- 刷新重试成功清除之前的错误
- 刷新重试失败保留最新的错误提示
- 所需的网关重启动在批量操作后只发生一次
- 自定义模型正确保存和重新加载
- 内置和实时 OpenRouter 功能正确显示

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

## 翻译

英文源文件为源 README。

翻译后的 README 文件存储在 `docs/` 目录下。当英文 README 发生更改时，应从英文源文件重新生成或更新翻译文件，而不是独立编辑每种语言。

应用程序界面的语言文件存储在：

```text
gui/src/i18n/lang/
```

## 许可证

MIT 许可证。详见 [LICENSE](../LICENSE)。
