# Anthro Bridge — Release Notes

## Unreleased

### Human-Gated Orchestrator Workflow

- Added a Human-Gated development workflow that separates planning, plan
  integration and review, implementation, validation, code review, and final
  human approval.
- Added Antigravity harness profile wiring for plan integration, implementation,
  and fixes, with configurable planner and reviewer profiles.
- Added explicit human approval, change-request, and abort decisions at the
  final review gate. Change requests return the run to the fix, validation, and
  code-review stages.
- Added a localhost mailbox for Antigravity workers, with authenticated task
  claims, progress reporting, result submission, and lease/epoch checks to
  reject stale worker requests.

### Lean Antigravity Mode and Dispatch Budgets

- Added a Dashboard toggle for Lean Antigravity Mode. When enabled, the
  Orchestrator adds bounded execution guidance intended to reduce unnecessary
  exploration and repeated verification while leaving the selected model
  unchanged.
- Added per-task and per-run Antigravity dispatch limits. When a limit is
  reached, the run pauses for user action instead of being reported as a
  workflow failure.
- The Dashboard reports which dispatch budget was reached and displays its
  actual count and limit (for example, task `2 / 2` or run `6 / 6`).

### Human-Gated Preset and Role Assignment Safety

- Added the Human-Gated workflow preset with Antigravity harness assignments
  for plan integration, implementation, and fixes, plus a Codex CLI code
  reviewer assignment.
- Preset application validates profile references across the complete proposed
  assignment map, including assignments for roles inactive in the selected
  workflow, and fails without saving when a referenced profile is unavailable.
- Preset-application errors use localized UI text rather than exposing raw
  resolver messages.

## v0.23.0 — 2026-09-25

### Dedicated Ollama Local Support for Claude Code

Anthro Bridge now provides first-class support for local LLMs via Ollama, dedicated exclusively to the Claude Code CLI workflow:

- **Dedicated Routing Architecture**: Claude Code CLI routing is separated from global provider settings via `claude_code.active_route` (`"gateway"` vs `"ollama"`).
- **Zero Cloud Leakage**: Claude Code communicates directly with the local Ollama loopback endpoint (`http://127.0.0.1:11434/v1`) using standard Anthropic messages compatibility. No API keys or cloud connections required.
- **Dynamic Local Model Discovery**: Loopback-only `GET /api/tags` discovery dynamically populates installed local models in the GUI with a 2-second bounded timeout and failure-safe caching.
- **Thinking Mode & Context Overrides**: Control reasoning budget (1024 tokens) and context window (`num_ctx`) per canonical alias (`opus`, `sonnet`, `haiku`) or globally.
- **Dashboard Quick-Switching**: Easily toggle between cloud providers and Ollama Local directly from the Dashboard tiles, with a dedicated "Show on Dashboard" visibility preference.
- **Complete Isolation**: Claude Desktop, Cowork on 3P, and Google Antigravity MCP continue to use configured cloud providers with zero disruption.
- **Automatic Migration**: Legacy Plan 32 configurations are seamlessly promoted to the explicit `active_route` schema on startup and save.

---

## v0.22.1 — 2026-09-23

### Xiaomi MiMo-V2.6 Support

Three new Xiaomi MiMo-V2.6 models are now available via the Direct MiMo provider:

- **`mimo-v2.6-flash`**: High-performance cost-efficient model ($0.14 input / $0.28 output per 1M tokens).
- **`mimo-v2.6-pro`**: Flagship trillion-parameter model for complex coding and agent tasks ($0.435 input / $0.87 output per 1M tokens).
- **`mimo-v2.6-pro-ultraspeed`**: Low-latency variant offering up to 20× output speed ($4.35 input / $8.70 output per 1M tokens).

All three models provide a 1,000,000-token context window, native multimodal support (text, image, video), and a Normal / Thinking toggle (no reasoning effort levels).

### Updated Direct MiMo Default Routes

The built-in Direct MiMo preset now routes:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking
- Default model: `mimo-v2.6-flash`

### V2.5 Backward Compatibility and Safe Migration

- Saved `mimo-v2.5`, `mimo-v2.5-pro`, and `mimo-v2.5-pro-ultraspeed` configurations continue to work without modification.
- Only untouched factory-default routes are automatically migrated to V2.6 on startup.
- User-customized route targets, thinking modes, and custom `default_model` values are never overwritten.
- Legacy `model_map`-only configurations now correctly resolve model-specific capabilities via the static resolver rather than inheriting provider-wide defaults.
- Saved V2.5 model IDs display as labeled entries (e.g., `MiMo-V2.5-Pro (Legacy / saved)`) in the Direct MiMo dropdown.

---

## v0.22.0 — 2026-09-12

### GPT-6 Astra Family on OpenRouter

Three new OpenAI models are now available through the **OpenRouter: chatGPT** profile:

- **`openai/gpt-6-astra`** — OpenAI's current flagship model with 1.05M context window and full reasoning-effort control (`low / medium / high / xhigh / max`). Image input supported via Gateway.
- **`openai/gpt-6-astra-pro`** — Same model served with `reasoning.mode = pro`, always-on Pro reasoning. No user-selectable reasoning effort; Anthro Bridge does not inject `reasoning.effort` for this model. Pricing: $10 / 1M input · $50 / 1M output.
- **`openai/gpt-astra-latest`** — Alias that tracks the latest Astra-family model. Inherits the same reasoning-effort controls as GPT-6 Astra. Pricing: $10 / 1M input · $50 / 1M output.

Built-in **OpenRouter: chatGPT** preset updated: Opus 5 → GPT-6 Astra / max · Sonnet 5 → GPT-6 Astra / high · Haiku 4.5 → GPT-6 Astra / medium.

### Unified OpenRouter OpenAI Model Selector

The **OpenRouter: chatGPT** profile model selector (Claude Code side) has been redesigned from a two-dropdown Tier × Mode matrix to a **single dropdown** containing the full 9-model OpenAI catalog:

```
GPT-6 Astra · GPT-6 Astra Pro · GPT Astra Latest
GPT-5.6 Sol · GPT-5.6 Sol Pro · GPT-5.6 Terra · GPT-5.6 Terra Pro · GPT-5.6 Luna · GPT-5.6 Luna Pro
```

- Existing saved profiles referencing any of the 9 model IDs load correctly without migration.
- The reasoning effort selector updates dynamically based on the selected model's capabilities.
- GPT-5.6 Pro variants retain their existing Pro reasoning semantics (`reasoning.mode = pro`).

### MCP Plan / Review: OpenRouter OpenAI Catalog Sync

The MCP Plan and Review settings panel now shows the full 9-model OpenAI catalog when an OpenRouter: chatGPT profile is selected, instead of only the 3 route-target upstream models (Sol / Terra / Luna). Model display names (e.g., "GPT-6 Astra") are now shown in MCP profile tiles and dropdowns. The `xhigh` reasoning effort option is correctly labeled in the MCP settings UI.

### DeepSeek V4.1 Flash

- **New model: `deepseek-v4.1-flash`** — Current-generation DeepSeek reasoning model, replacing V4 Flash as the recommended Direct DeepSeek choice.
- Built-in **Direct DeepSeek** preset updated: Opus 5 → V4.1 Flash / Max · Sonnet 5 → V4.1 Flash / High · Haiku 4.5 → V4.1 Flash / Low.
- `deepseek-v4-pro-0813` retained as a non-reasoning baseline option.
- Legacy `deepseek-v4-flash` and `deepseek-v4-flash-vision-exp` remain available for existing profiles.

### New Provider: Kimi Code

A dedicated Kimi Code API integration (`KIMI_CODE_API_KEY`) is now available, separate from the existing Moonshot/Kimi API:

- `kimi-for-coding` — Full-quality coding-specialist model.
- `kimi-for-coding-highspeed` — Low-latency variant optimized for interactive use.

### Model Catalog and Pricing Updates

- Pricing refreshed for GPT-6 Astra family, DeepSeek V4.1 Flash, Gemini Flash variants, and Laguna.
- `BUILTIN_OPENROUTER_MODELS` is now the single source of truth for all OpenRouter model metadata consumed by both the Claude Code dropdown and MCP settings panel.

### Compatibility

- Existing saved profiles (including those created with 0.21.x) load without migration.
- The two-dropdown Tier × Mode UI is replaced; saved model IDs continue to resolve correctly.
- No breaking changes to the 3P Gateway request format or MCP tool signatures.

---

## v0.21.2 — 2026-09-04

### Anthro-Review

- Clarified that local test execution during `/anthro-review` evidence collection is not subject to the exactly-once rule.
- Targeted tests, regression tests, full suites, package validation, reproducibility checks, RNG/deterministic-seed tests, statistical-invariance checks, and serial/parallel equivalence checks may be run iteratively as needed before review.
- The exactly-once rule continues to apply only to a successful usable `anthro-bridge/review` tool call.
- The existing single recovery retry for failed or unusable review calls remains unchanged.

## v0.21.1 — 2026-09-04

### Fixes

- Fixed the empty reasoning mode selector for Google Gemini 3.8 Flash in OpenRouter profiles.
- Added Gemini 3.8 Flash to the frontend OpenRouter model capability registry and test fixtures.
- Fixed the GUI version label so it matches the installed application version.
- Added regression coverage for Gemini 3.8 Flash reasoning options and version display consistency.

## v0.21.0 — 2026-09-03

### Anthro-Review

- **New MCP tool: `anthro-bridge/review`** — A post-implementation review gate that compares a completed implementation against the approved Anthro-Plan before commit.
- **Three-tier verdict**: `Approved`, `Approved with recommendations`, or `Not approved`, with a structured rationale for each finding.
- **Global Antigravity command: `/anthro-review`** — Installed alongside `/anthro-plan` and `/anthro-revise` as a third global skill, completing the plan → implement → review → commit workflow.
- **Coverage checklist**: The reviewer evaluates scope adherence, correctness, test coverage, edge cases, and whether any unintended files were modified.
- **Standalone MCP tool**: No Antigravity subscription capacity is consumed during the review step — the review is performed by the configured external planner model.

### OpenRouter: Gemini 3.8 Flash

- **New model: `google/gemini-3.8-flash`** — Added to the OpenRouter provider with 1,048,576-token context window.
- **Reasoning-effort support**: `low`, `medium`, and `high` levels mapped through the existing generic Gemini reasoning path.
- **Pricing**: Input $0.75 / 1M · Output $3.75 / 1M · Cache read $0.075 / 1M.
- **Built-in preset updated**: The **OpenRouter: Gemini** preset now maps Opus 5 → Gemini 3.8 Flash / High, Sonnet 5 → Gemini 3.8 Flash / Medium, Haiku 4.5 → Gemini 3.8 Flash / Low.
- **Exact-match migration safety**: The automatic migration from prior built-in Gemini defaults now uses exact-match comparison (route count, key set, `upstream_model`, `reasoning_effort`, `thinking_mode`) — custom profiles with any deviation from the historical defaults are left untouched.
- **Supported models (full list)**: `google/gemini-3.8-flash`, `google/gemini-3.7-flash`, `google/gemini-3.5-flash-lite`, `google/gemini-3.1-pro-preview`.

---

## v0.20.0 — 2026-08-23

### DeepSeek Vision and Pricing Updates

- Added `deepseek-v4-flash-vision-exp` as a selectable model for the 3P Gateway and Antigravity MCP planner.
- Added Base64 and image-URL input support for Vision Exp in the Gateway. DeepSeek Files API image references are not included.
- MCP planning uses Vision Exp as a text model; Antigravity interprets image attachments before sending the distilled task and context to Anthro Bridge.
- Added Beijing-time weekend off-peak pricing: Saturdays and Sundays are full-day VALLEY periods with a 50% discount, effective August 23, 2026.
- Updated PEAK / VALLEY indicators in the Gateway and MCP planner to reflect the weekend schedule.

### Model Capabilities and Documentation

- Added official 1,000,000-token context metadata for Vision Exp and its Normal, Low, High, and Max reasoning controls.
- Kept `deepseek-v4-pro` and `deepseek-v4-flash` available unchanged.
- Updated DeepSeek documentation across the supported language guides with Vision Exp input scope and weekend pricing details.

## v0.19.0 — 2026-08-19

### Google Antigravity and MCP Integration

- Added GUI-based Antigravity MCP registration and configuration updates, including custom or portable executable selection.
- Added global `/anthro-plan` and `/anthro-revise` skills for creating and revising implementation plans.
- Updated planner rules to avoid duplicate calls after a successful result and allow one recovery retry after failure.
- Added live DeepSeek PEAK / VALLEY indicators to the MCP Plan tab.

### Interface and Documentation

- Kept model pricing tiles and Antigravity configuration cards continuously visible.
- Separated provider/profile selection in the MCP workspace from detailed Antigravity model settings.
- Streamlined Settings navigation and titlebar behavior, including window dragging and version display.
- Synchronized documentation across the eight supported languages.

## v0.18.1 — 2026-08-19

### Unified MCP Server and Gateway Operation

- Added a single-binary MCP server mode. Launching `anthro-bridge.exe --mcp-server` starts the stdio MCP server without initializing the Tauri GUI or WebViews.
- Enabled the GUI 3P Gateway and MCP server to run concurrently.
- MCP planning now communicates directly with configured upstream providers and does not require the Gateway on port 4000.
- GUI and MCP modes share the same configuration directory and reload settings for each plan request.
- Refactored the README and added multilingual Antigravity MCP setup guides.

## v0.18.0 — 2026-08-19

### Workspace Navigation and MCP Planning

- Replaced standard window decorations with a custom titlebar and integrated workspace tabs for Anthro Bridge and MCP.
- Made workspace switching a top-level navigation action that returns the user from Settings to the selected workspace.
- Added an MCP planning settings panel with per-provider accordions and persistent configuration targets.

### Window and Installer Improvements

- Increased the base window height to 720px for a roomier dashboard and refined drag regions and Windows 11 titlebar controls.
- Added a multilingual NSIS installer with eight language choices and the Anthro Bridge application icon.

## v0.17.0 — 2026-08-18

### Gemini Models via OpenRouter

- Added built-in Gemini 3.1 Pro Preview and Gemini 3.7 Flash support, including pricing, capability data, and provider grouping.
- Added Gemini models to the context-capacity registry.

### Pricing and Documentation

- Added promotional 50% pricing for Gemini 3.7 Flash and GPT-5.6 Sol / Sol Pro, displaying regular prices alongside discounted rates.
- Refreshed DeepSeek V4 Pro and V4 Flash pricing and updated the pricing date across all eight locales.
- Updated the README and specification and produced a multilingual NSIS installer with the application icon.

## v0.16.2 — 2026-08-14

### DeepSeek Pricing and Timezone Display

- Added DeepSeek V4 Pro and V4 Flash pricing rows effective August 16, 2026.
- Changed PEAK / VALLEY hours in pricing notes to use the user's configured timezone instead of a fixed JST display.
- Added timezone-aware pricing-label helpers and localized the timezone-independent note prefix across all eight languages.
- Added the new DeepSeek model IDs to the model-capability registry.

## v0.16.1 — 2026-08-13

### DeepSeek V4 Pro Reasoning Effort

- Added Low, High, and Max reasoning-effort options for DeepSeek V4 Pro.
- Sent effort values using the `output_config.effort` field required by the DeepSeek Anthropic-compatible API.

## v0.16.0 — 2026-08-03

### Claude Code Context Management

- Added automatic context-window resolution from the models assigned to the Opus, Sonnet, and Haiku routes. Automatic management is enabled only when all three model capacities are known and uses the smallest capacity as the safe window.
- Added a header toggle and `auto`, `manual`, and `claude_default` context-management modes, with manual window and compaction-threshold configuration.
- Added context-capacity metadata for supported DeepSeek, MiniMax, Kimi, MiMo, and OpenRouter models.

### Claude Code Launch Command and Routing

- Added a PowerShell launch-command button that includes the Gateway URL, local authentication token, resolved context window, and compaction override.
- The command removes stale context variables when context management is disabled or cannot be resolved.
- Unified route-to-model resolution between Gateway requests and context management so both use the same upstream model.

### Installer

- Added installer language selection for eight languages and embedded the Anthro Bridge application icon.

## v0.15.2 — 2026-08-01

### Default Model Routing

- Updated default Direct DeepSeek routes for new installations: Opus 5 → V4 Flash / Thinking Max, Sonnet 5 → V4 Flash / Thinking High, and Haiku 4.5 → V4 Flash / Thinking Low. DeepSeek V4 Pro remains available for manual selection.
- Updated the built-in OpenAI GPT-5.6 Balanced OpenRouter profile defaults to Thinking High for Opus / Sol, Sonnet / Terra, and Haiku / Luna.
- Existing saved routing is not changed automatically.

## v0.15.1 — 2026-08-01

### DeepSeek V4 Flash Reasoning Controls

- Aligned V4 Flash (0731) with its three official reasoning-effort levels: Low, High, and Max. Effort is omitted in Normal mode.
- Aligned the V4 Pro controls to High and Max in Thinking mode and disabled effort in Normal mode.
- Added a startup migration that corrects legacy V4 Pro effort values and removes stale effort from non-thinking routes.
- Updated documentation and the new-install configuration template. The release includes an eight-language Windows installer with the app icon.

## v0.15.0 — 2026-08-01

### OpenRouter Profiles and Model Catalog

- Added independent OpenRouter profiles with profile-specific API keys and route mappings, drag-and-drop reordering, hidden-profile support, and persisted ordering.
- Added OpenRouter model families from Poolside Laguna, Tencent Hy3, InclusionAI, StepFun, and OpenAI GPT-5.6.
- Added pricing for GPT-5.6 Sol, Terra, Luna, and Pro variants, including applicable cached-input and long-context details.
- Added model-capability-aware Thinking and Reasoning Effort controls.
- Improved dashboard summaries by hiding vendor namespaces in display labels while retaining full routing IDs.

### Dashboard and Installer

- Added dashboard card-count tracking and dynamic window sizing for three-column layouts, with monitor, DPI, decoration, and minimum-size handling.
- Added regression coverage for save races, profile order, pricing data, dashboard card counts, and window sizing.
- Updated the README and specification and produced a multilingual Windows NSIS installer with the app icon.

## v0.14.0 — 2026-07-31

### OpenRouter Multi-Profile and Model Support

- Added multiple OpenRouter profiles, each with its own API key and model configuration, switchable from Dashboard or Settings without restarting.
- Added Tencent Hy3 reasoning controls, InclusionAI and StepFun providers, and configurable Kimi K3 reasoning effort and fallback behavior.
- Added a built-in OpenRouter model registry with model capabilities and pricing, plus provider-specific model-set cards and a dashboard visibility toggle.

### Configuration and UI Reliability

- Serialized configuration writes through a mutex-backed helper to prevent concurrent settings changes from overwriting one another.
- Fixed stale UI closures, cross-route rollback, generation races, and save sequencing when OpenRouter routes change during saves.
- Isolated development and stable build identities and data directories so both builds can run side by side.
- Added the multilingual installer and application icon.

## v0.13.1 — 2026-07-27

### Provider Response and Thinking Fixes

- Added detection and warning logs for Laguna S/XS 2.1 responses that reach the per-turn token cap without producing usable text or tool calls, in both streaming and non-streaming paths.
- Fixed translation of `thinking: { type: "disabled" }` to OpenRouter's `reasoning: { enabled: false }` for Poolside models when no saved setting exists.
- Changed the default `claude-opus-5` route for Laguna S 2.1 to Normal mode and added an idempotent migration; users can re-enable Thinking in Settings.
- Added Laguna behavior notes to the README, specification, and third-party inference guides in English and Japanese, plus regression tests for the fixes.

## v0.13.0 — 2026-07-26

### Response Model Identity and Communication Logs

- Normalized upstream model names to Claude model names in streaming and non-streaming responses so Claude clients display the expected identity.
- Added structured communication logs under `%APPDATA%\\Anthro Bridge\\Communication-Logs\\` with request correlation IDs and normalization outcomes, excluding prompts, request bodies, and API keys.
- Added an independent setting to enable or disable response model normalization.

### Configuration and Dashboard Fixes

- Made explicit `thinking_mode` take precedence over `force_thinking` in dashboard status display.
- Recomputed `force_thinking` from upstream model capabilities on startup and model save, and synchronized it when capability flags change.
- Added the eight-language installer with the app icon. Existing configurations are upgraded using the documented migration behavior.

## v0.12.2 — 2026-07-25

### MiniMax M3 Thinking Toggle

- Added a Thinking ON/OFF toggle for MiniMax M3 and mapped it to the provider's adaptive or disabled thinking parameter. When unset, Anthro Bridge omits the parameter and lets the API default apply.
- Migrated existing `thinking_only` settings to `thinking` while preserving an enabled state; new installations default to Thinking OFF.
- Updated the status panel so MiniMax M3 defaults to OFF while M2.x and Kimi K3 remain thinking-only.
- Updated the third-party inference and specification documentation in English, Japanese, and Chinese. The legacy root config retains `thinking_only` for migration compatibility.

## v0.12.1 — 2026-07-25

### Provider Switch Status

- Moved provider-switch progress and completion messages to the header beside the Gateway status badge.
- Added a compact inline spinner during provider switching and automatically cleared the success message after three seconds.
- Removed the duplicate spinner below the provider cards and corrected its circular styling.

## v0.12.0 — 2026-07-25

### OpenRouter and Poolside Laguna

- Added OpenRouter as a first-class provider with Poolside Laguna S 2.1 and Laguna XS 2.1 defaults and model-specific Thinking controls.
- Added live image/video capability flags fetched from OpenRouter and persisted in configuration, plus Laguna pricing in the model comparison table.

### Claude Model and UI Updates

- Migrated `claude-opus-4-8` to `claude-opus-5` across application layers and existing configurations.
- Replaced the generic OpenRouter Thinking default with model-specific options: Laguna S Max / Off and Laguna XS Thinking / Off.
- Unified OpenRouter and MiMo settings layouts, improved capability badges and price formatting, and added startup capability-flag migration.
- Updated all eight language resources and shipped a multilingual NSIS installer with language selection.

## v0.11.1 — 2026-07-18

### MiMo, Pricing, and Timezone Improvements

- Added MiMo-V2.5-Pro-UltraSpeed as a manually selectable MiMo / Xiaomi model.
- Displayed DeepSeek PEAK hours in the user's local timezone with a dashboard badge, and showed dynamic UTC offsets in the timezone selector.
- Translated model-pricing notes across all eight supported languages.
- Shipped an eight-language NSIS installer with the Anthro Bridge app icon; running it upgrades the existing installation while preserving settings.

## v0.11.0 — 2026-07-17

### Kimi and MiniMax Model Updates

- Added Kimi K3 with `reasoning_effort: "max"` handling and the K2.7-code-highspeed variant; updated the default Kimi routes.
- Changed the default Opus, Sonnet, and Haiku routes to MiniMax M3 with Thinking enabled.

### Pricing and Dashboard Synchronization

- Added a model-pricing table in Settings and input/output pricing columns to the dashboard routing table for DeepSeek, MiMo, MiniMax, and Kimi.
- Made model changes in Settings update dashboard cards and the routing table immediately without restarting the Gateway.
- Updated the K3 reasoning indicator, specifications, and eight-language installer.

## v0.10.1 — 2026-07-12

### Provider Settings and Reliability

- Added keyboard-accessible collapsible provider rows and per-provider Opus / Sonnet / Haiku model selectors, with Thinking Mode and supported Reasoning Effort controls.
- Added a custom upstream-model option and automatic saving for model and reasoning selections. Environment-variable names save on blur or Enter; API keys retain an explicit Save action.
- Added inline save status and stabilized the startup window size.
- Fixed provider-order changes on save, Thinking Mode persistence, NSIS installer icon configuration, and the LICENSE author name.
- Added an eight-language NSIS installer with the custom application icon.

## v0.10.0 — 2026-07-11

### Initial Provider Model Controls

- Added a three-tier model picker for Opus, Sonnet, and Haiku with per-tier Thinking Mode and supported Reasoning Effort controls.
- Added provider tiles in a three-column grid, fixed popover overflow, and simplified model summaries.
- Tightened API-key validation and added per-provider environment-variable configuration.
- Updated the language resources for English, Japanese, Simplified and Traditional Chinese, Korean, French, German, and Spanish.
