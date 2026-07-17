//! End-to-end coverage for the async pipeline (`run_pipeline`) over real
//! `temptree` filesystems. These complement `audit_pipeline.rs` (which drives
//! the sync `run_audit` kernel): here we exercise the tokio topology —
//! task-per-directory walk, bounded channels, single auditor + reporter — and
//! assert on the returned `Vec<Verdict>`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::needless_raw_string_hashes
)]

mod common;

use std::sync::Arc;

use auditah::audit::pipeline::run_pipeline;
use auditah::audit::report::{FindingCode, Verdict};
use auditah::config::Config;
use auditah::discovery::enumerator::ExcludeMatcher;
use auditah::registry::{LicenseRegistry, LicenseRegistryService, LicenseSpec};
use auditah::services::config::ConfigService;
use auditah::services::fs::FsService;
use auditah::services::{ClockService, RealClock, Services};
use auditah::test_support::FakeFs;
use std::path::PathBuf;
use temptree::temptree;

/// Build a real-`FsService`-backed [`Services`] rooted at `root` with `registry`.
fn services_with_registry(root: &std::path::Path, registry: LicenseRegistry) -> Arc<Services> {
    let config = Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    };
    Arc::new(
        Services::test()
            .fs(common::real_fs())
            .registry(LicenseRegistryService::new(Arc::new(registry)))
            .clock(ClockService::new(Arc::new(RealClock::new())))
            .config(ConfigService::new(Arc::from(root), Arc::new(config)))
            .build(),
    )
}

fn default_excludes() -> ExcludeMatcher {
    ExcludeMatcher::new(&auditah::discovery::all_excludes(&[])).unwrap()
}

/// Run the async pipeline against `root` with `--jobs N` and collect the
/// verdicts in arrival order. Registry + config default to an empty permissive
/// setup; callers seed the tree first.
async fn run_async(root: &std::path::Path, registry: LicenseRegistry, jobs: usize) -> Vec<Verdict> {
    let services = services_with_registry(root, registry);
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel::<()>(8);
    run_pipeline(services, default_excludes(), jobs, progress_tx)
        .await
        .expect("pipeline should not fail to drive")
}

/// Run the async pipeline while draining its progress channel. Returns the
/// verdicts plus the count of progress ticks observed — one tick per audited
/// asset proves progress is streamed per-asset (not buffered to the end).
async fn run_async_with_progress(
    root: &std::path::Path,
    registry: LicenseRegistry,
    jobs: usize,
) -> (Vec<Verdict>, usize) {
    let services = services_with_registry(root, registry);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<()>(64);
    let drive = tokio::spawn(run_pipeline(
        services,
        default_excludes(),
        jobs,
        progress_tx,
    ));
    let mut ticks = 0;
    while progress_rx.recv().await.is_some() {
        ticks += 1;
    }
    let verdicts = drive.await.expect("join").expect("drive ok");
    (verdicts, ticks)
}

/// Run the pipeline and return only the FAIL `FindingCode`s as sorted Debug
/// strings, for stable comparison regardless of task interleaving.
fn fail_codes(verdicts: &[Verdict]) -> Vec<String> {
    let mut codes: Vec<String> = verdicts
        .iter()
        .filter_map(|v| match v {
            Verdict::Failed(f) => Some(format!("{:?}", f.code)),
            _ => None,
        })
        .collect();
    codes.sort();
    codes
}

fn has_code(verdicts: &[Verdict], code: FindingCode) -> bool {
    verdicts
        .iter()
        .any(|v| matches!(v, Verdict::Failed(f) if f.code == code))
}

fn error_count(verdicts: &[Verdict]) -> usize {
    verdicts
        .iter()
        .filter(|v| matches!(v, Verdict::Error(..)))
        .count()
}

#[tokio::test]
async fn pipeline_accepts_clean_asset() {
    // Given a project with a covered asset and a resolvable license + text.
    let tree = temptree! {
        "hero.glb": "binary",
        "_manifest.toml": r#"
title = "Hero"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"#,
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Mit");
    let reg = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Mit"))
        .build();

    // When running the async pipeline.
    let verdicts = run_async(root, reg, 1).await;

    // Then the asset is Accepted (no failures).
    assert!(fail_codes(&verdicts).is_empty());
    assert!(verdicts.iter().any(|v| matches!(v, Verdict::Accepted(_))));
}

#[tokio::test]
async fn pipeline_flags_unlicensed_asset() {
    // Given a project with an uncovered asset (no manifest reaches it).
    let tree = temptree! {
        "sword.glb": "binary",
    };

    // When running the async pipeline.
    let verdicts = run_async(tree.path(), LicenseRegistry::empty(), 1).await;

    // Then it fails as UnlicensedAsset.
    assert!(has_code(&verdicts, FindingCode::UnlicensedAsset));
}

#[tokio::test]
async fn pipeline_detects_orphan_sidecar() {
    // Given a sidecar whose asset file is absent.
    let tree = temptree! {
        "ghost.glb.attr.toml": r#"
title = "Ghost"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"#,
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Mit");
    let reg = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Mit"))
        .build();

    // When running the async pipeline.
    let verdicts = run_async(root, reg, 1).await;

    // Then it fails as OrphanSidecar.
    assert!(has_code(&verdicts, FindingCode::OrphanSidecar));
}

#[tokio::test]
async fn pipeline_inherits_manifest_into_nested_directories() {
    // Given a root manifest and a nested asset with no local config.
    let tree = temptree! {
        "_manifest.toml": r#"
title = "Inherited"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"#,
        "sub": {
            "deep.glb": "binary",
        },
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Mit");
    let reg = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Mit"))
        .build();

    // When running the async pipeline.
    let verdicts = run_async(root, reg, 2).await;

    // Then the nested asset is Accepted via the inherited manifest.
    assert!(fail_codes(&verdicts).is_empty());
}

/// Fixture for case-6 tests: a root with two sibling subtrees.
/// `bad/` holds a malformed `_manifest.toml` plus an asset that should be
/// skipped; `good/` holds a clean, fully-covered asset. Both under a
/// resolvable license.
fn tree_with_bad_sibling() -> (tempfile::TempDir, LicenseRegistry) {
    let tree = temptree! {
        "good": {
            "_manifest.toml": r##"
title = "Good"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"##,
            "good.glb": "binary",
        },
        "bad": {
            "_manifest.toml": "this is not = valid toml {{{",
            "bad.glb": "binary",
        },
    };
    let root = tree.path();
    common::seed_license(root, "LicenseRef-Mit");
    let reg = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Mit"))
        .build();
    (tree, reg)
}

#[tokio::test]
async fn malformed_manifest_emits_error_verdict() {
    // Given a subtree with an unparseable _manifest.toml.
    let (tree, reg) = tree_with_bad_sibling();

    // When running the async pipeline.
    let verdicts = run_async(tree.path(), reg, 2).await;

    // Then exactly one Error verdict is emitted, for the bad directory.
    assert_eq!(error_count(&verdicts), 1);
    assert!(verdicts.iter().any(|v| matches!
        (v, Verdict::Error(p, _) if p.ends_with("bad"))));
}

#[tokio::test]
async fn malformed_manifest_skips_its_subtree() {
    // Given a bad subtree whose child asset should not be audited.
    let (tree, reg) = tree_with_bad_sibling();

    // When running the async pipeline.
    let verdicts = run_async(tree.path(), reg, 2).await;

    // Then the bad asset (bad/bad.glb) produces no verdict at all —
    // neither Accepted nor Failed — because its subtree was skipped.
    let touched_bad_asset = verdicts.iter().any(|v| match v {
        Verdict::Accepted(p) => p.ends_with("bad.glb"),
        Verdict::Failed(f) => f.asset.ends_with("bad.glb"),
        Verdict::Error(_, _) => false,
    });
    assert!(!touched_bad_asset);
}

#[tokio::test]
async fn malformed_manifest_does_not_block_sibling_subtree() {
    // Given a bad subtree alongside a clean sibling.
    let (tree, reg) = tree_with_bad_sibling();

    // When running the async pipeline.
    let verdicts = run_async(tree.path(), reg, 2).await;

    // Then the good sibling's asset is still Accepted.
    assert!(verdicts.iter().any(|v| matches!
        (v, Verdict::Accepted(p) if p.ends_with("good.glb"))));
}

#[tokio::test]
async fn findings_are_identical_across_job_counts() {
    // Given a tree with several assets across multiple directories, each
    // producing a distinct finding.
    let build = || {
        let tree = temptree! {
            "_manifest.toml": "not toml {{{",
            "a": { "a.glb": "binary" },
            "b": {
                "b.glb": "binary",
                "b.glb.attr.toml": r##"
title = "B"
author = "Artist"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com"
"##,
            },
            "c": { "ghost.glb.attr.toml": "" },
        };
        common::seed_license(tree.path(), "LicenseRef-Mit");
        tree
    };
    let reg = LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Mit"))
        .build();

    // When running the pipeline serially vs highly parallel.
    let serial = run_async(build().path(), reg.clone(), 1).await;
    let parallel = run_async(build().path(), reg, 8).await;

    // Then the multiset of findings is identical regardless of job count.
    assert_eq!(fail_codes(&serial), fail_codes(&parallel));
}

#[tokio::test]
async fn jobs_one_completes_without_deadlock_on_deep_tree() {
    // Given a deep, wide tree that exercises recursive task spawning.
    let tree = temptree! {
        "a": {
            "b": {
                "c": { "leaf.glb": "binary" },
                "d": { "leaf.glb": "binary" },
            },
            "e": { "leaf.glb": "binary" },
        },
        "f": { "leaf.glb": "binary" },
    };

    // When running the pipeline fully serially (--jobs 1).
    let verdicts = run_async(tree.path(), LicenseRegistry::empty(), 1).await;

    // Then it completes (no deadlock) and every leaf is flagged unlicensed.
    // A hang here means the permit scoping around child spawns is broken.
    let unlicensed = verdicts
        .iter()
        .filter(|v| matches!(v, Verdict::Failed(f) if f.code == FindingCode::UnlicensedAsset))
        .count();
    assert_eq!(unlicensed, 4);
}

#[tokio::test]
async fn large_tree_does_not_deadlock_or_oom_with_serial_jobs() {
    // Given a synthetic tree of many directories, each with an unlicensed asset.
    // If the bounded channels did not provide backpressure (auditor draining
    // while the walker produces), the walker would block on a full channel at
    // jobs=1 (tiny buffer) and this would hang or balloon in memory.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    for i in 0..150 {
        let sub = root.join(format!("d{i}"));
        std::fs::create_dir_all(&sub).expect("mkdir");
        std::fs::write(sub.join("asset.glb"), "binary").expect("write");
    }

    // When running the pipeline fully serially over the large tree.
    let verdicts = run_async(&root, LicenseRegistry::empty(), 1).await;

    // Then it completes and every asset is accounted for as unlicensed.
    let unlicensed = verdicts
        .iter()
        .filter(|v| matches!(v, Verdict::Failed(f) if f.code == FindingCode::UnlicensedAsset))
        .count();
    assert_eq!(unlicensed, 150);
}

#[tokio::test]
async fn child_subtree_failure_surfaces_as_error_not_lost() {
    // Given a FakeFs with a subdir whose list_dir is faulted: the child
    // walk_dir returns Err. The contract under test is that *any* child
    // failure (returned Err or panic) surfaces as a Verdict::Error instead of
    // silently dropping the subtree — the same handle.await failure path.
    use auditah::services::config::ConfigService;
    use auditah::services::fs::FsService;
    use auditah::test_support::FakeFs;
    use std::path::Path;

    let fs = FsService::new(Arc::new(
        FakeFs::with_files([
            (Path::new("/proj/good.glb"), "binary"),
            (Path::new("/proj/sub/bad.glb"), "binary"),
        ])
        .fail_list_dir(Path::new("/proj/sub")),
    ));
    let config = Arc::new(Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    });
    let services = Arc::new(
        Services::test()
            .fs(fs)
            .registry(LicenseRegistryService::new(Arc::new(
                LicenseRegistry::empty(),
            )))
            .config(ConfigService::new(Arc::from(Path::new("/proj")), config))
            .build(),
    );
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel::<()>(8);

    // When running the pipeline.
    let verdicts = run_pipeline(services, default_excludes(), 1, progress_tx)
        .await
        .expect("pipeline should drive");

    // Then the failed subtree surfaces as an Error verdict (not lost), and the
    // sibling good.glb is still audited.
    assert!(verdicts.iter().any(|v| matches!(v, Verdict::Error(..))));
    assert!(verdicts
        .iter()
        .any(|v| matches!(v, Verdict::Failed(f) if f.code == FindingCode::UnlicensedAsset)));
}

#[tokio::test]
async fn progress_channel_emits_one_tick_per_asset() {
    // Given a tree with several unlicensed assets spread across directories.
    let tree = temptree! {
        "a": { "a.glb": "binary" },
        "b": { "b.glb": "binary", "c.glb": "binary" },
    };

    // When running the pipeline while draining the progress channel.
    let (verdicts, ticks) = run_async_with_progress(tree.path(), LicenseRegistry::empty(), 2).await;

    // Then exactly one progress tick arrives per audited asset (3 assets = 3
    // ticks), proving progress streams during the run rather than being
    // buffered and dumped at the end.
    assert_eq!(ticks, 3);
    assert_eq!(verdicts.len(), 3);
}

/// Run the pipeline over a `FakeFs` whose `list_dir` is slowed by `delay_ms`
/// so concurrent directory descents overlap. Returns the high-water mark of
/// simultaneous in-flight `list_dir` calls — the observed concurrency.
///
/// This proves `--jobs` actually caps concurrent directory descents, not
/// merely that the run avoids deadlock.
async fn run_with_concurrency_probe(fs_backend: Arc<FakeFs>, jobs: usize) -> usize {
    let config = Arc::new(Config {
        commercial_project: false,
        redistributes_assets: false,
        manual_review_acknowledged: Vec::new(),
        exclude: Vec::new(),
    });
    let services = Arc::new(
        Services::test()
            .fs(FsService::new(fs_backend.clone()))
            .registry(LicenseRegistryService::new(Arc::new(
                LicenseRegistry::empty(),
            )))
            .config(ConfigService::new(
                Arc::from(PathBuf::from("/proj")),
                config,
            ))
            .build(),
    );
    let (progress_tx, _progress_rx) = tokio::sync::mpsc::channel::<()>(64);
    run_pipeline(services, default_excludes(), jobs, progress_tx)
        .await
        .expect("pipeline should drive to completion");
    fs_backend.list_dir_high_water()
}

#[tokio::test]
async fn jobs_caps_concurrent_directory_descents() {
    // Given a wide tree: a root with 8 sibling subdirectories.
    let fs = Arc::new(
        FakeFs::with_files([
            ("/proj/sub0/a.glb", "x"),
            ("/proj/sub1/a.glb", "x"),
            ("/proj/sub2/a.glb", "x"),
            ("/proj/sub3/a.glb", "x"),
            ("/proj/sub4/a.glb", "x"),
            ("/proj/sub5/a.glb", "x"),
            ("/proj/sub6/a.glb", "x"),
            ("/proj/sub7/a.glb", "x"),
        ])
        // 40ms per list_dir so 8 concurrent descents reliably overlap.
        .with_list_dir_delay_ms(40),
    );

    // When running with --jobs 3.
    let high_water = run_with_concurrency_probe(fs, 3).await;

    // Then the observed concurrency never exceeds the cap (3).
    assert!(
        high_water <= 3,
        "--jobs 3 should cap concurrency, but observed {high_water} in-flight list_dir calls"
    );
}

#[tokio::test]
async fn jobs_one_runs_subdirectories_serially() {
    // Given a wide tree: a root with 4 sibling subdirectories.
    let fs = Arc::new(
        FakeFs::with_files([
            ("/proj/sub0/a.glb", "x"),
            ("/proj/sub1/a.glb", "x"),
            ("/proj/sub2/a.glb", "x"),
            ("/proj/sub3/a.glb", "x"),
        ])
        .with_list_dir_delay_ms(40),
    );

    // When running with --jobs 1 (fully serial).
    let high_water = run_with_concurrency_probe(fs, 1).await;

    // Then at most one directory is ever listed at a time.
    assert_eq!(
        high_water, 1,
        "--jobs 1 must run serially, but observed {high_water} in-flight list_dir calls"
    );
}
