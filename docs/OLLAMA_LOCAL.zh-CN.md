[English](OLLAMA_LOCAL.md) | [日本語](OLLAMA_LOCAL.ja.md) | 中文(简体) | [中文(繁體)](OLLAMA_LOCAL.zh-TW.md) | [한국어](OLLAMA_LOCAL.ko.md) | [Français](OLLAMA_LOCAL.fr.md) | [Deutsch](OLLAMA_LOCAL.de.md) | [Español](OLLAMA_LOCAL.es.md)

[← 返回 Anthro Bridge README](../README.md)

# Claude Code + Ollama Local

Anthro Bridge 能够将 Claude Code 路由到本地运行的 Ollama 模型，同时保持 Claude Desktop 和 MCP 使用的云端提供商不变。

---

## 架构

```text
Claude Code
     │
     │ Anthro Bridge Claude Code 标识标记 (X-Anthro-Bridge-Client: claude-code)
     ▼
Anthro Bridge Gateway
     │
     ├─ Gateway 路由 (`active_route = "gateway"`) ──→ 全局云端提供商 (DeepSeek / MiMo / OpenRouter 等)
     │
     └─ Ollama 路由  (`active_route = "ollama"`)  ──→ http://127.0.0.1:11434 (`/v1/messages`)
                                                          │
                                                          ▼
                                                      本地模型
```

Ollama Local 专为 **Claude Code** 设计。它不会替换 Claude Desktop 或 Anthro Bridge MCP 工具使用的全局云端提供商。

---

## 环境要求

1. **Anthro Bridge** 已安装并运行。
2. **Ollama** 已在本地计算机上安装并运行。
3. Ollama 中已拉取至少一个模型（如 `gemma4:latest`、`llama3.3:70b`、`qwen2.5-coder:32b`），或输入自定义模型标签。
4. 使用 Anthro Bridge 生成的启动命令启动 Claude Code。

---

## 配置 Ollama Local

打开 Anthro Bridge：
1. 前往 **设置 > API 密钥**。
2. 在 API 密钥表格下方找到 **Ollama Local** 设置卡片。

### 主设置行

- **模型**: 选择已发现的 Ollama 模型或自定义模型。
- **刷新 (`🔄 刷新`)**: 查询本地 Ollama 的 `/api/tags` 端点以加载已安装的模型。
- **Thinking**: 选择 **常规 (禁用)** 或 **Thinking (启用)**。
- **在仪表板显示**: 控制 Ollama Local 是否作为可选卡片显示在仪表板上。

### 高级设置 (`▸ 高级设置`)

- **端点**: Ollama 的基础 URL。默认值为 `http://127.0.0.1:11434`。
- **视觉 (Base64)**: 启用或禁用多模态本地模型的图像输入（默认：禁用）。
- **上下文窗口**: 可选的显式上下文 Token 容量（例如 `131072`）。
- **无需 API 密钥**: 本地环回 Ollama 实例无需 API 密钥。

---

## 模型发现与刷新

点击 **刷新 (`🔄 刷新`)** 会向以下地址发送请求：

```http
GET http://127.0.0.1:11434/api/tags
```

### 安全与配置保留规则

- **仅限本地环回**: 仅查询本地环回地址（`127.0.0.1`、`localhost`、`[::1]`）。
- **非阻塞错误处理**: 若 Ollama 未运行或请求超时（2 秒上限），界面会显示提示，已保存的模型配置**绝不会被清空**。
- **自定义模型保留**: 如果已保存的模型不在返回列表中，下拉列表中仍会保持其选中状态。
- **自定义模型标签**: 选择 `+ 自定义模型标签...` 可手动输入任意模型标识符。

---

## 在仪表板上选择 Ollama

1. 在设置中启用 **在仪表板显示**。
2. 前往 **仪表板**。
3. 点击 **Ollama Local** 卡片：
   - 设置 `claude_code.active_route = "ollama"`。
   - 全局 `active_provider`（如 `deepseek`）**保持不变**。
   - Ollama 卡片高亮显示为 **Claude Code 使用中**。
4. 若要切回云端提供商路由：
   - 点击仪表板上的任意云端提供商卡片（如 DeepSeek 或 MiMo）。
   - Claude Code 自动重置为 `claude_code.active_route = "gateway"`。

---

## Thinking 行为规范

Anthro Bridge 将 Thinking 选择转换为 Ollama 所需的 Anthropic 兼容格式：

- **常规 (Normal)**:
  ```json
  "thinking": { "type": "disabled" }
  ```
- **Thinking (启用)**:
  ```json
  "thinking": { "type": "enabled" }
  ```

*注意：Ollama Local 不提供 reasoning effort 等级选择。*

---

## 上下文窗口与自动压缩

- `context_window` 为**可选**参数。
- 指定后，Anthro Bridge 将其用于 Claude Code 上下文管理与自动压缩（auto-compact）计算。
- 省略（`null`）时，Anthro Bridge 不会擅自推测上下文大小，而是采用默认规则。

---

## Claude Code 识别标记

Anthro Bridge 生成的 Claude Code 启动命令通过 `ANTHROPIC_CUSTOM_HEADERS` 注入内部标识标记：

```text
X-Anthro-Bridge-Client: claude-code
```

- **请求头规范化**: 不区分大小写规范化头部，合并或替换旧标记，并保留用户的其它自定义请求头。
- **向上游转发前剥离**: Anthro Bridge 仅在本地将其用于路由判断，在向上游提供商或 Ollama 转发请求前会将其彻底剥离。

---

## 客户端完全隔离

| 客户端 | 路由选择器 | 目标 |
| :--- | :--- | :--- |
| **Claude Code** | `claude_code.active_route` (`"gateway"` 或 `"ollama"`) | 仪表板选中的提供商 / Ollama Local |
| **Claude Desktop** | `active_provider` (全局云端提供商) | 云端网关 (DeepSeek, MiMo, OpenRouter 等) |
| **Antigravity MCP** | `mcp.provider` / `mcp.profile_id` | 专用的 MCP 规划与审查提供商 |

此设计确保将 Claude Code 切换至 Ollama Local 时，绝不会影响 Claude Desktop 或 Antigravity MCP 的工作流。

---

## 疑难解答

### 刷新无法找到 Ollama
1. 确认 Ollama 正在后台或终端运行：
   ```powershell
   ollama list
   ```
2. 确认高级设置中的端点为 `http://127.0.0.1:11434`。

### 已保存的模型未在列表中列出
即使 `/api/tags` 未返回该模型，Anthro Bridge 也会保留已保存的配置。您也可以通过 `+ 自定义模型标签...` 手动输入。

### Claude Code 仍在使用云端提供商
1. 检查仪表板，确认 **Ollama Local** 卡片处于激活状态。
2. 确保使用 Anthro Bridge 生成的启动命令（`复制 Claude Code 启动命令`）启动 Claude Code。

### Claude Desktop 仍在使用云端提供商
此为预期行为。Ollama Local 仅供 Claude Code 使用。
