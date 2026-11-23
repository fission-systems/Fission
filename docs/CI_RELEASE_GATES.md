# CI and release gates

| Gate | Trigger | Required evidence |
|---|---|---|
| Fast Gate | All PR targets, GitHub merge candidates and pushes to main | Change plan from event SHAs, with affected Linux tests, CLI smoke and pre-merge lanes when selected |
| Heavy | Nightly or workflow dispatch | Linux workspace tests, Windows and macOS coverage, MSRV, compile surface and the RustSec advisory check |
| DecBench full sweep | Nightly or workflow dispatch | 24 shards with exact row coverage, per-function results, and comparison with a compatible passing main sweep |
| Release E2E and packages | Intentional release dispatch or SemVer tag push | L0 and L1 on the exact candidate SHA, release CLI E2E and five unpacked platform packages |
| Publish | Successful release candidate run | SHA256 manifest and artifact provenance, complete draft release verified before publication |

## PR and merge queue

`Detect changes` and `CI Summary` are the stable, required check names. Fast Gate
reads the event's exact base/head SHAs and checks the checked-out commit. For
stacked PRs the base is the parent branch named in the event. For a merge group
it is the queue's base and synthetic head. Each main push keeps its own run.
Superseded updates to the same PR can still cancel older PR runs.

Changes to CI definitions run actionlint, the CI tool tests, affected Rust tests
and a CLI smoke run. A change to a lane's own definition also runs that lane:
editing `reusable-premerge.yml`, its smoke script, `scripts/macos/` or the
checked-in fixtures runs every pre-merge lane, and editing the DecBench sweep
(workflow, script, scorer or its pinned requirements) runs the nightly workflow
on one binary, which fails unless decompilation, GED and type scoring all
produced data. Enabled nextest suites fail if they discover no tests. An
enabled lane whose job is skipped, cancelled or failed makes `CI Summary` fail.

The native debugger lane exercises Linux, Windows and macOS processes on hosted
runners. The macOS job enables Developer Tools and signs the test executable.
Run this job on a real PR before making the merge queue required. The solver
lane uses a short differential suite with Z3 4.13.4. Loader changes cover ELF,
PE and Mach-O fixtures, and decompiler changes add a small checked-in corpus.
Packaging changes rehearse all five targets on PRs and merge candidates.

Operator rollout:

1. Land the workflow changes with the existing main protection settings.
2. Confirm a stacked PR publishes both checks. Confirm a native-debugger PR
   actually runs the macOS and Windows process tests.
3. Configure GitHub's merge queue for `main` with the existing required check
   names. Begin with one candidate at a time. Verify the queue reports both
   checks on its synthetic SHA before requiring it for all merges.
4. Set an approval requirement for CI and release policy changes when an
   independent reviewer is available. (The auto-merge workflow is gone; merges
   are explicit.)

Documentation and CLI changes run the marked [evaluation commands](EVALUATION.md)
against the built CLI and validate real JSON responses against the published
schemas. MkDocs diagnostics are compared with the event base. A newly broken local link
fails the required check, including on a merge candidate. See the
[fixture replay guide](VERIFY_REPLAY.md) for the separate native comparison job.

## What a required check may depend on

[ADR 0016](adr/0016-ci-signal-integrity.md) holds the reasoning and the
measurements. In short: a required check depends only on the change. The
RustSec advisory database moves on its own schedule, so a PR checks advisories
when it changes any `Cargo.toml`, `Cargo.lock`, `deny.toml`,
`.cargo/audit.toml`, or `.cargo/config` / `.cargo/config.toml`. Manual Fast Gate
dispatches and nightly Heavy also check advisories. The security lane checks
bans, licenses and sources on Rust or security metadata changes. The sccache
setup fails open. Every push to main runs workspace fmt and clippy.

## Checks on the checks

- Speed: every Rust PR counts instructions executed by its base and head CLIs
  on six fixed DecBench binaries (valgrind cachegrind; wall time on shared
  runners is too noisy) and flags +3% in its summary; nightly `speed.yml`
  compares main with the last good night and fails at +3%. Dispatch it with
  `accept_baseline` to accept a slowdown that is worth it.
- Decompiler changes: a PR that can move output gets an output diff on a
  fixed DecBench sample (about 3,000 functions) in its job summary: the
  functions whose text changed first, then GED and type_match before and after.
  Report-only; open the changed functions before trusting a score.
- Nightly: fuzzing of six targets (ELF, PE, Mach-O, COFF, any format, SLEIGH
  decode) seeded from the loader fixtures; an 8 GiB memory ceiling on the
  eight largest DecBench binaries; decompiler determinism. Weekly: mutation
  testing of lines changed on main, four of 48 rotating slices, report-only.
  Each reports into a `ci-red` issue when it fails.

- Workflows: every third-party action is pinned to a commit SHA (Dependabot
  updates the pins monthly), inputs reach shell code through `env`, and no
  workflow grants `actions: write`. Any workflow change runs actionlint and
  zizmor; a high-severity zizmor finding fails Script Checks.
- The PR fixture lane compares decompiled text across requests for 1, 4 and 4
  workers. The nightly `determinism.yml` requests 1, 4, 4 and 4 workers and
  checks whole DecBench binaries in SHA-256 order until their manifest rows
  reach 5,000. Both set `FISSION_RUST_DECOMP_WORKERS` and verify the CLI's
  reported worker count, capped by the number of functions. Nightly differences
  are reported in a `ci-red` issue and are outside the release gate.
- The PR fixture lane builds the release CLI with overflow checks and debug
  assertions on.

## Scheduled runs report into issues

Heavy, fuzz, determinism and the DecBench sweep end with `reusable-report-red.yml`. While a
workflow is red on main there is one open issue titled `CI red: <workflow>`
with the `ci-red` label; each further red run comments with its SHA and run,
and the first green run closes it. Cancelled runs report nothing. Fuzzing runs
on its schedule or by manual dispatch, restores its corpus from earlier runs,
and attaches crash inputs to the run for 30 days.

## Nightly DecBench

The source is [noelo-lab/decbench-dataset](https://huggingface.co/datasets/noelo-lab/decbench-dataset),
revision `815eb96f130b6d2ef7fa600e04669bf20139bf27`, config `full`.
Its manifest contains 96,103 function rows in 770 binaries across O0, O2 and
O2-noinline. CI downloads only the manifest, binaries and published source CFGs.
Binaries stay non-executable and only pass through static inspection, stripping
and decompilation. The grader reads original symbols and DWARF separately.
Runs disable decompilation and metric caches.

The scorer is upstream DecBench at `0cba66d66e9af0f8555c9844eb2a868440c366c1`,
the last commit before upstream #113 (2026-10-31) stopped counting inlined
callees' variables as a function's type ground truth. The pinned dataset's
published scores predate that change, and only the scorer that produced them can
be compared with them: crontab O2 `usage` has one variable after #113 and 19
before, and every published tool's score for it is a multiple of 1/19. The sweep
refuses to run with any other installed decbench. Move the scorer pin together
with the dataset revision, never alone.

The report checks every expected row, records decompilation status and output
hashes, and reports GED perfect count with total distance and type-match coverage
and scores. It reports per-row changes against the latest passing main sweep
when the corpus and scorer provenance match. A complete passing run establishes
a baseline after a revision or scorer change. Build failures, process crashes,
missing shards and previously successful rows now failing are blocking. Score
deltas are observational and do not change the accepted baseline thresholds.
Byte match is not run. See [the benchmark guide](BENCHMARK_DECBENCH.md).

## Promote a release

1. Select a candidate in main's history. Check for a successful Fast Gate
   **main push run** and a successful Heavy **scheduled or manually dispatched
   run** on the same SHA.
2. Dispatch on main with the version and exact candidate SHA.

   ```bash
   gh workflow run "Release Tag (CI green)" --ref main -f tag=vX.Y.Z -f ref=<candidate-sha>
   ```

3. The workflow runs release E2E and stages all five packages before creating
   any tag itself. A manually pushed SemVer tag enters the same verification
   path and must already resolve to that verified SHA.
4. The publish job verifies the staged digests and generates build provenance.
   It creates the tag if needed, populates a draft, then downloads and verifies
   the draft attachments before publishing. Retry with the same tag and SHA
   after a failed publication.

After validating the workflow, configure tag protection and the `release`
environment approval policy in GitHub. Enable immutable releases when the
draft-to-publish flow has been tested. A workflow change does not modify these
repository settings.

## Resource bundle (`utils/`) assets

Runtime data lives under [`utils/`](https://github.com/fission-systems/Fission/tree/main/utils) (Sleigh specs, signatures, ghidra-data).
It **is** checked into git as of 2026-08-19. Only `utils/source/` (the inputs the
packed artifacts are built from) stays ignored. Packing the databases into `.fpk`
brought the tree down to a size git can carry, which removed the `assets-v*` bundle
and the class of failure it caused: `assets-v3` shipped 1,048 of 3,468 files while
its inventory claimed all 3,468, and CI ran against that truncated tree for four days
without anyone being able to see it.

| Asset | How it is published | Who uses it |
|-------|---------------------|-------------|
| **Inside platform archives** (`fission-<target>.tar.gz` / `.zip`) | `cd.yml` copies a verified `utils/` into each archive as `share/fission/`, beside `bin/fission_cli` | Homebrew, `install.sh`, anyone unpacking a release |
| **`fission-resources.tar.gz`** | The same `share/fission/` contents on their own | `cargo install` users, who unpack it into `~/.cargo/share/fission` |
| **The `utils/` tree itself** | Committed; changed like any other source | CI — [`.github/actions/setup-utils`](https://github.com/fission-systems/Fission/tree/main/.github/actions/setup-utils) verifies the checkout rather than downloading anything |

Rules:

1. Platform packages **fail the release** if `utils/sleigh-specs` is missing/incomplete.
2. `utils/` reaches consumers through the clone, the platform archive, or `fission-resources.tar.gz`; all three are cut from the same tagged checkout.
3. Regenerate packed artifacts (`.fpk`) with the scripts in [`scripts/`](https://github.com/fission-systems/Fission/tree/main/scripts) and commit them; there is no separate publish step for CI to pick them up.
4. Every archive is smoke-tested the way a user runs it -- unpacked outside the checkout and invoked from an unrelated directory -- by [`scripts/release/smoke_package.sh`](https://github.com/fission-systems/Fission/blob/main/scripts/release/smoke_package.sh). The v0.2.x archives put `utils/` beside the binary, a place the binary never looks, and only worked from inside a source tree.

## Related files

- [`.github/CI_CD_GUIDE.md`](https://github.com/fission-systems/Fission/blob/main/.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/release-tag.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/cd.yml)
- [`utils/MANIFEST.md`](https://github.com/fission-systems/Fission/blob/main/utils/MANIFEST.md)
