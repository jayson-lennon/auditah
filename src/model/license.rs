//! License registry entry: one record per known license, bundled or project-local.

use serde::{Deserialize, Serialize};

use crate::model::terms::LicenseTerms;

/// One license in the registry. Bundled licenses (CC0, CC-BY-3.0, MIT, OFL) are
/// embedded in the binary via `include_str!`; project-local `licenses/*.toml`
/// files (including any `LicenseRef-*`) merge in at load and can override
/// bundled entries by `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LicenseRegistryEntry {
    /// SPDX license ID (`CC-BY-3.0`, `MIT`, `CC0-1.0`) or `LicenseRef-*` for custom.
    pub id: String,
    /// Human-readable license name.
    pub name: String,
    /// Canonical URL of the license.
    pub url: String,
    /// Full license text. Embedded licenses carry this at compile time.
    #[serde(default)]
    pub text: String,
    /// Obligations and permissions of this license.
    pub terms: LicenseTerms,
    /// Free-form notes, especially for bespoke/custom licenses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc0_entry() -> LicenseRegistryEntry {
        LicenseRegistryEntry {
            id: "CC0-1.0".to_string(),
            name: "Creative Commons Zero".to_string(),
            url: "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
            text: "(full text elided)".to_string(),
            terms: LicenseTerms {
                requires_attribution: false,
                requires_license_notice: false,
                requires_source_disclosure: false,
                requires_share_alike: false,
                requires_modification_notice: false,
                allows_commercial_use: true,
                allows_modifications: true,
            },
            notes: None,
        }
    }

    #[test]
    fn entry_round_trips_through_toml() {
        let entry = cc0_entry();
        let toml_str = toml::to_string(&entry).unwrap();
        let parsed: LicenseRegistryEntry = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn notes_omitted_when_none() {
        let toml_str = toml::to_string(&cc0_entry()).unwrap();
        // skip_serializing_if should drop the notes key entirely.
        assert!(!toml_str.contains("notes"));
    }
}
