use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCountQuality {
    Exact,
    Estimated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCountResult {
    pub count: usize,
    pub quality: TokenCountQuality,
}

#[derive(Debug, Clone, Default)]
pub struct TokenEstimator;

impl TokenEstimator {
    pub fn new() -> Self {
        Self
    }

    /// Produces a safety-oriented heuristic for multilingual text, CJK, emoji,
    /// code, and JSON. This is estimated, not a guaranteed upper bound.
    pub fn estimate(&self, text: &str) -> TokenCountResult {
        if text.is_empty() {
            return TokenCountResult {
                count: 0,
                quality: TokenCountQuality::Estimated,
            };
        }

        let mut token_estimate: f64 = 0.0;

        for ch in text.chars() {
            if ch.is_ascii() {
                if ch.is_ascii_whitespace() {
                    token_estimate += 0.25;
                } else if ch.is_ascii_punctuation() {
                    token_estimate += 0.5;
                } else {
                    // ASCII alphanumeric
                    token_estimate += 0.35; // ~1 token per 2.85 chars (conservative)
                }
            } else {
                // Non-ASCII (CJK characters, kana, hangul, emoji, combining marks)
                // Non-ASCII characters are weighted more heavily, while the
                // resulting heuristic remains explicitly non-exact.
                token_estimate += 1.5;
            }
        }

        let total_tokens = token_estimate.ceil() as usize;

        TokenCountResult {
            count: total_tokens.max(1),
            quality: TokenCountQuality::Estimated,
        }
    }

    pub fn estimate_tokens(&self, text: &str) -> usize {
        self.estimate(text).count
    }

    pub fn estimate_prompt_tokens(&self, text: &str) -> (usize, TokenCountQuality) {
        let res = self.estimate(text);
        (res.count, res.quality)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimator_ascii_and_cjk() {
        let estimator = TokenEstimator::new();

        let ascii_text = "fn main() { println!(\"hello world\"); }";
        let res_ascii = estimator.estimate(ascii_text);
        assert!(res_ascii.count > 0);
        assert_eq!(res_ascii.quality, TokenCountQuality::Estimated);

        let japanese_text = "マルチエージェント開発ワークフローの実行計画を策定します。";
        let res_ja = estimator.estimate(japanese_text);
        // Japanese must estimate conservatively (> 1 token per char)
        assert!(res_ja.count >= japanese_text.chars().count());

        // Monotonicity check: longer text must yield greater or equal token count
        let longer_ja = format!("{}{}", japanese_text, "さらに追加の指示文を実行します。");
        assert!(estimator.estimate(&longer_ja).count > res_ja.count);
    }

    #[test]
    fn test_token_estimator_emoji_and_code() {
        let estimator = TokenEstimator::new();
        let code_json = r#"{"name": "test", "items": [1, 2, 3], "status": "🚀 done"}"#;
        let res = estimator.estimate(code_json);
        assert!(res.count > 0);
    }

    #[test]
    fn test_estimator_covers_cjk_mixed_combining_and_json_fixtures() {
        let estimator = TokenEstimator::new();
        let fixtures = [
            "日本語の設計と検証",
            "中文上下文与测试",
            "한국어 문맥과 검증",
            "混合 CJK and ASCII identifiers",
            "e\u{301} combining mark",
            r#"{"key":"値","status":"확인"}"#,
        ];
        for fixture in fixtures {
            let result = estimator.estimate(fixture);
            assert!(result.count > 0, "fixture: {fixture}");
            assert_eq!(result.quality, TokenCountQuality::Estimated);
            assert!(estimator.estimate(&format!("{fixture} extra")).count >= result.count);
        }
    }
}
