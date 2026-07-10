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
    pub fn real(root: &Path) -> Result<Self, Report<ServicesError>> {
        Ok(Self {
            fs: FsService::new(Arc::new(RealFs::new())),
            registry: LicenseRegistry::load(&FsService::new(Arc::new(RealFs::new())), root)
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

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn services_real_constructs_without_panic() {
        // Given the real Services constructor.
        // When constructing real services.
        let _services = Services::real(Path::new(".")).expect("real services");

        // Then construction succeeds (no panic).
    }

    #[test]
    fn fs_service_round_trips_write_read_exists_via_fake_backend() {
        // Given a Services with a FakeFs backend.
        let services = Services {
            fs: FsService::new(Arc::new(FakeFs::default())),
            registry: LicenseRegistry::empty(),
        };
        let path = Path::new("/tmp/fake.txt");

        // When writing then reading via the FsService.
        services.fs.write(path, "hello").unwrap();

        // Then exists/read reflect the written content.
        assert!(services.fs.exists(path));
        assert_eq!(services.fs.read_to_string(path).unwrap(), "hello");
    }
}
