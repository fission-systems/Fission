# CI and release gates

| Gate | Trigger | Required evidence |
|---|---|---|
| Fast Gate | All PR targets, GitHub merge candidates and pushes to main | Change plan from event SHAs, with affected Linux tests, CLI smoke and pre-merge lanes when selected |
| Heavy | Nightly or workflow dispatch | Linux workspace tests, Windows and macOS coverage, MSRV, compile surface and the RustSec advisory check |
| FreshBench full sweep | Nightly or workflow dispatch | One shard per M1 build (48), per-function results, and comparison with a compatible passing main sweep |
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
checked-in fixtures runs every pre-merge lane, and editing the FreshBench sweep
(workflow, `scripts/ci/freshbench_sweep.py` or `freshbench_corpus.py`, which
pins the corpus and scorer) runs the nightly workflow on one binary, which fails
unless decompilation, GED and type scoring all produced data and no tool call
failed. Enabled nextest suites fail if they discover no tests. An
enabled lane whose job is skipped, cancelled or failed makes `CI Summary` fail.

The native debugger lane exercises Linux, Windows and macOS processes on hosted
runners. The macOS job enables Developer Tools and signs the test executable.
Run this job on a real PR before making the merge queue required. The solver
lane uses a short differential suite with Z3 4.13.4. SLEIGH and interpreter
changes run the native differential on x86-64 and AArch64 runners (below). Loader changes cover ELF,
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

- Speed: a PR that changes the decompiler, or carries the `type:performance`
  label when pushed, counts instructions executed by its base and head CLIs
  after its Linux tests pass
  on six fixed FreshBench M1 binaries (valgrind cachegrind; wall time on shared
  runners is too noisy) and flags +3% in its summary; nightly `speed.yml`
  compares main with the last good night and fails at +3%. Dispatch it with
  `accept_baseline` to accept a slowdown that is worth it.
- Decompiler changes: a PR that can move output gets an output diff on a
  fixed FreshBench M1 sample (about 3,000 functions) in its job summary: the
  `freshbench sweep diff` of base and head, functions whose text changed first,
  then scores gained and lost on those. Report-only; open the changed functions
  before trusting a score.
- Nightly: fuzzing of six targets (ELF, PE, Mach-O, COFF, any format, SLEIGH
  decode) seeded from the loader fixtures; an 8 GiB memory ceiling on the
  eight largest FreshBench M1 binaries; decompiler determinism. Weekly: mutation
  testing of lines changed on main, four of 48 rotating slices, report-only.
  Each reports into a `ci-red` issue when it fails.

- Workflows: every third-party action is pinned to a commit SHA (Dependabot
  updates the pins monthly), inputs reach shell code through `env`, and no
  workflow grants `actions: write`. Any workflow change runs actionlint and
  zizmor; a high-severity zizmor finding fails Script Checks.
- The PR fixture lane compares decompiled text across requests for 1, 4 and 4
  workers. The nightly `determinism.yml` requests 1, 4, 4 and 4 workers and
  checks whole stripped FreshBench M1 binaries in SHA-256 order until 5,000
  decompiled functions. Both set `FISSION_RUST_DECOMP_WORKERS` and verify the CLI's
  reported worker count, capped by the number of functions. Nightly differences
  are reported in a `ci-red` issue and are outside the release gate.
- The PR fixture lane builds the release CLI with overflow checks and debug
  assertions on.

## Parallel bisect

`parallel-bisect.yml` names the first main commit between a good and a bad
one that regressed. Every first-parent commit in the range, up to 48 and sampled
evenly beyond that, is built and measured at once, so one round answers what a
sequential bisect needs log2(N) rounds for. When the range was sampled, the
verdict says so and gives the narrower range to run again.

- **output probe:** a FreshBench sweep of one build or slice. A commit is bad when
  fewer rows have output than on the good commit, or more tool calls fail.
- **speed probe:** the cachegrind sample. Each probe also measures the good
  commit's CLI on its own runner, so the 3% threshold compares like with like.

The nightly FreshBench sweep runs it when rows lose their output, on the first
such build, from the baseline sweep's commit. The nightly speed check runs it
on a 3% rise, from the last good night's commit. Both comment the verdict on
the run's `ci-red` issue. It can also be dispatched by hand.

## Scheduled runs report into issues

Heavy, fuzz, determinism, the native differential and the FreshBench sweep end with `reusable-report-red.yml`. While a
workflow is red on main there is one open issue titled `CI red: <workflow>`
with the `ci-red` label; each further red run comments with its SHA and run,
and the first green run closes it. Cancelled runs report nothing. Fuzzing runs
on its schedule or by manual dispatch, restores its corpus from earlier runs,
and attaches crash inputs to the run for 30 days.

### Sentry monitors

Sentry (org `fission-systems`) watches two things from outside the run. The
plan has one cron and one uptime monitor seat, and both are in use.

- **Cron `ci-heavy-nightly`** (project `fission`). Scheduled Heavy runs in this
  repository check in `in_progress` when they start and `ok` or `error` from
  `heavy-summary`'s result (anything but success, cancelled included, is
  `error`). Manual dispatches never check in. Each check-in carries the
  monitor config, so Sentry creates and updates the monitor from
  `scripts/ci/sentry_cron_checkin.py`: crontab `30 2 * * *` UTC, max runtime
  180 minutes, one failure opens an issue and one success resolves it. The
  check-in margin is 900 minutes because GitHub starts this schedule 5 to 10
  hours late (43 to 717 minutes over 100 nights); a night with no run alerts
  at 17:30 UTC. Change the cron in the workflow and the script together (a
  unit test compares them). The jobs need the `SENTRY_DSN` repository secret
  (the `fission` project's DSN). Without it they skip, and a Sentry failure
  never changes the run's result.
- **Uptime "FreshBench results page"** (project `freshbench`) requests
  `https://fission-systems.github.io/freshbench/` every 5 minutes with a
  10-second timeout. It was created once through the API and needs no secret.

## CI defect events

Some red runs are defects someone should fix, not just a red check. CI sends
those to Sentry (org `fission-systems`, project `fission`) as structured
events, so a defect that comes back night after night is one Sentry issue with
a count, not a series of comments.
`scripts/ci/sentry_defects.py` reads what the jobs already produce. Each
job uploads its input as a `defect-evidence-*` artifact, and fuzzing keeps its
`fuzz-crashes-*` artifacts. A final report-only job
(`reusable-sentry-defects.yml`) turns them into events:

| Source | Event | Category |
|---|---|---|
| nextest JUnit (`NEXTEST_PROFILE=ci`), a test that failed and then passed on nextest's retry | `flaky_test` (warning) | `flaky` |
| nextest JUnit, a test killed by a signal on every attempt | `test_failure` | `crash` |
| native differential, a divergence `baseline.txt` does not list | `native_differential_mismatch`, one per instruction and field | `confirmed_semantic_defect` (`crash` for `panic`) |
| fixture replay witness, a lane that disagrees with the native run | `verify_diverged` | `confirmed_semantic_defect` |
| libFuzzer `crash-<sha1>` | `fuzz_crash`, one per input | `crash` |
| `fission_cli verify` JSON on an in-repo fixture, a `diverged` tier (no CI job runs this yet) | `verify_diverged` | `confirmed_semantic_defect` |

These are never sent: a test that fails the same way on every attempt (the
job's status and the `ci-red` issue already report it), timeouts, libFuzzer
`timeout`/`oom`/`leak` files, stale baseline entries, instructions Fission
cannot execute, and `suspect`, `unsupported`, `harness` or `inconclusive`
verify results.

Every tag value comes from a closed set or must match a pattern: `kind`,
`component` (a crate name), `arch`, `stage`, `invariant`, `category`,
`commit` (the run's head SHA), `repro_id` (`fixture:`, `fuzz:` or
`run:<id>/<artifact>`) and `run_url`. The exception value is the fixed text
`<kind> in <component>/<stage>`. An event with a value that fails its check is
dropped, and the reason is listed in the job summary; the value is never
cleaned up and sent. The reporter only looks at structure (element names,
closed-set fields, file names). It never reads panic text, disassembly,
decompiled output, symbol names or input bytes into an event. A test id longer
than 64 characters is cut to 55 characters plus 8 hex digits of its SHA-256.

The reporter sends at most 50 events per run and one per fingerprint. Defects
take the cap before flaky tests. It sends only from `push`, `merge_group`,
`workflow_dispatch` and `schedule` runs of this repository on `main` (a merge
group must target `main`), and only when the `SENTRY_DSN` repository secret is
set. Pull requests, forks and runs on other branches get a dry run. Every run
writes a job summary of what was sent, or what would have been. The job has
only `contents: read`, runs no build or test code, and is `continue-on-error`.
The script itself exits 0. It never changes a run's result.

Run it locally on downloaded artifacts with `--dry-run` (set the `GITHUB_*`
variables of the run). Its tests are `scripts/ci/test_sentry_defects.py`, and
they never touch the network.

## Native differential (SLEIGH lift against the CPU)

Fission lifts from Ghidra's SLEIGH specs, and its emulator and decompiler share
that lift, so neither Ghidra nor `fission_cli verify` can see a wrong spec
([ADR 0017](adr/0017-ghidra-is-not-an-oracle.md)). The native differential
(`crates/fission-emulator/tests/native_differential/`) runs single
instructions on the host CPU and through Fission's lift and p-code
interpreter from the same random state, and compares every general-purpose
register, flag, vector register and a 16 KiB scratch buffer.

- **Cases.** A curated list per ISA (x86-64: integer ALU and flags, shifts and
  rotates, MUL/DIV, bit scans, CMOVcc/SETcc, MOVSX/MOVZX, BT*, string
  instructions, SSE/SSE2/SSE4.1 conversions, every ROUND* and CMP* immediate,
  min/max, sqrt, shuffles, unpacks, packs; AArch64: integer and flags,
  CSEL/CSINC/CCMP, loads, FCVT*, FRINT*, SCVTF/UCVTF, FMIN/FMAX, basic NEON)
  from 24 states each, plus 3,000 random byte strings the decoder accepts.
  Anything that branches, calls into the host, touches state the two sides
  cannot share (FS/GS bases, x87/MMX, AVX, system registers, SVE) or is
  privileged is skipped and counted.
- **Masks.** Results the architecture manual leaves undefined are masked per
  instruction, and each mask cites the manual section
  (`target_x86_64.rs`, e.g. MUL's SF/ZF/AF/PF, BSF/BSR with a zero source,
  shift and rotate OF). MXCSR and FPSR exception flags are set, not compared:
  Fission does not model them.
- **Gate.** `baseline.txt` lists every known divergence with its issue. The
  test fails on a divergence not listed there and on a listed one that no
  longer reproduces. Each failure prints a one-line `REPRO` with the seed,
  case id, bytes, input state and both results, and the command that reruns
  only that case.
- **Where.** On pull requests and merge candidates that touch
  `crates/fission-sleigh/`, `utils/sleigh-specs/`,
  `crates/fission-emulator/src/pcode*`, `interp.rs`, `arch/`, the float
  helpers and CALLOTHER callbacks under `jit/` that the interpreter calls, or
  the test itself, the `native-diff` pre-merge lane runs it on `ubuntu-24.04`
  (x86-64) and `ubuntu-24.04-arm` (AArch64): about 10 s of test time after the
  build. `native-differential.yml` runs it nightly with 128 states and 100,000
  random cases per ISA and reports into a `ci-red` issue. The test also runs
  in every Fast Gate test job that selects `fission-emulator`, and locally on
  any x86-64 Linux or AArch64 Linux/macOS host
  (`cargo nextest run -p fission-emulator --test native_differential`).

The PR and nightly runs share the seed, so every PR case is also a nightly case.
A divergence only the nightly finds gets a curated case before it is added to
the baseline, so the PR run reproduces every entry.

## Nightly FreshBench

The corpus is FreshBench M1 ([`sjkim1127/freshbench-v2`](https://huggingface.co/datasets/sjkim1127/freshbench-v2),
revision `15b6f2c52c379cf9361f22edaa75886602e5e1ff`). Its manifest comes from
[fission-systems/freshbench](https://github.com/fission-systems/freshbench)
at the commit pinned in `scripts/ci/freshbench_corpus.py`, checked against the
published hash. Every downloaded file is checked against the manifest. Each shard
downloads one build: stripped and unstripped binaries, targets and the project's
preprocessed sources. Binaries stay non-executable and are only decompiled;
the grader reads the unstripped copies separately.

The scorer is `freshbench` built from the same pinned commit. `sweep run`
records the scorer id and refuses to compare runs scored differently. Move
`FRESHBENCH_COMMIT`, `REVISION` and `MANIFEST_SHA256` together, never alone.

`scripts/ci/freshbench_sweep.py report`:
- reports each build and the total: rows, perfect count next to distance,
  scored rows, perfect types, failed tool calls and CPU time;
- runs `sweep diff` against the latest passing main sweep's runs;
- blocks on a missing shard or on a row that had output on that baseline and
  has none now. Score deltas are observational.

A complete passing run with a new scorer or corpus establishes the next
baseline. See [the benchmark guide](BENCHMARK_FRESHBENCH.md).

## Output ledger

`output-ledger.yml` records one entry per main push that changes a
decompiler input (`DECOMPILER_INPUTS` in `scripts/ci/output_diff.py`, the
same list as the workflow's `paths`). It is report-only and runs one at a
time; a newer push replaces a waiting one, so commits can be skipped.

An entry (`scripts/ci/output_ledger.py record`) holds:
- each function's output hash on output_diff.py's fixed FreshBench sample, and
  that sample's totals;
- instructions executed on speed_compare.py's sample by that commit's CLI
  alone, with the runner's CPU model. Counts are comparable only on the same
  CPU model.

| artifact | kept | used by |
|---|---|---|
| `output-ledger-<commit>` (the entry) | 90 days | `output_ledger.py history KEY`: the main commits that changed one function's output, read from entries alone; `compare OLD NEW` for two entries |
| `output-ledger-run-<commit>` (the sweep run) | 30 days | the PR base-vs-head check's base side |

Base-vs-head takes the run of the newest first-parent commit at or before
the PR base that changed a decompiler input, because the base decompiles
exactly as that commit does. It uses the run only when it is clean and has
the head run's scorer, corpus and function count. Otherwise it decompiles
the base itself.

A second job sends each entry's totals to Sentry (`scripts/ci/sentry_ledger.py`)
as one `output-ledger` transaction. Its measurements are the sample's totals
and `instructions`; its tags are `commit` and `cpu`. To plot them, use Explore
or a dashboard with `transaction:output-ledger`, and group `instructions` by
`cpu`. Only those numbers, the commit hash and the CPU model are sent: no
function key, output hash, decompiled text or binary name. Values that fail
their pattern are rejected, not cleaned. The job runs only for main pushes
and dispatches, and never changes the run's result.

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
   full-corpus FreshBench table (no PR carries it) and, if no issue is labelled
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

### Sentry release tracking

After `publish`, the `sentry-release` job records the release in Sentry (org
`fission-systems`, project `fission`): `releases new`, `set-commits --auto`
(falling back to `--local`), `finalize`, and a `production` deploy linked to
the GitHub Release. The name is `fission-cli@<version>` (the tag without its
`v`), which is what the opt-in CLI telemetry reports when `SENTRY_RELEASE` /
`FISSION_RELEASE` are unset. It reads the `SENTRY_AUTH_TOKEN` repository
secret (an org token with `project:releases`); without it the job skips its
steps and succeeds. It runs only in this repository, only after the release
is public, and gates nothing. Re-run just this job if Sentry was unavailable.

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
section; nothing is added up across PRs. A benchmark
measures similarity to the source, not correctness (ADR 0019), and the notes
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
- [`.github/workflows/freshbench-sweep.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/freshbench-sweep.yml)
- [`.github/workflows/output-ledger.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/output-ledger.yml)
- [`.github/workflows/speed.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/speed.yml)
- [`.github/workflows/memory-ceiling.yml`](https://github.com/fission-systems/Fission/blob/main/.github/workflows/memory-ceiling.yml)
- [`utils/MANIFEST.md`](https://github.com/fission-systems/Fission/blob/main/utils/MANIFEST.md)
