# Reuse FID instruction bindings

Mechanical performance validation, 2026-10-15. **Kuna-level speed is not achieved.** This preserves producer outputs; it does not claim semantic/readability/score/ranking improvement.

## Source / invariant

Baseline main `416f441a1c341bb6ffddec081b2492ddb969f0ac`, release SHA256 `26dc790eed98282e603e8ee6b3ad91ba40590c9c61b25c51a25d72f38bdb8d0c`. Candidate release SHA256 `6f39d552d6dee077a45272ef89cc9821091eb4b327ca1d984b97472a9d080762`. [Compact evidence](fid-bound-state-performance.json) records exact source hashes/tree/patch and measured ranges. The three production/test source files match the frozen source snapshots. No runtime dependency, CLI/schema, adapter, archive format, function population, instruction limit, worker policy or FID mixing rule changes.

The continuous native baseline sample (`benchmark/artifacts/performance/2026-10-15-whole-preparation-profile/whole.txt`) exposes FID binding/lifting and allocation/drop work. Its six complete non-duration outputs match the immutable baseline. Sample leaf/inclusive counts are diagnostic, not elapsed-time percentages. Source inspection shows FID obtains an owned raw instruction binding and then decodes/binds/lifts that same bytes/address/default-context instruction again for each unresolved operand.

FID now completes that existing binding with the ordinary commit validation, checked length, exact emitter options/memory window, userop/details transport and full native description validation. The description helper borrows state. Any completion failure falls back to the normal decoder, preserving later-candidate success/error selection. The normal decoder's own error boundaries are unchanged. Only successful p-code is reused across operands of one instruction; resolver calls, operand order and final mixing still occur normally. No full bound tree or final hash/match is retained across instructions or functions. The existing reselect/verify/build-debug flags retain the original repeated decoder replay to preserve diagnostic behavior.

## Population / controls

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 250 stripped functions / 45 cells (Linux 108, Windows 97, macOS 45). All 45 input SHA256 values were reverified; sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Debug/source oracles are excluded from producer inputs. Input identities/commands/full results are retained locally.

M4 Pro, 14 logical CPUs, 24 GiB, native release with LTO / one codegen unit. Fresh processes, `--no-db`, `DECBENCH_NO_CACHE=1`, same addresses/resources/cwd/30-second function limit. Internal runtime caches remain enabled. No own build/test/profiler overlaps timing controls; OS caches are not flushed and desktop scheduling is uncontrolled.

## Matched baseline / candidate

Six requested functions in the large Windows input, three alternating pairs, existing default render/FID workers. Medians:

| Measurement | Baseline | Candidate | Change |
|---|---:|---:|---:|
| Wall seconds | 14.5483 | 13.2148 | -9.2% |
| CPU seconds | 121.3059 | 106.1036 | -12.5% |
| Maximum RSS bytes | 2,412,118,016 | 2,425,798,656 | +0.6% |

Wall ranges: baseline [14.32590012496803, 14.625063417013735], candidate [13.136208249954507, 13.788345000008121] seconds. RSS ranges: baseline [2392899584, 2424733696], candidate [2407956480, 2467364864] bytes. The memory measurement does not establish a memory reduction.

Small inputs, five alternating pairs, one requested render worker (native preparation/FID retain their existing worker width):

| Cell | Baseline seconds | Candidate seconds |
|---|---:|---:|
| 03 | 1.9378 | 1.9431 |
| 27 | 0.8634 | 0.8630 |
| 40 | 0.9731 | 0.9820 |

Differences stay within about 1%; no meaningful small-input latency gain is claimed. Every repeated complete non-time record matches.

## Fixed Kuna 1.808

Four batches / 22 identical requested addresses. Three alternating runs/tool/batch in each lane, requested jobs/render workers=1 or 14; the 14 lane also fixes Rayon=14. Fission render=1 does not serialize FID/preparation. Each tool selects its own faster observed median; this does not compare Fission 14 only against the slower Kuna 14 setting.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.9387 | 0.1883 | 1.3625 | 0.3310 | 7.23x |
| 22 | 14.3669 | 2.8606 | 13.1122 | 5.6368 | 4.58x |
| 27 | 0.8619 | 0.1110 | 0.6895 | 0.2486 | 6.21x |
| 40 | 0.9689 | 0.3297 | 0.9265 | 0.7030 | 2.81x |

Fission remains **2.81–7.23x slower** in this rerun. The earlier 2.73–6.10x range came from separate controls; Kuna's Linux median also fell, so these ranges must not be interpreted as a paired before/after Fission regression or improvement. This is bounded native CLI latency, not whole-binary/corpus throughput, latest Kuna, or quality equivalence. Kuna executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`.

## Validation / limits

- Nine targeted tests pass. The 1,024-input four-ISA matrix (746 lift successes, 278 errors, 27 parser traces) preserves complete lift tuples/error chains and raw bound-state records: 41,939,354 uncompressed bytes, SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Where binding succeeds, completing it also equals the ordinary full rebind result.
- A separate 512-input matrix produces 383 eight-instruction extents and 473 successful hashes across two resolver policies. Every hash and register callback order equals the pre-change baseline, SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`. The synthetic resolver is a deterministic callback control, not a register oracle.
- Explicit register/SIB/RIP/string-memory, AArch64 pair pre/post-index, Thumb context-commit and MIPS present/truncated-delay-slot controls pass. A p-code-emittable state with invalid display still fails full completion; invalid commit and length controls also fail. Existing Ghidra FID vectors remain unchanged. Crossbuild uses the unchanged emitter and options; no dedicated crossbuild fixture was exercised in this cycle.
- **1,956 related nextest tests pass, six skipped** across sleigh/signatures/static/pcode/decompiler/CLI. Crate checks, scoped CI Clippy, release CLI, workspace fmt and owner-boundary scan pass. Existing example warnings remain unchanged.
- Fresh **250/250** outputs match all complete non-duration function records against the immutable baseline. Recursively excluded fields are exactly `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. All other fields, including code/types/variables/PCs/maps/errors/fallbacks/evidence/counters, are compared. Whole population was regenerated once; focused sets repeat five pairs and three times in each Kuna worker lane. All 24 repeated Fission lane outputs match full fresh replay records. Both tools return the exact requested address sets, successful exits, nonempty/error-free outputs; Kuna code/variable/map digests repeat.
- No recovered/guest execution, recompilation, metric rescoring, external quality benchmark or leaderboard update is claimed.
- Three earlier prototypes (borrowed diagnostic frames, probe-buffer representation, observed-fallthrough reuse) were rejected after native controls showed no useful latency reduction. Their patches/binaries/rejection notes are archived separately and are absent from this change.

Artifacts: `benchmark/artifacts/performance/2026-10-15-fid-bound-state/`, including immutable binaries, frozen source patch/tree/hashes, proposals, input identities, all commands/JSON/stderr, comparison drivers and validation logs. Baseline/replay reference: `../2026-10-15-fid-candidates/`. Reproduce by running the logged validation commands, freezing the release source/binary with `freeze.py`, then `controlled.sh` and `summarize.py` in that artifact directory with the same inputs/resources and no overlapping workload. This experimental driver uses explicit local paths; production resource resolution remains unchanged.
