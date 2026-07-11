# Style Guide

This document defines the _coding conventions_, _patterns_, and _architecture_ for the `auditah` codebase.

- IGNORE ALL CODE IN `vendor/` UNLESS IT'S SPECIFICALLY RELATED TO THE TASK.

## 1. Overview

auditah is an obligation-aware license compliance + attribution tool for gamedev. It is a single-binary, stateless Rust CLI: parse args, walk the project tree, resolve each asset's effective license terms against the registry, emit findings and distribution artifacts, exit. No long-running processes, no message bus, no embedded scripting.

This style guide ensures consistent, maintainable Rust code across the codebase. It covers error handling, the service/trait abstractions, testing patterns, documentation standards, and the CLI pipeline layout. Following these patterns enables dependency injection for testability (real filesystem in production, in-memory `FakeFs` in tests) and clear separation between domain logic and the filesystem.

## 2. Core Patterns

### Error Handling

Use `wherror::Error` with `error_stack::Report` for all fallible operations.

**Colocate errors with their related types.** Never create standalone `error.rs` or `errors.rs` files. Error types belong in the same module as the trait, struct, or function that produces them. For example, `ConfigError` lives in `config.rs` alongside the `Config` loader, `RegistryError` lives in `registry.rs` alongside `LicenseRegistry`, and `FsError` lives in `services/fs.rs` alongside the `FsBackend` trait.

**Error type:**

```rust
use wherror::Error;

#[derive(Debug, Error)]
#[error(debug)]
pub struct AppError;
```

**Result with error context:**

```rust
use error_stack::{Report, ResultExt};

pub fn load(fs: &FsService, root: &Path) -> Result<Config, Report<ConfigError>> {
    let content = fs
        .read_to_string(&root.join("auditah.toml"))
        .change_context(ConfigError)
        .attach("failed to read auditah.toml")?;
    Ok(config)
}
```

**Document errors in functions:**

```rust
/// # Errors
///
/// Returns `RegistryError` if a `LICENSES/*.toml` fails to parse or read.
pub fn load(fs: &FsService, project_root: &Path) -> Result<Self, Report<RegistryError>>
```

### Trait Usage

Every external dependency or service must have a trait abstraction.

**Colocate traits with their related types.** Never create standalone `traits.rs` files. Traits belong in the same module as the types that implement them. For example, `FsBackend` lives in `services/fs.rs` alongside `RealFs` and `FakeFs`, not in a separate `traits.rs`.

**Capability trait pattern (`FsBackend` in `src/services/fs.rs`):**

```rust
use wherror::Error;

#[derive(Debug, Error)]
#[error(debug)]
pub struct FsError;

/// Capability trait: read/write/list/walk the filesystem.
///
/// Production uses `RealFs`; tests use a fake in-memory backend.
pub trait FsBackend: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, Report<FsError>>;
    fn write(&self, path: &Path, content: &str) -> Result<(), Report<FsError>>;
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn walk(&self, root: &Path) -> Result<Vec<PathBuf>, Report<FsError>>;
    fn exists(&self, path: &Path) -> bool;
    fn name(&self) -> &'static str;
}
```

**Service wrapper pattern (`FsService`):**

```rust
use std::sync::Arc;
use derive_more::Debug;

#[derive(Debug, Clone)]
pub struct FsService {
    #[debug("FsService<{}>", self.backend.name())]
    backend: Arc<dyn FsBackend>,
}

impl FsService {
    pub fn new(backend: Arc<dyn FsBackend>) -> Self {
        Self { backend }
    }
}
```

**Key trait design rules:**

- Include a `name(&self) -> &'static str` method on capability traits for debugging.
- Service structs wrap `Arc<dyn Trait>` for shared, cheap-to-clone ownership.

### Dependency Injection

**Services container (`Services` in `src/services.rs`):**

```rust
#[derive(Debug, Clone)]
pub struct Services {
    pub fs: FsService,
    pub registry: LicenseRegistry,
}
```

Constructed once in `main` (via `Services::real(root)`) and passed by reference into domain functions. Tests build a `Services` either via `Services::from_parts(fs, registry)` with a `FakeFs`, or via the helpers in `tests/common/mod.rs` (`services_with`, `services_empty`).

Fields are added as subsystems come online; the pattern is what matters, not the specific field list. Every field must either be cheap to clone or use the service-wrapper pattern above.

### Module System

Use the Rust module system throughout:

- **Top-level subsystem directories** use `mod.rs` (e.g., `src/cli/mod.rs`). This is the only exception.
- **All other modules** use `foo.rs` alongside a `foo/` directory - never `mod.rs` inside a non-top-level directory.
- Each CLI subcommand lives in its own `src/cli/<name>_cmd.rs` with a `pub fn run(cmd) -> Result<CommandStatus, Report<AppError>>`.

### Block Scoping

When a value requires multiple setup steps or intermediate bindings, wrap the sequence in a block expression so the final binding is immutable and temporaries don't leak into the surrounding scope. This reduces the number of variables floating around a function and makes the code easier to extract into a function later.

**Create-then-configure:**

```rust
// ❌ BAD - mutable binding lives past setup
let mut services = ServicesBuilder::new();
services.register(fs);
services.register(registry);
```

```rust
// ✅ GOOD - setup is scoped, final binding is immutable
let services = {
    let mut builder = ServicesBuilder::new();
    builder.register(fs);
    builder.register(registry);
    builder.build()
};
```

**Intermediate values:**

```rust
// ❌ BAD - a and b remain in scope after c is computed
let a = 1;
let b = 2;
let c = a + b;
```

```rust
// ✅ GOOD - a and b are scoped to the block
let c = {
    let a = 1;
    let b = 2;
    a + b
};
```

## 3. Tests

Important:

- Tests should only verify _observable behavior_
- Testing internal details is an _anti-pattern_.
- Prefer testing observable behavior ONLY. If observable behavior cannot be tested, then an abstraction needs to be created. Ask the user how to proceed in this case.

### One Test, One Behavior

**Every test must assert exactly one semantic concept.** A test should answer a single question about the system. When it fails, the test name alone must tell you _what_ broke.

This means each test has exactly **one** `// When` and **one** `// Then` block. A `// Then` may be followed by `// And` lines, but only those lines elaborate on the same observable behavior - never when they describe a different behavior.

**What counts as "one concept":**

- Checking multiple fields of the _same result_ - fine. All confirms "the result is correct."
- Checking that every item in a filtered list matches the filter - fine. All confirms "the filter worked."
- Checking that every `Finding` in a report maps to one asset - fine. All confirms "the report covered the assets."

**What counts as separate concepts (split into separate tests):**

- A command that updates state **and** emits a finding → two tests. State change and finding emission are separate observable behaviors.
- An audit run that FAILs one asset for one reason **and** another asset for another reason → two tests, one per finding.
- A multi-step lifecycle (enumerate, resolve, audit, emit) → one test per step. Each step is a separate state transition.

**Anti-patterns to avoid:**

```rust
// ❌ BAD - two When/Then blocks in one test
#[test]
fn audit_flags_missing_attribution_and_then_unknown_license() {
    // ...setup...
    // When auditing an asset missing attribution.
    // Then IncompleteAttribution is in the report.
    // When auditing an asset with an unknown license id.
    // Then UnknownLicense is in the report.
}
```

```rust
// ✅ GOOD - split into two tests
#[test]
fn audit_fails_when_attribution_incomplete() {
    // ...setup...
    // When auditing an asset whose sidecar omits author.
    // Then the report contains IncompleteAttribution.
}

#[test]
fn audit_fails_when_license_id_unknown_to_registry() {
    // ...setup...
    // When auditing an asset whose record.license does not resolve.
    // Then the report contains UnknownLicense.
}
```

**Duplicated test setup is acceptable.** Do not combine tests to avoid setup duplication.

### BDD-Style Tests (Given/When/Then)

Each Given/When/Then comment is followed by exactly one line of code. Name the test so it reads as a standalone behavior description.

```rust
#[test]
fn pop_returns_none_when_stack_empty() {
    // Given an empty stack.
    let mut stack = Stack::default();

    // When popping from the stack.
    let item = stack.pop();

    // Then we get nothing back.
    assert!(item.is_none());
}
```

```rust
#[test]
fn push_increments_length() {
    // Given an empty list.
    let mut list = MyList::new();

    // When pushing one item.
    list.push("a");

    // Then the length is one.
    assert_eq!(list.len(), 1);
}
```

### Parameterized Tests with rstest

If a test has many inputs, prefer parametrizing with `rstest`:

```rust
#[rstest::rstest]
#[case("MIT", true)]
#[case("GPL-3.0", false)]
fn commercial_use_allowed(#[case] id: &str, #[case] expected: bool) {
    // Given / When / Then inline for simple cases
    assert_eq!(terms_for(id).allows_commercial_use, expected);
}
```

For edge cases that don't easily fit into "expected", prefer a BDD-styled test instead.

Use rstest when you find yourself writing the same assertion logic against different inputs. Do _not_ use rstest to combine different behaviors into one test - each `#[case]` must test the same property.

### Where Tests Live

- **Unit tests** - colocated in `#[cfg(test)] mod tests { ... }` at the bottom of each module (`src/*.rs`, `src/**/*.rs`).
- **Integration tests** - one file per pipeline in `tests/` (e.g., `audit_pipeline.rs`, `bom_pipeline.rs`, `credits_pipeline.rs`). Each end-to-end test builds a `temptree!`, constructs a `Services` + `Config`, and asserts on the command's output or the `AuditReport`.
- **Shared helpers** - `tests/common/mod.rs` provides `services_with`, `services_empty`, `non_commercial_config`, `codes_for`, term builders (`permissive_terms`, `share_alike_terms`, ...). Pulled into each integration file via `mod common;`.

### Test Utilities

- `FakeFs` (in `src/test_support.rs`, gated behind the `test-helper` Cargo feature and `#[doc(hidden)]`) - in-memory `FsBackend` for tests. Construct via `FakeFs::with_files([...])`; supports fault injection (`fail_read`, `fail_write`).
- `temptree` - builds a real temp filesystem for end-to-end integration tests.
- Domain-specific builders (`permissive_terms()`, `non_commercial_config()`, `LicenseSpec::new(...).terms(...)`) live in `tests/common/mod.rs`.

## 4. Documentation

### Module-Level Documentation

Module level documentation should explain its purpose and high-level behaviors. Only explain technical details as necessary to make the high-level documentation understandable.

```rust
//! License registry: project-local `LICENSES/*.toml` definitions loaded at
//! runtime. No embedded licenses - every license is `LicenseRef-*` authored
//! via `add-license` (or hand-placed in `LICENSES/`).
//!
//! Each license is two files in a single `LICENSES/` directory:
//! `<id>.toml` (metadata + terms grid) and `<id>.txt` (full legal text). The
//! `.toml` is parsed here; the `.txt` presence is checked at audit time.
```

### Type Documentation

```rust
/// The license registry: `LICENSES/*.toml` definitions loaded at runtime.
#[derive(Debug, Clone)]
pub struct LicenseRegistry {
    entries: HashMap<String, LicenseRegistryEntry>,
}
```

## 5. Tooling

Read the `justfile` to determine what additional tooling is related to this project. Prioritize running commands from the `justfile` instead of manual invocation.

### Project Commands

Skills refer to commands by **role**; the table below resolves each role to this project's actual command.

| Role         | Command                        | Description                                                                       |
| ------------ | ------------------------------ | --------------------------------------------------------------------------------- |
| `vcs`        | git                            | This project uses git for version control (`git status`, `git diff`, `git log`, ...). |
| `check`      | `just check`                   | `cargo check` - fast compilation without codegen.                                 |
| `test`       | `just test`                    | `cargo nextest run` - **all tests must pass before committing**.                  |
| `lint`       | `just clippy`                  | `cargo clippy --all-targets -- -D warnings`.                                      |
| `format`     | `just fmt-fix`                 | Apply formatting fixes.                                                           |
| `commit`     | `git commit -m "<message>"`    | Commit changes.                                                                   |
| `sync-trunk` | `git pull --rebase`            | Fetch latest `origin/main` and surface conflicts locally.                         |

### Plan Directory

Task plans live in `.plans/<task>/` where `<task>` is a slugified task name. Each task directory contains:

- `plan.md` - the specification (source of truth for what to implement)
- `phase-N.md` - execution plans and phase reviews for each phase

The task list (managed via `todo_*` tools) tracks progress. The spec is an immutable reference - agents annotate it with divergence notes but never rewrite it.

## 6. Misc

- NEVER manually split a string using `.chars` or by indexing. Use the `unicode-segmentation` crate.
- No trivial setters for struct methods. Prefer meaningful semantic actions. It's an anti-pattern to directly inspect and manipulate state.
- Environment variables should only be accessed at program initialization and then saved into a struct as needed. Environment variables are a global namespace and should be avoided outside of program startup.
- Use `where` clause for all generics.
- Prefer `match` over `if` where appropriate.
- DO NOT USE CODE COMMENTS TO WRITE ABOUT "SPEC DIVERGENCES" OR "DIVERGENCES". Code comments in the codebase is not the place to discuss planning information. PLANS ARE NOT PERSISTED.
