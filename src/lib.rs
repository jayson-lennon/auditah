//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! Library crate: exposes core modules for integration tests and future
//! embedding. The binary target (`src/main.rs`) consumes these via the CLI.

use wherror::Error;

pub mod add;
pub mod audit;
pub mod config;
pub mod credits;
pub mod discovery;
pub mod init_licenses;
pub mod model;
pub mod registry;
pub mod services;

/// An application error occurred
#[derive(Debug, Error)]
#[error(debug)]
pub struct AppError;
