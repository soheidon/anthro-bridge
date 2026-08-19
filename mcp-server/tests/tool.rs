use anthro_bridge_mcp_server::mcp::PlannerTool;
use anthro_bridge_mcp_server::provider::{PlanResponse, PlannerProvider, ProviderError};
use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::{ClientHandler, ServiceExt};

struct FakeProvider {
    text: String,
}

impl PlannerProvider for FakeProvider {
    async fn plan(&self, _system: &str, _user: &str) -> Result<PlanResponse, ProviderError> {
        Ok(PlanResponse {
            text: self.text.clone(),
        })
    }
}

struct FailingProvider;

impl PlannerProvider for FailingProvider {
    async fn plan(&self, _system: &str, _user: &str) -> Result<PlanResponse, ProviderError> {
        Err(ProviderError::Timeout("simulated timeout".into()))
    }
}

#[derive(Debug, Clone)]
struct DummyClientHandler;

impl ClientHandler for DummyClientHandler {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

fn plan_args(task: &str, context: &str) -> CallToolRequestParams {
    let args = serde_json::json!({ "task": task, "context": context });
    CallToolRequestParams::new("plan").with_arguments(args.as_object().unwrap().clone())
}

#[tokio::test]
async fn exposes_exactly_the_plan_tool() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "unused".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 1);

    let tool = &tools[0];
    assert_eq!(tool.name, "plan");
    assert!(tool.description.as_ref().is_some_and(|d| !d.is_empty()));

    let schema = &tool.input_schema;
    assert_eq!(schema.get("type").and_then(|v| v.as_str()), Some("object"));

    let required = schema.get("required").and_then(|v| v.as_array()).unwrap();
    assert!(required.contains(&serde_json::json!("task")));
    assert!(required.contains(&serde_json::json!("context")));
    assert!(!required.contains(&serde_json::json!("constraints")));

    let properties = schema
        .get("properties")
        .and_then(|v| v.as_object())
        .unwrap();
    assert!(properties.contains_key("task"));
    assert!(properties.contains_key("context"));
    assert!(properties.contains_key("constraints"));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn valid_call_returns_plan_as_text_content() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "1. Do X\n2. Do Y".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let result = client
        .call_tool(plan_args("add a button", "Button.tsx"))
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false));
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str());
    assert_eq!(text, Some("1. Do X\n2. Do Y"));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn empty_task_is_rejected() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "unused".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let err = client.call_tool(plan_args("   ", "ctx")).await.unwrap_err();
    assert!(err.to_string().contains("must not be empty"));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn provider_error_becomes_clean_mcp_error() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FailingProvider);
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let err = client.call_tool(plan_args("t", "c")).await.unwrap_err();
    assert!(!err.to_string().contains("Bearer"));

    let _ = client.cancel().await;
}
