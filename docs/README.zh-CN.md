[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**将 Claude Code Desktop 用作编码框架，将实现路由到第三方 API，并将外部模型用作 Antigravity 的规划器。**

Anthro Bridge 是一个用于 AI 辅助软件开发的 Windows 配套应用程序，围绕两个主要工作流构建：

1. **Claude Code / Claude Desktop + 第三方网关 (3P Gateway)**：继续使用 Claude Code Desktop 作为编码框架，同时通过本地 Anthropic 兼容网关将模型请求路由到第三方 LLM API（DeepSeek、MiMo、MiniMax、Kimi 和 OpenRouter）。
2. **Antigravity + MCP 规划器 (MCP Planner)**：通过 Anthro Bridge MCP `plan` 工具（`anthro-bridge/plan`）将架构设计和实现规划委托给外部模型，同时使用 Antigravity 订阅内包含的模型额度执行文件编辑和测试。

---

## 两个主要工作流

### 1. Claude Code / Claude Desktop + 3P Gateway

继续使用 Claude Code Desktop 和 Claude Desktop 作为智能体编码框架，同时将底层模型请求路由到 Anthropic 客户端原生不支持的第三方 LLM API。

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **框架与模型分离**：保留 Claude 的仓库探索、工具使用、文件编辑和测试执行能力，同时将推理路由到第三方提供商。
- **动态多配置文件路由**：在 GUI 仪表板中随时切换当前提供商或 OpenRouter 配置文件，并在设置中自定义 Opus、Sonnet 和 Haiku 路线。
- **设置指南**：[Claude Desktop / Cowork 3P Gateway 设置指南](THIRD_PARTY_INFERENCE.zh-CN.md)

### 2. Antigravity + MCP Planner

通过 Anthro Bridge MCP `plan` 工具（`anthro-bridge/plan`）将实现规划和架构设计委托给外部模型，同时使用 Antigravity 的订阅模型额度执行实际的文件编辑和终端命令。

```text
Antigravity
    ↓
代码库探索 (收集上下文)
    ↓
anthro-bridge / plan (MCP)
    ↓
Anthro Bridge MCP 服务器
    ↓
配置的外部大语言模型
    ↓
结构化实现计划
    ↓
Antigravity 使用订阅额度
执行编辑、构建与测试
```

- **规划与执行分离**：外部模型生成高层计划；Antigravity 订阅额度执行高 token 消耗的代码编辑和测试循环。
- **实时 GUI 配置**：在 Anthro Bridge 中切换规划器提供商、模型或推理强度时，会在下一次 `plan()` 调用时立即生效，无需重启 Antigravity。
- **设置指南**：[Google Antigravity + Anthro Bridge MCP 设置指南](ANTIGRAVITY_MCP.zh-CN.md)

---

## 支持的提供商

| 提供商 | 连接类型 | 支持的模型系列 | 推理控制 |
|---|---|---|---|
| **DeepSeek** | 直接 API | DeepSeek V4 Pro, V4 Flash | Normal / Low / High / Max |
| **MiniMax** | 直接 API | MiniMax M3, M2.7 | 特定模型支持 |
| **Kimi / Moonshot** | 直接 API | Kimi K2.x, Kimi K3 | 思考 / 推理强度 |
| **MiMo / Xiaomi** | 直接 API | MiMo V2.5, V2.5 Pro | 思考模式 |
| **OpenRouter** | 多配置文件网关 | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini 等 | 特定模型 / 特定配置文件 |

---

## 安装

从 [Releases](https://github.com/soheidon/anthro-bridge/releases) 页面下载最新的 Windows 安装程序（`Anthro Bridge_x.x.x_x64-setup.exe`）并运行。

安装程序支持 8 种语言（英语、日语、简体中文、繁体中文、韩语、法语、德语、西班牙语），并在升级时保留现有的用户设置。

---

## 快速上手

### 工作流 1：适用于 Claude Code / Claude Desktop 的 3P Gateway

1. 打开 Anthro Bridge **设置 > API Key** 并配置所需提供商的 API 密钥。
2. 在仪表板上选择提供商或 OpenRouter 配置文件。
3. 点击 **启动网关 (Start Gateway)**（监听 `http://127.0.0.1:4000`）。
4. 连接 Claude Code 或 Claude Desktop：
   - **Claude Code**：在设置中点击 **复制 Claude Code 启动命令** 并粘贴到 PowerShell 中运行。
   - **Claude Desktop / Cowork**：参考 [Claude Desktop 3P 设置指南](THIRD_PARTY_INFERENCE.zh-CN.md)。

### 工作流 2：适用于 Google Antigravity 的 MCP Planner

1. 在 Anthro Bridge 中为您选择的规划器模型配置 API 密钥。
2. 选择 Anthro Bridge 的 **MCP** 选项卡，并在 **设置 > MCP Plan 详细设置** 中配置规划器模型和推理参数。
3. 在 Antigravity 的 MCP 配置中注册 `anthro-bridge-mcp-server.exe`。
4. 在 Antigravity 中调用 `anthro-bridge/plan`（或通过 Workspace Rule 自动化）。
5. 详细步骤请参阅 [Google Antigravity + Anthro Bridge MCP 设置指南](ANTIGRAVITY_MCP.zh-CN.md)。

---

## 文档

- [Claude Desktop / Cowork 3P Gateway 设置指南](THIRD_PARTY_INFERENCE.zh-CN.md)
- [Google Antigravity + Anthro Bridge MCP 设置指南](ANTIGRAVITY_MCP.zh-CN.md)
- [配置参考 (`config.json`)](CONFIGURATION.md)
- [提供商详情与模型行为](PROVIDERS.md)
- [开发与验证指南](DEVELOPMENT.md)

---

## 故障排除

### 端口 4000 已被占用
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 升级后设置还原
重启应用程序以运行配置迁移。配置文件保存在 `%APPDATA%\Anthro Bridge\config.json`。

### MCP Planner 调用失败
请确保 Anthro Bridge 的 **MCP** 选项卡中选择的提供商已配置 API 密钥，或已在 Windows 用户环境变量中设置（例如 `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`）。MCP 不需要运行 3P Gateway。

---

## 许可证

MIT License。详见 [LICENSE](../LICENSE)。
