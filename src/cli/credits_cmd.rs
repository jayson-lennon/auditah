//! `auditah credits` — generate a CREDITS.md from attribution data.

use std::path::PathBuf;

use clap::Args;

use auditah::config::Config;
use auditah::credits::{default_output_path, generate_credits, CreditsCtx};
use auditah::services::Services;

/// Generate a CREDITS.md from attribution sidecars/manifests.
#[derive(Debug, Args)]
pub struct CreditsCmd {
    /// Project root to scan (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// Output file path (defaults to `<root>/CREDITS.md`).
    #[arg(long)]
    pub output: Option<PathBuf>,
}

/// Run the credits command. Returns the process exit code.
pub fn run(cmd: &CreditsCmd) -> i32 {
    let root = &cmd.root;
    let services = Services::real();
    let config = match Config::load(&services.fs, root) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: failed to load config: {e:?}");
            return 2;
        }
    };
    let output = cmd
        .output
        .clone()
        .unwrap_or_else(|| default_output_path(root));
    let ctx = CreditsCtx {
        services: &services,
        config: &config,
        root,
    };
    match generate_credits(&ctx, &output) {
        Ok(()) => {
            println!("credits: wrote {}", output.display());
            0
        }
        Err(e) => {
            eprintln!("error: failed to generate credits: {e:?}");
            2
        }
    }
}
