[English](../README.md) | [日本語](README.ja.md) | [中文(简体)](README.zh-CN.md) | [中文(繁體)](README.zh-TW.md) | [한국어](README.ko.md) | [Français](README.fr.md) | [Deutsch](README.de.md) | [Español](README.es.md)

# Anthro Bridge

**以 Claude Code / Claude Desktop 作为编程框架，将推理请求路由至第三方 LLM API，并将外部模型用作 Google Antigravity 的规划器与审查器。**

Anthro Bridge 是一款面向 AI 辅助软件开发的 Windows 配套应用程序，支持两种互补的工作流：

1. **面向 Claude Code / Claude Desktop 的第三方推理网关** — 保留 Claude 的代码仓库探索、工具调用、文件编辑和测试执行能力，同时将推理请求路由至第三方提供商。
2. **面向 Google Antigravity 的 MCP 规划器与审查器** — 通过 `anthro-bridge/plan` 和 `anthro-bridge/review` MCP 工具，将实现规划与实现后审查委托给外部模型。

---

## 两大主要工作流

### 1. Claude Code / Claude Desktop 配合第三方推理网关

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **框架与模型分离**：保留 Claude 的代理工具能力，同时将推理请求路由至第三方提供商。
- **动态多配置文件路由**：通过图形界面动态切换活跃提供商、OpenRouter 配置文件和模型路由。
- **配置指南**：[Claude Desktop / Cowork 第三方推理网关配置](THIRD_PARTY_INFERENCE.md)

### 2. Claude Code 专用本地 LLM（Ollama）

```text
Claude Code CLI
      ↓ (Loopback ANTHROPIC_BASE_URL)
Ollama Local (127.0.0.1:11434/v1)
      ↓
Gemma 4 / Qwen / Llama 3 / DeepSeek-R1（本地设备端）
```

- **完全免费、离线与隐私**: 无需 API 密钥，无用量限制，无云端数据传输。
- **独立路由与模型发现**: 独立的 Claude Code 路由（`active_route: "ollama"`），支持 `/api/tags` 本地模型自动发现。
- **思考模式与上下文微调**: 支持为每个别名配置思考预算（1024 标记）与上下文窗口（`num_ctx`）。
- **完全隔离**: Claude Desktop、Cowork on 3P 和 Google Antigravity MCP 继续独立使用云端模型。
- **配置指南**: [Ollama Local 指南](OLLAMA_LOCAL.zh-CN.md)

### 3. Antigravity + MCP 规划器 / 审查器 配合 MCP 规划器与审查器

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
已配置的外部模型（规划器 / 审查器）
    ↓
实现计划 / 审查结论
    ↓
Antigravity 使用订阅容量
执行实现与测试
```

- **规划与执行分离**：外部模型负责生成高层次计划或审查结论；Antigravity 订阅容量负责执行耗费大量 token 的代码编辑任务。
- **实时图形界面配置**：切换规划器或审查器的提供商、模型或推理强度，下次调用时立即生效。
- **配置指南**：[Google Antigravity + Anthro Bridge MCP 配置](ANTIGRAVITY_MCP.zh-CN.md)

**Antigravity 全局命令：**

- **`/anthro-plan`** — 将实现规划委托给已配置的外部模型。
- **`/anthro-revise`** — 根据新的反馈或约束条件修订现有计划。
- **`/anthro-review`** — 在提交前，针对已批准的计划审查已完成的实现，并给出明确的 READY / NOT READY 结论。

**推荐工作流：**

```text
/anthro-plan → 实现与测试 → /anthro-review → 提交
```

---

## 支持的提供商

| 提供商 | 连接方式 | 支持的系列 | 推理强度控制 |
|---|---|---|---|
| **DeepSeek** | 直连 API | DeepSeek V4.1 Flash、V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | 直连 API | kimi-for-coding、kimi-for-coding-highspeed | 思考模式 |
| **MiniMax** | 直连 API | MiniMax M3、M2.7 | 模型专属 |
| **Kimi / Moonshot** | 直连 API | Kimi K2.x、Kimi K3 | Thinking / 推理强度 |
| **MiMo / Xiaomi** | 直连 API | MiMo V2.6 Flash、Pro、Pro-UltraSpeed（兼容 V2.5） | Normal / Thinking |
| **OpenRouter** | 多配置文件网关 | 请参见下方 OpenRouter 章节 | 模型专属 / 配置文件专属 |

### DeepSeek（直连）

内置 **Direct DeepSeek** 预设路由：Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low。

- `deepseek-v4.1-flash` — 当前旗舰推理模型（输入 $0.27 / 1M · 输出 $1.10 / 1M）。
- `deepseek-v4-pro-0813` — 不含扩展推理的高质量基线模型（输入 $0.27 / 1M · 输出 $1.10 / 1M）。

### Kimi Code（直连）

专用编程专家 API（`KIMI_CODE_API_KEY`），与 Moonshot Kimi 独立分开：

- `kimi-for-coding` — 完整质量的编程模型。
- `kimi-for-coding-highspeed` — 低延迟变体。

### MiMo / Xiaomi（直连）

内置 **Direct MiMo** 预设路由：
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- 默认模型：`mimo-v2.6-flash`

模型：`mimo-v2.6-flash`、`mimo-v2.6-pro`、`mimo-v2.6-pro-ultraspeed`（可选）。三者均支持 100 万 token 上下文窗口及原生多模态能力（文本、图像、视频）。

**Normal / Thinking**：MiMo 采用简单的 Normal/Thinking 切换 —— 不支持推理强度级别。

**V2.5 向下兼容**：已保存的 `mimo-v2.5`、`mimo-v2.5-pro`、`mimo-v2.5-pro-ultraspeed` 路由将继续正常运行。未修改的旧版默认配置将在启动时自动迁移至 V2.6。

### OpenRouter

支持多个命名配置文件。完整 OpenAI 模型目录（单一下拉列表）：

| 模型 ID | 显示名称 |
|---|---|
| `openai/gpt-6-astra` | GPT-6 Astra |
| `openai/gpt-6-astra-pro` | GPT-6 Astra Pro |
| `openai/gpt-astra-latest` | GPT Astra Latest |
| `openai/gpt-5.6-sol` | GPT-5.6 Sol |
| `openai/gpt-5.6-sol-pro` | GPT-5.6 Sol Pro |
| `openai/gpt-5.6-terra` | GPT-5.6 Terra |
| `openai/gpt-5.6-terra-pro` | GPT-5.6 Terra Pro |
| `openai/gpt-5.6-luna` | GPT-5.6 Luna |
| `openai/gpt-5.6-luna-pro` | GPT-5.6 Luna Pro |

**GPT-6 Astra**：105 万上下文 · 推理强度：`low / medium / high / xhigh / max`。
**GPT-6 Astra Pro**：105 万上下文 · 始终开启 Pro 推理（`reasoning.mode = pro`），不支持用户自选强度。
**GPT Astra Latest**：追踪 Astra 系列最新模型的别名。

内置 **OpenRouter: chatGPT** 预设：Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium。

同时支持：**OpenRouter: Gemini**（Gemini 3.8 Flash · 推理强度 `low / medium / high`）、**OpenRouter: Poolside**、**OpenRouter: Tencent**、**OpenRouter: InclusionAI**、**OpenRouter: StepFun**。

---

## 模型定价（截至 v0.23.0）

| 模型 | 输入 | 输出 |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash（OpenRouter） | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## 安装

从 [Releases](https://github.com/soheidon/anthro-bridge/releases) 页面下载最新的 Windows 安装程序（`Anthro Bridge_0.23.0_x64-setup.exe`）并运行。

安装程序支持 8 种语言，升级时会保留现有的用户设置。

---

## 快速开始

### 工作流 1：面向 Claude Code / Claude Desktop 的第三方推理网关

1. 打开 Anthro Bridge **Settings > API Key**，为所需提供商配置 API 密钥。
2. 在仪表盘上选择提供商或 OpenRouter 配置文件。
3. 点击 **Start Gateway**（运行于 `http://127.0.0.1:4000`）。
4. 连接 Claude Code 或 Claude Desktop：
   - **Claude Code**：在 Settings 中点击 **Copy Claude Code launch command**，将命令粘贴到 PowerShell 中执行。
   - **Claude Desktop / Cowork**：请参照 [Claude Desktop 第三方推理配置指南](THIRD_PARTY_INFERENCE.md)。

### 工作流 2：面向 Google Antigravity 的 MCP 规划器与审查器

1. 在 Anthro Bridge 中为所选规划器/审查器模型配置 API 密钥。
2. 选择 **MCP** 标签页，并在 **Settings > Antigravity > MCP Plan Settings** 中配置模型。
3. 在 Antigravity 的 MCP 配置中注册 `anthro-bridge.exe`，并附加参数 `["--mcp-server"]`（或在 Anthro Bridge 中点击 **Configure Automatically**）。
4. 使用 `/anthro-plan` 制定计划，使用 `/anthro-revise` 更新计划，使用 `/anthro-review` 在提交前审查实现。
5. 请参阅完整的 [Antigravity MCP 配置指南](ANTIGRAVITY_MCP.zh-CN.md)。

---

## API 密钥

| 提供商 | 环境变量 |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## 文档

- [Claude Desktop / Cowork 第三方推理网关配置](THIRD_PARTY_INFERENCE.md)
- [Google Antigravity + Anthro Bridge MCP 配置](ANTIGRAVITY_MCP.zh-CN.md)
- [配置参考（`config.json`）](CONFIGURATION.md)
- [提供商详情与推理强度控制](PROVIDERS.md)
- [开发与验证指南](DEVELOPMENT.md)

---

## 故障排查

### 端口 4000 已被占用
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### 升级后设置被还原
重新启动应用程序以执行迁移。配置文件存储于 `%APPDATA%\Anthro Bridge\config.json`。

### MCP 规划器调用失败
请确保已为 **MCP** 标签页中所选提供商设置 API 密钥，或已在 Windows 用户环境变量中导出相应密钥（例如 `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`）。MCP 功能无需运行第三方推理网关。

---

## 许可证

MIT 许可证。详见 [LICENSE](../LICENSE)。
