# CI and release gates

| Gate | Trigger | Required evidence |
|---|---|---|
| Fast Gate | All PR targets, GitHub merge candidates and pushes to main | Change plan from event SHAs, affected Linux tests and CLI smoke, plus the selected pre-merge lanes |
| Heavy | Nightly or workflow dispatch | Linux workspace tests, Windows and macOS coverage, MSRV and compile surface |
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
and a CLI smoke run. Enabled nextest suites fail if they discover no tests. An
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
4. Update or disable `auto-merge-approved-prs.yml` once the queue is active so
   merge strategy is controlled by the queue. Set an approval requirement for
   CI and release policy changes when an independent reviewer is available.

## Nightly DecBench

The source is [noelo-lab/decbench-dataset](https://huggingface.co/datasets/noelo-lab/decbench-dataset),
revision `815eb96f130b6d2ef7fa600e04669bf20139bf27`, config `full`.
Its manifest contains 96,103 function rows in 770 binaries across O0, O2 and
O2-noinline. CI downloads only the manifest, binaries and published source CFGs.
Binaries stay non-executable and only pass through static inspection, stripping
and decompilation. The grader reads original symbols and DWARF separately.
Runs disable decompilation and metric caches.

The report checks every expected row, records decompilation status and output
hashes, and reports GED perfect count with total distance and type-match coverage
and scores. It reports per-row changes against the latest passing main sweep
when the corpus and scorer provenance match. A complete passing run establishes
a baseline after a revision or scorer change. Build failures, process crashes,
missing shards and previously successful rows now failing are blocking. Score
deltas are observational and do not change the accepted baseline thresholds.
Byte match is not run. See [the benchmark guide](BENCHMARK_DECBENCH.md).

## Promote a release

1. Wait for a successful Fast Gate **main push run** and successful Heavy on
   the same candidate SHA. Heavy can be dispatched on a named ref at that SHA.
2. Dispatch on main with the version and exact candidate SHA.

   ```bash
   gh workflow run "Release Tag (CI green)" --ref main -f tag=vX.Y.Z -f ref=<candidate-sha>
   ```

3. The workflow runs release E2E and stages all five packages before creating
   any tag itself. A manually pushed SemVer tag enters the same verification
   path and must already resolve to that verified SHA.
4. The publish job downloads those exact artifacts, verifies their digests,
   generates provenance attestations and populates a draft. It downloads the
   draft attachments and verifies them before publishing. For a failed run,
   rerun with the same tag and SHA. Published releases are not overwritten.

After validating the workflow, configure tag protection and the `release`
environment approval policy in GitHub. Enable immutable releases when the
draft-to-publish flow has been tested. A workflow change does not modify these
repository settings.

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
| **Inside platform archives** (`fission-<target>.tar.gz` / `.zip`) | `cd.yml` copies a verified `utils/` into each archive as `share/fission/`, beside `bin/fission_cli` | Homebrew, `install.sh`, anyone unpacking a release |
| **`fission-resources.tar.gz`** | The same `share/fission/` contents on their own | `cargo install` users, who unpack it into `~/.cargo/share/fission` |
| **The `utils/` tree itself** | Committed; changed like any other source | CI — [`.github/actions/setup-utils`](../.github/actions/setup-utils) verifies the checkout rather than downloading anything |

Rules:

1. Platform packages **fail the release** if `utils/sleigh-specs` is missing/incomplete.
2. `utils/` reaches consumers through the clone, the platform archive, or `fission-resources.tar.gz`; all three are cut from the same tagged checkout.
3. Regenerate packed artifacts (`.fpk`) with the scripts in [`scripts/`](../scripts) and commit them; there is no separate publish step for CI to pick them up.
4. Every archive is smoke-tested the way a user runs it -- unpacked outside the checkout and invoked from an unrelated directory -- by [`scripts/release/smoke_package.sh`](../scripts/release/smoke_package.sh). The v0.2.x archives put `utils/` beside the binary, a place the binary never looks, and only worked from inside a source tree.

## Related files

- [`.github/CI_CD_GUIDE.md`](../.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/release-tag.yml`](../.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](../.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](../.github/workflows/cd.yml)
- [`utils/MANIFEST.md`](../utils/MANIFEST.md)
