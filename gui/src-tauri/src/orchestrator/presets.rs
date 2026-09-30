use super::types::*;
use std::collections::HashMap;
use std::path::Path;

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
        OrchestratorProfile {
            id: "antigravity-harness".to_string(),
            display_name: "Google Antigravity Harness (MCP Mailbox)".to_string(),
            adapter: ExecutionAdapterType::Antigravity,
            capabilities: vec![
                ProfileCapability::WorkspaceRead,
                ProfileCapability::WorkspaceWrite,
                ProfileCapability::CommandExecution,
                ProfileCapability::Reasoning,
            ],
            provider_id: None,
            provider_profile_id: None,
            model: None,
            thinking_mode: None,
            reasoning_effort: None,
            ollama_model: None,
            ollama_endpoint: None,
            executable: None,
            args: None,
            external_mcp_server: None,
            mcp_tool: None,
            context_window_tokens: None,
        },
    ]
}

pub fn default_validation_gates() -> Vec<ValidationGateConfig> {
    vec![
        ValidationGateConfig {
            id: "typecheck".to_string(),
            name: "TypeScript Check (tsc)".to_string(),
            category: Some(ValidationCategory::StaticCheck),
            executable: "npx".to_string(),
            args: vec!["tsc".to_string(), "--noEmit".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: false,
            success_criteria: Some(GateSuccessCriteria::ExitZero),
        },
        ValidationGateConfig {
            id: "test".to_string(),
            name: "Test Suite".to_string(),
            category: Some(ValidationCategory::Tests),
            executable: "npm".to_string(),
            args: vec!["test".to_string(), "--".to_string(), "--run".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: false,
            success_criteria: Some(GateSuccessCriteria::ExitZero),
        },
        ValidationGateConfig {
            id: "git-status".to_string(),
            name: "Git Status Check".to_string(),
            category: Some(ValidationCategory::RepositoryCheck),
            executable: "git".to_string(),
            args: vec!["status".to_string(), "--short".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: false,
            is_advanced_custom: false,
            success_criteria: Some(GateSuccessCriteria::EmptyOutput),
        },
    ]
}

pub const IGNORED_SCAN_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    "vendor",
    ".next",
    "out",
    ".gemini",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredProjectLocation {
    pub relative_path: Option<String>,
    pub full_path: std::path::PathBuf,
}

pub fn is_recognized_manifest_dir(path: &Path) -> bool {
    path.join("Cargo.toml").exists()
        || path.join("package.json").exists()
        || path.join("tsconfig.json").exists()
        || path.join("pyproject.toml").exists()
        || path.join("requirements.txt").exists()
        || path.join("setup.py").exists()
        || path.join("setup.cfg").exists()
        || path.join("go.mod").exists()
        || path.join("DESCRIPTION").exists()
        || path.join("renv.lock").exists()
        || path.join(".clasp.json").exists()
}

pub fn discover_project_locations(root: &Path, max_depth: usize) -> Vec<DiscoveredProjectLocation> {
    let mut locations = Vec::new();
    locations.push(DiscoveredProjectLocation {
        relative_path: None,
        full_path: root.to_path_buf(),
    });

    if max_depth == 0 {
        return locations;
    }

    fn scan_dir(
        root: &Path,
        current: &Path,
        current_depth: usize,
        max_depth: usize,
        locations: &mut Vec<DiscoveredProjectLocation>,
    ) {
        if current_depth > max_depth {
            return;
        }

        let entries = match std::fs::read_dir(current) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            if file_type.is_symlink() {
                continue;
            }

            if file_type.is_dir() {
                let file_name = entry.file_name();
                let name_str = file_name.to_string_lossy();
                if IGNORED_SCAN_DIRS
                    .iter()
                    .any(|&ignored| ignored.eq_ignore_ascii_case(&name_str))
                {
                    continue;
                }
                subdirs.push(entry.path());
            }
        }

        subdirs.sort();

        for subdir in subdirs {
            if is_recognized_manifest_dir(&subdir) {
                if let Ok(rel) = subdir.strip_prefix(root) {
                    let rel_str = rel.to_string_lossy().replace('\\', "/");
                    if !rel_str.is_empty() {
                        locations.push(DiscoveredProjectLocation {
                            relative_path: Some(rel_str),
                            full_path: subdir.clone(),
                        });
                    }
                }
            }

            if current_depth < max_depth {
                scan_dir(root, &subdir, current_depth + 1, max_depth, locations);
            }
        }
    }

    scan_dir(root, root, 1, max_depth, &mut locations);
    locations
}

fn format_gate_name(base_name: &str, rel_path: Option<&str>) -> String {
    if let Some(rel) = rel_path {
        if let Some(pos) = base_name.rfind(')') {
            format!("{} in {})", &base_name[..pos], rel)
        } else {
            format!("{} ({})", base_name, rel)
        }
    } else {
        base_name.to_string()
    }
}

pub fn generate_location_validation_gates(
    project_path: &Path,
    rel_path: Option<&str>,
) -> Vec<ValidationGateConfig> {
    let mut gates = Vec::new();

    let make_gate = |base_id: &str,
                     base_name: &str,
                     category: ValidationCategory,
                     executable: &str,
                     args: Vec<String>,
                     enabled: bool,
                     fail_on_error: bool,
                     success_criteria: GateSuccessCriteria| {
        ValidationGateConfig {
            id: if let Some(rel) = rel_path {
                format!("{}:{}", rel, base_id)
            } else {
                base_id.to_string()
            },
            name: format_gate_name(base_name, rel_path),
            category: Some(category),
            executable: executable.to_string(),
            args,
            enabled,
            working_dir: rel_path.map(|s| s.to_string()),
            fail_on_error,
            is_advanced_custom: false,
            success_criteria: Some(success_criteria),
        }
    };

    // 1. Rust (Cargo.toml)
    if project_path.join("Cargo.toml").exists() {
        gates.push(make_gate(
            "cargo-check",
            "Static Check (cargo check)",
            ValidationCategory::StaticCheck,
            "cargo",
            vec!["check".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "cargo-test",
            "Tests (cargo test)",
            ValidationCategory::Tests,
            "cargo",
            vec!["test".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "cargo-clippy",
            "Lint (cargo clippy)",
            ValidationCategory::Lint,
            "cargo",
            vec![
                "clippy".to_string(),
                "--".to_string(),
                "-D".to_string(),
                "warnings".to_string(),
            ],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "cargo-fmt",
            "Format Check (cargo fmt)",
            ValidationCategory::FormatCheck,
            "cargo",
            vec!["fmt".to_string(), "--check".to_string()],
            false,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "cargo-build",
            "Build (cargo build)",
            ValidationCategory::Build,
            "cargo",
            vec!["build".to_string()],
            false,
            true,
            GateSuccessCriteria::ExitZero,
        ));
    }

    // 2. Node / TypeScript (package.json or tsconfig.json)
    if project_path.join("package.json").exists() || project_path.join("tsconfig.json").exists() {
        let package_json_path = project_path.join("package.json");
        let parsed_package_json: Option<serde_json::Value> = if package_json_path.exists() {
            std::fs::read_to_string(&package_json_path)
                .ok()
                .and_then(|content| serde_json::from_str(&content).ok())
        } else {
            None
        };

        let has_tsconfig = project_path.join("tsconfig.json").exists();
        let scripts = parsed_package_json
            .as_ref()
            .and_then(|v| v.get("scripts"))
            .and_then(|s| s.as_object());
        let dev_deps = parsed_package_json
            .as_ref()
            .and_then(|v| v.get("devDependencies"))
            .and_then(|d| d.as_object());
        let deps = parsed_package_json
            .as_ref()
            .and_then(|v| v.get("dependencies"))
            .and_then(|d| d.as_object());

        let has_typecheck_script = scripts.map_or(false, |s| s.contains_key("typecheck"));
        let has_tsc = has_tsconfig
            || has_typecheck_script
            || dev_deps.map_or(false, |d| d.contains_key("typescript"))
            || deps.map_or(false, |d| d.contains_key("typescript"));

        if has_typecheck_script {
            gates.push(make_gate(
                "typecheck",
                "Static Check (npm run typecheck)",
                ValidationCategory::StaticCheck,
                "npm",
                vec!["run".to_string(), "typecheck".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        } else if has_tsc {
            gates.push(make_gate(
                "typecheck",
                "Static Check (npx tsc)",
                ValidationCategory::StaticCheck,
                "npx",
                vec!["tsc".to_string(), "--noEmit".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        let test_script = scripts.and_then(|s| s.get("test")).and_then(|t| t.as_str());
        let has_vitest = dev_deps.map_or(false, |d| d.contains_key("vitest"))
            || deps.map_or(false, |d| d.contains_key("vitest"))
            || project_path.join("vitest.config.ts").exists()
            || project_path.join("vitest.config.js").exists()
            || project_path.join("vitest.config.mts").exists();
        let has_jest = dev_deps.map_or(false, |d| d.contains_key("jest"))
            || deps.map_or(false, |d| d.contains_key("jest"))
            || project_path.join("jest.config.js").exists()
            || project_path.join("jest.config.ts").exists();

        if let Some(cmd) = test_script {
            let is_dummy_npm_test = cmd.contains("no test specified");
            if !is_dummy_npm_test {
                if cmd.contains("vitest") || has_vitest {
                    let args = if cmd.contains("run") {
                        vec!["test".to_string()]
                    } else {
                        vec!["test".to_string(), "--".to_string(), "--run".to_string()]
                    };
                    gates.push(make_gate(
                        "test",
                        "Tests (npm test)",
                        ValidationCategory::Tests,
                        "npm",
                        args,
                        true,
                        true,
                        GateSuccessCriteria::ExitZero,
                    ));
                } else {
                    gates.push(make_gate(
                        "test",
                        "Tests (npm test)",
                        ValidationCategory::Tests,
                        "npm",
                        vec!["test".to_string()],
                        true,
                        true,
                        GateSuccessCriteria::ExitZero,
                    ));
                }
            }
        } else if has_vitest {
            gates.push(make_gate(
                "test",
                "Tests (npx vitest run)",
                ValidationCategory::Tests,
                "npx",
                vec!["vitest".to_string(), "run".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        } else if has_jest {
            gates.push(make_gate(
                "test",
                "Tests (npx jest)",
                ValidationCategory::Tests,
                "npx",
                vec!["jest".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        if scripts.map_or(false, |s| s.contains_key("lint")) {
            gates.push(make_gate(
                "npm-lint",
                "Lint (npm run lint)",
                ValidationCategory::Lint,
                "npm",
                vec!["run".to_string(), "lint".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        } else if project_path.join("eslint.config.js").exists()
            || project_path.join("eslint.config.mjs").exists()
            || project_path.join(".eslintrc.json").exists()
            || project_path.join(".eslintrc.js").exists()
            || project_path.join(".eslintrc").exists()
        {
            gates.push(make_gate(
                "eslint",
                "Lint (npx eslint)",
                ValidationCategory::Lint,
                "npx",
                vec!["eslint".to_string(), ".".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        if scripts.map_or(false, |s| s.contains_key("format:check")) {
            gates.push(make_gate(
                "npm-format",
                "Format Check (npm run format:check)",
                ValidationCategory::FormatCheck,
                "npm",
                vec!["run".to_string(), "format:check".to_string()],
                false,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        } else if project_path.join(".prettierrc").exists()
            || project_path.join(".prettierrc.json").exists()
            || project_path.join(".prettierrc.js").exists()
            || project_path.join("prettier.config.js").exists()
        {
            gates.push(make_gate(
                "prettier-check",
                "Format Check (npx prettier)",
                ValidationCategory::FormatCheck,
                "npx",
                vec!["prettier".to_string(), "--check".to_string(), ".".to_string()],
                false,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        if scripts.map_or(false, |s| s.contains_key("build")) {
            gates.push(make_gate(
                "npm-build",
                "Build (npm run build)",
                ValidationCategory::Build,
                "npm",
                vec!["run".to_string(), "build".to_string()],
                false,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }
    }

    // 3. Python (pyproject.toml, requirements.txt, setup.py, setup.cfg)
    if project_path.join("pyproject.toml").exists()
        || project_path.join("requirements.txt").exists()
        || project_path.join("setup.py").exists()
        || project_path.join("setup.cfg").exists()
    {
        let pyproject_str =
            std::fs::read_to_string(project_path.join("pyproject.toml")).unwrap_or_default();
        let requirements_str =
            std::fs::read_to_string(project_path.join("requirements.txt")).unwrap_or_default();

        let has_mypy = project_path.join("mypy.ini").exists()
            || project_path.join(".mypy.ini").exists()
            || pyproject_str.contains("[tool.mypy]")
            || requirements_str.contains("mypy");
        if has_mypy {
            gates.push(make_gate(
                "mypy",
                "Static Check (mypy)",
                ValidationCategory::StaticCheck,
                "mypy",
                vec![".".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        let has_ruff = project_path.join("ruff.toml").exists()
            || project_path.join(".ruff.toml").exists()
            || pyproject_str.contains("[tool.ruff]")
            || requirements_str.contains("ruff");
        if has_ruff {
            gates.push(make_gate(
                "ruff-check",
                "Lint (ruff check)",
                ValidationCategory::Lint,
                "ruff",
                vec!["check".to_string(), ".".to_string()],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
            gates.push(make_gate(
                "ruff-format",
                "Format Check (ruff format)",
                ValidationCategory::FormatCheck,
                "ruff",
                vec![
                    "format".to_string(),
                    "--check".to_string(),
                    ".".to_string(),
                ],
                false,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }

        let has_pytest = project_path.join("pytest.ini").exists()
            || project_path.join("tests").is_dir()
            || pyproject_str.contains("[tool.pytest")
            || requirements_str.contains("pytest");
        if has_pytest {
            gates.push(make_gate(
                "pytest",
                "Tests (pytest)",
                ValidationCategory::Tests,
                "pytest",
                vec![],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }
    }

    // 4. Go (go.mod)
    if project_path.join("go.mod").exists() {
        gates.push(make_gate(
            "go-vet",
            "Static Check (go vet)",
            ValidationCategory::StaticCheck,
            "go",
            vec!["vet".to_string(), "./...".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "go-test",
            "Tests (go test)",
            ValidationCategory::Tests,
            "go",
            vec!["test".to_string(), "./...".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        gates.push(make_gate(
            "go-build",
            "Build (go build)",
            ValidationCategory::Build,
            "go",
            vec!["build".to_string(), "./...".to_string()],
            false,
            true,
            GateSuccessCriteria::ExitZero,
        ));
    }

    // 5. R (DESCRIPTION or renv.lock)
    if project_path.join("DESCRIPTION").exists() || project_path.join("renv.lock").exists() {
        gates.push(make_gate(
            "r-cmd-check",
            "Package Check (R CMD check)",
            ValidationCategory::ComprehensiveCheck,
            "R",
            vec!["CMD".to_string(), "check".to_string(), ".".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
        if project_path.join("tests").join("testthat").exists() {
            gates.push(make_gate(
                "r-testthat",
                "Tests (testthat)",
                ValidationCategory::Tests,
                "Rscript",
                vec![
                    "-e".to_string(),
                    "testthat::test_dir('tests/testthat')".to_string(),
                ],
                true,
                true,
                GateSuccessCriteria::ExitZero,
            ));
        }
    }

    // 6. GAS / clasp (.clasp.json)
    if project_path.join(".clasp.json").exists() {
        gates.push(make_gate(
            "clasp-status",
            "Status Check (clasp status)",
            ValidationCategory::StatusCheck,
            "clasp",
            vec!["status".to_string()],
            true,
            true,
            GateSuccessCriteria::ExitZero,
        ));
    }

    gates
}

/// Generates project-adaptive validation gate candidates based on detected manifests and tools across root and nested subprojects.
pub fn generate_project_validation_gates(project_path: &Path) -> Vec<ValidationGateConfig> {
    let locations = discover_project_locations(project_path, 2);
    let mut all_gates = Vec::new();

    for loc in &locations {
        let loc_gates =
            generate_location_validation_gates(&loc.full_path, loc.relative_path.as_deref());
        all_gates.extend(loc_gates);
    }

    // Universal Repository Check (git status --short) at repository root
    if project_path.join(".git").exists() || all_gates.is_empty() {
        all_gates.push(ValidationGateConfig {
            id: "git-status".to_string(),
            name: "Repository Check (git status)".to_string(),
            category: Some(ValidationCategory::RepositoryCheck),
            executable: "git".to_string(),
            args: vec!["status".to_string(), "--short".to_string()],
            enabled: true,
            working_dir: None,
            fail_on_error: false,
            is_advanced_custom: false,
            success_criteria: Some(GateSuccessCriteria::EmptyOutput),
        });
    }

    if all_gates.is_empty() {
        return default_validation_gates();
    }

    // Deduplicate gates by (id, working_dir, executable, args, success_criteria)
    let mut seen = std::collections::HashSet::new();
    let mut deduped_gates = Vec::new();
    for gate in all_gates {
        let key = (
            gate.id.clone(),
            gate.working_dir.clone(),
            gate.executable.clone(),
            gate.args.clone(),
            gate.success_criteria.clone(),
        );
        if seen.insert(key) {
            deduped_gates.push(gate);
        }
    }

    deduped_gates
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

    let mut human_gated_map = HashMap::new();
    human_gated_map.insert(AgentRole::Planner, "deepseek-v41-flash".to_string());
    human_gated_map.insert(AgentRole::PlanIntegrator, "antigravity-harness".to_string());
    human_gated_map.insert(AgentRole::PlanReviewer, "deepseek-v41-flash".to_string());
    human_gated_map.insert(AgentRole::Implementer, "antigravity-harness".to_string());
    human_gated_map.insert(AgentRole::Fixer, "antigravity-harness".to_string());
    human_gated_map.insert(AgentRole::CodeReviewer, "codex-cli".to_string());

    vec![
        OrchestratorPreset {
            id: "human-gated-development-loop".to_string(),
            name: "Human-Gated Development Loop (DeepSeek + Antigravity + Codex)".to_string(),
            description: "Complete autonomous development loop with DeepSeek planning, Antigravity implementation & fixing, disposable worktree Codex review, and interactive Human Gate.".to_string(),
            assignments: human_gated_map,
            iteration_limits: Some(LoopIterationLimits {
                max_plan_review_iterations: 3,
                max_fix_iterations: 5,
                max_code_review_iterations: 3,
            }),
            validation_gates: Some(default_validation_gates()),
            budget_limits: None,
        },
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
        AgentRole::PlanIntegrator,
        RoleAssignment {
            role: AgentRole::PlanIntegrator,
            profile_id: "antigravity-harness".to_string(),
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
        auto_validation_enabled: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_gates_malformed_package_json() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        std::fs::write(path.join("package.json"), "invalid json content").unwrap();

        let gates = generate_project_validation_gates(path);
        // Malformed json should not panic and should default to git-status/fallback
        assert!(!gates.is_empty());
    }

    #[test]
    fn test_generate_gates_node_vitest() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let pkg_json = r#"{
            "scripts": {
                "typecheck": "tsc --noEmit",
                "test": "vitest run",
                "lint": "eslint ."
            },
            "devDependencies": {
                "vitest": "^1.0.0"
            }
        }"#;
        std::fs::write(path.join("package.json"), pkg_json).unwrap();
        std::fs::write(path.join("tsconfig.json"), "{}").unwrap();

        let gates = generate_project_validation_gates(path);
        let ids: Vec<&str> = gates.iter().map(|g| g.id.as_str()).collect();
        assert!(ids.contains(&"typecheck"));
        assert!(ids.contains(&"test"));
        assert!(ids.contains(&"npm-lint"));
    }

    #[test]
    fn test_generate_gates_python_pyproject() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        let pyproject = r#"
[tool.ruff]
line-length = 88

[tool.pytest.ini_options]
minversion = "6.0"
"#;
        std::fs::write(path.join("pyproject.toml"), pyproject).unwrap();

        let gates = generate_project_validation_gates(path);
        let ids: Vec<&str> = gates.iter().map(|g| g.id.as_str()).collect();
        assert!(ids.contains(&"ruff-check"));
        assert!(ids.contains(&"ruff-format"));
        assert!(ids.contains(&"pytest"));
    }

    #[test]
    fn test_generate_gates_go() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        std::fs::write(path.join("go.mod"), "module example.com/test\n\ngo 1.22\n").unwrap();

        let gates = generate_project_validation_gates(path);
        let ids: Vec<&str> = gates.iter().map(|g| g.id.as_str()).collect();
        assert!(ids.contains(&"go-vet"));
        assert!(ids.contains(&"go-test"));
        assert!(ids.contains(&"go-build"));
    }

    #[test]
    fn test_generate_gates_rust_unique_ids() {
        let dir = tempdir().unwrap();
        let path = dir.path();
        std::fs::write(path.join("Cargo.toml"), "[package]\nname = \"test-crate\"\nversion = \"0.1.0\"\n").unwrap();

        let gates = generate_project_validation_gates(path);
        let mut id_set = std::collections::HashSet::new();
        for gate in &gates {
            assert!(
                id_set.insert(&gate.id),
                "Duplicate gate ID detected in generated gates: {}",
                gate.id
            );
        }

        let ids: Vec<&str> = gates.iter().map(|g| g.id.as_str()).collect();
        assert_eq!(ids.iter().filter(|&&id| id == "cargo-check").count(), 1);
        assert_eq!(ids.iter().filter(|&&id| id == "cargo-test").count(), 1);
        assert_eq!(ids.iter().filter(|&&id| id == "cargo-clippy").count(), 1);
        assert_eq!(ids.iter().filter(|&&id| id == "cargo-fmt").count(), 1);
        assert_eq!(ids.iter().filter(|&&id| id == "cargo-build").count(), 1);
    }

    #[test]
    fn test_generate_gates_nested_tauri_monorepo() {
        let dir = tempdir().unwrap();
        let path = dir.path();

        // 1. Root .git directory
        std::fs::create_dir_all(path.join(".git")).unwrap();

        // 2. Nested gui: package.json + tsconfig.json
        let gui_dir = path.join("gui");
        std::fs::create_dir_all(&gui_dir).unwrap();
        let pkg_json = r#"{
            "scripts": {
                "typecheck": "tsc --noEmit",
                "test": "vitest run",
                "build": "vite build"
            },
            "devDependencies": {
                "vitest": "^1.0.0",
                "typescript": "^5.0.0"
            }
        }"#;
        std::fs::write(gui_dir.join("package.json"), pkg_json).unwrap();
        std::fs::write(gui_dir.join("tsconfig.json"), "{}").unwrap();

        // 3. Nested gui/src-tauri: Cargo.toml
        let tauri_dir = gui_dir.join("src-tauri");
        std::fs::create_dir_all(&tauri_dir).unwrap();
        let cargo_toml = r#"[package]
name = "anthro-bridge-gui"
version = "0.24.0"
edition = "2021"
"#;
        std::fs::write(tauri_dir.join("Cargo.toml"), cargo_toml).unwrap();

        let gates = generate_project_validation_gates(path);
        let ids: Vec<&str> = gates.iter().map(|g| g.id.as_str()).collect();

        // gui gates
        assert!(ids.contains(&"gui:typecheck"));
        assert!(ids.contains(&"gui:test"));
        assert!(ids.contains(&"gui:npm-build"));

        // gui/src-tauri gates
        assert!(ids.contains(&"gui/src-tauri:cargo-check"));
        assert!(ids.contains(&"gui/src-tauri:cargo-test"));
        assert!(ids.contains(&"gui/src-tauri:cargo-clippy"));
        assert!(ids.contains(&"gui/src-tauri:cargo-fmt"));
        assert!(ids.contains(&"gui/src-tauri:cargo-build"));

        // root gate
        assert!(ids.contains(&"git-status"));

        // Check working_dirs
        let typecheck_gate = gates.iter().find(|g| g.id == "gui:typecheck").unwrap();
        assert_eq!(typecheck_gate.working_dir, Some("gui".to_string()));

        let cargo_check_gate = gates
            .iter()
            .find(|g| g.id == "gui/src-tauri:cargo-check")
            .unwrap();
        assert_eq!(
            cargo_check_gate.working_dir,
            Some("gui/src-tauri".to_string())
        );

        let git_status_gate = gates.iter().find(|g| g.id == "git-status").unwrap();
        assert_eq!(git_status_gate.working_dir, None);
    }

    #[test]
    fn test_discover_project_locations_bounded_depth() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Depth 1: apps/web
        let web_dir = root.join("apps").join("web");
        std::fs::create_dir_all(&web_dir).unwrap();
        std::fs::write(web_dir.join("package.json"), "{}").unwrap();

        // Depth 2: packages/core
        let core_dir = root.join("packages").join("core");
        std::fs::create_dir_all(&core_dir).unwrap();
        std::fs::write(core_dir.join("Cargo.toml"), "[package]\nname=\"core\"\n").unwrap();

        // Depth 3: packages/core/sub/deep (should be ignored with max_depth=2)
        let deep_dir = core_dir.join("sub").join("deep");
        std::fs::create_dir_all(&deep_dir).unwrap();
        std::fs::write(deep_dir.join("go.mod"), "module deep\n").unwrap();

        // Ignored dir: node_modules/foo
        let ignored_dir = root.join("node_modules").join("foo");
        std::fs::create_dir_all(&ignored_dir).unwrap();
        std::fs::write(ignored_dir.join("package.json"), "{}").unwrap();

        let locs = discover_project_locations(root, 2);
        let rel_paths: Vec<Option<String>> = locs.into_iter().map(|l| l.relative_path).collect();

        assert!(rel_paths.contains(&None)); // root
        assert!(rel_paths.contains(&Some("apps/web".to_string())));
        assert!(rel_paths.contains(&Some("packages/core".to_string())));
        assert!(!rel_paths.contains(&Some("packages/core/sub/deep".to_string())));
        assert!(!rel_paths.contains(&Some("node_modules/foo".to_string())));
    }
}
