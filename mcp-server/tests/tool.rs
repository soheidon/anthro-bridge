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

fn review_args(task: &str, plan: &str, diff: &str, status: &str) -> CallToolRequestParams {
    let args = serde_json::json!({
        "task": task,
        "approved_plan": plan,
        "git_diff": diff,
        "git_status": status,
    });
    CallToolRequestParams::new("review").with_arguments(args.as_object().unwrap().clone())
}

#[tokio::test]
async fn exposes_plan_and_review_tools() {
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
    assert_eq!(tools.len(), 5);

    // 1. Verify plan tool
    let plan_tool = tools.iter().find(|t| t.name == "plan").expect("plan tool missing");
    assert!(plan_tool.description.as_ref().is_some_and(|d| !d.is_empty()));
    let plan_schema = &plan_tool.input_schema;
    assert_eq!(plan_schema.get("type").and_then(|v| v.as_str()), Some("object"));
    let plan_req = plan_schema.get("required").and_then(|v| v.as_array()).unwrap();
    assert!(plan_req.contains(&serde_json::json!("task")));
    assert!(plan_req.contains(&serde_json::json!("context")));
    assert!(!plan_req.contains(&serde_json::json!("constraints")));

    // 2. Verify review tool
    let review_tool = tools.iter().find(|t| t.name == "review").expect("review tool missing");
    assert!(review_tool.description.as_ref().is_some_and(|d| !d.is_empty()));
    let review_schema = &review_tool.input_schema;
    assert_eq!(review_schema.get("type").and_then(|v| v.as_str()), Some("object"));
    let review_req = review_schema.get("required").and_then(|v| v.as_array()).unwrap();
    assert!(review_req.contains(&serde_json::json!("task")));
    assert!(review_req.contains(&serde_json::json!("approved_plan")));
    assert!(review_req.contains(&serde_json::json!("git_diff")));
    assert!(review_req.contains(&serde_json::json!("git_status")));
    assert!(!review_req.contains(&serde_json::json!("test_results")));
    assert!(!review_req.contains(&serde_json::json!("review_mode")));
    assert!(!review_req.contains(&serde_json::json!("additional_context")));

    // 3. Verify orchestrator tools
    assert!(tools.iter().any(|t| t.name == "orchestrator_claim_task"));
    assert!(tools.iter().any(|t| t.name == "orchestrator_report_progress"));
    assert!(tools.iter().any(|t| t.name == "orchestrator_submit_result"));

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

#[tokio::test]
async fn valid_review_call_returns_verdict() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "# Review Summary\n\nDecision: Approved\nCommit readiness: READY".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let result = client
        .call_tool(review_args("review task", "Plan content", "diff content", "M file.ts"))
        .await
        .unwrap();

    assert_eq!(result.is_error, Some(false));
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.as_str());
    assert_eq!(text, Some("# Review Summary\n\nDecision: Approved\nCommit readiness: READY"));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn review_requires_git_status_and_diff() {
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

    // Empty git_status rejected
    let err = client
        .call_tool(review_args("task", "plan", "diff", "   "))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("`git_status` must not be empty"));

    // Empty git_diff rejected
    let err = client
        .call_tool(review_args("task", "plan", "  ", "M file.ts"))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("`git_diff` must not be empty"));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn review_three_tier_verdict_contract_test() {
    let test_cases = vec![
        (
            "# Review Summary\n\n- Scope Compliance: Fully compliant\n\n## Decision\nDecision: Approved\nCommit readiness: READY\n",
            "Decision: Approved",
            "Commit readiness: READY",
        ),
        (
            "# Review Summary\n\n- Scope Compliance: Fully compliant\n\n## Minor Recommendations (Non-blocking)\n- Consider renaming var `t` to `timer`\n\n## Decision\nDecision: Approved with recommendations\nCommit readiness: READY\n",
            "Decision: Approved with recommendations",
            "Commit readiness: READY",
        ),
        (
            "# Review Summary\n\n- Scope Compliance: Violation detected\n\n## Blocking Issues\n- 1. Missing null check causes panic in edge case\n\n## Decision\nDecision: Not approved\nCommit readiness: NOT READY\n",
            "Decision: Not approved",
            "Commit readiness: NOT READY",
        ),
    ];

    for (output_text, expected_decision, expected_readiness) in test_cases {
        let (server_transport, client_transport) = tokio::io::duplex(4096);
        let tool = PlannerTool::new(FakeProvider {
            text: output_text.into(),
        });
        tokio::spawn(async move {
            if let Ok(service) = tool.serve(server_transport).await {
                let _ = service.waiting().await;
            }
        });

        let client = DummyClientHandler.serve(client_transport).await.unwrap();

        let result = client
            .call_tool(review_args("task", "plan", "diff", "M file.ts"))
            .await
            .unwrap();

        assert_eq!(result.is_error, Some(false));
        let text = result
            .content
            .first()
            .and_then(|c| c.as_text())
            .map(|t| t.text.as_str())
            .unwrap();

        assert!(text.contains(expected_decision));
        assert!(text.contains(expected_readiness));

        let _ = client.cancel().await;
    }
}

#[tokio::test]
async fn review_validates_review_mode_parameter() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "ok".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    // Unsupported mode rejected
    let mut args = serde_json::json!({
        "task": "task",
        "approved_plan": "plan",
        "git_diff": "diff",
        "git_status": "M file.ts",
        "review_mode": "strict"
    });
    let req = CallToolRequestParams::new("review").with_arguments(args.as_object().unwrap().clone());
    let err = client.call_tool(req).await.unwrap_err();
    assert!(err.to_string().contains("Invalid `review_mode`: 'strict'"));

    // Supported deep mode accepted
    args["review_mode"] = serde_json::json!("deep");
    let req = CallToolRequestParams::new("review").with_arguments(args.as_object().unwrap().clone());
    let res = client.call_tool(req).await.unwrap();
    assert_eq!(res.is_error, Some(false));

    let _ = client.cancel().await;
}
