//! Project configuration (`auditah.toml`): commercial-use flag + exclude globs.

use error_stack::{Report, ResultExt};
use serde::{Deserialize, Serialize};
use wherror::Error;

use crate::services::FsService;

use crate::discovery::enumerator::ExcludeMatcher;

/// Error loading or parsing `auditah.toml`.
#[derive(Debug, Error)]
#[error(debug)]
pub struct ConfigError;

/// Project root configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Whether the consuming project is commercial. When true, assets whose
    /// effective terms have `allows_commercial_use = false` FAIL the audit.
    #[serde(default)]
    pub commercial_project: bool,
    /// Whether the project redistributes assets (re-hosts / resells the raw
    /// asset itself, not just shipping it embedded in a product). When true,
    /// assets whose effective terms have `allows_redistribution = false` FAIL.
    #[serde(default)]
    pub redistributes_assets: bool,
    /// SPDX license ids whose `manual_review` obligation has been reviewed and
    /// acknowledged for this project. An acknowledged id suppresses its
    /// `ManualReviewRequired` FAIL. Acknowledgment is permanent and silent.
    #[serde(default)]
    pub manual_review_acknowledged: Vec<String>,
    /// User-supplied glob patterns to exclude from enumeration (in addition to
    /// the built-in default excludes). Matched against paths relative to root.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// File name of the project config at the project root.
pub const CONFIG_FILENAME: &str = "auditah.toml";

/// Validate that the default + user exclude globs all compile.
///
/// Called eagerly in [`Config::load`] so an invalid glob in `auditah.toml`
/// fails fast at config load rather than panicking later during the audit walk.
/// The globs are re-compiled inside `build_excludes` at use time (defense in
/// depth: a `Config` constructed directly, bypassing `load`, is still caught there).
fn validate_excludes(exclude: &[String]) -> Result<(), Report<ConfigError>> {
    let patterns = crate::discovery::all_excludes(exclude);
    ExcludeMatcher::new(&patterns)
        .change_context(ConfigError)
        .attach("invalid exclude glob in auditah.toml")?;
    Ok(())
}

impl Config {
    /// Load `auditah.toml` from `root`. Returns default config if the file is
    /// absent (configuration is optional).
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load(fs: &FsService, root: &std::path::Path) -> Result<Self, Report<ConfigError>> {
        let path = root.join(CONFIG_FILENAME);
        if !fs.exists(&path) {
            return Ok(Self::default());
        }
        let content = fs
            .read_to_string(&path)
            .change_context(ConfigError)
            .attach("failed to read project config")?;
        let cfg: Self = toml::from_str(&content)
            .change_context(ConfigError)
            .attach("failed to parse auditah.toml")
            .attach(path.display().to_string())?;
        validate_excludes(&cfg.exclude)?;
        Ok(cfg)
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    fn fs_with(files: &[(&str, &str)]) -> FsService {
        FsService::new(Arc::new(FakeFs::with_files(
            files.iter().map(|(p, c)| (*p, *c)),
        )))
    }

    #[test]
    fn missing_config_returns_default() {
        // Given a project with no auditah.toml.
        let fs = fs_with(&[]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then defaults are used (non-commercial, no redistribution, no acks, no excludes).
        assert!(!cfg.commercial_project);
        assert!(!cfg.redistributes_assets);
        assert!(cfg.manual_review_acknowledged.is_empty());
        assert!(cfg.exclude.is_empty());
    }

    #[test]
    fn commercial_flag_parses() {
        // Given a config with commercial_project = true.
        let fs = fs_with(&[("/proj/auditah.toml", "commercial_project = true\n")]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then the commercial flag is true.
        assert!(cfg.commercial_project);
    }

    #[test]
    fn exclude_globs_parse() {
        // Given a config with exclude globs.
        let fs = fs_with(&[(
            "/proj/auditah.toml",
            r#"
commercial_project = false
exclude = ["vendor/**", "*.bak"]
"#,
        )]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then the exclude globs parse into the expected vec.
        assert_eq!(cfg.exclude, vec!["vendor/**", "*.bak"]);
    }

    #[test]
    fn malformed_config_errors() {
        // Given a malformed auditah.toml.
        let fs = fs_with(&[("/proj/auditah.toml", "this is not = = valid toml")]);

        // When loading the config.
        let result = Config::load(&fs, Path::new("/proj"));

        // Then loading returns an error.
        assert!(result.is_err());
    }

    #[test]
    fn redistributes_assets_flag_parses() {
        // Given a config with redistributes_assets = true.
        let fs = fs_with(&[("/proj/auditah.toml", "redistributes_assets = true\n")]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then the redistribution flag is true.
        assert!(cfg.redistributes_assets);
    }

    #[test]
    fn manual_review_acknowledged_list_parses() {
        // Given a config with an acknowledged license id.
        let fs = fs_with(&[(
            "/proj/auditah.toml",
            "manual_review_acknowledged = [\"LicenseRef-StudioEULA\", \"OFL-1.1\"]\n",
        )]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then both acknowledged ids parse into the vec.
        assert_eq!(
            cfg.manual_review_acknowledged,
            vec!["LicenseRef-StudioEULA", "OFL-1.1"]
        );
    }

    #[test]
    fn invalid_exclude_glob_fails_fast_at_config_load() {
        // Given an auditah.toml with an invalid exclude glob.
        let fs = fs_with(&[("/proj/auditah.toml", "exclude = [\"**/[invalid\"]\n")]);

        // When loading the config.
        let result = Config::load(&fs, Path::new("/proj"));

        // Then loading errors eagerly (glob validated at config load).
        assert!(
            result.is_err(),
            "invalid exclude glob must fail fast at Config::load"
        );
    }

    #[test]
    fn valid_exclude_globs_load_and_preserve() {
        // Given an auditah.toml with valid exclude globs.
        let fs = fs_with(&[(
            "/proj/auditah.toml",
            "exclude = [\"vendor/**\", \"*.bak\"]\n",
        )]);

        // When loading the config.
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();

        // Then the globs validate and are preserved unchanged.
        assert_eq!(cfg.exclude, vec!["vendor/**", "*.bak"]);
    }
}
