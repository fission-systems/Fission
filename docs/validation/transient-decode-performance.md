# Transient no-return / FID decode reuse

Mechanical performance validation on 2026-10-15. No semantic, readability, score, execution-parity or ranking improvement is claimed. Kuna-level speed is not achieved.

## Source and inputs

Base main `533c80c94aedc578ab53c0dea77c55243764b2a7` (same tree as `a2bda6d744ce599f5c91bdd0eb9b9bd6fa7a8936`). Baseline release SHA256 `a4e3eafa56886bfc9dc7948364b291bc683dd69e2327c0e587eda1c6467f5e6d`; candidate `5ec5b83100b10e1d5280ea22a92f6b4fce8b3894c8d50f1e18038b67e0ac1430`. Production hashes, complete source-patch hash/tree, medians/ranges and diagnostic evidence are indexed in [the compact JSON](transient-decode-performance.json).

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): same 45 cells / 250 requested stripped functions (Linux 108, Windows 97, macOS 45). Sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`; all 45 stripped input hashes rechecked. The existing population catalog is `decompiler-startup-performance.json`. Source/debug oracles never enter producer inputs. Draft PRs #203/#204 and DecBench #103 are not included.

## Owner contract

A FID transaction explicitly requests first control-flow preparation before its parallel function loop. Only the winning cold CF/no-return initialization captures seeds. Ordinary callers and the global caches retain none. FID consumes a function's seed outside the shared map lock, then drops leftovers immediately after the parallel decode/hash loop. No-return remains sequential; the historical nested-Rayon/OnceLock self-wait is not reintroduced.

An opaque runtime seed owns the exact full byte window and frontend. Replay validates entry, complete bytes/bounds, language, EntrySpec, status and retained compiled Arc identity. Each raw record uses PC and the full optional context bits/mask, distinguishing None from Some(0). Capture happens before sequence-number mutation/assembly annotation. Original instructions, ops, lengths, template source and pending context commits round-trip; userop catalogs are checked/restored by Arc identity rather than serialized per instruction. Records are consuming; any missing or mismatched evidence executes the original native decoder. A private immutable archive failure is an invariant failure, never a fabricated result or silent eligible-population change.

FID still rebuilds CFG/reachability using its own memory context, additional entries, no-return sites, limits and indirect-edge inference. Whole no-return CFGs are never substituted. Its old temporary full-function map is replaced by ordered Return/Call-or-Branch target events and successors. Differential tests retain its exact offset-only target and malformed-successor behavior. Completed FID decode cache admission, cold/hot storage and downstream fallbacks are unchanged.

## Timing controls

Native Apple M4 Pro, 14 logical CPUs, 24 GiB RAM. Repository release profile: `lto=true`, `strip=true`, `codegen-units=1`. Fresh processes, `--no-db`, `DECBENCH_NO_CACHE=1`, same resource root/cwd/addresses/limits. Internal preparation caches retain their normal process-local operation. No own builds, tests or profilers ran during timed comparisons. OS caches were not flushed; desktop scheduling was uncontrolled.

Large Windows batch: six requested functions, three alternating baseline/candidate pairs, existing default worker policies. Values below are medians; CPU is process user+system time and RSS is `/usr/bin/time -l` bytes.

| Measurement | Baseline | Candidate |
|---|---:|---:|
| Wall seconds | 31.5073 | 26.9139 |
| CPU seconds | 141.8253 | 118.2984 |
| Maximum RSS bytes | 2,294,677,504 | 2,325,839,872 |

Wall falls **14.6%**, CPU **16.6%**. Median RSS rises **1.4%**; baseline range 2,176,466,944–2,346,975,232 bytes, candidate 2,325,708,800–2,363,326,464. No new memory reduction is claimed.

Small batches: five alternating pairs, one requested render worker, existing FID parallel policy unchanged.

| Cell / platform | Baseline seconds | Candidate seconds |
|---|---:|---:|
| 03 / Linux | 2.0867 | 2.0739 |
| 27 / Windows | 0.9343 | 0.9301 |
| 40 / macOS | 1.0300 | 1.0044 |

These differences are only 0.5–2.5%; they do not establish a general small-input speed gain. All non-time function records match across each timing pair and repeat.

### Pinned Kuna 1.808 comparison

Same four batches / 22 requested addresses, three alternating fresh-process runs per tool, one requested job/render worker. This lane is separate from the default-worker large comparison above.

| Cell | Fission seconds | Kuna seconds | Fission / Kuna |
|---|---:|---:|---:|
| 03 | 2.0751 | 0.1929 | 10.76x |
| 22 | 28.2183 | 2.7651 | 10.21x |
| 27 | 0.9171 | 0.1109 | 8.27x |
| 40 | 1.0060 | 0.3393 | 2.96x |

Kuna is the fixed local 1.808 executable, not a claim about its latest release. Both tools return the exact address sets with successful exits; Kuna outputs are nonempty/error-free and each tool's focused repeats are deterministic. These timings do not prove equal output quality, cross-tool semantic equivalence or a whole-corpus speed ratio.

## Output and mechanical validation

- Nine focused tests pass. Related sleigh/static/pcode/decompiler/CLI nextest: **1,847 passed / 6 skipped** (including the new explicitly local ignored probe).
- Crate checks, release CLI build, workspace fmt check and owner-boundary scan pass. Existing example warnings remain unrelated. An initial fixture used a nop-only budget with no p-code and was corrected; final targeted/full runs pass.
- Compact no-return projection matches an independent pre-change predicate over 600 generated graphs × 16 known-target subsets, including op order, missing/nonconstant targets, cycles and invalid successor indices.
- Complete raw tuple and rebuilt-function tests include x86-64, AArch64, Thumb and MIPS delay-slot behavior, changed bytes/window/frontend/entry/context, catalog identity, private archive errors, extra CFG entries and native decode errors. Additional standalone probes compare complete ARM little/big endian IT/context-commit functions, a partial same-window capture and changed initial context.
- Fresh replay: **250 returned / zero changed non-time function records**, compared to the previous clean main's immutable outputs. Compare code/layers, variables/types/PCs, line maps, errors/fallbacks/evidence/counters. Only the existing seven durations listed in the JSON are removed recursively. Whole 250-function generation was run once; focused 22-function records were also verified across three runs, and the small controls across five pairs.
- No recovered or guest program was executed/recompiled, and no GED/type/byte rescore or leaderboard publication was run. This is producer-output preservation evidence.

## Separate diagnostics and remaining costs

A loader-only local probe captured 38,186 functions / 1,930,323 raw records (408,001,368 packed bytes plus 9,922,678 owned input bytes). All 38,186 candidate windows matched the FID range helper. This is byte-window eligibility, not an observed hit rate or necessarily the CLI-prepared function population; contexts/reachability can still miss.

Native `sample` ran separately after timing controls, with a symbolized build of the identical production sources. `profile.profiling` inherits release LTO/one codegen unit, with `strip=false`/`debug=1`. Complete six-function non-time output matches the release build. The 12/20-second samples still show initial no-return raw binding/seed capture; the 28-second sample crosses later parallel FID/signature lookup. Sample counts/footprints are diagnostic snapshots, not whole-run attribution, speed timings or comparable maximum RSS measurements.

Warm CF/no-return caches can produce no seed; no analysis is recomputed just to populate one. Changed windows/frontends/contexts use the original decoder. Querying every completed function still promotes the existing cold cache to strongly retained hot IR, so this is **not an all-function memory cap**. The first no-return decode, remaining SLEIGH binding/allocation costs and FID lookup work remain targets for subsequent profiling.

## Artifacts and reproduction

Local artifacts: `benchmark/artifacts/performance/2026-10-15-transient-decode/`. Includes immutable baseline reference/candidate, `measured-source.patch`, `candidate-source.json`, input digests, `large/`, `paired/`, `replay/`, `rows-vs-kuna/`, all commands/raw JSON/stderr, test/build logs, `window-probe.log`, standalone context probe and symbolized samples. The baseline executable is `../2026-10-15-decode-storage/candidate/fission_cli`.

From the artifact directory, `large.py` runs the three large pairs, `paired.py --baseline ../2026-10-15-decode-storage/candidate/fission_cli --candidate candidate/fission_cli --label paired --iterations 5` runs small controls, `replay.py` with the same binary arguments and `--label replay` generates all 250 functions against the frozen prior output directory, and `vs_kuna.py` runs the pinned serial comparison. Paths are resolved by the retained drivers; both binaries use the fixed resource root/cwd `/Users/sjkim1127/.codex/worktrees/call-width-verified/Fission`.

Remote CI is a separate post-submission gate; this report records local validation and does not substitute for that gate.
