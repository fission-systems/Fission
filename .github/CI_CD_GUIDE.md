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
│  - ci.yml (Fast Gate · L0)             │
│  - ci-heavy.yml (Heavy · L1)           │
│  - release-e2e.yml (Release E2E · L2)  │
│  - release-tag.yml (tag after L0–L2)   │
│  - cd.yml (CD Release · L3 + utils)    │
│  - publish-utils-assets.yml (assets-v*)│
│  - ci-cd-monitor.yml (Status)          │
│  - fuzz.yml (Fuzzing)                  │
└─────────────────────────────────────────┘
```

**Utils assets:** each SemVer release ships `fission-utils.tar.gz`; CI pins a long-lived `assets-v*` tag via setup-utils. See [`docs/CI_RELEASE_GATES.md`](../docs/CI_RELEASE_GATES.md).

**Release gate policy (conservative):** [`docs/CI_RELEASE_GATES.md`](../docs/CI_RELEASE_GATES.md).

---

## 📋 Pipeline-by-Pipeline Guide

### 1. Fast Gate (ci.yml) 🟢

**When is it triggered?**
- PR creation/update
- Main branch push
- Manual trigger

**Runtime:** depends on path-filter **lane** (see below).

| Lane | Typical trigger | Jobs | Runtime (order of) |
|------|-----------------|------|--------------------|
| `docs` | `docs/`, `wiki/`, `*.md` only | summary short-circuit | ~1 min |
| `scripts` | `scripts/**` only | pass-gate + script syntax | ~2–4 min |
| `ci` | `.github/**` / deny meta only | pass-gate + YAML parse (+ security if deny) | ~2–5 min |
| `rust` (PR) | crates/utils/Cargo | Linux tests + smoke + NIR + lint | ~15–25 min |
| `rust` (main) | same | + macOS/Windows | ~35–45 min |

**Validation steps (rust lane):**
```
✓ Path filter (lane selection)
✓ Security Check (cargo deny + audit)
✓ Lint & Format (rustfmt + clippy, sccache)
✓ Pass-gate / owner-boundary scripts
✓ Core + midend Tests (Linux, single nextest multi -p, sccache)
✓ CLI smoke + NIR regression gate
✓ Multi-OS (main push only)
```

**Success criteria:**
- [ ] All security checks passed (code path)
- [ ] No formatting errors
- [ ] No lint warnings (`-D warnings`)
- [ ] Linux core+midend tests passed
- [ ] CLI smoke + NIR regression gate passed
- [ ] On main: macOS + Windows Fast Gate jobs passed

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
- Every push to `main` (L1 release gate signal)
- Daily at 02:30 UTC (nightly safety net)
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


✓ Automation NIR-Check (~45 min)
  ├─ Quality lane validation
  ├─ Verify regression < 5%
  └─ Generate artifacts
```

**Success criteria:**
- [ ] All Fast Gate criteria included
- [ ] Corpus manifest validation passed
- [ ] Full test suite passed

- [ ] NIR-Check regression < 5%

**If it fails:**
```bash
# 1. Corpus validation failure
cd benchmark/config/benchmark_corpus/
python3 -c "import json; json.load(open('smoke_corpus.json'))"

# 2. Test failure
cargo test --all



---

### 3. Release Tag + E2E (release-tag.yml · release-e2e.yml) 🟠

**Preferred path (conservative):**

1. Wait for **CI Fast Gate** and **CI Heavy** green on the target `main` commit.
2. Actions → **Release Tag (CI green)** → set `tag` / `ref`.
3. Workflow verifies L0+L1, runs **Release E2E** (release CLI + PE smoke + multi-decomp + fast NIR-check without latest promotion), then pushes an annotated tag on the **verified SHA**.
4. Tag push starts **CD Release** (`cd.yml`).

Do **not** auto-tag every `main` push. See [`docs/CI_RELEASE_GATES.md`](../docs/CI_RELEASE_GATES.md).

### 4. Release / CD (cd.yml) 🟡

**When is it triggered?**
- Push of `v*.*.*` tag (or bare `X.Y.Z`)

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

**How to release (preferred):**
```text
Actions → "Release Tag (CI green)" → tag=v0.2.0 ref=main
# → verifies L0+L1, runs L2 E2E, pushes tag → CD builds
# → Release page: https://github.com/sjkim1127/Fission/releases
# → Write release notes (web UI)
```

**Manual tag (not recommended; bypasses L2 unless you run Release E2E first):**
```bash
git tag v0.2.0 <green-sha>
git push origin v0.2.0
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
cargo clippy --workspace -- -D warnings
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
