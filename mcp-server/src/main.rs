//! stdio MCP server entry point.
//!
//! stdout is reserved for MCP protocol messages, so all diagnostic logging is
//! written to stderr.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    match anthro_bridge_mcp_server::run_stdio_mcp_server().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("[anthro-bridge-mcp-server] fatal: {err}");
            ExitCode::FAILURE
        }
    }
}
