//! `auditah generate` — produce all distribution artifacts in one invocation.
//!
//! Runs the audit-gate first (no artifacts on a failing project), then calls
//! the three internal generators: CREDITS.md (attribution), NOTICES.md
//! (license text reproduction), and BOM.md (compliance obligations).

use std::path::{Path, PathBuf};

use crate::audit::run_audit;
use crate::bom::{default_output_path as bom_default, generate_bom};
use crate::credits::{default_output_path as credits_default, generate_credits};
use crate::notices::{default_output_path as notices_default, generate_notices};
use crate::services::Services;
use crate::AppError;
use clap::Args;
use error_stack::{Report, ResultExt};

use super::CommandStatus;

/// Generate all distribution artifacts: CREDITS.md, NOTICES.md, BOM.md.
#[derive(Debug, Args)]
pub struct GenerateCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Output file for CREDITS.md (defaults to `<root>/CREDITS.md`).
    #[arg(long)]
    pub output_credits: Option<PathBuf>,

    /// Output file for NOTICES.md (defaults to `<root>/NOTICES.md`).
    #[arg(long)]
    pub output_notices: Option<PathBuf>,

    /// Output file for BOM.md (defaults to `<root>/BOM.md`).
    #[arg(long)]
    pub output_bom: Option<PathBuf>,
}

impl GenerateCmd {
    /// Anchor relative `--output-*` paths against `cwd` in place. `--root` is
    /// handled separately by the dispatch layer's root resolution.
    pub fn anchor_paths(&mut self, cwd: &Path) {
        if let Some(p) = &self.output_credits {
            self.output_credits = Some(crate::project::anchor(cwd, p));
        }
        if let Some(p) = &self.output_notices {
            self.output_notices = Some(crate::project::anchor(cwd, p));
        }
        if let Some(p) = &self.output_bom {
            self.output_bom = Some(crate::project::anchor(cwd, p));
        }
    }
}

/// Run the generate command.
///
/// # Errors
///
/// Returns an error if services, config load, audit-gate, or any generator
/// fails.
pub fn run(services: &Services, cmd: &GenerateCmd) -> Result<CommandStatus, Report<AppError>> {
    // root is part of the shared container; read it out for default output paths.
    let root = services.config.root();

    // Audit gate: no artifacts on a failing project.
    let report = run_audit(services).change_context(AppError)?;
    if report.has_failures() {
        return Err(Report::new(AppError)
            .attach(format!(
                "{} audit failure(s) - fix before generating artifacts",
                report.fail_count()
            ))
            .attach("run `auditah audit` for details"));
    }
    if report.has_errors() {
        return Err(Report::new(AppError)
            .attach(format!(
                "{} technical error(s) during audit - fix before generating artifacts",
                report.error_count()
            ))
            .attach("run `auditah audit` for details"));
    }

    let output_credits = cmd
        .output_credits
        .clone()
        .unwrap_or_else(|| credits_default(root));
    let output_notices = cmd
        .output_notices
        .clone()
        .unwrap_or_else(|| notices_default(root));
    let output_bom = cmd.output_bom.clone().unwrap_or_else(|| bom_default(root));

    generate_credits(services, &output_credits)
        .change_context(AppError)
        .attach("failed to generate CREDITS.md")?;
    generate_notices(services, &output_notices)
        .change_context(AppError)
        .attach("failed to generate NOTICES.md")?;
    generate_bom(services, &output_bom)
        .change_context(AppError)
        .attach("failed to generate BOM.md")?;

    println!("generate: wrote {}", output_credits.display());
    println!("generate: wrote {}", output_notices.display());
    println!("generate: wrote {}", output_bom.display());
    Ok(CommandStatus::Success)
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn anchor_paths_joins_relative_output_paths_against_cwd() {
        // Given a generate command with relative --output-* paths.
        let mut cmd = GenerateCmd {
            root: PathBuf::from("."),
            output_credits: Some(PathBuf::from("out/CREDITS.md")),
            output_notices: Some(PathBuf::from("out/NOTICES.md")),
            output_bom: Some(PathBuf::from("out/BOM.md")),
        };

        // When anchoring against a cwd.
        cmd.anchor_paths(Path::new("/work"));

        // Then each output path is joined against cwd.
        assert_eq!(
            cmd.output_credits.as_deref(),
            Some(Path::new("/work/out/CREDITS.md"))
        );
        assert_eq!(
            cmd.output_notices.as_deref(),
            Some(Path::new("/work/out/NOTICES.md"))
        );
        assert_eq!(
            cmd.output_bom.as_deref(),
            Some(Path::new("/work/out/BOM.md"))
        );
    }

    #[test]
    fn anchor_paths_leaves_none_outputs_untouched() {
        // Given a generate command with no --output-* (all default).
        let mut cmd = GenerateCmd {
            root: PathBuf::from("."),
            output_credits: None,
            output_notices: None,
            output_bom: None,
        };

        // When anchoring against a cwd.
        cmd.anchor_paths(Path::new("/work"));

        // Then the fields stay None (defaults are resolved later against --root).
        assert!(cmd.output_credits.is_none());
        assert!(cmd.output_notices.is_none());
        assert!(cmd.output_bom.is_none());
    }
}
