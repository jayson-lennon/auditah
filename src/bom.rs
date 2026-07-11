//! `auditah bom` — generate a license bill of materials.
//!
//! Walks the same discovery/resolution pipeline as `audit` and `credits`, but
//! produces a compliance-oriented artifact (BOM.md) rather than a human-credits
//! artifact (CREDITS.md). The BOM answers "what obligations does this
//! distribution carry?" and surfaces uncheckable obligations (source
//! disclosure, license notice, share-alike) as action items.
//!
//! Unlike `credits`, the BOM collects **all** licenses in use (including CC0 /
//! MIT / other permissive licenses), because even permissive licenses carry
//! obligations (e.g. reproducing the license notice). The BOM summarizes per
//! license, not per asset.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::config::Config;
use crate::discovery::enumerator::{enumerate, ExcludeMatcher};
use crate::discovery::resolver::resolve;
use crate::model::terms::LicenseTerms;
use crate::services::Services;

/// Error generating the BOM.
#[derive(Debug, Error)]
#[error(debug)]
pub struct BomError;

/// Subsystem context for BOM generation.
#[derive(Debug, Clone)]
pub struct BomCtx<'a> {
    pub services: &'a Services,
    pub config: &'a Config,
    pub root: &'a Path,
}

/// Per-license aggregate: metadata, base terms, and the list of assets under it.
#[derive(Debug, Clone)]
pub(crate) struct LicenseSummary {
    id: String,
    name: String,
    url: String,
    terms: LicenseTerms,
    assets: Vec<PathBuf>,
}

/// Collect per-license summaries, grouped by license id (sorted for stable output).
///
/// Unlike `credits`, collects **all** resolved assets regardless of attribution
/// requirement. Uses the license's **base terms** (from the registry), not
/// per-asset effective terms — the BOM describes the license, not each asset's
/// overrides.
///
/// # Errors
///
/// Returns `BomError` if enumeration or resolution fails.
pub(crate) fn collect_bom(ctx: &BomCtx) -> Result<Vec<LicenseSummary>, Report<BomError>> {
    let excludes = build_excludes(ctx)?;
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)
        .change_context(BomError)
        .attach("failed to enumerate assets for BOM")?;

    let mut by_license: BTreeMap<String, LicenseSummary> = BTreeMap::new();
    for asset in &assets {
        let Some(record) = resolve(&ctx.services.fs, asset, ctx.root)
            .change_context(BomError)
            .attach("failed to resolve asset during BOM generation")?
            .record
        else {
            continue;
        };
        let Some(entry) = ctx.services.registry.get(&record.license) else {
            continue;
        };
        let summary = by_license
            .entry(entry.id.clone())
            .or_insert_with(|| LicenseSummary {
                id: entry.id.clone(),
                name: entry.name.clone(),
                url: entry.url.clone(),
                terms: entry.terms.clone(),
                assets: Vec::new(),
            });
        summary.assets.push(asset.clone());
    }

    for summary in by_license.values_mut() {
        summary.assets.sort();
    }
    Ok(by_license.into_values().collect())
}

/// Build the exclude matcher from default + user globs.
///
/// Defense in depth: [`Config::load`] validates globs eagerly, but a
/// `Config` constructed directly (e.g. in tests) bypasses that, so this stays
/// fallible and propagates the error rather than panicking.
///
/// # Errors
///
/// Returns `BomError` if any exclude glob fails to compile.
fn build_excludes(ctx: &BomCtx) -> Result<ExcludeMatcher, Report<BomError>> {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .change_context(BomError)
        .attach("invalid exclude glob in auditah.toml")
}

/// Derive human-readable action items from the collected license summaries.
///
/// Surfaces the uncheckable obligations (source disclosure, license notice,
/// share-alike) as explicit TODOs, and warns if multiple distinct share-alike
/// licenses are in use (a potential compatibility conflict).
#[must_use]
pub(crate) fn derive_action_items(summaries: &[LicenseSummary]) -> Vec<String> {
    let mut items = Vec::new();

    // Share-alike conflict check first (warning).
    let sa_ids: Vec<&str> = summaries
        .iter()
        .filter(|s| s.terms.derivatives == crate::model::terms::Derivatives::ShareAlike)
        .map(|s| s.id.as_str())
        .collect();
    if sa_ids.len() > 1 {
        items.push(format!(
            "⚠ Multiple share-alike licenses in use ({}) — verify derivative works can satisfy both.",
            sa_ids.join(", ")
        ));
    }

    // Per-license action items (sorted by id, which collect_bom already ensures).
    for s in summaries {
        if s.terms.requires_source_disclosure {
            items.push(format!(
                "Offer corresponding source for {} {} asset(s): {}",
                s.assets.len(),
                s.id,
                format_asset_paths(&s.assets)
            ));
        }
        if s.terms.requires_license_notice {
            items.push(format!(
                "Reproduce license text for {} in your distribution — see NOTICES.md",
                s.id
            ));
        }
        if s.terms.derivatives == crate::model::terms::Derivatives::ShareAlike {
            items.push(format!(
                "Share-alike: modified {} assets must ship under {}",
                s.id, s.id
            ));
        }
    }

    items
}

/// Format asset paths as a comma-separated list of display paths (relative to root where possible).
fn format_asset_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render the collected license summaries as a BOM.md string.
#[must_use]
pub(crate) fn render_bom(summaries: &[LicenseSummary]) -> String {
    let mut out = String::from("# License Bill of Materials\n\n");

    if summaries.is_empty() {
        out.push_str("_No licensed assets found._\n");
        return out;
    }

    // Per-license summary section.
    out.push_str("## Licenses in use\n\n");
    for s in summaries {
        let _ = writeln!(
            out,
            "### {} ({}) — {} asset(s)",
            s.name,
            s.id,
            s.assets.len()
        );
        out.push('\n');
        out.push_str(&render_terms_bullets(&s.terms));
        let _ = writeln!(out, "- Source: {}", s.url);
        out.push('\n');
    }

    // Action items section.
    let action_items = derive_action_items(summaries);
    out.push_str("## Action items\n\n");
    if action_items.is_empty() {
        out.push_str("_No outstanding compliance actions._\n");
    } else {
        for (i, item) in action_items.iter().enumerate() {
            let _ = writeln!(out, "{}. {item}", i + 1);
        }
    }

    out
}

/// Render a license's obligation/permission flags as Markdown bullets.
fn render_terms_bullets(terms: &LicenseTerms) -> String {
    use crate::model::terms::Derivatives;
    let mut out = String::new();

    // Permissions (allows_*)
    if terms.allows_commercial_use {
        out.push_str("- Commercial use: permitted\n");
    } else {
        out.push_str("- Commercial use: **not permitted**\n");
    }
    if terms.allows_redistribution {
        out.push_str("- Redistribution: permitted\n");
    } else {
        out.push_str("- Redistribution: **not permitted**\n");
    }

    // Derivatives dimension.
    match terms.derivatives {
        Derivatives::Disallowed => out.push_str("- Derivatives: **disallowed**\n"),
        Derivatives::Allowed => out.push_str("- Derivatives: allowed\n"),
        Derivatives::ShareAlike => {
            out.push_str(
                "- Derivatives: **share-alike** (modified assets must ship under same license)\n",
            );
        }
    }

    // Obligations (requires_*)
    if terms.requires_attribution {
        out.push_str("- Attribution: required\n");
    }
    if terms.requires_license_notice {
        out.push_str("- License notice: **MUST reproduce**\n");
    }
    if terms.requires_source_disclosure {
        out.push_str("- Source disclosure: **MUST offer corresponding source**\n");
    }
    if terms.requires_modification_notice {
        out.push_str("- Modification notice: required (state modifications in credits)\n");
    }
    if terms.manual_review {
        out.push_str("- Manual review: required\n");
    }

    out
}

/// Full pipeline: collect, render, and write BOM.md to `output_path`.
///
/// Runs an audit pass first; if any FAIL findings exist, no BOM is generated.
/// The BOM is a compliance overview — it must not be produced for a project that
/// doesn't pass audit, or it would silently omit unresolved/unlicensed assets
/// (lying by omission).
///
/// # Errors
///
/// Returns `BomError` if BOM collection/render/write fails. The audit
/// gate is the caller's responsibility (`generate` runs it once before any
/// artifact generation).
pub fn generate_bom(ctx: &BomCtx, output_path: &Path) -> Result<(), Report<BomError>> {
    let summaries = collect_bom(ctx)?;
    let markdown = render_bom(&summaries);
    ctx.services
        .fs
        .write(output_path, &markdown)
        .change_context(BomError)
        .attach("failed to write BOM.md")
        .attach(output_path.display().to_string())
}

/// Default output path: `<root>/BOM.md`.
#[must_use]
pub fn default_output_path(root: &Path) -> PathBuf {
    root.join("BOM.md")
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::terms::Derivatives;

    fn summary(id: &str, terms: LicenseTerms, n_assets: usize) -> LicenseSummary {
        LicenseSummary {
            id: id.to_string(),
            name: format!("{id} Name"),
            url: format!("https://example.com/{id}"),
            terms,
            assets: (0..n_assets)
                .map(|i| PathBuf::from(format!("/proj/asset{i}.glb")))
                .collect(),
        }
    }

    #[test]
    fn derive_action_items_all_permissive_is_empty() {
        // Given summaries for two permissive licenses with no obligations.
        let summaries = vec![
            summary("CC0-1.0", LicenseTerms::permissive(), 3),
            summary("MIT", LicenseTerms::permissive(), 2),
        ];

        // When deriving action items.
        let items = derive_action_items(&summaries);

        // Then there are no action items.
        assert!(items.is_empty(), "expected no action items, got {items:?}");
    }

    #[test]
    fn derive_action_items_source_disclosure_lists_paths() {
        // Given a summary with source disclosure set and 2 assets.
        let mut terms = LicenseTerms::permissive();
        terms.requires_source_disclosure = true;
        let summaries = vec![summary("GPL-3.0-only", terms, 2)];

        // When deriving action items.
        let items = derive_action_items(&summaries);

        // Then there is one item mentioning both asset paths.
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("Offer corresponding source"));
        assert!(items[0].contains("GPL-3.0-only"));
        assert!(items[0].contains("asset0.glb"));
        assert!(items[0].contains("asset1.glb"));
    }

    #[test]
    fn derive_action_items_license_notice_references_notices() {
        // Given a summary with license notice set.
        let mut terms = LicenseTerms::permissive();
        terms.requires_license_notice = true;
        let summaries = vec![summary("CC-BY-4.0", terms, 1)];

        // When deriving action items.
        let items = derive_action_items(&summaries);

        // Then there is one item referencing NOTICES.md.
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("Reproduce license text"));
        assert!(items[0].contains("NOTICES.md"));
        assert!(items[0].contains("CC-BY-4.0"));
    }

    #[test]
    fn derive_action_items_single_share_alike_no_conflict() {
        // Given a single share-alike license.
        let terms = LicenseTerms::permissive().with_derivatives(Derivatives::ShareAlike);
        let summaries = vec![summary("CC-BY-SA-4.0", terms, 1)];

        // When deriving action items.
        let items = derive_action_items(&summaries);

        // Then there is one share-alike item and no conflict warning.
        assert_eq!(items.len(), 1);
        assert!(items[0].contains("Share-alike"));
        assert!(!items[0].contains("Multiple share-alike"));
    }

    #[test]
    fn derive_action_items_multiple_share_alike_warns_conflict() {
        // Given two distinct share-alike licenses.
        let sa_terms = || LicenseTerms::permissive().with_derivatives(Derivatives::ShareAlike);
        let summaries = vec![
            summary("CC-BY-SA-4.0", sa_terms(), 1),
            summary("GPL-3.0-only", sa_terms(), 1),
        ];

        // When deriving action items.
        let items = derive_action_items(&summaries);

        // Then the first item is the conflict warning naming both ids.
        assert!(items.iter().any(|i| i.contains("Multiple share-alike")));
        assert!(items
            .iter()
            .any(|i| i.contains("CC-BY-SA-4.0") && i.contains("GPL-3.0-only")));
    }

    #[test]
    fn render_bom_empty_shows_no_licensed_assets() {
        // Given no summaries.
        let out = render_bom(&[]);

        // Then the output says no licensed assets found.
        assert!(out.contains("# License Bill of Materials"));
        assert!(out.contains("_No licensed assets found._"));
        assert!(!out.contains("## Licenses in use"));
    }

    #[test]
    fn render_bom_includes_license_summary_and_action_items_sections() {
        // Given a permissive summary and a source-disclosure summary.
        let mut gpl_terms = LicenseTerms::permissive();
        gpl_terms.requires_source_disclosure = true;
        let summaries = vec![
            summary("MIT", LicenseTerms::permissive(), 2),
            summary("GPL-3.0-only", gpl_terms, 1),
        ];

        // When rendering.
        let out = render_bom(&summaries);

        // Then both sections are present with correct content.
        assert!(out.contains("## Licenses in use"));
        assert!(out.contains("### MIT Name (MIT) — 2 asset(s)"));
        assert!(out.contains("### GPL-3.0-only Name (GPL-3.0-only) — 1 asset(s)"));
        assert!(out.contains("## Action items"));
        assert!(out.contains("Offer corresponding source for 1 GPL-3.0-only asset"));
    }

    #[test]
    fn render_bom_all_permissive_shows_no_outstanding_actions() {
        // Given only permissive summaries.
        let summaries = vec![summary("CC0-1.0", LicenseTerms::permissive(), 1)];

        // When rendering.
        let out = render_bom(&summaries);

        // Then the action items section says no outstanding actions.
        assert!(out.contains("## Action items"));
        assert!(out.contains("_No outstanding compliance actions._"));
    }

    #[test]
    fn render_terms_bullets_maximally_restrictive_shows_all_flags() {
        // Given terms with EVERY obligation/permission flag set to test all render branches.
        let terms = LicenseTerms {
            requires_attribution: true,
            requires_license_notice: true,
            requires_source_disclosure: true,
            derivatives: Derivatives::Disallowed,
            requires_modification_notice: true,
            allows_commercial_use: false,
            allows_redistribution: false,
            manual_review: true,
        };
        let summaries = vec![summary("Custom-Restrictive", terms, 1)];

        // When rendering.
        let out = render_bom(&summaries);

        // Then every restrictive flag appears in the output.
        assert!(out.contains("Commercial use: **not permitted**"));
        assert!(out.contains("Redistribution: **not permitted**"));
        assert!(out.contains("Derivatives: **disallowed**"));
        assert!(out.contains("Attribution: required"));
        assert!(out.contains("License notice: **MUST reproduce**"));
        assert!(out.contains("Source disclosure: **MUST offer corresponding source**"));
        assert!(out.contains("Modification notice: required"));
        assert!(out.contains("Manual review: required"));
    }
}
