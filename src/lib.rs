//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! Library crate: exposes core modules for integration tests and future
//! embedding. The binary target (`src/main.rs`) consumes these via the CLI.

use wherror::Error;

pub mod add;
pub mod add_license;
pub mod audit;
pub mod bom;
pub mod cli;
pub mod config;
pub mod credits;
pub mod discovery;
pub mod model;
pub mod notices;
pub mod registry;
pub mod services;
pub mod well_known;

#[cfg(feature = "test-helper")]
#[doc(hidden)]
pub mod test_support;

/// An application error occurred
#[derive(Debug, Error)]
#[error(debug)]
pub struct AppError;
