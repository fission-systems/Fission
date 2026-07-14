# CI / CD release gates (conservative)

**Last updated:** 2026-07-15

This document is the policy source for how Fission promotes a git commit to a
**SemVer tag** and a **GitHub Release**. It aligns automation with
[`docs/RELEASE.md`](RELEASE.md) and [`docs/VERSIONING.md`](VERSIONING.md).

## Design principles

1. **No auto-tag on every `main` push.** Tags are intentional promotions.
2. **Layered gates:** fast feedback on PR/push; thick checks before tag/CD.
3. **Semantic quality ≠ official benchmark ranking.** Release E2E is go/stop on
   fixed fixtures; it must not publish to Pages / `results/latest.json`.
4. **Documented gates must be machine-enforced** where practical.

## Layers

| Layer | Workflow | When | Role |
|-------|----------|------|------|
| **L0 Fast Gate** | [`ci.yml`](../.github/workflows/ci.yml) | PR + every `main` push | Lint, security, core tests, CLI smoke, NIR regression gate, multi-OS |
| **L1 Heavy** | [`ci-heavy.yml`](../.github/workflows/ci-heavy.yml) | Every `main` push + nightly + dispatch | **Push:** release-critical crates, platforms, NIR-check, MSRV. **Nightly/dispatch:** also full workspace tests, Miri, coverage |
| **L2 Release E2E** | [`release-e2e.yml`](../.github/workflows/release-e2e.yml) | Before tag (and optional dispatch) | Release-profile CLI + fixed PE smoke + raw-pcode + multi-function decomp |
| **Tag** | [`release-tag.yml`](../.github/workflows/release-tag.yml) | Manual `workflow_dispatch` only | Requires L0 + L1 green on the SHA, runs L2, then creates/pushes tag |
| **L3 CD** | [`cd.yml`](../.github/workflows/cd.yml) | Tag push `v*.*.*` / `X.Y.Z` | Multi-platform release binaries → GitHub Release |

```text
main push ──► L0 Fast Gate ──► L1 Heavy (async)
                    │
                    ▼
     Actions → "Release Tag (CI green)"
                    │
                    ├─ verify L0 success (push event, that SHA)
                    ├─ verify L1 success (that SHA; any event)
                    ├─ run L2 Release E2E on that SHA
                    └─ annotated tag push ──► L3 CD Release
```

## What Release E2E covers (and does not)

**Covers (blocking for tag):**

- `cargo build -p fission-cli --locked --release`
- MinGW-built PE fixture from [`.github/fixtures/test_functions.c`](../.github/fixtures/test_functions.c)
- `info`, `list --json`, `decomp --json` (validated), `raw-pcode` at a listed address
- Decompile several listed functions (multi-addr smoke)
- Fast automation NIR-check lane with `--no-update-latest` (no baseline promotion)

**Does not cover:**

- Full fission-benchmark docker corpus or Pages promotion
- Official semantic leaderboard / `release_promotion_allowed` dashboard claims
  (still manual / evaluation path in [`docs/RELEASE.md`](RELEASE.md) § quality)

## Operator runbook (tag a release)

1. Land changes on `main`; wait for **CI Fast Gate** and **CI Heavy Validation**
   green on that commit.
2. Actions → **Release Tag (CI green)** → set `tag` (`v0.1.3`) and `ref` (`main` or SHA).
3. Workflow verifies L0+L1, runs L2, pushes the tag.
4. **CD Release** builds Linux/macOS/Windows assets automatically.
5. Edit GitHub Release notes; complete remaining checklist in
   [`docs/RELEASE.md`](RELEASE.md).

Optional: run **Release E2E Gate** alone (dispatch) to pre-validate a SHA without tagging.

## L1 job split (main push vs extended)

On **`push` to `main`**, Heavy runs the **release-critical** set only (so a green
L1 is achievable and meaningful for decompiler releases):

- Linux: `fission-core`, `loader`, `pcode`, `decompiler`, `automation`, `cli`,
  `analysis-db`, `signatures`, plus `fission-static`
- Windows release tests (pcode/decompiler/automation)
- macOS release CLI build
- Automation NIR-check (`--no-update-latest`)
- MSRV

On **nightly schedule** or **workflow_dispatch**, Heavy also runs:

- Full Linux workspace tests (excluding GUI `fission-dioxus`)
- Miri (soft environmental issues may still fail until isolation is fixed)
- Coverage (non-blocking)

`release-tag.yml` requires any successful `ci-heavy.yml` run on the SHA, so a
green **main-push** Heavy is sufficient to tag (nightly extended failures do
not block once that SHA already has a green push Heavy).

## Escape hatches

- **Emergency hot-fix tag without waiting for heavy:** not supported by the
  default gate. Prefer fixing heavy or using a documented exception in release
  notes only after an explicit ops decision (temporary workflow input may be
  added later; default remains full gates).
- **Local docker benchmark** remains a quality-loop tool; never promotes to
  official latest (see [`docs/BENCHMARK_DOCKER.md`](BENCHMARK_DOCKER.md)).

## Related files

- [`.github/CI_CD_GUIDE.md`](../.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/release-tag.yml`](../.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](../.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)
