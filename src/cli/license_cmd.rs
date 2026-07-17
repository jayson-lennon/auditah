//! `auditah license <sub>` — the license lifecycle command group.
//!
//! Holds the three license lifecycle operations as subcommands:
//! - `assign`    — write an attribution sidecar / directory manifest for an asset.
//! - `provision` — write a license definition into `LICENSES/` (well-known or `--custom`).
//! - `ack`       — acknowledge a manual-review license id into `auditah.toml`.
//!
//! Bare `auditah license` (no subcommand) errors and prints help: clap enforces
//! `subcommand_required` implicitly via the `#[command(subcommand)]` field.

use crate::services::Services;
use crate::AppError;
use clap::{Args, Subcommand};
use error_stack::Report;

use super::license_ack_cmd::LicenseAckCmd;
use super::license_assign_cmd::LicenseAssignCmd;
use super::license_provision_cmd::LicenseProvisionCmd;
use super::CommandStatus;

/// The `license` noun-command group.
#[derive(Debug, Args)]
pub struct LicenseCmd {
    #[command(subcommand)]
    pub command: LicenseSub,
}

/// Subcommands of the `license` group.
#[derive(Debug, Subcommand)]
pub enum LicenseSub {
    /// Assign a license to an asset (writes a sidecar or directory manifest).
    Assign(LicenseAssignCmd),
    /// Provision a license definition into `LICENSES/` (well-known or `--custom`).
    Provision(LicenseProvisionCmd),
    /// Acknowledge a manual-review license id (adds to `manual_review_acknowledged`).
    Ack(LicenseAckCmd),
}

/// Run the `license` group.
///
/// # Errors
///
/// Delegates to the selected subcommand; returns its error.
pub fn run(services: &Services, cmd: &LicenseCmd) -> Result<CommandStatus, Report<AppError>> {
    match &cmd.command {
        LicenseSub::Assign(c) => super::license_assign_cmd::run(services, c),
        LicenseSub::Provision(c) => super::license_provision_cmd::run(services, c),
        LicenseSub::Ack(c) => super::license_ack_cmd::run(services, c),
    }
}
