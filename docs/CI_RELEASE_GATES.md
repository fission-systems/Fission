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
| **L0 Fast Gate** | [`ci.yml`](../.github/workflows/ci.yml) | PR + every `main` push | Lint, security, core+midend tests, CLI smoke, NIR gate. **PR = Linux-first**; macOS test smoke on `main` push. Windows release tests remain in L1. Docs/wiki-only short-circuits |
| **L1 Heavy** | [`ci-heavy.yml`](../.github/workflows/ci-heavy.yml) | Nightly + manual dispatch | Full Linux workspace tests, Windows tests and native debugger, macOS release CLI build, MSRV, and non-blocking coverage. Dispatch it for the release candidate SHA before tagging. |
| **L2 Release E2E** | [`release-e2e.yml`](../.github/workflows/release-e2e.yml) | Before tag (and optional dispatch) | Release-profile CLI + fixed PE smoke + raw-pcode + multi-function decomp |
| **Tag** | [`release-tag.yml`](../.github/workflows/release-tag.yml) | Manual `workflow_dispatch` only | Requires L0 + L1 green on the SHA, runs L2, then creates/pushes tag |
| **L3 CD** | [`cd.yml`](../.github/workflows/cd.yml) | Tag push `v*.*.*` / `X.Y.Z` | Multi-platform CLI archives (each includes `utils/`) → GitHub Release |

```text
main push ──► L0 Fast Gate
nightly or manual dispatch on candidate ref ──► L1 Heavy
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

**Does not cover:**

- Full fission-benchmark docker corpus or Pages promotion
- Official semantic leaderboard / `release_promotion_allowed` dashboard claims
  (still manual / evaluation path in [`docs/RELEASE.md`](RELEASE.md) § quality)

## Operator runbook (tag a release)

1. Land changes on `main` and wait for **CI Fast Gate** to pass on that commit.
2. Run **CI Heavy Validation** manually on the release candidate ref and wait for
   it to pass on the same commit. A nightly run counts when `main` has not moved
   since that run.
3. Actions → **Release Tag (CI green)** → set `tag` (`v0.1.4`) and `ref` (`main` or SHA),
   or CLI:
   `gh workflow run "Release Tag (CI green)" -f tag=v0.1.4 -f ref=main`
4. Workflow verifies L0+L1, runs L2, pushes the annotated tag, then runs
   `gh workflow run "CD Release" -f tag=v0.1.4` (same-repo `workflow_dispatch`
   is allowed to chain with `GITHUB_TOKEN`; bare tag push from the token is not).
5. **CD Release** builds Linux/macOS/Windows assets automatically.
6. Edit GitHub Release notes; complete remaining checklist in
   [`docs/RELEASE.md`](RELEASE.md).

Manual CD re-run for an existing tag:

```bash
gh workflow run "CD Release" -f tag=v0.1.4
```

Official benchmark bake (after release assets exist):

```bash
# Preferred: bake GHCR then auto-chain official Benchmark & Deploy
gh workflow run "Publish Images" --repo fission-systems/fission-benchmark \
  -f services=fission -f fission_version=v0.1.4

# Or repository_dispatch (Publish Images only; chains benchmark itself):
gh api repos/fission-systems/fission-benchmark/dispatches --input - <<'EOF'
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

## L1 execution (nightly and release candidates)

Heavy does not run on every `main` push. Nightly and manual runs cover:

- Full Linux workspace tests (including `fission-static`)
- Windows release tests and native Win32 debugger tests
- macOS release CLI build
- MSRV
- Coverage (non-blocking)

For a release, dispatch Heavy on the candidate branch or ref and wait for that
run to finish before starting **Release Tag (CI green)**. The tag workflow
accepts only a successful Heavy run whose head SHA exactly matches the target.
A nightly run is sufficient when the candidate has not moved since it ran.

**L0 performance notes (2026-07-17):**

- Path-filter **lanes**: `docs` | `scripts` | `ci` | `rust`
  - `docs` — short-circuit green
  - `scripts` — pass-gate + Python/shell syntax only
  - `ci` — pass-gate + workflow YAML parse (+ security if `deny.toml`/dependabot)
  - `rust` — full Linux Fast Gate (macOS test smoke on `main` push)
- PR Fast Gate no longer runs macOS/Windows tests (macOS smoke is on main L0;
  Windows release tests are on L1).
- Fast Gate no longer performs separate macOS/Windows CLI release builds on `main`;
  those release builds remain in L1 Heavy while L0 keeps the macOS test smoke signal.
- `reusable-run-tests` runs multi-package nextest in **one** cargo process and
  skips webkit/GTK sysdeps unless GUI packages are required.
- **sccache** (GitHub Actions backend) on lint / test / CLI build reusables via
  [`.github/actions/setup-sccache`](../.github/actions/setup-sccache).

`release-tag.yml` requires a successful `ci-heavy.yml` run on the target SHA.
Scheduled and manually dispatched runs qualify; a run on an earlier commit does
not satisfy the release gate.

## Escape hatches

- **Emergency hot-fix tag without waiting for heavy:** not supported by the
  default gate. Prefer fixing heavy or using a documented exception in release
  notes only after an explicit ops decision (temporary workflow input may be
  added later; default remains full gates).
- **Local docker benchmark** remains a quality-loop tool; never promotes to
  official latest (see [`docs/BENCHMARK_DOCKER.md`](BENCHMARK_DOCKER.md)).

## Resource bundle (`utils/`) assets

Runtime data lives under [`utils/`](../utils/) (Sleigh specs, signatures, ghidra-data).
It **is** checked into git as of 2026-08-19. Only `utils/source/` (the inputs the
packed artifacts are built from) stays ignored. Packing the databases into `.fpk`
brought the tree down to a size git can carry, which removed the `assets-v*` bundle
and the class of failure it caused: `assets-v3` shipped 1,048 of 3,468 files while
its inventory claimed all 3,468, and CI ran against that truncated tree for four days
without anyone being able to see it.

| Asset | How it is published | Who uses it |
|-------|---------------------|-------------|
| **Inside platform archives** (`fission-linux-x64.tar.gz`, …) | `cd.yml` copies a verified `utils/` into each OS package | End-user installs that unpack the full release |
| **The `utils/` tree itself** | Committed; changed like any other source | CI — [`.github/actions/setup-utils`](../.github/actions/setup-utils) verifies the checkout rather than downloading anything |

Rules:

1. Platform packages **fail the release** if `utils/sleigh-specs` is missing/incomplete.
2. There is no standalone resource bundle any more; `utils/` reaches consumers through the clone or through the platform archive.
3. Regenerate packed artifacts (`.fpk`) with the scripts in [`scripts/`](../scripts) and commit them; there is no separate publish step for CI to pick them up.

## Related files

- [`.github/CI_CD_GUIDE.md`](../.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/release-tag.yml`](../.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](../.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)
- [`utils/MANIFEST.md`](../utils/MANIFEST.md)
