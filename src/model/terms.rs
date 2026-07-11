//! License terms: obligations and permissions of a license.
//!
//! Declared per-license in the registry; overridable per-asset via an
//! `[overrides]` block on an [`AttributionRecord`](super::attribution::AttributionRecord).

use serde::{Deserialize, Serialize};

/// Whether derivatives of the licensed work are permitted, and if so under
/// what relicensing constraint.
///
/// This is a single dimension, not two coordinated bools: a license either
/// forbids derivatives (`Disallowed`), permits them freely (`Allowed`), or
/// requires them to be relicensed under the same terms (`ShareAlike`). Folding
/// share-alike into this enum makes the contradictory state
/// "no-derivatives + share-alike" literally unconstructable.
///
/// Serialized to TOML as kebab-case strings: `disallowed`, `allowed`,
/// `share-alike`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Derivatives {
    /// You MAY NOT create derivatives (CC-BY-ND).
    Disallowed,
    /// You MAY create derivatives under any terms (MIT, CC-BY, CC0).
    Allowed,
    /// You MAY create derivatives only under the same license (OFL, GPL, CC-BY-SA).
    ShareAlike,
}

/// Obligations and permissions of a license.
///
/// `requires_*` fields are **obligations** — the auditor verifies they're
/// fulfilled. `allows_*` fields are **permissions** — the auditor verifies the
/// project stays inside the boundary. `derivatives` is a single dimension that
/// replaces the former separate `allows_modifications` + `requires_share_alike`
/// pair (see [`Derivatives`]). `manual_review` is a license-only escape hatch
/// that fails the audit until the license id is acknowledged in the project
/// config — it is intentionally not overridable per-asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
// `struct_excessive_bools` is a false positive here: a license is inherently a set
// of independent binary obligation/permission flags. Each bool carries distinct
// domain meaning with no interdependency (the only interdependent pair was
// collapsed into `derivatives`). The type system cannot meaningfully improve on
// `bool` for a single yes/no obligation, and sub-struct grouping just relocates
// the lint (one group still has 4 obligations).
#[allow(clippy::struct_excessive_bools)]
pub struct LicenseTerms {
    /// You MUST attribute the author (requires `title` + `author` + `source`).
    pub requires_attribution: bool,
    /// You MUST reproduce the license text in your distribution.
    pub requires_license_notice: bool,
    /// You MUST offer corresponding source code on distribution. Tracked in the BOM; no audit finding.
    pub requires_source_disclosure: bool,
    /// Derivatives dimension: `Disallowed` | `Allowed` | `ShareAlike`.
    pub derivatives: Derivatives,
    /// If `modified = true`, you MUST state the modification in credits.
    pub requires_modification_notice: bool,
    /// You MAY use this commercially.
    pub allows_commercial_use: bool,
    /// You MAY redistribute (re-host / resell) the asset itself, not just ship it embedded.
    pub allows_redistribution: bool,
    /// The license carries clauses the boolean grid cannot auto-verify (seat limits,
    /// territory, field-of-use, ...). FAILs the audit until the license id is listed in
    /// `manual_review_acknowledged` in `auditah.toml`. License-only: not in [`Overrides`].
    pub manual_review: bool,
}

impl LicenseTerms {
    /// Permissive baseline: the "use however you want" shape.
    ///
    /// Used as the default for the `add-license` template and as the starting
    /// point for test fixtures. All permissions granted, no obligations,
    /// derivatives allowed, no manual review.
    #[must_use]
    pub fn permissive() -> Self {
        Self {
            requires_attribution: false,
            requires_license_notice: false,
            requires_source_disclosure: false,
            derivatives: Derivatives::Allowed,
            requires_modification_notice: false,
            allows_commercial_use: true,
            allows_redistribution: true,
            manual_review: false,
        }
    }

    /// Builder-style override of the `derivatives` field.
    #[must_use]
    pub fn with_derivatives(mut self, d: Derivatives) -> Self {
        self.derivatives = d;
        self
    }

    /// Maximal fail-closed baseline: the "nothing granted until you engage" shape.
    ///
    /// Used as the default for both `add-license --custom` (a custom license whose
    /// grid the user must fill in) and the placeholder grid written when a
    /// well-known license has no authored grid yet. Every permission is false,
    /// every obligation except `manual_review` is false, derivatives are
    /// disallowed, and `manual_review = true` FAILs the audit until the user
    /// both fills in the real terms and acknowledges the id. This guarantees
    /// no scaffolded license can pass audit by accident.
    #[must_use]
    pub fn default_fail() -> Self {
        Self {
            requires_attribution: false,
            requires_license_notice: false,
            requires_source_disclosure: false,
            derivatives: Derivatives::Disallowed,
            requires_modification_notice: false,
            allows_commercial_use: false,
            allows_redistribution: false,
            manual_review: true,
        }
    }
}

/// Per-asset term overrides. All fields optional — only set fields replace
/// the corresponding registry term; unset fields inherit from the license.
/// This is *merge* semantics (spec algorithm: "then apply asset overrides"),
/// not wholesale replacement.
///
/// Note: `manual_review` is deliberately absent — it is a license-only property
/// and must not be loosened per-asset, or the fail-closed guarantee would be defeated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_attribution: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_license_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_source_disclosure: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derivatives: Option<Derivatives>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_modification_notice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_commercial_use: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allows_redistribution: Option<bool>,
}

/// Compute effective terms by merging [`Overrides`] on top of base [`LicenseTerms`].
///
/// Each override field that is `Some` replaces the base; `None` inherits.
/// An empty `Overrides` (all `None`) returns a copy of the base unchanged.
///
/// Infallible by construction: `derivatives` is a single enum dimension, so an
/// override can only ever produce another *valid* variant — there is no
/// contradictory state to validate against.
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
        derivatives: overrides.derivatives.unwrap_or(base.derivatives),
        requires_modification_notice: overrides
            .requires_modification_notice
            .unwrap_or(base.requires_modification_notice),
        allows_commercial_use: overrides
            .allows_commercial_use
            .unwrap_or(base.allows_commercial_use),
        allows_redistribution: overrides
            .allows_redistribution
            .unwrap_or(base.allows_redistribution),
        manual_review: base.manual_review,
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn cc_by_terms() -> LicenseTerms {
        LicenseTerms {
            requires_attribution: true,
            requires_license_notice: false,
            requires_source_disclosure: false,
            derivatives: Derivatives::Allowed,
            requires_modification_notice: false,
            allows_commercial_use: true,
            allows_redistribution: true,
            manual_review: false,
        }
    }

    #[test]
    fn empty_overrides_inherits_base_unchanged() {
        // Given a CC-BY base and empty overrides.
        let base = cc_by_terms();

        // When applying the empty overrides.
        let effective = effective_terms(&base, &Overrides::default());

        // Then the effective terms equal the base.
        assert_eq!(effective, base);
    }

    #[test]
    fn partial_override_flips_only_commercial_use() {
        // Given a CC-BY base and an override flipping only commercial use.
        let base = cc_by_terms();
        let overrides = Overrides {
            allows_commercial_use: Some(false),
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then only commercial use is flipped to false.
        assert!(!effective.allows_commercial_use);
    }

    #[test]
    fn partial_override_preserves_attribution_requirement() {
        // Given a CC-BY base and an override flipping only commercial use.
        let base = cc_by_terms();
        let overrides = Overrides {
            allows_commercial_use: Some(false),
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then the attribution requirement is inherited from the base.
        assert!(effective.requires_attribution);
    }

    #[test]
    fn partial_override_preserves_derivatives_dimension() {
        // Given a CC-BY base (Allowed derivatives) and an override flipping only commercial use.
        let base = cc_by_terms();
        let overrides = Overrides {
            allows_commercial_use: Some(false),
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then the derivatives dimension is inherited from the base.
        assert_eq!(effective.derivatives, Derivatives::Allowed);
    }

    #[test]
    fn override_to_disallowed_replaces_derivatives_variant() {
        // Given an Allowed-derivatives base.
        let base = cc_by_terms();
        let overrides = Overrides {
            derivatives: Some(Derivatives::Disallowed),
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then the derivatives dimension is the overridden variant.
        assert_eq!(effective.derivatives, Derivatives::Disallowed);
        // And other fields inherit from the base.
        assert!(effective.requires_attribution);
    }

    #[test]
    fn override_to_share_alike_produces_valid_variant() {
        // Given an Allowed-derivatives base.
        let base = cc_by_terms();
        let overrides = Overrides {
            derivatives: Some(Derivatives::ShareAlike),
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then the effective derivatives is ShareAlike — a single coherent variant,
        // never a contradictory "disallowed + share-alike".
        assert_eq!(effective.derivatives, Derivatives::ShareAlike);
    }

    #[test]
    fn manual_review_is_never_overridable() {
        // Given a base with manual_review = true and an override trying to flip it
        // (there is no such field, so an empty override is the most an attacker can express).
        let mut base = cc_by_terms();
        base.manual_review = true;
        let overrides = Overrides {
            ..Default::default()
        };

        // When applying the override.
        let effective = effective_terms(&base, &overrides);

        // Then manual_review is inherited from the base, never cleared.
        assert!(effective.manual_review);
    }

    #[test]
    fn stale_allows_modifications_key_is_rejected() {
        // Given a terms TOML carrying the removed `allows_modifications` key.
        let toml = r#"
            requires_attribution = false
            requires_license_notice = false
            requires_source_disclosure = false
            derivatives = "allowed"
            requires_modification_notice = false
            allows_commercial_use = true
            allows_redistribution = true
            manual_review = false
            allows_modifications = true
        "#;

        // When parsing.
        let result: Result<LicenseTerms, _> = toml::from_str(toml);

        // Then parsing fails — stale keys are rejected, not silently ignored.
        assert!(result.is_err());
    }

    #[test]
    fn stale_requires_share_alike_key_is_rejected() {
        // Given a terms TOML carrying the removed `requires_share_alike` key.
        let toml = r#"
            requires_attribution = false
            requires_license_notice = false
            requires_source_disclosure = false
            derivatives = "share-alike"
            requires_modification_notice = false
            allows_commercial_use = true
            allows_redistribution = true
            manual_review = false
            requires_share_alike = true
        "#;

        // When parsing.
        let result: Result<LicenseTerms, _> = toml::from_str(toml);

        // Then parsing fails — the enum replaces the old bool.
        assert!(result.is_err());
    }

    #[test]
    fn default_fail_is_maximally_restrictive_and_manual_review() {
        // Given nothing.
        // When constructing the default_fail terms.
        let terms = LicenseTerms::default_fail();

        // Then every permission/obligation is false except manual_review, and derivatives are disallowed.
        assert!(!terms.requires_attribution);
        assert!(!terms.requires_license_notice);
        assert!(!terms.requires_source_disclosure);
        assert_eq!(terms.derivatives, Derivatives::Disallowed);
        assert!(!terms.requires_modification_notice);
        assert!(!terms.allows_commercial_use);
        assert!(!terms.allows_redistribution);
        assert!(terms.manual_review);
    }
}
