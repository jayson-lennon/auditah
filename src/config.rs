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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

impl Default for Config {
    fn default() -> Self {
        Self {
            commercial_project: false,
            exclude: Vec::new(),
        }
    }
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
    use crate::services::fs::{FsBackend, FsError};
    use error_stack::Report;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    struct FakeFs {
        files: Mutex<HashMap<PathBuf, String>>,
    }
    impl FsBackend for FakeFs {
        fn read_to_string(&self, p: &Path) -> Result<String, Report<FsError>> {
            self.files
                .lock()
                .unwrap()
                .get(p)
                .cloned()
                .ok_or_else(|| Report::new(FsError))
        }
        fn write(&self, p: &Path, c: &str) -> Result<(), Report<FsError>> {
            self.files
                .lock()
                .unwrap()
                .insert(p.to_path_buf(), c.to_string());
            Ok(())
        }
        fn list_dir(&self, p: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(self
                .files
                .lock()
                .unwrap()
                .keys()
                .filter(|k| k.parent() == Some(p))
                .cloned()
                .collect())
        }
        fn walk(&self, _root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(Vec::new())
        }
        fn exists(&self, p: &Path) -> bool {
            self.files.lock().unwrap().contains_key(p)
        }
        fn name(&self) -> &'static str {
            "FakeFs"
        }
    }

    fn fs_with(files: &[(&str, &str)]) -> FsService {
        let mut map = HashMap::new();
        for (k, v) in files {
            map.insert(PathBuf::from(k), (*v).to_string());
        }
        FsService::new(std::sync::Arc::new(FakeFs {
            files: Mutex::new(map),
        }))
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
