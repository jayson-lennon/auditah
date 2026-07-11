//! Integration tests: the `credits` pipeline end-to-end against a real temp fs.
//! Asserts on the generated CREDITS.md content (the public contract).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use auditah::config::Config;
use auditah::credits::{generate_credits, CreditsCtx};
use auditah::services::Services;
use temptree::temptree;

mod common;
use auditah::model::terms::LicenseTerms;
use auditah::registry::LicenseSpec;
use common::{non_commercial_config, permissive_terms, services_with};

/// Generate credits to `<root>/CREDITS.md` and return the file contents.
fn generated(ctx: &CreditsCtx) -> String {
    let root = ctx.root;
    let out = root.join("CREDITS.md");
    generate_credits(ctx, &out).expect("credits generation should succeed");
    std::fs::read_to_string(&out).expect("CREDITS.md should be readable")
}

fn ctx<'a>(svc: &'a Services, cfg: &'a Config, root: &'a Path) -> CreditsCtx<'a> {
    CreditsCtx {
        services: svc,
        config: cfg,
        root,
    }
}

// CC0 (attribution-free) assets are omitted from credits entirely.
#[test]
fn cc0_assets_are_omitted_from_credits() {
    // Given a CC0 asset.
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": r#"
title = "Rock"
author = "Quaternius"
year = 2022
license = "LicenseRef-Cc0"
source = "https://poly.pizza"
"#
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-Cc0").terms(permissive_terms())]);
    let cfg = non_commercial_config();

    // When generating credits.
    let content = generated(&ctx(&svc, &cfg, root));

    // Then the credits note no attribution-required assets and the CC0 asset is omitted.
    assert!(
        content.contains("_No attribution-required assets found._"),
        "CC0 should produce empty credits; got:\n{content}"
    );
    assert!(
        !content.contains("Rock"),
        "CC0 asset leaked into credits:\n{content}"
    );
}

// CC-BY assets (attribution-required) produce entries grouped by author,
// sorted by title within each author group.
#[test]
fn cc_by_assets_grouped_by_author() {
    // Given three CC-BY assets from two authors.
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": r#"
title = "Alpha"
author = "Oliver Herklotz"
year = 2019
license = "LicenseRef-CcBy"
source = "https://example.com/a"
"#,
        "b.glb": "binary",
        "b.glb.attr.toml": r#"
title = "Beta"
author = "Quaternius"
year = 2022
license = "LicenseRef-CcBy"
source = "https://example.com/b"
"#,
        "c.glb": "binary",
        "c.glb.attr.toml": r#"
title = "Gamma"
author = "Oliver Herklotz"
year = 2020
license = "LicenseRef-CcBy"
source = "https://example.com/c"
"#
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-CcBy").terms(LicenseTerms {
        requires_attribution: true,
        ..permissive_terms()
    })]);
    let cfg = non_commercial_config();

    // When generating credits.
    let content = generated(&ctx(&svc, &cfg, root));

    // Then both author headers are present and entries are title-sorted within each group.
    assert!(content.contains("## Oliver Herklotz"));
    assert!(content.contains("## Quaternius"));
    let alpha = content.find("**Alpha**").expect("Alpha missing");
    let gamma = content.find("**Gamma**").expect("Gamma missing");
    assert!(
        alpha < gamma,
        "entries not sorted by title: Alpha should precede Gamma"
    );
}

/// Build a credits output for a single CC-BY asset with the given title,
/// `modified` flag, and `requires_modification_notice` override. Reads inside so
/// the temptree outlives the read.
fn credits_for_asset(title: &str, modified: bool, mod_notice_override: bool) -> String {
    let sidecar = format!(
        r#"
title = "{title}"
author = "A"
year = 2020
license = "LicenseRef-CcBy"
source = "https://example.com"
modified = {modified}

[overrides]
requires_modification_notice = {mod_notice_override}
"#
    );
    let tree = temptree! {
        "asset.glb": "binary",
        "asset.glb.attr.toml": sidecar,
    };
    let root = tree.path();
    let svc = services_with([LicenseSpec::new("LicenseRef-CcBy").terms(LicenseTerms {
        requires_attribution: true,
        ..permissive_terms()
    })]);
    let cfg = non_commercial_config();
    generated(&ctx(&svc, &cfg, root))
}

#[test]
fn modification_notice_present_when_modified_and_required() {
    // Given a modified asset whose license requires a modification notice.
    // When generating credits.
    let content = credits_for_asset("Mod1", true, true);

    // Then the entry carries the "(modified from original)" notice.
    let line = content
        .lines()
        .find(|l| l.contains("**Mod1**"))
        .unwrap_or_else(|| panic!("Mod1 entry missing from:\n{content}"));
    assert!(
        line.contains("(modified from original)"),
        "expected modification notice; line: {line}"
    );
}

#[test]
fn modification_notice_absent_when_modified_but_not_required() {
    // Given a modified asset whose license does NOT require a notice.
    // When generating credits.
    let content = credits_for_asset("Mod2", true, false);

    // Then the entry has no modification notice.
    let line = content
        .lines()
        .find(|l| l.contains("**Mod2**"))
        .unwrap_or_else(|| panic!("Mod2 entry missing from:\n{content}"));
    assert!(
        !line.contains("(modified from original)"),
        "expected no modification notice; line: {line}"
    );
}

#[test]
fn modification_notice_absent_when_required_but_not_modified() {
    // Given an unmodified asset whose license requires a notice.
    // When generating credits.
    let content = credits_for_asset("Mod3", false, true);

    // Then the entry has no modification notice.
    let line = content
        .lines()
        .find(|l| l.contains("**Mod3**"))
        .unwrap_or_else(|| panic!("Mod3 entry missing from:\n{content}"));
    assert!(
        !line.contains("(modified from original)"),
        "expected no modification notice; line: {line}"
    );
}
