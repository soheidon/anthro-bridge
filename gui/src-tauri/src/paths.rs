use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// AppChannel — compile-time channel selection
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AppChannel {
    Dev,
    Stable,
}

/// Channel-unset defaults to Dev (safe side).
/// Only an explicit ANTHRO_BRIDGE_CHANNEL=stable selects Stable.
pub(crate) fn app_channel() -> AppChannel {
    match option_env!("ANTHRO_BRIDGE_CHANNEL") {
        Some("stable") => AppChannel::Stable,
        Some("dev") | None => AppChannel::Dev,
        Some(other) => {
            eprintln!(
                "Unknown ANTHRO_BRIDGE_CHANNEL={other}; using isolated dev data directory"
            );
            AppChannel::Dev
        }
    }
}

// ---------------------------------------------------------------------------
// Pure path functions — no env var access, testable without #[ignore]
// ---------------------------------------------------------------------------

/// Pure — no env var access. Testable with temp paths.
pub(crate) fn user_data_dir_for(base: &Path, channel: AppChannel) -> PathBuf {
    match channel {
        AppChannel::Dev => base.join("Anthro Bridge Dev"),
        AppChannel::Stable => base.join("Anthro Bridge"),
    }
}

/// Resolves APPDATA (or USERPROFILE) at runtime.
pub(crate) fn user_data_dir() -> PathBuf {
    let appdata = std::env::var("APPDATA")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    user_data_dir_for(Path::new(&appdata), app_channel())
}

// ---------------------------------------------------------------------------
// Convenience combinators — all rooted in user_data_dir()
// ---------------------------------------------------------------------------

pub(crate) fn config_path() -> PathBuf {
    user_data_dir().join("config.json")
}

pub(crate) fn user_prefs_path() -> PathBuf {
    user_data_dir().join("user_prefs.json")
}

pub(crate) fn openrouter_models_cache_path() -> PathBuf {
    user_data_dir().join("openrouter_models.json")
}

pub(crate) fn log_dir() -> PathBuf {
    user_data_dir().join("Communication-Logs")
}

// ---------------------------------------------------------------------------
// Pure _for variants — testable with temp paths, no env var access
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn config_path_for(base: &Path, channel: AppChannel) -> PathBuf {
    user_data_dir_for(base, channel).join("config.json")
}

#[cfg(test)]
pub(crate) fn user_prefs_path_for(base: &Path, channel: AppChannel) -> PathBuf {
    user_data_dir_for(base, channel).join("user_prefs.json")
}

#[cfg(test)]
pub(crate) fn openrouter_cache_path_for(base: &Path, channel: AppChannel) -> PathBuf {
    user_data_dir_for(base, channel).join("openrouter_models.json")
}

#[cfg(test)]
pub(crate) fn log_dir_for(base: &Path, channel: AppChannel) -> PathBuf {
    user_data_dir_for(base, channel).join("Communication-Logs")
}

// ---------------------------------------------------------------------------
// Migration policy
// ---------------------------------------------------------------------------

/// Only stable channel migrates from old-product configs (Terra Bridge, APG).
/// Dev channel never touches stable data.
pub(crate) fn should_migrate_old_config(channel: AppChannel) -> bool {
    channel == AppChannel::Stable
}

// ---------------------------------------------------------------------------
// Model-set number parsing — shared with frontend semantics
// ---------------------------------------------------------------------------

/// Parse a canonical "Model N" name where N is a positive integer in standard
/// decimal notation (no leading zeros, no trailing text).
///
/// Accepted:  "Model 1", "Model 42"
/// Rejected:  "Model 0", "Model 01", "Model -1", "Model 1 extra"
///
/// The frontend must use the identical regex: `^Model ([1-9]\d*)$`
pub(crate) fn parse_model_set_number(name: &str) -> Option<u32> {
    let value = name.strip_prefix("Model ")?;
    if value.is_empty()
        || value.starts_with('0')
        || !value.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    value.parse::<u32>().ok().filter(|n| *n > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── AppChannel ──────────────────────────────────────────────────

    #[test]
    fn dev_and_stable_data_dirs_differ() {
        let base = Path::new("test-appdata");
        assert_ne!(
            user_data_dir_for(base, AppChannel::Dev),
            user_data_dir_for(base, AppChannel::Stable),
        );
    }

    #[test]
    fn dev_channel_never_migrates_old_config() {
        assert!(!should_migrate_old_config(AppChannel::Dev));
    }

    #[test]
    fn stable_channel_does_migrate_old_config() {
        assert!(should_migrate_old_config(AppChannel::Stable));
    }

    // ── Path functions ──────────────────────────────────────────────

    #[test]
    fn all_dev_paths_share_dev_root() {
        let base = Path::new("test-appdata");
        let channel = AppChannel::Dev;
        let root = user_data_dir_for(base, channel);

        assert_eq!(
            config_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            user_prefs_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            openrouter_cache_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            log_dir_for(base, channel).parent(),
            Some(root.as_path())
        );
    }

    #[test]
    fn all_stable_paths_share_stable_root() {
        let base = Path::new("test-appdata");
        let channel = AppChannel::Stable;
        let root = user_data_dir_for(base, channel);

        assert_eq!(
            config_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            user_prefs_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            openrouter_cache_path_for(base, channel).parent(),
            Some(root.as_path())
        );
        assert_eq!(
            log_dir_for(base, channel).parent(),
            Some(root.as_path())
        );
    }

    // ── parse_model_set_number ──────────────────────────────────────

    #[test]
    fn parse_model_set_number_valid() {
        assert_eq!(parse_model_set_number("Model 1"), Some(1));
        assert_eq!(parse_model_set_number("Model 42"), Some(42));
        assert_eq!(parse_model_set_number("Model 999"), Some(999));
    }

    #[test]
    fn parse_model_set_number_rejects_zero() {
        assert_eq!(parse_model_set_number("Model 0"), None);
    }

    #[test]
    fn parse_model_set_number_rejects_leading_zeros() {
        assert_eq!(parse_model_set_number("Model 01"), None);
        assert_eq!(parse_model_set_number("Model 001"), None);
    }

    #[test]
    fn parse_model_set_number_rejects_trailing_text() {
        assert_eq!(parse_model_set_number("Model 1 extra"), None);
        assert_eq!(parse_model_set_number("Model 2a"), None);
    }

    #[test]
    fn parse_model_set_number_rejects_non_matching() {
        assert_eq!(parse_model_set_number("Model X"), None);
        assert_eq!(parse_model_set_number("Custom"), None);
        assert_eq!(parse_model_set_number("OpenRouter"), None);
        assert_eq!(parse_model_set_number("OpenRouter: Hy3"), None);
    }

    #[test]
    fn parse_model_set_number_rejects_negative() {
        assert_eq!(parse_model_set_number("Model -1"), None);
    }
}
