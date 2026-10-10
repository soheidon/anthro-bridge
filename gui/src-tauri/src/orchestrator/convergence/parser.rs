use super::types::{
    PlanConvergenceError, PlannerOperationProposal, PlannerProposal, ReviewDecision, ReviewFinding,
    ReviewedCandidateBinding, ReviewerResponseProposal, ScopeEffect,
};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;

pub const MAX_PAYLOAD_BYTES: usize = 1024 * 1024; // 1 MiB
pub const MAX_FINDINGS: usize = 100;
pub const MAX_STRING_FIELD_LEN: usize = 50_000;
pub const MAX_TITLE_LEN: usize = 500;
pub const MAX_SECTION_TYPE_LEN: usize = 100;

struct StrictJsonValue(serde_json::Value);

struct StrictJsonVisitor;

impl<'de> Visitor<'de> for StrictJsonVisitor {
    type Value = StrictJsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value with no duplicate object keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::Bool(value)))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::Number(value.into())))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        serde_json::Number::from_f64(value)
            .map(|number| StrictJsonValue(serde_json::Value::Number(number)))
            .ok_or_else(|| E::custom("non-finite JSON number"))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::String(value.to_owned())))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::String(value)))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(StrictJsonValue(serde_json::Value::Null))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        self.visit_unit()
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<StrictJsonValue>()? {
            values.push(value.0);
        }
        Ok(StrictJsonValue(serde_json::Value::Array(values)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = serde_json::Map::new();
        let mut seen = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(de::Error::custom(format!(
                    "duplicate JSON object key '{key}'"
                )));
            }
            let value = map.next_value::<StrictJsonValue>()?;
            values.insert(key, value.0);
        }
        Ok(StrictJsonValue(serde_json::Value::Object(values)))
    }
}

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

fn parse_json_rejecting_duplicate_keys(raw: &str) -> Result<serde_json::Value, serde_json::Error> {
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    let value = StrictJsonValue::deserialize(&mut deserializer)?.0;
    deserializer.end()?;
    Ok(value)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Generates deterministic, stable canonical UTF-8 JSON bytes for operation payload hashing.
pub fn canonical_operation_bytes(op: &PlannerOperationProposal) -> Vec<u8> {
    match op {
        PlannerOperationProposal::AppendSection {
            target_plan_id,
            section_type,
            section_title,
            section_content,
        } => {
            let mut map = serde_json::Map::new();
            map.insert("kind".to_string(), serde_json::Value::String("append_section".to_string()));
            map.insert("section_content".to_string(), serde_json::Value::String(section_content.clone()));
            map.insert("section_title".to_string(), serde_json::Value::String(section_title.clone()));
            map.insert("section_type".to_string(), serde_json::Value::String(section_type.clone()));
            map.insert("target_plan_id".to_string(), serde_json::Value::String(target_plan_id.clone()));
            serde_json::to_vec(&serde_json::Value::Object(map)).unwrap_or_default()
        }
        PlannerOperationProposal::NewPrimaryPlan {
            proposed_revision,
            title,
            initial_content,
        } => {
            let mut map = serde_json::Map::new();
            map.insert("initial_content".to_string(), serde_json::Value::String(initial_content.clone()));
            map.insert("kind".to_string(), serde_json::Value::String("new_primary_plan".to_string()));
            map.insert("proposed_revision".to_string(), serde_json::json!(proposed_revision));
            map.insert("title".to_string(), serde_json::Value::String(title.clone()));
            serde_json::to_vec(&serde_json::Value::Object(map)).unwrap_or_default()
        }
    }
}

/// Strictly parses a model-authored planner proposal.
pub fn parse_planner_proposal(raw_text: &str) -> Result<PlannerProposal, PlanConvergenceError> {
    if raw_text.len() > MAX_PAYLOAD_BYTES {
        return Err(PlanConvergenceError::new(
            "PC_CANDIDATE_OVERSIZED",
            format!("Planner proposal size {} bytes exceeds limit of {} bytes", raw_text.len(), MAX_PAYLOAD_BYTES),
        ));
    }

    if raw_text.contains("```") {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_PROPOSAL",
            "Planner proposal contains markdown code fences; exact JSON is required",
        ));
    }

    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_PROPOSAL",
            "Planner proposal must be exactly one JSON object without surrounding prose",
        ));
    }

    let val = parse_json_rejecting_duplicate_keys(trimmed).map_err(|e| {
        PlanConvergenceError::new(
            "PC_INVALID_PROPOSAL",
            format!("Failed to parse planner proposal as JSON: {e}"),
        )
    })?;

    let obj = val.as_object().ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Top-level JSON value must be an object")
    })?;

    // Check unknown top-level keys
    for key in obj.keys() {
        if key != "schemaVersion" && key != "operation" {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_PROPOSAL",
                format!("Unknown top-level field in planner proposal: '{key}'"),
            ));
        }
    }

    let schema_version = obj
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing or invalid schema_version")
        })?;

    if schema_version != 1 {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_PROPOSAL",
            format!("Unsupported schema_version {schema_version}; expected 1"),
        ));
    }

    let op_val = obj.get("operation").ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing 'operation' object in proposal")
    })?;

    let op_obj = op_val.as_object().ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_PROPOSAL", "'operation' must be a JSON object")
    })?;

    let kind = op_obj
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing 'kind' discriminator in operation")
        })?;

    let operation = match kind {
        "append_section" => {
            for key in op_obj.keys() {
                if key != "kind"
                    && key != "targetPlanId"
                    && key != "sectionType"
                    && key != "sectionTitle"
                    && key != "sectionContent"
                {
                    return Err(PlanConvergenceError::new(
                        "PC_INVALID_PROPOSAL",
                        format!("Unknown field in append_section operation: '{key}'"),
                    ));
                }
            }

            let target_plan_id = op_obj
                .get("targetPlanId")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing target_plan_id")
                })?
                .trim()
                .to_string();

            if target_plan_id.is_empty() {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    "target_plan_id cannot be empty",
                ));
            }
            if target_plan_id.contains('/') || target_plan_id.contains('\\') || target_plan_id.contains("..") {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    "target_plan_id cannot contain path separators or traversal components",
                ));
            }

            let section_type = op_obj
                .get("sectionType")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing section_type")
                })?
                .trim()
                .to_string();

            if section_type.is_empty() || section_type.len() > MAX_SECTION_TYPE_LEN {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    format!("section_type must be between 1 and {MAX_SECTION_TYPE_LEN} characters"),
                ));
            }

            let section_title = op_obj
                .get("sectionTitle")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing section_title")
                })?
                .trim()
                .to_string();

            if section_title.is_empty() || section_title.len() > MAX_TITLE_LEN {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    format!("section_title must be between 1 and {MAX_TITLE_LEN} characters"),
                ));
            }

            let section_content = op_obj
                .get("sectionContent")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing section_content")
                })?
                .trim()
                .to_string();

            if section_content.is_empty() {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    "section_content cannot be empty",
                ));
            }

            PlannerOperationProposal::AppendSection {
                target_plan_id,
                section_type,
                section_title,
                section_content,
            }
        }
        "new_primary_plan" => {
            for key in op_obj.keys() {
                if key != "kind"
                    && key != "proposedRevision"
                    && key != "title"
                    && key != "initialContent"
                {
                    return Err(PlanConvergenceError::new(
                        "PC_INVALID_PROPOSAL",
                        format!("Unknown field in new_primary_plan operation: '{key}'"),
                    ));
                }
            }

            let proposed_revision = op_obj
                .get("proposedRevision")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing or invalid proposed_revision")
                })?;

            if proposed_revision == 0 {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    "proposed_revision must be greater than zero",
                ));
            }

            let title = op_obj
                .get("title")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing title")
                })?
                .trim()
                .to_string();

            if title.is_empty() || title.len() > MAX_TITLE_LEN {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    format!("title must be between 1 and {MAX_TITLE_LEN} characters"),
                ));
            }

            let initial_content = op_obj
                .get("initialContent")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    PlanConvergenceError::new("PC_INVALID_PROPOSAL", "Missing initial_content")
                })?
                .trim()
                .to_string();

            if initial_content.is_empty() {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_PROPOSAL",
                    "initial_content cannot be empty",
                ));
            }

            PlannerOperationProposal::NewPrimaryPlan {
                proposed_revision,
                title,
                initial_content,
            }
        }
        unknown => {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_PROPOSAL",
                format!("Unsupported operation kind '{unknown}'; expected 'append_section' or 'new_primary_plan'"),
            ));
        }
    };

    Ok(PlannerProposal {
        schema_version: 1,
        operation,
    })
}

/// Validates that a string is a 64-character lowercase SHA-256 hex string.
fn validate_sha256_hex(digest: &str, field_name: &str) -> Result<(), PlanConvergenceError> {
    if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Field '{field_name}' must be a 64-character lowercase SHA-256 hex string, got '{digest}'"),
        ));
    }
    Ok(())
}

/// Strictly parses a model-authored review proposal.
pub fn parse_reviewer_proposal(raw_text: &str) -> Result<ReviewerResponseProposal, PlanConvergenceError> {
    if raw_text.len() > MAX_PAYLOAD_BYTES {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Reviewer proposal size {} bytes exceeds limit of {} bytes", raw_text.len(), MAX_PAYLOAD_BYTES),
        ));
    }

    if raw_text.contains("```") {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            "Reviewer proposal contains markdown code fences; exact JSON is required",
        ));
    }

    let trimmed = raw_text.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            "Reviewer proposal must be exactly one JSON object without surrounding prose",
        ));
    }

    let val = parse_json_rejecting_duplicate_keys(trimmed).map_err(|e| {
        PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Failed to parse reviewer proposal as JSON: {e}"),
        )
    })?;

    let obj = val.as_object().ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_VERDICT", "Top-level JSON value must be an object")
    })?;

    for key in obj.keys() {
        if key != "schemaVersion"
            && key != "decision"
            && key != "reviewedCandidate"
            && key != "summary"
            && key != "findings"
        {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                format!("Unknown top-level field in reviewer proposal: '{key}'"),
            ));
        }
    }

    let schema_version = obj
        .get("schemaVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing or invalid schema_version")
        })?;

    if schema_version != 1 {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Unsupported schema_version {schema_version}; expected 1"),
        ));
    }

    let decision_str = obj
        .get("decision")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing 'decision' field")
        })?;

    let decision = match decision_str {
        "APPROVE" => ReviewDecision::Approve,
        "REQUEST_CHANGES" => ReviewDecision::RequestChanges,
        "ESCALATE" => ReviewDecision::Escalate,
        other => {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                format!("Invalid review decision '{other}'; expected APPROVE, REQUEST_CHANGES, or ESCALATE"),
            ));
        }
    };

    let rc_val = obj
        .get("reviewedCandidate")
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing 'reviewed_candidate' object")
        })?;

    let rc_obj = rc_val.as_object().ok_or_else(|| {
        PlanConvergenceError::new("PC_INVALID_VERDICT", "'reviewed_candidate' must be a JSON object")
    })?;

    for key in rc_obj.keys() {
        if key != "sequence"
            && key != "candidateId"
            && key != "operationPayloadDigest"
            && key != "basePlanDigest"
            && key != "planContextDigest"
        {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                format!("Unknown field in reviewed_candidate: '{key}'"),
            ));
        }
    }

    let sequence = rc_obj
        .get("sequence")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing or invalid reviewed_candidate.sequence")
        })? as u32;

    if sequence == 0 {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            "reviewed_candidate.sequence must be greater than zero",
        ));
    }

    let candidate_id = rc_obj
        .get("candidateId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing reviewed_candidate.candidate_id")
        })?
        .trim()
        .to_string();

    if candidate_id.is_empty() {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            "reviewed_candidate.candidate_id cannot be empty",
        ));
    }

    let operation_payload_digest = rc_obj
        .get("operationPayloadDigest")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing reviewed_candidate.operation_payload_digest")
        })?
        .trim()
        .to_string();
    validate_sha256_hex(&operation_payload_digest, "operation_payload_digest")?;

    let base_plan_digest = match rc_obj.get("basePlanDigest") {
        Some(v) if v.is_null() => None,
        Some(v) => {
            let s = v.as_str().ok_or_else(|| {
                PlanConvergenceError::new("PC_INVALID_VERDICT", "base_plan_digest must be string or null")
            })?.trim().to_string();
            validate_sha256_hex(&s, "base_plan_digest")?;
            Some(s)
        }
        None => None,
    };

    let plan_context_digest = rc_obj
        .get("planContextDigest")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing reviewed_candidate.plan_context_digest")
        })?
        .trim()
        .to_string();
    validate_sha256_hex(&plan_context_digest, "plan_context_digest")?;

    let reviewed_candidate = ReviewedCandidateBinding {
        sequence,
        candidate_id,
        operation_payload_digest,
        base_plan_digest,
        plan_context_digest,
    };

    let summary = obj
        .get("summary")
        .and_then(|v| v.as_str())
        .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing 'summary' string"))?
        .trim()
        .to_string();

    if summary.len() > MAX_STRING_FIELD_LEN {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Summary length {} exceeds limit of {MAX_STRING_FIELD_LEN}", summary.len()),
        ));
    }

    let findings_arr = obj
        .get("findings")
        .and_then(|v| v.as_array())
        .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing 'findings' array"))?;

    if findings_arr.len() > MAX_FINDINGS {
        return Err(PlanConvergenceError::new(
            "PC_INVALID_VERDICT",
            format!("Findings count {} exceeds maximum of {MAX_FINDINGS}", findings_arr.len()),
        ));
    }

    let mut findings = Vec::with_capacity(findings_arr.len());
    let mut seen_ids = HashSet::new();

    for f_val in findings_arr {
        let f_obj = f_val.as_object().ok_or_else(|| {
            PlanConvergenceError::new("PC_INVALID_VERDICT", "Each finding item must be an object")
        })?;

        for key in f_obj.keys() {
            if key != "findingId"
                && key != "affectedRequirement"
                && key != "problem"
                && key != "whyBlocking"
                && key != "requiredChange"
                && key != "scopeEffect"
                && key != "blocking"
            {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_VERDICT",
                    format!("Unknown field in finding: '{key}'"),
                ));
            }
        }

        let finding_id = f_obj
            .get("findingId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing finding_id in finding")
            })?
            .trim()
            .to_string();

        if finding_id.is_empty() || finding_id.len() > MAX_STRING_FIELD_LEN {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                "finding_id must be nonempty and within length bounds",
            ));
        }

        if !seen_ids.insert(finding_id.clone()) {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                format!("Duplicate finding_id '{finding_id}' in review findings"),
            ));
        }

        let affected_requirement = f_obj
            .get("affectedRequirement")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing affected_requirement")
            })?
            .trim()
            .to_string();

        if affected_requirement.is_empty() || affected_requirement.len() > MAX_STRING_FIELD_LEN {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                "affected_requirement must be nonempty and within length bounds",
            ));
        }

        let problem = f_obj
            .get("problem")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing problem"))?
            .trim()
            .to_string();

        if problem.is_empty() || problem.len() > MAX_STRING_FIELD_LEN {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                "problem must be nonempty and within length bounds",
            ));
        }

        let why_blocking = f_obj
            .get("whyBlocking")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing why_blocking"))?
            .trim()
            .to_string();

        if why_blocking.is_empty() || why_blocking.len() > MAX_STRING_FIELD_LEN {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                "why_blocking must be nonempty and within length bounds",
            ));
        }

        let required_change = f_obj
            .get("requiredChange")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing required_change"))?
            .trim()
            .to_string();

        if required_change.is_empty() || required_change.len() > MAX_STRING_FIELD_LEN {
            return Err(PlanConvergenceError::new(
                "PC_INVALID_VERDICT",
                "required_change must be nonempty and within length bounds",
            ));
        }

        let scope_effect_str = f_obj
            .get("scopeEffect")
            .and_then(|v| v.as_str())
            .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing scope_effect"))?;

        let scope_effect = match scope_effect_str {
            "NONE" => ScopeEffect::None,
            "CURRENT_PLAN_EXPANSION" => ScopeEffect::CurrentPlanExpansion,
            "NEW_REVISION_CANDIDATE" => ScopeEffect::NewRevisionCandidate,
            other => {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_VERDICT",
                    format!("Invalid scope_effect '{other}'; expected NONE, CURRENT_PLAN_EXPANSION, or NEW_REVISION_CANDIDATE"),
                ));
            }
        };

        let blocking = f_obj
            .get("blocking")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| PlanConvergenceError::new("PC_INVALID_VERDICT", "Missing boolean 'blocking'"))?;

        findings.push(ReviewFinding {
            finding_id,
            affected_requirement,
            problem,
            why_blocking,
            required_change,
            scope_effect,
            blocking,
        });
    }

    // Semantic consistency validations:
    match decision {
        ReviewDecision::Approve => {
            if findings.iter().any(|f| f.blocking) {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_VERDICT",
                    "APPROVE decision cannot contain blocking findings",
                ));
            }
            if findings.iter().any(|f| f.scope_effect != ScopeEffect::None) {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_VERDICT",
                    "APPROVE decision cannot contain findings with non-NONE scope_effect",
                ));
            }
        }
        ReviewDecision::RequestChanges => {
            if !findings.iter().any(|f| f.blocking) {
                return Err(PlanConvergenceError::new(
                    "PC_INVALID_VERDICT",
                    "REQUEST_CHANGES decision must contain at least one blocking finding",
                ));
            }
        }
        ReviewDecision::Escalate => {}
    }

    Ok(ReviewerResponseProposal {
        schema_version: 1,
        decision,
        reviewed_candidate,
        summary,
        findings,
    })
}
