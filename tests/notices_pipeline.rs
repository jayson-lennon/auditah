//! Integration tests: the `notices` generator directly (via the public API).
//! The CLI-level flow is covered by `generate_pipeline.rs`; these tests verify
//! `generate_notices` in isolation — the audit-gate is the caller's job.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::model::terms::LicenseTerms;
use auditah::notices::generate_notices;
use auditah::registry::{LicenseRegistry, LicenseSpec};
use auditah::test_support::ServicesTestBuilder;

use temptree::temptree;

mod common;
use common::{config, real_fs, services_with};

const SIDECAR_A: &str = r#"
title = "Alpha"
author = "Quaternius"
year = 2022
license = "LicenseRef-Notice"
source = "https://example.com"
"#;

#[test]
fn notices_dedupes_multiple_assets_sharing_one_license() {
    // Given two assets covered by the same notice-required license.
    let tree = temptree! {
        "a.glb": "",
        "a.glb.attr.toml": SIDECAR_A,
        "b.glb": "",
        "b.glb.attr.toml": SIDECAR_A,
    };
    let root = tree.path();
    let svc = services_with(
        root,
        config(),
        [LicenseSpec::new("LicenseRef-Notice").terms(LicenseTerms {
            requires_license_notice: true,
            ..LicenseTerms::permissive()
        })],
    );
    common::seed_license_text(root, &["LicenseRef-Notice"]);
    let out = root.join("NOTICES.md");

    // When generating NOTICES.
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then the license section appears exactly once (deduped).
    let count = notices.matches("## LicenseRef-Notice").count();
    assert_eq!(count, 1, "expected 1 section, got {count}:\n{notices}");
}

#[test]
fn notices_omits_licenses_without_notice_requirement() {
    // Given a CC0-like (no notice) asset.
    let tree = temptree! {
        "a.glb": "",
        "a.glb.attr.toml": r#"
title = "A"
author = "X"
year = 2024
license = "LicenseRef-CC0Like"
source = "https://x"
"#,
    };
    let root = tree.path();
    let svc = services_with(root, config(), [LicenseSpec::new("LicenseRef-CC0Like")]);
    common::seed_license_text(root, &["LicenseRef-CC0Like"]);
    let out = root.join("NOTICES.md");

    // When generating NOTICES.
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then no license section appears (placeholder only).
    assert!(
        notices.contains("_No license-notice-required assets found._"),
        "CC0-only project should have empty notices:\n{notices}"
    );
}

#[test]
fn notices_reproduces_text_for_notice_required_license() {
    // Given a notice-required asset.
    let tree = temptree! {
        "a.glb": "",
        "a.glb.attr.toml": SIDECAR_A,
    };
    let root = tree.path();
    let svc = services_with(
        root,
        config(),
        [LicenseSpec::new("LicenseRef-Notice").terms(LicenseTerms {
            requires_license_notice: true,
            ..LicenseTerms::permissive()
        })],
    );
    common::seed_license_text(root, &["LicenseRef-Notice"]);
    let out = root.join("NOTICES.md");

    // When generating NOTICES.
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then the license text body appears under the header.
    assert!(
        notices.contains("license body"),
        "text should appear:\n{notices}"
    );
    assert!(
        notices.contains("## LicenseRef-Notice"),
        "header should appear:\n{notices}"
    );
}

#[test]
fn notices_empty_project_shows_placeholder() {
    // Given an empty project (no assets).
    let tree = temptree! {
        "auditah.toml": "",
    };
    let root = tree.path();
    let svc = services_with(root, config(), []);
    let out = root.join("NOTICES.md");

    // When generating NOTICES.
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then the placeholder appears.
    assert!(
        notices.contains("_No license-notice-required assets found._"),
        "empty project should have placeholder:\n{notices}"
    );
}

#[test]
fn notices_multiple_notice_licenses_each_get_section() {
    // Given two distinct notice-required licenses.
    let tree = temptree! {
        "a.glb": "",
        "a.glb.attr.toml": r#"
title = "A"
author = "X"
year = 2024
license = "LicenseRef-MITLike"
source = "https://x"
"#,
        "b.glb": "",
        "b.glb.attr.toml": r#"
title = "B"
author = "Y"
year = 2024
license = "LicenseRef-BSDLike"
source = "https://y"
"#,
    };
    let root = tree.path();
    let svc = services_with(
        root,
        config(),
        [
            LicenseSpec::new("LicenseRef-MITLike").terms(LicenseTerms {
                requires_license_notice: true,
                ..LicenseTerms::permissive()
            }),
            LicenseSpec::new("LicenseRef-BSDLike").terms(LicenseTerms {
                requires_license_notice: true,
                ..LicenseTerms::permissive()
            }),
        ],
    );
    common::seed_license_text(root, &["LicenseRef-MITLike", "LicenseRef-BSDLike"]);
    let out = root.join("NOTICES.md");

    // When generating NOTICES.
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then both sections appear.
    assert!(notices.contains("## LicenseRef-MITLike"));
    assert!(notices.contains("## LicenseRef-BSDLike"));
}

#[test]
fn notices_reads_text_from_disk_via_real_services() {
    // Given a real on-disk license (not in-memory services).
    let tree = temptree! {
        "a.glb": "",
        "a.glb.attr.toml": SIDECAR_A,
    };
    let root = tree.path();
    LicenseRegistry::builder()
        .license(LicenseSpec::new("LicenseRef-Notice").terms(LicenseTerms {
            requires_license_notice: true,
            ..LicenseTerms::permissive()
        }))
        .commit(root, &real_fs())
        .expect("commit");
    common::seed_license_text(root, &["LicenseRef-Notice"]);

    let svc = ServicesTestBuilder::load_from_disk(root)
        .expect("load real services from disk")
        .build();
    let out = root.join("NOTICES.md");
    generate_notices(&svc, &out).expect("notices generation");
    let notices = std::fs::read_to_string(&out).expect("NOTICES readable");

    // Then the on-disk text was read and reproduced.
    assert!(
        notices.contains("license body"),
        "text should appear:\n{notices}"
    );
}
