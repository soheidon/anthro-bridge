use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Capabilities provided by an execution profile or required by an agent role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileCapability {
    Reasoning,
    Review,
    WorkspaceRead,
    WorkspaceWrite,
    CommandExecution,
}

/// Abstract role in the autonomous development workflow.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Planner,
    PlanReviewer,
    Implementer,
    Fixer,
    CodeReviewer,
}

/// Type of execution adapter powering a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionAdapterType {
    Provider,
    Ollama,
    Cli,
    Mcp,
}

/// Concrete execution profile configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorProfile {
    pub id: String,
    #[serde(alias = "display_name")]
    pub display_name: String,
    pub adapter: ExecutionAdapterType,
    #[serde(default)]
    pub capabilities: Vec<ProfileCapability>,

    // Provider adapter fields
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "provider_id")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "provider_profile_id")]
    pub provider_profile_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "thinking_mode")]
    pub thinking_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "reasoning_effort")]
    pub reasoning_effort: Option<String>,

    // Ollama adapter fields
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "ollama_model")]
    pub ollama_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "ollama_endpoint")]
    pub ollama_endpoint: Option<String>,

    // CLI adapter fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    // External MCP adapter fields
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "external_mcp_server")]
    pub external_mcp_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "mcp_tool")]
    pub mcp_tool: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "context_window_tokens")]
    pub context_window_tokens: Option<u64>,
}

/// Binding of a role to a profile with optional prompt overrides or escalation targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleAssignment {
    pub role: AgentRole,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "custom_prompt_supplement")]
    pub custom_prompt_supplement: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "escalation_role")]
    pub escalation_role: Option<AgentRole>,
}

/// Run control lifecycle state for pause/resume and cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunControlState {
    Running,
    Paused,
    Cancelled,
}

/// Workflow state machine states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    Idle,
    LoadingProject,
    BuildingContext,
    PlanGeneration,
    PlanReview,
    PlanRevision,
    Implementation,
    Validation,
    FailureAnalysis,
    FindingAggregation,
    Fix,
    CodeReview,
    WaitingForUser,
    WaitingForBlockingResolution,
    Complete,
    Failed,
    Paused,
    Cancelled,
}

/// Response returned immediately upon starting an orchestrator run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRunResponse {
    #[serde(alias = "run_id")]
    pub run_id: String,
}

/// A log item correlated to the backend-owned Orchestrator run that emitted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunLogEvent {
    #[serde(alias = "run_id")]
    pub run_id: String,
    pub message: String,
}

/// Configurable limits on loop iterations to prevent unbounded execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoopIterationLimits {
    #[serde(default = "default_max_plan_review")]
    #[serde(alias = "max_plan_review_iterations")]
    pub max_plan_review_iterations: u32,
    #[serde(default = "default_max_fix")]
    #[serde(alias = "max_fix_iterations")]
    pub max_fix_iterations: u32,
    #[serde(default = "default_max_code_review")]
    #[serde(alias = "max_code_review_iterations")]
    pub max_code_review_iterations: u32,
}

fn default_max_plan_review() -> u32 {
    2
}
fn default_max_fix() -> u32 {
    3
}
fn default_max_code_review() -> u32 {
    2
}

impl Default for LoopIterationLimits {
    fn default() -> Self {
        Self {
            max_plan_review_iterations: default_max_plan_review(),
            max_fix_iterations: default_max_fix(),
            max_code_review_iterations: default_max_code_review(),
        }
    }
}

/// Review verdict options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approved,
    ChangesRequired,
    NeedsClarification,
    Failed,
}

/// Severity level for review findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Structured finding from review or validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewFinding {
    pub id: String,
    pub severity: FindingSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub issue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<String>,
    #[serde(default)]
    #[serde(alias = "is_blocking")]
    pub is_blocking: bool,
}

/// Result of a plan review or code review step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewResult {
    pub verdict: ReviewVerdict,
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    pub summary: String,
    #[serde(alias = "raw_output")]
    pub raw_output: String,
}

/// Direct process execution configuration for validation gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationGateConfig {
    pub id: String,
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "working_dir")]
    pub working_dir: Option<String>,
    #[serde(default = "default_true")]
    #[serde(alias = "fail_on_error")]
    pub fail_on_error: bool,
    #[serde(default)]
    #[serde(alias = "is_advanced_custom")]
    pub is_advanced_custom: bool,
}
fn default_true() -> bool {
    true
}

/// Authorized record for custom validation commands stored and verified on the backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorizedCustomGate {
    #[serde(alias = "gate_id")]
    pub gate_id: String,
    pub executable: String,
    pub args: Vec<String>,
    #[serde(alias = "canonical_working_dir")]
    pub canonical_working_dir: String,
    #[serde(alias = "command_hash")]
    pub command_hash: String,
}

/// Output captured by ProcessRunner with 1 MiB retained limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturedOutput {
    pub text: String,
    #[serde(alias = "is_truncated")]
    pub is_truncated: bool,
    #[serde(alias = "total_bytes")]
    pub total_bytes: usize,
}

/// Result of running a process via ProcessRunner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    #[serde(alias = "exit_code")]
    pub exit_code: Option<i32>,
    pub stdout: CapturedOutput,
    pub stderr: CapturedOutput,
    #[serde(alias = "timed_out")]
    pub timed_out: bool,
    pub cancelled: bool,
    #[serde(alias = "duration_ms")]
    pub duration_ms: u64,
}

/// Budget and rate-limiting limits for a model or role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetConfig {
    #[serde(alias = "max_calls_per_run")]
    pub max_calls_per_run: u32,
    #[serde(alias = "max_consecutive_calls")]
    pub max_consecutive_calls: u32,
    #[serde(alias = "timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "fallback_profile_id")]
    pub fallback_profile_id: Option<String>,
    #[serde(default = "default_rate_limit_action")]
    #[serde(alias = "on_rate_limit")]
    pub on_rate_limit: String, // "pause" | "fallback" | "escalate" | "stop"
}

fn default_rate_limit_action() -> String {
    "fallback".to_string()
}

/// Named presets combining role assignments, gates, and limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub assignments: HashMap<AgentRole, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "iteration_limits")]
    pub iteration_limits: Option<LoopIterationLimits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "validation_gates")]
    pub validation_gates: Option<Vec<ValidationGateConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "budget_limits")]
    pub budget_limits: Option<HashMap<String, BudgetConfig>>,
}

/// Root persistent configuration for the Orchestrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "project_path")]
    pub project_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "active_workflow_id")]
    pub active_workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "active_preset_id")]
    pub active_preset_id: Option<String>,
    #[serde(default)]
    pub profiles: Vec<OrchestratorProfile>,
    #[serde(default)]
    pub assignments: HashMap<AgentRole, RoleAssignment>,
    #[serde(default)]
    #[serde(alias = "iteration_limits")]
    pub iteration_limits: LoopIterationLimits,
    #[serde(default)]
    #[serde(alias = "validation_gates")]
    pub validation_gates: Vec<ValidationGateConfig>,
    #[serde(default)]
    #[serde(alias = "budget_limits")]
    pub budget_limits: HashMap<String, BudgetConfig>,
    #[serde(default)]
    #[serde(alias = "custom_presets")]
    pub custom_presets: Vec<OrchestratorPreset>,
    #[serde(default)]
    #[serde(alias = "authorized_custom_gates")]
    pub authorized_custom_gates: Vec<AuthorizedCustomGate>,
}

/// Frozen snapshot of all parameters for a specific run (run_id is runtime session state, not in snapshot).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunConfigurationSnapshot {
    #[serde(alias = "project_path")]
    pub project_path: String,
    pub assignments: HashMap<AgentRole, OrchestratorProfile>,
    #[serde(alias = "iteration_limits")]
    pub iteration_limits: LoopIterationLimits,
    #[serde(alias = "validation_gates")]
    pub validation_gates: Vec<ValidationGateConfig>,
    #[serde(alias = "budget_limits")]
    pub budget_limits: HashMap<String, BudgetConfig>,
    #[serde(alias = "created_at_unix")]
    pub created_at_unix: u64,
}

/// Validation error when a profile lacks capabilities required by its assigned role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityValidationError {
    pub role: AgentRole,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    #[serde(alias = "profile_name")]
    pub profile_name: String,
    #[serde(alias = "missing_capabilities")]
    pub missing_capabilities: Vec<ProfileCapability>,
    pub message: String,
}

/// Checks whether a profile satisfies all capabilities required by the specified role.
pub fn validate_role_capabilities(
    role: &AgentRole,
    profile: Option<&OrchestratorProfile>,
) -> Result<(), CapabilityValidationError> {
    let profile = match profile {
        Some(p) => p,
        None => {
            return Err(CapabilityValidationError {
                role: role.clone(),
                profile_id: String::new(),
                profile_name: "Unassigned".to_string(),
                missing_capabilities: required_capabilities_for_role(role),
                message: format!("Role '{:?}' has no assigned profile.", role),
            });
        }
    };

    let required = required_capabilities_for_role(role);
    let missing: Vec<ProfileCapability> = required
        .into_iter()
        .filter(|cap| !profile.capabilities.contains(cap))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        let missing_names: Vec<String> = missing.iter().map(|c| format!("{:?}", c)).collect();
        Err(CapabilityValidationError {
            role: role.clone(),
            profile_id: profile.id.clone(),
            profile_name: profile.display_name.clone(),
            missing_capabilities: missing,
            message: format!(
                "Role '{:?}' cannot run with profile '{}': missing capability [{}]",
                role,
                profile.display_name,
                missing_names.join(", ")
            ),
        })
    }
}

    pub fn required_capabilities_for_role(role: &AgentRole) -> Vec<ProfileCapability> {
    match role {
        AgentRole::Planner => vec![ProfileCapability::Reasoning],
        AgentRole::PlanReviewer => vec![ProfileCapability::Review],
        AgentRole::Implementer => vec![ProfileCapability::WorkspaceWrite],
        AgentRole::Fixer => vec![ProfileCapability::WorkspaceWrite],
        AgentRole::CodeReviewer => vec![ProfileCapability::Review],
    }
}

/// Metadata discovered about a target project directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedFiles {
    #[serde(alias = "spec_md")]
    pub spec_md: bool,
    #[serde(alias = "implementation_plan_md")]
    pub implementation_plan_md: bool,
    #[serde(alias = "agents_md")]
    pub agents_md: bool,
    #[serde(alias = "readme_md")]
    pub readme_md: bool,
    #[serde(alias = "package_json")]
    pub package_json: bool,
    #[serde(alias = "pyproject_toml")]
    pub pyproject_toml: bool,
    #[serde(alias = "cargo_toml")]
    pub cargo_toml: bool,
    pub git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadataResponse {
    pub path: String,
    pub exists: bool,
    #[serde(alias = "is_directory")]
    pub is_directory: bool,
    #[serde(alias = "project_type")]
    pub project_type: String, // "TypeScript / Node" | "Rust" | "Python" | "Mixed" | "Unknown"
    #[serde(alias = "detected_files")]
    pub detected_files: DetectedFiles,
}

#[cfg(test)]
mod wire_contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_serializes_to_camel_case_and_round_trips() {
        let mut config = crate::orchestrator::default_orchestrator_config();
        config.active_workflow_id = Some("review_only".to_string());
        let json = serde_json::to_value(&config).unwrap();

        assert_eq!(json["activeWorkflowId"], "review_only");
        assert!(json.get("active_workflow_id").is_none());
        assert_eq!(json["profiles"][0]["displayName"], config.profiles[0].display_name);
        assert!(json["profiles"][0].get("display_name").is_none());
        assert!(json["iterationLimits"].get("maxPlanReviewIterations").is_some());
        assert_eq!(json["assignments"]["planner"]["profileId"], config.assignments[&AgentRole::Planner].profile_id);

        let decoded: OrchestratorConfig = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, config);
    }

    #[test]
    fn legacy_snake_case_config_deserializes_and_exposes_frontend_shape() {
        let legacy = json!({
            "project_path":"C:/legacy/project",
            "active_workflow_id":"review_only",
            "active_preset_id":"custom",
            "profiles":[{
                "id":"legacy-provider","display_name":"Legacy Provider","adapter":"provider",
                "capabilities":["reasoning"],"provider_id":"deepseek","model":"deepseek-v4.1-flash",
                "context_window_tokens":128000
            }],
            "assignments":{"planner":{"role":"planner","profile_id":"legacy-provider"}},
            "iteration_limits":{"max_plan_review_iterations":4,"max_fix_iterations":5,"max_code_review_iterations":6},
            "validation_gates":[{"id":"test","name":"Test","executable":"cargo","args":["test"],"enabled":true,"working_dir":"C:/legacy/project","fail_on_error":true,"is_advanced_custom":false}],
            "budget_limits":{"planner":{"max_calls_per_run":8,"max_consecutive_calls":2,"timeout_seconds":500,"on_rate_limit":"pause"}},
            "custom_presets":[{"id":"kept","name":"Kept","description":"legacy","assignments":{}}]
        });

        let decoded: OrchestratorConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.active_workflow_id.as_deref(), Some("review_only"));
        assert_eq!(decoded.profiles[0].display_name, "Legacy Provider");
        assert_eq!(decoded.profiles[0].provider_id.as_deref(), Some("deepseek"));
        assert_eq!(decoded.profiles[0].context_window_tokens, Some(128000));
        assert_eq!(decoded.assignments[&AgentRole::Planner].profile_id, "legacy-provider");
        assert_eq!(decoded.validation_gates[0].working_dir.as_deref(), Some("C:/legacy/project"));
        assert_eq!(decoded.budget_limits["planner"].max_calls_per_run, 8);
        assert_eq!(decoded.custom_presets[0].id, "kept");

        let frontend = serde_json::to_value(&decoded).unwrap();
        assert_eq!(frontend["activeWorkflowId"], "review_only");
        assert_eq!(frontend["profiles"][0]["displayName"], "Legacy Provider");
        assert_eq!(frontend["assignments"]["planner"]["profileId"], "legacy-provider");
    }

    #[test]
    fn run_snapshot_uses_camel_case_and_round_trips_legacy_snake_case() {
        let profile = crate::orchestrator::default_orchestrator_profiles()
            .into_iter()
            .find(|profile| profile.id == "mimo-v26-pro")
            .unwrap();
        let snapshot = RunConfigurationSnapshot {
            project_path: "C:/project".to_string(),
            assignments: HashMap::from([(AgentRole::Planner, profile)]),
            iteration_limits: LoopIterationLimits::default(),
            validation_gates: vec![ValidationGateConfig {
                id: "test".to_string(),
                name: "Rust tests".to_string(),
                executable: "cargo".to_string(),
                args: vec!["test".to_string()],
                enabled: true,
                working_dir: Some("C:/project".to_string()),
                fail_on_error: true,
                is_advanced_custom: false,
            }],
            budget_limits: HashMap::new(),
            created_at_unix: 123,
        };

        let wire = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(wire["projectPath"], "C:/project");
        assert_eq!(wire["createdAtUnix"], 123);
        assert_eq!(wire["assignments"]["planner"]["displayName"], "MiMo-V2.6-Pro (Direct API)");
        assert_eq!(wire["validationGates"][0]["workingDir"], "C:/project");
        assert!(wire.get("project_path").is_none());
        assert!(wire.get("created_at_unix").is_none());
        assert_eq!(serde_json::from_value::<RunConfigurationSnapshot>(wire).unwrap(), snapshot);

        let legacy = json!({
            "project_path":"C:/legacy",
            "assignments":{"planner":{
                "id":"p","display_name":"Legacy","adapter":"cli","capabilities":["reasoning"],
                "executable":"codex","context_window_tokens":8192
            }},
            "iteration_limits":{"max_plan_review_iterations":2,"max_fix_iterations":3,"max_code_review_iterations":2},
            "validation_gates":[{"id":"g","name":"Gate","executable":"cargo","args":["test"],"enabled":true,"working_dir":"C:/legacy","fail_on_error":true,"is_advanced_custom":false}],
            "budget_limits":{},"created_at_unix":456
        });
        let decoded: RunConfigurationSnapshot = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.project_path, "C:/legacy");
        assert_eq!(decoded.created_at_unix, 456);
        assert_eq!(decoded.assignments[&AgentRole::Planner].display_name, "Legacy");
        assert_eq!(decoded.validation_gates[0].working_dir.as_deref(), Some("C:/legacy"));
    }
}
