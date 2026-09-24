[English](README.md) | [日本語](docs/README.ja.md) | [中文(简体)](docs/README.zh-CN.md) | [中文(繁體)](docs/README.zh-TW.md) | [한국어](docs/README.ko.md) | [Français](docs/README.fr.md) | [Deutsch](docs/README.de.md) | [Español](docs/README.es.md)

# Anthro Bridge

**Use Claude Code / Claude Desktop as the coding harness, route inference to third-party LLM APIs, and use external models as planners and reviewers for Google Antigravity.**

Anthro Bridge is a Windows companion application for AI-assisted software development. It supports three core workflows:

1. **3P Gateway for Claude Code / Claude Desktop** — Keep Claude's repository exploration, tool use, file editing, and test execution while routing inference to third-party cloud providers.
2. **Dedicated Local LLMs for Claude Code (Ollama)** — Run Claude Code completely offline with zero API costs using local Ollama models.
3. **MCP Planner & Reviewer for Google Antigravity** — Keep Claude's repository exploration, tool use, file editing, and test execution while routing inference to third-party providers.
2. **MCP Planner & Reviewer for Google Antigravity** — Delegate implementation planning and post-implementation review to external models via the `anthro-bridge/plan` and `anthro-bridge/review` MCP tools.

---

## Three Main Workflows

### 1. Claude Code / Claude Desktop with 3P Gateway

```text
Claude Code / Claude Desktop
             ↓
  Anthro Bridge 3P Gateway
             ↓
DeepSeek / Kimi Code / OpenRouter / MiniMax / MiMo
```

- **Harness & Model Separation**: Keep Claude's agentic tooling while routing inference to third-party providers.
- **Dynamic Multi-Profile Routing**: Switch active providers, OpenRouter profiles, and model routes from the GUI.
- **Setup Guide**: [Claude Desktop / Cowork 3P Gateway Setup](docs/THIRD_PARTY_INFERENCE.md)

### 2. Claude Code with Dedicated Local LLMs (Ollama)

```text
Claude Code CLI
      ↓ (Loopback ANTHROPIC_BASE_URL)
Ollama Local (127.0.0.1:11434/v1)
      ↓
Gemma 4 / Qwen / Llama 3 / DeepSeek-R1 (Local On-Device)
```

- **Free, Offline & Private**: Run Claude Code without API keys, usage limits, or cloud data transfer.
- **Dedicated Routing & Model Discovery**: Switch between Gateway and Ollama (`active_route: "ollama"`) with automatic model discovery from `/api/tags`.
- **Reasoning & Context Tuning**: Full support for thinking token budgets (1024 tokens) and context window overrides (`num_ctx`) per alias.
- **Total Isolation**: Claude Desktop, Cowork on 3P, and Google Antigravity MCP continue to use cloud models independently.
- **Setup Guide**: [Ollama Local Setup Guide](docs/OLLAMA_LOCAL.md)

### 3. Antigravity with MCP Planner & Reviewer

```text
Antigravity
    ↓ stdio
anthro-bridge.exe --mcp-server
    ↓
Configured external model (planner / reviewer)
    ↓
Implementation plan / Review verdict
    ↓
Antigravity implements and tests
using subscription-backed capacity
```

- **Planning vs. Execution Split**: External models generate the high-level plan or review verdict; Antigravity subscription capacity executes token-intensive code edits.
- **Live GUI Configuration**: Switching the planner or reviewer provider, model, or reasoning effort takes effect immediately on the next invocation.
- **Setup Guide**: [Google Antigravity + Anthro Bridge MCP Setup](docs/ANTIGRAVITY_MCP.md)

**Global Antigravity commands:**

- **`/anthro-plan`** — Delegate implementation planning to the configured external model.
- **`/anthro-revise`** — Revise an existing plan based on new feedback or constraints.
- **`/anthro-review`** — Review a completed implementation against the approved plan before commit, with explicit READY / NOT READY verdicts.

**Recommended workflow:**

```text
/anthro-plan → Implementation & Tests → /anthro-review → Commit
```

---

## Supported Providers

| Provider | Connection | Supported Families | Reasoning Controls |
|---|---|---|---|
| **DeepSeek** | Direct API | DeepSeek V4.1 Flash, V4 Pro 0813 | Normal / Low / High / Max |
| **Kimi Code** | Direct API | kimi-for-coding, kimi-for-coding-highspeed | Thinking mode |
| **MiniMax** | Direct API | MiniMax M3, M2.7 | Model-specific |
| **Kimi / Moonshot** | Direct API | Kimi K2.x, Kimi K3 | Thinking / Reasoning effort |
| **MiMo / Xiaomi** | Direct API | MiMo V2.6 Flash, Pro, Pro-UltraSpeed (V2.5 backward-compatible) | Normal / Thinking |
| **Ollama Local** | Local Loopback | Gemma 4, Qwen 2.5, Llama 3.3, DeepSeek-R1 | Thinking (1024 budget) / Disabled |
| **OpenRouter** | Multi-profile Gateway | See OpenRouter section below | Model-specific / Profile-specific |

### DeepSeek (Direct)

Built-in **Direct DeepSeek** preset routes Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low.

- `deepseek-v4.1-flash` — Current flagship reasoner ($0.27 / 1M input · $1.10 / 1M output).
- `deepseek-v4-pro-0813` — High-quality baseline without extended reasoning ($0.27 / 1M input · $1.10 / 1M output).

### Kimi Code (Direct)

Dedicated coding-specialist API (`KIMI_CODE_API_KEY`), separate from Moonshot Kimi:

- `kimi-for-coding` — Full-quality coding model.
- `kimi-for-coding-highspeed` — Low-latency variant.

### MiMo / Xiaomi (Direct)

Built-in **Direct MiMo** preset routes:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- Default model: `mimo-v2.6-flash`

Models: `mimo-v2.6-flash`, `mimo-v2.6-pro`, `mimo-v2.6-pro-ultraspeed` (selectable). All three support a 1M-token context window and native multimodal capabilities (text, image, video).

**Normal / Thinking**: MiMo uses a simple Normal/Thinking toggle — no reasoning effort levels.

**V2.5 backward compatibility**: Saved `mimo-v2.5`, `mimo-v2.5-pro`, and `mimo-v2.5-pro-ultraspeed` routes are preserved and continue to function. Untouched legacy defaults migrate to V2.6 automatically on startup.

### Dedicated Local LLM (Ollama Local)

Dedicated local inference backend for **Claude Code CLI**:
- Endpoint: `http://127.0.0.1:11434/v1` (native Anthropic messages loopback)
- Dynamic discovery of installed local models via loopback `/api/tags`
- Thinking budget support (1024 tokens) and context window (`num_ctx`) overrides per alias
- See [Ollama Local Guide](docs/OLLAMA_LOCAL.md) for detailed configuration

### OpenRouter

Supports multiple named profiles. Full OpenAI model catalog (single dropdown):

| Model ID | Display Name |
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

**GPT-6 Astra**: 1.05M context · reasoning effort: `low / medium / high / xhigh / max`.
**GPT-6 Astra Pro**: 1.05M context · always-on Pro reasoning (`reasoning.mode = pro`), no user-selectable effort.
**GPT Astra Latest**: alias tracking the latest Astra family model.

Built-in **OpenRouter: chatGPT** preset: Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium.

Also available: **OpenRouter: Gemini** (Gemini 3.8 Flash · reasoning effort `low / medium / high`), **OpenRouter: Poolside**, **OpenRouter: Tencent**, **OpenRouter: InclusionAI**, **OpenRouter: StepFun**.

---

## Model Pricing (as of v0.23.0)

| Model | Input | Output |
|---|---|---|
| DeepSeek V4.1 Flash | \$0.27 / 1M | \$1.10 / 1M |
| DeepSeek V4 Pro 0813 | \$0.27 / 1M | \$1.10 / 1M |
| GPT-6 Astra / Astra Pro / Astra Latest | \$10 / 1M | \$50 / 1M |
| GPT-5.6 Sol / Terra / Luna | \$5 / 1M | \$25 / 1M |
| GPT-5.6 Sol Pro / Terra Pro / Luna Pro | \$5 / 1M | \$25 / 1M |
| Gemini 3.8 Flash (OpenRouter) | \$0.75 / 1M | \$3.75 / 1M |
| MiMo-V2.6-Flash | \$0.14 / 1M | \$0.28 / 1M |
| MiMo-V2.6-Pro | \$0.435 / 1M | \$0.87 / 1M |
| MiMo-V2.6-Pro-UltraSpeed | \$4.35 / 1M | \$8.70 / 1M |

---

## Installation

Download the latest Windows installer (`Anthro Bridge_0.23.0_x64-setup.exe`) from the [Releases](https://github.com/soheidon/anthro-bridge/releases) page and run it.

The installer supports 8 languages and preserves existing user settings during upgrades.

---

## Quick Start

### Workflow 1: 3P Gateway for Claude Code / Claude Desktop

1. Open Anthro Bridge **Settings > API Key** and configure an API key for your desired provider.
2. Select your provider or OpenRouter profile on the dashboard.
3. Click **Start Gateway** (runs on `http://127.0.0.1:4000`).
4. Connect Claude Code or Claude Desktop:
   - **Claude Code**: Click **Copy Claude Code launch command** in Settings and paste it into PowerShell.
   - **Claude Desktop / Cowork**: Follow the [Claude Desktop 3P Setup Guide](docs/THIRD_PARTY_INFERENCE.md).

### Workflow 2: Dedicated Local LLMs for Claude Code (Ollama)

1. Ensure Ollama is running locally (`http://127.0.0.1:11434`).
2. Select **Ollama Local** on the Anthro Bridge Dashboard (or configure models in **Settings > Claude Code / Local LLMs**).
3. Click **Copy Claude Code launch command** and paste it into PowerShell.
4. Claude Code connects directly to your local Ollama instance with full thinking and context window support.

### Workflow 3: MCP Planner & Reviewer for Google Antigravity

1. Configure an API key for your chosen planner/reviewer model in Anthro Bridge.
2. Select the **MCP** tab and configure your model in **Settings > Antigravity > MCP Plan Settings**.
3. Register `anthro-bridge.exe` with `["--mcp-server"]` in Antigravity's MCP configuration (or click **Configure Automatically** in Anthro Bridge).
4. Use `/anthro-plan` to design plans, `/anthro-revise` to update plans, and `/anthro-review` to review implementations before commit.
5. Follow the complete [Antigravity MCP Setup Guide](docs/ANTIGRAVITY_MCP.md).

---

## API Keys

| Provider | Environment Variable |
|---|---|
| DeepSeek | `DEEPSEEK_API_KEY` |
| Kimi Code | `KIMI_CODE_API_KEY` |
| Kimi / Moonshot | `MOONSHOT_API_KEY` |
| MiniMax | `MINIMAX_API_KEY` |
| MiMo / Xiaomi | `XIAOMI_API_KEY` |
| OpenRouter | `OPENROUTER_API_KEY` |

---

## Documentation

- [Claude Desktop / Cowork 3P Gateway Setup](docs/THIRD_PARTY_INFERENCE.md)
- [Ollama Local Setup & CLI Configuration](docs/OLLAMA_LOCAL.md)
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
Ensure an API key is set for the provider selected under the **MCP** tab, or exported in your Windows user environment variables (e.g., `DEEPSEEK_API_KEY`, `OPENROUTER_API_KEY`). The 3P Gateway does not need to be running for MCP.

---

## License

MIT License. See [LICENSE](LICENSE).
