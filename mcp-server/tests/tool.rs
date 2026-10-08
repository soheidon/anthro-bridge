use anthro_bridge_mcp_server::mcp::PlannerTool;
use anthro_bridge_mcp_server::provider::{PlanResponse, PlannerProvider, ProviderError};
use rmcp::model::{CallToolRequestParams, ClientInfo};
use rmcp::{ClientHandler, ServiceExt};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

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

struct CountingProvider {
    calls: Arc<AtomicUsize>,
    captured_users: Arc<Mutex<Vec<String>>>,
    text: String,
}

impl PlannerProvider for CountingProvider {
    async fn plan(&self, _system: &str, user: &str) -> Result<PlanResponse, ProviderError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.captured_users.lock().unwrap().push(user.to_string());
        Ok(PlanResponse {
            text: format!("{}\n{}", self.text, user),
        })
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
    let frozen_schema = &plan_schema["properties"]["frozen_plan"];
    assert!(frozen_schema["anyOf"].as_array().unwrap().iter().any(|variant| {
        variant["$ref"] == "#/$defs/StructuredFrozenPlanPayload"
    }));
    let typed_frozen_schema = &plan_schema["$defs"]["StructuredFrozenPlanPayload"];
    assert_eq!(typed_frozen_schema["type"], "object");
    assert_eq!(typed_frozen_schema["properties"]["schemaVersion"]["type"], "integer");
    assert_eq!(typed_frozen_schema["properties"]["contentDigest"]["type"], "string");

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

#[tokio::test]
async fn plan_validates_structured_plan_context() {
    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let tool = PlannerTool::new(FakeProvider {
        text: "ok plan".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    // 1. Invalid unstructured string rejected
    let bad_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": "free-form unstructured text not a json object"
    });
    let req = CallToolRequestParams::new("plan").with_arguments(bad_args.as_object().unwrap().clone());
    let err = client.call_tool(req).await.unwrap_err();
    assert!(err.to_string().contains("must be a valid serialized PlanContext JSON object"));

    // 2. Valid structured JSON accepted
    let valid_plan_context = serde_json::json!({
        "projectRootIdentity": "canonical-identity",
        "planDirectory": ".plan",
        "applicationVersion": "0.23.0",
        "planSeriesVersion": "0.24.0",
        "activeSupplementalPlans": [],
        "resolverStatus": "resolved"
    });
    let good_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string()
    });
    let req = CallToolRequestParams::new("plan").with_arguments(good_args.as_object().unwrap().clone());
    let res = client.call_tool(req).await.unwrap();
    assert_eq!(res.is_error, Some(false));

    let _ = client.cancel().await;
}

#[tokio::test]
async fn plan_validates_frozen_plan_payload() {
    use sha2::{Digest, Sha256};

    let (server_transport, client_transport) = tokio::io::duplex(4096);

    let provider_calls = Arc::new(AtomicUsize::new(0));
    let captured_users = Arc::new(Mutex::new(Vec::new()));
    let tool = PlannerTool::new(CountingProvider {
        calls: Arc::clone(&provider_calls),
        captured_users: Arc::clone(&captured_users),
        text: "ok plan".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });

    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    let primary_source_content = "# V0.24.0 Plan\n\nPhase A Implementation";
    let primary_source_digest = format!("{:x}", Sha256::digest(primary_source_content.as_bytes()));
    let effective_plan_content = format!(
        "<<<PLAN_SOURCE kind=\"primary\" id=\"V0.24.0-r22\">>>\n{}\n<<<END_PLAN_SOURCE>>>",
        primary_source_content
    );
    let content_digest = format!("{:x}", Sha256::digest(effective_plan_content.as_bytes()));
    let mut identity_hasher = Sha256::new();
    identity_hasher.update(b"V0.24.0-r22");
    identity_hasher.update(b"\0");
    identity_hasher.update(primary_source_digest.as_bytes());
    identity_hasher.update(b"\0");
    let effective_digest = format!("{:x}", identity_hasher.finalize());

    let valid_plan_context = serde_json::json!({
        "projectRootIdentity": "canonical-identity",
        "planDirectory": ".plan",
        "applicationVersion": "0.23.0",
        "planSeriesVersion": "0.24.0",
        "currentPrimaryPlan": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_source_digest,
            "revision": 22
        },
        "activeSupplementalPlans": [],
        "currentLeafPlanId": "V0.24.0-r22",
        "effectivePlanDigest": effective_digest,
        "resolverStatus": "resolved"
    });

    // 1. Opaque JSON strings are no longer accepted by the structured tool schema.
    let bad_json_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "frozen_plan": "not a json object"
    });
    let req = CallToolRequestParams::new("plan").with_arguments(bad_json_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.to_lowercase().contains("deserialize"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);

    // 2. Hash mismatch between effective_plan_content and content_digest rejected
    let bad_digest_frozen = serde_json::json!({
        "schemaVersion": 1,
        "effectivePlanContent": effective_plan_content,
        "contentDigest": "bad_digest_here",
        "effectivePlanDigest": effective_digest,
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_source_digest,
            "revision": 22
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r22",
                "path": ".plan/V0.24.0-r22.md",
                "sourceDigest": primary_source_digest,
                "content": primary_source_content
            }
        ]
    });
    let bad_digest_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string(),
        "frozen_plan": bad_digest_frozen
    });
    let req = CallToolRequestParams::new("plan").with_arguments(bad_digest_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("content_digest"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);

    // 3. Plan-bound requests cannot omit their frozen payload.
    let missing_payload_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string()
    });
    let req = CallToolRequestParams::new("plan").with_arguments(missing_payload_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("frozen_plan` is required"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);

    // 4. A mismatched effective identity digest is rejected independently from content_digest.
    let bad_effective_frozen = serde_json::json!({
        "schemaVersion": 1,
        "effectivePlanContent": effective_plan_content,
        "contentDigest": content_digest,
        "effectivePlanDigest": "wrong-identity-digest",
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_source_digest,
            "revision": 22
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r22",
                "path": ".plan/V0.24.0-r22.md",
                "sourceDigest": primary_source_digest,
                "content": primary_source_content
            }
        ]
    });
    let bad_effective_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string(),
        "frozen_plan": bad_effective_frozen
    });
    let req = CallToolRequestParams::new("plan").with_arguments(bad_effective_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("does not match `plan_context.effective_plan_digest`"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);

    // 5. Mismatch with plan_context primary plan rejected.
    let mismatched_primary_frozen = serde_json::json!({
        "schemaVersion": 1,
        "effectivePlanContent": effective_plan_content,
        "contentDigest": content_digest,
        "effectivePlanDigest": effective_digest,
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r21",
            "path": ".plan/V0.24.0-r21.md",
            "digest": "digest-0",
            "revision": 21
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r21",
                "path": ".plan/V0.24.0-r21.md",
                "sourceDigest": primary_source_digest,
                "content": primary_source_content
            }
        ]
    });
    let mismatched_primary_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string(),
        "frozen_plan": mismatched_primary_frozen
    });
    let req = CallToolRequestParams::new("plan").with_arguments(mismatched_primary_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("primary source mismatch with `plan_context`"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 0);

    // 6. Valid matching structured payload is accepted and reaches the provider with the exact body.
    let valid_frozen = serde_json::json!({
        "schemaVersion": 1,
        "effectivePlanContent": effective_plan_content,
        "contentDigest": content_digest,
        "effectivePlanDigest": effective_digest,
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_source_digest,
            "revision": 22
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r22",
                "path": ".plan/V0.24.0-r22.md",
                "sourceDigest": primary_source_digest,
                "content": primary_source_content
            }
        ]
    });
    let good_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string(),
        "frozen_plan": valid_frozen
    });
    let req = CallToolRequestParams::new("plan").with_arguments(good_args.as_object().unwrap().clone());
    let res = client.call_tool(req).await.unwrap();
    assert_eq!(res.is_error, Some(false));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    assert!(captured_users.lock().unwrap()[0].contains(&effective_plan_content));

    // 7. Unsupported schema versions fail before another provider invocation.
    let unsupported_frozen = serde_json::json!({
        "schemaVersion": 99,
        "effectivePlanContent": effective_plan_content,
        "contentDigest": content_digest,
        "effectivePlanDigest": effective_digest,
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_source_digest,
            "revision": 22
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r22",
                "path": ".plan/V0.24.0-r22.md",
                "sourceDigest": primary_source_digest,
                "content": primary_source_content
            }
        ]
    });
    let unsupported_args = serde_json::json!({
        "task": "do something",
        "context": "some context",
        "plan_context": valid_plan_context.to_string(),
        "frozen_plan": unsupported_frozen
    });
    let req = CallToolRequestParams::new("plan").with_arguments(unsupported_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(req).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("schema_version` is unsupported"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    let _ = client.cancel().await;
}

#[tokio::test]
async fn plan_accepts_shared_tauri_frozen_payload_fixture() {
    use anthro_bridge_mcp_server::mcp::StructuredFrozenPlanPayload;

    let payload: StructuredFrozenPlanPayload = serde_json::from_str(include_str!(
        "fixtures/tauri_frozen_plan_payload.json"
    ))
    .unwrap();
    let plan_context = serde_json::json!({
        "projectRootIdentity": "fixture-root",
        "planDirectory": ".plan",
        "applicationVersion": "0.23.0",
        "planSeriesVersion": "0.24.0",
        "currentPrimaryPlan": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": payload.primary_plan_identity_and_digest.as_ref().unwrap().digest,
            "revision": 22
        },
        "activeSupplementalPlans": [],
        "currentLeafPlanId": "V0.24.0-r22",
        "effectivePlanDigest": payload.effective_plan_digest,
        "resolverStatus": "resolved"
    });
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let captured_users = Arc::new(Mutex::new(Vec::new()));
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let tool = PlannerTool::new(CountingProvider {
        calls: Arc::clone(&provider_calls),
        captured_users: Arc::clone(&captured_users),
        text: "accepted".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });
    let client = DummyClientHandler.serve(client_transport).await.unwrap();
    let args = serde_json::json!({
        "task": "review the frozen plan",
        "context": "fixture repository",
        "plan_context": plan_context.to_string(),
        "frozen_plan": serde_json::to_value(&payload).unwrap()
    });
    let request = CallToolRequestParams::new("plan").with_arguments(args.as_object().unwrap().clone());
    let result = client.call_tool(request).await.unwrap();

    assert_eq!(result.is_error, Some(false));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    let captured = captured_users.lock().unwrap();
    assert_eq!(captured.len(), 1);
    assert!(captured[0].contains(&payload.effective_plan_content));
    assert!(captured[0].contains(&payload.effective_plan_digest));
    assert_ne!(payload.content_digest, payload.effective_plan_digest);

    let _ = client.cancel().await;
}

#[tokio::test]
async fn plan_rejects_supplemental_identity_and_order_mismatch_before_provider_call() {
    use sha2::{Digest, Sha256};

    let primary_content = "# Primary plan\n";
    let primary_digest = format!("{:x}", Sha256::digest(primary_content.as_bytes()));

    let supp_a_content = "# Supplemental A plan\n";
    let supp_a_digest = format!("{:x}", Sha256::digest(supp_a_content.as_bytes()));

    let supp_b_content = "# Supplemental B plan\n";
    let supp_b_digest = format!("{:x}", Sha256::digest(supp_b_content.as_bytes()));

    let ordered_sources_struct = vec![
        anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSource {
            kind: anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSourceKind::Primary,
            id: "V0.24.0-r22".into(),
            suffix: None,
            path: ".plan/V0.24.0-r22.md".into(),
            source_digest: primary_digest.clone(),
            content: primary_content.into(),
        },
        anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSource {
            kind: anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSourceKind::Supplemental,
            id: "V0.24.0-r22a".into(),
            suffix: Some("a".into()),
            path: ".plan/V0.24.0-r22a.md".into(),
            source_digest: supp_a_digest.clone(),
            content: supp_a_content.into(),
        },
        anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSource {
            kind: anthro_bridge_mcp_server::mcp::StructuredFrozenPlanSourceKind::Supplemental,
            id: "V0.24.0-r22b".into(),
            suffix: Some("b".into()),
            path: ".plan/V0.24.0-r22b.md".into(),
            source_digest: supp_b_digest.clone(),
            content: supp_b_content.into(),
        },
    ];
    let effective_content = anthro_bridge_mcp_server::mcp::canonical_assemble_sources(&ordered_sources_struct).unwrap();
    let content_digest = format!("{:x}", Sha256::digest(effective_content.as_bytes()));
    let identity_digest = "plan-set-identity-digest";

    let plan_context = serde_json::json!({
        "projectRootIdentity": "supplemental-test-root",
        "planDirectory": ".plan",
        "applicationVersion": "0.23.0",
        "planSeriesVersion": "0.24.0",
        "currentPrimaryPlan": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_digest,
            "revision": 22
        },
        "activeSupplementalPlans": [
            {
                "id": "V0.24.0-r22a",
                "path": ".plan/V0.24.0-r22a.md",
                "suffix": "a",
                "digest": supp_a_digest
            },
            {
                "id": "V0.24.0-r22b",
                "path": ".plan/V0.24.0-r22b.md",
                "suffix": "b",
                "digest": supp_b_digest
            }
        ],
        "currentLeafPlanId": "V0.24.0-r22b",
        "effectivePlanDigest": identity_digest,
        "resolverStatus": "resolved"
    });
    let valid_payload = serde_json::json!({
        "schemaVersion": 1,
        "effectivePlanContent": effective_content,
        "contentDigest": content_digest,
        "effectivePlanDigest": identity_digest,
        "primaryPlanIdentityAndDigest": {
            "id": "V0.24.0-r22",
            "path": ".plan/V0.24.0-r22.md",
            "digest": primary_digest,
            "revision": 22
        },
        "orderedSupplementalPlanIdentitiesAndDigests": [
            {
                "id": "V0.24.0-r22a",
                "path": ".plan/V0.24.0-r22a.md",
                "digest": supp_a_digest,
                "suffix": "a"
            },
            {
                "id": "V0.24.0-r22b",
                "path": ".plan/V0.24.0-r22b.md",
                "digest": supp_b_digest,
                "suffix": "b"
            }
        ],
        "orderedSources": [
            {
                "kind": "primary",
                "id": "V0.24.0-r22",
                "path": ".plan/V0.24.0-r22.md",
                "sourceDigest": primary_digest,
                "content": primary_content
            },
            {
                "kind": "supplemental",
                "id": "V0.24.0-r22a",
                "path": ".plan/V0.24.0-r22a.md",
                "suffix": "a",
                "sourceDigest": supp_a_digest,
                "content": supp_a_content
            },
            {
                "kind": "supplemental",
                "id": "V0.24.0-r22b",
                "path": ".plan/V0.24.0-r22b.md",
                "suffix": "b",
                "sourceDigest": supp_b_digest,
                "content": supp_b_content
            }
        ]
    });

    let provider_calls = Arc::new(AtomicUsize::new(0));
    let (server_transport, client_transport) = tokio::io::duplex(4096);
    let tool = PlannerTool::new(CountingProvider {
        calls: Arc::clone(&provider_calls),
        captured_users: Arc::new(Mutex::new(Vec::new())),
        text: "must not be called".into(),
    });
    tokio::spawn(async move {
        if let Ok(service) = tool.serve(server_transport).await {
            let _ = service.waiting().await;
        }
    });
    let client = DummyClientHandler.serve(client_transport).await.unwrap();

    // The correct ordered source list is valid at the typed MCP boundary.
    let valid_args = serde_json::json!({
        "task": "plan",
        "context": "repository",
        "plan_context": plan_context.to_string(),
        "frozen_plan": valid_payload
    });
    let request = CallToolRequestParams::new("plan").with_arguments(valid_args.as_object().unwrap().clone());
    let valid_result = client.call_tool(request).await.unwrap();
    assert_eq!(valid_result.is_error, Some(false));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // Swapping two otherwise valid identities/sources must fail before provider invocation.
    let mut reordered_payload = valid_payload.clone();
    reordered_payload["orderedSources"]
        .as_array_mut()
        .unwrap()
        .swap(1, 2);
    let reordered_args = serde_json::json!({
        "task": "plan",
        "context": "repository",
        "plan_context": plan_context.to_string(),
        "frozen_plan": reordered_payload
    });
    let request = CallToolRequestParams::new("plan").with_arguments(reordered_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(request).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("supplemental source mismatch") || error_text.contains("does not match"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // Replacing a source identity while preserving the body digests also fails closed.
    let mut mismatched_payload = valid_payload.clone();
    mismatched_payload["orderedSources"][1]["path"] =
        serde_json::json!(".plan/escaped-plan.md");
    let mismatched_args = serde_json::json!({
        "task": "plan",
        "context": "repository",
        "plan_context": plan_context.to_string(),
        "frozen_plan": mismatched_payload
    });
    let request = CallToolRequestParams::new("plan").with_arguments(mismatched_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(request).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("supplemental source mismatch") || error_text.contains("does not match"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // Omitting a supplemental source (e.g. leaf only) fails closed.
    let mut leaf_only_payload = valid_payload.clone();
    leaf_only_payload["orderedSources"] = serde_json::json!([
        {
            "kind": "supplemental",
            "id": "V0.24.0-r22b",
            "path": ".plan/V0.24.0-r22b.md",
            "suffix": "b",
            "sourceDigest": supp_b_digest,
            "content": supp_b_content
        }
    ]);
    let leaf_only_args = serde_json::json!({
        "task": "plan",
        "context": "repository",
        "plan_context": plan_context.to_string(),
        "frozen_plan": leaf_only_payload
    });
    let request = CallToolRequestParams::new("plan").with_arguments(leaf_only_args.as_object().unwrap().clone());
    let error_text = match client.call_tool(request).await {
        Err(err) => err.to_string(),
        Ok(result) => {
            assert_eq!(result.is_error, Some(true));
            format!("{:?}", result.content)
        }
    };
    assert!(error_text.contains("count mismatch"));
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    let _ = client.cancel().await;
}

#[test]
fn test_build_user_prompt_with_plan_context_and_frozen_plan() {
    use anthro_bridge_mcp_server::mcp::build_user_prompt_with_plan_context_and_frozen_plan;

    let frozen = anthro_bridge_mcp_server::mcp::StructuredFrozenPlanPayload {
        schema_version: 1,
        effective_plan_content: "Exact Frozen Plan Body".into(),
        content_digest: "sha256:body".into(),
        effective_plan_digest: "sha256:identity".into(),
        primary_plan_identity_and_digest: None,
        ordered_supplemental_plan_identities_and_digests: vec![],
        ordered_sources: vec![],
    };

    let prompt = build_user_prompt_with_plan_context_and_frozen_plan(
        "implement phase A",
        "repo details",
        Some("strict mode"),
        None,
        Some(&frozen),
    );

    assert!(prompt.contains("## Task\n\nimplement phase A"));
    assert!(prompt.contains("## Repository context\n\nrepo details"));
    assert!(prompt.contains("## Approved Frozen Implementation Plan\n\nExact Frozen Plan Body"));
    assert!(prompt.contains("## Constraints\n\nstrict mode"));
}
