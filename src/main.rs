//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! A license is not an identifier; it is a set of obligations and permissions.
//! See `.plans/auditah/plan.md` for the full specification.

mod cli;
use auditah::AppError;
use clap::{Parser, Subcommand};

use cli::{
    add_cmd::AddCmd, audit_cmd::AuditCmd, credits_cmd::CreditsCmd,
    init_licenses_cmd::InitLicensesCmd, init_pack_cmd::InitPackCmd,
};
use error_stack::Report;

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
    /// Scaffold an attribution sidecar for a single asset.
    Add(AddCmd),
    /// Write full license text files to LICENSES/.
    InitLicenses(InitLicensesCmd),
    /// Write a directory manifest.toml covering a folder.
    InitPack(InitPackCmd),
}

fn main() -> Result<(), Report<AppError>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Audit(cmd) => cli::audit_cmd::run(&cmd),
        Command::Credits(cmd) => cli::credits_cmd::run(&cmd),
        Command::Add(cmd) => cli::add_cmd::run(&cmd),
        Command::InitLicenses(cmd) => cli::init_licenses_cmd::run(&cmd),
        Command::InitPack(cmd) => cli::init_pack_cmd::run(&cmd),
    }
}
