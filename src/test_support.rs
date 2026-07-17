//! Shared test infrastructure for auditah.
//!
//! `FakeFs` is the single in-memory `FsBackend` used by every unit and
//! integration test. It replaces the seven divergent, behaviour-conflicting
//! copies that previously lived in each module's `#[cfg(test)]` block.
//!
//! Semantics mirror `RealFs` faithfully so tests exercise real discovery:
//!   - `list_dir(p)` → immediate children of `p` (keys whose parent is `p`).
//!   - `walk(root)`  → every file recursively under `root` (keys starting with `root`).
//!
//! IO-error injection: register a path (or walk-root) on the relevant
//! `fail_*` set; the matching operation returns `Err(FsError)` before any
//! lookup. This lets error-scenario tests trigger technical failures without
//! a real filesystem.
//!
//! Gated behind the `test-helper` Cargo feature and `#[doc(hidden)]` so it
//! ships in no default build and never appears in public docs.

#![cfg(feature = "test-helper")]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use error_stack::Report;
use parking_lot::Mutex;

use crate::config::Config;
use crate::registry::{LicenseRegistry, LicenseRegistryService, LicenseSpec};
use crate::services::clock::{ClockBackend, ClockError, ClockService};
use crate::services::config::ConfigService;
use crate::services::fs::{DirEntry, FsBackend, FsError, FsService};
use crate::services::Services;

/// Internal mutable state behind a single lock.
#[derive(Debug, Default)]
struct FsState {
    files: HashMap<PathBuf, String>,
    /// Explicitly created directories (`create_dir_all`). Tracked separately
    /// from `files` so an empty dir is observable as `exists` without
    /// polluting `walk`/`list_dir` (which iterate `files` and treat every
    /// key as a regular file).
    dirs: HashSet<PathBuf>,
    fail_reads: HashSet<PathBuf>,
    fail_writes: HashSet<PathBuf>,
    fail_walks: HashSet<PathBuf>,
    fail_list_dirs: HashSet<PathBuf>,
    /// Per-path count of `list_dir`/`list_dir_typed` calls. Lets tests
    /// assert IO-contract invariants like "each directory listed once".
    list_dir_calls: HashMap<PathBuf, u32>,
    /// In-flight `list_dir`/`list_dir_typed` calls (for concurrency probes).
    list_dir_in_flight: usize,
    /// High-water mark of concurrent `list_dir` calls observed so far.
    list_dir_high_water: usize,
    /// When non-zero, `list_dir` sleeps this many milliseconds (outside the
    /// lock) so concurrent calls overlap — used to assert `--jobs` caps.
    list_dir_delay_ms: u64,
}

impl FsState {
    fn empty() -> Self {
        Self {
            files: HashMap::new(),
            dirs: HashSet::new(),
            fail_reads: HashSet::new(),
            fail_writes: HashSet::new(),
            fail_walks: HashSet::new(),
            fail_list_dirs: HashSet::new(),
            list_dir_calls: HashMap::new(),
            list_dir_in_flight: 0,
            list_dir_high_water: 0,
            list_dir_delay_ms: 0,
        }
    }
}

/// In-memory `FsBackend` for tests. Construct via [`FakeFs::with_files`] then
/// chain builder methods (`insert`, `fail_read`, `fail_write`, `fail_walk`,
/// `fail_list_dir`); wrap in `FsService::new(Arc::new(...))` when done.
#[doc(hidden)]
#[derive(Debug, Default)]
pub struct FakeFs {
    state: Mutex<FsState>,
}

impl FakeFs {
    /// Create a fake pre-populated with `files`. Each entry is `(path, content)`.
    #[must_use]
    pub fn with_files<I, P, S>(files: I) -> Self
    where
        I: IntoIterator<Item = (P, S)>,
        P: Into<PathBuf>,
        S: Into<String>,
    {
        let mut state = FsState::empty();
        for (path, content) in files {
            state.files.insert(path.into(), content.into());
        }
        Self {
            state: Mutex::new(state),
        }
    }

    /// Insert a file at `path` with `content`. Consuming builder.
    #[must_use]
    pub fn insert<P, S>(self, path: P, content: S) -> Self
    where
        P: Into<PathBuf>,
        S: Into<String>,
    {
        self.state.lock().files.insert(path.into(), content.into());
        self
    }

    /// Force `read_to_string(path)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_read<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_reads.insert(path.into());
        self
    }

    /// Force `write(path, _)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_write<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_writes.insert(path.into());
        self
    }

    /// Force `walk(root)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_walk<P>(self, root: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_walks.insert(root.into());
        self
    }

    /// Force `list_dir(path)` to return `Err(FsError)`.
    #[must_use]
    pub fn fail_list_dir<P>(self, path: P) -> Self
    where
        P: Into<PathBuf>,
    {
        self.state.lock().fail_list_dirs.insert(path.into());
        self
    }

    /// How many times `list_dir`/`list_dir_typed` has been called on `path`.
    /// Lets tests assert IO-contract invariants like "each directory listed once".
    #[must_use]
    pub fn list_dir_call_count(&self, path: &Path) -> u32 {
        self.state
            .lock()
            .list_dir_calls
            .get(path)
            .copied()
            .unwrap_or(0)
    }

    /// Inject a sleep into every `list_dir`/`list_dir_typed` call (outside the
    /// lock) so concurrent calls overlap. Used with
    /// [`list_dir_high_water`](Self::list_dir_high_water) to assert `--jobs`
    /// caps concurrent directory descents.
    #[must_use]
    pub fn with_list_dir_delay_ms(self, ms: u64) -> Self {
        self.state.lock().list_dir_delay_ms = ms;
        self
    }

    /// Maximum number of `list_dir`/`list_dir_typed` calls ever in flight at
    /// once. Pair with [`with_list_dir_delay_ms`](Self::with_list_dir_delay_ms)
    /// to observe the concurrency cap.
    #[must_use]
    pub fn list_dir_high_water(&self) -> usize {
        self.state.lock().list_dir_high_water
    }

    /// Bump the in-flight counter + high-water; returns the configured delay.
    fn enter_list_dir(&self) -> u64 {
        let mut state = self.state.lock();
        state.list_dir_in_flight += 1;
        state.list_dir_high_water = state.list_dir_high_water.max(state.list_dir_in_flight);
        state.list_dir_delay_ms
    }

    /// Decrement the in-flight counter.
    fn exit_list_dir(&self) {
        self.state.lock().list_dir_in_flight -= 1;
    }
}

impl FsBackend for FakeFs {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>> {
        let state = self.state.lock();
        if state.fail_reads.contains(path) {
            return Err(Report::new(FsError));
        }
        state
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| Report::new(FsError))
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>> {
        let mut state = self.state.lock();
        if state.fail_writes.contains(path) {
            return Err(Report::new(FsError));
        }
        state.files.insert(path.to_path_buf(), content.to_string());
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), Report<FsError>> {
        let mut state = self.state.lock();
        state.dirs.insert(path.to_path_buf());
        Ok(())
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        let delay = self.enter_list_dir();
        // Delay OUTSIDE the lock so concurrent calls overlap.
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        let result = {
            let mut state = self.state.lock();
            *state.list_dir_calls.entry(path.to_path_buf()).or_insert(0) += 1;
            if state.fail_list_dirs.contains(path) {
                Err(Report::new(FsError))
            } else {
                Ok(state
                    .files
                    .keys()
                    .filter(|k| k.parent() == Some(path))
                    .cloned()
                    .collect())
            }
        };
        self.exit_list_dir();
        result
    }

    fn list_dir_typed(&self, path: &Path) -> Result<Vec<DirEntry>, Report<FsError>> {
        let delay = self.enter_list_dir();
        // Delay OUTSIDE the lock so concurrent calls overlap.
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        let result = {
            let mut state = self.state.lock();
            *state.list_dir_calls.entry(path.to_path_buf()).or_insert(0) += 1;
            if state.fail_list_dirs.contains(path) {
                Err(Report::new(FsError))
            } else {
                let mut entries: Vec<DirEntry> = Vec::new();
                let mut seen_subdirs: HashSet<PathBuf> = HashSet::new();
                for k in state.files.keys() {
                    let Some(rel) = k.strip_prefix(path).ok() else {
                        continue;
                    };
                    let mut comps = rel.components();
                    let Some(first) = comps.next() else { continue };
                    // If `first` is the only component, it's a file directly under `path`.
                    if comps.next().is_none() {
                        entries.push(DirEntry::file(k.clone()));
                    } else {
                        // Otherwise `first` names an immediate subdir of `path`.
                        let subdir = path.join(first);
                        if seen_subdirs.insert(subdir.clone()) {
                            entries.push(DirEntry::dir(subdir));
                        }
                    }
                }
                Ok(entries)
            }
        };
        self.exit_list_dir();
        result
    }

    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>> {
        let state = self.state.lock();
        if state.fail_walks.contains(root) {
            return Err(Report::new(FsError));
        }
        Ok(state
            .files
            .keys()
            .filter(|k| k.starts_with(root))
            .cloned()
            .collect())
    }

    fn exists(&self, path: &Path) -> bool {
        let state = self.state.lock();
        // A path exists if it's a seeded file, an explicitly created dir,
        // or if any seeded file lives beneath it (implicit directory).
        state.files.contains_key(path)
            || state.dirs.contains(path)
            || state.files.keys().any(|k| k.starts_with(path))
    }

    fn copy_file(&self, src: &Path, dst: &Path) -> Result<(), Report<FsError>> {
        let mut state = self.state.lock();
        if state.fail_writes.contains(dst) {
            return Err(Report::new(FsError));
        }
        // FakeFs stores content as String, so it cannot model true binary
        // bytes; cloning the stored entry is the faithful test contract.
        let content = state
            .files
            .get(src)
            .cloned()
            .ok_or_else(|| Report::new(FsError))?;
        state.files.insert(dst.to_path_buf(), content);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "FakeFs"
    }
}

/// In-memory `ClockBackend` for tests. Construct via [`FakeClock::fixed`] for a
/// normal epoch-second instant, or [`FakeClock::broken`] to model a
/// pre-epoch / unreadable clock that yields [`ClockError`].
#[doc(hidden)]
#[derive(Debug, Clone)]
pub struct FakeClock {
    state: ClockState,
}

#[derive(Debug, Clone)]
enum ClockState {
    /// Returns a fixed epoch-second value.
    Fixed(u64),
    /// Always errors (pre-epoch / unreadable clock).
    Broken,
}

impl FakeClock {
    /// A clock pinned to `epoch_secs`.
    #[must_use]
    pub fn fixed(epoch_secs: u64) -> Self {
        Self {
            state: ClockState::Fixed(epoch_secs),
        }
    }

    /// A clock that always fails to read (models a pre-epoch clock).
    #[must_use]
    pub fn broken() -> Self {
        Self {
            state: ClockState::Broken,
        }
    }
}

impl ClockBackend for FakeClock {
    fn now_epoch_secs(&self) -> Result<u64, Report<ClockError>> {
        match self.state {
            ClockState::Fixed(secs) => Ok(secs),
            ClockState::Broken => Err(Report::new(ClockError)),
        }
    }

    fn name(&self) -> &'static str {
        "FakeClock"
    }
}

/// Fluent builder for a test [`Services`] container.
///
/// Defaults: a [`FakeFs`], [`FakeClock::fixed`] at the Unix epoch, an
/// empty [`LicenseRegistry`], and a default [`Config`] rooted at `/proj`.
/// Override any slot via the builder methods, then call [`build`](Self::build).
///
/// This is the canonical way to spin up a test `Services`; the production
/// assembly lives only in `main`.
#[doc(hidden)]
#[derive(Debug, Clone, Default)]
pub struct ServicesTestBuilder {
    fs: Option<FsService>,
    registry: Option<LicenseRegistryService>,
    clock: Option<ClockService>,
    config: Option<ConfigService>,
}

impl ServicesTestBuilder {
    /// Load real backends from disk: `RealFs`, `LicenseRegistry` from
    /// `LICENSES/*.toml`, `Config` from `auditah.toml` (default if absent),
    /// and `RealClock`. Replaces the old `Services::real_services_test`.
    ///
    /// Use this in integration tests that exercise real disk I/O. Everything
    /// is assembled inside `build()`, so no `Services { ... }` literal leaks
    /// outside `main` and this builder.
    ///
    /// # Errors
    /// Returns `AppError` if loading `LICENSES/*.toml` fails.
    pub fn load_from_disk(root: &Path) -> Result<Self, Report<crate::AppError>> {
        use crate::registry::LicenseRegistryService;
        use crate::services::clock::RealClock;
        use crate::services::fs::RealFs;
        use error_stack::ResultExt;
        let fs = FsService::new(Arc::new(RealFs::new()));
        let registry = LicenseRegistryService::load(&fs, root)
            .change_context(crate::AppError)
            .attach("failed to load license registry")?;
        let clock = ClockService::new(Arc::new(RealClock::new()));
        let config = ConfigService::load(&fs, root)
            .change_context(crate::AppError)
            .attach("failed to load config")?;
        Ok(Self {
            fs: Some(fs),
            registry: Some(registry),
            clock: Some(clock),
            config: Some(config),
        })
    }

    /// Use this `FsService` (otherwise a default empty `FakeFs`).
    #[must_use]
    pub fn fs(mut self, fs: FsService) -> Self {
        self.fs = Some(fs);
        self
    }

    /// Use this `ClockService` (otherwise `FakeClock::fixed(0)`).
    #[must_use]
    pub fn clock(mut self, clock: ClockService) -> Self {
        self.clock = Some(clock);
        self
    }

    /// Use this `LicenseRegistryService` (otherwise an empty registry).
    #[must_use]
    pub fn registry(mut self, registry: LicenseRegistryService) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Seed the registry from `specs` (otherwise an empty registry).
    #[must_use]
    pub fn registry_specs<I, S>(self, specs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<LicenseSpec>,
    {
        let mut builder = LicenseRegistry::builder();
        for spec in specs {
            builder = builder.license(spec.into());
        }
        self.registry(LicenseRegistryService::new(Arc::new(builder.build())))
    }

    /// Use this `ConfigService` (otherwise default config at `/proj`).
    #[must_use]
    pub fn config(mut self, config: ConfigService) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the config to `config` rooted at `root` (convenience over
    /// `.config(ConfigService::new(...))`).
    #[must_use]
    pub fn config_root(self, root: impl AsRef<Path>, config: Config) -> Self {
        self.config(ConfigService::new(
            Arc::from(root.as_ref()),
            Arc::new(config),
        ))
    }

    /// Assemble the [`Services`] container, filling any unset slot with its
    /// default.
    #[must_use]
    pub fn build(self) -> Services {
        let fs = self
            .fs
            .unwrap_or_else(|| FsService::new(Arc::new(FakeFs::default())));
        let registry = self
            .registry
            .unwrap_or_else(|| LicenseRegistryService::new(Arc::new(LicenseRegistry::empty())));
        let clock = self
            .clock
            .unwrap_or_else(|| ClockService::new(Arc::new(FakeClock::fixed(0))));
        let config = self.config.unwrap_or_else(|| {
            ConfigService::new(Arc::from(Path::new("/proj")), Arc::new(Config::default()))
        });
        Services {
            fs,
            registry,
            clock,
            config,
        }
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_dir_returns_immediate_children_only() {
        // Given a fake seeded with a nested file tree.
        let fs = FakeFs::with_files([
            (Path::new("/proj/a.txt"), "1"),
            (Path::new("/proj/b.txt"), "2"),
            (Path::new("/proj/sub/c.txt"), "3"),
            (Path::new("/proj/sub/deep/d.txt"), "4"),
        ]);

        // When listing the directory.
        let children = fs.list_dir(Path::new("/proj")).unwrap();
        let names: Vec<String> = children
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();

        // Then only immediate children are returned (nested files excluded).
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"b.txt".to_string()));
        assert!(
            !children
                .iter()
                .any(|p| p.to_string_lossy().contains("c.txt")),
            "nested files must not appear in parent list_dir"
        );
    }

    #[test]
    fn walk_returns_all_files_recursively() {
        // Given a fake seeded with a nested file tree.
        let fs = FakeFs::with_files([
            (Path::new("/proj/a.txt"), "1"),
            (Path::new("/proj/sub/b.txt"), "2"),
            (Path::new("/proj/sub/deep/c.txt"), "3"),
            (Path::new("/other/x.txt"), "4"),
        ]);

        // When walking the root.
        let files = fs.walk(Path::new("/proj")).unwrap();

        // Then every file recursively under the root is returned (others excluded).
        assert_eq!(files.len(), 3, "only files under /proj");
        assert!(!files
            .iter()
            .any(|p| p.to_string_lossy().contains("/other/")));
    }

    #[test]
    fn fail_read_injects_error() {
        // Given a fake with a registered read failure.
        let fs = FakeFs::with_files([(Path::new("/x"), "data")]).fail_read(Path::new("/x"));

        // When reading that path.
        let result = fs.read_to_string(Path::new("/x"));

        // Then the operation errors.
        assert!(result.is_err());
    }

    #[test]
    fn fail_write_injects_error_and_does_not_store() {
        // Given a fake with a registered write failure.
        let fs = FakeFs::default().fail_write(Path::new("/out"));

        // When writing that path.
        let result = fs.write(Path::new("/out"), "x");

        // Then the operation errors and no file is stored.
        assert!(result.is_err());
        assert!(!fs.exists(Path::new("/out")));
    }

    #[test]
    fn fail_walk_injects_error() {
        // Given a fake with a registered walk failure on a root.
        let fs = FakeFs::default().fail_walk(Path::new("/proj"));

        // When walking that root.
        let result = fs.walk(Path::new("/proj"));

        // Then the operation errors.
        assert!(result.is_err());
    }

    #[test]
    fn fail_list_dir_injects_error() {
        // Given a fake with a registered list_dir failure.
        let fs = FakeFs::default().fail_list_dir(Path::new("/proj"));

        // When listing that directory.
        let result = fs.list_dir(Path::new("/proj"));

        // Then the operation errors.
        assert!(result.is_err());
    }
    #[test]
    fn exists_returns_true_for_implicit_directory() {
        // Given a fake with a file nested under a directory path.
        let fs = FakeFs::with_files([(Path::new("/proj/sub/a.txt"), "x")]);

        // When checking whether the parent directory exists.
        let parent_exists = fs.exists(Path::new("/proj"));

        // Then the parent exists.
        assert!(parent_exists);

        // When checking whether the nested directory exists.
        let nested_exists = fs.exists(Path::new("/proj/sub"));

        // Then the nested directory exists.
        assert!(nested_exists);
    }

    #[test]
    fn test_builder_seeds_registry_from_specs() {
        // Given specs for two licenses.
        // When building with them.
        let services = Services::test()
            .registry_specs([
                LicenseSpec::new("LicenseRef-A"),
                LicenseSpec::new("LicenseRef-B"),
            ])
            .build();

        // Then both resolve and len matches.
        assert!(services.registry.get("LicenseRef-A").is_some());
        assert!(services.registry.get("LicenseRef-B").is_some());
        assert_eq!(services.registry.len(), 2);
    }

    #[test]
    fn test_builder_defaults_to_fake_fs_and_fake_clock() {
        // Given a defaulted builder.
        // When building.
        let services = Services::test().build();

        // Then fs is a FakeFs (write/read round-trip works).
        let p = Path::new("/tmp/x");
        services.fs.write(p, "hi").expect("write");
        assert_eq!(services.fs.read_to_string(p).unwrap(), "hi");
        // And clock is FakeClock::fixed(0) (now_epoch_secs == 0).
        assert_eq!(services.clock.now_epoch_secs().unwrap(), 0);
    }

    // --- copy_file ---

    #[test]
    fn copy_file_clones_stored_entry_to_destination() {
        // Given a fake filesystem with a seeded source file.
        let fs = FakeFs::with_files([("/lib/sword.glb", "asset-bytes")]);
        let dst = Path::new("/game/sword.glb");

        // When copying the source to a new destination.
        fs.copy_file(Path::new("/lib/sword.glb"), dst)
            .expect("copy");

        // Then the destination holds an independent copy of the source content.
        assert_eq!(fs.read_to_string(dst).expect("read dst"), "asset-bytes");
    }

    #[test]
    fn copy_file_surfaces_error_when_destination_write_is_faulted() {
        // Given a fake filesystem faulting writes to the destination.
        let fs =
            FakeFs::with_files([("/lib/sword.glb", "asset-bytes")]).fail_write("/game/sword.glb");

        // When copying into the faulted destination.
        let result = fs.copy_file(Path::new("/lib/sword.glb"), Path::new("/game/sword.glb"));

        // Then the copy surfaces an FsError and nothing is written.
        assert!(result.is_err());
        assert!(!fs.exists(Path::new("/game/sword.glb")));
    }
}
