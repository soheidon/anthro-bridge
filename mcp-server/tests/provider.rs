use std::time::Duration;

use anthro_bridge_mcp_server::provider::deepseek::DeepSeekProvider;
use anthro_bridge_mcp_server::provider::{PlannerProvider, ProviderError};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn mock_provider(endpoint: &str, api_key: &str) -> DeepSeekProvider {
    DeepSeekProvider::new(api_key).with_endpoint(format!("{endpoint}/v1/messages"))
}

#[tokio::test]
async fn successful_plan_extracts_text_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "content": [
                { "type": "thinking", "thinking": "hidden reasoning" },
                { "type": "text", "text": "Step 1: add the button" },
                { "type": "text", "text": "Step 2: write a test" }
            ]
        })))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key");
    let result = provider.plan("system", "user").await.unwrap();

    assert_eq!(result.text, "Step 1: add the button\nStep 2: write a test");
}

#[tokio::test]
async fn request_includes_model_effort_auth_and_split_prompts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "ok" }]
        })))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key");
    let result = provider.plan("system prompt", "user prompt").await.unwrap();

    assert_eq!(result.text, "ok");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);

    let request = &requests[0];
    assert_eq!(
        request
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok()),
        Some("Bearer test-key")
    );

    let body: serde_json::Value = request.body_json().unwrap();
    assert_eq!(body["model"], "deepseek-v4-pro");
    assert_eq!(body["system"], "system prompt");
    assert_eq!(body["messages"][0]["role"], "user");
    assert_eq!(body["messages"][0]["content"], "user prompt");
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["stream"], false);
    assert!(body["max_tokens"].is_number());
}

#[tokio::test]
async fn non_2xx_becomes_http_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal failure"))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key");
    let err = provider.plan("system", "user").await.unwrap_err();

    match err {
        ProviderError::Http { status, .. } => assert_eq!(status, 500),
        other => panic!("expected Http error, got {other:?}"),
    }
}

#[tokio::test]
async fn unauthorized_becomes_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("invalid api key"))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "bad-key");
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(matches!(err, ProviderError::Auth(_)));
}

#[tokio::test]
async fn api_key_never_appears_in_error_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let key = "super-secret-api-key";
    let provider = mock_provider(&server.uri(), key);
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(!err.to_string().contains(key));
    assert!(!err.to_string().contains("Bearer"));
}

#[tokio::test]
async fn api_key_in_response_body_is_redacted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_string("authentication failed for super-secret-api-key"),
        )
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "super-secret-api-key");
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(!err.to_string().contains("super-secret-api-key"));
    assert!(!err.to_string().contains("Bearer"));
}

#[tokio::test]
async fn timeout_becomes_timeout_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"content": [{"type": "text", "text": "ok"}]}))
                .set_delay(Duration::from_millis(300)),
        )
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key").with_timeout(Duration::from_millis(50));
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(matches!(err, ProviderError::Timeout(_)));
}

#[tokio::test]
async fn malformed_response_becomes_response_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key");
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(matches!(err, ProviderError::Response(_)));
}

#[tokio::test]
async fn response_without_text_becomes_response_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "thinking", "thinking": "hidden" }]
        })))
        .mount(&server)
        .await;

    let provider = mock_provider(&server.uri(), "test-key");
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(matches!(err, ProviderError::Response(_)));
}

#[tokio::test]
async fn empty_api_key_becomes_config_error() {
    let provider = DeepSeekProvider::new("");
    let err = provider.plan("system", "user").await.unwrap_err();

    assert!(matches!(err, ProviderError::Config(_)));
}

#[tokio::test]
async fn dynamic_bridge_provider_dispatches_configured_target() {
    use anthro_bridge_mcp_server::provider::adapter::{DynamicBridgeProvider, resolve_mcp_target};
    use std::io::Write;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "Plan from OpenRouter Gemini" }]
        })))
        .mount(&server)
        .await;

    // Create a temporary config file pointing to our mock server
    let temp_dir = std::env::temp_dir();
    let config_file = temp_dir.join(format!("test_config_{}.json", std::process::id()));

    std::env::set_var("TEST_OPENROUTER_KEY", "sk-openrouter-dynamic-key");

    let cfg_json = serde_json::json!({
        "config_version": "1.0",
        "active_provider": "openrouter",
        "providers": {
            "openrouter": {
                "display_name": "OpenRouter",
                "upstream_url": format!("{}/v1/messages", server.uri()),
                "api_key_env": "TEST_OPENROUTER_KEY",
                "default_model": "google/gemini-2.5-flash",
                "profiles": [
                    {
                        "id": "gemini_prof",
                        "display_name": "Gemini Profile",
                        "models": {
                            "claude-opus-5": {
                                "upstream_model": "google/gemini-2.5-pro",
                                "thinking_mode": "thinking",
                                "reasoning_effort": "low"
                            }
                        }
                    }
                ]
            }
        },
        "mcp": {
            "provider": "openrouter",
            "profile_id": "gemini_prof",
            "model": "google/gemini-2.5-pro",
            "thinking_mode": "thinking",
            "reasoning_effort": "low"
        }
    });

    let mut file = std::fs::File::create(&config_file).unwrap();
    file.write_all(cfg_json.to_string().as_bytes()).unwrap();

    let target = resolve_mcp_target(&config_file).unwrap();
    assert_eq!(target.provider_id, "openrouter");
    assert_eq!(target.model, "google/gemini-2.5-pro");
    assert_eq!(target.thinking_mode.as_deref(), Some("thinking"));
    assert_eq!(target.reasoning_effort.as_deref(), Some("low"));
    assert_eq!(target.api_key, "sk-openrouter-dynamic-key");

    let provider = DynamicBridgeProvider::new().with_config_path(config_file.clone());
    let response = provider.plan("sys", "user").await.unwrap();
    assert_eq!(response.text, "Plan from OpenRouter Gemini");

    let _ = std::fs::remove_file(&config_file);
}

#[tokio::test]
async fn openrouter_gemini_uses_reasoning_and_never_deepseek_output_config() {
    use anthro_bridge_mcp_server::provider::adapter::{DynamicBridgeProvider, ResolvedMcpTarget};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "Plan from OpenRouter Gemini" }]
        })))
        .mount(&server)
        .await;

    let target = ResolvedMcpTarget {
        provider_id: "openrouter".to_string(),
        endpoint: format!("{}/v1/messages", server.uri()),
        api_key: "openrouter-test-key".to_string(),
        model: "google/gemini-2.5-flash".to_string(),
        thinking_mode: Some("thinking".to_string()),
        reasoning_effort: Some("medium".to_string()),
        is_openrouter: true,
    };

    let provider = DynamicBridgeProvider::new().with_target(target);
    let response = provider.plan("sys", "user").await.unwrap();
    assert_eq!(response.text, "Plan from OpenRouter Gemini");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], "google/gemini-2.5-flash");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["reasoning"]["effort"], "medium");
    assert!(body.get("output_config").is_none(), "DeepSeek output_config must NOT be attached to OpenRouter requests");
}

#[tokio::test]
async fn deepseek_uses_output_config_effort() {
    use anthro_bridge_mcp_server::provider::adapter::{DynamicBridgeProvider, ResolvedMcpTarget};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "Plan from DeepSeek" }]
        })))
        .mount(&server)
        .await;

    let target = ResolvedMcpTarget {
        provider_id: "deepseek".to_string(),
        endpoint: format!("{}/v1/messages", server.uri()),
        api_key: "deepseek-test-key".to_string(),
        model: "deepseek-v4-pro".to_string(),
        thinking_mode: Some("thinking".to_string()),
        reasoning_effort: Some("high".to_string()),
        is_openrouter: false,
    };

    let provider = DynamicBridgeProvider::new().with_target(target);
    let response = provider.plan("sys", "user").await.unwrap();
    assert_eq!(response.text, "Plan from DeepSeek");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], "deepseek-v4-pro");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert_eq!(body["output_config"]["effort"], "high");
}

#[tokio::test]
async fn kimi_k3_suppresses_thinking_and_sends_reasoning_effort() {
    use anthro_bridge_mcp_server::provider::adapter::{DynamicBridgeProvider, ResolvedMcpTarget};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "Plan from Kimi K3" }]
        })))
        .mount(&server)
        .await;

    let target = ResolvedMcpTarget {
        provider_id: "kimi".to_string(),
        endpoint: format!("{}/v1/messages", server.uri()),
        api_key: "kimi-test-key".to_string(),
        model: "kimi-k3".to_string(),
        thinking_mode: Some("thinking_only".to_string()),
        reasoning_effort: Some("max".to_string()),
        is_openrouter: false,
    };

    let provider = DynamicBridgeProvider::new().with_target(target);
    let response = provider.plan("sys", "user").await.unwrap();
    assert_eq!(response.text, "Plan from Kimi K3");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], "kimi-k3");
    assert!(body.get("thinking").is_none());
    assert_eq!(body["reasoning_effort"], "max");
}

#[tokio::test]
async fn minimax_omits_output_config() {
    use anthro_bridge_mcp_server::provider::adapter::{DynamicBridgeProvider, ResolvedMcpTarget};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{ "type": "text", "text": "Plan from MiniMax" }]
        })))
        .mount(&server)
        .await;

    let target = ResolvedMcpTarget {
        provider_id: "minimax".to_string(),
        endpoint: format!("{}/v1/messages", server.uri()),
        api_key: "minimax-test-key".to_string(),
        model: "MiniMax-M3".to_string(),
        thinking_mode: Some("thinking_only".to_string()),
        reasoning_effort: Some("high".to_string()),
        is_openrouter: false,
    };

    let provider = DynamicBridgeProvider::new().with_target(target);
    let response = provider.plan("sys", "user").await.unwrap();
    assert_eq!(response.text, "Plan from MiniMax");

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let body: serde_json::Value = requests[0].body_json().unwrap();
    assert_eq!(body["model"], "MiniMax-M3");
    assert_eq!(body["thinking"]["type"], "enabled");
    assert!(body.get("output_config").is_none());
}
