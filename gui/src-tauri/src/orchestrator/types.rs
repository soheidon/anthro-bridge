use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::plan_workspace::PlanWorkspaceConfig;

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
    PlanIntegrator,
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
    Antigravity,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowState {
    #[default]
    Idle,
    LoadingProject,
    BuildingContext,
    PlanDraft,
    PlanGeneration,
    AwaitingAntigravityClaim,
    PlanIntegration,
    PlanReview,
    PlanRevision,
    Implementation,
    Validation,
    FailureAnalysis,
    FindingAggregation,
    Fix,
    CodeReview,
    HumanGate,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCategory {
    StaticCheck,
    Lint,
    FormatCheck,
    Tests,
    Build,
    SecurityAudit,
    RepositoryCheck,
    ComprehensiveCheck,
    StatusCheck,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateSuccessCriteria {
    ExitZero,
    EmptyOutput,
}

/// Direct process execution configuration for validation gates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationGateConfig {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ValidationCategory>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(alias = "success_criteria")]
    pub success_criteria: Option<GateSuccessCriteria>,
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

/// A dashboard shortcut referencing an existing execution profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorQuickSlot {
    pub id: String,
    #[serde(alias = "profile_id")]
    pub profile_id: String,
    pub label: String,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub order: u32,
}

/// Per-run changes that are overlaid on a cloned snapshot and never persisted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunTransientOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "gate_overrides")]
    pub gate_overrides: Option<HashMap<String, bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "limit_overrides")]
    pub limit_overrides: Option<PartialLoopIterationLimits>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PartialLoopIterationLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_plan_review_iterations")]
    pub max_plan_review_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_fix_iterations")]
    pub max_fix_iterations: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "max_code_review_iterations")]
    pub max_code_review_iterations: Option<u32>,
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

/// Supported transport types for MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpServerTransport {
    #[default]
    Stdio,
}

/// Supported tool contracts for MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum McpToolContract {
    #[default]
    #[serde(rename = "prompt_envelope_v1")]
    PromptEnvelopeV1,
}

/// Persisted configuration definition for an external MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub transport: McpServerTransport,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(alias = "working_directory")]
    pub working_directory: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[serde(alias = "allowed_environment")]
    pub allowed_environment: Vec<String>,
    #[serde(default)]
    #[serde(alias = "tool_contract")]
    pub tool_contract: McpToolContract,
}

/// Root persistent configuration for the Orchestrator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default = "default_quick_slots")]
    #[serde(alias = "quick_slots")]
    pub quick_slots: Vec<OrchestratorQuickSlot>,
    #[serde(default)]
    #[serde(alias = "auto_validation_enabled")]
    pub auto_validation_enabled: bool,
    #[serde(default)]
    #[serde(alias = "lean_antigravity_mode")]
    pub lean_antigravity_mode: bool,
    #[serde(default)]
    #[serde(alias = "plan_workspace")]
    pub plan_workspace: PlanWorkspaceConfig,
    #[serde(default)]
    #[serde(alias = "mcp_servers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            project_path: None,
            active_workflow_id: None,
            active_preset_id: None,
            profiles: Vec::new(),
            assignments: HashMap::new(),
            iteration_limits: LoopIterationLimits::default(),
            validation_gates: Vec::new(),
            budget_limits: HashMap::new(),
            custom_presets: Vec::new(),
            authorized_custom_gates: Vec::new(),
            quick_slots: default_quick_slots(),
            auto_validation_enabled: false,
            lean_antigravity_mode: false,
            plan_workspace: PlanWorkspaceConfig::default(),
            mcp_servers: HashMap::new(),
        }
    }
}


pub fn default_quick_slots() -> Vec<OrchestratorQuickSlot> {
    [
        ("slot-1", "mimo-v26-pro", "MiMo Pro"),
        ("slot-2", "deepseek-v41-flash", "DeepSeek Flash"),
        ("slot-3", "ollama-mimo-9b", "Local MiMo 9B"),
        ("slot-4", "codex-cli", "Codex CLI"),
    ]
    .into_iter()
    .enumerate()
    .map(|(order, (id, profile_id, label))| OrchestratorQuickSlot {
        id: id.to_string(),
        profile_id: profile_id.to_string(),
        label: label.to_string(),
        visible: true,
        order: order as u32,
    })
    .collect()
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
    #[serde(default)]
    #[serde(alias = "lean_antigravity_mode")]
    pub lean_antigravity_mode: bool,
    #[serde(default)]
    #[serde(alias = "plan_workspace")]
    pub plan_workspace: PlanWorkspaceConfig,
    #[serde(default)]
    #[serde(alias = "mcp_servers")]
    pub mcp_servers: HashMap<String, McpServerConfig>,
}

/// Per-run archive directory for an approved implementation plan.
/// This is deliberately separate from the persisted Orchestrator config/snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArchiveOptions {
    pub directory: String,
}

/// Advisory preview returned by the backend archive allocator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanArchivePreview {
    pub next_file_name: String,
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

/// Returns the active roles required for the given workflow mode.
pub fn active_roles_for_workflow(workflow_type: &str) -> Result<Vec<AgentRole>, String> {
    match workflow_type {
        "full_loop" => Ok(vec![
            AgentRole::Planner,
            AgentRole::PlanReviewer,
            AgentRole::Implementer,
            AgentRole::Fixer,
            AgentRole::CodeReviewer,
        ]),
        "human_gated_loop" => Ok(vec![
            AgentRole::Planner,
            AgentRole::PlanIntegrator,
            AgentRole::PlanReviewer,
            AgentRole::Implementer,
            AgentRole::Fixer,
            AgentRole::CodeReviewer,
        ]),
        "plan_only" => Ok(vec![AgentRole::Planner, AgentRole::PlanReviewer]),
        "implement_only" => Ok(vec![AgentRole::Implementer, AgentRole::Fixer]),
        "review_only" => Ok(vec![AgentRole::CodeReviewer]),
        _ => Err(format!(
            "Unsupported Orchestrator workflow '{workflow_type}'. Supported workflows are: full_loop, human_gated_loop, plan_only, implement_only, review_only."
        )),
    }
}

/// Validates that a CodeReviewer profile meets criteria for the review_only workflow.
pub fn validate_review_only_profile(
    profile: &OrchestratorProfile,
) -> Result<(), CapabilityValidationError> {
    if profile.adapter != ExecutionAdapterType::Provider
        && profile.adapter != ExecutionAdapterType::Ollama
    {
        return Err(CapabilityValidationError {
            role: AgentRole::CodeReviewer,
            profile_id: profile.id.clone(),
            profile_name: profile.display_name.clone(),
            missing_capabilities: vec![],
            message: format!(
                "Review-only workflow requires a read-only adapter (Provider or Ollama). Profile '{}' uses adapter '{:?}'.",
                profile.display_name, profile.adapter
            ),
        });
    }

    if !profile.capabilities.contains(&ProfileCapability::Review) {
        return Err(CapabilityValidationError {
            role: AgentRole::CodeReviewer,
            profile_id: profile.id.clone(),
            profile_name: profile.display_name.clone(),
            missing_capabilities: vec![ProfileCapability::Review],
            message: format!(
                "Role 'CodeReviewer' cannot run with profile '{}': missing capability [Review]",
                profile.display_name
            ),
        });
    }

    Ok(())
}

/// Validates role capabilities in the context of the active workflow.
pub fn validate_workflow_role_capabilities(
    workflow_type: &str,
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

    if workflow_type == "review_only" && *role == AgentRole::CodeReviewer {
        validate_review_only_profile(profile)
    } else {
        validate_role_capabilities(role, Some(profile))
    }
}

pub fn required_capabilities_for_role(role: &AgentRole) -> Vec<ProfileCapability> {
    match role {
        AgentRole::Planner => vec![ProfileCapability::Reasoning],
        AgentRole::PlanIntegrator => vec![ProfileCapability::WorkspaceWrite],
        AgentRole::PlanReviewer => vec![ProfileCapability::Review],
        AgentRole::Implementer => vec![ProfileCapability::WorkspaceWrite],
        AgentRole::Fixer => vec![ProfileCapability::WorkspaceWrite],
        AgentRole::CodeReviewer => vec![ProfileCapability::Review],
    }
}

/// Decision made by a human reviewer at the HumanGate stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HumanGateDecision {
    Approve,
    RequestChanges { feedback: String },
    Abort,
}

/// Task envelope delivered to Antigravity worker via Localhost HTTP Mailbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestratorTaskEnvelope {
    pub run_id: String,
    pub task_id: String,
    pub stage: WorkflowState,
    pub role: AgentRole,
    pub epoch: u64,
    pub project_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_feedback: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_context: Option<super::plan_workspace::PlanContext>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frozen_plan: Option<super::plan_workspace::FrozenPlanPayload>,
}

/// Ephemeral runtime session descriptor written to `%APPDATA%/Anthro Bridge/orchestrator_session.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailboxSessionDescriptor {
    pub run_id: String,
    pub project_path: String,
    pub port: u16,
    pub token: String,
    pub created_at: u64,
    pub expires_at: u64,
}

/// Request to claim the current active task via Localhost HTTP Mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimTaskRequest {
    pub run_id: String,
    #[serde(default)]
    pub wait_seconds: Option<u64>,
}

/// Request to report incremental step progress from Antigravity worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProgressRequest {
    pub run_id: String,
    pub task_id: String,
    pub epoch: u64,
    pub message: String,
    #[serde(default)]
    pub percent: Option<u32>,
}

/// Request to submit completed work from Antigravity worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTaskRequest {
    pub run_id: String,
    pub task_id: String,
    pub epoch: u64,
    #[serde(default)]
    pub idempotency_key: Option<String>,
    pub status: String,
    pub summary: String,
    #[serde(default)]
    pub modified_files: Vec<String>,
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
    #[serde(default)]
    #[serde(alias = "tsconfig_json")]
    pub tsconfig_json: bool,
    #[serde(alias = "pyproject_toml")]
    pub pyproject_toml: bool,
    #[serde(default)]
    #[serde(alias = "requirements_txt")]
    pub requirements_txt: bool,
    #[serde(alias = "cargo_toml")]
    pub cargo_toml: bool,
    #[serde(default)]
    #[serde(alias = "go_mod")]
    pub go_mod: bool,
    #[serde(default)]
    pub description: bool,
    #[serde(default)]
    #[serde(alias = "renv_lock")]
    pub renv_lock: bool,
    #[serde(default)]
    #[serde(alias = "clasp_json")]
    pub clasp_json: bool,
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
    pub project_type: String, // "TypeScript / Node" | "Rust" | "Python" | "Go" | "R" | "Google Apps Script (clasp)" | "Mixed" | "Unknown"
    #[serde(alias = "detected_files")]
    pub detected_files: DetectedFiles,
    #[serde(default)]
    #[serde(alias = "suggested_gates")]
    pub suggested_gates: Vec<ValidationGateConfig>,
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
        assert_eq!(json["quickSlots"][0]["profileId"], "mimo-v26-pro");
        assert!(json["quickSlots"][0].get("profile_id").is_none());
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
            "custom_presets":[{"id":"kept","name":"Kept","description":"legacy","assignments":{}}],
            "quick_slots":[{"id":"legacy-slot","profile_id":"legacy-provider","label":"Legacy","visible":false,"order":7}]
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
        assert_eq!(decoded.quick_slots, vec![OrchestratorQuickSlot {
            id: "legacy-slot".to_string(),
            profile_id: "legacy-provider".to_string(),
            label: "Legacy".to_string(),
            visible: false,
            order: 7,
        }]);

        let frontend = serde_json::to_value(&decoded).unwrap();
        assert_eq!(frontend["activeWorkflowId"], "review_only");
        assert_eq!(frontend["profiles"][0]["displayName"], "Legacy Provider");
        assert_eq!(frontend["assignments"]["planner"]["profileId"], "legacy-provider");
        assert_eq!(frontend["quickSlots"][0]["profileId"], "legacy-provider");
    }

    #[test]
    fn missing_quick_slots_seed_only_known_profile_references() {
        let decoded: OrchestratorConfig = serde_json::from_value(json!({})).unwrap();
        let slots = decoded.quick_slots;
        assert_eq!(slots.len(), 4);
        assert_eq!(slots.iter().map(|slot| slot.profile_id.as_str()).collect::<Vec<_>>(), vec![
            "mimo-v26-pro", "deepseek-v41-flash", "ollama-mimo-9b", "codex-cli",
        ]);
        assert!(slots.iter().all(|slot| slot.visible));
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
                category: Some(ValidationCategory::Tests),
                executable: "cargo".to_string(),
                args: vec!["test".to_string()],
                enabled: true,
                working_dir: Some("C:/project".to_string()),
                fail_on_error: true,
                is_advanced_custom: false,
                success_criteria: Some(GateSuccessCriteria::ExitZero),
            }],
            budget_limits: HashMap::new(),
            created_at_unix: 123,
            lean_antigravity_mode: false,
            plan_workspace: PlanWorkspaceConfig::default(),
            mcp_servers: HashMap::new(),
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

    #[test]
    fn orchestrator_config_defaults_and_serde_round_trip() {
        let default_config = OrchestratorConfig::default();
        assert!(!default_config.auto_validation_enabled);
        assert_eq!(default_config.quick_slots, default_quick_slots());

        // Deserializing empty object produces the exact same defaults
        let empty_json = json!({});
        let decoded: OrchestratorConfig = serde_json::from_value(empty_json).unwrap();
        assert!(!decoded.auto_validation_enabled);
        assert_eq!(decoded.quick_slots, default_quick_slots());
        assert_eq!(decoded, default_config);

        // Deserializing explicit true (camelCase and snake_case)
        let camel = json!({ "autoValidationEnabled": true, "leanAntigravityMode": true });
        let decoded_camel: OrchestratorConfig = serde_json::from_value(camel).unwrap();
        assert!(decoded_camel.auto_validation_enabled);
        assert!(decoded_camel.lean_antigravity_mode);

        let snake = json!({ "auto_validation_enabled": true, "lean_antigravity_mode": true });
        let decoded_snake: OrchestratorConfig = serde_json::from_value(snake).unwrap();
        assert!(decoded_snake.auto_validation_enabled);
        assert!(decoded_snake.lean_antigravity_mode);

        // Serialization produces camelCase autoValidationEnabled and leanAntigravityMode
        let serialized = serde_json::to_value(&decoded_camel).unwrap();
        assert_eq!(serialized["autoValidationEnabled"], true);
        assert_eq!(serialized["leanAntigravityMode"], true);
        assert!(serialized.get("auto_validation_enabled").is_none());
        assert!(serialized.get("lean_antigravity_mode").is_none());
    }
}
