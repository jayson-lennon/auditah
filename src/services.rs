//! Service layer: dependency-injection container + backend abstractions.

pub mod clock;
pub mod config;
pub mod fs;

pub use self::config::ConfigService;
pub use crate::config::ConfigError;
pub use crate::registry::{LicenseRegistry, LicenseRegistryService};
pub use clock::{ClockBackend, ClockError, ClockService, RealClock};
pub use fs::{DirEntry, FsBackend, FsError, FsService, RealFs};

/// Dependency-injection container. Constructed once in `main` (real backends)
/// or in tests (fakes). Cheap to clone; every field is a service wrapper or
/// otherwise cheap-to-clone. Pass it by reference to anything that needs a
/// service — do not split individual fields out into function signatures.
#[derive(Debug, Clone)]
pub struct Services {
    pub fs: FsService,
    pub registry: LicenseRegistryService,
    pub clock: ClockService,
    pub config: ConfigService,
}

impl Services {
    /// Start a test-only [`ServicesTestBuilder`]. Defaults: empty `FakeFs`,
    /// `FakeClock::fixed(0)`, empty registry, default config at `/proj`.
    ///
    /// Tests override only what they care about (e.g. `.registry_specs(...)`)
    /// and call `.build()`. Production assembly lives only in `main`.
    #[cfg(feature = "test-helper")]
    #[must_use]
    pub fn test() -> crate::test_support::ServicesTestBuilder {
        crate::test_support::ServicesTestBuilder::default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::test_support::FakeFs;
    use std::path::Path;
    use std::sync::Arc;

    #[test]
    fn test_builder_defaults_to_fake_backends_and_empty_registry() {
        // Given a freshly-defaulted ServicesTestBuilder.
        // When building.
        let services = Services::test().build();

        // Then the registry is empty and the config root is the default.
        assert!(services.registry.is_empty());
        assert_eq!(services.config.root(), Path::new("/proj"));
    }

    #[test]
    fn services_container_exposes_all_four_fields_and_is_clone() {
        // Given a Services built via the test builder.
        let services = Services::test().build();

        // When cloning.
        let cloned = services.clone();

        // Then all four service fields are present and the clone shares the
        // same root (Arc-backed ConfigService) — confirms the container is
        // cheap to clone and exposes every subsystem.
        assert_eq!(services.config.root(), cloned.config.root());
        assert!(services.registry.is_empty());
        assert_eq!(services.clock.now_epoch_secs().unwrap(), 0);
    }

    #[test]
    fn services_test_builder_seeds_config() {
        // Given a non-default config.
        let cfg = crate::config::Config {
            commercial_project: true,
            ..Default::default()
        };

        // When building with that config rooted at /custom.
        let services = Services::test()
            .config_root(Path::new("/custom"), cfg)
            .build();

        // Then the config and root are the seeded values.
        assert_eq!(services.config.root(), Path::new("/custom"));
        assert!(services.config.config().commercial_project);
    }

    #[test]
    fn fs_service_round_trips_write_read_exists_via_fake_backend() {
        // Given a Services with a FakeFs backend.
        let services = Services::test()
            .fs(FsService::new(Arc::new(FakeFs::default())))
            .build();
        let path = Path::new("/tmp/fake.txt");

        // When writing then reading via the FsService.
        services.fs.write(path, "hello").unwrap();

        // Then exists/read reflect the written content.
        assert!(services.fs.exists(path));
        assert_eq!(services.fs.read_to_string(path).unwrap(), "hello");
    }
}
