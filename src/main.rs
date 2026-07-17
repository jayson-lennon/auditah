//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

use auditah::cli::command_to_exit_code;
use auditah::cli::{
    ack_cmd::AckCmd, audit_cmd::AuditCmd, generate_cmd::GenerateCmd, init_cmd::InitCmd,
    init_pack_cmd::InitPackCmd, license_cmd::LicenseCmd, sidecar_cmd::SidecarCmd, CommandStatus,
};
use clap::{Parser, Subcommand};
use error_stack::Report;

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
    /// Scaffold an attribution sidecar for a single asset.
    Sidecar(SidecarCmd),
    /// Scaffold a new license definition (LICENSES/<id>.toml).
    License(LicenseCmd),
    /// Generate all distribution artifacts (CREDITS.md, NOTICES.md, BOM.md).
    Generate(GenerateCmd),
    /// Write a commented `auditah.toml` at the project root.
    Init(InitCmd),
    /// Acknowledge a manual-review license id (adds to `manual_review_acknowledged`).
    Ack(AckCmd),
    /// Write a directory `_manifest.toml` covering a folder.
    InitPack(InitPackCmd),
}

/// Dispatch a parsed command to its handler and return its `CommandStatus`.
fn dispatch(command: Command) -> Result<CommandStatus, Report<AppError>> {
    match command {
        Command::Audit(cmd) => auditah::cli::audit_cmd::run(&cmd),
        Command::Sidecar(cmd) => auditah::cli::sidecar_cmd::run(&cmd),
        Command::License(cmd) => auditah::cli::license_cmd::run(&cmd),
        Command::Init(cmd) => auditah::cli::init_cmd::run(&cmd),
        Command::Ack(cmd) => auditah::cli::ack_cmd::run(&cmd),
        Command::InitPack(cmd) => auditah::cli::init_pack_cmd::run(&cmd),
        Command::Generate(cmd) => auditah::cli::generate_cmd::run(&cmd),
    }
}

fn main() {
    let cli = Cli::parse();
    let result = dispatch(cli.command);
    let exit_code = command_to_exit_code(&result);
    if let Err(report) = &result {
        eprintln!("{report:?}");
    }
    std::process::exit(exit_code);
}
