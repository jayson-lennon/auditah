//! `auditah license provision` — scaffold a license grid (+ text for well-known ids).

use std::path::PathBuf;

use crate::license_def::{grid_id_from_path, write_grid, write_license_template, write_text};
use crate::services::Services;
use crate::well_known::{self, ResolveResult};
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Scaffold a license grid (`.toml`) — and for well-known SPDX ids, the license
/// text (`.txt`) — in `<root>/LICENSES/`.
///
/// Without `--custom`: sources from the embedded well-known SPDX corpus.
/// `license provision MIT` extracts canonical `MIT.txt` + the authored `MIT.toml` grid.
/// If a grid isn't authored for that id, a `default_fail()` placeholder grid is
/// written and a warning is printed (the text is still extracted).
///
/// With `--custom`: writes a `LicenseRef-<name>` grid using `default_fail()`
/// defaults; refuses if `<name>` collides with a well-known SPDX id (case-
/// insensitive).
#[derive(Debug, Args)]
pub struct LicenseProvisionCmd {
    /// License name. Either a well-known SPDX id (e.g. `MIT`, no flag) or a custom
    /// name (with `--custom`, prefixed as `LicenseRef-<name>`).
    pub name: String,

    /// Create a custom `LicenseRef-<name>` license instead of sourcing from the
    /// well-known SPDX corpus.
    #[arg(long)]
    pub custom: bool,

    /// Project root containing `LICENSES/`. Defaults to the current directory.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Run the license provision command.
///
/// `cwd` is the process working directory captured at program start, used to
/// anchor a relative `--root` value. The project root is discovered by
/// walking up from `--root` for a `LICENSES/` directory (`init` is the sole
/// creator of `LICENSES/`).
///
/// # Errors
///
/// Returns an error if `--root` cannot be resolved to a project root, services
/// fail, the name doesn't resolve (unknown SPDX id), a `--custom` name collides
/// with a well-known id, or a target file already exists.
pub fn run(
    services: &Services,
    cmd: &LicenseProvisionCmd,
) -> Result<CommandStatus, Report<AppError>> {
    let root = services.config.root();

    if cmd.custom {
        // Refuse if the custom name collides with a well-known id (case-insensitive).
        if let ResolveResult::Found(_) = well_known::resolve(&cmd.name) {
            return Err(Report::new(AppError).attach(format!(
                "{:?} is a known SPDX id; use `license provision {}` (without --custom) to source it from the corpus",
                cmd.name, cmd.name
            )));
        }
        let path = write_license_template(services, root, &cmd.name).change_context(AppError)?;
        let id = grid_id_from_path(&path, &cmd.name);
        eprintln!(
            "warning: wrote default_fail() grid for {name:?} (manual_review = true). \
             Fill in LICENSES/{id}.toml and add the id to `manual_review_acknowledged` \
             when ready.",
            name = cmd.name,
            id = id,
        );
        println!("provision: wrote {}", path.display());
        return Ok(CommandStatus::Success);
    }

    // Well-known path.
    match well_known::resolve(&cmd.name) {
        ResolveResult::NotFound => Err(Report::new(AppError).attach(format!(
            "unknown SPDX id {:?}; use `--custom` to create a custom (LicenseRef-) license",
            cmd.name
        ))),
        ResolveResult::Found(canonical) => {
            // Always extract the canonical text.
            let text = well_known::extract_text(&canonical);
            let text_path =
                write_text(services, root, &canonical, &text).change_context(AppError)?;

            // Authored grid if present, else default_fail() placeholder + warning.
            let (grid_content, placeholder) = match well_known::extract_grid(&canonical) {
                Some(g) => (g, false),
                None => (
                    crate::license_def::render_license_template(&canonical),
                    true,
                ),
            };
            let grid_path =
                write_grid(services, root, &canonical, &grid_content).change_context(AppError)?;

            if placeholder {
                eprintln!(
                    "warning: no authored grid for {canonical} — wrote a default_fail() \
                     placeholder (manual_review = true). Fill in LICENSES/{canonical}.toml \
                     and add the id to `manual_review_acknowledged` when ready."
                );
            }
            println!(
                "provision: wrote {} , {}",
                text_path.display(),
                grid_path.display()
            );
            Ok(CommandStatus::Success)
        }
    }
}
