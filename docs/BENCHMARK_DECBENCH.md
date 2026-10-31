# DecBench — the main benchmark

DecBench is the benchmark decompiler-quality work is measured on. It replaces
the docker multi-decompiler loop described in
[`BENCHMARK_DOCKER.md`](BENCHMARK_DOCKER.md), which is retired.

The data and harness live outside this repository, under
`~/fission-benchmark` (the directory name is historical; it hosts DecBench):

| Item | Path |
|------|------|
| Dataset (do not re-download) | `~/fission-benchmark/decbench-data` — binaries, published source CFGs, per-function scores of the other decompilers |
| Sweep harness | `~/fission-benchmark/decbench-harness/fullrun.py`, `fullagg.py` (legacy full-dataset sweep); `python -m fbench run` in the reorganised repository |
| Python environment | `~/fission-benchmark/.venv-decbench/bin/python` (has `decbench` installed editable from `vendor/decbench`, plus `pydantic`, `networkx`, `scipy`, `cfgutils`; the system Python has neither `pydantic` nor a CFG parser) |
| Scoring code | `scripts/quality/decbench_scoring.py` (this repository) calling the `decbench` package |
| CFG parser | `rust-joern`, built from source at `~/rust-joern` (see below) |
| Sample-set kit | `vendor/decbench-evalkit/decbench-evalkit-sample-set` |

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
| Script | `scripts/quality/decbench_sample_set_gate.py` | `decbench-harness/fullrun.py` + `fullagg.py` |
| Rows | 250 (233 scored for GED) | up to 34,406 (`unoptimized` config) |
| Time | ~5 min, ~10 min cold | hours (dominated by a few huge binaries) |
| Use for | quick signal; refuses a drop below a stored floor | any ranking or release claim, same-row comparison |

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
cd ~/fission-benchmark/decbench-harness
CONFIG=unoptimized CKPT=/tmp/decbench-full-ckpt python3 fullrun.py
CONFIG=unoptimized CKPT=/tmp/decbench-full-ckpt python3 fullagg.py
```

Environment: `CONFIG` (`unoptimized`, `optimized`, `inlined`, `large`, …),
`CKPT` (one checkpoint per binary, so an interrupted sweep resumes), `LIMIT`
(binaries), `ONLY` (comma-separated), `BINARY_WORKERS`, `JOERN_CHUNK`. `FISSION_BENCHMARK_NO_CACHE=1` disables the decompile / GED / type
caches; **use it for any before/after**, otherwise the second run reads the
first run's answers.

`fullagg.py` compares only on rows Fission actually scored. The published
Fission row covers 100 of 34,406 functions, so quote same-row comparisons, not
the leaderboard percentages.

`fullrun.py` runs `/Users/sjkim1127/Fission/target/release/fission_cli`.

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
- Local results are never an official ranking. A leaderboard or release claim is
  the sweep, on same rows, from a tagged build.
