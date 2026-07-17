//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use auditah::cli::command_to_exit_code;
use auditah::cli::{
    audit_cmd::AuditCmd, generate_cmd::GenerateCmd, init_cmd::InitCmd,
    license_assign_cmd::LicenseAssignCmd, license_cmd::LicenseCmd, CommandStatus,
};
use auditah::registry::LicenseRegistryService;
use auditah::services::clock::{ClockService, RealClock};
use auditah::services::config::ConfigService;
use auditah::services::fs::{FsService, RealFs};
use auditah::services::Services;
use clap::{Parser, Subcommand};
use error_stack::{Report, ResultExt};

use auditah::AppError;

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(name = "auditah", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Audit license compliance of assets.
    Audit(AuditCmd),
    /// Generate all distribution artifacts (CREDITS.md, NOTICES.md, BOM.md).
    Generate(GenerateCmd),
    /// Write a commented `auditah.toml` at the project root.
    Init(InitCmd),
    /// License lifecycle: assign, provision, ack.
    License(LicenseCmd),
    /// Assign a license to an asset (shortcut for `license assign`).
    Assign(LicenseAssignCmd),
}

/// Dispatch a parsed command: resolve its project root, assemble the
/// [`Services`] container once, and hand both off to the command's `run`.
///
/// `cwd` is the process working directory captured at program start; it anchors
/// relative `--root` / target values so command code never reads the process
/// environment itself.
///
/// Root resolution is command-specific:
/// - `init`/`ack` (via `license ack`) anchor directly at `cmd.root` (no `LICENSES/`
///   walk — `init` is the sole creator of `LICENSES/`).
/// - `audit`/`generate`/`provision` walk up from `cmd.root` for a `LICENSES/`.
/// - `assign` walks up from `--root`, the target, or the target's parent
///   depending on the target's filesystem type.
fn dispatch(command: Command, cwd: &Path) -> Result<CommandStatus, Report<AppError>> {
    let root = resolve_root(&command, cwd)?;

    // Create-then-configure: exactly one `FsService` is built and reused by the
    // registry and config loads. The final `services` is immutable and escapes;
    // none of the throwaway construction pieces leak into command scope.
    let services = {
        let fs = FsService::new(Arc::new(RealFs::new()));
        let registry = LicenseRegistryService::load(&fs, &root)
            .change_context(AppError)
            .attach("failed to load license registry")?;
        let clock = ClockService::new(Arc::new(RealClock::new()));
        let config = ConfigService::load(&fs, &root)
            .change_context(AppError)
            .attach("failed to load config")?;
        Services {
            fs,
            registry,
            clock,
            config,
        }
    };

    match command {
        Command::Audit(cmd) => auditah::cli::audit_cmd::run(&services, &cmd),
        Command::Generate(cmd) => auditah::cli::generate_cmd::run(&services, &cmd),
        Command::Init(cmd) => auditah::cli::init_cmd::run(&services, &cmd),
        Command::License(cmd) => auditah::cli::license_cmd::run(&services, &cmd),
        Command::Assign(cmd) => auditah::cli::license_assign_cmd::run(&services, &cmd),
    }
}

/// Resolve the project root for `command`, anchoring relative starts against
/// `cwd`.
///
/// `init` and `ack` (via `license ack`) use `cmd.root` verbatim — they neither
/// discover nor require a pre-existing `LICENSES/`. All other commands walk up
/// from their start for a `LICENSES/` directory via [`auditah::project::resolve_or_error`].
///
/// # Errors
///
/// Returns `AppError` when a `LICENSES/`-discovering command cannot find one.
fn resolve_root(command: &Command, cwd: &Path) -> Result<PathBuf, Report<AppError>> {
    match command {
        // init: no LICENSES/ walk. init is the sole creator of LICENSES/.
        Command::Init(c) => Ok(resolve_anchor(cwd, &c.root)),

        // audit/generate: walk up from --root for LICENSES/.
        Command::Audit(c) => auditah::project::resolve_or_error(cwd, &c.root),
        Command::Generate(c) => auditah::project::resolve_or_error(cwd, &c.root),

        // Top-level assign shortcut: same discovery as `license assign`.
        Command::Assign(c) => resolve_assign(cwd, c),

        // license group: dispatch root resolution to the selected subcommand.
        // assign -> target-aware walk; provision -> walk from --root;
        // ack -> anchor at --root verbatim (may write auditah.toml with no LICENSES/).
        Command::License(c) => match &c.command {
            auditah::cli::license_cmd::LicenseSub::Assign(inner) => resolve_assign(cwd, inner),
            auditah::cli::license_cmd::LicenseSub::Provision(inner) => {
                auditah::project::resolve_or_error(cwd, &inner.root)
            }
            auditah::cli::license_cmd::LicenseSub::Ack(inner) => {
                Ok(resolve_anchor(cwd, &inner.root))
            }
        },
    }
}

/// Resolve the project root for an `assign` command: probe the target's
/// filesystem type (cheap metadata call) so the walk-up start matches the
/// command's file-vs-dir branch, then walk up for a `LICENSES/`.
fn resolve_assign(
    cwd: &Path,
    cmd: &auditah::cli::license_assign_cmd::LicenseAssignCmd,
) -> Result<PathBuf, Report<AppError>> {
    let is_dir = std::fs::metadata(&cmd.target).is_ok_and(|m| m.is_dir());
    let start = auditah::cli::license_assign_cmd::resolve_start(cmd, is_dir);
    auditah::project::resolve_or_error(cwd, &start)
}

/// Anchor a relative `path` against `cwd`, leaving absolute paths untouched.
/// Used by `init`/`ack`, which take `--root` verbatim without a `LICENSES/` walk.
fn resolve_anchor(cwd: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn main() {
    let cli = Cli::parse();
    // Capture the process cwd once at program start; commands receive it as
    // an injected parameter rather than reading the environment themselves.
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => {
            eprintln!("failed to read process cwd at startup: {e}");
            std::process::exit(2);
        }
    };
    let result = dispatch(cli.command, &cwd);
    let exit_code = command_to_exit_code(&result);
    if let Err(report) = &result {
        eprintln!("{report:?}");
    }
    std::process::exit(exit_code);
}
