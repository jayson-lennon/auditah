//! `auditah audit` — obligation-aware compliance check.
//!
//! Output contract (per the streaming-cascade design):
//! - **stderr**: a single live spinner line that updates in place (one tick
//!   per audited asset) so a large project never feels like a dead terminal.
//! - **stdout**: the batched, **sorted** FAILED list + a summary line, with
//!   ACCEPTED asset paths only under `--verbose`.
//! - **technical errors are printed dead last**, after the summary, so they are
//!   never lost in compliance-finding noise.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clap::Args;
use error_stack::{Report, ResultExt};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use super::CommandStatus;
use crate::audit::build_excludes;
use crate::audit::pipeline::run_pipeline;
use crate::audit::report::Verdict;
use crate::config::Config;
use crate::services::Services;
use crate::AppError;

/// Audit license compliance of assets under the project.
#[derive(Debug, Default, Args)]
pub struct AuditCmd {
    /// Project root to audit (defaults to current directory).
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
    /// Maximum concurrent directory descents. Defaults to the number of CPUs;
    /// falls back to 1 if that cannot be determined. 1 = fully serial.
    #[arg(long, default_value_t = default_jobs())]
    pub jobs: usize,
    /// Print every accepted asset path in addition to failures.
    #[arg(long, short = 'v')]
    pub verbose: bool,
}

/// Run the audit command.
///
/// Returns `Ok(Success)` when the project is clean, `Ok(ComplianceFailure)`
/// when FAIL findings are present (exit 1), and `Err` on technical failures
/// (exit 2).
///
/// # Errors
///
/// Returns an error if the services, config, or audit pipeline fail.
pub fn run(cmd: &AuditCmd) -> Result<CommandStatus, Report<AppError>> {
    let root = &cmd.root;
    let services = Services::real(root).change_context(AppError)?;
    let config = Config::load(&services.fs, root)
        .change_context(AppError)
        .attach("failed to load config")?;
    let excludes = build_excludes(&config)
        .change_context(AppError)
        .attach("failed to build exclude matcher")?;

    let services = Arc::new(services);
    let config = Arc::new(config);

    let rt = Runtime::new()
        .change_context(AppError)
        .attach("failed to build tokio runtime")?;
    let verdicts = rt.block_on(async move {
        let (progress_tx, mut progress_rx) = mpsc::channel::<()>(cmd.jobs.max(1) * 4);
        let pipeline = tokio::spawn(run_pipeline(
            Arc::clone(&services),
            Arc::clone(&config),
            root.clone(),
            excludes,
            cmd.jobs,
            progress_tx,
        ));
        // Live progress to stderr: a single spinner line that overwrites
        // itself in place. Created only when stderr is a TTY; the non-TTY
        // path still drains the channel (preserving backpressure) but draws
        // nothing, matching prior piped-output behavior.
        let spinner = if std::io::stderr().is_terminal() {
            let pb = ProgressBar::new_spinner();
            pb.enable_steady_tick(Duration::from_millis(120));
            pb.set_style(
                ProgressStyle::with_template("{spinner} {msg}")
                    .unwrap_or_else(|_| ProgressStyle::default_spinner()),
            );
            Some(pb)
        } else {
            None
        };

        let mut count = 0usize;
        while progress_rx.recv().await.is_some() {
            count += 1;
            if let Some(pb) = spinner.as_ref() {
                pb.set_message(format!("auditing… {count}"));
            }
        }

        let verdicts = pipeline
            .await
            .change_context(AppError)
            .attach("audit driver task panicked")?
            .change_context(AppError)
            .attach("audit pipeline failed");

        // Clear the spinner before the async block yields so the line is gone
        // before `render()` prints the FAIL list — no orphan spinner line
        // sits above the results, even when the driver errored.
        if let Some(pb) = spinner {
            pb.finish_and_clear();
        }

        verdicts
    })?;

    render(&verdicts, cmd.verbose)
}

/// `CommandStatus` for compliance; technical `Error` verdicts escalate to
/// `Err(AppError)` so they map to exit code 2 (more severe than a compliance
/// failure's exit 1).
fn render(verdicts: &[Verdict], verbose: bool) -> Result<CommandStatus, Report<AppError>> {
    let out = format_verdicts(verdicts, verbose);
    if !out.stdout.is_empty() {
        print!("{}", out.stdout);
    }
    if !out.stderr.is_empty() {
        eprint!("{}", out.stderr);
    }
    out.status
}

/// Pure formatter: turns the raw verdict stream into the deterministic
/// stdout + stderr text and the command status. Separated from `render` so it
/// is directly unit-testable without capturing process stdio.
///
/// Ordering contract: stdout = sorted FAILED list + summary (and ACCEPTED
/// paths only under `verbose`); stderr = the ERRORS block, printed dead last
/// after the summary so technical errors are never lost in compliance noise.
#[must_use]
fn format_verdicts(verdicts: &[Verdict], verbose: bool) -> FormattedOutput {
    let mut failed: Vec<(String, &str)> = Vec::new();
    let mut accepted: Vec<String> = Vec::new();
    let mut errors: Vec<(String, &str)> = Vec::new();
    for v in verdicts {
        match v {
            Verdict::Accepted(path) => accepted.push(path.display().to_string()),
            Verdict::Failed(finding) => {
                failed.push((finding.asset.display().to_string(), &finding.detail));
            }
            Verdict::Error(path, detail) => errors.push((path.display().to_string(), detail)),
        }
    }

    // Batched + sorted FAILED list (stable across --jobs values).
    failed.sort_by(|a, b| a.0.cmp(&b.0));

    let status = if failed.is_empty() {
        CommandStatus::Success
    } else {
        CommandStatus::ComplianceFailure
    };

    let mut stdout: Vec<String> = Vec::new();
    if verbose {
        stdout.push(format!("ACCEPTED ({}):", accepted.len()));
        for path in &accepted {
            stdout.push(format!("  {path}"));
        }
    }
    if failed.is_empty() {
        stdout.push("audit: no compliance failures".to_string());
    } else {
        stdout.push(format!("FAIL ({}):", failed.len()));
        for (asset, detail) in &failed {
            stdout.push(format!("  {asset} — {detail}"));
        }
    }
    stdout.push(format!(
        "summary: {} accepted, {} failed",
        accepted.len(),
        failed.len()
    ));
    let stdout = join_lines(&stdout);

    // Technical errors escalate to exit 2: they are infrastructure failures,
    // not compliance findings, and must never be lost. They are printed dead
    // last on stderr, after the summary above.
    let mut stderr: Vec<String> = Vec::new();
    let final_status = if errors.is_empty() {
        Ok(status)
    } else {
        stderr.push(format!("ERRORS ({}):", errors.len()));
        for (path, detail) in &errors {
            stderr.push(format!("  {path} — {detail}"));
        }
        Err(Report::new(AppError).attach(format!(
            "{} technical error(s) surfaced during the audit",
            errors.len()
        )))
    };
    let stderr = join_lines(&stderr);

    FormattedOutput {
        stdout,
        stderr,
        status: final_status,
    }
}

/// Join pre-built lines into a single newline-terminated block. Avoids
/// `write!` into `String` (ambiguous between `std::io::Write` and
/// `std::fmt::Write` in this module) and the `format_push_string` lint.
fn join_lines(lines: &[String]) -> String {
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Output of [`format_verdicts`]: the formatted streams + exit status.
struct FormattedOutput {
    stdout: String,
    stderr: String,
    status: Result<CommandStatus, Report<AppError>>,
}

/// Default worker count: available CPUs, falling back to 1.
fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::{format_verdicts, CommandStatus};
    use crate::audit::report::{Finding, FindingCode, Severity, Verdict};
    use std::path::PathBuf;

    fn failed(asset: &str, detail: &str) -> Verdict {
        Verdict::Failed(Finding {
            severity: Severity::Fail,
            code: FindingCode::UnlicensedAsset,
            asset: PathBuf::from(asset),
            detail: detail.to_string(),
        })
    }

    #[test]
    fn clean_verdicts_yield_success_status() {
        // Given a single accepted verdict.
        let verdicts = vec![Verdict::Accepted(PathBuf::from("/p/a.glb"))];

        // When formatting.
        let out = format_verdicts(&verdicts, false);

        // Then the status is Success and stdout reports no failures.
        assert!(out.status.is_ok());
        assert!(out.stdout.contains("no compliance failures"));
    }

    #[test]
    fn failed_verdicts_yield_compliance_failure_and_sorted_list() {
        // Given two failures arriving out of path order.
        let verdicts = vec![
            failed("/p/z.glb", "no license"),
            failed("/p/a.glb", "no license"),
        ];

        // When formatting.
        let out = format_verdicts(&verdicts, false);

        // Then the status is ComplianceFailure and the list is path-sorted.
        assert_eq!(
            out.status.expect("status"),
            CommandStatus::ComplianceFailure
        );
        let a_pos = out.stdout.find("a.glb").expect("a.glb");
        let z_pos = out.stdout.find("z.glb").expect("z.glb");
        assert!(a_pos < z_pos);
    }

    #[test]
    fn error_verdicts_escalate_to_exit_two_on_stderr_after_summary() {
        // Given a failure and a technical error.
        let verdicts = vec![
            failed("/p/a.glb", "no license"),
            Verdict::Error(PathBuf::from("/p/sub"), "unreadable manifest".to_string()),
        ];

        // When formatting.
        let out = format_verdicts(&verdicts, false);

        // Then the status is an error (exit 2) and the summary lives on stdout
        // while the ERRORS block lives on stderr — errors are never mixed into
        // the compliance output.
        assert!(out.status.is_err());
        assert!(out.stdout.contains("summary:"));
        assert!(!out.stdout.contains("ERRORS"));
        assert!(out.stderr.contains("ERRORS (1):"));
        assert!(out.stderr.contains("unreadable manifest"));
    }

    #[test]
    fn accepted_paths_shown_only_when_verbose() {
        // Given an accepted asset.
        let verdicts = vec![Verdict::Accepted(PathBuf::from("/p/a.glb"))];

        // When formatting quietly.
        let quiet = format_verdicts(&verdicts, false);
        // Then stdout has no ACCEPTED block.
        assert!(!quiet.stdout.contains("ACCEPTED"));

        // When formatting verbosely.
        let loud = format_verdicts(&verdicts, true);
        // Then stdout lists the accepted path.
        assert!(loud.stdout.contains("ACCEPTED (1):"));
        assert!(loud.stdout.contains("/p/a.glb"));
    }
}
