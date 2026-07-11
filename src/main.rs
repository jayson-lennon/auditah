//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

use auditah::cli::command_to_exit_code;
use auditah::cli::{
    add_cmd::AddCmd, add_license_cmd::AddLicenseCmd, audit_cmd::AuditCmd, bom_cmd::BomCmd,
    credits_cmd::CreditsCmd, init_pack_cmd::InitPackCmd, CommandStatus,
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
    /// Generate a CREDITS.md from attribution data.
    Credits(CreditsCmd),
    /// Generate a license bill of materials (BOM.md).
    Bom(BomCmd),
    /// Scaffold an attribution sidecar for a single asset.
    Add(AddCmd),
    /// Scaffold a new license definition (LICENSES/<id>.toml).
    AddLicense(AddLicenseCmd),
    /// Write a directory manifest.toml covering a folder.
    InitPack(InitPackCmd),
}

/// Dispatch a parsed command to its handler and return its `CommandStatus`.
fn dispatch(command: Command) -> Result<CommandStatus, Report<AppError>> {
    match command {
        Command::Audit(cmd) => auditah::cli::audit_cmd::run(&cmd),
        Command::Credits(cmd) => auditah::cli::credits_cmd::run(&cmd),
        Command::Bom(cmd) => auditah::cli::bom_cmd::run(&cmd),
        Command::Add(cmd) => auditah::cli::add_cmd::run(&cmd),
        Command::AddLicense(cmd) => auditah::cli::add_license_cmd::run(&cmd),
        Command::InitPack(cmd) => auditah::cli::init_pack_cmd::run(&cmd),
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
