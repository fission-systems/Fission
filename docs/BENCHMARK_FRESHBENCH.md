# FreshBench — the benchmark

[FreshBench](https://github.com/fission-systems/freshbench) is the benchmark
Fission's output-changing work is measured on
([ADR 0019](adr/0019-freshbench-is-the-benchmark.md)).

**It measures closeness to the source, not correctness.** GED compares the
shape of the output's control-flow graph with the source function's. Types are
compared against DWARF, and recompilation compares instructions. A wrong
value, argument or call can still score well, and recovering missing code can
raise the distance. Correctness is shown with native replay, `fission_cli
verify`, manuals, DWARF/PDB and call sites counted by instruction address.
FreshBench is the comparison and regression measurement reported next to that
evidence ([ADR 0018](adr/0018-decbench-is-not-a-correctness-oracle.md), whose
principle ADR 0019 carries over).

## What it measures

The full metric list is in FreshBench's README. In short:
- **Ranked:** GED distance from the source (mean, median, rows at 0, with
  unscored rows charged the whole source graph) and function boundaries.
- **Reported, not ranked:** speed (CPU time of the process tree, the figure to
  compare), perfect rates, Joern GED, types, recompilation and call sites.

Every table is split by source size band. Report total or mean distance next
to the perfect count: a count alone has ranked a regression as a win.

## Corpus

M1 has 285,875 rows:
- C, 20 projects.
- x86-64, i686, AArch64 and ARMv7 Thumb-2 (hard float).
- GCC 16.2.0 and Clang 23.1.6.
- O0, O1, O2, O3, Os and O2-noinline.

Fission sees stripped binaries only. Build names carry the architecture
except on x86-64: `gcc-O0`, `i686-gcc-O0`, `aarch64-clang-Os`,
`armv7-gcc-O2-noinline`. The revision and hashes are pinned in FreshBench's
[`corpus/m1`](https://github.com/fission-systems/freshbench/tree/main/corpus/m1).

```bash
git clone https://github.com/fission-systems/freshbench ~/freshbench
(cd ~/freshbench && cargo build --release --locked -p freshbench)
FB=~/freshbench/target/release/freshbench

# about 13 GB for all four architectures; see corpus/m1/README.md to fetch one
hf download sjkim1127/freshbench-v2 --repo-type dataset \
  --revision 15b6f2c52c379cf9361f22edaa75886602e5e1ff \
  --include "m1/**" --local-dir ~/freshbench-data
$FB manifest verify ~/freshbench-data/m1 ~/freshbench/corpus/m1/manifest.jsonl.gz
```

## Before / after

```bash
cargo build -p fission-cli --release
$FB sweep run ~/freshbench-data/m1 --cli target/release/fission_cli --repo "$PWD" --label after
$FB sweep diff runs/before runs/after
$FB sweep report runs/after
```

- **`run`** decompiles every selected function of every binary and scores it.
  - It writes `runs/<label>/` with the CLI's sha256, the commit, the scorer id
    and the function-set id.
  - It resumes from per-binary checkpoints, and refuses a different CLI or
    function set in the same run.
  - A call that crashes or times out is bisected until the failing function is
    alone, so the other functions of the binary keep their output.
- **`diff`** compares output hashes first and shows scores only for rows whose
  output changed. It refuses runs scored by different scorers. Compare output
  before reading score deltas: rows that move with identical text are scorer
  noise.
- **`report`** gives scores, failures and timing; `--json` for the numbers.

Runs go to `runs/<label>/` under the current directory (`--runs` to change it).

For a quick pre-check before a full run, slice the corpus:
- `--builds gcc-O2,armv7-clang-O0` for some builds;
- `--only <project>` for binaries whose `<build>/<project>/<binary>` contains
  a substring.

A slice is a pre-check, not the evidence for a quality claim.

Measurements run with telemetry off. `sweep run` refuses to start while
`FRESHBENCH_SENTRY`, `FISSION_SENTRY` or `SENTRY_DSN` is set.

## Anchoring a change

A quality change starts from a concrete row:
`<build>/<project>/<binary>` and the function's address. The address comes
from the unstripped binary under `binaries/`, and the target list from
`truth/<build>/<project>/targets.json`. Decompile it from the stripped copy:

```bash
fission_cli decomp ~/freshbench-data/m1/stripped/gcc-O2/<project>/<binary> --addr 0x...
```

Then generalise to an invariant, implement it at its owner, and remeasure the
same rows and the whole corpus with the same scorer. `AGENTS.md` has the
evidence rules.
