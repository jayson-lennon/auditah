//! `auditah init` — scaffold a commented `auditah.toml` at the project root.

use std::path::PathBuf;

use crate::config::{render_config_template, CONFIG_FILENAME};
use crate::services::Services;
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Write a commented `auditah.toml` with default values at `<root>/auditah.toml`.
///
/// Refuses to overwrite an existing file unless `--force` is passed.
#[derive(Debug, Args)]
pub struct InitCmd {
    /// Project root where `auditah.toml` will be written. Defaults to the
    /// current directory.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Overwrite an existing `auditah.toml`.
    #[arg(long)]
    pub force: bool,
}

/// Run the init command.
///
/// `init` is the sole creator of `LICENSES/`. The dispatch layer therefore
/// skips the `LICENSES/` discovery walk for `init` and anchors `root` at
/// `cmd.root` directly — so `services.config.root()` here is `cmd.root`.
///
/// # Errors
///
/// Returns an error if an existing file is present without `--force`, or
/// writing the file or creating `LICENSES/` fails.
pub fn run(services: &Services, cmd: &InitCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = services.config.root();
    let path = root.join(CONFIG_FILENAME);

    if services.fs.exists(&path) && !cmd.force {
        return Err(Report::new(AppError).attach(format!(
            "{} already exists; pass --force to overwrite",
            path.display()
        )));
    }

    let content = render_config_template(&[]);
    services
        .fs
        .write(&path, &content)
        .change_context(AppError)
        .attach("failed to write auditah.toml")?;
    println!("init: wrote {}", path.display());

    // LICENSES/ is the project's license home. `init` is the sole command
    // that creates it; other commands discover it rather than create it.
    // create_dir_all is idempotent, so re-running init on an existing
    // project leaves an already-present LICENSES untouched.
    let licenses_dir = root.join("LICENSES");
    services
        .fs
        .create_dir_all(&licenses_dir)
        .change_context(AppError)
        .attach("failed to create LICENSES directory")?;
    println!("init: ensured {} exists", licenses_dir.display());

    Ok(CommandStatus::Success)
}
