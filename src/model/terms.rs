//! License terms: obligations and permissions of a license.
//!
//! Declared per-license in the registry; overridable per-asset via an
//! `[overrides]` block on an [`AttributionRecord`](super::attribution::AttributionRecord).

use serde::{Deserialize, Serialize};

/// Obligations and permissions of a license.
///
/// `requires_*` fields are **obligations** — the auditor verifies they're
/// fulfilled. `allows_*` fields are **permissions** — the auditor verifies the
/// project stays inside the boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Term flags are independent by design; enums would complicate TOML + merge.
pub struct LicenseTerms {
    /// You MUST attribute the author (requires `title` + `author` + `source`).
    pub requires_attribution: bool,
    /// You MUST reproduce the license text in your distribution.
    pub requires_license_notice: bool,
    /// You MUST offer corresponding source code on distribution. Auto-unverifiable → FLAG.
    pub requires_source_disclosure: bool,
    /// You MUST license derivatives under the same terms. Auto-unverifiable → FLAG.
    pub requires_share_alike: bool,
    /// If `modified = true`, you MUST state the modification in credits.
    pub requires_modification_notice: bool,
    /// You MAY use this commercially.
    pub allows_commercial_use: bool,
    /// You MAY create derivatives.
    pub allows_modifications: bool,
}

/// Per-asset term overrides. All fields optional — only set fields replace
/// the corresponding registry term; unset fields inherit from the license.
/// This is *merge* semantics (spec algorithm: "then apply asset overrides"),
/// not wholesale replacement.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_attribution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_license_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_source_disclosure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_share_alike: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_modification_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_commercial_use: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_modifications: Option<bool>,
}

/// Compute effective terms by merging [`Overrides`] on top of base [`LicenseTerms`].
///
/// Each override field that is `Some` replaces the base; `None` inherits.
/// An empty `Overrides` (all `None`) returns a copy of the base unchanged.
#[must_use]
pub fn effective_terms(base: &LicenseTerms, overrides: &Overrides) -> LicenseTerms {
    LicenseTerms {
        requires_attribution: overrides
            .requires_attribution
            .unwrap_or(base.requires_attribution),
        requires_license_notice: overrides
            .requires_license_notice
            .unwrap_or(base.requires_license_notice),
        requires_source_disclosure: overrides
            .requires_source_disclosure
            .unwrap_or(base.requires_source_disclosure),
        requires_share_alike: overrides
            .requires_share_alike
            .unwrap_or(base.requires_share_alike),
        requires_modification_notice: overrides
            .requires_modification_notice
            .unwrap_or(base.requires_modification_notice),
        allows_commercial_use: overrides
            .allows_commercial_use
            .unwrap_or(base.allows_commercial_use),
        allows_modifications: overrides
            .allows_modifications
            .unwrap_or(base.allows_modifications),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cc_by_terms() -> LicenseTerms {
        LicenseTerms {
            requires_attribution: true,
            requires_license_notice: false,
            requires_source_disclosure: false,
            requires_share_alike: false,
            requires_modification_notice: false,
            allows_commercial_use: true,
            allows_modifications: true,
        }
    }

    #[test]
    fn empty_overrides_inherits_base() {
        let base = cc_by_terms();
        let effective = effective_terms(&base, &Overrides::default());
        assert_eq!(effective, base);
    }

    #[test]
    fn partial_override_flips_one_field() {
        let base = cc_by_terms();
        let overrides = Overrides {
            allows_commercial_use: Some(false),
            ..Default::default()
        };
        let effective = effective_terms(&base, &overrides);
        assert!(!effective.allows_commercial_use);
        assert!(effective.requires_attribution);
        assert!(effective.allows_modifications);
    }
}
