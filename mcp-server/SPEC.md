# Anthro Bridge MCP Server — SPEC

## 1. Overview

`mcp-server` is a standalone Rust MCP server developed inside the `anthro-bridge` repository.

The initial purpose is to let an MCP-capable coding agent such as Google Antigravity call an external LLM through a small, controlled tool interface. The first supported use case is:

> Use DeepSeek V4 Pro to produce an implementation plan, then let the calling agent perform repository exploration, editing, build, and test execution.

The server is intentionally separated from the existing Tauri GUI during the first stage. After its behavior is validated, the MCP implementation is expected to be merged into Anthro Bridge and connected to Anthro Bridge's existing provider/model selection and API-key management.

Expected repository layout:

```text
anthro-bridge/
├─ gui/
│  ├─ src/
│  └─ src-tauri/
│
├─ mcp-server/
│  ├─ Cargo.toml
│  ├─ SPEC.md
│  └─ src/
│     ├─ main.rs
│     ├─ mcp.rs
│     └─ provider/
│        ├─ mod.rs
│        └─ deepseek.rs
│
├─ README.md
└─ ...
```

---

## 2. Goals

The MVP has five goals.

1. Run as a standalone Rust MCP server without modifying the existing Anthro Bridge GUI or proxy.
2. Be discoverable and callable from Antigravity as an MCP tool server.
3. Expose one planning tool that sends a bounded planning request to DeepSeek V4 Pro.
4. Return only the resulting plan to the calling agent.
5. Keep the internal design provider-agnostic enough that the implementation can later be merged into Anthro Bridge and use Anthro Bridge's existing provider/model selection.

The MCP server is not intended to replace the calling agent's own coding harness. The calling agent remains responsible for reading files, editing files, running commands, building, testing, and deciding when the task is complete.

---

## 3. Non-goals for the MVP

The first implementation must not expand beyond the minimum required to validate the architecture.

The MVP does **not** include:

- a GUI;
- changes to `gui/` or `gui/src-tauri/`;
- automatic modification of repository files by the MCP server;
- shell-command execution by the MCP server;
- repository-wide autonomous exploration by DeepSeek;
- arbitrary chat with DeepSeek;
- multiple planning agents;
- automatic repeated DeepSeek calls;
- a review tool;
- a debugging tool;
- Anthro Bridge profile selection;
- Anthro Bridge `config.json` integration;
- reuse of the current Anthro Bridge API-key resolver;
- OpenRouter, MiniMax, Kimi, MiMo, Gemini, or other providers;
- HTTP/SSE/Streamable HTTP transport;
- remote network exposure;
- usage-history storage;
- cost accounting;
- GUI status indicators;
- release packaging.

These can be considered only after the standalone MVP works reliably.

---

## 4. Core operating model

The intended workflow is:

```text
User
  ↓
Antigravity
  ↓
Antigravity explores the repository and collects relevant context
  ↓
MCP tool: plan
  ↓
anthro-bridge/mcp-server
  ↓
DeepSeek V4 Pro API
  ↓
Implementation plan
  ↓
Antigravity
  ↓
Antigravity edits / builds / tests using its own subscription quota
```

The important separation of responsibilities is:

### Calling agent

The calling agent should:

- inspect the repository;
- identify relevant files;
- collect only the context needed for planning;
- call the MCP planning tool;
- interpret the returned plan;
- make file changes;
- run tests and builds;
- decide whether further work is necessary.

### MCP server

The MCP server should:

- validate the tool arguments;
- construct a planning prompt;
- make one LLM request;
- return the plan;
- report failures clearly.

### Provider model

The provider model should:

- reason about the supplied task and context;
- identify likely affected files and logic;
- propose a concrete implementation sequence;
- identify risks and regression checks;
- not claim to have inspected files that were not supplied;
- not perform edits or tool calls.

---

## 5. MCP transport

### MVP transport

The first version uses **stdio** transport.

Conceptually:

```text
Antigravity
   │ stdin/stdout
   ▼
mcp-server.exe
   │ HTTPS
   ▼
DeepSeek API
```

Reasons for using stdio in the MVP:

- no port selection;
- no localhost server lifecycle;
- no authentication layer for local HTTP;
- no port-collision handling;
- no firewall considerations;
- easier isolation from the current Anthro Bridge GUI;
- easier diagnosis of the initial MCP handshake and tool-calling behavior.

The process must reserve stdout for MCP protocol messages. Diagnostic logging must therefore go to stderr.

### Future transport

When merged into the running Anthro Bridge application, a localhost MCP transport may be introduced so that Antigravity can connect to the already-running Anthro Bridge process.

That transport change must not require rewriting provider or planning logic.

---

## 6. MCP tools

### 6.1 `plan`

The MVP exposes exactly one tool:

```text
plan
```

Purpose:

> Generate an implementation plan for a software-development task using the supplied task description, relevant repository context, and constraints.

The tool must not edit files or invoke commands.

### Input

Logical schema:

```json
{
  "task": "string",
  "context": "string",
  "constraints": "string | optional"
}
```

#### `task`

Required.

A concise statement of what the user wants changed or implemented.

#### `context`

Required.

Relevant repository information collected by the calling agent.

This may contain:

- relevant file paths;
- code excerpts;
- configuration excerpts;
- test excerpts;
- current behavior;
- known related functions;
- current implementation constraints.

The calling agent should provide focused context rather than the entire repository.

#### `constraints`

Optional.

Explicit limitations, for example:

```text
Do not change unrelated providers.
Do not commit.
Keep API model IDs unchanged.
Run cargo test and vitest after implementation.
```

### Output

The tool returns plain text containing the implementation plan.

The plan should normally contain:

1. understanding of the requested change;
2. current implementation inferred from the supplied context;
3. files or components likely to require changes;
4. exact implementation sequence;
5. things that must not be changed;
6. tests and verification;
7. unresolved points, if the supplied context is insufficient.

No application-level JSON output format is required for the MVP unless required by the MCP library's result wrapper.

---

## 7. Planning prompt contract

The MCP server owns the planning prompt.

The calling agent supplies the task and context, but should not have to reproduce the planner system prompt on every invocation.

The planner prompt should instruct the model to:

- act as a software implementation planner, not as the implementation agent;
- base conclusions only on the supplied context;
- explicitly distinguish confirmed facts from assumptions;
- avoid broad refactors unless required;
- prefer minimal changes;
- preserve existing behavior outside the requested scope;
- identify affected files/functions precisely where the supplied context permits;
- provide ordered implementation steps;
- provide verification steps;
- identify missing information when the context is insufficient;
- never claim that a change has already been made;
- never produce fake test results;
- never assume access to the repository beyond the supplied context.

The prompt should favor a plan that can be handed directly back to an autonomous coding agent.

---

## 8. Context strategy

The MVP deliberately does **not** allow DeepSeek to autonomously scan the repository.

Instead:

```text
Antigravity repository exploration
        ↓
focused relevant context
        ↓
MCP plan
        ↓
DeepSeek reasoning
```

This is intentional for both efficiency and cost control.

The calling agent should normally send:

- the user request;
- the relevant file names;
- the smallest useful code excerpts;
- related tests;
- important architectural constraints.

It should avoid sending:

- the whole repository;
- generated files;
- build output unrelated to the task;
- binary content;
- long dependency lockfiles;
- unrelated source files.

The MCP server may enforce a configurable maximum request size in a later revision. The MVP should at minimum fail clearly if the request cannot be sent safely to the provider.

---

## 9. Provider abstraction

Although the MVP supports only DeepSeek, the provider call must not be embedded directly into MCP protocol-handling code.

Suggested module structure:

```text
src/
├─ main.rs
├─ mcp.rs
└─ provider/
   ├─ mod.rs
   └─ deepseek.rs
```

Conceptually:

```rust
pub struct PlanRequest {
    pub task: String,
    pub context: String,
    pub constraints: Option<String>,
}

pub struct PlanResponse {
    pub text: String,
}
```

Provider abstraction may use a trait or equivalent boundary, for example:

```rust
pub trait PlannerProvider {
    async fn plan(&self, request: &PlanRequest) -> Result<PlanResponse, ProviderError>;
}
```

The exact Rust trait syntax may be adapted to the chosen async/runtime design. Do not introduce abstraction solely for abstraction's sake; the requirement is simply that MCP protocol handling and provider-specific HTTP logic remain separate.

Future implementation should be able to replace:

```text
DeepSeekProvider
```

with an Anthro Bridge-selected provider without rewriting the MCP tool interface.

---

## 10. DeepSeek MVP provider

### Provider

MVP provider:

```text
DeepSeek
```

### Model

Default model:

```text
deepseek-v4-pro
```

The implementation must use the current API model identifier already used by Anthro Bridge rather than inventing a version-specific identifier.

### Reasoning

The MVP should use a fixed planning-oriented reasoning configuration.

Initial default:

```text
High
```

The reasoning configuration should live inside the provider layer rather than in MCP protocol code so it can later be replaced by Anthro Bridge's selected model/profile settings.

### API key

The standalone MVP reads the API key from an environment variable:

```text
DEEPSEEK_API_KEY
```

The API key must:

- never be accepted as an MCP tool argument;
- never be returned in an MCP result;
- never be printed to stdout;
- never be logged to stderr;
- never be persisted by the MCP server.

If the environment variable is missing, the failure behavior must be clear and tested. It is acceptable either to fail at startup or to expose the tool and return a configuration error at invocation time, provided the behavior is documented consistently.

### HTTP client

Requirements:

- HTTPS;
- bounded request timeout;
- explicit non-success HTTP handling;
- bounded response-body handling where practical;
- no retry loop in the MVP unless required for transport-level robustness.

The MVP must not silently issue a second paid provider request after a model/API error.

---

## 11. Error handling

At minimum distinguish:

```text
Configuration error
- missing API key
- invalid local configuration

Input error
- missing task
- empty task
- invalid arguments

Provider authentication error
- rejected API key

Provider request error
- non-success HTTP response
- provider validation error

Provider timeout/network error
- connection failure
- request timeout

Provider response error
- malformed or unusable response

Internal MCP error
- unexpected server failure
```

Error messages returned to the client must not include:

- API keys;
- authorization headers;
- complete sensitive HTTP request dumps.

Where a provider supplies a useful error message, return a sanitized concise version.

---

## 12. Logging

Because the MVP uses stdio MCP:

```text
stdout = MCP protocol only
stderr = diagnostic logs
```

The server must not write ordinary log lines to stdout.

Recommended diagnostics:

```text
server starting
tool invocation started
provider request started
provider request completed
tool invocation completed
sanitized error classification
elapsed duration
```

Do not log full prompt/context bodies by default.

Do not log authorization data.

Token counts or provider usage metadata may be logged later if returned reliably by the provider, but usage telemetry is not required for the MVP.

---

## 13. Security

The MCP server is intended for local developer use.

MVP security requirements:

- API key only from environment;
- no key in tool arguments;
- no key in source code;
- no key in repository files;
- no secrets in stdout/stderr;
- no file-writing capability;
- no shell-execution capability;
- no arbitrary URL fetch tool;
- no external server listening port;
- only the explicitly defined MCP tool is exposed.

The planner's input is untrusted text. It must be treated as data sent to the provider, not as instructions that alter the MCP server's own security behavior.

---

## 14. Dependencies

Dependencies should remain minimal.

Likely categories:

- async runtime;
- MCP protocol/server implementation;
- HTTP client;
- serde/JSON;
- error handling;
- optional structured logging.

Before adding a crate, verify that it is necessary and actively maintained.

Avoid adding a large web framework for the stdio MVP.

Avoid creating a workspace-level dependency reorganization solely for this experiment.

---

## 15. Configuration

MVP configuration is intentionally minimal.

Required:

```text
DEEPSEEK_API_KEY
```

Optional future environment variables may include:

```text
ANTHRO_MCP_MODEL
ANTHRO_MCP_REASONING
ANTHRO_MCP_TIMEOUT_SECONDS
```

These should not be introduced unless there is a concrete need during MVP testing.

Hard-coded MVP defaults are acceptable for:

```text
provider = DeepSeek
model = deepseek-v4-pro
reasoning = High
```

provided those values are isolated in the provider/config layer and not scattered through MCP code.

---

## 16. Testing strategy

The standalone server must have automated tests before Anthro Bridge integration.

### 16.1 Input validation tests

At minimum:

```text
empty task → rejected
valid task + context → accepted
constraints omitted → accepted
```

### 16.2 Prompt construction tests

Verify that:

- task is included;
- context is included;
- constraints are included when present;
- the planner role instructions are included;
- the prompt does not falsely imply repository access.

### 16.3 Provider request tests

Use a mock/local test server or dependency boundary rather than the real API.

Verify:

```text
model = deepseek-v4-pro
expected reasoning configuration is applied
authorization is present in the outgoing provider request
API key never appears in returned error text
successful provider result is extracted correctly
non-2xx response becomes a provider error
timeout becomes a timeout/network error
malformed response becomes a response error
```

### 16.4 MCP tool tests

Verify:

```text
server exposes exactly the expected MVP tool
tool name = plan
tool arguments match the declared schema
plan result is returned as MCP tool content
provider errors become clean MCP tool errors
```

### 16.5 stdout discipline

Where practical, test or manually verify that diagnostic logging does not contaminate stdout MCP traffic.

### 16.6 Real API smoke

A real DeepSeek API request is optional and should be minimal.

If credentials are available, perform only enough testing to confirm:

```text
MCP client
→ plan
→ Rust server
→ DeepSeek API
→ plan text
→ MCP client
```

Do not run repeated paid benchmarking as part of the automated suite.

---

## 17. Antigravity smoke test

The MVP is considered functionally validated only after an actual Antigravity test.

Recommended test task:

```text
Inspect the relevant files for a very small UI text change.
Call the `plan` MCP tool once with the relevant context.
Use the returned plan to implement the change.
Run the appropriate build/test command.
```

Record:

- whether Antigravity discovers the MCP server;
- whether the `plan` tool appears correctly;
- whether Antigravity calls it when instructed;
- whether DeepSeek's returned plan is usable;
- whether Antigravity follows the plan;
- number of DeepSeek API calls;
- approximate task duration;
- any duplicated repository reads before/after planning;
- Antigravity quota change if visible;
- DeepSeek API cost/usage if available.

The first experiment should keep DeepSeek calls to one planning request.

---

## 18. Success criteria for MVP

Phase 1 is complete when all of the following are true:

1. `mcp-server` builds independently.
2. Existing Anthro Bridge GUI/Tauri code is unchanged.
3. Antigravity can launch/connect to the stdio MCP server.
4. Antigravity discovers exactly the intended `plan` tool.
5. A valid `plan` call reaches DeepSeek V4 Pro.
6. The returned plan reaches Antigravity correctly.
7. API-key handling is safe.
8. Automated tests pass.
9. One real end-to-end Antigravity smoke test succeeds.
10. One user task can be completed with DeepSeek used for planning and Antigravity used for implementation/build/test.

---

## 19. Phase 2 — provider-neutral planner core

Only after Phase 1 succeeds.

Goals:

- formalize the provider abstraction;
- separate planning prompt construction from provider HTTP details;
- add explicit configuration object;
- improve response/usage metadata handling if useful;
- prepare code for Anthro Bridge integration.

Possible internal shape:

```text
MCP protocol
   ↓
PlannerService
   ↓
PlannerProvider
   ↓
DeepSeekProvider
```

The MCP layer must not know DeepSeek-specific payload details.

---

## 20. Phase 3 — integration into Anthro Bridge

After the standalone implementation proves useful, merge the reusable pieces into Anthro Bridge.

Target architecture:

```text
Antigravity
   ↓ MCP
Anthro Bridge
   ↓
PlannerService
   ↓
Anthro Bridge provider/profile selection
   ↓
Selected API / model
```

The principal goal of integration is to reuse Anthro Bridge's existing strength:

> Provider and model selection is controlled by Anthro Bridge rather than by the MCP client.

At this stage, Anthro Bridge should eventually be able to choose among supported providers/models without changing Antigravity's MCP configuration.

Expected integration areas:

- existing API-key resolution;
- existing provider adapters;
- existing model IDs;
- existing reasoning/thinking configuration;
- existing OpenRouter profiles;
- provider connection validation where reusable.

Do not duplicate existing Anthro Bridge provider logic if it can be cleanly reused.

---

## 21. Phase 4 — optional GUI

GUI work is intentionally deferred until the MCP architecture is validated.

Possible future settings:

```text
MCP Server
[ Enabled ]

Planner source
[ Existing Anthro Bridge profile/model selector ]

Status
Connected / Stopped / Error
```

The final UI should prefer reuse of Anthro Bridge's existing provider/model selection rather than creating a second independent provider configuration system.

The exact GUI design is out of scope for the standalone MCP implementation.

---

## 22. Possible future tools

Do not implement these in the MVP.

### `review`

Potential later tool:

```text
review
```

Input:

- original task;
- implementation plan;
- diff or changed code;
- test results.

Purpose:

> Ask the selected external model to review the completed implementation once.

A controlled two-call workflow could then be:

```text
DeepSeek/provider → plan
Antigravity       → implement/test
DeepSeek/provider → review (optional)
```

### Other tools

Do not add generic tools such as:

```text
chat
debug
ask
code
shell
read_file
write_file
```

unless a separate design decision explicitly requires them.

The value of this MCP server is controlled delegation, not turning it into another autonomous coding harness.

---

## 23. Cost-control principles

One reason for this architecture is to use paid API calls only where they provide high value.

Therefore:

- one `plan` invocation should normally equal one provider request;
- no hidden retry loop;
- no autonomous iterative conversation with the provider;
- repository exploration stays with the calling agent;
- provider context should be focused;
- repeated planning should require an explicit second tool call.

The server should optimize for:

```text
high-value reasoning via API
+
tool loop / editing / testing via the calling agent
```

rather than moving the entire agent loop into the paid provider.

---

## 24. Source-control rules during initial development

Until the user explicitly requests otherwise:

- do not commit automatically;
- do not modify unrelated Anthro Bridge files;
- do not reformat the existing project;
- do not restructure the repository into a Cargo workspace;
- do not change release configuration;
- do not change existing provider behavior;
- keep all experimental implementation under `mcp-server/`.

A typical initial change set should therefore be limited to:

```text
mcp-server/Cargo.toml
mcp-server/SPEC.md
mcp-server/src/main.rs
mcp-server/src/mcp.rs
mcp-server/src/provider/mod.rs
mcp-server/src/provider/deepseek.rs
mcp-server/tests/...
```

plus a root-level ignore/documentation adjustment only if strictly necessary.

---

## 25. Initial implementation order

Implement in this order:

### Step 1
Create the standalone Rust package under:

```text
anthro-bridge/mcp-server/
```

### Step 2
Implement the minimum stdio MCP server and expose the `plan` tool.

### Step 3
Add input types and validation.

### Step 4
Implement planner prompt construction independently from the provider.

### Step 5
Implement `DeepSeekProvider`.

### Step 6
Connect `plan` to the provider.

### Step 7
Add unit/integration tests with mocked provider HTTP behavior.

### Step 8
Run inside `mcp-server/`:

```bash
cargo fmt --check
cargo check
cargo test
```

### Step 9
Configure Antigravity to launch the MCP server via stdio.

### Step 10
Perform one end-to-end planning smoke test.

Do not begin Anthro Bridge GUI integration until this sequence is working.

---

## 26. Definition of done for the standalone experiment

The standalone experiment is done when the following interaction works reliably:

```text
Antigravity:
"I inspected these files. Use the Anthro Bridge planner tool to make a plan."

        ↓

MCP `plan`
        ↓

DeepSeek V4 Pro
        ↓

Concrete implementation plan
        ↓

Antigravity implements it
        ↓

Antigravity builds/tests it
        ↓

Task complete
```

At that point, evaluate whether the architecture provides a meaningful improvement in:

- planning quality;
- implementation reliability;
- Antigravity tool-call efficiency;
- API cost;
- total completion time.

Only after that evaluation should the MCP server be merged into the main Anthro Bridge runtime and exposed through the GUI.
