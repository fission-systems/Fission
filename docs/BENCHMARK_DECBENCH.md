# DecBench — the main benchmark

DecBench is the benchmark decompiler-quality work is measured on. It replaces
the docker multi-decompiler loop described in
[`BENCHMARK_DOCKER.md`](BENCHMARK_DOCKER.md), which is retired.

**Quality is measured on the whole dataset.** Every quality claim cites the
full-dataset sweep: all 96,103 functions of `noelo-lab/decbench-dataset`, broken
down by config (`unoptimized`, `optimized`, `inlined`, `large`, `sample-set`),
with the published tools recomputed on the same rows. The 250-function
sample-set gate is the quick pre-check before a sweep, not the evidence. The
published Fission leaderboard row covers 100 rows of a config, so a rank read off
the leaderboard was never same-denominator.

The data and harness live outside this repository, under
`~/fission-benchmark` (the directory name is historical; it hosts DecBench):

| Item | Path |
|------|------|
| Dataset for the sweep | `~/fission-benchmark/decbench-full/data` — revision `815eb96f130b6d2ef7fa600e04669bf20139bf27` (the 2026-10-09 rebuild): every binary of the `full` manifest, published source CFGs, each config's `manifest.json` and `function_results.json` (per-function scores of every published tool); hashes verified against the Hub and the manifests |
| Full-dataset sweep | `~/fission-benchmark/decbench-full/sweep.py` (`run`, `report`, `diff`); its `README.md` documents every step |
| Dataset for the gate | `~/fission-benchmark/decbench-data` — the July build the sample-set kit and gate floor were taken on; its binaries predate the 2026-10-09 rebuild, so do not sweep on it |
| Python environment | `~/fission-benchmark/.venv-decbench/bin/python` (has `decbench` installed editable from `vendor/decbench`, plus `pydantic`, `networkx`, `scipy`, `cfgutils`; the system Python has neither `pydantic` nor a CFG parser) |
| Scoring code | `scripts/quality/decbench_scoring.py` (this repository) calling the `decbench` package |
| CFG parser | `rust-joern`, built from source at `~/rust-joern` (see below) |
| Sample-set kit | `vendor/decbench-evalkit/decbench-evalkit-sample-set` |


## CI full sweep

[DecBench full dataset sweep](../.github/workflows/decbench-sweep.yml) measures
the published Hugging Face `full` manifest on Nightly. The pinned dataset
revision contains 96,103 rows. The workflow shards the manifest, verifies binary
hashes, scores statically on stripped inputs, and preserves its row-level report.
The 250-row sample-set gate below remains a local development tool. See
[CI and release gates](CI_RELEASE_GATES.md#nightly-decbench) for the CI policy.

The nightly and the local sweep score with different DecBench code. The local
`scorer/vendor/decbench` is a snapshot with no upstream history; CI installs
the public upstream commit that produced the pinned revision's published scores.
On the same CLI and 970 rows over 18 binaries (every opt level, ELF x86-64,
ELF ARM and PE) the two agree on status everywhere; on decompiled output except
18 PE rows, whose stripped inputs differ; on GED except one row with identical
output (GED's known nondeterminism); and on type_match except 12 x86-64 O2 and
O2-noinline rows. Compare a nightly with a nightly and a local run
with a local run, not one with the other.

## CFG parser: rust-joern

GED's source- and decompiled-side CFGs come from
[rust-joern](https://github.com/Noelo-Lab/rust-joern), not the JVM-backed
`pyjoern` (DecBench switched on 2026-10-09, Noelo-Lab/decbench#99). It is not on
PyPI:

```bash
git clone https://github.com/sjkim1127/rust-joern ~/rust-joern   # or Noelo-Lab/rust-joern
(cd ~/rust-joern && cargo build --release)
```

`scripts/quality/decbench_scoring.py` calls
`rust_joern.parse_source(path, no_ddg=True, strict=True, preprocessed=True)`,
the same call as DecBench. Override the checkout with `RUST_JOERN_DIR`. The gate
parses a trivial function when it starts and stops with an error if the parser is
missing, instead of finishing with every function unscored. The `decbench` commits
that the reorganised `fission-benchmark` pins and `vendor/decbench` still call
pyjoern, so `fbench` numbers are on the JVM parser and are not comparable with the
gate's.
`FISSION_GED_BACKEND=pyjoern` restores the old parser **only** to reproduce a
number taken before the switch; do not compare across backends.

Measured on 259 rows (same Fission build, same published source CFGs): the two
parsers give identical output-node counts on 258, identical GED on 202, and GED
sums 26,718 (pyjoern) vs 26,710 (rust-joern), maximum per-row difference 12 in
either direction. The JVM parse used to be ~84% of a full sweep's worker time;
it is now close to zero.

## Two tools, two jobs

| | Local sample-set gate | Full-dataset sweep |
|---|---|---|
| Script | `scripts/quality/decbench_sample_set_gate.py` | `~/fission-benchmark/decbench-full/sweep.py` |
| Rows | 250 (233 scored for GED) | 96,103 (`full` = `unoptimized` ∪ `optimized` ∪ `inlined`; `large` and `sample-set` are subsets) |
| Time | ~5 min, ~10 min cold | hours; see the baseline below for measured wall and CPU time |
| Use for | quick pre-check; refuses a drop below a stored floor | **every quality claim**, ranking or release claim, same-row comparison with the published tools |

### Local gate

```bash
cargo build -p fission-cli --release
~/fission-benchmark/.venv-decbench/bin/python scripts/quality/decbench_sample_set_gate.py check \
  --cli target/release/fission_cli --dump /tmp/gate_after.json
```

GED, source-CFG loading and decompiled-C preparation are DecBench's own
(`GEDMetric`, `rebuild_cfg`, `preprocess_decompiled_c`). GED is exact up to
`DECBENCH_GED_MAX_NODES` (200) nodes and a size lower bound of at least 1 above
it. Floors recorded before 2026-10-31 used a local copy that approximated above 60
nodes and could score two equally sized large graphs 0: on the same binary the 29
rows that differ are 28 functions over 60 nodes (total distance 6,226 to 7,769,
perfect 67 to 68). Do not compare totals across that change.

`--quick` scores the first 60 binaries. `update` raises the stored floor
(`scripts/quality/decbench_sample_set_baseline.json`) after a measured gain.
`--dump` writes one row per function (GED, source and output node counts) so two
runs can be compared per function rather than only in aggregate. The summary prints
the total GED distance next to the perfect count; report both.

Compare decompiler *output* before reading score deltas. The previous scorer
flipped one row between GED 0 and 11 on identical text; the current one gave
byte-identical dumps on two runs of the same binary, but a few rows moving between
two different builds is still not evidence until their decompiled text differs.

The gate remembers each binary's DWARF ground-truth types under
`~/fission-benchmark/.gate-cache/ground_truth_types/` (about 56 MB for the sample
set). Reading them took 93 of 243 seconds in a measured run; with a warm cache
the same run took 136 s against 278 s with the cache off in the same session, and
all three runs gave byte-identical `--dump` output. The key is the SHA-256 of the
binary and of the extractor's source, so it does not hold anything Fission
produced and `FISSION_BENCHMARK_NO_CACHE=1` still gives an honest before/after.
`FISSION_GATE_GT_CACHE=0` bypasses it.

Resolve the CLI explicitly with `--cli`; the gate otherwise picks the newest of
`release`, `quick-release` and `debug`, and a stale build silently scores the
wrong thing.

### Full sweep

```bash
PY=~/fission-benchmark/.venv-decbench/bin/python
SWEEP=~/fission-benchmark/decbench-full/sweep.py
$PY -I $SWEEP run --cli target/release/fission_cli --repo "$PWD" --label after   # --config full by default
$PY -I $SWEEP diff ~/fission-benchmark/decbench-full/runs/before ~/fission-benchmark/decbench-full/runs/after
$PY -I $SWEEP report ~/fission-benchmark/decbench-full/runs/after                 # summary.md / summary.json
```

What one `run` does, per binary of the config:

- Fission sees only a stripped copy (`llvm-strip --strip-all -R .comment -R
  .note.gnu.build-id`, DecBench's eval-kit recipe, checked by DecBench's own
  `_verify_strip`). Names, addresses (`evalkit.resolve.name_to_addr`) and DWARF
  ground-truth types come from the unstripped original, for grading only.
- `fission_cli decomp --addresses-file` with `FISSION_BENCHMARK_NO_CACHE=1`, the
  checkout as working directory, in chunks of 400 functions. A chunk that crashes,
  times out or passes the memory cap is bisected until the failing function is
  named, so one bad function costs that function, never its binary. Each call's
  wall time, CPU time and peak RSS are recorded.
- The sha256 of every function's output is stored, so `diff` compares two runs on
  output first and shows scores only for rows whose text changed (and how many
  rows moved with identical text, i.e. scorer noise).
- GED and type_match use the same code as the gate (`decbench_scoring.py`, the
  `decbench` package, rust-joern), frozen under `decbench-full/scorer/` with
  their hashes recorded in each run.
- **byte_match is not run**: it needs DecBench's recompile toolchain (gcc for each
  target under `linux/amd64`), whose Docker image is gone. Results say "not run"
  instead of 0, Union is reported as Union(GED, type), and the published tools
  are recomputed on the same two metrics.

`report` gives, per config: rows, decompiled, failures, GED perfect next to total
distance (and median/p90), type perfect and mean, Union(GED, type), decompile wall
and CPU time; then Kuna, IDA, Binary Ninja, angr, Ghidra and the published Fission
row on exactly the same rows, with GED distance compared on the rows every tool
scored. Type numbers are not like-for-like across tools (Fission is scored through
its structured variables, the published rows through each tool's adapter); use
them for Fission's own before/after.

A run resumes (one result file per binary) and refuses a different `fission_cli`
or scorer in the same run directory. Re-scoring the rivals' published output
through this GED path reproduces their published per-function GED on 84-87% of
rows, totals within 3% (`validate_key.py`; the residue is that DecBench parses a
rival's whole file while the check feeds each function alone, which does not
apply to Fission's per-function submission).

The previous harness (`decbench-harness/fullrun.py` + `fullagg.py`) ran on the
July build's `unoptimized` config only and is superseded.

### Baseline: `main` 5073a1c63 (2026-11-21)

One `full` run, 4 workers: 34 min wall on a shared machine, 8,051 s CPU in
`fission_cli`. 916 decompile calls, none failed; 22 rows have no DWARF address;
0 decompile failures. byte_match not run.

| config | rows | GED perfect / scored | GED total distance | type perfect / scored (mean) | Union(GED, type) | decompile wall / CPU (s) |
|---|---:|---:|---:|---:|---:|---:|
| full | 96,103 | 32,136 / 90,459 | 1,519,008 | 5,799 / 86,934 (0.229) | 35,421 (36.9%) | 5,261 / 7,057 |
| unoptimized | 34,505 | 13,475 / 32,973 | 387,650 | 3,650 / 31,181 (0.353) | 15,539 (45.0%) | 1,781 / 2,524 |
| optimized | 35,399 | 11,151 / 33,135 | 520,443 | 1,495 / 32,054 (0.173) | 12,052 (34.0%) | 1,549 / 2,213 |
| inlined | 26,199 | 7,510 / 24,351 | 610,915 | 654 / 23,699 (0.144) | 7,830 (29.9%) | 1,931 / 2,320 |
| large | 1,962 | 6 / 1,920 | 170,252 | 2 / 1,924 (0.128) | 8 (0.4%) | (subset) |
| sample-set | 250 | 69 / 246 | 7,752 | 18 / 228 (0.236) | 81 (32.4%) | (subset) |

Same rows, published scores (`full`; Union(GED, type) counted only on rows
Fission can be scored on, because the published source-CFG file lacks 1,137
functions DecBench scored the rivals on; GED distance on the 67,263 rows every
tool has a GED for):

| tool | Union(GED, type) | GED perfect | common-row GED distance |
|---|---:|---:|---:|
| Kuna 1.515 | 38,102 | 35,398 | 879,499 |
| IDA 920 | 37,437 | 33,966 | 955,356 |
| angr 9.3.4 | 36,678 | 32,510 | 1,037,320 |
| **Fission main** | **35,421** | **32,136** | **1,227,074** |
| Binary Ninja 5.3 | 30,448 | 26,048 | 1,100,194 |
| Ghidra 12.1 | 29,909 | 25,835 | 1,062,654 |

Fission is 4th of six on Union in `full`, `unoptimized`, `optimized` and
`inlined`, last on `large`, and 2nd on the sample set (81 against Kuna's 88).
On total GED distance it is last in every config. The sample-set rank does
not hold on the whole dataset, which is why the whole dataset is the measure.

## Rules

- **Measure on the one output surface.** `code` is the only output
  ([ADR 0015](adr/0015-single-output-surface.md)); `--layer` is ignored, so the
  harness's `LAYER` setting no longer selects anything.
- **Report total distance next to the perfect count.** The perfect count cannot
  see a regression in the tail: a change once ranked ahead on perfect matches
  and 9.4% behind on distance. See *Fitting the metric instead of the row* in
  `AGENTS.md`.
- **Do not rebuild `fission_cli` while a run is in flight** — the output becomes
  a mix of two binaries. Build, then measure; run the before and the after from
  separately named binaries if both are needed.
- **Never execute a dataset binary.** The corpus includes malware compiled from
  source; strip, `nm` and decompile are static and fine.
- **Do not tune repeatedly against the scored rows.** `corpus/scale` under
  `~/fission-benchmark` carries published source CFGs for 64,231 functions that
  the sample set does not score; use it as the unseen split (it is `-O0` with
  symbols, so a shape visible only there is a different overfit).
- **Quality claims cite the full-dataset sweep**, per config, total distance next
  to the perfect count, failures included. The sample-set gate is a pre-check.
- Local results are never an official ranking. A leaderboard or release claim is
  the sweep, on same rows, from a tagged build.
