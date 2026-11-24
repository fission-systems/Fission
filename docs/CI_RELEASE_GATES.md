# CI and release gates

| Gate | Trigger | Required evidence |
|---|---|---|
| Fast Gate | All PR targets, GitHub merge candidates and pushes to main | Change plan from event SHAs, with affected Linux tests, CLI smoke and pre-merge lanes when selected |
| Heavy | Nightly or workflow dispatch | Linux workspace tests, Windows and macOS coverage, MSRV, compile surface and the RustSec advisory check |
| DecBench full sweep | Nightly or workflow dispatch | 24 shards with exact row coverage, per-function results, and comparison with a compatible passing main sweep |
| Release PR | `Prepare Release PR` dispatch | Version bump, drafted notes and the pinned candidate; never tags |
| Release E2E and packages | Intentional release dispatch or SemVer tag push | L0 and L1 on the exact SHA, which must be the merged Release PR on top of its candidate; release CLI E2E and five unpacked platform packages |
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
bans, licenses and sources on Rust or security metadata changes. Every push to
main runs workspace fmt and clippy.

The sccache setup fails open, decided before the build
(`.github/actions/setup-sccache/probe.sh`). sccache checks its storage when its
server starts, and a failed check kills the compile that started the server, so
the setup starts the server the build will use, in the build's environment,
and compiles one fixed crate through it. Only if that works does it export
`RUSTC_WRAPPER=sccache`; otherwise the job builds uncached with a warning. The
server then lives for the whole job (`SCCACHE_IDLE_TIMEOUT=0`), so no later
cargo step repeats the check, and `SCCACHE_IGNORE_SERVER_IO_ERROR=1` compiles a
unit locally if the server drops a connection. Once the server is up, storage
errors are cache misses, not failures. There is no retry of a failed cargo
step without the wrapper: the step cannot tell an sccache failure from a
compile error without parsing its output, and a shell-script `RUSTC_WRAPPER`
that could do it per `rustc` call does not run on the Windows runners. The
`sccache outage fallback` job in `ci.yml` points the cache at an unresolvable
host and requires a green uncached build, and requires that the probed server
still serves later steps; it runs when the setup or `ci.yml` changes.

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

## Release process

The maintainer reviews and merges one Release PR; the workflows do the rest.

1. **Prepare.** Actions, **Prepare Release PR**, on main, with `version`
   (`vX.Y.Z`) and optionally `ref` (default main HEAD):

   ```bash
   gh workflow run "Prepare Release PR" --ref main -f version=vX.Y.Z
   ```

   It bumps every workspace crate, `Cargo.lock` and the docs nav
   (`scripts/release/bump_version.py`), drafts `docs/changelog/vX.Y.Z.md`
   from the PRs merged since the previous tag
   (`scripts/release/draft_notes.py`), records the candidate SHA in
   `.github/release-candidate.json`, and opens `release: vX.Y.Z` from branch
   `release/vX.Y.Z`. Re-running it for the same version updates that branch
   and PR in place; it never opens a second one, never force-pushes, and keeps
   the notes' maintainer blocks. `dry_run` only writes the draft into the run
   summary. It never tags or publishes.
2. **Start CI on the PR.** GitHub does not start workflows for a PR or push
   made with `GITHUB_TOKEN`, so the required checks do not appear on their
   own. This repository also has "Allow GitHub Actions to create and approve
   pull requests" turned off, so GitHub refuses the PR itself; the run then
   prints a prefilled link, and the maintainer opening the PR from it starts
   CI like any PR. If that setting is turned on and the workflow opens the PR,
   **close and reopen it once** (Fast Gate runs on `reopened`). Do not add a
   PAT for this.
3. **Review.** Fill every `TODO(release)` block in the notes: the
   full-dataset DecBench table (no PR carries it) and, if no issue is labelled
   `release:blocker` or `bug` + `status:confirmed`, the known limitations.
   Optionally add a hand-written summary for a major release. Label PRs named
   under "Needs a label" and re-run step 1 if their section matters.
4. **Merge** through the merge queue. Then wait for the Fast Gate main push
   run, and dispatch **CI Heavy Validation** on main (or wait for the nightly
   on that commit).
5. **Tag.** Dispatch **Release Tag (CI green)** with `ref` = the Release PR's
   merge commit. The tag defaults to the one the Release PR recorded.

   ```bash
   gh workflow run "Release Tag (CI green)" --ref main -f ref=<merge-sha>
   ```

### What the tag workflow refuses

Before any build, `release-tag.yml` checks, at the exact SHA:

- L0 (Fast Gate main push) and L1 (Heavy) passed on it;
- `.github/release-candidate.json` names this tag, so a Release PR exists;
- the SHA is that Release PR's merge commit (head `release/vX.Y.Z`), not a
  later main commit;
- the diff from the recorded candidate to the SHA touches only release files
  (crate manifests, `Cargo.lock`, `mkdocs.yml`, `docs/changelog/`, the
  candidate record). If main moved before the merge, the notes miss PRs:
  re-run step 1;
- the notes exist and contain no `TODO(release)`.

It then runs release E2E (L2) and stages all five packages before creating
any tag. A manually pushed SemVer tag enters the same verification path. The
publish job verifies the staged digests, generates build provenance, creates
the tag if needed, populates a draft with `docs/changelog/vX.Y.Z.md` as its
notes, verifies the draft attachments, and publishes. Retry with the same tag
and SHA after a failed publication.

### Release notes

`draft_notes.py` is deterministic and uses no AI. A PR is in the range when
its merge commit is reachable from the candidate and not from the previous
tag. Sections, in order: Breaking Changes (`risk:breaking`), Correctness
Fixes (`bug`, `risk:semantic`), Reliability (`phase:2-reliability`),
Performance (`type:performance`), New Capabilities (`enhancement`), Internal
(`type:refactor`), Testing & Infrastructure (`type:test`, `area:ci`),
Documentation (`documentation`), Other. The first matching label wins; a PR
with none is classified by its conventional title prefix (`fix`, `perf`,
`docs`, `test`/`ci`, `refactor`, `feat`, `type!:` for breaking). Anything
else is listed under Other and named in a "Needs a label" line. Bot authors
and `skip-changelog` PRs are left out and recorded in a trailing comment.
`.github/release.yml` gives GitHub's own generated notes the same sections.

Numbers are quoted verbatim, with their PR, only from a PR's `Benchmark ...`,
`Correctness Evidence` or top-level `## Measurement ...` / `## Measured ...`
section; nothing is added up across PRs. DecBench
measures similarity to the source, not correctness (ADR 0018), and the notes
say so.

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
4. Every bundle carries `fission-resources.json`: the Fission version, tag and commit it was cut from, and a digest of each directory. A release binary embeds the same tag and commit. The package smokes and the publish job refuse a release whose tag, binary identity (`resources status --json`) and manifests disagree, and the tag must equal the CLI's Cargo version before anything builds ([`check_identity.py`](https://github.com/fission-systems/Fission/blob/main/scripts/release/check_identity.py)).
5. Every archive is smoke-tested the way a user runs it -- unpacked outside the checkout and invoked from an unrelated directory -- by [`scripts/release/smoke_package.sh`](https://github.com/fission-systems/Fission/blob/main/scripts/release/smoke_package.sh). The v0.2.x archives put `utils/` beside the binary, a place the binary never looks, and only worked from inside a source tree.

## Related files

- [`.github/CI_CD_GUIDE.md`](https://github.com/fission-systems/Fission/blob/main/.github/CI_CD_GUIDE.md) — operator overview
- [`.github/workflows/prepare-release.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/prepare-release.yml)
- [`.github/workflows/release-tag.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-tag.yml)
- [`.github/workflows/release-e2e.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/release-e2e.yml)
- [`.github/workflows/cd.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/cd.yml)
- [`utils/MANIFEST.md`](https://github.com/fission-systems/Fission/blob/main/utils/MANIFEST.md)
