//! Service layer: dependency-injection container + backend abstractions.

pub mod fs;

pub use fs::{FsBackend, FsError, FsService, RealFs};

use derive_more::Debug;

/// Dependency-injection container. Constructed once in `main` (real backends)
/// or in tests (fakes). Cheap to clone; every field is a service wrapper.
///
/// Fields are added as subsystems come online. `registry` joins in Phase 2.
#[derive(Debug, Clone)]
pub struct Services {
    pub fs: FsService,
    pub registry: crate::registry::LicenseRegistry,
}

impl Services {
    /// Build the production service container backed by the real filesystem.
    ///
    /// Phase 2 will extend this to also load the license registry.
    #[must_use]
    pub fn real() -> Self {
        Self {
            fs: FsService::new(std::sync::Arc::new(RealFs::new())),
            registry: crate::registry::LicenseRegistry::load(
                &FsService::new(std::sync::Arc::new(RealFs::new())),
                std::path::Path::new("."),
            )
            .expect("failed to load license registry"),
        }
    }

    /// Build a service container from explicit parts. Used by tests and by
    /// callers that construct pieces independently (e.g. command runners).
    #[must_use]
    pub fn from_parts(fs: FsService, registry: crate::registry::LicenseRegistry) -> Self {
        Self { fs, registry }
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

    /// In-memory fake backend for unit tests (no real filesystem).
    struct FakeFs {
        files: Mutex<HashMap<PathBuf, String>>,
    }

    impl FakeFs {
        fn empty() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
            }
        }
    }

    impl FsBackend for FakeFs {
        fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| Report::new(FsError))
        }
        fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>> {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), content.to_string());
            Ok(())
        }
        fn list_dir(&self, _path: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(Vec::new())
        }
        fn walk(&self, _root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
            Ok(self.files.lock().unwrap().keys().cloned().collect())
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
        }
        fn name(&self) -> &'static str {
            "FakeFs"
        }
    }

    #[test]
    fn services_real_constructs() {
        let _services = Services::real();
    }

    #[test]
    fn fs_service_round_trip_via_fake_backend() {
        let services = Services {
            fs: FsService::new(std::sync::Arc::new(FakeFs::empty())),
            registry: crate::registry::LicenseRegistry::embedded_only(),
        };
        let path = Path::new("/tmp/fake.txt");
        services.fs.write(path, "hello").unwrap();
        assert!(services.fs.exists(path));
        assert_eq!(services.fs.read_to_string(path).unwrap(), "hello");
    }
}
