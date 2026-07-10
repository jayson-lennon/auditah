//! Integration tests: the `credits` pipeline end-to-end against a real temp fs.
//! Asserts on the generated CREDITS.md content (the public contract).

use auditah::config::Config;
use auditah::credits::{generate_credits, CreditsCtx};
use auditah::services::Services;
use temptree::temptree;

mod common;
use common::{non_commercial_config, services};


/// Generate credits to `<root>/CREDITS.md` and return the file contents.
fn generated(ctx: &CreditsCtx) -> String {
    let root = ctx.root;
    let out = root.join("CREDITS.md");
    generate_credits(ctx, &out).expect("credits generation should succeed");
    std::fs::read_to_string(&out).expect("CREDITS.md should be readable")
}

fn ctx<'a>(svc: &'a Services, cfg: &'a Config, root: &'a std::path::Path) -> CreditsCtx<'a> {
    CreditsCtx {
        services: svc,
        config: cfg,
        root,
    }
}

// CC0 (attribution-free) assets are omitted from credits entirely.
#[test]
fn cc0_assets_are_omitted_from_credits() {
    let tree = temptree! {
        "rock.glb": "binary",
        "rock.glb.attr.toml": r#"
title = "Rock"
author = "Quaternius"
year = 2022
license = "CC0-1.0"
source = "https://poly.pizza"
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let content = generated(&ctx(&svc, &cfg, root));
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
    let tree = temptree! {
        "a.glb": "binary",
        "a.glb.attr.toml": r#"
title = "Alpha"
author = "Oliver Herklotz"
year = 2019
license = "CC-BY-3.0"
source = "https://example.com/a"
"#,
        "b.glb": "binary",
        "b.glb.attr.toml": r#"
title = "Beta"
author = "Quaternius"
year = 2022
license = "CC-BY-3.0"
source = "https://example.com/b"
"#,
        "c.glb": "binary",
        "c.glb.attr.toml": r#"
title = "Gamma"
author = "Oliver Herklotz"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com/c"
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let content = generated(&ctx(&svc, &cfg, root));

    // Both author headers present.
    assert!(content.contains("## Oliver Herklotz"));
    assert!(content.contains("## Quaternius"));
    // Alpha appears before Gamma (title sort) within Oliver's group.
    let alpha = content.find("**Alpha**").expect("Alpha missing");
    let gamma = content.find("**Gamma**").expect("Gamma missing");
    assert!(
        alpha < gamma,
        "entries not sorted by title: Alpha should precede Gamma"
    );
}

// Modification notice appears only when requires_modification_notice + modified.
#[test]
fn modification_notice_emitted_only_when_required_and_modified() {
    let tree = temptree! {
        // modified + requires_modification_notice (via override) → notice present
        "mod1.glb": "binary",
        "mod1.glb.attr.toml": r#"
title = "Mod1"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"
modified = true

[overrides]
requires_modification_notice = true
"#,
        // modified but license does NOT require notice → no notice
        "mod2.glb": "binary",
        "mod2.glb.attr.toml": r#"
title = "Mod2"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"
modified = true

[overrides]
requires_modification_notice = false
"#,
        // requires notice but NOT modified → no notice
        "mod3.glb": "binary",
        "mod3.glb.attr.toml": r#"
title = "Mod3"
author = "A"
year = 2020
license = "CC-BY-3.0"
source = "https://example.com"
modified = false

[overrides]
requires_modification_notice = true
"#
    };
    let root = tree.path();
    let svc = services();
    let cfg = non_commercial_config();
    let content = generated(&ctx(&svc, &cfg, root));

    // Find each entry's line and check for the notice on it.
    for (title, expect_notice) in [("Mod1", true), ("Mod2", false), ("Mod3", false)] {
        let line = content
            .lines()
            .find(|l| l.contains(&format!("**{title}**")))
            .unwrap_or_else(|| panic!("{title} entry missing from:\n{content}"));
        let has_notice = line.contains("(modified from original)");
        assert_eq!(
            has_notice, expect_notice,
            "{title}: notice was {has_notice}, expected {expect_notice}\nline: {line}"
        );
    }
}
