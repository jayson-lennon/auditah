//! Integration tests: discovery + resolution precedence against a real
//! temp filesystem. These exercise `walkdir`/`globset` behavior that the
//! in-memory unit-test fakes do not.

use auditah::discovery::enumerator::{enumerate, ExcludeMatcher};
use auditah::discovery::resolver::{resolve, ResolutionSource, MANIFEST_FILENAME, SIDECAR_SUFFIX};
use temptree::temptree;

mod common;
use common::{default_excludes, real_fs};

#[test]
fn sidecar_wins_over_manifest_in_same_dir() {
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": r#"
title = "Sword"
author = "Smith"
year = 2020
license = "MIT"
source = "https://example.com/sword"
"#,
        "manifest.toml": r#"
title = "Pack"
author = "Quaternius"
year = 2022
license = "CC0-1.0"
source = "https://example.com/pack"
"#,
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sword.glb");
    let r = resolve(&fs, &asset, root).unwrap();
    assert!(
        matches!(r.source, ResolutionSource::Sidecar(_)),
        "sidecar should win"
    );
    assert_eq!(r.record.unwrap().license, "MIT");
}

#[test]
fn subdir_manifest_overrides_parent_manifest() {
    let tree = temptree! {
        "manifest.toml": r#"
title = "Parent"
author = "P"
year = 2020
license = "CC0-1.0"
source = "https://example.com"
"#,
        "sub": {
            "manifest.toml": r#"
title = "Child"
author = "C"
year = 2021
license = "MIT"
source = "https://example.com/child"
"#,
            "rock.glb": "binary",
        }
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sub").join("rock.glb");
    let r = resolve(&fs, &asset, root).unwrap();
    assert!(matches!(r.source, ResolutionSource::Manifest(_)));
    assert_eq!(
        r.record.unwrap().license,
        "MIT",
        "subdir manifest should win"
    );
}

#[test]
fn parent_manifest_is_fallback_when_no_subdir_config() {
    let tree = temptree! {
        "manifest.toml": r#"
title = "Parent"
author = "P"
year = 2020
license = "CC0-1.0"
source = "https://example.com"
"#,
        "sub": {
            "leaf.glb": "binary"
        }
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sub").join("leaf.glb");
    let r = resolve(&fs, &asset, root).unwrap();
    assert!(matches!(r.source, ResolutionSource::Manifest(_)));
    assert_eq!(r.record.unwrap().license, "CC0-1.0");
}

#[test]
fn uncovered_when_no_config_anywhere() {
    let tree = temptree! {
        "orphan.glb": "binary"
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("orphan.glb");
    let r = resolve(&fs, &asset, root).unwrap();
    assert_eq!(r.source, ResolutionSource::None);
    assert!(r.record.is_none());
}

#[test]
fn excluded_glob_is_not_enumerated_as_candidate() {
    let tree = temptree! {
        "assets": {
            "keep.glb": "binary",
            "skip.bak": "junk"
        }
    };
    let root = tree.path();
    let fs = real_fs();
    // User exclude: *.bak
    let patterns = auditah::discovery::all_excludes(&["**/*.bak".to_string()]);
    let excludes = ExcludeMatcher::new(&patterns).unwrap();
    let got = enumerate(&fs, root, &excludes).unwrap();
    let names: Vec<String> = got
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert!(names.contains(&"keep.glb".to_string()));
    assert!(
        !names.contains(&"skip.bak".to_string()),
        "excluded glob should be filtered out"
    );
}

#[test]
fn sidecar_and_manifest_themselves_are_not_enumerated_as_assets() {
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": "metadata",
        "manifest.toml": "metadata"
    };
    let root = tree.path();
    let fs = real_fs();
    let got = enumerate(&fs, root, &default_excludes()).unwrap();
    let names: Vec<String> = got
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    assert_eq!(names, vec!["sword.glb"]);
    // Sidecar suffix and manifest name are part of the default excludes.
    let _ = SIDECAR_SUFFIX;
    let _ = MANIFEST_FILENAME;
}

#[test]
fn filename_with_spaces_resolves_sidecar() {
    let tree = temptree! {
        "Gunny Sack.glb": "binary",
        "Gunny Sack.glb.attr.toml": r#"
title = "Gunny Sack"
author = "Oliver Herklotz"
year = 2019
license = "CC-BY-3.0"
source = "https://poly.pizza/m/download/Gunny-Sack"
"#
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("Gunny Sack.glb");
    let r = resolve(&fs, &asset, root).unwrap();
    assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
    let rec = r.record.unwrap();
    assert_eq!(rec.title, "Gunny Sack");
    assert_eq!(rec.author, "Oliver Herklotz");
    // Re-enumerate to confirm the spaced filename survives the walk + exclude filter.
    let got = enumerate(&fs, root, &default_excludes()).unwrap();
    assert_eq!(got.len(), 1);
    assert!(got[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("Gunny Sack"));
}
