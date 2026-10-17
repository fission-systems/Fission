# Metal literal-presence prototype: measured negative result

Date: 2026-10-18. Base: `49ea4ebc175a4b28b28ed17266c367c42f3caea0`.

## Decision and scope

Keep the existing production CPU loader/CLI path. This standalone Metal prototype
is slower on the measured literal-presence workload, including resident input.
It does not justify GPU adoption for this operator. This is not a claim about
all GPU algorithms, large pattern sets, other GPUs, or the complete loader.

The experiment implements exact case-sensitive presence for nonempty UTF-8
literals in binary bytes. CPU1 uses the escaped `regex::bytes::RegexSet` primitive
from `DieMatcher::fill_string_cache`. CPU14 is a new experimental comparator,
not the current production path. Metal assigns byte positions to threads and
buckets candidate literals by their first byte. GPU errors fail the experiment;
there is no hidden CPU fallback. Product source, dependencies, CLI and formats
are unchanged. The tools are under
`crates/fission-loader/examples/metal_scan_prototype/` with reproduction instructions.

## Frozen inputs and correctness

Apple M4 Pro, 14 logical CPUs, 24 GiB, macOS 26.5.1 (25F80).
Rust 1.97.0 (`2d8144b78`), Apple Swift 6.2.4.
[Qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489)
revision `07dd95dbe6214b8a78fc1e4a55961ab515986489`.

Both runs checked 45 complete stripped files against 25 unique literals from
20 checked-in PE signatures: all 1,125 CPU/GPU presence flags agree per run.
These literals are scanned against entire files from all formats; this does not
reproduce DIE format filtering, scan budgets, or complete detector evidence.
This is 45 binary scans, not 250 function decompilations or a benchmark score.

Each run also passes 21 fixture cases across four backends, with two scans each
(168 complete flag-vector checks). Cases cover empty input/pattern sets,
duplicates, overlap, NUL/invalid input bytes, Unicode, literal regex characters,
long needles, last offsets, GPU/CPU chunk boundaries and 12 seeded random cases.
All timed output vectors were checked too. Source snapshots and all 45 input
hashes were rechecked; executable and shader hashes are retained.

## Timing protocol

All units below are **milliseconds**, medians of per-process results. Builds
finish before measurements. Fresh means external wall time for a new process,
including reads, matcher/device/runtime shader/pipeline setup, first result and
JSON. It does not guarantee cold shader, driver, or OS caches. GPU runtime
setup materially varies between runs, so both runs are shown.

Warm means seven operations per process, discarding the first; take the median
of the remaining six, then the median across processes. CPU retains its matcher;
GPU retains pipeline/pattern buffers. Upload includes per-operation input
allocation/copy, submit, wait and result read. Resident retains input and is a
separate persistent-workspace scenario. Shared physical memory does not remove
the explicit input copy used here. Fresh file-read implementations differ
between Rust and Swift; raw reports separate read/setup/operation timings.
Execution order alternates; confirmation reverses the initial order. No own
build or profiler overlaps measurement. Scheduling/thermal state is not fixed;
no statistical significance or end-to-end loader speed claim is made.

### initial (3 rounds)

| Input bytes | Mode | CPU1 | CPU14 experimental | GPU upload | GPU resident |
|---:|---|---:|---:|---:|---:|
| 53776 | fresh | 2.538041 | 2.939666 | 31.627500 | 31.500208 |
| 720896 | fresh | 2.778500 | 2.808125 | 32.216375 | 33.502750 |
| 39033344 | fresh | 10.304416 | 7.932792 | 49.218042 | 47.143791 |
| 53776 | warm | 0.004209 | 0.030146 | 0.410375 | 0.446313 |
| 720896 | warm | 0.058458 | 0.049792 | 0.760251 | 0.736250 |
| 39033344 | warm | 3.314291 | 0.628146 | 7.047708 | 4.191063 |

### confirmation (5 rounds, reversed first order)

| Input bytes | Mode | CPU1 | CPU14 experimental | GPU upload | GPU resident |
|---:|---|---:|---:|---:|---:|
| 53776 | fresh | 2.762458 | 3.568208 | 53.399291 | 50.466292 |
| 720896 | fresh | 3.238875 | 3.593208 | 50.835708 | 49.695042 |
| 39033344 | fresh | 10.607792 | 8.574834 | 72.038042 | 65.250292 |
| 53776 | warm | 0.004834 | 0.051062 | 0.185667 | 0.175292 |
| 720896 | warm | 0.057063 | 0.074959 | 0.809083 | 0.678354 |
| 39033344 | warm | 3.221188 | 0.586688 | 7.883396 | 4.178479 |

The large input's resident GPU operation remains about 4.18 ms in both runs,
versus CPU1 3.22–3.31 ms and experimental CPU14 0.59–0.63 ms. Setup/copy costs
are therefore not the only reason this prototype loses. CPU parallelism is a
candidate for separate owner-level measurement, not an already landed speedup.

## Evidence and limits

Compact hashes/input identities/medians are in `metal-literal-scan-results.json`.
Complete commands, source snapshots, compiler versions, binaries, fixtures,
stderr and per-operation JSON remain locally under:

- `benchmark/artifacts/performance/2026-10-18-metal-literal-scan/`
- `benchmark/artifacts/performance/2026-10-18-metal-literal-confirmation/`

No wildcard/full-regex matching, GPU parser, program metadata, SLEIGH, NIR/HIR,
type/address mapping, semantic quality, rankings or execution parity is measured.
Inputs/pattern pools must fit UInt32, single-dispatch and device buffer limits.
No cross-platform GPU backend or automatic routing policy is proposed.

## Local validation

- `cargo nextest run --locked -p fission-loader`: 171 passed, zero skipped.
- `cargo check --locked -p fission-loader --example literal_scan_cpu`: passed.
- `cargo fmt --all --check` and Python `py_compile`: passed.
- Release CPU example and optimized Swift binary built before both real Metal runs.
- Strict Clippy (`-D warnings`) stops in unchanged
  `crates/fission-core/src/core/ghidra_no_return.rs:377` (`clippy::question_mark`).
  This existing warning was not repaired as part of the prototype.
- Production CLI binary hash and the seven unrelated linked-main files were
  verified unchanged after measurement. No complete loader/decompiler benchmark
  was run for a speed claim, since this primitive did not improve.

Metal execution is local macOS evidence. Ordinary Linux CI cannot validate
this device-specific kernel; no remote GPU validation is claimed.
