//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

use auditah::cli::command_to_exit_code;
use auditah::cli::{
    ack_cmd::AckCmd, add_license_cmd::AddLicenseCmd, audit_cmd::AuditCmd,
    generate_cmd::GenerateCmd, init_cmd::InitCmd, license_cmd::LicenseCmd, CommandStatus,
};
use clap::{Parser, Subcommand};
use error_stack::Report;

use auditah::AppError;
use std::path::Path;

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

/// Dispatch a parsed command to its handler and return its `CommandStatus`.
///
/// `cwd` is the process working directory captured once at program start; it is
/// threaded into the LICENSES-dependent commands so they never read the
/// environment themselves (relative `--root` values resolve against `cwd`).
fn dispatch(command: Command, cwd: &Path) -> Result<CommandStatus, Report<AppError>> {
    match command {
        Command::Audit(cmd) => auditah::cli::audit_cmd::run(&cmd, cwd),
        Command::AddLicense(cmd) => auditah::cli::add_license_cmd::run(&cmd, cwd),
        Command::License(cmd) => auditah::cli::license_cmd::run(&cmd, cwd),
        Command::Generate(cmd) => auditah::cli::generate_cmd::run(&cmd, cwd),
        Command::Init(cmd) => auditah::cli::init_cmd::run(&cmd),
        Command::Ack(cmd) => auditah::cli::ack_cmd::run(&cmd),
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
