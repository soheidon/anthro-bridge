use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio_util::sync::CancellationToken;

use super::process_runner::{ProcessRunner, VALIDATION_SAFETY_TIMEOUT};
use super::types::{
    AuthorizedCustomGate, GateSuccessCriteria, ValidationCategory, ValidationGateConfig,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationGateResult {
    #[serde(alias = "gate_id")]
    pub gate_id: String,
    #[serde(alias = "gate_name")]
    pub gate_name: String,
    pub executable: String,
    pub args: Vec<String>,
    #[serde(alias = "exit_code")]
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    #[serde(alias = "is_truncated")]
    pub is_truncated: bool,
    #[serde(alias = "duration_ms")]
    pub duration_ms: u64,
    #[serde(alias = "fail_on_error")]
    pub fail_on_error: bool,
    #[serde(alias = "timed_out")]
    pub timed_out: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationRunSummary {
    pub passed: bool,
    #[serde(alias = "total_gates_run")]
    pub total_gates_run: usize,
    #[serde(alias = "failed_gate_names")]
    pub failed_gate_names: Vec<String>,
    pub results: Vec<ValidationGateResult>,
    #[serde(alias = "formatted_diagnostics")]
    pub formatted_diagnostics: String,
}

/// Computes a deterministic SHA-256 identity hash for a validation gate command.
pub fn compute_gate_command_hash(
    gate_id: &str,
    executable: &str,
    args: &[String],
    canonical_workdir: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"v1-gate-auth:\n");
    hasher.update(format!("id:{}\n", gate_id).as_bytes());
    hasher.update(format!("exe:{}\n", executable).as_bytes());
    hasher.update(format!("argc:{}\n", args.len()).as_bytes());
    for (i, arg) in args.iter().enumerate() {
        hasher.update(format!("arg[{}]:{}\n", i, arg).as_bytes());
    }
    hasher.update(format!("workdir:{}\n", canonical_workdir).as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Built-in gate definition template.
struct BuiltInGateTemplate {
    id: &'static str,
    executable: &'static str,
    args: &'static [&'static str],
}

static BUILT_IN_GATES: &[BuiltInGateTemplate] = &[
    BuiltInGateTemplate {
        id: "typecheck",
        executable: "npx",
        args: &["tsc", "--noEmit"],
    },
    BuiltInGateTemplate {
        id: "typecheck",
        executable: "npm",
        args: &["run", "typecheck"],
    },
    BuiltInGateTemplate {
        id: "test",
        executable: "npm",
        args: &["test", "--", "--run"],
    },
    BuiltInGateTemplate {
        id: "test",
        executable: "npm",
        args: &["test"],
    },
    BuiltInGateTemplate {
        id: "test",
        executable: "npx",
        args: &["vitest", "run"],
    },
    BuiltInGateTemplate {
        id: "test",
        executable: "npx",
        args: &["jest"],
    },
    BuiltInGateTemplate {
        id: "git-status",
        executable: "git",
        args: &["status", "--short"],
    },
    BuiltInGateTemplate {
        id: "npm-test",
        executable: "npm",
        args: &["test"],
    },
    BuiltInGateTemplate {
        id: "npm-typecheck",
        executable: "npx",
        args: &["tsc", "--noEmit"],
    },
    BuiltInGateTemplate {
        id: "npm-lint",
        executable: "npm",
        args: &["run", "lint"],
    },
    BuiltInGateTemplate {
        id: "npm-format",
        executable: "npm",
        args: &["run", "format:check"],
    },
    BuiltInGateTemplate {
        id: "npm-format",
        executable: "npm",
        args: &["run", "format"],
    },
    BuiltInGateTemplate {
        id: "npm-build",
        executable: "npm",
        args: &["run", "build"],
    },
    BuiltInGateTemplate {
        id: "eslint",
        executable: "npx",
        args: &["eslint", "."],
    },
    BuiltInGateTemplate {
        id: "prettier-check",
        executable: "npx",
        args: &["prettier", "--check", "."],
    },
    BuiltInGateTemplate {
        id: "cargo-check",
        executable: "cargo",
        args: &["check"],
    },
    BuiltInGateTemplate {
        id: "cargo-test",
        executable: "cargo",
        args: &["test"],
    },
    BuiltInGateTemplate {
        id: "cargo-clippy",
        executable: "cargo",
        args: &["clippy", "--", "-D", "warnings"],
    },
    BuiltInGateTemplate {
        id: "cargo-fmt",
        executable: "cargo",
        args: &["fmt", "--check"],
    },
    BuiltInGateTemplate {
        id: "cargo-build",
        executable: "cargo",
        args: &["build"],
    },
    BuiltInGateTemplate {
        id: "pytest",
        executable: "pytest",
        args: &[],
    },
    BuiltInGateTemplate {
        id: "python-unittest",
        executable: "python",
        args: &["-m", "unittest"],
    },
    BuiltInGateTemplate {
        id: "mypy",
        executable: "mypy",
        args: &["."],
    },
    BuiltInGateTemplate {
        id: "ruff-check",
        executable: "ruff",
        args: &["check", "."],
    },
    BuiltInGateTemplate {
        id: "ruff-format",
        executable: "ruff",
        args: &["format", "--check", "."],
    },
    BuiltInGateTemplate {
        id: "go-vet",
        executable: "go",
        args: &["vet", "./..."],
    },
    BuiltInGateTemplate {
        id: "go-test",
        executable: "go",
        args: &["test", "./..."],
    },
    BuiltInGateTemplate {
        id: "go-build",
        executable: "go",
        args: &["build", "./..."],
    },
    BuiltInGateTemplate {
        id: "golangci-lint",
        executable: "golangci-lint",
        args: &["run"],
    },
    BuiltInGateTemplate {
        id: "r-cmd-check",
        executable: "R",
        args: &["CMD", "check", "."],
    },
    BuiltInGateTemplate {
        id: "r-testthat",
        executable: "Rscript",
        args: &["-e", "testthat::test_dir('tests/testthat')"],
    },
    BuiltInGateTemplate {
        id: "r-lintr",
        executable: "Rscript",
        args: &["-e", "lintr::lint_package()"],
    },
    BuiltInGateTemplate {
        id: "r-cmd-build",
        executable: "R",
        args: &["CMD", "build", "."],
    },
    BuiltInGateTemplate {
        id: "clasp-status",
        executable: "clasp",
        args: &["status"],
    },
];

fn is_valid_builtin_gate(gate: &ValidationGateConfig) -> bool {
    let base_id = gate.id.split(':').last().unwrap_or(&gate.id);
    BUILT_IN_GATES.iter().any(|tmpl| {
        (gate.id == tmpl.id || base_id == tmpl.id)
            && gate.executable == tmpl.executable
            && gate.args.as_slice() == tmpl.args
    })
}

#[derive(Debug, Clone, Default)]
pub struct ValidationRunner {
    process_runner: ProcessRunner,
}

impl ValidationRunner {
    pub fn new() -> Self {
        Self {
            process_runner: ProcessRunner::new(),
        }
    }

    /// Executes all enabled validation gates sequentially with authorization, path containment, and timeout.
    pub async fn run_gates(
        &self,
        gates: &[ValidationGateConfig],
        project_root: &Path,
        authorized_custom_gates: &[AuthorizedCustomGate],
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ValidationRunSummary, String> {
        let canonical_root = project_root.canonicalize().map_err(|e| {
            format!(
                "Invalid project root path '{}': {}",
                project_root.display(),
                e
            )
        })?;

        let mut results = Vec::new();
        let mut failed_gate_names = Vec::new();
        let mut overall_passed = true;
        let mut diagnostics = String::new();

        for gate in gates {
            if !gate.enabled {
                continue;
            }

            // Check cancellation before each gate
            if let Some(token) = cancel_token {
                if token.is_cancelled() {
                    return Err("Validation cancelled by user.".to_string());
                }
            }

            // 1. Resolve and validate working directory containment
            let target_workdir = if let Some(ref rel) = gate.working_dir {
                if rel.trim().is_empty() {
                    canonical_root.clone()
                } else {
                    canonical_root.join(rel)
                }
            } else {
                canonical_root.clone()
            };

            let canonical_workdir = match target_workdir.canonicalize() {
                Ok(cd) => cd,
                Err(e) => {
                    let err_msg = format!(
                        "Working directory '{}' does not exist or is inaccessible: {}",
                        target_workdir.display(),
                        e
                    );
                    if gate.fail_on_error {
                        overall_passed = false;
                        failed_gate_names.push(gate.name.clone());
                    }
                    diagnostics.push_str(&format!(
                        "### Gate Failed: {} ({})\nError: {}\n\n",
                        gate.name, gate.id, err_msg
                    ));
                    results.push(ValidationGateResult {
                        gate_id: gate.id.clone(),
                        gate_name: gate.name.clone(),
                        executable: gate.executable.clone(),
                        args: gate.args.clone(),
                        exit_code: -1,
                        success: false,
                        stdout: String::new(),
                        stderr: err_msg,
                        is_truncated: false,
                        duration_ms: 0,
                        fail_on_error: gate.fail_on_error,
                        timed_out: false,
                        cancelled: false,
                    });
                    continue;
                }
            };

            // Path containment check
            if !canonical_workdir.starts_with(&canonical_root) {
                let err_msg = format!(
                    "Security violation: working directory '{}' escapes project root '{}'",
                    canonical_workdir.display(),
                    canonical_root.display()
                );
                if gate.fail_on_error {
                    overall_passed = false;
                    failed_gate_names.push(gate.name.clone());
                }
                diagnostics.push_str(&format!(
                    "### Gate Failed: {} ({})\nError: {}\n\n",
                    gate.name, gate.id, err_msg
                ));
                results.push(ValidationGateResult {
                    gate_id: gate.id.clone(),
                    gate_name: gate.name.clone(),
                    executable: gate.executable.clone(),
                    args: gate.args.clone(),
                    exit_code: -1,
                    success: false,
                    stdout: String::new(),
                    stderr: err_msg,
                    is_truncated: false,
                    duration_ms: 0,
                    fail_on_error: gate.fail_on_error,
                    timed_out: false,
                    cancelled: false,
                });
                continue;
            }

            // 2. Authorization check
            if gate.is_advanced_custom {
                let can_workdir_str = canonical_workdir.to_string_lossy().to_string();
                let expected_hash = compute_gate_command_hash(
                    &gate.id,
                    &gate.executable,
                    &gate.args,
                    &can_workdir_str,
                );
                let is_authorized = authorized_custom_gates.iter().any(|auth| {
                    auth.gate_id == gate.id
                        && auth.executable == gate.executable
                        && auth.args == gate.args
                        && auth.canonical_working_dir == can_workdir_str
                        && auth.command_hash == expected_hash
                });

                if !is_authorized {
                    let err_msg = format!(
                        "Unauthorized custom validation gate '{}' (hash: {}). Explicit approval required.",
                        gate.name, expected_hash
                    );
                    if gate.fail_on_error {
                        overall_passed = false;
                        failed_gate_names.push(gate.name.clone());
                    }
                    diagnostics.push_str(&format!(
                        "### Gate Failed: {} ({})\nError: {}\n\n",
                        gate.name, gate.id, err_msg
                    ));
                    results.push(ValidationGateResult {
                        gate_id: gate.id.clone(),
                        gate_name: gate.name.clone(),
                        executable: gate.executable.clone(),
                        args: gate.args.clone(),
                        exit_code: -1,
                        success: false,
                        stdout: String::new(),
                        stderr: err_msg,
                        is_truncated: false,
                        duration_ms: 0,
                        fail_on_error: gate.fail_on_error,
                        timed_out: false,
                        cancelled: false,
                    });
                    continue;
                }
            } else if !is_valid_builtin_gate(gate) {
                let err_msg = format!(
                    "Invalid built-in gate configuration for '{}': modified executable or arguments must be authorized as a custom gate.",
                    gate.id
                );
                if gate.fail_on_error {
                    overall_passed = false;
                    failed_gate_names.push(gate.name.clone());
                }
                diagnostics.push_str(&format!(
                    "### Gate Failed: {} ({})\nError: {}\n\n",
                    gate.name, gate.id, err_msg
                ));
                results.push(ValidationGateResult {
                    gate_id: gate.id.clone(),
                    gate_name: gate.name.clone(),
                    executable: gate.executable.clone(),
                    args: gate.args.clone(),
                    exit_code: -1,
                    success: false,
                    stdout: String::new(),
                    stderr: err_msg,
                    is_truncated: false,
                    duration_ms: 0,
                    fail_on_error: gate.fail_on_error,
                    timed_out: false,
                    cancelled: false,
                });
                continue;
            }

            // 3. Execute gate via ProcessRunner
            let proc_res = self
                .process_runner
                .run(
                    &gate.executable,
                    &gate.args,
                    Some(&canonical_workdir),
                    VALIDATION_SAFETY_TIMEOUT,
                    cancel_token,
                )
                .await;

            let res = match proc_res {
                Ok(pr) => {
                    let exit_code = pr.exit_code.unwrap_or(-1);
                    let mut success = pr.exit_code == Some(0) && !pr.timed_out && !pr.cancelled;
                    let mut stderr = pr.stderr.text;
                    let is_empty_output_criteria = gate.success_criteria
                        == Some(GateSuccessCriteria::EmptyOutput)
                        || (!gate.is_advanced_custom
                            && gate.executable == "git"
                            && gate.args == ["status", "--short"]);
                    if success && is_empty_output_criteria {
                        let trimmed = pr.stdout.text.trim();
                        if !trimmed.is_empty() {
                            success = false;
                            let msg = if gate.executable == "git" && gate.args == ["status", "--short"] {
                                format!("Repository working tree is dirty:\n{}", trimmed)
                            } else {
                                format!("Validation gate failed: expected empty output, but received:\n{}", trimmed)
                            };
                            if stderr.is_empty() {
                                stderr = msg;
                            } else {
                                stderr = format!("{}\n{}", stderr, msg);
                            }
                        }
                    }
                    ValidationGateResult {
                        gate_id: gate.id.clone(),
                        gate_name: gate.name.clone(),
                        executable: gate.executable.clone(),
                        args: gate.args.clone(),
                        exit_code,
                        success,
                        stdout: pr.stdout.text,
                        stderr,
                        is_truncated: pr.stdout.is_truncated || pr.stderr.is_truncated,
                        duration_ms: pr.duration_ms,
                        fail_on_error: gate.fail_on_error,
                        timed_out: pr.timed_out,
                        cancelled: pr.cancelled,
                    }
                }
                Err(e) => ValidationGateResult {
                    gate_id: gate.id.clone(),
                    gate_name: gate.name.clone(),
                    executable: gate.executable.clone(),
                    args: gate.args.clone(),
                    exit_code: -1,
                    success: false,
                    stdout: String::new(),
                    stderr: e,
                    is_truncated: false,
                    duration_ms: 0,
                    fail_on_error: gate.fail_on_error,
                    timed_out: false,
                    cancelled: false,
                },
            };

            if !res.success && gate.fail_on_error {
                overall_passed = false;
                failed_gate_names.push(gate.name.clone());
            }

            if !res.success {
                diagnostics.push_str(&format!(
                    "### Gate Failed: {} (`{} {}`)\nExit Code: {}\n",
                    gate.name,
                    gate.executable,
                    gate.args.join(" "),
                    res.exit_code
                ));
                if res.timed_out {
                    diagnostics.push_str("Status: TIMED OUT after safety limit (30 minutes)\n");
                }
                if res.cancelled {
                    diagnostics.push_str("Status: CANCELLED\n");
                }
                if !res.stdout.is_empty() {
                    diagnostics.push_str(&format!(
                        "#### Standard Output\n```\n{}\n```\n",
                        res.stdout.trim()
                    ));
                }
                if !res.stderr.is_empty() {
                    diagnostics.push_str(&format!(
                        "#### Standard Error\n```\n{}\n```\n",
                        res.stderr.trim()
                    ));
                }
                diagnostics.push('\n');
            }

            results.push(res);
        }

        Ok(ValidationRunSummary {
            passed: overall_passed,
            total_gates_run: results.len(),
            failed_gate_names,
            results,
            formatted_diagnostics: diagnostics,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_validation_runner_builtin_and_custom_auth() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let runner = ValidationRunner::new();

        let can_root = root.canonicalize().unwrap();
        let can_root_str = can_root.to_string_lossy().to_string();

        let custom_gate = ValidationGateConfig {
            id: "custom-echo".to_string(),
            name: "Custom echo".to_string(),
            #[cfg(windows)]
            executable: "powershell".to_string(),
            #[cfg(not(windows))]
            executable: "echo".to_string(),
            #[cfg(windows)]
            args: vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "Write-Output authorized_test".to_string(),
            ],
            #[cfg(not(windows))]
            args: vec!["authorized_test".to_string()],
            enabled: true,
            category: None,
            success_criteria: None,
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: true,
        };

        let hash = compute_gate_command_hash(
            &custom_gate.id,
            &custom_gate.executable,
            &custom_gate.args,
            &can_root_str,
        );

        // Run without authorization -> fails
        let summary_unauth = runner
            .run_gates(&[custom_gate.clone()], root, &[], None)
            .await
            .unwrap();
        assert!(!summary_unauth.passed);
        assert!(summary_unauth.results[0]
            .stderr
            .contains("Unauthorized custom validation gate"));

        // Run with authorization -> passes
        let auth_record = AuthorizedCustomGate {
            gate_id: custom_gate.id.clone(),
            executable: custom_gate.executable.clone(),
            args: custom_gate.args.clone(),
            canonical_working_dir: can_root_str.clone(),
            command_hash: hash,
        };

        let summary_auth = runner
            .run_gates(&[custom_gate], root, &[auth_record], None)
            .await
            .unwrap();
        assert!(summary_auth.passed);
        assert_eq!(summary_auth.results[0].exit_code, 0);
        assert!(summary_auth.results[0].stdout.contains("authorized_test"));
    }

    #[tokio::test]
    async fn test_empty_output_success_criteria() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let runner = ValidationRunner::new();

        // Builtin git-status gate with EmptyOutput criteria
        let gate = ValidationGateConfig {
            id: "git-status".to_string(),
            name: "Repository Status Check".to_string(),
            executable: "git".to_string(),
            args: vec!["status".to_string(), "--short".to_string()],
            enabled: true,
            category: Some(ValidationCategory::RepositoryCheck),
            success_criteria: Some(GateSuccessCriteria::EmptyOutput),
            working_dir: None,
            fail_on_error: true,
            is_advanced_custom: false,
        };

        // Initialize a clean git repo in tempdir
        let _ = std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output();

        let summary = runner
            .run_gates(&[gate.clone()], root, &[], None)
            .await
            .unwrap();
        assert!(summary.passed);
        assert_eq!(summary.results[0].exit_code, 0);

        // Create an untracked file to make git status non-empty
        std::fs::write(root.join("untracked.txt"), "hello").unwrap();

        let summary_dirty = runner
            .run_gates(&[gate], root, &[], None)
            .await
            .unwrap();
        assert!(!summary_dirty.passed);
        assert!(!summary_dirty.results[0].success);
        assert_eq!(summary_dirty.results[0].exit_code, 0);
        assert!(summary_dirty.results[0]
            .stderr
            .contains("Repository working tree is dirty"));
    }

    #[test]
    fn test_gate_command_hash_stability_and_collision_resistance() {
        let h1 = compute_gate_command_hash(
            "g1",
            "cargo",
            &["test".to_string(), "foo".to_string()],
            "/path",
        );
        let h2 = compute_gate_command_hash(
            "g1",
            "cargo",
            &["test".to_string(), "foo".to_string()],
            "/path",
        );
        assert_eq!(h1, h2);

        // Argument separation resistance: ["ab", "c"] vs ["a", "bc"]
        let h_ab_c =
            compute_gate_command_hash("g1", "cargo", &["ab".to_string(), "c".to_string()], "/path");
        let h_a_bc =
            compute_gate_command_hash("g1", "cargo", &["a".to_string(), "bc".to_string()], "/path");
        assert_ne!(h_ab_c, h_a_bc);
    }

    #[test]
    fn test_all_builtin_gate_variations_are_valid() {
        for tmpl in BUILT_IN_GATES {
            let gate = ValidationGateConfig {
                id: tmpl.id.to_string(),
                name: tmpl.id.to_string(),
                executable: tmpl.executable.to_string(),
                args: tmpl.args.iter().map(|s| s.to_string()).collect(),
                enabled: true,
                category: None,
                success_criteria: None,
                working_dir: None,
                fail_on_error: true,
                is_advanced_custom: false,
            };
            assert!(
                is_valid_builtin_gate(&gate),
                "Built-in gate candidate with id='{}', executable='{}', args='{:?}' should be recognized as valid",
                tmpl.id,
                tmpl.executable,
                tmpl.args
            );
        }
    }

    #[test]
    fn test_prefixed_subproject_builtin_gates_are_valid() {
        let gate = ValidationGateConfig {
            id: "gui/src-tauri:cargo-check".to_string(),
            name: "Static Check (cargo check in gui/src-tauri)".to_string(),
            executable: "cargo".to_string(),
            args: vec!["check".to_string()],
            enabled: true,
            category: Some(ValidationCategory::StaticCheck),
            success_criteria: Some(GateSuccessCriteria::ExitZero),
            working_dir: Some("gui/src-tauri".to_string()),
            fail_on_error: true,
            is_advanced_custom: false,
        };
        assert!(is_valid_builtin_gate(&gate));

        let node_gate = ValidationGateConfig {
            id: "gui:typecheck".to_string(),
            name: "Static Check (npm run typecheck in gui)".to_string(),
            executable: "npm".to_string(),
            args: vec!["run".to_string(), "typecheck".to_string()],
            enabled: true,
            category: Some(ValidationCategory::StaticCheck),
            success_criteria: Some(GateSuccessCriteria::ExitZero),
            working_dir: Some("gui".to_string()),
            fail_on_error: true,
            is_advanced_custom: false,
        };
        assert!(is_valid_builtin_gate(&node_gate));
    }
}
