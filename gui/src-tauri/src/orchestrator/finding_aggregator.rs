use super::types::{FindingSeverity, ReviewFinding, ReviewResult, ReviewVerdict};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Deserialize)]
struct RawReviewJson {
    verdict: Option<String>,
    summary: Option<String>,
    findings: Option<Vec<RawFindingJson>>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFindingJson {
    id: Option<String>,
    severity: Option<String>,
    category: Option<String>,
    file: Option<String>,
    line: Option<u32>,
    issue: Option<String>,
    recommendation: Option<String>,
    is_blocking: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct FindingAggregator;

impl FindingAggregator {
    pub fn new() -> Self {
        Self
    }

    /// Parses raw LLM text response into a structured ReviewResult.
    pub fn parse_review_output(&self, raw_output: &str) -> ReviewResult {
        // 1. Try parsing JSON (extract from ```json ... ``` or direct JSON)
        if let Some(json_str) = extract_json_block(raw_output) {
            if let Ok(parsed) = serde_json::from_str::<RawReviewJson>(&json_str) {
                let verdict = match parsed
                    .verdict
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .as_str()
                {
                    "approved" | "ready" | "pass" => ReviewVerdict::Approved,
                    "needs_clarification" | "clarify" => ReviewVerdict::NeedsClarification,
                    "failed" | "error" => ReviewVerdict::Failed,
                    _ => ReviewVerdict::ChangesRequired,
                };

                let mut findings = Vec::new();
                if let Some(raw_findings) = parsed.findings {
                    for (idx, rf) in raw_findings.into_iter().enumerate() {
                        let sev = match rf.severity.as_deref().unwrap_or("").to_lowercase().as_str()
                        {
                            "critical" => FindingSeverity::Critical,
                            "high" => FindingSeverity::High,
                            "low" => FindingSeverity::Low,
                            _ => FindingSeverity::Medium,
                        };
                        let is_blocking = rf.is_blocking.unwrap_or(
                            sev == FindingSeverity::Critical || sev == FindingSeverity::High,
                        );
                        findings.push(ReviewFinding {
                            id: rf.id.unwrap_or_else(|| format!("F-{:02}", idx + 1)),
                            severity: sev,
                            category: rf.category,
                            file: rf.file,
                            line: rf.line,
                            issue: rf.issue.unwrap_or_else(|| "Unspecified issue".to_string()),
                            recommendation: rf.recommendation,
                            is_blocking,
                        });
                    }
                }

                return ReviewResult {
                    verdict,
                    findings: self.deduplicate_findings(&findings),
                    summary: parsed.summary.unwrap_or_default(),
                    raw_output: raw_output.to_string(),
                };
            }
        }

        // 2. Fallback: Parse heuristic markdown / text
        let lower = raw_output.to_lowercase();
        let verdict = if lower.contains("verdict: approved")
            || lower.contains("verdict: ready")
            || lower.contains("status: ready")
        {
            ReviewVerdict::Approved
        } else if lower.contains("verdict: needs_clarification") {
            ReviewVerdict::NeedsClarification
        } else {
            ReviewVerdict::ChangesRequired
        };

        let mut findings = Vec::new();
        let mut finding_count = 0;
        for line in raw_output.lines() {
            let trim = line.trim();
            if (trim.starts_with("- [ ]")
                || trim.starts_with("- [x]")
                || trim.starts_with("* ")
                || trim.starts_with("- "))
                && (trim.contains("issue:")
                    || trim.contains("fix:")
                    || trim.contains("bug:")
                    || trim.contains("error:")
                    || trim.contains("warning:"))
            {
                finding_count += 1;
                let sev = if trim.to_lowercase().contains("critical") {
                    FindingSeverity::Critical
                } else if trim.to_lowercase().contains("high") {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                };
                findings.push(ReviewFinding {
                    id: format!("F-{:02}", finding_count),
                    severity: sev,
                    category: None,
                    file: None,
                    line: None,
                    issue: trim.to_string(),
                    recommendation: None,
                    is_blocking: sev == FindingSeverity::Critical || sev == FindingSeverity::High,
                });
            }
        }

        ReviewResult {
            verdict,
            findings,
            summary: raw_output.lines().take(3).collect::<Vec<_>>().join(" "),
            raw_output: raw_output.to_string(),
        }
    }

    /// Deduplicates findings and sorts by severity (Critical > High > Medium > Low).
    pub fn deduplicate_findings(&self, findings: &[ReviewFinding]) -> Vec<ReviewFinding> {
        let mut seen = HashSet::new();
        let mut unique = Vec::new();

        for f in findings {
            let key = (
                f.file.clone().unwrap_or_default(),
                f.line.unwrap_or(0),
                f.issue.trim().to_lowercase(),
            );
            if !seen.contains(&key) {
                seen.insert(key);
                unique.push(f.clone());
            }
        }

        unique.sort_by(|a, b| b.severity.cmp(&a.severity));
        unique
    }

    /// Detects if the exact same blocking finding persists across consecutive iterations.
    pub fn has_repeated_blocking_findings(
        &self,
        prev_findings: &[ReviewFinding],
        curr_findings: &[ReviewFinding],
    ) -> bool {
        let prev_blocking: HashSet<_> = prev_findings
            .iter()
            .filter(|f| f.is_blocking)
            .map(|f| (f.file.clone(), f.line, f.issue.trim().to_lowercase()))
            .collect();

        if prev_blocking.is_empty() {
            return false;
        }

        curr_findings.iter().any(|f| {
            f.is_blocking
                && prev_blocking.contains(&(f.file.clone(), f.line, f.issue.trim().to_lowercase()))
        })
    }
}

fn extract_json_block(text: &str) -> Option<String> {
    if let Some(start_idx) = text.find("```json") {
        let json_part = &text[start_idx + 7..];
        if let Some(end_idx) = json_part.find("```") {
            return Some(json_part[..end_idx].trim().to_string());
        }
    }

    // Direct JSON check
    if let Some(start_idx) = text.find('{') {
        if let Some(end_idx) = text.rfind('}') {
            if end_idx > start_idx {
                return Some(text[start_idx..=end_idx].to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_review_output() {
        let raw = r#"
Here is the review verdict:
```json
{
  "verdict": "changes_required",
  "summary": "Found 2 issues that must be addressed",
  "findings": [
    {
      "id": "F-01",
      "severity": "high",
      "category": "Security",
      "file": "src/auth.rs",
      "line": 105,
      "issue": "Missing input validation",
      "recommendation": "Add regex check",
      "is_blocking": true
    },
    {
      "id": "F-02",
      "severity": "low",
      "category": "Style",
      "file": "src/auth.rs",
      "line": 12,
      "issue": "Unused variable",
      "is_blocking": false
    }
  ]
}
```
"#;
        let agg = FindingAggregator::new();
        let res = agg.parse_review_output(raw);

        assert_eq!(res.verdict, ReviewVerdict::ChangesRequired);
        assert_eq!(res.findings.len(), 2);
        assert_eq!(res.findings[0].id, "F-01");
        assert_eq!(res.findings[0].severity, FindingSeverity::High);
        assert!(res.findings[0].is_blocking);
    }
}
