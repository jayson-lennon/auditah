//! `auditah credits` — generate a human-facing CREDITS.md from attribution data.
//!
//! Reads every asset, resolves its attribution record, filters to those whose
//! effective terms require attribution (CC0 and other attribution-free licenses
//! are omitted), groups by author, and renders Markdown entries. Modification
//! notices appear only when `requires_modification_notice` is set AND the asset
//! is marked modified.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use error_stack::{Report, ResultExt};
use wherror::Error;

use crate::config::Config;
use crate::discovery::enumerator::{enumerate, ExcludeMatcher};
use crate::discovery::resolver::resolve;
use crate::model::attribution::AttributionRecord;
use crate::model::terms::{effective_terms, LicenseTerms};
use crate::services::Services;

/// Error generating credits.
#[derive(Debug, Error)]
#[error(debug)]
pub struct CreditsError;

/// A single credits entry ready for rendering.
#[derive(Debug, Clone)]
pub(crate) struct CreditEntry {
    title: String,
    license: String,
    source: String,
    year: u16,
    modified_notice: Option<String>,
}

/// Subsystem context for credits generation.
#[derive(Debug, Clone)]
pub struct CreditsCtx<'a> {
    pub services: &'a Services,
    pub config: &'a Config,
    pub root: &'a Path,
}

/// Collect attribution-bearing credit entries, grouped by author.
/// CC0 and attribution-free licenses are omitted.
///
/// # Errors
///
/// Returns `CreditsError` if enumeration or resolution fails.
pub(crate) fn collect_credits(ctx: &CreditsCtx) -> Result<BTreeMap<String, Vec<CreditEntry>>, Report<CreditsError>> {
    let excludes = build_excludes(ctx);
    let assets = enumerate(&ctx.services.fs, ctx.root, &excludes)
        .change_context(CreditsError)
        .attach("failed to enumerate assets for credits")?;

    let mut by_author: BTreeMap<String, Vec<CreditEntry>> = BTreeMap::new();
    for asset in &assets {
        let Some(record) = resolve(&ctx.services.fs, asset, ctx.root)
            .change_context(CreditsError)
            .attach("failed to resolve asset during credits generation")?
            .record
        else {
            continue;
        };
        if let Some(entry) = entry_if_attribution_required(&record, ctx) {
            by_author
                .entry(record.author.clone())
                .or_default()
                .push(entry);
        }
    }
    sort_entries(&mut by_author);
    Ok(by_author)
}

/// Build the exclude matcher from default + user globs.
fn build_excludes(ctx: &CreditsCtx) -> ExcludeMatcher {
    let patterns = crate::discovery::all_excludes(&ctx.config.exclude);
    ExcludeMatcher::new(&patterns)
        .expect("exclude patterns must compile")
}

/// Decide whether an asset needs a credits entry, and build it if so.
/// Returns None when the effective terms do not require attribution.
fn entry_if_attribution_required(
    record: &AttributionRecord,
    ctx: &CreditsCtx,
) -> Option<CreditEntry> {
    let entry = ctx.services.registry.get(&record.license)?;
    let terms = effective_terms(&entry.terms, &record.overrides);
    if !terms.requires_attribution {
        return None;
    }
    let modified_notice = modification_notice(&terms, record);
    Some(CreditEntry {
        title: record.title.clone(),
        license: entry.id.clone(),
        source: record.source.clone(),
        year: record.year,
        modified_notice,
    })
}

/// Emit a '(modified from original)' notice only when the license requires it
/// and the asset is marked modified.
fn modification_notice(terms: &LicenseTerms, record: &AttributionRecord) -> Option<String> {
    if terms.requires_modification_notice && record.modified {
        Some("(modified from original)".to_string())
    } else {
        None
    }
}

/// Sort each author's entries alphabetically by title (stable display order).
fn sort_entries(by_author: &mut BTreeMap<String, Vec<CreditEntry>>) {
    for entries in by_author.values_mut() {
        entries.sort_by(|a, b| a.title.cmp(&b.title));
    }
}

/// Render the collected credits as a CREDITS.md string.
#[must_use]
pub(crate) fn render_credits(by_author: &BTreeMap<String, Vec<CreditEntry>>) -> String {
    let mut out = String::from("# Credits\n\n");
    if by_author.is_empty() {
        out.push_str("_No attribution-required assets found._\n");
        return out;
    }
    for (author, entries) in by_author {
        out.push_str(&format!("## {author}\n\n"));
        for e in entries {
            out.push_str(&format!(
                "- **{}** ({}), {} — [source]({})",
                e.title, e.license, e.year, e.source
            ));
            if let Some(notice) = &e.modified_notice {
                out.push_str(&format!(" {notice}"));
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// Full pipeline: collect, render, and write CREDITS.md to `output_path`.
///
/// # Errors
///
/// Returns `CreditsError` on enumeration, resolution, or write failure.
pub fn generate_credits(
    ctx: &CreditsCtx,
    output_path: &Path,
) -> Result<(), Report<CreditsError>> {
    let by_author = collect_credits(ctx)?;
    let markdown = render_credits(&by_author);
    ctx.services
        .fs
        .write(output_path, &markdown)
        .change_context(CreditsError)
        .attach("failed to write CREDITS.md")
        .attach(output_path.display().to_string())
}

/// Default output path: `<root>/CREDITS.md`.
#[must_use]
pub fn default_output_path(root: &Path) -> PathBuf {
    root.join("CREDITS.md")
}
