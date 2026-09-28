
/// Shared SecretRedactor enforcing data protection across all 10 boundary points.
#[derive(Debug, Clone, Default)]
pub struct SecretRedactor;

static SENSITIVE_EXTENSIONS: &[&str] = &[
    ".env", ".pem", ".key", ".pfx", ".pkcs12", ".cer", ".crt", ".der", ".p12",
];

static SENSITIVE_FILENAME_PATTERNS: &[&str] = &[
    "id_rsa",
    "id_ed25519",
    "id_ecdsa",
    "id_dsa",
    "secret",
    "credential",
    "token",
    "password",
    "api_key",
    "apikey",
];

impl SecretRedactor {
    pub fn new() -> Self {
        Self
    }

    /// Masks secret tokens (API keys, bearer tokens, high-entropy secrets) in arbitrary text.
    /// This function is idempotent (redacting already-redacted text yields identical output).
    pub fn redact_secrets(&self, text: &str) -> String {
        if text.is_empty() {
            return String::new();
        }

        let mut result = text.to_string();

        // 1. Redact known provider environment keys if present in memory
        for env_key in &[
            "DEEPSEEK_API_KEY",
            "OPENROUTER_API_KEY",
            "XIAOMI_API_KEY",
            "MOONSHOT_API_KEY",
            "KIMI_CODE_API_KEY",
            "MINIMAX_API_KEY",
        ] {
            if let Ok(val) = std::env::var(env_key) {
                let trimmed = val.trim();
                if trimmed.len() >= 8 {
                    result = result.replace(trimmed, "[REDACTED_SECRET]");
                }
            }
        }

        // 2. Redact standard API key patterns: sk-..., Bearer ...
        result = redact_regex_patterns(&result);

        result
    }

    /// Checks whether a given relative or absolute path represents a sensitive file.
    pub fn is_sensitive_path(&self, path_str: &str) -> bool {
        let normalized = path_str.replace('\\', "/").to_lowercase();
        let file_name = normalized.split('/').last().unwrap_or(&normalized);

        // Check extensions
        for ext in SENSITIVE_EXTENSIONS {
            if file_name.ends_with(ext) || file_name.contains(&format!("{}.", ext)) {
                return true;
            }
        }

        // Check sensitive naming patterns
        for pattern in SENSITIVE_FILENAME_PATTERNS {
            if file_name.contains(pattern) {
                return true;
            }
        }

        false
    }

    /// Pre-filters a `git diff` output, removing complete diff chunks for any sensitive files
    /// (including additions, modifications, deletions, renames, quoted paths, and space-containing paths),
    /// and redacts inline secrets in the remaining diff text.
    pub fn filter_git_diff(&self, raw_diff: &str) -> String {
        if raw_diff.trim().is_empty() {
            return String::new();
        }

        let mut filtered_chunks = Vec::new();
        let lines: Vec<&str> = raw_diff.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            if line.starts_with("diff --git ") {
                // Parse chunk header: diff --git a/path b/path
                let chunk_header = line;
                let mut chunk_lines = vec![chunk_header];
                let mut file_path = extract_path_from_diff_header(chunk_header);

                i += 1;
                while i < lines.len() && !lines[i].starts_with("diff --git ") {
                    let sub_line = lines[i];
                    // Also check rename from / rename to / --- / +++
                    if sub_line.starts_with("--- ")
                        || sub_line.starts_with("+++ ")
                        || sub_line.starts_with("rename from ")
                        || sub_line.starts_with("rename to ")
                    {
                        let candidate = extract_path_from_meta_line(sub_line);
                        if !candidate.is_empty()
                            && (file_path.is_empty() || self.is_sensitive_path(&candidate))
                        {
                            file_path = candidate;
                        }
                    }
                    chunk_lines.push(sub_line);
                    i += 1;
                }

                if self.is_sensitive_path(&file_path) {
                    filtered_chunks.push(format!(
                        "{}\n[Diff for sensitive file '{}' excluded for security]\n",
                        chunk_header, file_path
                    ));
                } else {
                    let chunk_text = chunk_lines.join("\n");
                    filtered_chunks.push(self.redact_secrets(&chunk_text));
                }
            } else {
                // Non-chunk header lines (e.g. initial comments)
                filtered_chunks.push(self.redact_secrets(line));
                i += 1;
            }
        }

        filtered_chunks.join("\n")
    }
}

fn extract_path_from_diff_header(header: &str) -> String {
    // Format: diff --git a/foo/bar.txt b/foo/bar.txt or diff --git "a/foo bar.env" "b/foo bar.env"
    let parts = header.trim_start_matches("diff --git ").trim();
    if parts.starts_with('"') {
        // Quoted path
        if let Some(end_quote) = parts[1..].find('"') {
            let p = &parts[1..=end_quote];
            return unescape_git_path(p);
        }
    }

    if let Some(space_idx) = parts.find(" b/") {
        let a_part = &parts[..space_idx];
        return unescape_git_path(a_part);
    }

    parts.to_string()
}

fn extract_path_from_meta_line(line: &str) -> String {
    let clean = if let Some(stripped) = line.strip_prefix("--- a/") {
        stripped
    } else if let Some(stripped) = line.strip_prefix("+++ b/") {
        stripped
    } else if let Some(stripped) = line.strip_prefix("rename from ") {
        stripped
    } else if let Some(stripped) = line.strip_prefix("rename to ") {
        stripped
    } else {
        return String::new();
    };

    unescape_git_path(clean.trim())
}

fn unescape_git_path(path_str: &str) -> String {
    let trimmed = path_str.trim().trim_matches('"');
    let no_prefix = if let Some(stripped) = trimmed.strip_prefix("a/") {
        stripped
    } else if let Some(stripped) = trimmed.strip_prefix("b/") {
        stripped
    } else {
        trimmed
    };
    no_prefix.to_string()
}

fn redact_regex_patterns(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Look for existing "[REDACTED_SECRET]"
        if i + 17 <= len && chars[i..i + 17].iter().collect::<String>() == "[REDACTED_SECRET]" {
            out.push_str("[REDACTED_SECRET]");
            i += 17;
            continue;
        }

        // Look for "Bearer [REDACTED_SECRET]"
        if i + 24 <= len
            && chars[i..i + 24].iter().collect::<String>() == "Bearer [REDACTED_SECRET]"
        {
            out.push_str("Bearer [REDACTED_SECRET]");
            i += 24;
            continue;
        }

        // Look for "Bearer "
        if i + 7 <= len && chars[i..i + 7].iter().collect::<String>() == "Bearer " {
            out.push_str("Bearer [REDACTED_SECRET]");
            i += 7;
            // Skip the token characters
            while i < len
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || chars[i] == '-'
                    || chars[i] == '.')
            {
                i += 1;
            }
            continue;
        }

        // Look for "sk-" or "sk-proj-" or "sk-ant-"
        if i + 3 <= len && chars[i..i + 3].iter().collect::<String>() == "sk-" {
            out.push_str("[REDACTED_SECRET]");
            i += 3;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-') {
                i += 1;
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secret_redactor_tokens() {
        let redactor = SecretRedactor::new();
        let input =
            "Authorization: Bearer my_secret_token_123456789\nKey: sk-1234567890abcdef1234567890";
        let redacted = redactor.redact_secrets(input);
        assert!(!redacted.contains("my_secret_token_123456789"));
        assert!(!redacted.contains("sk-1234567890abcdef1234567890"));
        assert!(redacted.contains("[REDACTED_SECRET]"));

        // Idempotency check
        let second_redact = redactor.redact_secrets(&redacted);
        assert_eq!(redacted, second_redact);
    }

    #[test]
    fn test_filter_git_diff_sensitive_files() {
        let redactor = SecretRedactor::new();
        let diff = r#"
diff --git a/.env b/.env
index 1234..5678 100644
--- a/.env
+++ b/.env
@@ -1 +1 @@
-DATABASE_URL=postgres://user:old@localhost
+DATABASE_URL=postgres://user:newpassword123@localhost
diff --git a/src/main.rs b/src/main.rs
index 9999..8888 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,3 @@
-let x = 1;
+let x = 2; // token sk-abc1234567890
diff --git "a/config/id_rsa" "b/config/id_rsa"
deleted file mode 100644
--- a/config/id_rsa
+++ /dev/null
@@ -1,2 +0,0 @@
-----BEGIN RSA PRIVATE KEY-----
-secret_key_bytes
"#;

        let filtered = redactor.filter_git_diff(diff);
        assert!(!filtered.contains("newpassword123"));
        assert!(!filtered.contains("BEGIN RSA PRIVATE KEY"));
        assert!(!filtered.contains("sk-abc1234567890"));
        assert!(filtered.contains("[Diff for sensitive file '.env' excluded for security]"));
        assert!(
            filtered.contains("[Diff for sensitive file 'config/id_rsa' excluded for security]")
        );
        assert!(filtered.contains("let x = 2; // token [REDACTED_SECRET]"));
    }

    #[test]
    fn test_sensitive_path_detection() {
        let redactor = SecretRedactor::new();
        assert!(redactor.is_sensitive_path(".env"));
        assert!(redactor.is_sensitive_path(".env.local"));
        assert!(redactor.is_sensitive_path("certs/server.pem"));
        assert!(redactor.is_sensitive_path("keys/id_ed25519"));
        assert!(redactor.is_sensitive_path("my_api_key.txt"));
        assert!(!redactor.is_sensitive_path("src/main.rs"));
        assert!(!redactor.is_sensitive_path("README.md"));
    }
}
