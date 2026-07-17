//! `auditah notices` — generate a NOTICES.md reproducing license text.
//!
//! Walks the same discovery/resolution pipeline as `audit` and `bom`, but
//! produces a distribution artifact (NOTICES.md) reproducing the full legal
//! text of every distinct license whose terms require a license notice
//! (`requires_license_notice = true`). Licenses that waive the notice
//! requirement (e.g. CC0) are omitted.
//!
//! NOTICES.md is the "ship these license texts with your product" artifact.
//! CREDITS.md answers attribution; NOTICES.md answers the legal-text
//! reproduction obligation.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::config::Config;
use crate::discovery::enumerator::{enumerate, ExcludeMatcher};
use crate::discovery::resolver::resolve;
use crate::services::Services;

/// Error generating NOTICES.
#[derive(Debug, Error)]
#[error(debug)]
pub struct NoticesError;

/// Subsystem context for NOTICES generation.
#[derive(Debug, Clone)]
pub struct NoticesCtx<'a> {
    pub services: &'a Services,
    pub config: &'a Config,
    pub root: &'a Path,
}

/// Per-license text entry: the license's canonical name + its full legal text.
#[derive(Debug, Clone)]
pub(crate) struct LicenseText {
    id: String,
    name: String,
    text: String,
}

/// Collect distinct license texts for licenses requiring a notice.
///
/// Walks all assets, resolves each, and collects the legal text for every
/// distinct license id whose terms have `requires_license_notice = true`.
/// Deduplicates by license id (50 MIT assets → one MIT entry). Reads the text
/// from `<root>/LICENSES/<id>.txt`.
///
/// Uses the license's **base terms** (from the registry), not per-asset
/// effective terms — the notice obligation is a property of the license, not
/// of each asset's overrides.
///
/// # Errors
///
/// Returns `NoticesError` if enumeration, resolution, or text read fails.
pub(crate) fn collect_notices(ctx: &NoticesCtx) -> Result<Vec<LicenseText>, Report<NoticesError>> {
    let excludes = build_excludes(ctx)?;
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)
        .change_context(NoticesError)
        .attach("failed to enumerate assets for NOTICES")?;

    let mut by_license: BTreeMap<String, LicenseText> = BTreeMap::new();
    for asset in &assets {
        let Some(record) = resolve(&ctx.services.fs, asset, ctx.root)
            .change_context(NoticesError)
            .attach("failed to resolve asset during NOTICES generation")?
            .record
        else {
            continue;
        };
        let Some(entry) = ctx.services.registry.get(&record.license) else {
            continue;
        };
        if !entry.terms.requires_license_notice {
            continue;
        }
        if by_license.contains_key(&entry.id) {
            continue;
        }
        let text_path = ctx.root.join("LICENSES").join(format!("{}.txt", entry.id));
        let text = ctx
            .services
            .fs
            .read_to_string(&text_path)
            .change_context(NoticesError)
            .attach("failed to read license text for NOTICES")
            .attach(text_path.display().to_string())?;
        by_license.insert(
            entry.id.clone(),
            LicenseText {
                id: entry.id.clone(),
                name: entry.name.clone(),
                text,
            },
        );
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
/// Returns `NoticesError` if any exclude glob fails to compile.
fn build_excludes(ctx: &NoticesCtx) -> Result<ExcludeMatcher, Report<NoticesError>> {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .change_context(NoticesError)
        .attach("invalid exclude glob in auditah.toml")
}

/// Render the collected license texts as a NOTICES.md string.
#[must_use]
pub(crate) fn render_notices(licenses: &[LicenseText]) -> String {
    let mut out = String::from("# License Notices\n\n");

    if licenses.is_empty() {
        out.push_str("_No license-notice-required assets found._\n");
        return out;
    }

    for lic in licenses {
        let _ = writeln!(out, "## {}: {}\n", lic.id, lic.name);
        out.push_str("```text\n");
        out.push_str(&lic.text);
        if !lic.text.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```\n\n");
    }

    out
}

/// Full pipeline: collect, render, and write NOTICES.md to `output_path`.
///
/// The audit gate is the caller's responsibility (`generate` runs it once
/// before any artifact generation).
///
/// # Errors
///
/// Returns `NoticesError` on enumeration/resolution failure or write failure.
pub fn generate_notices(ctx: &NoticesCtx, output_path: &Path) -> Result<(), Report<NoticesError>> {
    let licenses = collect_notices(ctx)?;
    let markdown = render_notices(&licenses);
    ctx.services
        .fs
        .write(output_path, &markdown)
        .change_context(NoticesError)
        .attach("failed to write NOTICES.md")
        .attach(output_path.display().to_string())
}

/// Default output path: `<root>/NOTICES.md`.
#[must_use]
pub fn default_output_path(root: &Path) -> PathBuf {
    root.join("NOTICES.md")
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn license(id: &str, name: &str, text: &str) -> LicenseText {
        LicenseText {
            id: id.to_string(),
            name: name.to_string(),
            text: text.to_string(),
        }
    }

    #[test]
    fn render_notices_empty_shows_placeholder() {
        // Given no license texts.
        let out = render_notices(&[]);

        // Then the output has the placeholder.
        assert!(out.contains("# License Notices"));
        assert!(out.contains("_No license-notice-required assets found._"));
        assert!(!out.contains("## "));
    }

    #[test]
    fn render_notices_single_license_includes_text_under_header() {
        // Given one license text.
        let licenses = vec![license("MIT", "MIT License", "MIT license body text")];

        // When rendering.
        let out = render_notices(&licenses);

        // Then the header and text appear.
        assert!(out.contains("## MIT: MIT License"));
        assert!(out.contains("```text"));
        assert!(out.contains("MIT license body text"));
        assert!(out.contains("```"));
    }

    #[test]
    fn render_notices_multiple_licenses_each_get_section() {
        // Given two license texts.
        let licenses = vec![
            license(
                "CC-BY-4.0",
                "Creative Commons Attribution 4.0",
                "CC-BY body",
            ),
            license("MIT", "MIT License", "MIT body"),
        ];

        // When rendering.
        let out = render_notices(&licenses);

        // Then both sections appear (BTreeMap ordering = sorted by id).
        assert!(out.contains("## CC-BY-4.0: Creative Commons Attribution 4.0"));
        assert!(out.contains("CC-BY body"));
        assert!(out.contains("## MIT: MIT License"));
        assert!(out.contains("MIT body"));
    }

    #[test]
    fn render_notices_text_without_trailing_newline_gets_one() {
        // Given a license text with no trailing newline.
        let licenses = vec![license("MIT", "MIT", "no newline here")];

        // When rendering.
        let out = render_notices(&licenses);

        // Then the closing fence is on its own line.
        assert!(out.contains("no newline here\n```\n"));
    }
}
