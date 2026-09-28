use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

use super::process_runner::{ProcessRunner, GIT_COMMAND_SAFETY_TIMEOUT};
use super::secrets::SecretRedactor;
use super::token_estimator::{TokenCountQuality, TokenEstimator};
use super::types::{OrchestratorProfile, ReviewFinding};
use crate::model_capabilities::try_resolve_static_context_window;

const MAX_FILE_SIZE_BYTES: u64 = 512 * 1024; // 512 KB
const MIN_PROJECT_CONTEXT_TOKENS: usize = 2_000;
const DEFAULT_OUTPUT_RESERVE_TOKENS: usize = 4_096;

/// Trim report indicating what context sections were included, truncated, or dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrimReport {
    pub task_prompt_retained: bool,
    pub findings_retained: bool,
    pub findings_truncated: bool,
    pub git_diff_retained: bool,
    pub git_diff_truncated: bool,
    pub tier1_specs_retained: bool,
    pub tier1_specs_truncated: bool,
    pub tier5_readme_retained: bool,
    pub dropped_sections: Vec<String>,
    pub estimated_tokens: usize,
    pub token_quality: TokenCountQuality,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltContext {
    pub prompt: String,
    pub trim_report: TrimReport,
    pub estimated_tokens: usize,
    pub token_quality: TokenCountQuality,
}

#[derive(Debug, Clone, Default)]
pub struct ContextBuilder {
    process_runner: ProcessRunner,
    redactor: SecretRedactor,
    token_estimator: TokenEstimator,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self {
            process_runner: ProcessRunner::new(),
            redactor: SecretRedactor::new(),
            token_estimator: TokenEstimator::new(),
        }
    }

    /// Resolves the total context window for a given profile.
    pub fn resolve_profile_context_window(
        &self,
        profile: &OrchestratorProfile,
    ) -> Result<usize, String> {
        if let Some(tokens) = profile.context_window_tokens {
            if tokens > 0 {
                return Ok(tokens as usize);
            }
        }

        if let (Some(ref pid), Some(ref model)) = (&profile.provider_id, &profile.model) {
            if let Some(cw) = try_resolve_static_context_window(pid, model) {
                return Ok(cw.context_length as usize);
            }
        }

        Err(format!(
            "Unable to resolve context window for profile '{}' (adapter: {:?}, model: {:?}). Specify context_window_tokens explicitly or select a model with known context metadata.",
            profile.display_name, profile.adapter, profile.model
        ))
    }

    /// Calculates available budget for project context after subtracting system prompt, task prompt, output reserve, and margin.
    pub fn calculate_project_budget(
        &self,
        total_context_window: usize,
        system_prompt: &str,
        task_prompt: &str,
        quality: TokenCountQuality,
    ) -> Result<usize, String> {
        let sys_tokens = self.token_estimator.estimate_tokens(system_prompt);
        let task_tokens = self.token_estimator.estimate_tokens(task_prompt);

        // Safety margin: 10% for Exact, 25% for Estimated
        let margin_ratio = match quality {
            TokenCountQuality::Exact => 0.10,
            TokenCountQuality::Estimated => 0.25,
        };
        let margin_tokens = (total_context_window as f64 * margin_ratio).ceil() as usize;

        let overhead = sys_tokens
            .saturating_add(task_tokens)
            .saturating_add(DEFAULT_OUTPUT_RESERVE_TOKENS)
            .saturating_add(margin_tokens);
        if total_context_window <= overhead {
            return Err(format!(
                "Context window ({} tokens) is too small for system/task prompt overhead ({} tokens).",
                total_context_window, overhead
            ));
        }

        let remaining = total_context_window - overhead;
        if remaining < MIN_PROJECT_CONTEXT_TOKENS {
            return Err(format!(
                "Remaining budget for project context ({} tokens) is below the minimum required {} tokens.",
                remaining, MIN_PROJECT_CONTEXT_TOKENS
            ));
        }

        Ok(remaining)
    }

    /// Builds context adhering to Collection Tiers 1-3 and Trim Priorities 1-5.
    pub async fn build(
        &self,
        project_path: &Path,
        task_prompt: &str,
        explicit_files: Option<&[&str]>,
        findings: Option<&[ReviewFinding]>,
        max_tokens_budget: Option<usize>,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<BuiltContext, String> {
        if !project_path.exists() {
            return Err(format!(
                "Project directory does not exist: {}",
                project_path.display()
            ));
        }

        let canonical_root = project_path
            .canonicalize()
            .map_err(|e| format!("Invalid project path '{}': {}", project_path.display(), e))?;

        let budget_tokens = max_tokens_budget.unwrap_or(32_000);
        let max_budget_chars = budget_tokens.saturating_mul(4);

        // 1. Priority 1 (Protected): Task prompt
        let p1_task = format!("## Task Instructions\n{}\n", task_prompt.trim());

        // 2. Priority 2: Findings & Diagnostics
        let mut p2_findings = String::new();
        if let Some(f_list) = findings {
            if !f_list.is_empty() {
                p2_findings.push_str("## Active Findings & Validation Errors\n");
                for f in f_list {
                    let loc = match (&f.file, f.line) {
                        (Some(file), Some(line)) => format!(" ({}:{})", file, line),
                        (Some(file), None) => format!(" ({})", file),
                        _ => String::new(),
                    };
                    p2_findings.push_str(&format!(
                        "- [{:?}] {}{}: {}\n",
                        f.severity, f.id, loc, f.issue
                    ));
                    if let Some(ref rec) = f.recommendation {
                        p2_findings.push_str(&format!("  Recommendation: {}\n", rec));
                    }
                }
                p2_findings.push('\n');
            }
        }

        // 3. Priority 3: Git Status & Git Diff (collected asynchronously via ProcessRunner and filtered for secrets)
        let (git_status, git_diff) = self.collect_git_info(&canonical_root, cancel_token).await;
        let mut p3_git = String::new();
        if !git_status.is_empty() {
            p3_git.push_str(&format!(
                "## Git Status\n```\n{}\n```\n\n",
                git_status.trim()
            ));
        }
        if !git_diff.is_empty() {
            p3_git.push_str(&format!(
                "## Git Diff\n```diff\n{}\n```\n\n",
                git_diff.trim()
            ));
        }

        // 4. Priority 4: Tier 1 Specs (SPEC.md, IMPLEMENTATION_PLAN.md, AGENTS.md) + Explicit Tier 3 files
        let mut p4_specs = String::new();
        let tier1_spec_names = ["SPEC.md", "IMPLEMENTATION_PLAN.md", "AGENTS.md"];
        for name in &tier1_spec_names {
            if let Some(content) = self.safe_read_project_file(&canonical_root, name) {
                p4_specs.push_str(&format!(
                    "## Project Spec: {}\n```markdown\n{}\n```\n\n",
                    name,
                    content.trim()
                ));
            }
        }
        if let Some(targets) = explicit_files {
            for target in targets {
                if !tier1_spec_names.contains(target) && *target != "README.md" {
                    if let Some(content) = self.safe_read_project_file(&canonical_root, target) {
                        p4_specs.push_str(&format!(
                            "## Target File: {}\n```\n{}\n```\n\n",
                            target,
                            content.trim()
                        ));
                    }
                }
            }
        }

        // 5. Priority 5: README.md & Supplementary
        let mut p5_readme = String::new();
        if let Some(content) = self.safe_read_project_file(&canonical_root, "README.md") {
            p5_readme.push_str(&format!(
                "## README.md\n```markdown\n{}\n```\n\n",
                content.trim()
            ));
        }

        // Trimming algorithm: trim starting from Priority 5 down to Priority 2
        let mut p5_retained = true;
        let mut p4_retained = true;
        let mut p4_truncated = false;
        let mut p3_retained = true;
        let mut p3_truncated = false;
        let mut p2_retained = true;
        let mut p2_truncated = false;
        let mut dropped_sections = Vec::new();

        let total_chars =
            p1_task.len() + p2_findings.len() + p3_git.len() + p4_specs.len() + p5_readme.len();
        if total_chars > max_budget_chars {
            // Trim Priority 5 first
            if !p5_readme.is_empty() {
                p5_readme.clear();
                p5_retained = false;
                dropped_sections.push("README.md (Priority 5)".to_string());
            }
        }

        let current_len = p1_task.len() + p2_findings.len() + p3_git.len() + p4_specs.len();
        if current_len > max_budget_chars {
            // Trim Priority 4 next
            let budget_for_p4 =
                max_budget_chars.saturating_sub(p1_task.len() + p2_findings.len() + p3_git.len());
            if budget_for_p4 == 0 {
                p4_specs.clear();
                p4_retained = false;
                dropped_sections.push("Specifications / AGENTS (Priority 4)".to_string());
            } else if p4_specs.len() > budget_for_p4 {
                p4_specs = truncate_with_notice(&p4_specs, budget_for_p4);
                p4_truncated = true;
            }
        }

        let current_len = p1_task.len() + p2_findings.len() + p3_git.len() + p4_specs.len();
        if current_len > max_budget_chars {
            // Trim Priority 3 next (Git diff)
            let budget_for_p3 =
                max_budget_chars.saturating_sub(p1_task.len() + p2_findings.len() + p4_specs.len());
            if budget_for_p3 == 0 {
                p3_git.clear();
                p3_retained = false;
                dropped_sections.push("Git diff & status (Priority 3)".to_string());
            } else if p3_git.len() > budget_for_p3 {
                p3_git = truncate_with_notice(&p3_git, budget_for_p3);
                p3_truncated = true;
            }
        }

        let current_len = p1_task.len() + p2_findings.len() + p3_git.len() + p4_specs.len();
        if current_len > max_budget_chars {
            // Trim Priority 2 next (Findings)
            let budget_for_p2 =
                max_budget_chars.saturating_sub(p1_task.len() + p3_git.len() + p4_specs.len());
            if budget_for_p2 == 0 {
                p2_findings.clear();
                p2_retained = false;
                dropped_sections.push("Findings / Errors (Priority 2)".to_string());
            } else if p2_findings.len() > budget_for_p2 {
                p2_findings = truncate_with_notice(&p2_findings, budget_for_p2);
                p2_truncated = true;
            }
        }

        let mut combined_prompt = String::new();
        combined_prompt.push_str(&p1_task);
        if !p2_findings.is_empty() {
            combined_prompt.push_str(&p2_findings);
        }
        if !p4_specs.is_empty() {
            combined_prompt.push_str(&p4_specs);
        }
        if !p3_git.is_empty() {
            combined_prompt.push_str(&p3_git);
        }
        if !p5_readme.is_empty() {
            combined_prompt.push_str(&p5_readme);
        }

        let (estimated_tokens, token_quality) = self
            .token_estimator
            .estimate_prompt_tokens(&combined_prompt);

        let trim_report = TrimReport {
            task_prompt_retained: true,
            findings_retained: p2_retained && !p2_findings.is_empty(),
            findings_truncated: p2_truncated,
            git_diff_retained: p3_retained && !p3_git.is_empty(),
            git_diff_truncated: p3_truncated,
            tier1_specs_retained: p4_retained && !p4_specs.is_empty(),
            tier1_specs_truncated: p4_truncated,
            tier5_readme_retained: p5_retained && !p5_readme.is_empty(),
            dropped_sections,
            estimated_tokens,
            token_quality,
        };

        Ok(BuiltContext {
            prompt: combined_prompt,
            trim_report,
            estimated_tokens,
            token_quality,
        })
    }

    /// Safely reads a project file, ensuring canonical path containment, secret exclusion, and size limits.
    pub fn safe_read_project_file(&self, canonical_root: &Path, rel_path: &str) -> Option<String> {
        let clean_rel = PathBuf::from(rel_path);

        // Disallow path traversal components
        for comp in clean_rel.components() {
            if let std::path::Component::ParentDir = comp {
                return None;
            }
        }

        let target_path = canonical_root.join(&clean_rel);
        let canonical_target = target_path.canonicalize().ok()?;

        // Verify containment
        if !canonical_target.starts_with(canonical_root) {
            return None;
        }

        if !canonical_target.is_file() {
            return None;
        }

        // Check if sensitive file
        if self.redactor.is_sensitive_path(rel_path)
            || self
                .redactor
                .is_sensitive_path(&canonical_target.to_string_lossy())
        {
            return None;
        }

        // Check metadata size
        if let Ok(meta) = fs::metadata(&canonical_target) {
            if meta.len() > MAX_FILE_SIZE_BYTES {
                return Some(format!(
                    "[File '{}' omitted: exceeds 512KB limit]",
                    rel_path
                ));
            }
        }

        let raw = fs::read(&canonical_target).ok()?;
        // Binary check: search for null bytes in first 1024 bytes
        let sample = &raw[..raw.len().min(1024)];
        if sample.contains(&0) {
            return Some(format!("[Binary file '{}' skipped]", rel_path));
        }

        String::from_utf8(raw).ok()
    }

    async fn collect_git_info(
        &self,
        canonical_root: &Path,
        cancel_token: Option<&CancellationToken>,
    ) -> (String, String) {
        let status_res = self
            .process_runner
            .run(
                "git",
                &["status".to_string(), "--short".to_string()],
                Some(canonical_root),
                GIT_COMMAND_SAFETY_TIMEOUT,
                cancel_token,
            )
            .await;

        let status_output = match status_res {
            Ok(res) if res.exit_code == Some(0) => res.stdout.text,
            _ => String::new(),
        };

        let diff_res = self
            .process_runner
            .run(
                "git",
                &["diff".to_string(), "HEAD".to_string()],
                Some(canonical_root),
                GIT_COMMAND_SAFETY_TIMEOUT,
                cancel_token,
            )
            .await;

        let diff_output = match diff_res {
            Ok(res) if res.exit_code == Some(0) => self.redactor.filter_git_diff(&res.stdout.text),
            _ => String::new(),
        };

        (status_output, diff_output)
    }
}

fn truncate_with_notice(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let notice = "\n[... Content truncated due to context window budget ...]\n";
    let take_chars = max_chars.saturating_sub(notice.len());
    let mut truncated = text.chars().take(take_chars).collect::<String>();
    truncated.push_str(notice);
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestrator::types::FindingSeverity;
    use tempfile::tempdir;

    fn unresolved_profile(
        adapter: super::super::types::ExecutionAdapterType,
    ) -> super::super::types::OrchestratorProfile {
        super::super::types::OrchestratorProfile {
            id: "unresolved".to_string(),
            display_name: "Unresolved test profile".to_string(),
            adapter,
            capabilities: Vec::new(),
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
        }
    }

    #[test]
    fn unresolved_ollama_and_cli_context_windows_fail_closed() {
        let builder = ContextBuilder::new();
        for adapter in [
            super::super::types::ExecutionAdapterType::Ollama,
            super::super::types::ExecutionAdapterType::Cli,
        ] {
            assert!(builder
                .resolve_profile_context_window(&unresolved_profile(adapter))
                .is_err());
        }
    }

    #[tokio::test]
    async fn test_context_builder_tiers_and_priorities() {
        let dir = tempdir().unwrap();
        let project_root = dir.path();

        // Create Tier 1 files
        fs::write(
            project_root.join("SPEC.md"),
            "# Spec\nFeature specification.",
        )
        .unwrap();
        fs::write(
            project_root.join("README.md"),
            "# Readme\nProject overview.",
        )
        .unwrap();

        let builder = ContextBuilder::new();
        let findings = vec![ReviewFinding {
            id: "F-01".to_string(),
            severity: FindingSeverity::High,
            category: Some("Logic".to_string()),
            file: Some("src/main.rs".to_string()),
            line: Some(42),
            issue: "Null pointer issue".to_string(),
            recommendation: Some("Use Option".to_string()),
            is_blocking: true,
        }];

        // Build with generous budget
        let res = builder
            .build(
                project_root,
                "Implement login feature",
                None,
                Some(&findings),
                Some(10_000),
                None,
            )
            .await
            .unwrap();

        assert!(res.prompt.contains("Implement login feature"));
        assert!(res.prompt.contains("F-01"));
        assert!(res.prompt.contains("Feature specification"));
        assert!(res.prompt.contains("Project overview"));
        assert!(res.trim_report.task_prompt_retained);
        assert!(res.trim_report.tier5_readme_retained);
        assert!(res.trim_report.tier1_specs_retained);

        // Build with very small budget (e.g. 50 tokens = 200 chars) -> README should be dropped first
        let tight_res = builder
            .build(
                project_root,
                "Implement login feature",
                None,
                Some(&findings),
                Some(50),
                None,
            )
            .await
            .unwrap();

        assert!(tight_res.prompt.contains("Implement login feature"));
        assert!(!tight_res.trim_report.tier5_readme_retained);
        assert!(tight_res
            .trim_report
            .dropped_sections
            .iter()
            .any(|s| s.contains("Priority 5")));
    }

    #[test]
    fn test_sensitive_file_exclusion() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();

        fs::write(root.join(".env"), "SECRET_KEY=12345").unwrap();
        fs::write(root.join("id_rsa"), "PRIVATE KEY").unwrap();
        fs::write(root.join("normal.txt"), "hello world").unwrap();

        let builder = ContextBuilder::new();
        assert!(builder.safe_read_project_file(&root, ".env").is_none());
        assert!(builder.safe_read_project_file(&root, "id_rsa").is_none());
        assert_eq!(
            builder.safe_read_project_file(&root, "normal.txt").unwrap(),
            "hello world"
        );
        // Path traversal rejection
        assert!(builder
            .safe_read_project_file(&root, "../outside.txt")
            .is_none());
    }
}
