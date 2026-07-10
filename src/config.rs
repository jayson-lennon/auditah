//! Project configuration (`auditah.toml`): commercial-use flag + exclude globs.

use error_stack::{Report, ResultExt};
use serde::{Deserialize, Serialize};
use wherror::Error;

use crate::services::FsService;

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
    /// User-supplied glob patterns to exclude from enumeration (in addition to
    /// the built-in default excludes). Matched against paths relative to root.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// File name of the project config at the project root.
pub const CONFIG_FILENAME: &str = "auditah.toml";

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
        toml::from_str(&content)
            .change_context(ConfigError)
            .attach("failed to parse auditah.toml")
            .attach(path.display().to_string())
    }
}

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
        let fs = fs_with(&[]);
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();
        assert!(!cfg.commercial_project);
        assert!(cfg.exclude.is_empty());
    }

    #[test]
    fn commercial_flag_parses() {
        let fs = fs_with(&[("/proj/auditah.toml", "commercial_project = true\n")]);
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();
        assert!(cfg.commercial_project);
    }

    #[test]
    fn exclude_globs_parse() {
        let fs = fs_with(&[(
            "/proj/auditah.toml",
            r#"
commercial_project = false
exclude = ["vendor/**", "*.bak"]
"#,
        )]);
        let cfg = Config::load(&fs, Path::new("/proj")).unwrap();
        assert_eq!(cfg.exclude, vec!["vendor/**", "*.bak"]);
    }

    #[test]
    fn malformed_config_errors() {
        let fs = fs_with(&[("/proj/auditah.toml", "this is not = = valid toml")]);
        assert!(Config::load(&fs, Path::new("/proj")).is_err());
    }
}
