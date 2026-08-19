//! DeepSeek provider backed by the Anthropic-compatible Messages API.
//!
//! The provider owns only the HTTP details (model, reasoning, auth, timeout).
//! It knows that a `system` prompt and a `user` prompt are sent separately, but
//! does not know the content of the planning prompts.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::{PlanResponse, PlannerProvider, ProviderError};

const DEFAULT_ENDPOINT: &str = "https://api.deepseek.com/anthropic/v1/messages";
const DEFAULT_MODEL: &str = "deepseek-v4-pro";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TOKENS: u32 = 8192;
const REASONING_EFFORT: &str = "high";

/// Sends planning requests to DeepSeek's Anthropic-compatible Messages API.
#[derive(Debug, Clone)]
pub struct DeepSeekProvider {
    client: reqwest::Client,
    api_key: String,
    endpoint: String,
    model: String,
    timeout: Duration,
}

impl DeepSeekProvider {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.into(),
            endpoint: DEFAULT_ENDPOINT.to_string(),
            model: DEFAULT_MODEL.to_string(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Override the endpoint (used by tests to point at a mock server).
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    /// Override the request timeout (used by tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Override the model id (used by tests).
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Redact the API key (and its `Bearer` form) from a provider message,
    /// then truncate. Providers can echo the offending key back in error
    /// bodies, so we strip it here rather than trusting the response.
    fn sanitize_provider_message(&self, message: String) -> String {
        if self.api_key.trim().is_empty() {
            return sanitize(message);
        }
        let redacted = message
            .replace(&format!("Bearer {}", self.api_key), "[REDACTED]")
            .replace(&self.api_key, "[REDACTED]");
        sanitize(redacted)
    }
}

impl PlannerProvider for DeepSeekProvider {
    async fn plan(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<PlanResponse, ProviderError> {
        if self.api_key.trim().is_empty() {
            return Err(ProviderError::Config("DEEPSEEK_API_KEY is not set".into()));
        }

        let request = DeepSeekRequest {
            model: &self.model,
            max_tokens: MAX_TOKENS,
            system: system_prompt,
            messages: vec![Message {
                role: "user",
                content: user_prompt,
            }],
            thinking: Thinking { r#type: "enabled" },
            output_config: OutputConfig {
                effort: REASONING_EFFORT,
            },
            stream: false,
        };

        let response = self
            .client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(classify_request_error)?;

        let status = response.status();
        if !status.is_success() {
            let message = self.sanitize_provider_message(response.text().await.unwrap_or_default());
            return Err(classify_http_error(status, message));
        }

        let body: DeepSeekResponse = response.json().await.map_err(|e| {
            ProviderError::Response(format!("failed to parse provider response: {e}"))
        })?;

        let text = body
            .content
            .into_iter()
            .filter(|block| block.r#type == "text")
            .filter_map(|block| block.text)
            .collect::<Vec<_>>()
            .join("\n");

        if text.trim().is_empty() {
            return Err(ProviderError::Response(
                "provider returned no text content".into(),
            ));
        }

        Ok(PlanResponse { text })
    }
}

fn classify_request_error(err: reqwest::Error) -> ProviderError {
    if err.is_timeout() {
        ProviderError::Timeout(err.to_string())
    } else {
        ProviderError::Network(err.to_string())
    }
}

fn classify_http_error(status: reqwest::StatusCode, message: String) -> ProviderError {
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        ProviderError::Auth(message)
    } else {
        ProviderError::Http {
            status: status.as_u16(),
            message,
        }
    }
}

/// Truncate provider-supplied error bodies so a pathological response cannot
/// bloat a log line or MCP error message.
fn sanitize(message: String) -> String {
    const MAX_LEN: usize = 500;
    if message.chars().count() > MAX_LEN {
        let mut truncated: String = message.chars().take(MAX_LEN).collect();
        truncated.push('\u{2026}');
        truncated
    } else {
        message
    }
}

#[derive(Debug, Serialize)]
struct DeepSeekRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
    thinking: Thinking,
    output_config: OutputConfig,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Serialize)]
struct Thinking {
    r#type: &'static str,
}

#[derive(Debug, Serialize)]
struct OutputConfig {
    effort: &'static str,
}

#[derive(Debug, Deserialize)]
struct DeepSeekResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    r#type: String,
    #[serde(default)]
    text: Option<String>,
}
