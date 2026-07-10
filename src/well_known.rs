//! Well-known SPDX license text + grid corpus, embedded as a zip.
//!
//! The corpus lives in `well_known_licenses/` (source-controlled, raw) and is
//! packaged into `spdx-licenses.zip` by `build.rs`. Each entry is a bare
//! filename — `MIT.txt` (license text) or `MIT.toml` (authored obligation grid)
//! — with no directory prefix, so `ZipArchive::by_name("MIT.txt")` is the
//! access path.
//!
//! This module owns:
//! - [`SPDX_ZIP`]: the embedded bytes (`include_bytes!`).
//! - [`archive`]: a freshly-opened `ZipArchive` over those bytes.
//!
//! Matching / extraction helpers (`resolve`, `grid_for`, `extract_text`, …)
//! are added in later phases.

use std::io::{Cursor, Read};

use zip::ZipArchive;

/// The full SPDX text + grid corpus, packaged by `build.rs`.
///
/// Measured ~1.9MB compressed (814 text files + authored grids).
pub(crate) const SPDX_ZIP: &[u8] = include_bytes!("../spdx-licenses.zip");

/// Open a fresh `ZipArchive` over the embedded corpus.
///
/// `ZipArchive` mutably borrows its reader for `by_name`, so it can't be held
/// as a long-lived shared handle. The bytes are `const` (already `'static`);
/// re-opening per logical operation is cheap (in-memory, no I/O).
#[allow(dead_code, clippy::expect_used)] // consumed by Phase 4; embedded blob is build-time-validated, panic = broken binary
pub(crate) fn archive() -> ZipArchive<Cursor<&'static [u8]>> {
    ZipArchive::new(Cursor::new(SPDX_ZIP)).expect("embedded spdx-licenses.zip must be valid")
}

/// Read a single entry from the embedded corpus by name, e.g. `"MIT.txt"`.
///
/// Returns `None` if no such entry exists.
#[allow(dead_code)] // consumed by Phase 4 resolve/extract helpers
pub(crate) fn read_entry(name: &str) -> Option<String> {
    let mut zip = archive();
    let mut entry = zip.by_name(name).ok()?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf).ok()?;
    Some(buf)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_zip_has_expected_entry_count() {
        // Given the embedded corpus.
        let zip = archive();

        // When counting entries.
        let count = zip.len();

        // Then it matches the vendored corpus (~814 text files; grids added
        // incrementally, so >= 814).
        assert!(count >= 814, "expected >= 814 entries, got {count}");
    }

    #[test]
    fn mit_text_is_extractable_by_name() {
        // Given the embedded corpus.
        // When extracting MIT.txt by name.
        let text = read_entry("MIT.txt");

        // Then it is present and non-empty.
        let text = text.expect("MIT.txt must be in the corpus");
        assert!(
            text.contains("MIT License"),
            "MIT.txt body unexpected: {text}"
        );
    }

    #[test]
    fn missing_entry_returns_none() {
        // Given the embedded corpus.
        // When extracting a nonexistent entry.
        let text = read_entry("DefinitelyNotARealLicense.txt");

        // Then it is absent.
        assert!(text.is_none());
    }
}
