//! Async audit pipeline: the tokio executor over the sync cascade + check kernel.
//!
//! Topology: one recursive `walk_dir` task per directory (capped by a
//! `Semaphore(--jobs)`) fans `AuditInput`s over a bounded channel to a single
//! auditor task, which runs `audit_asset` and forwards `Verdict`s over a second
//! bounded channel to a single reporter task. This is the only module that
//! depends on tokio; the kernel (`cascade::descend`, `audit_asset`) stays sync.
//!
//! Shutdown is driven by sender drops: when every `walk_dir` finishes, all
//! `tx_in` clones drop → the auditor's `recv()` returns `None` → it drops
//! `tx_out` → the reporter's `recv()` returns `None` → it flushes.

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use error_stack::{Report, ResultExt};
use std::future::Future;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinError;
use wherror::Error;

use crate::audit::cascade::{descend, AuditInput, DirResult, Inherited};
use crate::audit::report::Verdict;
use crate::audit::{audit_asset, orphan_verdict, AuditCtx};
use crate::config::Config;
use crate::discovery::enumerator::ExcludeMatcher;
use crate::services::{FsService, Services};

/// Technical failure surfaced from the async driver itself (not a compliance
/// finding). Distinct from `AuditError` so task panics / join failures aren't
/// conflated with kernel I/O errors; both are surfaced as errors-last.
#[derive(Debug, Error)]
#[error(debug)]
pub struct PipelineError;

/// One unit of work the FS worker sends to the auditor. Carries the resolved
/// record (or orphan path) plus the manifest path to attribute findings to.
#[derive(Debug, Clone)]
enum FsMessage {
    /// A resolved asset awaiting obligation checks.
    Asset(AuditInput),
    /// A directory that could not be descended (list/manifest failure).
    DirError(PathBuf, String),
}

/// Run the async pipeline and return the accumulated verdicts in arrival order.
///
/// `jobs` caps concurrent directory descents; `progress` receives a tick per
/// audited asset so the caller can stream progress to stderr. The returned
/// `Vec<Verdict>` is consumed by the reporter/CLI for output.
///
/// # Errors
///
/// Returns `PipelineError` if the driver itself fails to spawn/join tasks
/// (surfaces panics). Kernel I/O errors arrive as `Verdict::Error` entries,
/// not as `Err`.
pub async fn run_pipeline(
    services: Arc<Services>,
    config: Arc<Config>,
    root: PathBuf,
    excludes: ExcludeMatcher,
    jobs: usize,
    progress: mpsc::Sender<()>,
) -> Result<Vec<Verdict>, Report<PipelineError>> {
    // Bounded channels: backpressure throughout, bounded memory on huge trees.
    let (tx_in, rx_in) = mpsc::channel::<FsMessage>(jobs.max(1) * 4);
    let (tx_out, rx_out) = mpsc::channel::<Verdict>(jobs.max(1) * 4);

    let sem = Arc::new(Semaphore::new(jobs.max(1)));

    // Spawn the recursive FS walker as a single root task. It fans out child
    // tasks internally; when it (and all children) finish, the last `tx_in`
    // clone drops, closing the channel.
    let walker = tokio::spawn(walk_dir(
        services.fs.clone(),
        root.clone(),
        root.clone(),
        excludes,
        None,
        tx_in.clone(),
        sem.clone(),
    ));
    // Drop the driver's own sender so the channel closes only when `walker`
    // (and every transitive child) is done.
    drop(tx_in);

    // Single auditor task: owns Arc-shared services+config, runs the check
    // kernel, forwards verdicts to the reporter.
    let auditor = tokio::spawn(auditor_task(
        services,
        config,
        root.clone(),
        rx_in,
        tx_out,
        progress,
    ));

    // Single reporter task: consumes verdicts, accumulates in arrival order.
    let reporter = tokio::spawn(reporter_task(rx_out));

    // Await the FS tree first; then the auditor; then the reporter.
    let mut errors: Vec<(PathBuf, String)> = Vec::new();

    match walker.await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            errors.push((root.clone(), format!("{e:?}")));
        }
        Err(join) => errors.push((root.clone(), join_error_string(&join))),
    }

    if let Err(join) = auditor.await {
        errors.push((root.clone(), join_error_string(&join)));
    }

    let mut verdicts = match reporter.await {
        Ok(v) => v,
        Err(join) => {
            errors.push((root.clone(), join_error_string(&join)));
            Vec::new()
        }
    };

    // Surface any driver-level errors last (errors-dead-last invariant).
    for (path, detail) in errors {
        verdicts.push(Verdict::Error(path, detail));
    }

    Ok(verdicts)
}

/// Recursive per-directory walker. Lists one directory (via `spawn_blocking`
/// over the sync `descend`), sends its assets/orphans/dir-errors downstream,
/// then spawns + awaits child walkers for each subdir.
///
/// The semaphore permit is acquired **only** around the directory's own I/O
/// and released before spawning/awaiting children — the #1 deadlock risk at
/// `--jobs 1`. Holding a permit across child `await` would deadlock when a
/// child needs a permit the parent is squatting on.
fn walk_dir(
    fs: FsService,
    dir: PathBuf,
    root: PathBuf,
    excludes: ExcludeMatcher,
    inherited: Option<Inherited>,
    tx_in: mpsc::Sender<FsMessage>,
    sem: Arc<Semaphore>,
) -> Pin<Box<dyn Future<Output = Result<(), Report<PipelineError>>> + Send>> {
    Box::pin(walk_dir_inner(
        fs, dir, root, excludes, inherited, tx_in, sem,
    ))
}

async fn walk_dir_inner(
    fs: FsService,
    dir: PathBuf,
    root: PathBuf,
    excludes: ExcludeMatcher,
    inherited: Option<Inherited>,
    tx_in: mpsc::Sender<FsMessage>,
    sem: Arc<Semaphore>,
) -> Result<(), Report<PipelineError>> {
    // 1. Acquire permit, run blocking I/O, drop permit — all before any await
    //    on children. `descend` is sync (`std::fs`/walkdir), so it must run in
    //    `spawn_blocking` to avoid stalling the runtime thread.
    let dir_result = {
        let _permit = sem
            .acquire()
            .await
            .change_context(PipelineError)
            .attach("semaphore closed")?;
        let fs_clone = fs.clone();
        let excludes_clone = excludes.clone();
        let inherited_clone = inherited.clone();
        let dir_clone = dir.clone();
        let root_clone = root.clone();
        tokio::task::spawn_blocking(move || {
            descend(
                &fs_clone,
                &dir_clone,
                &root_clone,
                &excludes_clone,
                inherited_clone,
            )
        })
        .await
        .change_context(PipelineError)
        .attach("descend task panicked")?
    };

    let DirResult {
        assets,
        orphans,
        effective,
        subdirs,
    } = match dir_result {
        Ok(r) => r,
        Err(e) => {
            // Could not descend this directory: surface a dir error, do not
            // descend the subtree (the manifest read/parse failure explains
            // why). Siblings continue.
            let _ = tx_in
                .send(FsMessage::DirError(dir.clone(), format!("{e:?}")))
                .await;
            return Ok(());
        }
    };

    // 2. Send this directory's audit inputs downstream.
    for asset in assets {
        if tx_in
            .send(FsMessage::Asset(AuditInput::Asset(asset)))
            .await
            .is_err()
        {
            // Receiver gone (downstream task panicked); stop sending.
            return Ok(());
        }
    }
    for orphan in orphans {
        if tx_in
            .send(FsMessage::Asset(AuditInput::Orphan(orphan)))
            .await
            .is_err()
        {
            return Ok(());
        }
    }

    // 3. Spawn + await each child walker. The permit is already released, so
    //    children can acquire their own without deadlocking.
    let mut handles = Vec::with_capacity(subdirs.len());
    for subdir in subdirs {
        let handle = tokio::spawn(walk_dir(
            fs.clone(),
            subdir,
            root.clone(),
            excludes.clone(),
            effective.clone(),
            tx_in.clone(),
            sem.clone(),
        ));
        handles.push(handle);
    }

    // Await every child; a panic or error in one is surfaced as a dir error
    // but does not abort the others — never lose a subtree.
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                let _ = tx_in
                    .send(FsMessage::DirError(root.clone(), format!("{e:?}")))
                    .await;
            }
            Err(join) => {
                let _ = tx_in
                    .send(FsMessage::DirError(root.clone(), join_error_string(&join)))
                    .await;
            }
        }
    }

    Ok(())
}

/// The auditor: runs the pure check kernel per asset, maps orphans and dir
/// errors to verdicts, forwards every verdict to the reporter. Emits one
/// progress tick per asset so the CLI can stream progress to stderr.
async fn auditor_task(
    services: Arc<Services>,
    config: Arc<Config>,
    root: PathBuf,
    mut rx_in: mpsc::Receiver<FsMessage>,
    tx_out: mpsc::Sender<Verdict>,
    progress: mpsc::Sender<()>,
) {
    let ctx = AuditCtx {
        services: &services,
        config: &config,
        root: &root,
    };

    while let Some(msg) = rx_in.recv().await {
        let inputs = match msg {
            FsMessage::Asset(input) => vec![input],
            FsMessage::DirError(path, detail) => {
                if tx_out.send(Verdict::Error(path, detail)).await.is_err() {
                    break;
                }
                continue;
            }
        };

        for input in inputs {
            let verdicts = match input {
                AuditInput::Asset(resolved) => {
                    let _ = progress.try_send(());
                    audit_asset(&resolved, &ctx)
                }
                AuditInput::Orphan(path) => {
                    let _ = progress.try_send(());
                    vec![orphan_verdict(&path)]
                }
            };
            for verdict in verdicts {
                if tx_out.send(verdict).await.is_err() {
                    return;
                }
            }
        }
    }
    // rx_in closed → all walkers done → dropping tx_out signals the reporter.
}

/// The reporter: consumes verdicts in arrival order and returns the
/// accumulated `Vec<Verdict>`. Sorting/bucketing happens in the CLI layer
/// (observable output is a CLI concern, not a kernel concern).
async fn reporter_task(mut rx_out: mpsc::Receiver<Verdict>) -> Vec<Verdict> {
    let mut collected = Vec::new();
    while let Some(verdict) = rx_out.recv().await {
        collected.push(verdict);
    }
    collected
}

/// Format a `JoinError` (panic or cancel) as a human-readable string so a
/// lost task is never silently swallowed — it becomes a `Verdict::Error`.
fn join_error_string(join: &JoinError) -> String {
    if join.is_panic() {
        format!("worker task panicked: {join}")
    } else {
        format!("worker task was cancelled: {join}")
    }
}
