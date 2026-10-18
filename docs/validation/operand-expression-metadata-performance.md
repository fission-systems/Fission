# Borrowed operand-expression metadata: performance and preservation

Mechanical CPU performance validation, 2026-10-18. **Kuna-level speed is not achieved.** No decompilation quality, score, ranking, recompilation or guest-execution claim. [Compact evidence](operand-expression-metadata-performance.json); [pre-implementation proposal and owner proof](../proposals/2026-10-18-borrow-operand-expression-metadata.md).

## Source, compiler and owner

Baseline main `49ea4ebc175a4b28b28ed17266c367c42f3caea0`; candidate production/test patch SHA256 `ddb588b582b65de608550fe923a2c58c404195306429245a63a97ef1894f6d06`. Immutable release executables: baseline `7a3eb9c923537a3280fb80407822fa1a35188b1c8bb71e25279249347ef3ae61`, candidate `00b474686d53c8b9e21184bdeeb2f8989af186fe89176e966ed9cbdfa9566c84`. Both use Rust 1.97.0 (`2d8144b7880597b6e6d3dfd63a9a9efae3f533d3`), release opt-level=3/LTO=true/codegen-units=1, the same isolated checkout, resources and inputs. Source and both executables are reverified after measurements. The unrelated linked-main files are preserved.

A symbol-enabled diagnostic from the identical baseline source tree, with complete output equal to the accepted release, recorded 448,881,845 positive sampled thread-CPU microseconds. `eval_pattern_expression` self-weight was 151,582,399; offsets 0x294 and 0x5e0 contributed 79,203,318 and 64,036,441 immediately after the atomic increment/decrement instructions of an inlined operand-spec Arc clone/drop. These weights diagnose CPU work; they are not wall-time percentages. Diagnostic binary times are excluded from acceptance. An initial symbol-demangling hypothesis was not implemented.

`RuntimeSelection<'a>` stores `&'a Arc<CompiledExecutableConstructor>`. The constructor and operand specs are immutable frontend metadata. A private typed lookup returns `&'a CompiledOperandSpec`; it remains valid through mutable/recursive walker calls. The evaluator no longer creates a temporary Arc owner solely to avoid borrowing the mutable walker. Returned handle/state ownership, offset and expression evaluation order, errors, context/PC witnesses, and actual decoding remain in the existing owner. No new cache, unsafe code, dependency, ISA/function/address predicate, work limit, CLI/schema or program-fact map. Ghidra `context.hh` was consulted for constructor/walker lifetime invariants only; no copied implementation or vendor dependency.

## Native measurements

M4 Pro, 14 logical CPUs / 24 GiB. Fresh sequential CLI processes; no own build/test/profiler overlaps timings. Same resource root, cwd, address sets, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second per-function limit. Existing internal runtime/resource caches remain enabled. OS cache and desktop scheduling are uncontrolled. **`info` includes function discovery and decode preparation, not just loader parsing.** This is not cold-storage latency or whole-corpus throughput evidence.

Primary workload: real 156 MiB arm64 Mach-O `libLLVM.dylib`, using `info --json --no-db --identity --detections`. Initial three alternating pairs gave a 32.37% separate-median wall reduction. Five independent pairs invert the initial order; the predeclared >=2% paired-median wall reduction / <=2% CPU regression gate passes. All complete metadata agrees in all 16 runs.

| Input | Baseline wall (s) | Candidate wall (s) | Separate-median reduction | Paired-median reduction | Paired CPU reduction |
|---|---:|---:|---:|---:|---:|
| llvm | 40.443693 | 26.449099 | 34.60% | 34.31% | 38.80% |

Maximum RSS confirmation medians: baseline 2,854,584,320 bytes; candidate 2,767,503,360 bytes. Ranges are retained in compact evidence; this change does not guarantee a memory reduction.

Two other actual large LLVM inputs, three alternating pairs each, complete metadata equal:

| Input | Baseline wall (s) | Candidate wall (s) | Separate-median reduction | Paired-median reduction | Paired CPU reduction |
|---|---:|---:|---:|---:|---:|
| mlir | 20.672878 | 17.096010 | 17.30% | 17.30% | 35.42% |
| mlir-opt | 10.649719 | 8.130759 | 23.65% | 23.83% | 37.81% |

Their median RSS bytes (baseline/candidate): libMLIR 2,259,648,512/2,283,388,928; mlir-transform-opt 1,400,438,784/1,397,686,272. Broader controls are not tuning targets; all three large inputs are from the same LLVM distribution and do not establish platform-wide speedups.

Four frozen qualification batches, five alternating pairs each, same complete non-duration output. All satisfy the no >2% separate-median latency regression control:

| Input | Baseline wall (s) | Candidate wall (s) | Separate-median reduction | Paired-median reduction | Paired CPU reduction |
|---|---:|---:|---:|---:|---:|
| 03 | 1.263215 | 1.251894 | 0.90% | 1.45% | 4.38% |
| 22 | 6.994536 | 6.934430 | 0.86% | 0.84% | 1.70% |
| 27 | 0.630840 | 0.631304 | -0.07% | 0.05% | -0.00% |
| 40 | 0.888339 | 0.858743 | 3.33% | 3.19% | 16.04% |

These smaller changes do not establish a general small-input gain. Cell 22 is the large x86-64 Windows six-request input; the other three are bounded Linux/Windows/macOS controls. Full per-run CPU, RSS, commands and outputs remain in artifacts.

## Fixed Kuna comparison

Four frozen batches / 22 requests, three alternating native runs per tool/setting. Fixed Kuna 1.808 executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`. Both use exact requested addresses and 30-second limits; jobs/render 1 and 14 measured separately, with Rayon 14 in the 14 lane. Fission render 1 does not serialize preparation. Each tool's faster observed median supplies the final ratio. No equal-quality or latest-Kuna claim.

| Cell | Fission jobs 1 (s) | Kuna jobs 1 (s) | Fission jobs 14 (s) | Kuna jobs 14 (s) | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.8334 | 0.1844 | 1.2483 | 0.3217 | 6.77x |
| 22 | 8.1051 | 2.7029 | 6.9358 | 5.4170 | 2.57x |
| 27 | 0.8075 | 0.1078 | 0.6320 | 0.2440 | 5.86x |
| 40 | 0.9031 | 0.3205 | 0.8660 | 0.6759 | 2.70x |

## Preservation and validation

Frozen [qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489): 45 stripped inputs / 250 requests (Linux 108, Windows 97, macOS 45); sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Rechecked all 45 corpus inputs and three large files against previously frozen hashes. Source/debug truth is not supplied to Fission. [Dataset](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification); [earlier raw corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-raw-corpus).

- Three targeted nextest tests pass: frontend lifetime/pointer/strong-count proof plus original clone-based evaluator comparisons on x86-64, AArch64 and Thumb. Compare values, full logical error chains and the selected mutable walker fields. The independent reference is test-only.
- **2,404 related nextest tests passed / 9 skipped** across loader, analysis-db, sleigh, signatures, static, pcode, decompiler, CLI and emulator. Crate checks, CI-scoped Sleigh Clippy, release CLI, workspace fmt and owner-boundary scan pass. Existing unrelated example warnings remain.
- Initial full validation exposed a new test-harness issue: Debug error strings included the different physical evaluator backtraces. Compare logical error-chain messages instead; rerun targeted and full tests with `RUST_BACKTRACE=1`, all passed. Retain the failed log. No production behavior was changed to silence that failure.
- Complete raw-state/lift/error matrix 1,024 records is byte-identical (746 successes /278 errors /27 traces), uncompressed SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. The 512-record FID hash/resolver matrix is byte-identical, SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`. Host backtraces are not part of the raw matrix contract.
- Fresh matched-baseline/candidate **45 complete identity/detections JSONs and 45 whole-function lists /334,275 records** are equal with no field exclusions. This does not serialize every internal LoadedBinary/ProgramSnapshot field.
- Fresh matched-baseline/candidate **250/250 complete function records**, all 250 nonempty, are equal. Code, NIR/HIR, types, variables, addresses/maps, errors/fallbacks, evidence and counters are included. Recursively exclude exactly `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`.
- All 64 repeated Fission control/Kuna-lane outputs equal the fresh full replay after the same duration exclusions. Kuna selected code/variable/map digests repeat per setting. Full 250 replay is generated once per executable; no full-corpus determinism or execution-parity claim.

## Reproduction and bounds

Pin compiler and both immutable executable hashes. Run targeted nextest `-p fission-sleigh -E 'test(operand_expression_)'`; related nextest/checks; `cargo clippy -p fission-sleigh --locked -- -D warnings -A clippy::style -A clippy::complexity`; `cargo build --locked -p fission-cli --release`; workspace fmt and `bash scripts/check/owner_boundaries.sh`. Native scripts `compare.py initial`, `compare.py confirm`, `accepted-followup.sh`, `corpus.py`, `freeze-verify.py` and all commands/stdout/stderr, binaries, profile/symbols/disassembly, failed-test log and input/source hashes are retained under `benchmark/artifacts/performance/2026-10-18-loader-metadata/`. Local paths are measurement configuration, not production resource lookup.

GPU work remains discarded. This removes one observed CPU cost. Other preparation, allocation and decompilation costs remain; the measured Kuna gap persists. Output parity and focused regression coverage do not prove full guest-function execution, recompilability or source equivalence. No external semantic bake, score rerun, official ranking or Pages update.
