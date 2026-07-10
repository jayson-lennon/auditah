//! Integration tests: discovery + resolution precedence against a real
//! temp filesystem. These exercise `walkdir`/`globset` behavior that the
//! in-memory unit-test fakes do not.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use auditah::discovery::enumerator::{enumerate, ExcludeMatcher};
use auditah::discovery::resolver::{resolve, ResolutionSource, MANIFEST_FILENAME, SIDECAR_SUFFIX};
use temptree::temptree;

mod common;
use common::{default_excludes, real_fs};

#[test]
fn sidecar_wins_over_manifest_in_same_dir() {
    // Given a dir with both a sidecar and a manifest for the same asset.
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": r#"
title = "Sword"
author = "Smith"
year = 2020
license = "LicenseRef-Mit"
source = "https://example.com/sword"
"#,
        "manifest.toml": r#"
title = "Pack"
author = "Quaternius"
year = 2022
license = "LicenseRef-Cc0"
source = "https://example.com/pack"
"#,
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sword.glb");

    // When resolving.
    let r = resolve(&fs, &asset, root).unwrap();

    // Then the sidecar wins and its license is used.
    assert!(
        matches!(r.source, ResolutionSource::Sidecar(_)),
        "sidecar should win"
    );
    assert_eq!(r.record.unwrap().license, "LicenseRef-Mit");
}

#[test]
fn subdir_manifest_overrides_parent_manifest() {
    // Given a parent manifest and a subdir manifest with different licenses.
    let tree = temptree! {
        "manifest.toml": r#"
title = "Parent"
author = "P"
year = 2020
license = "LicenseRef-Cc0"
source = "https://example.com"
"#,
        "sub": {
            "manifest.toml": r#"
title = "Child"
author = "C"
year = 2021
license = "LicenseRef-Mit"
source = "https://example.com/child"
"#,
            "rock.glb": "binary",
        }
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sub").join("rock.glb");

    // When resolving the subdir asset.
    let r = resolve(&fs, &asset, root).unwrap();

    // Then the subdir manifest wins (nearest).
    assert!(matches!(r.source, ResolutionSource::Manifest(_)));
    assert_eq!(
        r.record.unwrap().license,
        "LicenseRef-Mit",
        "subdir manifest should win"
    );
}

#[test]
fn parent_manifest_is_fallback_when_no_subdir_config() {
    // Given a parent manifest and an uncovered subdir asset.
    let tree = temptree! {
        "manifest.toml": r#"
title = "Parent"
author = "P"
year = 2020
license = "LicenseRef-Cc0"
source = "https://example.com"
"#,
        "sub": {
            "leaf.glb": "binary"
        }
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("sub").join("leaf.glb");

    // When resolving the subdir asset.
    let r = resolve(&fs, &asset, root).unwrap();

    // Then the parent manifest is the fallback.
    assert!(matches!(r.source, ResolutionSource::Manifest(_)));
    assert_eq!(r.record.unwrap().license, "LicenseRef-Cc0");
}

#[test]
fn uncovered_when_no_config_anywhere() {
    // Given an asset with no sidecar and no manifest anywhere.
    let tree = temptree! {
        "orphan.glb": "binary"
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("orphan.glb");

    // When resolving.
    let r = resolve(&fs, &asset, root).unwrap();

    // Then the source is None and no record is present.
    assert_eq!(r.source, ResolutionSource::None);
    assert!(r.record.is_none());
}

#[test]
fn excluded_glob_is_not_enumerated_as_candidate() {
    // Given a dir with a kept asset and a *.bak file.
    let tree = temptree! {
        "assets": {
            "keep.glb": "binary",
            "skip.bak": "junk"
        }
    };
    let root = tree.path();
    let fs = real_fs();
    let patterns = auditah::discovery::all_excludes(&["**/*.bak".to_string()]);
    let excludes = ExcludeMatcher::new(&patterns).unwrap();

    // When enumerating with the *.bak exclude.
    let got = enumerate(&fs, root, &excludes).unwrap();
    let names: Vec<String> = got
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // Then keep.glb is included and skip.bak is excluded.
    assert!(names.contains(&"keep.glb".to_string()));
    assert!(
        !names.contains(&"skip.bak".to_string()),
        "excluded glob should be filtered out"
    );
}

#[test]
fn sidecar_and_manifest_themselves_are_not_enumerated_as_assets() {
    // Given a dir with an asset plus its sidecar and a manifest.
    let tree = temptree! {
        "sword.glb": "binary",
        "sword.glb.attr.toml": "metadata",
        "manifest.toml": "metadata"
    };
    let root = tree.path();
    let fs = real_fs();

    // When enumerating with default excludes.
    let got = enumerate(&fs, root, &default_excludes()).unwrap();
    let names: Vec<String> = got
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    // Then only the asset is enumerated; sidecar and manifest are excluded.
    assert_eq!(names, vec!["sword.glb"]);
    let _ = SIDECAR_SUFFIX;
    let _ = MANIFEST_FILENAME;
}

#[test]
fn filename_with_spaces_resolves_sidecar() {
    // Given an asset with spaces in its name and a matching sidecar.
    let tree = temptree! {
        "Gunny Sack.glb": "binary",
        "Gunny Sack.glb.attr.toml": r#"
title = "Gunny Sack"
author = "Oliver Herklotz"
year = 2019
license = "LicenseRef-CcBy"
source = "https://poly.pizza/m/download/Gunny-Sack"
"#
    };
    let root = tree.path();
    let fs = real_fs();
    let asset = root.join("Gunny Sack.glb");

    // When resolving and re-enumerating.
    let r = resolve(&fs, &asset, root).unwrap();
    let got = enumerate(&fs, root, &default_excludes()).unwrap();

    // Then the sidecar resolves and the spaced filename survives the walk.
    assert!(matches!(r.source, ResolutionSource::Sidecar(_)));
    let rec = r.record.unwrap();
    assert_eq!(rec.title, "Gunny Sack");
    assert_eq!(rec.author, "Oliver Herklotz");
    assert_eq!(got.len(), 1);
    assert!(got[0]
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("Gunny Sack"));
}
