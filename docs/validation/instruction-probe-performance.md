# Instruction binding probe reuse: measured performance

## Scope and source

Mechanical runtime optimization only. Keep the original complete second constructor walk whenever binding reads the root fallthrough estimate (`inst_next`). Otherwise reuse the complete first probe state. The shared observation flag is local to that instruction and covers descendant exports. Full p-code emission and CFG construction still run. No instruction results are shared between images, byte windows, addresses or context overrides; no persistent instruction cache, new dependency, CLI/JSON change, worker-policy change, FID shortcut or no-return shortcut is introduced.

Base main: `2b1fa5c6adb54c7b8b98266c043f098d2fd2a8db`, whose complete tree equals `686f52329361588ae93cf98816a5dd8455f293e5`. The baseline binary was built from that clean equivalent source; this candidate is exactly that tree plus the production/test patch recorded in the evidence index. Drafts #203/#204 are not prerequisites or included. Original worktrees and unrelated changes are preserved.

- Baseline executable SHA256: `2ebaf8d171886efe8e1d14fa5f4b6d816708f97313ac2331a213642fe48685fd`.
- Candidate executable SHA256: `b2731b579df71ecd640d3026b388c25be15ccf0ddf6b56c4724817170b927749`.
- [Proposal](../proposals/2026-10-15-instruction-probe-reuse.md), [compact raw measurements](instruction-probe-performance.json).
- [Qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification), [frozen revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489), [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).

## Measurements

Apple M4 Pro, 14 logical CPUs, 24 GiB RAM, native arm64 binaries built with the repository release profile (`lto=true`, one codegen unit). All commands start fresh producer processes with `--no-db` and `DECBENCH_NO_CACHE=1`; internal preparation caches retain their existing process-local behavior. OS caches were not flushed and desktop scheduling is uncontrolled. No own build, test or profiler ran during the timed comparisons. Both builds use the same fixed resources and working directory.

Five alternating fresh-process pairs per small batch. `FISSION_RUST_DECOMP_WORKERS=1` controls function rendering; FID retains its existing Rayon policy. Numbers are whole-batch wall medians, not per-function times.

| Frozen cell / requested batch | Baseline seconds | Candidate seconds | Wall reduction |
| --- | ---: | ---: | ---: |
| 03 / Linux AArch64 Snappy O0 / 6 functions | 2.3426 | 2.1588 | 7.8% |
| 27 / Windows x64 Snappy O2 / 5 functions | 0.9785 | 0.9377 | 4.2% |
| 40 / macOS arm64 Ninja O2 / 5 functions | 1.4099 | 1.0196 | 27.7% |

Three alternating pairs on the large Windows x64 Tracy O0 batch (cell 22, six requested functions), default worker policy, `/usr/bin/time -l`, one producer at a time:

| Median | Baseline | Candidate |
| --- | ---: | ---: |
| Whole-batch wall seconds | 42.9857 | 31.6041 |
| Child CPU seconds | 229.8420 | 152.1112 |
| Maximum RSS bytes | 3,883,384,832 | 3,897,802,752 |

Wall time falls **26.5%**, CPU time **33.8%** in this bounded comparison. Observed memory remains about 3.9 GB; no memory reduction is claimed. Timed and profiling runs are separate. Post-PR206 baseline samples at 12/24 seconds show sequential no-return preparation inside CF initialization, mostly parser binding, while other Rayon workers wait; these samples motivate the owner change and are not performance timings.

## Output preservation and tests

- Fresh generation of all **250 stripped requests across 45 cells** returns 250 function records, with **zero changed non-time function records** against the retained clean-main replay. All 45 producer input hashes match the frozen population.
- Compare every function field recursively, removing exactly seven duration keys: `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. This includes all emitted code layers, variables/types, addresses, errors/fallbacks, evidence and counters. Root build metadata is outside this comparison.
- Five-pair small repeats and three-pair large repeats are deterministic for complete non-time function records. The separate three-run Fission/Kuna lane also verifies complete Fission non-time repeat determinism for its 22 functions. The entire 250-function population is replayed once; full-population two-run determinism is not claimed.
- Three focused tests pass: complete parser-state comparison with the original unconditional two-walk algorithm on x86, AArch64 and Thumb (prefixes, register/immediate/memory operands, relative calls, RIP memory with trailing immediates, pre-index stores and context overrides/IT commits), plus observation sharing/isolation. These are successful-bind comparisons; the first probe's failure propagation remains before the new gate. Existing runtime error, context, delay-slot and FID golden tests remain in the full suite.
- **1,831 tests pass, five skipped** across sleigh/static/pcode/decompiler/CLI. Crate checks, release CLI, workspace fmt/diff checks and owner-boundary scan pass. Remote PR CI is required separately and was not run at evidence recording time.

No guest binary or recovered source was executed. This is output preservation on a bounded corpus, not source-semantic execution parity, a new quality score or a leaderboard result. The optimization intentionally produces the same output.

## Pinned Kuna comparison

Kuna **1.808**, not a claim about its latest release. Three alternating fresh-process runs per tool/batch, one requested function worker/job; same stripped requests and fixed resources. All requested addresses return, Kuna code is nonempty with no reported error, and code/variable/map repeat evidence is deterministic within each tool. Different tools perform different preparation work; cross-tool semantic equivalence is not established.

| Cell | Fission median seconds | Kuna median seconds | Fission / Kuna |
| --- | ---: | ---: | ---: |
| 03 | 2.0879 | 0.1932 | 10.81x |
| 22 | 32.1566 | 2.7577 | 11.66x |
| 27 | 0.9369 | 0.1154 | 8.12x |
| 40 | 1.0305 | 0.3356 | 3.07x |

**Kuna-level speed is not achieved.** Full-program no-return/FID decoding and retained decoded IR remain costs. Sharing across those analyses still requires proving identical byte-window and context contracts; this patch only removes redundant walks within an individual instruction bind.

## Reproduction and artifacts

The frozen 45-cell input paths, hashes and address lists are already checked in under `population` in [decompiler-startup-performance.json](decompiler-startup-performance.json). Extract each cell's requested addresses to a text file. Run both pinned release binaries against the stripped input, with identical resources and cwd:

```bash
FISSION_RUST_DECOMP_WORKERS=1 DECBENCH_NO_CACHE=1 "$FISSION_BIN"   --resource-root "$RESOURCE_ROOT" decomp "$STRIPPED_INPUT"   --addresses-file "$ADDRESSES" --json --no-db --layer nir   --no-header --no-warnings --timeout-ms 30000
```

For the large comparison, unset `FISSION_RUST_DECOMP_WORKERS`, wrap the command with `/usr/bin/time -l`, and cap the external process at 600 seconds. Alternate baseline/candidate order across repetitions. Replay all 45 cells with two independent producer processes at most for regression checking; do not use those concurrent replay timings as a speed comparison.

Full scripts, commands, stdout/stderr, source/binary hashes, input/reference indexes and native sample reports remain in `benchmark/artifacts/performance/2026-10-15-decode/` locally. `paired.py`, `large.py`, `vs_kuna.py`, `replay.py` and log SHA256s are recorded in the compact JSON. The baseline reference is the prior clean-main candidate's `actual-main-replay/replay-<cell>.json`, authenticated by the reference index. No result cache or oracle source/debug input is passed to either producer.
