use serde_json::{json, Value};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

use super::super::process_runner::HTTP_INFERENCE_SAFETY_TIMEOUT;
use super::super::secrets::SecretRedactor;
use super::{AdapterExecutionInput, AdapterExecutionOutput};

#[derive(Debug, Clone, Default)]
pub struct OllamaAdapter {
    client: reqwest::Client,
    redactor: SecretRedactor,
}

impl OllamaAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder().build().unwrap_or_default(),
            redactor: SecretRedactor::new(),
        }
    }

    pub async fn execute(
        &self,
        input: &AdapterExecutionInput,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, String> {
        let profile = &input.profile;
        let base_endpoint = profile
            .ollama_endpoint
            .as_deref()
            .unwrap_or("http://127.0.0.1:11434");
        let model = profile.ollama_model.as_deref().unwrap_or("mimo-v2.6:9b");

        let endpoint = format!("{}/api/chat", base_endpoint.trim_end_matches('/'));

        let mut body = json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": input.system_prompt
                },
                {
                    "role": "user",
                    "content": input.user_prompt
                }
            ],
            "stream": false
        });

        if let Some(temp) = input.temperature {
            body["options"] = json!({
                "temperature": temp
            });
        }

        let start = Instant::now();
        let cancel_fut = async {
            if let Some(token) = cancel_token {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        let request_fut = async {
            let res = self
                .client
                .post(&endpoint)
                .json(&body)
                .send()
                .await
                .map_err(|e| {
                    format!(
                        "Ollama request to '{}' failed: {}. Is Ollama running on {}?",
                        endpoint, e, base_endpoint
                    )
                })?;

            let status = res.status();
            let raw_text = res
                .text()
                .await
                .map_err(|e| format!("Failed to read Ollama response: {}", e))?;

            if !status.is_success() {
                let redacted_err = self.redactor.redact_secrets(&raw_text);
                return Err(format!(
                    "Ollama returned HTTP {}: {}",
                    status.as_u16(),
                    redacted_err
                ));
            }

            let parsed: Value = serde_json::from_str(&raw_text)
                .map_err(|e| format!("Failed to parse Ollama JSON response: {}", e))?;

            let content = parsed
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .ok_or_else(|| "Could not find message.content in Ollama response".to_string())?
                .to_string();

            let prompt_tokens = parsed
                .get("prompt_eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let eval_tokens = parsed
                .get("eval_count")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tokens_used = if prompt_tokens + eval_tokens > 0 {
                Some(prompt_tokens + eval_tokens)
            } else {
                None
            };

            let duration_ms = start.elapsed().as_millis() as u64;

            Ok(AdapterExecutionOutput {
                content: self.redactor.redact_secrets(&content),
                raw_json: Some(self.redactor.redact_secrets(&raw_text)),
                tokens_used,
                model_used: model.to_string(),
                duration_ms,
            })
        };

        tokio::select! {
            _ = tokio::time::sleep(HTTP_INFERENCE_SAFETY_TIMEOUT) => {
                Err("Ollama inference request timed out after 180 seconds.".to_string())
            }
            _ = cancel_fut => {
                Err("Ollama inference request cancelled by user.".to_string())
            }
            res = request_fut => {
                res
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::types::{AgentRole, ExecutionAdapterType, OrchestratorProfile};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc;

    #[tokio::test]
    async fn ollama_receives_original_prompts_while_output_is_redacted() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            let headers = String::from_utf8_lossy(&request);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap();
            let mut body = vec![0_u8; content_length];
            stream.read_exact(&mut body).unwrap();
            tx.send(serde_json::from_slice::<Value>(&body).unwrap())
                .unwrap();
            let response = r#"{"message":{"content":"sk-testsecret123456789"}}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        });

        let input = AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: OrchestratorProfile {
                id: "ollama-test".into(),
                display_name: "Ollama Test".into(),
                adapter: ExecutionAdapterType::Ollama,
                capabilities: vec![],
                provider_id: None,
                provider_profile_id: None,
                model: None,
                thinking_mode: None,
                reasoning_effort: None,
                ollama_model: Some("test-model".into()),
                ollama_endpoint: Some(endpoint),
                executable: None,
                args: None,
                external_mcp_server: None,
                mcp_tool: None,
                context_window_tokens: Some(8192),
            },
            system_prompt: "system sk-testsecret123456789 unchanged".into(),
            user_prompt: "user sk-testsecret123456789 unchanged".into(),
            project_path: PathBuf::from("."),
            temperature: None,
        };

        let output = OllamaAdapter::new().execute(&input, None).await.unwrap();
        let body = rx.recv().unwrap();
        server.join().unwrap();
        assert_eq!(body["messages"][0]["content"], "system sk-testsecret123456789 unchanged");
        assert_eq!(body["messages"][1]["content"], "user sk-testsecret123456789 unchanged");
        assert!(output.content.contains("[REDACTED_SECRET]"));
        assert!(output.raw_json.unwrap().contains("[REDACTED_SECRET]"));
    }
}
