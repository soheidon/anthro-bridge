//! DynamicBridgeProvider: resolves target provider, profile, model, and reasoning
//! settings from Anthro Bridge's `config.json` and dispatches planning requests.

use std::env;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use super::{PlanResponse, PlannerProvider, ProviderError};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_TOKENS: u32 = 8192;

/// Resolves the APPDATA configuration path for Anthro Bridge.
pub fn resolve_anthro_bridge_config_path() -> PathBuf {
    let appdata = env::var("APPDATA")
        .or_else(|_| env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());

    let channel_dir = match option_env!("ANTHRO_BRIDGE_CHANNEL") {
        Some("stable") => "Anthro Bridge",
        Some("dev") | None => "Anthro Bridge Dev",
        Some(_) => "Anthro Bridge Dev",
    };

    let p = PathBuf::from(appdata).join(channel_dir).join("config.json");
    if p.exists() {
        return p;
    }

    // Fallback: check stable directory if dev does not exist
    let p_stable = PathBuf::from(env::var("APPDATA").unwrap_or_default())
        .join("Anthro Bridge")
        .join("config.json");
    if p_stable.exists() {
        return p_stable;
    }

    p
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ConfigMcpSection {
    provider: Option<String>,
    profile_id: Option<String>,
    model: Option<String>,
    thinking_mode: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProviderEntry {
    upstream_url: String,
    api_key_env: String,
    #[serde(default)]
    default_model: String,
    #[serde(default)]
    profiles: Vec<ProfileEntry>,
    #[serde(default)]
    models: std::collections::HashMap<String, ModelEntryRaw>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProfileEntry {
    id: String,
    #[allow(dead_code)]
    display_name: String,
    #[serde(default)]
    models: std::collections::HashMap<String, ModelEntryRaw>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelEntryRaw {
    upstream_model: String,
    thinking_mode: Option<String>,
    reasoning_effort: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BridgeConfig {
    #[serde(default)]
    active_provider: Option<String>,
    #[serde(default)]
    active_openrouter_profile_id: Option<String>,
    #[serde(default)]
    providers: std::collections::HashMap<String, ProviderEntry>,
    #[serde(default)]
    mcp: Option<ConfigMcpSection>,
}

#[derive(Debug, Clone)]
pub struct ResolvedMcpTarget {
    pub provider_id: String,
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub thinking_mode: Option<String>,
    pub reasoning_effort: Option<String>,
    pub is_openrouter: bool,
}

pub fn resolve_mcp_target(config_path: &std::path::Path) -> Result<ResolvedMcpTarget, ProviderError> {
    if !config_path.exists() {
        // Fallback to legacy environment variable if config.json not yet created
        let api_key = env::var("DEEPSEEK_API_KEY").unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err(ProviderError::Config(format!(
                "Anthro Bridge config not found at {} and DEEPSEEK_API_KEY is not set",
                config_path.display()
            )));
        }
        return Ok(ResolvedMcpTarget {
            provider_id: "deepseek".to_string(),
            endpoint: "https://api.deepseek.com/anthropic/v1/messages".to_string(),
            api_key,
            model: "deepseek-v4-pro".to_string(),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: Some("high".to_string()),
            is_openrouter: false,
        });
    }

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| ProviderError::Config(format!("Failed to read {}: {e}", config_path.display())))?;
    let cfg: BridgeConfig = serde_json::from_str(&content)
        .map_err(|e| ProviderError::Config(format!("Failed to parse {}: {e}", config_path.display())))?;

    let mcp = cfg.mcp.unwrap_or_default();
    let provider_id = mcp
        .provider
        .clone()
        .or_else(|| cfg.active_provider.clone())
        .unwrap_or_else(|| "deepseek".to_string());

    let provider = cfg.providers.get(&provider_id).ok_or_else(|| {
        ProviderError::Config(format!("Provider '{}' not configured in config.json", provider_id))
    })?;

    let api_key = env::var(&provider.api_key_env).unwrap_or_default();
    if api_key.trim().is_empty() {
        return Err(ProviderError::Config(format!(
            "API key environment variable '{}' for provider '{}' is not set or is empty",
            provider.api_key_env, provider_id
        )));
    }

    let is_openrouter = provider_id == "openrouter";

    let (model, tm, re) = if is_openrouter {
        let active_prof = if let Some(ref pid) = mcp.profile_id {
            provider.profiles.iter().find(|p| p.id == *pid)
        } else if let Some(ref pid) = cfg.active_openrouter_profile_id {
            provider.profiles.iter().find(|p| p.id == *pid)
        } else {
            provider.profiles.first()
        };

        let m_name = mcp.model.clone().or_else(|| {
            active_prof.and_then(|p| p.models.get("claude-opus-5").map(|m| m.upstream_model.clone()))
        }).unwrap_or_else(|| "deepseek/deepseek-r1".to_string());

        let tm = mcp.thinking_mode.clone().or_else(|| {
            active_prof.and_then(|p| p.models.get("claude-opus-5").and_then(|m| m.thinking_mode.clone()))
        });

        let re = mcp.reasoning_effort.clone().or_else(|| {
            active_prof.and_then(|p| p.models.get("claude-opus-5").and_then(|m| m.reasoning_effort.clone()))
        });

        (m_name, tm, re)
    } else {
        let m_name = mcp.model.clone().or_else(|| {
            provider.models.get("claude-opus-5").map(|m| m.upstream_model.clone())
        }).unwrap_or_else(|| provider.default_model.clone());

        let tm = mcp.thinking_mode.clone().or_else(|| {
            provider.models.get("claude-opus-5").and_then(|m| m.thinking_mode.clone())
        });

        let re = mcp.reasoning_effort.clone().or_else(|| {
            provider.models.get("claude-opus-5").and_then(|m| m.reasoning_effort.clone())
        });

        (m_name, tm, re)
    };

    let base_url = provider.upstream_url.trim_end_matches('/');
    let endpoint = if base_url.ends_with("/v1/messages") {
        base_url.to_string()
    } else if base_url.ends_with("/v1") {
        format!("{base_url}/messages")
    } else {
        format!("{base_url}/v1/messages")
    };

    Ok(ResolvedMcpTarget {
        provider_id,
        endpoint,
        api_key,
        model,
        thinking_mode: tm,
        reasoning_effort: re,
        is_openrouter,
    })
}

/// Dynamic MCP planner provider that reads `config.json` on each call (or uses injectable target).
#[derive(Debug, Clone)]
pub struct DynamicBridgeProvider {
    client: reqwest::Client,
    config_path: Option<PathBuf>,
    override_target: Option<ResolvedMcpTarget>,
    timeout: Duration,
}

impl DynamicBridgeProvider {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            config_path: Some(resolve_anthro_bridge_config_path()),
            override_target: None,
            timeout: DEFAULT_TIMEOUT,
        }
    }

    pub fn with_config_path(mut self, path: PathBuf) -> Self {
        self.config_path = Some(path);
        self
    }

    pub fn with_target(mut self, target: ResolvedMcpTarget) -> Self {
        self.override_target = Some(target);
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn sanitize_provider_message(&self, message: String, api_key: &str) -> String {
        if api_key.trim().is_empty() {
            return sanitize(message);
        }
        let redacted = message
            .replace(&format!("Bearer {}", api_key), "[REDACTED]")
            .replace(api_key, "[REDACTED]");
        sanitize(redacted)
    }
}

impl Default for DynamicBridgeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PlannerProvider for DynamicBridgeProvider {
    async fn plan(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<PlanResponse, ProviderError> {
        let target = if let Some(ref t) = self.override_target {
            t.clone()
        } else if let Some(ref p) = self.config_path {
            resolve_mcp_target(p)?
        } else {
            return Err(ProviderError::Config("No configuration path set".into()));
        };

        if target.api_key.trim().is_empty() {
            return Err(ProviderError::Config(format!(
                "API key for provider '{}' is not set",
                target.provider_id
            )));
        }

        let is_thinking = target.thinking_mode.as_deref() == Some("thinking")
            || target.thinking_mode.as_deref() == Some("thinking_only");

        let mut req_body = serde_json::json!({
            "model": target.model,
            "max_tokens": MAX_TOKENS,
            "system": system_prompt,
            "messages": [
                {
                    "role": "user",
                    "content": user_prompt
                }
            ],
            "stream": false
        });

        match target.provider_id.as_str() {
            "deepseek" => {
                if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                    if let Some(ref effort) = target.reasoning_effort {
                        if !effort.trim().is_empty() {
                            req_body["output_config"] = serde_json::json!({ "effort": effort });
                        }
                    }
                }
            }
            "mimo" => {
                if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                }
            }
            "minimax" => {
                if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                }
                // Do NOT send output_config to MiniMax
            }
            "kimi" => {
                let is_k3 = target.model.contains("k3");
                if is_k3 {
                    // K3: do NOT send thinking parameter; send reasoning_effort directly
                    let effort = target
                        .reasoning_effort
                        .as_deref()
                        .filter(|e| !e.trim().is_empty())
                        .unwrap_or("max");
                    req_body["reasoning_effort"] = serde_json::json!(effort);
                } else if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                }
            }
            "openrouter" => {
                let is_poolside = target.model.contains("laguna") || target.model.contains("poolside");
                if is_poolside {
                    if is_thinking {
                        if target.reasoning_effort.as_deref() == Some("max") {
                            req_body["reasoning"] = serde_json::json!({ "effort": "max" });
                        } else {
                            req_body["reasoning"] = serde_json::json!({ "enabled": true });
                        }
                    } else {
                        req_body["reasoning"] = serde_json::json!({ "enabled": false });
                    }
                } else if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                    if let Some(ref effort) = target.reasoning_effort {
                        if !effort.trim().is_empty() {
                            req_body["reasoning"] = serde_json::json!({ "effort": effort });
                        }
                    }
                }
            }
            _ => {
                // Unknown/fallback provider
                if is_thinking {
                    req_body["thinking"] = serde_json::json!({ "type": "enabled" });
                }
            }
        }

        let response = self
            .client
            .post(&target.endpoint)
            .header("Authorization", format!("Bearer {}", target.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(classify_request_error)?;

        let status = response.status();
        if !status.is_success() {
            let message = self.sanitize_provider_message(response.text().await.unwrap_or_default(), &target.api_key);
            return Err(classify_http_error(status, message));
        }

        let body: AnthropicMessagesResponse = response.json().await.map_err(|e| {
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

#[derive(Debug, Deserialize)]
struct AnthropicMessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    r#type: String,
    #[serde(default)]
    text: Option<String>,
}
