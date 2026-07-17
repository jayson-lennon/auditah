//! Project-config service: bundles the resolved project root with the loaded
//! `auditah.toml` [`Config`] into a cheaply-clonable pair (`Arc<Path>` +
//! `Arc<Config>`).
//!
//! Unlike the trait-backed services (`FsService`, `ClockService`,
//! [`crate::registry::LicenseRegistryService`]), `Config` is plain data loaded
//! *through* the already-abstracted `FsBackend` — there is no behaviour to
//! swap, so no `ConfigBackend` trait. The Arc wrapping makes `ConfigService`
//! cheap to clone (two refcount bumps), satisfying the Services-container rule
//! ("every field is cheaply clonable or behind a trait").
//!
//! Carrying `root` together with `config` here lets the [`Services`] container
//! expose both without per-subsystem `*Ctx` structs or `cwd` plumbing.

use std::path::Path;
use std::sync::Arc;

use error_stack::Report;

use crate::config::{Config, ConfigError};
use crate::services::fs::FsService;

/// Cheaply-clonable pair of the resolved project root and the loaded config.
///
/// Stored as a field of [`crate::services::Services`]. Read it via
/// [`ConfigService::root`] / [`ConfigService::config`].
#[derive(Debug, Clone)]
pub struct ConfigService {
    root: Arc<Path>,
    config: Arc<Config>,
}

impl ConfigService {
    /// Construct from pre-built parts.
    #[must_use]
    pub fn new(root: Arc<Path>, config: Arc<Config>) -> Self {
        Self { root, config }
    }

    /// The resolved project root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The loaded `auditah.toml` config.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Load `auditah.toml` from `root` and bundle them together.
    ///
    /// Bootstrap factory: takes a `&FsService` (not `&Services`) because it
    /// runs while the [`crate::services::Services`] container is still being
    /// assembled. Returns default config when `auditah.toml` is absent.
    ///
    /// # Errors
    ///
    /// Propagates [`ConfigError`] if `auditah.toml` exists but cannot be read
    /// or parsed.
    pub fn load(fs: &FsService, root: &Path) -> Result<Self, Report<ConfigError>> {
        let cfg = Config::load(fs, root)?;
        Ok(Self::new(Arc::from(root), Arc::new(cfg)))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::services::fs::FsService;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    fn fake_fs_with_config(toml: &str) -> FsService {
        FsService::new(Arc::new(FakeFs::with_files([(
            "/proj/auditah.toml".to_string(),
            toml,
        )])))
    }

    #[test]
    fn config_service_load_reads_toml_and_exposes_root_and_config() {
        // Given an auditah.toml on disk marking the project commercial.
        let fs = fake_fs_with_config("commercial_project = true\n");

        // When loading ConfigService.
        let svc = ConfigService::load(&fs, Path::new("/proj")).expect("load");

        // Then root() is the given root.
        assert_eq!(svc.root(), Path::new("/proj"));
        // And the parsed config reflects commercial_project = true.
        assert!(svc.config().commercial_project);
    }

    #[test]
    fn config_service_load_defaults_when_no_toml() {
        // Given an empty filesystem (no auditah.toml).
        let fs = FsService::new(Arc::new(FakeFs::default()));

        // When loading ConfigService.
        let svc = ConfigService::load(&fs, Path::new("/proj")).expect("load");

        // Then config() is the default.
        assert_eq!(svc.config(), &Config::default());
        // And root() is still the given root.
        assert_eq!(svc.root(), Path::new("/proj"));
    }
}
