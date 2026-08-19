[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← 返回 Anthro Bridge README](README.zh-CN.md)

# 在 Google Antigravity 中使用 Anthro Bridge MCP

Anthro Bridge 内置了一个 Model Context Protocol (MCP) 服务器，提供专门的 `plan` 规划工具（`anthro-bridge/plan`）。这使得像 Google Antigravity 这样的智能体编码环境能够将架构设计和实现规划委托给外部大语言模型（如 DeepSeek V4、MiMo、Kimi、MiniMax 或 OpenRouter 上的模型），同时使用 Antigravity 订阅所包含的模型额度执行高 token 消耗的文件编辑、命令运行、构建和测试。

---

## 1. 该工作流的运作方式

```text
Antigravity
    ↓
代码库探索 (检查相关文件并提取上下文)
    ↓
anthro-bridge / plan (携带任务、上下文和约束发起 MCP 调用)
    ↓
Anthro Bridge MCP 服务器
    ↓
外部规划器模型 (在 Anthro Bridge GUI 中配置)
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

## 2. 前提条件

1. 在 Windows 上安装了 **Anthro Bridge**。
2. 已构建或可获取 **`anthro-bridge-mcp-server.exe`**（例如位于 `mcp-server/target/release/anthro-bridge-mcp-server.exe`）。
3. 已为计划使用的规划器模型配置了 **API 密钥**。
4. **Google Antigravity** 已安装并运行。

---

## 3. 在 Antigravity 中配置 MCP 服务器

1. 打开 Google Antigravity。
2. 导航至：
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. 将 `anthro-bridge` 服务器配置添加到 `mcpServers` 对象中：

```json
{
  "mcpServers": {
    "anthro-bridge": {
      "command": "C:\\Users\\<USER>\\path\\to\\anthro-bridge\\mcp-server\\target\\release\\anthro-bridge-mcp-server.exe"
    }
  }
}
```

> [!TIP]
> 无需在 MCP 配置文件中以明文写入 API 密钥。MCP 服务器会自动读取 Windows 用户环境变量（如 `DEEPSEEK_API_KEY`、`OPENROUTER_API_KEY`、`MOONSHOT_API_KEY`、`MINIMAX_API_KEY`、`XIAOMI_API_KEY`）或 Anthro Bridge 中保存的配置。

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

1. 打开 **Anthro Bridge** 桌面客户端。
2. 选择顶部的 **MCP** 选项卡。
3. 选择当前生效的规划器 **提供商 (Provider)** 或 **配置文件 (Profile)**（如 DeepSeek、MiMo、OpenRouter 等）。
4. 打开 **设置 (Settings)**（或 MCP Plan 详细设置），配置以下参数：
   - **模型 (Model)**
   - **思考模式 (Thinking Mode)**
   - **推理强度 (Reasoning Effort)**
5. 保存配置。

> [!NOTE]
> Anthro Bridge MCP 服务器在每次调用 `plan()` 工具时都会动态读取当前配置。在 GUI 中更改规划器提供商或模型时，**无需**重启 MCP 服务器或 Antigravity。

---

## 6. 手动调用 plan 工具

您可以在 Antigravity 聊天中直接要求智能体调用规划器：

```text
请调查这个项目，然后使用 anthro-bridge/plan MCP 工具制定实现计划。先不要开始实现。
```

Antigravity 将探索相关文件、总结关键架构上下文、调用 `anthro-bridge/plan` 并展示生成的实现计划供您审阅。

---

## 7. 使用 Workspace Rule 自动化规划调用

您可以通过在 [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) 创建工作区规则文件，在进行复杂代码更改时自动触发规划器：

```markdown
---
trigger: model_decision
description: Use for implementation, debugging, architectural changes, or multi-file code changes where an external implementation plan would be useful. Do not use for trivial text-only edits.
---

# Planner Rule

For non-trivial implementation tasks in this repository:

1. First inspect the repository yourself and identify the files and code relevant to the task.
2. Summarize only the context necessary for implementation planning.
3. Call the `anthro-bridge/plan` MCP tool exactly once with:
   - the user's task;
   - the relevant repository context;
   - important constraints.
4. Use the returned plan as the basis for implementation.
5. Perform file edits, builds, and tests yourself.
6. Do not call `anthro-bridge/plan` again unless the implementation encounters a major unresolved problem.
7. Do not ask the user to repeat this workflow.
8. Do not use the planner for trivial tasks such as a one-word text change unless planning would materially help.
```

---

## 8. 典型自动化工作流

```text
用户：“重构功能 X 以支持多配置文件。”
    ↓
Antigravity 探索代码并总结上下文
    ↓
Antigravity 自动触发 anthro-bridge/plan 工具调用
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

- **独立运行**：MCP 服务器独立于 Anthro Bridge 3P Gateway 运行。无需启动 3P Gateway 即可使用 MCP 工具。
- **账单分离**：调用 `anthro-bridge/plan` 会产生由相应提供商收取的 API 费用。随后的文件编辑和测试消耗 Antigravity 自身的订阅额度。
- **实时生效**：在 Anthro Bridge GUI 中更改规划器设置将在下一次 `plan()` 调用时立即生效。
