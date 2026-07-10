//! Service layer: dependency-injection container + backend abstractions.

pub mod fs;

use std::{path::Path, sync::Arc};

use error_stack::{Report, ResultExt};
pub use fs::{FsBackend, FsError, FsService, RealFs};

use derive_more::Debug;
use wherror::Error;

use crate::registry::LicenseRegistry;

/// Error with the Services.
#[derive(Debug, Error)]
#[error(debug)]
pub struct ServicesError;

/// Dependency-injection container. Constructed once in `main` (real backends)
/// or in tests (fakes). Cheap to clone; every field is a service wrapper.
///
/// Fields are added as subsystems come online. `registry` joins in Phase 2.
#[derive(Debug, Clone)]
pub struct Services {
    pub fs: FsService,
    pub registry: LicenseRegistry,
}

impl Services {
    /// Build the production service container backed by the real filesystem.
    ///
    /// Phase 2 will extend this to also load the license registry.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the license registry fails to load (toml parse or read failure).
    pub fn real() -> Result<Self, Report<ServicesError>> {
        Ok(Self {
            fs: FsService::new(Arc::new(RealFs::new())),
            registry: LicenseRegistry::load(
                &FsService::new(Arc::new(RealFs::new())),
                Path::new("."),
            )
            .change_context(ServicesError)?,
        })
    }

    /// Build a service container from explicit parts. Used by tests and by
    /// callers that construct pieces independently (e.g. command runners).
    #[must_use]
    pub fn from_parts(fs: FsService, registry: LicenseRegistry) -> Self {
        Self { fs, registry }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn services_real_constructs() {
        let _services = Services::real();
    }

    #[test]
    fn fs_service_round_trip_via_fake_backend() {
        let services = Services {
            fs: FsService::new(Arc::new(FakeFs::default())),
            registry: LicenseRegistry::embedded_only(),
        };
        let path = Path::new("/tmp/fake.txt");
        services.fs.write(path, "hello").unwrap();
        assert!(services.fs.exists(path));
        assert_eq!(services.fs.read_to_string(path).unwrap(), "hello");
    }
}
