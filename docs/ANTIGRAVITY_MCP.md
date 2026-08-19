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
2. Provider authentication configured in Anthro Bridge or system environment variables for the provider you want to use for planning.
3. **Google Antigravity** installed and running.
 
---
 
## 3. Configure the MCP Server in Antigravity

### Method 1 — GUI Configuration via Anthro Bridge (Recommended)

1. Open Anthro Bridge and go to **Settings** (`[Settings]` tab) > **Antigravity** in the left sub-navigation.
2. In the **Antigravity Integration** card:
   - **Target Executable**: By default, the currently running `anthro-bridge.exe` path is shown. If you wish to use a different binary (e.g. portable or custom build), click **Change** (`antigravity.btnChangeExe`) and select the executable.
   - **Register / Update**: Click **Update Antigravity Configuration** (`antigravity.btnUpdate`) to safely register or update the `anthro-bridge` entry in `%USERPROFILE%\.gemini\config\mcp_config.json`. All other MCP server configurations in that file remain preserved.
   - **Remove**: Click **Remove Configuration** (`antigravity.btnRemove`) if you wish to unregister the server from Antigravity.
   - **Open Folder**: Click **Open Settings Folder** (`antigravity.btnOpenFolder`) to inspect the directory in Windows Explorer.

---

### Method 2 — Manual Configuration (Advanced)

1. In Anthro Bridge **Settings > Antigravity**, click **Open Settings Folder** to open `%USERPROFILE%\.gemini\config\` in Windows Explorer.
2. Open or create `mcp_config.json` and add the `anthro-bridge` entry under `mcpServers`:

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
 
For development builds, point directly to the release build executable:
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
> You do **not** need to write provider API keys into Antigravity's `mcp_config.json`. The MCP server leverages Anthro Bridge's credential resolution (reading from Windows user environment variables like `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`, `MOONSHOT_API_KEY`, `MINIMAX_API_KEY`, `XIAOMI_API_KEY`, or saved application settings).

---

## 4. Verify the MCP Connection

In Antigravity's **Installed MCP Servers** view, confirm that `anthro-bridge` is recognized:

```text
anthro-bridge
  1 tool enabled
  - plan
```

---

## 5. Configure Planner Models in Anthro Bridge

Anthro Bridge clearly separates planner selection from detailed parameter management:

1. **Top-Level `MCP` Tab (`MCP for Antigravity`)**:
   - Displays available providers (DeepSeek, OpenRouter, MiniMax, MiMo, Kimi) and profiles.
   - Click a provider card to switch the active planner destination immediately.
2. **`Settings` > `Antigravity`**:
   - **MCP Plan Settings** card: Configure model selection, Thinking mode, and Reasoning Effort per provider/profile.
   - **Antigravity Integration** card: Manage MCP server registration and Antigravity Commands (Global Skills).

> [!NOTE]
> The Anthro Bridge MCP server reads current configuration dynamically on every `plan()` tool invocation. You do **not** need to restart the MCP server or Antigravity when changing planner providers or model settings in the GUI.

---

## 6. Antigravity Commands (`/anthro-plan` & `/anthro-revise`) (Recommended)

From **Settings > Antigravity > Antigravity Integration**, you can install Global Skills to use slash commands across all Antigravity workspaces:

- Click **Install All Commands** (`antigravity.btnInstallAll`) or click **Install** (`antigravity.commandBtnInstall`) next to individual commands.

### Create a new implementation plan:
```text
/anthro-plan <description of the task or feature>
```
*Gathers repository context, invokes `anthro-bridge/plan`, and stops cleanly after presenting the plan without making code edits or running build commands.*

### Revise an existing implementation plan:
```text
/anthro-revise <feedback or updated requirements to incorporate>
```
*Identifies the current implementation plan (from active context or `implementation_plan.md`), passes the plan and feedback to `anthro-bridge/plan`, and updates the plan while preserving unaffected sections.*

> [!IMPORTANT]
> When executing through `/anthro-plan` or `/anthro-revise`, the command workflow owns the single planner call. Workspace rules will not trigger additional duplicate planner calls.

---

## 7. Automating Planning via Workspace Rules

Place a workspace rule like [`.agents/rules/deepseek-planner.md`](../.agents/rules/deepseek-planner.md) in your project to automate external planner invocation for complex tasks:

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

### Triggering Policy:
- **Trivial / localized tasks** (e.g., fixing a typo, one-line edits, small syntax adjustments): Do not trigger the planner.
- **Non-trivial tasks** (architectural changes, multi-file features, complex debugging): Antigravity inspects repository context, invokes `anthro-bridge/plan` once, and executes the implementation based on the returned plan.

---

## 8. Typical Automated Workflow

```text
User: "Refactor feature X to support multiple profiles."
    ↓
Antigravity reads files and summarizes the codebase context
    ↓
Antigravity triggers anthro-bridge/plan tool call (exactly once)
    ↓
Anthro Bridge sends prompt to configured external model
    ↓
Antigravity receives structured implementation plan
    ↓
User reviews/approves plan
    ↓
Antigravity modifies files, executes tests, and verifies changes
```

---

## 9. Important Notes

- **Independent Operation**: The MCP server operates completely independently of the Anthro Bridge 3P Gateway. The 3P Gateway does not need to be running for MCP calls to work.
- **Separate Billing**: Calls to `anthro-bridge/plan` incur external API costs billed by the chosen provider. Subsequent file editing, tool execution, and testing use Antigravity's subscription capacity.
- **Dynamic Configuration**: Switching the active MCP provider, profile, or model parameters in the Anthro Bridge GUI takes effect immediately on the next `plan()` invocation.

