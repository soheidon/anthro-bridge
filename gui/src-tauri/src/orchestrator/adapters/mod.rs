pub mod codex_cli;
pub mod ollama;
pub mod provider;

use super::types::{AgentRole, OrchestratorProfile};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterExecutionInput {
    pub role: AgentRole,
    pub profile: OrchestratorProfile,
    pub system_prompt: String,
    pub user_prompt: String,
    pub project_path: PathBuf,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterExecutionOutput {
    pub content: String,
    pub raw_json: Option<String>,
    pub tokens_used: Option<u64>,
    pub model_used: String,
    pub duration_ms: u64,
}
