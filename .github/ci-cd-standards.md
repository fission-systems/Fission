# CI/CD Pipeline Standards

## Overview

Fission's CI/CD pipelines consist of 3 levels:
- **Fast Gate** (PR/Push): Quick feedback, ~40 minutes
- **Heavy Validation** (Main/Nightly): Comprehensive validation, ~90 minutes
- **Release** (Tag): Release builds, ~45 minutes

---

## Pipeline Structure

```
Pull Request/Feature Branch
         ↓
   ┌─────────────────────────────────────┐
   │ 🟢 Fast Gate (ci.yml)                │
   │  ├─ Lint & Format (2 min)            │
   │  ├─ Security Check (3 min)           │
   │  ├─ Build (5 min, 3 platforms)       │
   │  └─ Tests (10 min, core modules)     │
   │  Total: ~40 minutes                  │
   └─────────────────────────────────────┘
         ↓ (Approved)
   Main Branch Merge
         ↓
   ┌─────────────────────────────────────┐
   │ 🔵 Heavy Validation (ci-heavy.yml)   │
   │  ├─ Corpus Validation (15 min)       │
   │  ├─ Full Test Suite (20 min)         │
   │  ├─ Build All Products (15 min)      │
   │  └─ Automation NIR-Check (45 min)    │
   │  Total: ~80 minutes                  │
   └─────────────────────────────────────┘
         ↓ (Manual)
   Tag Creation (v*.*.*)
         ↓
   ┌─────────────────────────────────────┐
   │ 🟡 Release (cd.yml)                  │
   │  ├─ Build CLI (3 platforms, 15 min)  │
   │  ├─ Package (5 min)                  │
   │  └─ Upload (5 min)                   │
   │  Total: ~45 minutes                  │
   └─────────────────────────────────────┘
```

---

## Standard Environment Variables

All workflows use these common environment variables:

```yaml
env:
  RUST_VERSION: "1.85"
  RUST_BACKTRACE: "1"
  CARGO_INCREMENTAL: "0"
  CARGO_NET_RETRY: "10"
  CARGO_TERM_COLOR: always
```

**Interpretation:**
- `RUST_VERSION`: Use stable Rust 1.85
- `RUST_BACKTRACE`: Print full stack trace on errors
- `CARGO_INCREMENTAL`: Disable incremental builds in CI (consistency)
- `CARGO_NET_RETRY`: Retry network operations 10 times
- `CARGO_TERM_COLOR`: Enable colored output

---

## Timeout Standards

| Step | Level | Timeout | Description |
|------|-------|---------|-------------|
| Lint/Format | Fast | 2 min | rustfmt + clippy |
| Security Check | Fast | 3 min | cargo-deny + npm audit |
| Build (single platform) | Fast | 5 min | Basic release build |
| Tests (core modules) | Fast | 10 min | fission-pcode, automation, loader |
| **Fast Gate (total)** | Fast | **40 min** | Total timeout |
| Full Test Suite | Heavy | 20 min | Full workspace tests |
| Build All | Heavy | 15 min | CLI build |
| NIR-Check | Heavy | 45 min | Automation benchmark |
| **Heavy Validation (total)** | Heavy | **80 min** | Total timeout |
| Release Build (3 platforms) | Release | 45 min | Linux + macOS + Windows |

---

## Success Criteria

### Fast Gate (required before PR approval)
- ✅ `cargo fmt --all -- --check` 100% pass
- ✅ `cargo clippy` 0 warnings (`-D warnings`)
- ✅ `cargo deny check all` security pass
- ✅ All platforms build successfully
- ✅ Core module tests pass (fission-pcode, automation, loader)

### Heavy Validation (auto-runs after merge to main)
- ✅ All Fast Gate criteria included
- ✅ Full workspace tests pass (`cargo test --workspace`)
- ✅ Corpus manifest validation pass
- ✅ NIR-Check complete (Regression < 5%)

### Release (on tag creation)
- ✅ All Heavy Validation criteria included
- ✅ 3 platform binaries created
- ✅ Packaging complete (.tar.gz, .zip)
- ✅ Uploaded to GitHub Release

---

## Platform Matrix

### Fast Gate
| OS | Role | Timeout |
|----|------|---------|
| `ubuntu-latest` | Primary validation (Lint, Security, Tests) | 40 min |
| `windows-latest` | Windows build/test | 45 min |
| `macos-latest` | macOS build/test | 45 min |

### Release
| OS | Target | Artifact | Archive |
|----|--------|----------|---------|
| `ubuntu-latest` | x86_64-unknown-linux-gnu | fission_cli | tar.gz |
| `macos-latest` | aarch64-apple-darwin | fission_cli | tar.gz |
| `windows-latest` | x86_64-pc-windows-msvc | fission_cli.exe | zip |

---

## Artifact Management

### Storage structure

```
artifacts/
├── ci-fast-{github.run_id}/
│   ├── logs/
│   │   ├── lint-format.log
│   │   ├── security-check.log
│   │   ├── build-{platform}.log
│   │   └── test-{module}.log
│   └── test-results.json
│
├── ci-heavy-{github.run_id}/
│   ├── corpus-validation.json
│   ├── test-report.html
│   ├── nir-check/
│   │   ├── per_binary/
│   │   ├── summary.json
│   └── regression-report.md
│
└── releases/
    └── v{version}/
        ├── fission-linux-x64.tar.gz
        ├── fission-macos-arm64.tar.gz
        ├── fission-windows-x64.zip
        ├── SHA256SUMS
        └── RELEASE-NOTES.md
```

### Retention periods
- Fast Gate: 7 days
- Heavy Validation: 14 days
- Release: indefinite

---

## Caching Strategy

### Rust Build Cache (`Swatinem/rust-cache@v2`)
- Automatically caches `$CARGO_HOME`
- Key: `rust-${{ matrix.os }}-${{ matrix.target }}`
- Retention: 14 days



---

## Retry Policy

- Network errors (cargo): 3 retries
- System errors (GitHub Actions): no auto-retry, manual trigger required
- Timeouts: no retry (job splitting recommended)

---

## Security Policy

### Dependency Checks
- `cargo deny check all`: runs on all PRs
  - Known CVE validation
  - License verification (AGPL-3.0-or-later)
  - Source verification

### Permission Policy
```yaml
permissions:
  contents: read           # Fast Gate, Heavy
  contents: write          # Release (tag upload)
  pull-requests: read      # PR info read
```

---

## Monitoring & Failure Response

### Failure notifications
- PR: GitHub comment with auto report
- Main: Slack/email alert (optional)
- Release: manual review

### Common failure causes

| Symptom | Cause | Solution |
|---------|-------|----------|
| `cargo fmt` failure | Formatting error | Run `cargo fmt --all` |
| `clippy` warning | Code style | Run `cargo clippy --fix` |
| Test failure | Logic error | Run `cargo test --all` locally |
| Build failure | Compile error | Run `cargo build --all` locally |


---

## Customization Guide

### Adding new jobs
1. Create `.github/workflows/reusable-new-task.yml` (must live directly under `.github/workflows/`)
2. Define input parameters (`on.workflow_call.inputs`)
3. Call from main workflow:
   ```yaml
   - uses: ./.github/workflows/reusable-new-task.yml
     with:
       param: value
   ```

### Adjusting timeouts
- Change `timeout-minutes` value
- Update this document table accordingly
- Note: timeout increase = cost increase

### Adding platforms
1. Add new platform to `cd.yml` matrix
2. Handle artifact packaging in `build-cli.yml`
3. Update this document

---

## Version Management

- **Document version**: 1.0
- **Last updated**: 2026-04-21
- **Next review**: 2026-06-21 (2 months)
