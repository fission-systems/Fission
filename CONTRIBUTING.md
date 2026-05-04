# Contributing to Fission

First off, thank you for considering contributing to Fission! It's people like you that make Fission a great tool for the reverse engineering community.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
- [Contributor License Agreement](#contributor-license-agreement)
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

## Contributor License Agreement

Fission accepts contributions under the Contributor License Agreement in [`CLA.md`](./CLA.md).

By submitting a pull request or other intentional contribution to the project, you agree to that CLA.

This is required so the core engine can remain open while preserving the ability to operate Fission as an open-core project.

## License And Source Header Policy

Fission uses repository-level licensing as the default source of truth:

- the repository license is defined by [`LICENSE`](./LICENSE)
- public Rust packages also declare `license = "AGPL-3.0-or-later"` in their `Cargo.toml`

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
// SPDX-License-Identifier: AGPL-3.0-or-later
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
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# Build Fission
cargo build

# Build fission-cli
cargo build -p fission-cli --release

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

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
  "rust-analyzer.cargo.features": ["gui", "cli"],
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
# Format code
cargo fmt

# Lint code
cargo clippy -- -D warnings

# Fix automatically
cargo clippy --fix
```

### Code Organization

Fission is a **Cargo workspace**. There is **no** single-crate `src/analysis` layout; contributor-facing layout lives under [`crates/`](./crates/) per crate.

**Authoritative map:** [`docs/PROJECT_MAP.md`](./docs/PROJECT_MAP.md) — workspace members, `benchmark/`, `docs/`, `utils/`, `vendor/`, and CI roots.

**Ownership table:** [`AGENTS.md`](./AGENTS.md) — where to implement fixes (“canonical owner”), anti-patterns, and automation alignment.

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
   cargo fmt -- --check
   cargo clippy --workspace --exclude fission-tauri -- -D warnings
   cargo test -p fission-pcode -p fission-automation -p fission-loader --verbose
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

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
How was this tested?

## Screenshots (if applicable)
Add screenshots for GUI changes

## Checklist
- [ ] Code follows style guidelines
- [ ] Tests added
- [ ] Documentation updated
```

### Review Process

1. **Automated checks** must pass (CI, tests, lints)
2. **At least one approval** from maintainers
3. **All discussions resolved**
4. **No merge conflicts**

**Maintainers will:**
- Review within 3-7 days
- Provide constructive feedback
- Merge when approved

---

## CI/CD Workflow

Fission uses a two-layer CI strategy to keep local development fast while preserving deep validation in GitHub.

### 1) Fast PR Gate (`.github/workflows/ci.yml`)

Runs on pull requests and pushes to `main`:

- Linux fast gate
  - `cargo deny` and Node security audit
  - formatting/lint checks
  - focused Rust test set
  - decompiler smoke test
- macOS build/test
- Windows build/test

This is the main merge gate and should fail quickly when regressions are introduced.

### 2) Heavy Validation (`.github/workflows/ci-heavy.yml`)

Runs on `main`, nightly schedule, and manual dispatch:

- Linux full validation
  - broader Rust test coverage
  - Tauri frontend build (`npm run build`)
  - decompiler smoke validation
- Windows heavy build/test
- automation nir-check lanes (`nir`, `regression`) with artifact upload

Heavy checks may take longer and are intentionally offloaded to GitHub.

### Automation artifacts

Heavy workflow uploads `benchmark/artifacts/automation/` so contributors can inspect:

- `summary.json` / `summary.md`
- `decision_insights.json`
- `diagnosis.json`
- per-binary candidate outputs

If your PR impacts decompilation quality logic, review these artifacts before concluding regression status.

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

Place in `tests/` directory:

```rust
// tests/cli_tests.rs
use assert_cmd::Command;

#[test]
fn test_cli_info() {
    Command::cargo_bin("fission_cli")
        .unwrap()
        .args(&["--info", "test/struct_test.exe"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Binary Information"));
}
```

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_load_pe

# With output
cargo test -- --nocapture

# Integration tests only
cargo test --test cli_tests
```

### Test Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html
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
- **Design** - UI/UX improvements, logos, graphics
- **Testing** - Test on different platforms
- **Triage** - Help organize issues
- **Translation** - Internationalization (future)

---

## License

By contributing to Fission, you agree that your contributions are governed by the repository license and the Contributor License Agreement in [`CLA.md`](./CLA.md).

For the public repository, the project license is AGPL-3.0-or-later.

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
