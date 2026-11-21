# Contributing to Fission

First off, thank you for considering contributing to Fission! It's people like you that make Fission a great tool for the reverse engineering community.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [License And Source Header Policy](#license-and-source-header-policy)
- [Development Setup](#development-setup)
- [Coding Standards](#coding-standards)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Testing](#testing)
- [CI/CD Workflow](#cicd-workflow)
- [Documentation](#documentation)
- [Community](#community)

---

## Code of Conduct

### Our Pledge

We pledge to make participation in our project a harassment-free experience for everyone, regardless of age, body size, disability, ethnicity, gender identity, level of experience, nationality, personal appearance, race, religion, or sexual identity and orientation.

### Our Standards

**Positive behavior includes:**
- Using welcoming and inclusive language
- Being respectful of differing viewpoints and experiences
- Gracefully accepting constructive criticism
- Focusing on what is best for the community
- Showing empathy towards other community members

**Unacceptable behavior includes:**
- Trolling, insulting/derogatory comments, and personal or political attacks
- Public or private harassment
- Publishing others' private information without explicit permission
- Other conduct which could reasonably be considered inappropriate

---

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check existing issues to avoid duplicates.

**When creating a bug report, include:**
- **Clear title** - Brief description of the issue
- **Environment** - OS, Rust version, Fission version
- **Steps to reproduce** - Detailed steps to trigger the bug
- **Expected behavior** - What should happen
- **Actual behavior** - What actually happens
- **Binary sample** (if applicable) - Link or attachment
- **Logs** - Run with `RUST_LOG=debug` and include output

**Example:**
```markdown
**Title:** Decompiler crashes on ARM64 binaries

**Environment:**
- OS: Ubuntu 22.04
- Rust: 1.85+
- Fission: 0.1.0

**Steps:**
1. Load ARM64 ELF binary
2. Select function at 0x1000
3. Click "Decompile"

**Expected:** Decompiled C code appears
**Actual:** Application crashes with SIGSEGV

**Logs:**
```
thread 'main' panicked at 'attempt to subtract with overflow'
```
```

### Suggesting Features

Feature requests are welcome! Please:
- **Use a clear title** describing the feature
- **Provide detailed description** of the proposed functionality
- **Explain use cases** - Why is this feature useful?
- **Consider alternatives** - Other ways to achieve the same goal
- **Mock up examples** - Screenshots, code samples, or diagrams

### Code Contributions

Contributions are made through pull requests. See [Pull Request Process](#pull-request-process) below.

**Good first issues:**
- Documentation improvements
- Test coverage expansion
- Bug fixes with clear reproduction steps
- Adding support for new binary formats
- Improving error messages

## License And Source Header Policy

Fission uses repository-level licensing as the default source of truth:

- the repository license is defined by [`LICENSE`](./LICENSE)
- public Rust packages also declare `license = "Apache-2.0"` in their `Cargo.toml`

Current source header policy:

- do **not** mass-insert long copyright headers into every source file
- prefer a clean repository-level license with package metadata
- add a short SPDX header only when it is specifically useful or required

Use a short SPDX-style header in cases like:

- new standalone source files that may be copied out of the repository
- generated files that need explicit provenance
- files containing imported or adapted third-party material that require attribution

Preferred short header format:

```text
// SPDX-License-Identifier: Apache-2.0
```

Do not add mixed or ad-hoc custom headers without a clear reason. If a contribution introduces third-party code or unusual attribution requirements, document that explicitly in the pull request.

---

## Development Setup

### Prerequisites

```bash
# Install Rust 1.85+
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install development tools
cargo install cargo-watch  # Auto-rebuild
cargo install cargo-edit   # Manage dependencies
cargo install cargo-audit  # Security audits
```

### Clone and Build

```bash
git clone https://github.com/fission-systems/Fission.git
cd Fission

# Build Fission
cargo build

# Build fission-cli
cargo build -p fission-cli --release

# Run tests (nextest is the default runner)
cargo nextest run -p fission-pcode

# Run the CLI with logging, from the repo root (it reads ./utils)
RUST_LOG=debug cargo run -p fission-cli -- info path/to/binary
```

Fission is agent-first: the CLI and its JSON contract are the product
surface. There is no GUI or TUI; both were built and deliberately removed
(see [`docs/roadmap/1.0-criteria.md`](./docs/roadmap/1.0-criteria.md)).

### IDE Setup

#### VS Code (Recommended)

Install extensions:
- **rust-analyzer** - LSP for Rust
- **CodeLLDB** - Debugging
- **Better TOML** - Cargo.toml editing

**.vscode/settings.json:**
```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "editor.formatOnSave": true
}
```

#### RustRover / IntelliJ IDEA

1. Install Rust plugin
2. Open project
3. Right-click `Cargo.toml` → "Attach Cargo Project"

---

## Coding Standards

### Rust Style Guide

Follow the [Rust Style Guide](https://doc.rust-lang.org/style-guide/). Use `rustfmt` and `clippy`:

```bash
# Format code -- CI runs `cargo fmt --all --check`
cargo fmt --all

# Lint code -- CI's exact line
cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity
```

Run `cargo fmt --all` with your change, not as a separate sweep, and never
run `rustfmt` on a single file: it follows `mod` declarations into files you
did not touch (see [`AGENTS.md`](./AGENTS.md)).

### Code Organization

Fission is a **Cargo workspace**. There is **no** single-crate `src/analysis` layout; contributor-facing layout lives under [`crates/`](./crates/) per crate.

**Authoritative map:** [`docs/PROJECT_MAP.md`](./docs/PROJECT_MAP.md) — workspace members, `benchmark/`, `docs/`, `utils/`, `vendor/`, and CI roots.

**Ownership table:** [`AGENTS.md`](./AGENTS.md) — where to implement fixes (“canonical owner”), anti-patterns, and how quality is measured.

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Modules | snake_case | `debug_engine` |
| Structs/Enums | PascalCase | `BinaryInfo` |
| Functions/Methods | snake_case | `load_binary()` |
| Constants | SCREAMING_SNAKE_CASE | `MAX_WORKERS` |
| Type Parameters | Single letter or PascalCase | `T`, `BinaryType` |

### Documentation

**All public items must have doc comments:**

```rust
/// Load a binary file from the given path.
///
/// This function supports PE, ELF, and Mach-O formats.
///
/// # Arguments
///
/// * `path` - Path to the binary file
///
/// # Returns
///
/// * `Ok(BinaryInfo)` - Successfully loaded binary
/// * `Err(FissionError)` - Failed to load (file not found, unsupported format, etc.)
///
/// # Examples
///
/// ```
/// let binary = load_binary("test.exe")?;
/// println!("Loaded: {}", binary.format);
/// ```
pub fn load_binary(path: &Path) -> Result<BinaryInfo> {
    // Implementation
}
```

### Error Handling

**Use `Result` and custom error types:**

```rust
// ✅ Good
pub fn decompile(addr: u64) -> Result<String, DecompileError> {
    let func = find_function(addr)?;
    let code = ghidra_decompile(func)?;
    Ok(code)
}

// ❌ Bad
pub fn decompile(addr: u64) -> String {
    let func = find_function(addr).unwrap();  // Can panic!
    ghidra_decompile(func).unwrap()
}
```

**Custom error types:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum DecompileError {
    #[error("Function not found at address 0x{0:x}")]
    FunctionNotFound(u64),
    
    #[error("Decompilation timeout after {0}ms")]
    Timeout(u64),
    
    #[error("Decompiler error: {0}")]
    GhidraError(String),
}
```

### Avoid `unwrap()` and `expect()`

**Only use in:**
- Tests
- Main function initialization (where panic is acceptable)
- After explicit checks proving it's safe

**Prefer:**
```rust
// ✅ Use ? operator
let value = option_value.ok_or(Error::NotFound)?;

// ✅ Use map_err for context
let file = File::open(path)
    .map_err(|e| Error::FileOpen(path.to_owned(), e))?;

// ✅ Use if-let or match
if let Some(value) = option_value {
    process(value);
}
```

---

## Commit Guidelines

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Types:**
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `style`: Code style changes (formatting, no logic change)
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `test`: Adding or updating tests
- `chore`: Build system, dependencies, CI/CD

**Examples:**

```bash
# Feature
git commit -m "feat(cli): add --xrefs flag for cross-reference analysis"

# Bug fix
git commit -m "fix(decomp): resolve crash on nested structure access"

# Documentation
git commit -m "docs: add plugin development guide"

# Multi-line
git commit -m "refactor(unpacker): rename debug_engine to unpacker

- Clarifies purpose as memory analysis tool
- Updates all references in 6 files
- Adds comprehensive documentation"
```

### Atomic Commits

- One logical change per commit
- Each commit should be buildable
- Squash WIP commits before PR

```bash
# Bad: Multiple unrelated changes
git commit -m "fix parser, update readme, add feature X"

# Good: Separate commits
git commit -m "fix(parser): handle empty input"
git commit -m "docs: update README examples"
git commit -m "feat(cli): add --strings flag"
```

---

## Pull Request Process

### Before Submitting

1. **Fork the repository**
2. **Create a feature branch**
   ```bash
   git checkout -b feat/my-awesome-feature
   ```

3. **Make your changes**
4. **Write tests**
5. **Run checks**
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity
   cargo nextest run -p <crates you touched>
   cargo build -p fission-cli --release
   ```

6. **Update documentation**
7. **Commit with conventional format**

### PR Checklist

- [ ] Code follows style guidelines
- [ ] Self-review completed
- [ ] Comments added for complex logic
- [ ] Documentation updated
- [ ] Tests added/updated
- [ ] All tests pass
- [ ] No new warnings
- [ ] Commit messages follow conventions

### PR Template

GitHub fills in [`.github/pull_request_template.md`](./.github/pull_request_template.md)
when you open a pull request. If your change can alter decompiler output, its
"Benchmark Impact" block needs a DecBench measurement (see
[`docs/BENCHMARK_DECBENCH.md`](./docs/BENCHMARK_DECBENCH.md)).

### Review Process

1. **Automated checks** must pass (CI, tests, lints)
2. **At least one approval** from maintainers
3. **All discussions resolved**
4. **No merge conflicts**

**Maintainers will:**
- Review within 3-7 days
- Provide constructive feedback
- Approve changes that meet the project requirements

For non-draft pull requests targeting `main`, GitHub Actions requests native
squash auto-merge after approval or when an approved draft becomes ready for
review. A 15-minute scheduled check catches missed events. GitHub completes the
merge only after all branch protection rules pass. Manual workflow runs default
to a dry run.

---

## CI/CD Workflow

Fission uses a two-layer CI strategy to keep local development fast while preserving deep validation in GitHub.

### 1) Fast PR Gate (`.github/workflows/ci.yml`)

Runs on pull requests and pushes to `main`. Jobs run only for the paths a
change touches (`Detect changes`):

- Security check (`cargo deny`, `cargo audit`)
- Lint & Format, Script Checks, and the anti-overfitting Pass-Gate Lint
- Tests (Linux), split into core and extras
- Native Debugger Tests (Linux)
- CLI Smoke (Linux, PE sample)
- Tests (macOS)

This is the main merge gate and should fail quickly when regressions are introduced.

### 2) Heavy Validation (`.github/workflows/ci-heavy.yml`)

Runs nightly and on manual dispatch (not on every push to `main`):

- Heavy Build & Test and Heavy Tests (Windows)
- Native Win32 Debugger (Windows)
- Heavy Build (macOS)
- Full Tests (Linux, extended) and Compile Surface (all features)
- MSRV check and code coverage

Because the Windows debugger tests run only here, dispatch it on a branch
that touches them and check that job before calling the change verified.

Heavy checks may take longer and are intentionally offloaded to GitHub.

---

## Testing

### Unit Tests

Place tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_pe() {
        let binary = load_binary("test/struct_test.exe").unwrap();
        assert_eq!(binary.format, "PE");
        assert_eq!(binary.arch, "x86_64");
    }

    #[test]
    fn test_invalid_format() {
        let result = load_binary("test/invalid.txt");
        assert!(result.is_err());
    }
}
```

### Integration Tests

Place in the crate's `tests/` directory. CLI tests run the built binary
from the repo root, as `crates/fission-cli/tests/*_cli.rs` do:

```rust
// crates/fission-cli/tests/info_cli.rs
use std::process::Command;

#[test]
fn info_reports_the_format() {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["info", "crates/fission-dir/testdata/ground_truth_fixture.elf", "--json"])
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli");
    assert!(output.status.success(), "{output:?}");
}
```

### Running Tests

```bash
# One crate (nextest is the default runner)
cargo nextest run -p fission-loader

# One test by name
cargo nextest run -p fission-loader test_load_pe

# With output
cargo nextest run -p fission-loader test_load_pe --no-capture

# Doctests (nextest does not run them)
cargo test --doc -p fission-loader
```

Every new test needs a positive control: disable the fix and show the test
fails. A test that cannot fail proves nothing.

### Test Coverage

CI measures coverage with `cargo-llvm-cov` (`.github/workflows/reusable-coverage.yml`):

```bash
cargo install cargo-llvm-cov
cargo llvm-cov nextest -p fission-loader --html
```

---

## Documentation

### Code Documentation

- Public APIs must have doc comments
- Include examples in doc comments
- Document panics, errors, and safety concerns

```rust
/// # Panics
/// Panics if the address is not 8-byte aligned.
///
/// # Safety
/// The caller must ensure `ptr` points to valid memory.
///
/// # Errors
/// Returns `Err` if the address is not within the binary's range.
```

### User Documentation

**When adding features, update:**
- README.md (if user-facing)
- docs/ folder (detailed guides)
- [`docs/changelog/Legacy/`](docs/changelog/Legacy/) — archived dated logs; for new releases use GitHub Release notes / forthcoming `CHANGELOG.md` (see [`docs/RELEASE.md`](docs/RELEASE.md))
- Code examples
- CLI help text

**Orientation:**
- Repo layout: [`docs/PROJECT_MAP.md`](docs/PROJECT_MAP.md)
- Versioning / tags: [`docs/VERSIONING.md`](docs/VERSIONING.md)
- Maintainer release gate: [`docs/RELEASE.md`](docs/RELEASE.md)
- Issue taxonomy: [`docs/contributing/LABELS.md`](docs/contributing/LABELS.md)
- New contributor paths: [`docs/onboarding/`](docs/onboarding/)

### Generating API Docs

```bash
# Generate docs
cargo doc --no-deps --open

# Include private items
cargo doc --document-private-items
```

---

## Community

### Getting Help

- **GitHub Issues** - Bug reports and feature requests (see [`docs/contributing/LABELS.md`](docs/contributing/LABELS.md) for the suggested label taxonomy)
- **GitHub Discussions** - Questions and general discussion
- **Matrix Chat** - Coming soon

### Contributing Beyond Code

- **Documentation** - Improve guides, fix typos
- **Agent ergonomics** - CLI and JSON contract improvements, error messages
- **Testing** - Test on different platforms
- **Triage** - Help organize issues
- **Translation** - Internationalization (future)

---

## License

By contributing to Fission, you agree that your contributions are governed by the repository license.

The project license is Apache-2.0.

---

## Recognition

Contributors are recognized in:
- `CONTRIBUTORS.md` file
- GitHub contributors page
- Release notes (for significant contributions)

---

## Questions?

Don't hesitate to ask! Open an issue or discussion if anything is unclear.

Thank you for contributing to Fission! 🚀
