//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use auditah::cli::command_to_exit_code;
use auditah::cli::{
    ack_cmd::AckCmd, add_license_cmd::AddLicenseCmd, audit_cmd::AuditCmd,
    generate_cmd::GenerateCmd, init_cmd::InitCmd, license_cmd::LicenseCmd, CommandStatus,
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
    /// Scaffold a new license definition (LICENSES/<id>.toml).
    AddLicense(AddLicenseCmd),
    /// Scaffold an attribution sidecar (file) or directory manifest (dir).
    License(LicenseCmd),
    /// Generate all distribution artifacts (CREDITS.md, NOTICES.md, BOM.md).
    Generate(GenerateCmd),
    /// Write a commented `auditah.toml` at the project root.
    Init(InitCmd),
    /// Acknowledge a manual-review license id (adds to `manual_review_acknowledged`).
    Ack(AckCmd),
}

/// Dispatch a parsed command: resolve its project root, assemble the
/// [`Services`] container once, and hand both off to the command's `run`.
///
/// `cwd` is the process working directory captured at program start; it anchors
/// relative `--root` / target values so command code never reads the process
/// environment itself.
///
/// Root resolution is command-specific:
/// - `init`/`ack` anchor directly at `cmd.root` (no `LICENSES/` walk — `init`
///   is the sole creator of `LICENSES/`).
/// - `audit`/`generate`/`add-license` walk up from `cmd.root` for a `LICENSES/`.
/// - `license` walks up from `--root`, the target, or the target's parent
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
        Command::AddLicense(cmd) => auditah::cli::add_license_cmd::run(&services, &cmd),
        Command::License(cmd) => auditah::cli::license_cmd::run(&services, &cmd),
        Command::Generate(cmd) => auditah::cli::generate_cmd::run(&services, &cmd),
        Command::Init(cmd) => auditah::cli::init_cmd::run(&services, &cmd),
        Command::Ack(cmd) => auditah::cli::ack_cmd::run(&services, &cmd),
    }
}

/// Resolve the project root for `command`, anchoring relative starts against
/// `cwd`.
///
/// `init` and `ack` use `cmd.root` verbatim — they neither discover nor require
/// a pre-existing `LICENSES/`. All other commands walk up from their start for
/// a `LICENSES/` directory via [`auditah::project::resolve_or_error`].
///
/// # Errors
///
/// Returns `AppError` when a `LICENSES/`-discovering command cannot find one.
fn resolve_root(command: &Command, cwd: &Path) -> Result<PathBuf, Report<AppError>> {
    match command {
        // init/ack: no LICENSES/ walk. init is the sole creator of LICENSES/.
        Command::Init(c) => Ok(resolve_anchor(cwd, &c.root)),
        Command::Ack(c) => Ok(resolve_anchor(cwd, &c.root)),

        // audit/generate/add-license: walk up from --root for LICENSES/.
        Command::Audit(c) => auditah::project::resolve_or_error(cwd, &c.root),
        Command::Generate(c) => auditah::project::resolve_or_error(cwd, &c.root),
        Command::AddLicense(c) => auditah::project::resolve_or_error(cwd, &c.root),

        // license: start depends on the target's filesystem type. Probe it here
        // (cheap metadata call) so the walk-up start matches the command's
        // file-vs-dir branch.
        Command::License(c) => {
            let is_dir = std::fs::metadata(&c.target).is_ok_and(|m| m.is_dir());
            let start = auditah::cli::license_cmd::resolve_start(c, is_dir);
            auditah::project::resolve_or_error(cwd, &start)
        }
    }
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
