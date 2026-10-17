# Experimental Metal literal-presence scan

Manual, isolated macOS prototype. **Fission's production CPU loader and CLI do not
call these tools.** No new Cargo dependency, GPU requirement or product option.

The bounded operator is the presence of each nonempty, case-sensitive UTF-8
literal in arbitrary binary bytes. The CPU comparator uses the same escaped
`regex::bytes::RegexSet` operator as `DieMatcher::fill_string_cache`; it does not
run the complete detector or its source-format/scan-budget/evidence policy.
`cpu14` adds an experimental parallel comparator with byte-length overlap across
chunks. It is not the existing production implementation.

The Metal kernel assigns input positions to threads, buckets patterns by their
first byte, and atomically records presence. A complete CPU differential checks
every pattern flag. No variable addresses, oracle data or decompilation enter it.
There is no wildcard/full-regex support, program metadata owner or GPU parser.

## Run

Requires macOS with a Metal device, Xcode command-line Swift/Metal frameworks,
Python 3 and the repository's Rust toolchain/dependencies. CPU comparator is a
manual example; Swift/Metal have no role in ordinary builds. Resource/corpus
paths are explicit experiment arguments.

```sh
python3 crates/fission-loader/examples/metal_scan_prototype/run.py \
  --output /absolute/path/to/a/new/artifact-directory \
  --signatures /absolute/path/to/pe_signatures.json \
  --sample /absolute/path/to/sample.json \
  --corpus-root /absolute/path/to/qualification-v2
```

The sample must contain `cells` with `id` and corpus-relative `input` paths.
Default timing cells `41,26,22` select small/medium/large binaries in the frozen
45-cell qualification manifest; override `--timing-cells` for another sample.
All sample cells receive exact CPU/GPU presence comparison. Builds finish before
timing. Full commands, per-run JSON/stderr, inputs/source/binary hashes, compiler
versions, pattern set and timings are retained in the new output directory.

## Measurements

- Fresh process: input/pattern read, matcher/device/library/pipeline setup,
  buffers, first result and JSON output. Three alternating rounds by default.
- Warm: seven scans in each of three new processes, discarding the first scan.
  CPU retains RegexSet; GPU retains pipeline and pattern buffers.
- GPU `upload`: every operation includes input allocation/copy, submission,
  completion wait and result read. GPU `resident` keeps input in a Metal buffer;
  it is a separate persistent-workspace scenario. Result flags are reset every
  scan. GPU command timestamps are diagnostic, not the acceptance metric.
- Correctness covers empty input/pattern sets, duplicates, overlaps, NUL/invalid
  input bytes, Unicode, regex metacharacters, final offsets, threadgroup and CPU
  chunk boundaries, long needles and seeded randomized inputs.

Report CPU1, CPU14, GPU upload and GPU resident separately. Fresh-process results
do not guarantee a cold GPU/shader cache. OS caching and scheduling are not
controlled. Buffer copies remain despite shared physical memory. Prototype
inputs and pattern pools must fit UInt32/single-dispatch/device buffer limits;
GPU failures are reported, never disguised as CPU measurements.

Any production adoption needs complete rule/domain/error preservation, bounded
resource policy, CPU fallback and platform support plus real end-to-end loader
measurements. A faster primitive alone proves no loader or decompiler speedup.

For an independent confirmation use a new output directory, `--rounds 5` and
`--reverse-first` to invert the first execution order. Source hashes are frozen
before builds and checked again after measurement.

## Recorded result

The [2026-10-18 report](../../../../docs/validation/metal-literal-scan-prototype.md)
records two local runs: exact correctness, but slower GPU execution on all
three timing inputs. This prototype is retained for reproducibility; it is not
a production acceleration path.
