use serde_json::{json, Value};
use std::env;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

use super::super::process_runner::HTTP_INFERENCE_SAFETY_TIMEOUT;
use super::super::secrets::SecretRedactor;
use super::{AdapterExecutionInput, AdapterExecutionOutput};

static ALLOWED_PROVIDERS: &[&str] = &[
    "mimo",
    "deepseek",
    "kimi",
    "kimi_code",
    "minimax",
    "openrouter",
];

#[derive(Debug, Clone, Default)]
pub struct ProviderAdapter {
    client: reqwest::Client,
    redactor: SecretRedactor,
    #[cfg(test)]
    endpoint_override: Option<String>,
    #[cfg(test)]
    api_key_override: Option<String>,
}

impl ProviderAdapter {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder().build().unwrap_or_default(),
            redactor: SecretRedactor::new(),
            #[cfg(test)]
            endpoint_override: None,
            #[cfg(test)]
            api_key_override: None,
        }
    }

    pub async fn execute(
        &self,
        input: &AdapterExecutionInput,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<AdapterExecutionOutput, String> {
        let profile = &input.profile;
        let provider_id = profile.provider_id.as_deref().ok_or_else(|| {
            "Provider profile is missing required provider_id; refusing to select a default provider.".to_string()
        })?;

        // Fail closed for unknown provider IDs before making any HTTP request
        if !ALLOWED_PROVIDERS.contains(&provider_id) {
            return Err(format!(
                "Unsupported provider ID '{}'. Allowed providers: {}",
                provider_id,
                ALLOWED_PROVIDERS.join(", ")
            ));
        }

        let model = profile.model.as_deref().unwrap_or("deepseek-v4.1-flash");
        let (endpoint, api_key_env) = resolve_provider_endpoint_and_env(provider_id)?;
        #[cfg(test)]
        let endpoint = self
            .endpoint_override
            .clone()
            .unwrap_or(endpoint);

        #[cfg(test)]
        let configured_api_key = self.api_key_override.clone();
        #[cfg(not(test))]
        let configured_api_key: Option<String> = None;
        let api_key = configured_api_key.or_else(|| env::var(&api_key_env).ok()).ok_or_else(|| {
            format!(
                "API key environment variable '{}' is not set for provider '{}'",
                api_key_env, provider_id
            )
        })?;

        if api_key.trim().is_empty() {
            return Err(format!(
                "API key '{}' is empty for provider '{}'",
                api_key_env, provider_id
            ));
        }

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
            body["temperature"] = json!(temp);
        }

        if let Some(ref tm) = profile.thinking_mode {
            if tm == "thinking" {
                body["thinking"] = json!({"type": "enabled"});
            } else if tm == "normal" {
                body["thinking"] = json!({"type": "disabled"});
            }
        }
        if let Some(ref re) = profile.reasoning_effort {
            body["reasoning_effort"] = json!(re);
        }

        let start = Instant::now();
        let mut req = self
            .client
            .post(&endpoint)
            .header("Authorization", format!("Bearer {}", api_key.trim()))
            .header("Content-Type", "application/json");

        if provider_id == "openrouter" {
            req = req
                .header("HTTP-Referer", "https://github.com/soheidon/anthro-bridge")
                .header("X-Title", "Anthro Bridge Orchestrator");
        }

        let cancel_fut = async {
            if let Some(token) = cancel_token {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        };

        // Wrap the entire lifecycle (send + headers + body download + parse) in timeout and cancellation
        let request_fut = async {
            let res = req.json(&body).send().await.map_err(|e| {
                format!(
                    "HTTP request to '{}' ({}) failed: {}",
                    provider_id, endpoint, e
                )
            })?;

            let status = res.status();
            let raw_text = res
                .text()
                .await
                .map_err(|e| format!("Failed to read response body: {}", e))?;

            if !status.is_success() {
                let redacted_err = self.redactor.redact_secrets(&raw_text);
                return Err(format!(
                    "Provider '{}' returned HTTP {}: {}",
                    provider_id,
                    status.as_u16(),
                    redacted_err
                ));
            }

            let parsed: Value = serde_json::from_str(&raw_text).map_err(|e| {
                format!(
                    "Failed to parse JSON response from '{}': {}",
                    provider_id, e
                )
            })?;

            let content = extract_content_from_response(&parsed).ok_or_else(|| {
                "Could not extract message content from provider response".to_string()
            })?;

            let tokens_used = parsed
                .get("usage")
                .and_then(|u| u.get("total_tokens"))
                .and_then(|t| t.as_u64());

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
                Err(format!("Inference request to provider '{}' timed out after 180 seconds.", provider_id))
            }
            _ = cancel_fut => {
                Err("Inference request cancelled by user.".to_string())
            }
            res = request_fut => {
                res
            }
        }
    }
}

fn resolve_provider_endpoint_and_env(provider_id: &str) -> Result<(String, String), String> {
    match provider_id {
        "mimo" => Ok((
            "https://api.xiaomimimo.com/v1/chat/completions".to_string(),
            "MIMO_API_KEY".to_string(),
        )),
        "kimi" => Ok((
            "https://api.moonshot.cn/v1/chat/completions".to_string(),
            "MOONSHOT_API_KEY".to_string(),
        )),
        "kimi_code" => Ok((
            "https://api.kimi.com/coding/v1/chat/completions".to_string(),
            "KIMI_CODE_API_KEY".to_string(),
        )),
        "minimax" => Ok((
            "https://api.minimax.chat/v1/text/chatcompletion_v2".to_string(),
            "MINIMAX_API_KEY".to_string(),
        )),
        "openrouter" => Ok((
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            "OPENROUTER_API_KEY".to_string(),
        )),
        "deepseek" => Ok((
            "https://api.deepseek.com/chat/completions".to_string(),
            "DEEPSEEK_API_KEY".to_string(),
        )),
        _ => Err(format!("Unknown provider ID '{}'", provider_id)),
    }
}

fn extract_content_from_response(json: &Value) -> Option<String> {
    if let Some(content) = json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|val| val.as_str())
    {
        return Some(content.to_string());
    }

    if let Some(text) = json
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|c0| c0.get("text"))
        .and_then(|val| val.as_str())
    {
        return Some(text.to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::types::{AgentRole, ExecutionAdapterType, OrchestratorProfile};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::mpsc;

    fn receive_request() -> (String, mpsc::Receiver<Option<Value>>, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/chat", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => {
                        connection
                            .0
                            .set_nonblocking(false)
                            .expect("failed to restore blocking mode on accepted test stream");
                        break connection;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            let _ = tx.send(None);
                            return;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(error) => panic!("HTTP test listener failed: {error}"),
                }
            };
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
            tx.send(Some(serde_json::from_slice(&body).unwrap())).unwrap();
            let response = r#"{"choices":[{"message":{"content":"sk-testsecret123456789"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
        });
        (endpoint, rx, handle)
    }

    fn input(provider_id: Option<&str>) -> AdapterExecutionInput {
        AdapterExecutionInput {
            role: AgentRole::Planner,
            profile: OrchestratorProfile {
                id: "test".into(),
                display_name: "Test".into(),
                adapter: ExecutionAdapterType::Provider,
                capabilities: vec![],
                provider_id: provider_id.map(str::to_owned),
                provider_profile_id: None,
                model: Some("test-model".into()),
                thinking_mode: None,
                reasoning_effort: None,
                ollama_model: None,
                ollama_endpoint: None,
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
        }
    }

    #[tokio::test]
    async fn provider_receives_original_prompts_while_output_is_redacted() {
        let (endpoint, rx, server) = receive_request();
        let adapter = ProviderAdapter {
            endpoint_override: Some(endpoint),
            api_key_override: Some("test-key".into()),
            ..ProviderAdapter::new()
        };
        let output = adapter.execute(&input(Some("deepseek")), None).await.unwrap();
        let body = rx.recv().unwrap().unwrap();
        server.join().unwrap();

        assert_eq!(body["messages"][0]["content"], "system sk-testsecret123456789 unchanged");
        assert_eq!(body["messages"][1]["content"], "user sk-testsecret123456789 unchanged");
        assert!(output.content.contains("[REDACTED_SECRET]"));
        assert!(output.raw_json.unwrap().contains("[REDACTED_SECRET]"));
    }

    #[tokio::test]
    async fn missing_or_unknown_provider_id_fails_before_http_request() {
        let (endpoint, rx, server) = receive_request();
        let adapter = ProviderAdapter {
            endpoint_override: Some(endpoint),
            api_key_override: Some("test-key".into()),
            ..ProviderAdapter::new()
        };

        let missing = adapter.execute(&input(None), None).await.unwrap_err();
        assert!(missing.contains("missing required provider_id"));
        let unknown = adapter
            .execute(&input(Some("unknown-provider")), None)
            .await
            .unwrap_err();
        assert!(unknown.contains("Unsupported provider ID"));

        drop(adapter);
        assert_eq!(rx.recv().unwrap(), None);
        server.join().unwrap();
    }
}
