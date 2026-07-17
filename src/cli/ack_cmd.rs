//! `auditah ack` — acknowledge a manual-review license id by adding it to
//! `manual_review_acknowledged` in `auditah.toml`.

use std::path::PathBuf;

use crate::config::{ack_ids, render_config_template, CONFIG_FILENAME};
use crate::well_known::{self, ResolveResult};
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use crate::services::Services;

use super::CommandStatus;

/// Acknowledge one or more license ids, suppressing their `ManualReviewRequired`
/// audit findings.
///
/// Writes to `<root>/auditah.toml`: creates the file (defaults template + the
/// ids) if absent, otherwise appends each id to `manual_review_acknowledged`
/// in-place via `toml_edit`, preserving comments and key order. Ids already
/// present are skipped (idempotent).
///
/// Ids unknown to both the project registry and the well-known corpus produce a
/// warning on stderr but are still written (fail-open).
#[derive(Debug, Args)]
pub struct AckCmd {
    /// License id(s) to acknowledge (e.g. `LicenseRef-StudioEULA`). At least
    /// one is required.
    #[arg(num_args = 1..)]
    pub ids: Vec<String>,

    /// Project root containing `auditah.toml`. Defaults to the current
    /// directory.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the ack command.
///
/// # Errors
///
/// Returns an error if services fail or the existing `auditah.toml` cannot be
/// read, parsed, or written.
pub fn run(cmd: &AckCmd) -> Result<CommandStatus, Report<AppError>> {
    let services = Services::real(&cmd.root).change_context(AppError)?;
    let path = cmd.root.join(CONFIG_FILENAME);

    warn_unknown_ids(&services, &cmd.ids);

    if !services.fs.exists(&path) {
        let content = render_config_template(&cmd.ids);
        services
            .fs
            .write(&path, &content)
            .change_context(AppError)
            .attach("failed to write auditah.toml")?;
        println!("ack: wrote {} (created)", path.display());
        return Ok(CommandStatus::Success);
    }

    let existing = services
        .fs
        .read_to_string(&path)
        .change_context(AppError)
        .attach("failed to read auditah.toml")?;
    let updated = ack_ids(&existing, &cmd.ids).change_context(AppError)?;
    services
        .fs
        .write(&path, &updated)
        .change_context(AppError)
        .attach("failed to write auditah.toml")?;
    println!("ack: updated {}", path.display());
    Ok(CommandStatus::Success)
}

/// Warn on stderr for any id that is unknown to both the project registry and
/// the well-known SPDX corpus. Does not block the write (fail-open).
fn warn_unknown_ids(services: &Services, ids: &[String]) {
    for id in ids {
        let known = services.registry.get(id).is_some()
            || matches!(well_known::resolve(id), ResolveResult::Found(_));
        if !known {
            eprintln!("warning: license id {id:?} is not in LICENSES/ or the well-known corpus");
        }
    }
}
