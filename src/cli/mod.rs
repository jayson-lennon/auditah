//! CLI command handlers.
//!
//! Each subcommand lives in its own submodule and exposes a `run()` function
//! returning `Result<CommandStatus, Report<AppError>>`. `main` maps the result
//! to a process exit code (see [`command_to_exit_code`]).

use crate::AppError;
use error_stack::Report;

pub mod audit_cmd;
pub mod generate_cmd;
pub mod init_cmd;
pub mod license_ack_cmd;
pub mod license_assign_cmd;
pub mod license_cmd;
pub mod license_provision_cmd;

/// The outcome of a command that completed its work.
///
/// `Err` is reserved for *technical* failures (the command could not do its
/// job: IO errors, config load failures, walk failures). When a command
/// *succeeds* but the result is actionable — specifically, `audit` finds FAIL
/// findings — it returns `Ok(ComplianceFailure)` so `main` can surface a
/// distinct exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    /// The command succeeded and there is nothing actionable.
    Success,
    /// The command succeeded, but found compliance violations (audit only).
    ComplianceFailure,
}

/// Map a command `Result` to a process exit code.
///
/// - `Ok(Success)` → `0` (all good)
/// - `Ok(ComplianceFailure)` → `1` (audit found FAIL findings)
/// - `Err(_)` → `2` (technical failure)
#[must_use]
pub fn command_to_exit_code(result: &Result<CommandStatus, Report<AppError>>) -> i32 {
    match result {
        Ok(CommandStatus::Success) => 0,
        Ok(CommandStatus::ComplianceFailure) => 1,
        Err(_) => 2,
    }
}
