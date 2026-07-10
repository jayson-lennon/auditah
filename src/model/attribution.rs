//! Asset attribution record: the per-asset metadata that travels with an asset
//! via a sidecar (`<name>.attr.toml`) or a directory manifest (`manifest.toml`).

use serde::{Deserialize, Serialize};

use crate::model::terms::Overrides;

/// Attribution metadata for a single asset (or, in a manifest, for a whole dir).
///
/// Required fields are present-but-possibly-empty; the audit command checks that
/// obligation-bearing licenses have non-empty `title` + `author` + `source`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttributionRecord {
    /// Human-readable title of the work (CC-BY attribution requires this).
    pub title: String,
    /// Author / copyright holder.
    pub author: String,
    /// Copyright year.
    pub year: u16,
    /// SPDX license ID (e.g. `CC-BY-3.0`, `CC0-1.0`, `MIT`).
    pub license: String,
    /// Source URL where the asset was obtained.
    pub source: String,
    /// Whether the asset has been modified from the original.
    #[serde(default)]
    pub modified: bool,
    /// Pack name, when the asset came from a bundle. Omitted/empty otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// Per-asset term overrides. Absent block = inherit license defaults.
    #[serde(default)]
    pub overrides: Overrides,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_record_round_trips_without_overrides_or_package() {
        let toml = r#"
title = "Gunny Sack"
author = "Oliver Herklotz"
year = 2019
license = "CC-BY-3.0"
source = "https://poly.pizza/m/download/Gunny-Sack"
"#;
        let record: AttributionRecord = toml::from_str(toml).unwrap();
        assert_eq!(record.title, "Gunny Sack");
        assert_eq!(record.year, 2019);
        assert!(!record.modified);
        assert!(record.package.is_none());
        // Empty overrides block (all None) deserializes from absence.
        assert_eq!(record.overrides, Overrides::default());
    }

    #[test]
    fn record_with_override_block_parses() {
        let toml = r#"
title = "Pack Item"
author = "Author"
year = 2020
license = "CC-BY-4.0"
source = "https://example.com"
modified = true

[overrides]
allows_commercial_use = false
"#;
        let record: AttributionRecord = toml::from_str(toml).unwrap();
        assert!(record.modified);
        assert_eq!(record.overrides.allows_commercial_use, Some(false));
        assert_eq!(record.overrides.requires_attribution, None);
    }
}
