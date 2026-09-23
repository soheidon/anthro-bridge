# Anthro Bridge v0.22.1

## Xiaomi MiMo-V2.6 Support

### What's New

#### Direct MiMo Provider — MiMo-V2.6 Series

Three new Xiaomi MiMo-V2.6 models are now available via the Direct MiMo provider:

- **`mimo-v2.6-flash`**: High-performance cost-efficient model ($0.14 input / $0.28 output per 1M tokens).
- **`mimo-v2.6-pro`**: Flagship trillion-parameter model for complex coding and agent tasks ($0.435 input / $0.87 output per 1M tokens).
- **`mimo-v2.6-pro-ultraspeed`**: Low-latency variant offering up to 20× output speed ($4.35 input / $8.70 output per 1M tokens).

All three models provide:
- 1,000,000-token context window
- Native multimodal support (text, image, video)
- Normal / Thinking toggle (no reasoning effort levels)

#### Updated Default Routes

The built-in Direct MiMo preset now routes:
- Opus 5 → `mimo-v2.6-pro` / Thinking
- Sonnet 5 → `mimo-v2.6-pro` / Normal
- Haiku 4.5 → `mimo-v2.6-flash` / Thinking

#### V2.5 Backward Compatibility and Safe Migration

- Saved `mimo-v2.5`, `mimo-v2.5-pro`, and `mimo-v2.5-pro-ultraspeed` configurations continue to work without modification.
- Only untouched factory-default routes are automatically migrated to V2.6 on startup.
- User-customized route targets, thinking modes, and custom `default_model` values are never overwritten.
- If you had previously customized your Direct MiMo route to V2.5, your settings are preserved exactly.

#### Legacy Configuration Compatibility

- Legacy configurations using `model_map` (without an explicit `models` object) now correctly resolve V2.6 route capabilities (image and video support) from the static model capability resolver, rather than inheriting the provider-wide defaults.
- V2.5 models in those same configurations correctly retain their original, narrower capabilities.

#### UI Improvements

- Previously saved V2.5 model IDs now display as labeled entries (e.g., `MiMo-V2.5-Pro (Legacy / saved)`) in the Direct MiMo dropdown, rather than falling back to the generic "Custom..." input.

---

### Installation

The v0.22.1 Windows installer is provided as a multilingual setup executable (`Anthro Bridge_0.22.1_x64-setup.exe`) on the [Releases](https://github.com/soheidon/anthro-bridge/releases) page.

The installer supports 8 languages (English, Japanese, Simplified Chinese, Traditional Chinese, Korean, French, German, Spanish) and preserves existing user settings during upgrades.
