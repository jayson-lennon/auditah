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
    use crate::model::license::LicenseRegistryEntry;
    use rstest::rstest;

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

    #[rstest]
    #[case::mit("MIT")]
    #[case::isc("ISC")]
    #[case::bsd2("BSD-2-Clause")]
    #[case::bsd3("BSD-3-Clause")]
    #[case::obsd("0BSD")]
    #[case::apache("Apache-2.0")]
    #[case::cc0("CC0-1.0")]
    #[case::cc_by("CC-BY-4.0")]
    #[case::cc_by_sa("CC-BY-SA-4.0")]
    #[case::cc_by_nd("CC-BY-ND-4.0")]
    #[case::ofl("OFL-1.1")]
    #[case::gpl("GPL-3.0-only")]
    #[case::lgpl("LGPL-3.0-only")]
    #[case::mpl("MPL-2.0")]
    fn authored_grid_round_trips_through_toml(#[case] id: &str) {
        // Given the embedded corpus contains an authored grid for this id.
        let toml_str = read_entry(&format!("{id}.toml"))
            .unwrap_or_else(|| panic!("{id}.toml must be authored"));

        // When parsing it as a registry entry.
        let entry: LicenseRegistryEntry =
            toml::from_str(&toml_str).unwrap_or_else(|e| panic!("{id}.toml failed to parse: {e}"));

        // Then the entry's id matches and url is non-empty.
        assert_eq!(entry.id, id);
        assert!(!entry.url.is_empty());
    }
}
