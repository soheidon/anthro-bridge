[English](README.md) | [日本語](docs/README.ja.md) | [中文(简体)](docs/README.zh-CN.md) | [中文(繁體)](docs/README.zh-TW.md) | [韓国語](docs/README.ko.md) | [Français](docs/README.fr.md) | [Deutsch](docs/README.de.md) | [Español](docs/README.es.md)

# Anthro Bridge

**Use Claude Code Desktop as the coding harness, route implementation to third-party APIs, and use external models as planners for Antigravity.**

Anthro Bridge is a Windows companion application for AI-assisted software development. It supports two main workflows: routing Claude-compatible coding sessions to third-party APIs through a local 3P Gateway, and using external models as planners for Google Antigravity through MCP.

---

## Two Main Workflows

### 1. Claude Code / Claude Desktop with 3P Gateway

Keep using Claude Code Desktop and Claude Desktop as the agentic coding harness, while routing underlying model requests to third-party LLM APIs that Anthropic clients do not natively support.

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / MiniMax / Kimi / MiMo / OpenRouter
```

- **Harness & Model Separation**: Keep Claude's repository exploration, tool use, file editing, and test execution while routing inference to third-party providers.
- **Dynamic Multi-Profile Routing**: Switch active providers or OpenRouter profiles and customize Opus, Sonnet, and Haiku model routes from the GUI.
- **Setup Guide**: [Claude Desktop / Cowork 3P Gateway Setup](docs/THIRD_PARTY_INFERENCE.md)

### 2. Antigravity with MCP Planner

Delegate implementation planning and architecture design to external models via the Anthro Bridge MCP `plan` tool (`anthro-bridge/plan`), while executing the actual multi-file edits and terminal commands using Antigravity's subscription-backed model allocation.

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Configured external planner API
    ↓
Implementation plan
    ↓
Antigravity implements and tests
using subscription-backed capacity
```

- **Planning vs. Execution Split**: External models generate the high-level plan; Antigravity subscription capacity executes the token-intensive code edits and test loops.
- **Live GUI Configuration**: Switching the planner provider, model, or reasoning effort in Anthro Bridge takes effect immediately on the next `plan()` invocation.
- **Setup Guide**: [Google Antigravity + Anthro Bridge MCP Setup](docs/ANTIGRAVITY_MCP.md)

#### Antigravity Planner Workflow

Anthro Bridge separates repository/context discovery from implementation planning.

In the Antigravity workflow, Antigravity first inspects the repository, relevant files, UI state, screenshots, and other available context. It then sends the distilled task and context to Anthro Bridge through the `anthro-bridge/plan` MCP tool.

The external planner model is responsible for producing the implementation plan from that prepared context. Anthro Bridge MCP planning is therefore currently text-based: image attachments are interpreted by Antigravity before the planning context is sent to the external planner.

`deepseek-v4-flash-vision-exp` may be selected as an MCP planner model, but its vision capability is not currently used directly through the MCP planning pipeline. In MCP mode, it operates as a text-based planner model.

Direct image input is supported separately through the Anthro Bridge Gateway for models whose capabilities allow Base64 or image URL content.

---

## Supported Providers

| Provider | Connection Type | Supported Families | Reasoning Controls |
|---|---|---|---|
| **DeepSeek** | Direct API | DeepSeek V4 Pro, V4 Flash, V4 Flash Vision Exp | Normal / Low / High / Max |
| **MiniMax** | Direct API | MiniMax M3, M2.7 | Model-specific |
| **Kimi / Moonshot** | Direct API | Kimi K2.x, Kimi K3 | Thinking / Reasoning effort |
| **MiMo / Xiaomi** | Direct API | MiMo V2.5, V2.5 Pro | Thinking mode |
| **OpenRouter** | Multi-profile Gateway | Poolside, Tencent, InclusionAI, StepFun, OpenAI GPT-5.6, Google Gemini, etc. | Model-specific / Profile-specific |

> **Note on `deepseek-v4-flash-vision-exp`**: Supports direct image input through the Gateway (Base64 / image URL). In the Antigravity MCP planner workflow, it is currently used as a text-based planner model.

---

## Installation

Download the latest Windows installer (`Anthro Bridge_x.x.x_x64-setup.exe`) from the [Releases](https://github.com/soheidon/anthro-bridge/releases) page and run it.

The installer supports 8 languages (English, Japanese, Simplified Chinese, Traditional Chinese, Korean, French, German, Spanish) and preserves existing user settings during upgrades.

---

## Quick Start

### Workflow 1: 3P Gateway for Claude Code / Claude Desktop

1. Open Anthro Bridge **Settings > API Key** and configure an API key for your desired provider.
2. Select your provider or OpenRouter profile on the dashboard.
3. Click **Start Gateway** (runs on `http://127.0.0.1:4000`).
4. Connect Claude Code or Claude Desktop:
   - **Claude Code**: Click **Copy Claude Code launch command** in Settings and paste it into PowerShell.
   - **Claude Desktop / Cowork**: Follow the [Claude Desktop 3P Setup Guide](docs/THIRD_PARTY_INFERENCE.md).

### Workflow 2: MCP Planner for Google Antigravity

1. Configure an API key for your chosen planner model in Anthro Bridge.
2. Select the **MCP** tab in Anthro Bridge and configure your planner model in **Settings > MCP Plan Settings**.
3. Register `anthro-bridge.exe` with `["--mcp-server"]` in Antigravity's MCP configuration.
4. Invoke `anthro-bridge/plan` in Antigravity (or automate it with a workspace rule).
5. Follow the complete [Antigravity MCP Setup Guide](docs/ANTIGRAVITY_MCP.md).

---

## Documentation

- [Claude Desktop / Cowork 3P Gateway Setup](docs/THIRD_PARTY_INFERENCE.md)
- [Google Antigravity + Anthro Bridge MCP Setup](docs/ANTIGRAVITY_MCP.md)
- [Configuration Reference (`config.json`)](docs/CONFIGURATION.md)
- [Provider Details & Reasoning Controls](docs/PROVIDERS.md)
- [Development & Verification Guide](docs/DEVELOPMENT.md)

---

## Troubleshooting

### Port 4000 Is Already in Use
```powershell
netstat -ano | findstr :4000
taskkill /PID <PID> /F
```

### Settings Revert After an Upgrade
Restart the application so migrations can run. Configuration is stored under `%APPDATA%\Anthro Bridge\config.json`.

### MCP Planner Calls Fail
Ensure an API key is set for the provider selected under the **MCP** tab in Anthro Bridge, or exported in your Windows user environment variables (e.g., `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`). The 3P Gateway does not need to be running for MCP.

---

## License

MIT License. See [LICENSE](LICENSE).
