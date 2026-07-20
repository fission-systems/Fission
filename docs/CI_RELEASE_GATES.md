# CI / CD release gates (conservative)

**Last updated:** 2026-07-17

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
| **L0 Fast Gate** | [`ci.yml`](../.github/workflows/ci.yml) | PR + every `main` push | Lint, security, core+midend tests, CLI smoke, NIR gate. **PR = Linux-first**; multi-OS on `main` push. Docs/wiki-only short-circuits |
| **L1 Heavy** | [`ci-heavy.yml`](../.github/workflows/ci-heavy.yml) | Every `main` push + nightly + dispatch | **Push:** release-critical crates, platforms, NIR-check, MSRV. **Nightly/dispatch:** also full workspace tests, Miri, coverage |
| **L2 Release E2E** | [`release-e2e.yml`](../.github/workflows/release-e2e.yml) | Before tag (and optional dispatch) | Release-profile CLI + fixed PE smoke + raw-pcode + multi-function decomp |
| **Tag** | [`release-tag.yml`](../.github/workflows/release-tag.yml) | Manual `workflow_dispatch` only | Requires L0 + L1 green on the SHA, runs L2, then creates/pushes tag |
| **L3 CD** | [`cd.yml`](../.github/workflows/cd.yml) | Tag push `v*.*.*` / `X.Y.Z` | Multi-platform CLI archives (each includes `utils/`) + standalone **`fission-utils.tar.gz`** → GitHub Release |

```text
main push ──► L0 Fast Gate ──► L1 Heavy (async)
                    │
                    ▼
     Actions / gh workflow run "Release Tag (CI green)"
                    │
                    ├─ verify L0 success (push event, that SHA)
                    ├─ verify L1 success (that SHA; any event)
                    ├─ run L2 Release E2E on that SHA
                    ├─ annotated tag push
                    └─ gh workflow run "CD Release" -f tag=vX.Y.Z  ──► L3 CD
                       (GITHUB_TOKEN tag pushes do not re-fire on.push CD;
                        workflow_dispatch is the reliable path)
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
2. Actions → **Release Tag (CI green)** → set `tag` (`v0.1.4`) and `ref` (`main` or SHA),
   or CLI:
   `gh workflow run "Release Tag (CI green)" -f tag=v0.1.4 -f ref=main`
3. Workflow verifies L0+L1, runs L2, pushes the annotated tag, then runs
   `gh workflow run "CD Release" -f tag=v0.1.4` (same-repo `workflow_dispatch`
   is allowed to chain with `GITHUB_TOKEN`; bare tag push from the token is not).
4. **CD Release** builds Linux/macOS/Windows assets automatically.
5. Edit GitHub Release notes; complete remaining checklist in
   [`docs/RELEASE.md`](RELEASE.md).

Manual CD re-run for an existing tag:

```bash
gh workflow run "CD Release" -f tag=v0.1.4
```

Official benchmark bake (after release assets exist):

```bash
# Preferred: bake GHCR then auto-chain official Benchmark & Deploy
gh workflow run "Publish Images" --repo sjkim1127/fission-benchmark \
  -f services=fission -f fission_version=v0.1.4

# Or repository_dispatch (Publish Images only; chains benchmark itself):
gh api repos/sjkim1127/fission-benchmark/dispatches --input - <<'EOF'
{
  "event_type": "fission-release",
  "client_payload": { "fission_version": "v0.1.4" }
}
EOF
```

Optional: set repo secret `FISSION_BENCHMARK_DISPATCH_TOKEN` (PAT with
`actions:write` on `fission-benchmark`) so **Release Tag** auto-runs the bake
after CD is queued.

Optional: run **Release E2E Gate** alone (dispatch) to pre-validate a SHA without tagging.

## L1 job split (main push vs extended)

On **`push` to `main`**, Heavy runs the **release-critical** set only (so a green
L1 is achievable and meaningful for decompiler releases):

- Linux (single nextest process): `fission-core`, midend crates, `loader`,
  `analysis-db`, `pcode`, `static`, `decompiler`, `automation`, `cli`,
  `signatures`
- L0 Fast Gate Linux also runs `fission-analysis-db` (typed program metadata;
  ADR 0010) so PRs catch snapshot regressions without waiting for L1
- Windows release tests (midend-structuring/pcode/decompiler/automation)
- macOS release CLI build
- Automation NIR-check (`--no-update-latest`)
- MSRV

**L0 performance notes (2026-07-17):**

- Path-filter **lanes**: `docs` | `scripts` | `ci` | `rust`
  - `docs` — short-circuit green
  - `scripts` — pass-gate + Python/shell syntax only
  - `ci` — pass-gate + workflow YAML parse (+ security if `deny.toml`/dependabot)
  - `rust` — full Linux Fast Gate (multi-OS on `main` push)
- PR Fast Gate no longer runs macOS/Windows matrices (covered on `main` L0 + L1).
- `reusable-run-tests` runs multi-package nextest in **one** cargo process and
  skips webkit/GTK sysdeps unless GUI packages are required.
- **sccache** (GitHub Actions backend) on lint / test / CLI build reusables via
  [`.github/actions/setup-sccache`](../.github/actions/setup-sccache).

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

## Resource bundle (`utils/`) assets

Runtime data lives under [`utils/`](../utils/) (Sleigh specs, signatures, ghidra-data).
It is **not** checked into git at all — `.gitignore` excludes `utils/`, and the tree is
populated from the `fission-utils.tar.gz` release asset (see table below), not Git LFS.

| Asset | How it is published | Who uses it |
|-------|---------------------|-------------|
| **Inside platform archives** (`fission-linux-x64.tar.gz`, …) | `cd.yml` copies a verified `utils/` into each OS package | End-user installs that unpack the full release |
| **`fission-utils.tar.gz` on the SemVer release** | `cd.yml` job `publish-utils-bundle` (once per tag) | Offline installs; pin utils to the same version as the CLI |
| **`fission-utils.tar.gz` on `assets-v*`** | [`publish-utils-assets.yml`](../.github/workflows/publish-utils-assets.yml) (manual) | CI [`.github/actions/setup-utils`](../.github/actions/setup-utils) default pin (`assets-v1`) |

Rules:

1. Platform packages **fail the release** if `utils/sleigh-specs` is missing/incomplete.
2. Prefer **version-matched** `fission-utils.tar.gz` on the same `vX.Y.Z` release for production installs.
3. Bump or refresh `assets-v1` only when CI/bootstrap should pick up new specs for all branches (not every SemVer).

## Related files

- [`.github/CI_CD_GUIDE.md`](../.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/release-tag.yml`](../.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](../.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)
- [`.github/workflows/publish-utils-assets.yml`](../.github/workflows/publish-utils-assets.yml)
- [`utils/MANIFEST.md`](../utils/MANIFEST.md)
