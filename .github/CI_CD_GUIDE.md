# CI/CD Pipeline User Guide

## Overview

Fission's CI/CD pipeline is designed with **standardization**, **reusability**, and **scalability** at its core.

---

## 🚀 Quick Start

### CI/CD Pipeline Architecture

```
┌─────────────────────────────────────────┐
│  Reusable workflows                     │
│  (.github/workflows/reusable-*.yml)    │
│  GitHub requires workflow_call files    │
│  at the workflows directory root        │
│                                         │
│  - reusable-setup-rust.yml             │
│  - reusable-security-check.yml        │
│  - reusable-lint-format.yml           │
│  - reusable-run-tests.yml             │
│  - reusable-cli-smoke.yml              │
│  - reusable-build-cli.yml             │
│  - reusable-build-tauri.yml           │
│  - reusable-nir-check.yml             │
│  - reusable-corpus-validation.yml     │
│  - reusable-benchmark.yml             │
│  - reusable-upload-artifacts.yml       │
└─────────────────────────────────────────┘
         ↓ (uses)
┌─────────────────────────────────────────┐
│  Main Workflows                         │
│  (.github/workflows/)                   │
│                                         │
│  - ci.yml (Fast Gate)                  │
│  - ci-heavy.yml (Heavy Validation)     │
│  - cd.yml (Release)                    │
│  - ci-cd-monitor.yml (Status)          │
│  - fuzz.yml (Fuzzing)                  │
└─────────────────────────────────────────┘
```

---

## 📋 Pipeline-by-Pipeline Guide

### 1. Fast Gate (ci.yml) 🟢

**When is it triggered?**
- PR creation/update
- Main branch push
- Manual trigger

**Runtime:** ~40 minutes

**Validation steps:**
```
✓ Security Check
  ├─ cargo deny (CVE, licenses, sources)
  ├─ cargo audit (known vulnerabilities)
  └─ npm audit (Tauri dependencies)

✓ Lint & Format
  ├─ rustfmt check
  └─ clippy (0 warnings)

✓ Core Tests
  ├─ fission-pcode
  ├─ fission-automation
  └─ fission-loader

✓ CLI smoke (Linux, after core tests)
  └─ `fission_cli` on `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe` (tracked C source cross-compiles with MinGW on the runner; `.exe` is gitignored) — `info`, `list --json`, then `decomp --json` at an address from the list or the documented fallback

✓ Platform Builds
  ├─ Linux (ubuntu-latest)
  ├─ Windows (windows-latest)
  └─ macOS (macos-latest)
```

**Success criteria:**
- [ ] All security checks passed
- [ ] No formatting errors
- [ ] No lint warnings (`-D warnings`)
- [ ] All tests passed
- [ ] CLI smoke passed (PE sample commands)
- [ ] All platforms built successfully

**If it fails:**
```bash
# Reproduce locally
cargo fmt --all
cargo clippy --all -- -D warnings
cargo test -p fission-pcode

# After fixing issues
git add .
git commit -m "fix: resolve CI failures"
git push
```

---

### 2. Heavy Validation (ci-heavy.yml) 🔵

**When is it triggered?**
- After merge to main
- Daily at 02:30 UTC (nightly)
- Manual trigger

**Runtime:** ~90 minutes

**Validation steps:**
```
✓ Corpus Validation (~15 min)
  └─ Validate benchmark binary manifests

✓ Full Test Suite (~25 min)
  ├─ Full workspace tests
  └─ fission-static tests

✓ Heavy Platform Tests (~20 min)
  ├─ Windows release build
  └─ macOS release build

✓ Tauri Build (~15 min)
  └─ Desktop UI build

✓ Automation NIR-Check (~45 min)
  ├─ Quality lane validation
  ├─ Verify regression < 5%
  └─ Generate artifacts
```

**Success criteria:**
- [ ] All Fast Gate criteria included
- [ ] Corpus manifest validation passed
- [ ] Full test suite passed
- [ ] Tauri build successful
- [ ] NIR-Check regression < 5%

**If it fails:**
```bash
# 1. Corpus validation failure
cd benchmark/config/benchmark_corpus/
python3 -c "import json; json.load(open('smoke_corpus.json'))"

# 2. Test failure
cargo test --all

# 3. Tauri build failure
cd crates/fission-tauri
npm ci && npm run build
```

---

### 3. Release (cd.yml) 🟡

**When is it triggered?**
- Push of `v*.*.*` tag

**Runtime:** ~45 minutes

**Build targets:**
```
- Linux:   x86_64-unknown-linux-gnu
- macOS:   aarch64-apple-darwin
- Windows: x86_64-pc-windows-msvc
```

**Success criteria:**
- [ ] All 3 platform binaries created
- [ ] Each binary packaged
- [ ] SHA256 checksums generated
- [ ] Uploaded to GitHub Release

**How to release:**
```bash
# 1. Create version tag
git tag v0.2.0
git push origin v0.2.0

# 2. GitHub Actions builds automatically
# → Release page: https://github.com/sjkim1127/Fission/releases

# 3. Write release notes (web UI)
```

---

## 🔧 Customizing Workflows

### Using Reusable Workflows

**Example: Add a new test job**

```yaml
# .github/workflows/ci.yml
test-new-module:
  name: 🧪 Test New Module
  needs: [security-check, lint-format]
  uses: ./.github/workflows/reusable-run-tests.yml
  with:
    os: ubuntu-latest
    crates: "fission-newmodule"
    profile: debug
    coverage: false
```

**Available reusable workflows:**

| Workflow | Inputs | Purpose |
|----------|--------|---------|
| `reusable-setup-rust.yml` | os, target, components | Initialize Rust environment |
| `reusable-security-check.yml` | check_npm | Security validation |
| `reusable-lint-format.yml` | os, exclude_crates | Code style checks |
| `reusable-run-tests.yml` | os, crates, profile, coverage | Run tests |
| `reusable-build-cli.yml` | os, target, profile | Build CLI |
| `reusable-build-tauri.yml` | os | Build Tauri |
| `reusable-nir-check.yml` | run_profile, functions_limit | NIR validation |
| `reusable-corpus-validation.yml` | - | Corpus validation |
| `reusable-benchmark.yml` | (see workflow) | Full benchmark lane |
| `reusable-upload-artifacts.yml` | artifact_name, paths | Upload artifacts |

---

## 📊 Monitoring & Debugging

### Check workflow status

```bash
# View recent runs
gh run list --workflow ci.yml -L 5

# View specific run logs
gh run view RUN_ID

# Download logs
gh run download RUN_ID
```

### Common errors & solutions

| Error | Cause | Solution |
|-------|-------|----------|
| `cargo fmt` failure | Formatting mismatch | Run `cargo fmt --all` |
| `clippy` warning | Code style issue | Run `cargo clippy --fix` |
| Test failure | Logic error | Run `cargo test --all -- --nocapture` locally |
| Build timeout | Build time exceeded | Split jobs or increase timeout |
| Tauri build failure | npm dependency issue | Run `npm ci --prefer-offline` |
| Network error | Temporary GitHub API issue | Manual retry |

---

## 🎯 Best Practices

### 1. Get fast feedback

```bash
# Validate locally before PR submission
cargo fmt --all
cargo clippy --all -- -D warnings
cargo test -p fission-pcode -p fission-automation

# Takes ~5 minutes (CI takes 40 minutes)
```

### 2. Minimize CI failures

✅ **Good**
```bash
git commit -m "fix(pcode): handle edge case in lowering"
```

❌ **Bad**
```bash
git commit -m "wip: various fixes"
git commit -m "fix formatting"
git commit -m "add test"
```

### 3. Managing large changes

For big refactors:
1. Break into small units
2. One logical change per PR
3. Test CI pipeline first
4. Merge after review

---

## 🚨 Troubleshooting

### CI fails frequently

**Step 1: Reproduce locally**
```bash
# Simulate Fast Gate
cargo fmt --all -- --check
cargo clippy --workspace --exclude fission-tauri -- -D warnings
cargo test -p fission-pcode -p fission-automation
```

**Step 2: Check detailed logs**
```bash
# GitHub UI: Actions → Workflow Run → Job → Step
# Or CLI
gh run view RUN_ID --log
```

**Step 3: Consult documentation**
- [ci-cd-standards.md](./ci-cd-standards.md) - Standard definitions
- [CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guide

---

## 📚 References

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Rust Cargo Documentation](https://doc.rust-lang.org/cargo/)
- [Fission AGENTS.md](../AGENTS.md) - Project structure
- [Fission CONTRIBUTING.md](../CONTRIBUTING.md) - Contribution guide

---

## 🔗 Related Files

```
.github/
├── ci-cd-standards.md          ← Standard definitions
├── CI_CD_GUIDE.md              ← This file
├── workflows/
│   ├── ci.yml                  ← Fast Gate
│   ├── ci-heavy.yml            ← Heavy Validation
│   ├── cd.yml                  ← Release
│   ├── ci-cd-monitor.yml       ← Monitoring
│   ├── fuzz.yml                ← Fuzzing
│   ├── reusable-*.yml        ← workflow_call reusables (repo root rule)
│   └── ...
```

---

## 💡 Feedback & Improvements

Suggestions for CI/CD pipeline improvements:
1. [Create GitHub Issue](https://github.com/sjkim1127/Fission/issues)
2. Label: `infrastructure`, `ci-cd`
3. Update this guide alongside changes

---

**Last updated:** 2026-04-21
**Document version:** 1.0
