# Anthro Bridge — Release Notes

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

## Previous Releases

- **v0.20.0** — DeepSeek V4 Flash Vision Exp support and Weekend Off-Peak pricing (2026-08-23)
- **v0.19.0** — Antigravity MCP integration, settings sub-navigation, `/anthro-plan` and `/anthro-revise` global skills (2026-08-19)
