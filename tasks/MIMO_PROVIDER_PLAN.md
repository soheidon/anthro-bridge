# MiMo Provider Implementation Plan

## Purpose

Add MiMo provider support to Anthro Bridge.

This task should be implemented in small phases.
Do not perform a large rewrite.
Do not change existing provider behavior unless it is necessary for MiMo support.

The existing providers are:

* DeepSeek
* MiniMax
* Kimi / Moonshot

MiMo should be added as a fourth provider.

---

## Target UI

The current provider selection UI has three cards:

```text
[ DeepSeek ] [ MiniMax ] [ Kimi / Moonshot ]
```

Add MiMo as a fourth card.

The preferred layout is a 2x2 card layout:

```text
[ DeepSeek ] [ MiMo ]
[ MiniMax  ] [ Kimi / Moonshot ]
```

MiMo should appear next to DeepSeek because it is another general-purpose upstream provider.

Do not redesign the whole UI.
Keep the existing card style, spacing, border, selected state, and typography as much as possible.

---

## Target MiMo Provider

Add a new provider named:

```text
MiMo
```

Provider key should be consistent with existing naming conventions.
If the code uses lowercase provider IDs, use:

```text
mimo
```

API key status should be displayed together with existing providers.

Example:

```text
MiniMax: ✓   Kimi / Moonshot: ✓
DeepSeek: ✓  MiMo: ✓
```

If no MiMo API key is configured, display the same unavailable or missing-key state used by other providers.

---

## MiMo API

Use MiMo's Anthropic-compatible API route.

Expected endpoint:

```text
https://api.xiaomimimo.com/anthropic/v1/messages
```

Do not implement a separate OpenAI-compatible path in this task.

Use the same request/response path style as the existing Anthropic-compatible providers wherever possible.

---

## Environment Variable

Add an environment variable for the MiMo API key.

Preferred name:

```text
MIMO_API_KEY
```

If the repository already has a consistent provider-specific naming pattern, follow the existing pattern.

Do not hard-code API keys.

---

## Model Mapping

Add MiMo mappings for the gateway models.

Initial mapping:

```text
claude-sonnet-4-6 -> mimo-v2.5-pro
claude-haiku-4-5  -> mimo-v2.5
```

Displayed labels:

```text
Sonnet 4.6 → mimo-v2.5-pro
Haiku 4.5 → mimo-v2.5
```

Rationale:

* `mimo-v2.5-pro` is the high-performance text / reasoning model.
* `mimo-v2.5` is the model to use for image pass-through testing.

Do not remove or rename existing model mappings.

---

## Capability Policy

### mimo-v2.5-pro

Initial capability policy:

```text
text: true
thinking: true
image: false
image_url: false
image_base64: false
video: false
video_url: false
video_base64: false
audio: false
audio_url: false
audio_base64: false
```

UI summary:

```text
テキスト・推論
```

or:

```text
テキスト入力対応
```

### mimo-v2.5

Initial capability policy:

```text
text: true
thinking: true
image: true
image_url: true
image_base64: true
video: false
video_url: false
video_base64: false
audio: false
audio_url: false
audio_base64: false
```

UI summary:

```text
画像入力対応
```

### General Policy

For this task, image input should be allowed for MiMo models that declare image support.

Audio and video should not be implemented in this task.

If MiMo rejects image input at runtime, return a clear provider error rather than silently stripping the image.

---

## Available Model Table

When MiMo is selected, the available model table should show the MiMo upstream models.

Expected rows:

```text
GATEWAYモデル          UPSTREAM          役割        IMG URL   IMG B64   VID URL   VID B64   THINKING
claude-sonnet-4-6      mimo-v2.5-pro     Sonnet 4.6  NO        NO        NO        NO        DEFAULT
claude-haiku-4-5       mimo-v2.5         Haiku 4.5   対応      対応      NO        NO        DEFAULT
```

If the existing UI uses other labels such as `テキスト化`, `NO`, `DEFAULT`, or checkmarks, follow the existing style.

Do not introduce a new visual language only for MiMo.

---

## Image Pass-through Policy

For MiMo models that declare image support:

* Do not strip image URL payloads.
* Do not strip base64 image payloads.
* Preserve `media_type` when present.
* Preserve the Anthropic-style image content blocks as much as possible.
* If local image files are handled elsewhere in the app, allow the existing local-file-to-base64 path to work for MiMo too.

This task should not add a new image upload UI unless the app already has one.

The goal is to allow the existing image input path to pass through to MiMo.

---

## Error Handling

If MiMo rejects a request, show a clear error.

The error should make it possible to distinguish:

```text
1. Missing MiMo API key
2. MiMo provider rejected the request
3. Anthro Bridge transformed the request incorrectly
4. Network or endpoint error
```

Do not hide provider errors behind a generic unknown error if more specific information is available.

---

## Logging

Add or preserve logs that help debug MiMo requests.

Logs should make clear:

* selected provider
* selected upstream model
* whether the request contained image URL input
* whether the request contained base64 image input
* whether thinking mode was requested
* provider response status when an error occurs

Do not log API keys.
Do not log full base64 image data.

---

# Implementation Phases

## Phase 1: Provider Configuration Only

Goal:

Add the MiMo provider configuration without changing the visual layout yet.

Tasks:

* Add provider key / provider definition for MiMo.
* Add MiMo API endpoint.
* Add `MIMO_API_KEY` support.
* Add API key status detection.
* Ensure existing providers still work.
* Do not add the MiMo card yet unless the existing architecture requires it.

Acceptance criteria:

* Project builds.
* Type check passes.
* Existing providers are unchanged.
* MiMo API key status can be detected internally.
* No broad refactoring is performed.

Stop after Phase 1 and report:

* changed files
* what was added
* how it was verified
* any unresolved issues

Do not proceed to Phase 2 unless explicitly instructed.

---

## Phase 2: Add MiMo Provider Card

Goal:

Add MiMo to the provider selection UI.

Tasks:

* Add a MiMo card.
* Place it next to DeepSeek.
* Change provider card layout to 2x2 if needed.
* Keep existing card styling.
* Make MiMo selectable.
* Make selected state work the same way as other providers.

Preferred layout:

```text
[ DeepSeek ] [ MiMo ]
[ MiniMax  ] [ Kimi / Moonshot ]
```

MiMo card text:

```text
MiMo
画像入力対応

Sonnet 4.6 → mimo-v2.5-pro
Haiku 4.5 → mimo-v2.5
```

Alternative summary, if the UI needs a shorter label:

```text
テキスト・画像 入力対応
```

Acceptance criteria:

* MiMo card appears next to DeepSeek.
* Selecting MiMo updates the selected provider.
* Existing provider cards still work.
* The layout remains visually clean at the current desktop window size.
* No unrelated UI redesign is performed.

Stop after Phase 2 and report:

* changed files
* screenshot or description of the final layout
* verification command results
* any layout concerns

Do not proceed to Phase 3 unless explicitly instructed.

---

## Phase 3: Model Mapping and Capability Table

Goal:

Make the MiMo selection update the available model table.

Tasks:

* Add model mappings:

  * `claude-sonnet-4-6 -> mimo-v2.5-pro`
  * `claude-haiku-4-5 -> mimo-v2.5`
* Add capability rows for MiMo.
* Show image support for `mimo-v2.5`.
* Keep video support disabled.
* Keep audio support disabled if audio is represented in the code.
* Keep thinking mode displayed consistently with other providers.

Expected table behavior:

```text
claude-sonnet-4-6 -> mimo-v2.5-pro
claude-haiku-4-5  -> mimo-v2.5
```

Expected capabilities:

```text
mimo-v2.5-pro:
  IMG URL: NO
  IMG B64: NO
  VID URL: NO
  VID B64: NO
  THINKING: DEFAULT

mimo-v2.5:
  IMG URL: 対応
  IMG B64: 対応
  VID URL: NO
  VID B64: NO
  THINKING: DEFAULT
```

If the existing application uses `テキスト化` to mean image input is converted to text, preserve the existing terminology unless it is clearly wrong.

Acceptance criteria:

* MiMo selection shows MiMo upstream models.
* Existing provider model tables are unchanged.
* Type check passes.
* No unrelated model mapping changes are made.

Stop after Phase 3 and report:

* changed files
* final MiMo model table behavior
* verification command results
* any remaining ambiguity about capability labels

Do not proceed to Phase 4 unless explicitly instructed.

---

## Phase 4: Image Pass-through and Runtime Test Support

Goal:

Allow image payloads to reach MiMo for models that declare image support.

Tasks:

* Ensure image URL content blocks are passed through for `mimo-v2.5`.
* Ensure base64 image content blocks are passed through for `mimo-v2.5`.
* Do not pass video or audio payloads in this task.
* If unsupported media is provided, return a clear error.
* Add logging that helps determine whether image blocks were preserved or stripped.
* Do not log full base64 data.

Expected behavior:

* Text-only requests to MiMo work.
* Image URL requests to `mimo-v2.5` are sent upstream.
* Base64 image requests to `mimo-v2.5` are sent upstream.
* If upstream rejects the image request, the error is visible and understandable.
* Existing providers are not affected.

Acceptance criteria:

* Project builds.
* Type check passes.
* Text-only request path remains intact.
* Image payloads are not stripped for MiMo image-capable models.
* Provider rejection is handled gracefully.

Stop after Phase 4 and report:

* changed files
* verification command results
* any manual test instructions
* known limitations

---

# Out of Scope

Do not implement these in this task:

* Audio input
* Video input
* TTS
* ASR
* Voice cloning
* OpenAI-compatible MiMo route
* New file upload UI
* New model management UI
* Full redesign of the dashboard
* Renaming existing providers
* Removing existing provider support
* Large refactoring unrelated to MiMo

---

# General Constraints

Follow the existing repository style.

Before changing code:

1. Inspect the current implementation.
2. Identify the files that need changes.
3. Prefer minimal, reversible edits.
4. Reuse existing provider patterns.

After changing code:

1. Run the existing type check or build command.
2. Report the command and result.
3. Report all changed files.
4. Mention any behavior that was not verified.

Do not proceed to the next phase unless explicitly instructed by the user.

---

# Manual Test Notes

## Prerequisites

1. Obtain a MiMo API key from Xiaomi MIMO.
2. Open Anthro Bridge → Settings (⚙) → **API Key** tab.
3. Find the MiMo row, enter the key, click **Save Key**.
4. Confirm the env var `MIMO_API_KEY` status shows ✓.

## Test Steps

### Test 1: MiMo + Sonnet 4.6 (text only)

1. Dashboard → Select **MiMo** tile.
2. Click **Start Gateway**.
3. In Claude Desktop / Cowork on 3P, select the Sonnet 4.6 model.
4. Send a plain text message: "Hello, what model are you?"
5. Confirm the response arrives normally.

Expected:
- Log shows `claude-sonnet-4-6 -> mimo-v2.5-pro`.
- No `image_blocks` in the log line.
- Text response from MiMo.

### Test 2: MiMo + Haiku 4.5 (text only)

1. Select the Haiku 4.5 model in Claude Desktop.
2. Send a plain text message.
3. Confirm the response arrives normally.

Expected:
- Log shows `claude-haiku-4-5 -> mimo-v2.5`.
- Text response from MiMo.

### Test 3: MiMo + Haiku 4.5 + image

1. In Claude Desktop, attach a small PNG or JPEG image (e.g. under 100 KB).
2. Send with a prompt like "Describe this image."
3. Check the Anthro Bridge log.

Expected:
- Log shows `img_url=X` or `img_b64=X after sanitize` with non-zero values.
- `sanitized=false` — image blocks preserved.
- MiMo returns an image description.

### Test 4: MiMo + Sonnet 4.6 + image

1. Switch to Sonnet 4.6 model in Claude Desktop.
2. Attach the same image.
3. Send with a prompt like "Describe this image."
4. Check the Anthro Bridge log.

Expected:
- Log shows `sanitized=true` — image blocks replaced with placeholder text.
- Response mentions placeholder text like `[Image omitted: ...]`.
- No API error from MiMo (image was stripped before sending).

## Log Checks

Open Settings (⚙) → **Log** panel, or tail the latest log file.

Key fields in each `POST /v1/messages` log line:

| Field | Meaning |
|-------|---------|
| `claude_model=` | Gateway model name the client sent |
| `upstream_model=` | Actual model sent to MiMo |
| `provider=` | Should be `mimo` |
| `image_blocks=` | Total image blocks detected (before sanitize) |
| `img_url=` | Image URL blocks after sanitize |
| `img_b64=` | Base64 image blocks after sanitize |
| `sanitized=` | `true` if any blocks were replaced/dropped |
| `thinking_mode=` | `Default` / `Forced` / `Disabled` / `Enabled` |

## Troubleshooting

### Missing MiMo API key

Symptom: Clicking Start Gateway shows `MIMO_API_KEY not set`.

Fix: Settings → API Key → MiMo row → enter key → Save Key.

### MiMo rejected the request

Symptom: Log shows `upstream error status=4xx body=...`.

Check:
- API key is valid and not expired.
- The model name (`mimo-v2.5-pro` / `mimo-v2.5`) is correct for MiMo's API.

### Image rejected by MiMo

Symptom: MiMo returns 400 error when sending images to `mimo-v2.5`.

Check:
- Image is under MiMo's size limit.
- Image format is supported (PNG, JPEG).
- Log confirms `sanitized=false` (image was not stripped by Anthro Bridge).

### Network or endpoint error

Symptom: Log shows connection error or timeout to `api.xiaomimimo.com`.

Check:
- Network connectivity.
- Firewall/proxy settings.
- MiMo API endpoint is reachable.

## Implementation Summary

Files changed across all 4 phases:

| Phase | Files |
|-------|-------|
| 1. Config | `config.json`, `gui/src-tauri/resources/config.json`, `gui/src-tauri/src/proxy.rs`, `gui/src/modelCapabilities.ts` |
| 2. UI Card | `gui/src/components/ProviderTiles.tsx`, `gui/src/App.css`, `gui/src/i18n/lang/*.ts` (6 languages) |
| 3. Model Table | No changes needed (already covered by Phase 1) |
| 4. Logging | `gui/src-tauri/src/proxy.rs` (image type counters + log enhancement) |

No changes to existing provider behavior. No build errors (TypeScript + Cargo clean).

