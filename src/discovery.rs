//! Default exclude set for asset enumeration, plus merging with user excludes.

pub mod enumerator;
pub mod resolver;

/// Built-in glob patterns always excluded from enumeration.
///
/// These cover: VCS/tooling metadata, sidecars and manifests themselves, the
/// license registry dirs, build output, archives, and the tool's own files.
/// Matched against paths relative to the enumeration root using `globset`.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    // VCS + tooling metadata
    "**/.git/**",
    "**/.git",
    "**/.fossil*",
    "**/.godot/**",
    "**/.import/**",
    // Sidecars + manifests themselves (they are metadata, not assets)
    "**/*.attr.toml",
    resolver::MANIFEST_EXCLUDE_GLOB,
    // License definitions dir
    "**/LICENSES/**",
    // Build output
    "**/target/**",
    // Archives (containers, not the assets themselves)
    "**/*.zip",
    "**/*.tar",
    "**/*.tar.gz",
    "**/*.tgz",
    "**/*.tar.bz2",
    // Tool's own files
    "auditah.toml",
    "Cargo.toml",
    "Cargo.lock",
    "**/*.lock",
    "CREDITS.md",
    "NOTICES.md",
    "BOM.md",
    // Common non-asset config
    "**/justfile",
    "**/Justfile",
    "**/README.md",
];

/// All glob patterns that apply: built-in defaults merged with user-supplied
/// `[exclude]` entries from `auditah.toml`. Order: defaults first, then user.
#[must_use]
pub fn all_excludes(user_excludes: &[String]) -> Vec<String> {
    let mut all: Vec<String> = DEFAULT_EXCLUDES.iter().map(|s| (*s).to_string()).collect();
    all.extend(user_excludes.iter().cloned());
    all
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_excludes_include_sidecar_and_manifest_and_registry() {
        // Given the DEFAULT_EXCLUDES set.
        // When spot-checking load-bearing entries.
        // Then sidecar, manifest, registry, and config are all excluded.
        assert!(DEFAULT_EXCLUDES.contains(&"**/*.attr.toml"));
        assert!(DEFAULT_EXCLUDES.contains(&resolver::MANIFEST_EXCLUDE_GLOB));
        assert!(!DEFAULT_EXCLUDES.contains(&"**/manifest.toml"));
        assert!(DEFAULT_EXCLUDES.contains(&"**/LICENSES/**"));
        assert!(DEFAULT_EXCLUDES.contains(&"auditah.toml"));
        assert!(DEFAULT_EXCLUDES.contains(&"CREDITS.md"));
        assert!(DEFAULT_EXCLUDES.contains(&"NOTICES.md"));
        assert!(DEFAULT_EXCLUDES.contains(&"BOM.md"));
    }

    #[test]
    fn all_excludes_merges_defaults_then_user() {
        // Given user-supplied exclude patterns.
        let user = vec!["vendor/**".to_string(), "*.bak".to_string()];

        // When merging with defaults.
        let merged = all_excludes(&user);

        // Then defaults come first and user patterns are appended after.
        assert!(merged.iter().any(|p| p == "**/.git/**"));
        assert_eq!(merged.last().unwrap(), "*.bak");
        assert!(merged.contains(&"vendor/**".to_string()));
    }

    #[test]
    fn all_excludes_empty_user_returns_only_defaults() {
        // Given no user excludes.
        // When merging with defaults.
        let merged = all_excludes(&[]);

        // Then the result contains only the defaults.
        assert!(merged.len() == DEFAULT_EXCLUDES.len());
    }

    #[test]
    fn manifest_exclude_glob_matches_filename() {
        // Given the manifest filename and its exclude glob.
        // When checking they stay in sync.
        // Then the glob ends with the filename.
        assert!(resolver::MANIFEST_EXCLUDE_GLOB.ends_with(resolver::MANIFEST_FILENAME));
    }
}
