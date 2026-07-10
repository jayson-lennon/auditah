//! `auditah add` / `auditah init-pack` — scaffold attribution files.
//!
//! Core writers take an [`AttributionRecord`] and produce a sidecar
//! (`<asset>.attr.toml`) or a directory manifest (`manifest.toml`) via
//! `toml_edit`, so the emitted files carry human-readable field comments.

use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use toml_edit::{table, value, DocumentMut};
use wherror::Error;

use crate::model::attribution::AttributionRecord;
use crate::model::terms::Overrides;
use crate::services::Services;

/// Error writing a scaffolded attribution file.
#[derive(Debug, Error)]
#[error(debug)]
pub struct AddError;

/// Render an [`AttributionRecord`] as a TOML document with field comments.
/// Shared by sidecar and manifest writers so both formats are identical and
/// round-trip through the resolver/auditor.
#[must_use]
pub fn render_record(record: &AttributionRecord) -> String {
    let mut doc = DocumentMut::new();
    doc.decor_mut()
        .set_prefix("# Attribution record — see auditah docs.\n");

    doc["title"] = value(&record.title);
    doc["author"] = value(&record.author);
    doc["year"] = value(i64::from(record.year));
    doc["license"] = value(&record.license);
    doc["source"] = value(&record.source);
    doc["modified"] = value(record.modified);
    if let Some(pkg) = &record.package {
        doc["package"] = value(pkg);
    }
    if has_any_override(&record.overrides) {
        doc["overrides"] = override_table(&record.overrides);
    }
    doc.to_string()
}

/// Write a sidecar `<asset_path>.attr.toml` next to the asset.
///
/// # Errors
///
/// Returns `AddError` if writing fails.
pub fn write_sidecar(
    services: &Services,
    asset_path: &Path,
    record: &AttributionRecord,
) -> Result<(), Report<AddError>> {
    let sidecar = sidecar_path(asset_path);
    let content = render_record(record);
    services
        .fs
        .write(&sidecar, &content)
        .change_context(AddError)
        .attach("failed to write attribution sidecar")
        .attach(sidecar.display().to_string())
}

/// Write a directory `manifest.toml` covering `dir` and its subtree.
///
/// # Errors
///
/// Returns `AddError` if writing fails.
pub fn write_manifest(
    services: &Services,
    dir: &Path,
    record: &AttributionRecord,
) -> Result<(), Report<AddError>> {
    let manifest = dir.join("manifest.toml");
    let content = render_record(record);
    services
        .fs
        .write(&manifest, &content)
        .change_context(AddError)
        .attach("failed to write directory manifest")
        .attach(manifest.display().to_string())
}

/// Compute the sidecar path for an asset: `<asset>.attr.toml`.
#[must_use]
pub fn sidecar_path(asset_path: &Path) -> PathBuf {
    // Append `.attr.toml` to the full file name (preserves any extension).
    let mut name = asset_path
        .file_name()
        .map(std::ffi::OsStr::to_os_string)
        .unwrap_or_default();
    name.push(".attr.toml");
    asset_path.with_file_name(name)
}

/// Whether any override field is set (controls whether `[overrides]` is emitted).
fn has_any_override(o: &Overrides) -> bool {
    o.requires_attribution.is_some()
        || o.requires_license_notice.is_some()
        || o.requires_source_disclosure.is_some()
        || o.requires_share_alike.is_some()
        || o.requires_modification_notice.is_some()
        || o.allows_commercial_use.is_some()
        || o.allows_modifications.is_some()
}

/// Build the `[overrides]` sub-table, including only set fields.
fn override_table(o: &Overrides) -> toml_edit::Item {
    let mut t = table();
    if let Some(v) = o.requires_attribution {
        t["requires_attribution"] = value(v);
    }
    if let Some(v) = o.requires_license_notice {
        t["requires_license_notice"] = value(v);
    }
    if let Some(v) = o.requires_source_disclosure {
        t["requires_source_disclosure"] = value(v);
    }
    if let Some(v) = o.requires_share_alike {
        t["requires_share_alike"] = value(v);
    }
    if let Some(v) = o.requires_modification_notice {
        t["requires_modification_notice"] = value(v);
    }
    if let Some(v) = o.allows_commercial_use {
        t["allows_commercial_use"] = value(v);
    }
    if let Some(v) = o.allows_modifications {
        t["allows_modifications"] = value(v);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::terms::Overrides;

    fn sample_record() -> AttributionRecord {
        AttributionRecord {
            title: "Gunny Sack".to_string(),
            author: "Oliver Herklotz".to_string(),
            year: 2019,
            license: "CC-BY-3.0".to_string(),
            source: "https://poly.pizza".to_string(),
            modified: false,
            package: None,
            overrides: Overrides::default(),
        }
    }

    #[test]
    fn rendered_record_round_trips_through_toml_parse() {
        // Given a minimal attribution record.
        let record = sample_record();

        // When rendering to TOML and parsing back.
        let toml = render_record(&record);
        let parsed: AttributionRecord = toml::from_str(&toml).expect("must round-trip");

        // Then the parsed record equals the original.
        assert_eq!(parsed, record);
    }

    #[test]
    fn rendered_record_with_overrides_round_trips() {
        // Given a record with override fields set.
        let mut record = sample_record();
        record.overrides = Overrides {
            allows_commercial_use: Some(false),
            requires_attribution: Some(true),
            ..Default::default()
        };

        // When rendering to TOML and parsing back.
        let toml = render_record(&record);
        let parsed: AttributionRecord = toml::from_str(&toml).expect("must round-trip");

        // Then the parsed record equals the original including overrides.
        assert_eq!(parsed, record);
        assert_eq!(parsed.overrides.allows_commercial_use, Some(false));
    }

    #[test]
    fn rendered_record_with_package_round_trips() {
        // Given a record with a package field.
        let mut record = sample_record();
        record.package = Some("Nature Pack".to_string());

        // When rendering to TOML and parsing back.
        let toml = render_record(&record);
        let parsed: AttributionRecord = toml::from_str(&toml).expect("must round-trip");

        // Then the package field round-trips.
        assert_eq!(parsed.package.as_deref(), Some("Nature Pack"));
    }

    #[test]
    fn sidecar_path_appends_attr_toml() {
        // Given an asset path.
        // When computing its sidecar path.
        let p = sidecar_path(Path::new("/proj/sword.glb"));

        // Then the sidecar path is the asset path with .attr.toml appended.
        assert_eq!(p, PathBuf::from("/proj/sword.glb.attr.toml"));
    }
}
