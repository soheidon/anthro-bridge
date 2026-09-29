use super::types::*;
use std::collections::HashMap;

pub fn default_orchestrator_profiles() -> Vec<OrchestratorProfile> {
    vec![
        OrchestratorProfile {
            id: "mimo-v26-pro".to_string(),
            display_name: "MiMo-V2.6-Pro (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("mimo".to_string()),
            provider_profile_id: None,
            model: Some("mimo-v2.6-pro".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(1_000_000),
        },
        OrchestratorProfile {
            id: "mimo-v26-flash".to_string(),
            display_name: "MiMo-V2.6-Flash (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("mimo".to_string()),
            provider_profile_id: None,
            model: Some("mimo-v2.6-flash".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(1_000_000),
        },
        OrchestratorProfile {
            id: "deepseek-v41-flash".to_string(),
            display_name: "DeepSeek V4.1 Flash (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("deepseek".to_string()),
            provider_profile_id: None,
            model: Some("deepseek-v4.1-flash".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: Some("high".to_string()),
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(128_000),
        },
        OrchestratorProfile {
            id: "minimax-m3".to_string(),
            display_name: "MiniMax M3 (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("minimax".to_string()),
            provider_profile_id: None,
            model: Some("MiniMax-M3".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(1_000_000),
        },
        OrchestratorProfile {
            id: "kimi-k3".to_string(),
            display_name: "Kimi K3 (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("kimi".to_string()),
            provider_profile_id: None,
            model: Some("kimi-k3".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(1_048_576),
        },
        OrchestratorProfile {
            id: "kimi-for-coding".to_string(),
            display_name: "Kimi for Coding (Direct API)".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("kimi-code".to_string()),
            provider_profile_id: None,
            model: Some("kimi-for-coding".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(200_000),
        },
        OrchestratorProfile {
            id: "openrouter-gpt-56-sol".to_string(),
            display_name: "OpenRouter GPT-5.6 Sol".to_string(),
            adapter: ExecutionAdapterType::Provider,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: Some("openrouter".to_string()),
            provider_profile_id: None,
            model: Some("openai/gpt-5.6-sol".to_string()),
            thinking_mode: Some("thinking".to_string()),
            reasoning_effort: Some("high".to_string()),
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(1_050_000),
        },
        OrchestratorProfile {
            id: "ollama-mimo-9b".to_string(),
            display_name: "Ollama MiMo-V2.6-9B (Local)".to_string(),
            adapter: ExecutionAdapterType::Ollama,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: Some("mimo-v2.6:9b".to_string()),
            ollama_endpoint: Some("http://127.0.0.1:11434".to_string()),
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(32_768),
        },
        OrchestratorProfile {
            id: "ollama-qwen-coder".to_string(),
            display_name: "Ollama Qwen2.5-Coder 14B (Local)".to_string(),
            adapter: ExecutionAdapterType::Ollama,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: Some("qwen2.5-coder:14b".to_string()),
            ollama_endpoint: Some("http://127.0.0.1:11434".to_string()),
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(32_768),
        },
        OrchestratorProfile {
            id: "codex-cli".to_string(),
            display_name: "Codex CLI (Local Agent)".to_string(),
            adapter: ExecutionAdapterType::Cli,
            capabilities: vec![
                ProfileCapability::Reasoning,
                ProfileCapability::Review,
                ProfileCapability::WorkspaceRead,
                ProfileCapability::WorkspaceWrite,
                ProfileCapability::CommandExecution,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: Some("codex".to_string()),
            args: Some(vec!["exec".to_string()]),
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: Some(200_000),
        },
    ]
}

pub fn default_validation_gates() -> Vec<ValidationGateConfig> {
    vec![
        ValidationGateConfig {
            id: "typecheck".to_string(),
            name: "TypeScript Check (tsc)".to_string(),
            executable: "npx".to_string(),
            args: vec!["tsc".to_string(), "--noEmit".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: false,
        },
        ValidationGateConfig {
            id: "test".to_string(),
            name: "Test Suite".to_string(),
            executable: "npm".to_string(),
            args: vec!["test".to_string(), "--".to_string(), "--run".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: false,
        },
        ValidationGateConfig {
            id: "git-status".to_string(),
            name: "Git Status Check".to_string(),
            executable: "git".to_string(),
            args: vec!["status".to_string(), "--short".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: false,
            is_advanced_custom: false,
        },
    ]
}

pub fn builtin_presets() -> Vec<OrchestratorPreset> {
    let mut balanced_map = HashMap::new();
    balanced_map.insert(AgentRole::Planner, "mimo-v26-pro".to_string());
    balanced_map.insert(AgentRole::PlanReviewer, "deepseek-v41-flash".to_string());
    balanced_map.insert(AgentRole::Implementer, "codex-cli".to_string());
    balanced_map.insert(AgentRole::Fixer, "codex-cli".to_string());
    balanced_map.insert(AgentRole::CodeReviewer, "codex-cli".to_string());

    let mut cheap_hybrid_map = HashMap::new();
    cheap_hybrid_map.insert(AgentRole::Planner, "mimo-v26-pro".to_string());
    cheap_hybrid_map.insert(AgentRole::PlanReviewer, "ollama-mimo-9b".to_string());
    cheap_hybrid_map.insert(AgentRole::Implementer, "codex-cli".to_string());
    cheap_hybrid_map.insert(AgentRole::Fixer, "codex-cli".to_string());
    cheap_hybrid_map.insert(AgentRole::CodeReviewer, "ollama-mimo-9b".to_string());

    let mut gemini_light_map = HashMap::new();
    gemini_light_map.insert(AgentRole::Planner, "mimo-v26-pro".to_string());
    gemini_light_map.insert(AgentRole::PlanReviewer, "deepseek-v41-flash".to_string());
    gemini_light_map.insert(AgentRole::Implementer, "codex-cli".to_string());
    gemini_light_map.insert(AgentRole::Fixer, "codex-cli".to_string());
    gemini_light_map.insert(AgentRole::CodeReviewer, "codex-cli".to_string());

    let mut high_quality_map = HashMap::new();
    high_quality_map.insert(AgentRole::Planner, "mimo-v26-pro".to_string());
    high_quality_map.insert(AgentRole::PlanReviewer, "codex-cli".to_string());
    high_quality_map.insert(AgentRole::Implementer, "codex-cli".to_string());
    high_quality_map.insert(AgentRole::Fixer, "codex-cli".to_string());
    high_quality_map.insert(AgentRole::CodeReviewer, "codex-cli".to_string());

    vec![
        OrchestratorPreset {
            id: "balanced".to_string(),
            name: "Balanced Agentic Development".to_string(),
            description: "MiMo Pro for planning, DeepSeek for review, Codex CLI for implementation, fixing, and diff review.".to_string(),
            assignments: balanced_map,
            iteration_limits: Some(LoopIterationLimits::default()),
            validation_gates: Some(default_validation_gates()),
            budget_limits: None,
        },
        OrchestratorPreset {
            id: "cheap-hybrid".to_string(),
            name: "Cheap Hybrid (Local Reviewer)".to_string(),
            description: "Cloud API planning, local Ollama for reviews, Codex CLI for code modifications.".to_string(),
            assignments: cheap_hybrid_map,
            iteration_limits: Some(LoopIterationLimits::default()),
            validation_gates: Some(default_validation_gates()),
            budget_limits: None,
        },
        OrchestratorPreset {
            id: "gemini-light".to_string(),
            name: "Gemini-Light".to_string(),
            description: "Conserves Gemini capacity by utilizing API models for planning and review.".to_string(),
            assignments: gemini_light_map,
            iteration_limits: Some(LoopIterationLimits::default()),
            validation_gates: Some(default_validation_gates()),
            budget_limits: None,
        },
        OrchestratorPreset {
            id: "high-quality".to_string(),
            name: "High-Quality (Strict)".to_string(),
            description: "MiMo Pro planning with full Codex CLI validation across review, implementation, and fixing.".to_string(),
            assignments: high_quality_map,
            iteration_limits: Some(LoopIterationLimits::default()),
            validation_gates: Some(default_validation_gates()),
            budget_limits: None,
        },
    ]
}

pub fn default_role_assignments() -> HashMap<AgentRole, RoleAssignment> {
    let mut map = HashMap::new();
    map.insert(
        AgentRole::Planner,
        RoleAssignment {
            role: AgentRole::Planner,
            profile_id: "mimo-v26-pro".to_string(),
            custom_prompt_supplement: None,
            escalation_role: None,
        },
    );
    map.insert(
        AgentRole::PlanReviewer,
        RoleAssignment {
            role: AgentRole::PlanReviewer,
            profile_id: "deepseek-v41-flash".to_string(),
            custom_prompt_supplement: None,
            escalation_role: None,
        },
    );
    map.insert(
        AgentRole::Implementer,
        RoleAssignment {
            role: AgentRole::Implementer,
            profile_id: "codex-cli".to_string(),
            custom_prompt_supplement: None,
            escalation_role: None,
        },
    );
    map.insert(
        AgentRole::Fixer,
        RoleAssignment {
            role: AgentRole::Fixer,
            profile_id: "codex-cli".to_string(),
            custom_prompt_supplement: None,
            escalation_role: Some(AgentRole::Implementer),
        },
    );
    map.insert(
        AgentRole::CodeReviewer,
        RoleAssignment {
            role: AgentRole::CodeReviewer,
            profile_id: "codex-cli".to_string(),
            custom_prompt_supplement: None,
            escalation_role: None,
        },
    );
    map
}

pub fn default_orchestrator_config() -> OrchestratorConfig {
    OrchestratorConfig {
        project_path: None,
        active_workflow_id: Some("full_loop".to_string()),
        active_preset_id: Some("balanced".to_string()),
        profiles: default_orchestrator_profiles(),
        assignments: default_role_assignments(),
        iteration_limits: LoopIterationLimits::default(),
        validation_gates: default_validation_gates(),
        budget_limits: HashMap::new(),
        custom_presets: Vec::new(),
        authorized_custom_gates: Vec::new(),
        quick_slots: default_quick_slots(),
    }
}
