//! `anthro-bridge-mcp-server` — a standalone MCP server exposing a single
//! `plan` tool backed by DeepSeek V4 Pro.
//!
//! The library target exists so integration tests under `tests/` can exercise
//! prompt construction, provider HTTP, and the MCP tool layer without
//! launching the binary. The binary entry point is defined in `src/main.rs`.

pub mod mcp;
pub mod provider;
