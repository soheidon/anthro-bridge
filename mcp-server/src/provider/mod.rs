//! Provider abstraction: separates MCP protocol handling from provider-specific
//! HTTP details. See `mcp-server/SPEC.md` section 9.

pub mod adapter;
pub mod deepseek;

/// A successfully parsed planning response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanResponse {
    pub text: String,
}

/// Errors that can occur while talking to a planner provider.
///
/// Error messages returned to the MCP client must never include API keys,
/// authorization headers, or raw request bodies. Each variant carries only a
/// sanitized, human-readable message.
#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("provider not configured: {0}")]
    Config(String),

    #[error("provider authentication failed: {0}")]
    Auth(String),

    #[error("provider request failed (HTTP {status}): {message}")]
    Http { status: u16, message: String },

    #[error("provider timeout: {0}")]
    Timeout(String),

    #[error("provider network error: {0}")]
    Network(String),

    #[error("provider returned an unusable response: {0}")]
    Response(String),
}

/// A provider that turns a planning prompt into a plan.
///
/// Internal trait only, used generically (never made into a trait object), so
/// `async_trait` is not required. The method is declared with an explicit
/// `impl Future + Send` return type rather than `async fn` because `rmcp`'s
/// tool router requires the tool future to be `Send`. The `'static` bound keeps
/// `PlannerTool<P>` `'static` (required by `rmcp`'s `ServerHandler`).
pub trait PlannerProvider: Send + Sync + 'static {
    fn plan(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> impl std::future::Future<Output = Result<PlanResponse, ProviderError>> + Send;
}
