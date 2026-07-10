//! auditah — obligation-aware license compliance + attribution tool for gamedev.
//!
//! Library crate: exposes core modules for integration tests and future
//! embedding. The binary target (`src/main.rs`) consumes these via the CLI.

pub mod audit;
pub mod config;
pub mod discovery;
pub mod model;
pub mod registry;
pub mod services;
