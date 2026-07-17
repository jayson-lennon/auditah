//! Wall-clock service: abstraction over reading the current time so that
//! core logic is testable without a real system clock.
//!
//! Mirrors the `FsBackend` / `FsService` capability pattern: a `ClockBackend`
//! trait, a production `RealClock`, and a shared `ClockService` wrapper. Tests
//! substitute a fake backend (see `test_support::FakeClock`).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use derive_more::Debug;
use error_stack::{Report, ResultExt};
use wherror::Error;

/// Error type for clock reads. Colocated with [`ClockBackend`] per the
/// service-trait pattern.
#[derive(Debug, Error)]
#[error(debug)]
pub struct ClockError;

/// Capability trait: read the current wall-clock time as Unix epoch seconds.
///
/// Production uses [`RealClock`]; tests use a fake backend.
pub trait ClockBackend: Send + Sync {
    /// Return the current time as seconds since the Unix epoch.
    ///
    /// # Errors
    ///
    /// Returns [`ClockError`] if the wall clock cannot be read or is before
    /// the Unix epoch (e.g. a broken/misconfigured system clock).
    fn now_epoch_secs(&self) -> Result<u64, Report<ClockError>>;

    /// Backend name for debugging.
    fn name(&self) -> &'static str;
}

/// Shared, cloneable wrapper around a [`ClockBackend`] trait object.
#[derive(Debug, Clone)]
pub struct ClockService {
    #[debug("ClockService<{}>", self.backend.name())]
    backend: Arc<dyn ClockBackend>,
}

impl ClockService {
    /// Wrap a backend. The service is cheap to clone (one [`Arc`] refcount).
    #[must_use]
    pub fn new(backend: Arc<dyn ClockBackend>) -> Self {
        Self { backend }
    }

    /// Current time as seconds since the Unix epoch. See
    /// [`ClockBackend::now_epoch_secs`].
    ///
    /// # Errors
    ///
    /// Propagates [`ClockError`] from the backend.
    pub fn now_epoch_secs(&self) -> Result<u64, Report<ClockError>> {
        self.backend
            .now_epoch_secs()
            .attach("failed to read system clock")
    }
}

/// Production [`ClockBackend`] backed by the real system clock. Construct via
/// [`RealClock::new`] and wrap in [`ClockService`].
#[derive(Debug, Default, Clone, Copy)]
pub struct RealClock;

impl RealClock {
    /// Create a new real-clock backend.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ClockBackend for RealClock {
    fn now_epoch_secs(&self) -> Result<u64, Report<ClockError>> {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .change_context(ClockError)
            .attach("system clock is before the Unix epoch or unreadable")
    }

    fn name(&self) -> &'static str {
        "RealClock"
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_clock_now_epoch_secs_is_in_the_current_era() {
        // Given the real production clock.
        let clock = RealClock::new();

        // When reading the current epoch seconds.
        let secs = clock.now_epoch_secs().expect("real clock readable");

        // Then the seconds place the current year well past 2020 (guard
        // against the epoch-zero / pre-epoch regressions).
        assert!(
            secs > 1_577_836_800,
            "real clock should read after 2020-01-01; got {secs}"
        );
    }
}
