# DecBench — the main benchmark

DecBench is the benchmark decompiler-quality work is measured on. It replaces
the docker multi-decompiler loop described in
[`BENCHMARK_DOCKER.md`](BENCHMARK_DOCKER.md), which is retired.

The data and harness live outside this repository, under
`~/fission-benchmark` (the directory name is historical; it hosts DecBench):

| Item | Path |
|------|------|
| Dataset (do not re-download) | `~/fission-benchmark/decbench-data` — binaries, published source CFGs, per-function scores of the other decompilers |
| Sweep harness | `~/fission-benchmark/decbench-harness/fullrun.py`, `fullagg.py` |
| Python environment | `~/fission-benchmark/.venv-decbench/bin/python` (has `pydantic`, `pyjoern`; the system Python has neither) |
| Sample-set kit | `vendor/decbench-evalkit/decbench-evalkit-sample-set` |

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

`--quick` scores the first 60 binaries. `update` raises the stored floor
(`scripts/quality/decbench_sample_set_baseline.json`) after a measured gain.
`--dump` writes one row per function (GED, source and output node counts) so two
runs can be compared per function rather than only in aggregate.

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
(binaries), `ONLY` (comma-separated), `BINARY_WORKERS`, `JOERN_CHUNK`,
`JVM_CAP`. `FISSION_BENCHMARK_NO_CACHE=1` disables the decompile / GED / type
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
- Do not run more Joern JVMs than memory allows: each wants ~1.5 GB.
- Local results are never an official ranking. A leaderboard or release claim is
  the sweep, on same rows, from a tagged build.
