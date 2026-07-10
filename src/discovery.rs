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
    "**/manifest.toml",
    // License registry dirs
    "**/licenses/**",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_excludes_include_sidecar_and_manifest_and_registry() {
        // Spot-check load-bearing entries.
        assert!(DEFAULT_EXCLUDES.contains(&"**/*.attr.toml"));
        assert!(DEFAULT_EXCLUDES.contains(&"**/manifest.toml"));
        assert!(DEFAULT_EXCLUDES.contains(&"**/LICENSES/**"));
        assert!(DEFAULT_EXCLUDES.contains(&"auditah.toml"));
    }

    #[test]
    fn all_excludes_merges_defaults_then_user() {
        let user = vec!["vendor/**".to_string(), "*.bak".to_string()];
        let merged = all_excludes(&user);
        // Defaults present first.
        assert!(merged.iter().any(|p| p == "**/.git/**"));
        // User patterns appended after.
        assert_eq!(merged.last().unwrap(), "*.bak");
        assert!(merged.contains(&"vendor/**".to_string()));
    }

    #[test]
    fn all_excludes_empty_user_returns_only_defaults() {
        let merged = all_excludes(&[]);
        assert!(merged.len() == DEFAULT_EXCLUDES.len());
    }
}
