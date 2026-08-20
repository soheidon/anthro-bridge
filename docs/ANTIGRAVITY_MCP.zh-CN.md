[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← 返回 Anthro Bridge README](README.zh-CN.md)

# 在 Google Antigravity 中使用 Anthro Bridge MCP

Anthro Bridge 不需要单独的 MCP 服务器可执行文件。安装的单一 `anthro-bridge.exe` 同时提供桌面 GUI 应用程序和 MCP 服务器功能。Antigravity 通过附带 `--mcp-server` 参数启动同一可执行文件来进入 MCP 模式。

```text
普通启动
anthro-bridge.exe
→ Anthro Bridge 桌面应用 / 3P Gateway

MCP启动
anthro-bridge.exe --mcp-server
→ 面向 Antigravity 的无头 stdio MCP 服务器
```

这使得 Google Antigravity 等智能体编码环境能够将架构设计和实现规划委托给外部大语言模型（如 DeepSeek V4、MiMo、Kimi、MiniMax 或 OpenRouter 模型）通过 `anthro-bridge/plan` 完成，而实际的高 token 消耗代码编辑、命令执行、构建和测试则使用 Antigravity 订阅所包含的模型额度。

---

## 1. 该工作流的运作方式

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
配置的外部规划器模型
    ↓
返回结构化实现计划
    ↓
Antigravity 使用订阅额度
执行编辑、构建与测试
```

- **外部 API**：仅负责根据相关代码库上下文生成实现计划（由相应提供商按量计费）。
- **Antigravity 订阅**：负责重度的文件读取、代码编辑、工具调用和测试运行循环。
- **职责分离**：享受高智能外部模型规划带来的优势，而不会在常规代码编写中消耗昂贵的外部 API token。

---

## 2. 前置要求

1. 在 Windows 上安装 **Anthro Bridge**。
2. 为想要用于规划的提供商配置认证信息（在 Anthro Bridge 内配置或设置系统环境变量）。
3. 安装并运行 **Google Antigravity**。

---

## 3. 在 Antigravity 中配置 MCP 服务器

### 方法 1 — 通过 Anthro Bridge GUI 配置（推荐）

1. 打开 Anthro Bridge，进入顶部导航的 **设置**（`[设置]` 选项卡）> 左侧子导航的 **Antigravity**。
2. 查看 **Google Antigravity 集成** 卡片：
   - **目标可执行文件**：默认显示当前正在运行的 `anthro-bridge.exe` 路径。如果想使用其他二进制文件（如便携版或自定义构建版），点击 **更改** (`antigravity.btnChangeExe`) 选择可执行文件。
   - **注册 / 更新**：点击 **更新 Antigravity 配置** (`antigravity.btnUpdate`)，即可在保留 `%USERPROFILE%\.gemini\config\mcp_config.json` 中其他 MCP 服务器配置的前提下，安全地注册或更新 `anthro-bridge`。
   - **移除注册**：如需从 Antigravity 中移除，点击 **移除配置** (`antigravity.btnRemove`)。
   - **查看配置文件夹**：点击 **打开设置文件夹** (`antigravity.btnOpenFolder`) 可直接在 Windows 资源管理器中打开该目录。

---

### 方法 2 — 手动配置（高级）

1. 在 Anthro Bridge **设置 > Antigravity** 中点击 **打开设置文件夹**，在 Windows 资源管理器中打开 `%USERPROFILE%\.gemini\config\`。
2. 打开 `mcp_config.json`，在 `mcpServers` 对象中添加 `anthro-bridge` 配置：

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\AppData\\Local\\Anthro Bridge\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

开发构建版本可直接指向 Release 路径：
```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\gui\\src-tauri\\target\\release\\anthro-bridge.exe",
      "args": ["--mcp-server"]
    }
  }
}
```

> [!TIP]
> 无需在 Antigravity 的 `mcp_config.json` 中写入提供商 API 密钥。MCP 服务器会利用 Anthro Bridge 现有的凭据解析机制（从 Windows 用户环境变量如 `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY` 或已保存的应用设置中自动读取）。

---

## 4. 验证 MCP 连接

在 Antigravity 的 **Installed MCP Servers** 界面中确认 `anthro-bridge` 已被识别：

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. 在 Anthro Bridge 中配置规划器模型

Anthro Bridge 明确划分了规划器选择与详细参数管理的职责：

1. **顶层 `MCP` 选项卡 (`MCP for Antigravity`)**：
   - 显示可用提供商（DeepSeek、OpenRouter、MiniMax、MiMo、Kimi）和配置文件的卡片列表。
   - 点击卡片即可立即切换生效的规划器目标。
2. **`设置` > `Antigravity`**：
   - **MCP Plan 详细设置** 卡片：按提供商/配置文件详细配置所选模型、思考模式 (Thinking Mode) 及推理强度 (Reasoning Effort)。
   - **Google Antigravity 集成** 卡片：管理 MCP 服务器注册状态以及 Antigravity Commands（全局技能）。

> [!NOTE]
> Anthro Bridge MCP 服务器在每次调用 `plan()` 工具时都会动态读取当前配置。在 GUI 中更改规划器提供商或模型参数时，**无需**重启 MCP 服务器或 Antigravity。

---

## 6. 使用 Antigravity Commands (`/anthro-plan` & `/anthro-revise`)（推荐）

在 **设置 > Antigravity** 的 **Google Antigravity 集成** 卡片中安装全局技能后，可在所有 Antigravity 工作区中使用斜杠命令：

- 点击 **全部安装** (`antigravity.btnInstallAll`) 或点击命令旁的 **安装** (`antigravity.commandBtnInstall`)。

### 创建新实现计划:
```text
/anthro-plan <要实现的课题或功能描述>
```
*收集代码库上下文，调用 `anthro-bridge/plan`，在展示计划后安全停止，不进行文件修改或运行构建命令。*

### 修订现有实现计划:
```text
/anthro-revise <要反映的反馈或变更需求>
```
*从活动上下文或 `implementation_plan.md` 中确定当前实现计划，连同反馈一起传递给 `anthro-bridge/plan` 进行修订，同时保留未受影响的部分。*

> [!IMPORTANT]
> 通过 `/anthro-plan` 或 `/anthro-revise` 执行时，由命令自身管理单次 planner 调用，Workspace Rule 不会触发额外的重复 planner 调用。

---

## 7. 通过 Workspace Rule 自动化计划流程

可以在项目中放置如 [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) 的 Workspace Rule，在复杂编码任务时自动化调用外部规划器：

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# DeepSeek Planner Rule

For non-trivial implementation tasks in this repository:

1. If the current task is being executed through the `/anthro-plan` or `/anthro-revise` command, do NOT invoke `anthro-bridge/plan` separately. The command workflow owns the planner call.
2. First inspect the repository yourself and identify the files and code relevant to the task.
3. Summarize only the context necessary for implementation planning.
4. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
   Note: "Exactly once" means duplicate planner calls are prohibited once a successful usable result is obtained. If the tool call itself fails or returns an unusable response (e.g. transport or decoding error), exactly 1 recovery retry is permitted.
5. Use the returned DeepSeek plan as the basis for implementation.
6. Perform file edits, builds, and tests yourself.
7. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
8. Do not ask the user to repeat this workflow.
9. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

### 触发策略:
- **微小 / 局部任务 (Trivial / localized tasks)**（如修改错别字、单行微调、语法调整）：不会触发规划器。
- **非普通任务 (Non-trivial tasks)**（架构设计变更、跨多文件功能实现、复杂调试等）：Antigravity 会调查代码库上下文，调用 1 次 `anthro-bridge/plan`，并根据返回的计划进行实现。

---

## 8. 典型自动化工作流

```text
用户：“重构功能 X 以支持多配置文件。”
    ↓
Antigravity 探索代码并总结上下文
    ↓
Antigravity 自动触发 anthro-bridge/plan 工具调用 (仅1次)
    ↓
Anthro Bridge 向选定的外部模型发送请求
    ↓
Antigravity 接收结构化实现计划
    ↓
用户审阅并批准计划
    ↓
Antigravity 执行代码编辑、运行测试并验证修改
```

---

## 9. 重要说明

- **独立运行**：MCP 服务器完全独立于 Anthro Bridge 3P Gateway 运行。无需启动（开启）3P Gateway 即可使用 MCP 工具。
- **账单分离**：调用 `anthro-bridge/plan` 会产生由相应提供商收取的 API 费用。随后的文件编辑和测试消耗 Antigravity 自身的订阅额度。
- **实时生效**：在 Anthro Bridge GUI 中更改规划器提供商或模型参数将在下一次 `plan()` 调用时立即生效。


