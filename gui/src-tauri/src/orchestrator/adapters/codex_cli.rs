use tokio_util::sync::CancellationToken;

use super::super::process_runner::{ProcessRunner, CLI_AGENT_SAFETY_TIMEOUT};
use super::{AdapterExecutionInput, AdapterExecutionOutput};

#[derive(Debug, Clone, Default)]
pub struct CodexCliAdapter {
    process_runner: ProcessRunner,
}

impl CodexCliAdapter {
    pub fn new() -> Self {
        Self {
            process_runner: ProcessRunner::new(),
        }
    }

    pub async fn execute(
        &self,
        input: &AdapterExecutionInput,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, String> {
        let profile = &input.profile;
        let executable = profile.executable.as_deref().unwrap_or("codex");

        let mut args = if let Some(ref custom_args) = profile.args {
            custom_args.clone()
        } else {
            // Default arguments for autonomous execution
            vec![
                "exec".to_string(),
                "--cd".to_string(),
                input.project_path.to_string_lossy().to_string(),
            ]
        };
        // `codex exec -` reads the prompt from stdin and avoids placing a
        // potentially large/untrusted prompt in the process command line.
        if !args.iter().any(|arg| arg == "-") {
            args.push("-".to_string());
        }

        let prompt = format!(
            "## System Instructions\n{}\n\n## User Prompt and Context\n{}",
            input.system_prompt, input.user_prompt
        );

        let result = self
            .process_runner
            .run_with_stdin(
                executable,
                &args,
                Some(&input.project_path),
                Some(prompt.as_bytes()),
                CLI_AGENT_SAFETY_TIMEOUT,
                cancel_token,
            )
            .await?;

        if result.cancelled {
            return Err("CLI agent execution was cancelled.".to_string());
        }
        if result.timed_out {
            return Err("CLI agent execution timed out after 60 minutes.".to_string());
        }

        let combined = if result.exit_code == Some(0) {
            if result.stdout.text.trim().is_empty() && !result.stderr.text.trim().is_empty() {
                result.stderr.text
            } else {
                result.stdout.text
            }
        } else {
            format!(
                "CLI process exited with code {:?}\nStdout:\n{}\nStderr:\n{}",
                result.exit_code, result.stdout.text, result.stderr.text
            )
        };

        Ok(AdapterExecutionOutput {
            content: combined,
            raw_json: None,
            tokens_used: None,
            model_used: executable.to_string(),
            duration_ms: result.duration_ms,
        })
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::orchestrator::types::{AgentRole, ExecutionAdapterType, OrchestratorProfile};
    use std::path::PathBuf;

    #[tokio::test]
    async fn adapter_delivers_system_and_user_prompts_to_cli_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let profile = OrchestratorProfile {
            id: "codex-test".into(),
            display_name: "Codex test".into(),
            adapter: ExecutionAdapterType::Cli,
            capabilities: Vec::new(),
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: Some("cat".into()),
            args: Some(Vec::new()),
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(32_768),
        };
        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile,
            system_prompt: "system\n日本語".into(),
            user_prompt: "user\n🚀".into(),
            project_path: PathBuf::from(dir.path()),
            temperature: None,
        };
        let output = CodexCliAdapter::new().execute(&input, None).await.unwrap();
        assert_eq!(
            output.content,
            "## System Instructions\nsystem\n日本語\n\n## User Prompt and Context\nuser\n🚀"
        );
    }
}
