[English](ANTIGRAVITY_MCP.md) | [日本語](ANTIGRAVITY_MCP.ja.md) | [中文(简体)](ANTIGRAVITY_MCP.zh-CN.md) | [中文(繁體)](ANTIGRAVITY_MCP.zh-TW.md) | [한국어](ANTIGRAVITY_MCP.ko.md) | [Français](ANTIGRAVITY_MCP.fr.md) | [Deutsch](ANTIGRAVITY_MCP.de.md) | [Español](ANTIGRAVITY_MCP.es.md)

[← Back to Anthro Bridge README](../README.md)

# Using Anthro Bridge MCP with Google Antigravity

Anthro Bridge does not require a separate MCP server executable. The installed `anthro-bridge.exe` provides both the desktop application and the MCP server. Antigravity starts the MCP mode by launching the same executable with `--mcp-server`.

```text
Normal launch
anthro-bridge.exe
→ Anthro Bridge desktop app / 3P Gateway

MCP launch
anthro-bridge.exe --mcp-server
→ headless stdio MCP server for Antigravity
```

This allows agentic environments such as Google Antigravity to delegate architectural and implementation planning to external LLMs (e.g., DeepSeek V4, MiMo, Kimi, MiniMax, or OpenRouter models) via `anthro-bridge/plan`, while performing the actual token-intensive code edits, terminal commands, builds, and tests using Antigravity's subscription-backed model allocation.

---

## 1. What This Workflow Does

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Configured external planner model
    ↓
Structured implementation plan returned
    ↓
Antigravity executes edits, builds, and tests
using subscription-backed capacity
```

- **External API**: Responsible only for generating the implementation plan based on the relevant repository context. Billed by the respective provider.
- **Antigravity Subscription**: Performs the heavy file-reading, code-editing, tool-calling, and test-running loops.
- **Separation of Concerns**: You get high-quality external reasoning for architecture and plans without exhausting external API tokens on routine code generation.

---

## 2. Requirements
 
1. **Anthro Bridge** installed on Windows.
2. An **API key** configured for the provider you want to use for planning.
3. **Google Antigravity** installed and running.
 
---
 
## 3. Configure the MCP Server in Antigravity
 
1. Open Google Antigravity.
2. Navigate to:
   ```text
   Settings → Customizations → Installed MCP Servers → Open MCP Config
   ```
3. Add the `anthro-bridge` server configuration to your `mcpServers` object using the installed `anthro-bridge.exe` and the `--mcp-server` argument:
 
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
 
For development builds, you can point directly to the dev executable:
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
> You do **not** need to put API keys in plain text in the MCP config. The MCP server automatically reads API keys from your existing Windows user environment variables (e.g., `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY`) or from the Anthro Bridge configuration.

---

## 4. Verify the MCP Connection

In Antigravity's **Installed MCP Servers** view, confirm that `anthro-bridge` is recognized:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Configure the Planner Model in Anthro Bridge

1. Open the **Anthro Bridge** desktop app.
2. Select the **MCP** tab at the top.
3. Choose the active planner **Provider** or **Profile** (e.g., DeepSeek, MiMo, or OpenRouter).
4. Click **Settings** (or open the MCP Plan Settings accordion) to configure:
   - **Model**
   - **Thinking Mode**
   - **Reasoning Effort**
5. Save your settings.

> [!NOTE]
> The Anthro Bridge MCP server reads the current configuration dynamically on every `plan()` tool invocation. You do **not** need to restart the MCP server or Antigravity when changing planner providers or models in the GUI.

---

## 6. Using the Plan Tool Manually

You can ask Antigravity directly in chat to invoke the planner:

```text
Inspect this project, then use the anthro-bridge/plan MCP tool to create an implementation plan. Do not implement yet.
```

Antigravity will inspect relevant files, extract key architecture context, call `anthro-bridge/plan`, and present the resulting plan for your review.

---

## 7. Automate Planning with an Antigravity Workspace Rule

You can automate planner invocation for non-trivial coding tasks by creating a workspace rule file at [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) (or similar rule file in `.agents/rules/`):

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

With this rule active, Antigravity will automatically use the Anthro Bridge planner whenever a complex coding or debugging request arrives.

---

## 8. Typical Automated Workflow

```text
User: "Refactor feature X to support multiple profiles."
    ↓
Antigravity reads files and summarizes the codebase context
    ↓
Antigravity triggers anthro-bridge/plan tool call
    ↓
Anthro Bridge sends the prompt to the selected external model
    ↓
Antigravity receives the structured implementation plan
    ↓
User reviews/approves plan
    ↓
Antigravity modifies files, executes tests, and verifies changes
```

---

## 9. Important Notes

- **Independent Operation**: The MCP server operates independently of the Anthro Bridge 3P Gateway. The 3P Gateway does not need to be running for MCP calls to work.
- **Separate Billing**: Calls to `anthro-bridge/plan` incur external API costs billed by the chosen provider. Subsequent file editing, tool execution, and testing use Antigravity's subscription capacity.
- **Dynamic Configuration**: Switching the active MCP provider, profile, or model parameters in the Anthro Bridge GUI takes effect immediately on the next `plan()` invocation.
