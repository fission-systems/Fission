# Decompiler preparation performance — 2026-10-14

This mechanical patch removes repeated allocations and reachability queries. It does not claim improved semantic/readability quality, execution parity, a corpus-wide speedup, or an official benchmark ranking.

## Accepted changes

- Borrow the existing process-lifetime SLEIGH registry and entry catalogs; clone only the selected frontend entry. Public owned entry discovery stays independently mutable.
- Compute operation-input reachability over the existing out-of-SSA edge-copy graph once per emission plan and reuse it in the two existing phi filters. Return-ABI exceptions and observable-use handling are unchanged.

No production dependency, pass, CLI option, worker policy, type policy, timeout option or output schema changed. The two NIR/HIR builds retain their separate policies. Whole-program control-flow snapshot borrowing was explored but is excluded.

## Fixed inputs and release builds

Frozen [qualification corpus revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): `07dd95dbe6214b8a78fc1e4a55961ab515986489`. All 45 stripped-input hashes were rechecked; the 250-address population and normalized output hashes are in [the compact evidence](decompiler-preparation-performance.json). Only stripped bytes and requested entry addresses go to the producer. Source/debug information is not supplied.

Baseline source: `87825864d` (main `52c81dd15` plus PR #203 scoped line provenance). Candidate production commit: `308d0c317` on that source. Both were built with the same release profile (thin LTO / one codegen unit from the repository configuration), rustc 1.97.0, native arm64. The x86-64 compiled SLEIGH cache hashes also agree.

Baseline CLI SHA256: `20b4ccf68b5562084953fb4441b0e84dea25c1151b1dc1b325d05e36505571d0`.
Candidate CLI SHA256: `63e11b25c2412e510351e03396434ba2f51103300eab52ba83a5b60e684d1db5`.

## Controlled small-batch timing

Apple M4 Pro / 14 CPUs / 24 GiB RAM. Five alternating fresh-process samples per build and batch; requested-function workers fixed to one. The user stopped other builds/benchmarks, and this task had no concurrent builds/tests. OS caches were not flushed; desktop/OS scheduling and startup effects remain. Both executables use the basename `fission_cli` and the same resource root, cwd, flags and inputs. CPU seconds are the child process user + system time, and can exceed wall time.

| Batch | Functions | Baseline wall median, s | Candidate wall median, s | Wall decrease | Baseline CPU median, s | Candidate CPU median, s |
|---|---:|---:|---:|---:|---:|---:|
| Linux AArch64 / Snappy O0 | 6 | 2.3934 | 2.3243 | +2.89% | 3.8443 | 3.6689 |
| Windows x86-64 / Snappy O2 | 5 | 1.2901 | 1.2525 | +2.91% | 1.7925 | 1.7872 |
| macOS arm64 / Ninja O2 | 5 | 1.4918 | 1.4878 | +0.27% | 3.1716 | 3.2926 |

These samples show modest wall-time reductions on Linux/Windows and nearly unchanged macOS time. macOS median CPU time increases; Windows CPU time is nearly unchanged. Five samples do not establish a robust platform-wide gain. Raw samples are retained rather than attributing every difference to the patch. This does not close the previously observed gap to Kuna.

## Large-input control

One sequential candidate-first / baseline-second Tracy O0 pair, using the same default worker policy, CLI timeout of 30,000 ms and external process cap of 600 seconds. It runs after the parallel replay exits. Both outputs match the frozen reference. This is a regression control, not a statistically robust speed estimate.

| Build | Wall seconds | CPU seconds |
|---|---:|---:|
| baseline | 155.854 | 1554.789 |
| candidate | 149.095 | 1590.412 |

## Output preservation

Fresh candidate generation returned **250/250** entries with **zero changed functions**, across Linux / Windows / macOS and all three optimization modes. All code layers, variables/types, direct PCs, scoped line mappings, errors/fallbacks, evidence and non-time counters were compared. Only these existing duration keys were excluded: `build_duration_ms`, `decomp_sec`, `normalize_duration_ms`, `postprocess_sec`, `render_duration_ms`, `structuring_duration_ms`, `total_time_ms`.

The replay uses default requested-function worker policy and two binary processes at a time, matching the retained baseline-generation policy. It uses `--no-db` and no decompilation result cache. Five repeated samples of each of the 16 timed functions are deterministic for both builds; repeated generation of all 250 functions is not claimed. The 16 timed functions are a subset of the 250 replayed entries. No corpus executable was run. This is producer-output parity, not source execution parity or C++ policy qualification.

## Local validation

- Final nextest across fission-sleigh, fission-pcode, fission-decompiler and fission-cli: **1,752 passed / 4 skipped**.
- Related crate checks, release CLI build, workspace fmt, diff check and owner-boundary scan passed.
- The graph equivalence test checks every directed three-value graph, every seed set and every root: **12,288** comparisons against the previous recursive predicate.
- Tests prove shared catalog storage and independence of the owned discovery result.
- An initial SLEIGH full run had two walker failures (decision-tree node index / expected parser backtrace). A repeat passed all 328 tests; baseline full run passed 326. Isolated fresh-cache runs of both affected tests passed for both builds. The initial cause was not established. The final owner suite passed.
- Remote CI remains a separate gate and is not inferred from local checks.

## Rejected experiment

An earlier snapshot-borrowing prototype preserved all 250 outputs, but its sequential large-input pair took 250.183 s baseline / 331.526 s candidate while concurrent machine work had been observed. The cause was not isolated. That production change is excluded, and the raw evidence plus the local experiment branch are preserved. Earlier Arc catalog views were replaced with direct static borrows because the existing catalog already has process lifetime. No performance attribution to that representation change alone is made.

## Reproduction and raw artifacts

Raw artifacts live under `benchmark/artifacts/performance/2026-10-14` in the primary local checkout: immutable final-baseline/final-candidate binaries, `compare.py`, `controlled_large.py`, `controlled-input-hashes.json`, `controlled-final/summary.json`, `controlled-large/summary.json`, every output/stderr, symbolized profiles and `review-final-*` validation logs. The compact evidence committed with this report contains the raw paired times, population, hashes and duration exclusions. Local drivers use the installed corpus/resource paths and need those paths adjusted elsewhere.

Representative producer invocation (replace paths; put the selected cell addresses from the compact evidence in a newline-separated addresses file):

```sh
FISSION_RUST_DECOMP_WORKERS=1 /path/to/fission_cli \
  --resource-root /path/to/Fission decomp /path/to/qualification-v2/inputs/... \
  --addresses-file /path/to/addresses.txt --json --no-db --layer nir \
  --no-header --no-warnings --timeout-ms 30000
```

Unset `FISSION_RUST_DECOMP_WORKERS` for the 250-entry replay and large control. Alternate build order for the small samples; run the large control sequentially. Regenerate outputs rather than using decompilation-result caches.

Next performance investigation should isolate whole-program preparation and large-function analysis costs before changing worker parallelism. The current patch only removes two proven forms of duplicate work.
