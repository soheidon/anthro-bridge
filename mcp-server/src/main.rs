//! stdio MCP server entry point.
//!
//! stdout is reserved for MCP protocol messages, so all diagnostic logging is
//! written to stderr.

use std::process::ExitCode;

use anthro_bridge_mcp_server::mcp::PlannerTool;
use anthro_bridge_mcp_server::provider::adapter::DynamicBridgeProvider;
use rmcp::{transport::stdio, ServiceExt};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let provider = DynamicBridgeProvider::new();
    let tool = PlannerTool::new(provider);

    tracing::info!("starting Anthro Bridge MCP server over stdio");

    let service = match tool.serve(stdio()).await {
        Ok(service) => service,
        Err(err) => {
            tracing::error!("failed to start MCP service: {err:?}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(err) = service.waiting().await {
        tracing::error!("MCP service stopped with an error: {err:?}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}
