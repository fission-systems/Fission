# Changelog

## 2026-04-10

### Added
- Added `rustc-hash` integration across NIR builder/cache hot paths and static analysis cache maps (`callgraph`, `xrefs`, CFG traversal/build helper maps) with boundary-safe usage.
- Added `allocator-mimalloc` and `allocator-jemallocator` feature flags to CLI, automation, and tauri crates, plus opt-in global allocator wiring in executable entrypoints.
- Added canonical NIR pass-level telemetry aggregation (`pass_metrics`) to `NirBuildStats` and merge propagation support for downstream reporting.

### Changed
- Refactored NIR normalize pass execution to a pass-logged flow that records per-pass timings/reductions and emits structured tracing fields.
- Updated automation summary output to show slowest and most impactful NIR passes using aggregated pass metrics.
- Added automation regression-gate checks for pass-level timing deltas against baseline summaries.
- Expanded pcode builder internal cache maps/sets (`materialized_vns`, alias/terminator/linear caches) to fast hash aliases while preserving external type boundaries.

### Validation
- `cargo check -p fission-pcode`
- `cargo check -p fission-static`
- `cargo check -p fission-cli`
- `cargo run -p fission-cli -- samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --engine nir --profile speed --decomp-all --decomp-limit 10 --benchmark --json --timeout-ms 1200`
- `cargo run -p fission-cli -- samples/windows/x64/putty.exe --engine nir --profile speed --decomp-all --decomp-limit 10 --benchmark --json --timeout-ms 1200`
- `artifacts/compare_hash_phase2_cli_nir_diff.json` aggregate comparison: wall-clock `-2.39%` (`0.248193s -> 0.242255s`), total decomp time `+0.14%` (`0.441573s -> 0.442209s`).

## 2026-04-09

### Added
- Introduced initial Global Type Inference module (`crates/fission-pcode/src/nir/types/inference.rs`) featuring a Hindley-Milner inspired Union-Find Disjoint-Set structure (`TypeEquivalenceClass`) tailored for O(N) SSA variable type constraints.

### Changed
- Replaced `NirBlock`'s basic lexical `phis: Vec<String>` handling with explicitly modeled O(N+E) SSA `NirPhiNode` structures carrying strict source var (`SsaVarId`) mapping.
- Synchronized structuring dominator facts with CFG mutations by refreshing the cached dominator tree after irreducible-edge pruning and node-splitting, and switched `analyze_cfg_dominators()` to consume the cached fact source instead of recomputing on every call.
- Tightened terminal exit merging in linear structuring so `Return` and `End` are no longer treated as interchangeable terminal exits; merged the previous duplicated acceptance checks into a single helper to keep rule-block-if-no-exit accounting consistent.
- Added a cache-backed CFG fact bundle in structuring (`CfgFactCache`) and wired follow/postdom/frontier lookups through a single invalidation path after graph mutation to reduce stale-fact retries.
- Strengthened NIR type inference fixed-point propagation by feeding already-known binding types into alias resolution and returning an explicit changed-flag from the pass, then using that signal to converge quickly without over-iterating.
- Expanded normalization/structuring regression coverage, including a fixpoint idempotence test for type inference and broader guarded-tail/conditional follow stability checks.

### Validation
- `cargo check -p fission-pcode`
- `cargo test -p fission-pcode structuring_ -- --nocapture`
- `cargo test -p fission-pcode reports_change_and_reaches_fixpoint -- --nocapture`
- `cargo test -p fission-pcode structuring_ -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo check -p fission-automation`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/test_control_flow_x64_O0.exe --limit 10 --baseline-dir artifacts/batch_benchmark/test_control_flow_x64_O0-20260409-154945`
- 2-way regression check passed (`avg_norm` 34.69%, no degradation detected).
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --auto-limit-functions 40`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --auto-limit-functions 40 --baseline-dir artifacts/batch_benchmark/putty-20260409-180504`
- Putty 2-way regression check passed (`avg_norm` 4.34%, no degradation detected).
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --fission-bin target/release/fission-cli --pairwise-similarity-mode auto --pairwise-auto-shared-full-max 2000 --aggregate-similarity-mode weighted`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/test_control_flow_x64_O2.exe --fission-bin target/release/fission-cli --pairwise-similarity-mode auto --pairwise-auto-shared-full-max 2000 --aggregate-similarity-mode weighted`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/test_real_world_algorithms_x64_O2.exe --fission-bin target/release/fission-cli --pairwise-similarity-mode auto --pairwise-auto-shared-full-max 2000 --aggregate-similarity-mode weighted`
- 3-gate KPI snapshot:
	- putty: `avg_norm=34.03`, `both_success=99.889%`, `high_div=97.081%`, `wall_speedup=1.51x`, `throughput_speedup=3.233x`, delta overall=`degraded` (0 improved / 3 degraded / 9 unchanged).
	- test_control_flow_x64_O2: `avg_norm=35.25`, `both_success=100.0%`, `high_div=92.143%`, `wall_speedup=1.268x`, `throughput_speedup=2.074x`, delta overall=`improved` (2 improved / 1 degraded / 9 unchanged).
	- test_real_world_algorithms_x64_O2: `avg_norm=34.62`, `both_success=100.0%`, `high_div=94.964%`, `wall_speedup=1.257x`, `throughput_speedup=2.045x`, delta overall=`degraded` (1 improved / 3 degraded / 8 unchanged).

## 2026-04-08

### Changed
- Hardened Rust-only decompile execution in `fission-cli` by running function rendering on explicitly sized worker stacks (`FISSION_RUST_DECOMP_STACK_MB`, default 32MB), applying the same stack sizing to fan-out workers, and converting spawn/join failures into structured per-function fallback results instead of hard aborts.
- Fixed recursive NIR expression lowering cycle tracking in `fission-pcode` by reusing the existing `visiting` set for call argument lowering rather than creating per-argument fresh sets, preventing recursion blowups on cyclic varnode chains.
- Fixed branch-indirect candidate selection panic in `fission-pcode` terminator lowering by replacing eager indexing logic with a guarded `len()==1` branch.

### Validation
- Full EverPlanet rust-sleigh decompile-all lane completed without crash after the above fixes.

## 2026-04-06

### Changed
- Extended rust-sleigh x86 SIMD follow-up semantic ownership by promoting additional `66 0F` packed ops to intrinsic-backed dataflow (including `PUNPCKLQDQ/HQDQ`, `PSHUFD`, `PADDQ`, `PMULLW`, `PSUB*`, and `PADD*` forms).
- Expanded x86 extended-opcode dispatch coverage so newly promoted SIMD ext bytes are routed to SIMD semantics instead of falling through to policy paths.
- Added additional 3-byte `0F 3A` intrinsic mappings for `BLENDPS` and `BLENDPD` while preserving `AESKEYGENASSIST` coverage.
- Updated x86 semantic regressions to validate new SIMD/3-byte intrinsic outputs and immediate propagation behavior.

### Validation
- `cargo test -p fission-sleigh decode_simd_p1_followup_queue_instructions_emit_intrinsics -- --nocapture`
- `cargo test -p fission-sleigh decode_high_frequency_0f38_0f3a_intrinsics_emit_xmm_dataflow -- --nocapture`
- `cargo test -p fission-sleigh --lib`
- `cargo check -p fission-pcode`
- `cargo check -p fission-automation`

## 2026-04-04

### Added
- Dependency-free direct rust-sleigh instruction lifting path is now active for CLI `--engine rust-sleigh`.
- AArch64 semantic lifting coverage expanded with concrete p-code ops for ALU/memory flows, including move-wide and writeback-addressing forms.
- New `fission-decompiler-core` crate added as a decompiler orchestration boundary, starting with prebuilt p-code -> shared NIR routing entrypoint.
- Added crate-level `crates/fission-sleigh/README.md` documenting ownership, API surface, usage examples, and validation workflow.

### Changed
- Reorganized `fission-sleigh` lifter into architecture-oriented module trees.
- Split AArch64 implementation into facade/semantic/control modules.
- Split x86 implementation into facade/length/control modules.
- Refactored AArch64 semantic decoding into focused submodules (`arithmetic`, `logical`, `memory`, `misc`) and kept `semantic.rs` as a thin dispatcher.
- Split semantic unit tests by submodule file and normalized test names with a `decode_` prefix for consistent per-file coverage.
- rust-sleigh one-shot decode path now reconstructs multi-block p-code CFG (branch/cond-branch targets + fallthrough successors) instead of flattening all ops into one block.
- rust-sleigh render path now disables the PE-x64-only NIR gate for this engine and enables conservative irreducible fallback, allowing non-PE/non-x64 binaries to flow through NIR->C print.
- Integrated rust-sleigh render flow with shared `fission-static` NIR routing/recovery layer via a new prebuilt-pcode routing entrypoint, keeping fallback classification and recovery policy centralized.
- Added prebuilt-pcode routing wrapper tests in `fission-static` (`select_nir_output_from_pcode_*`) to lock engine facade behavior.
- rust-sleigh CLI path now calls the new `fission-decompiler-core` API instead of invoking `fission-static` routing directly.
- Shifted function-level Raw p-code ownership into `fission-sleigh`: lifter now provides a contract API (`lift_raw_pcode_function_with_contract`) that returns block/successor-aware `PcodeFunction` plus stop reason diagnostics.
- CLI rust-sleigh decompile path now consumes Sleigh's function-level lift contract directly instead of rebuilding CFG blocks in `run.rs`.
- Expanded AArch64 control-flow decoder test coverage in `fission-sleigh` with focused checks for `RET`, `B`, `BL`, `B.cond`, `CBZ/CBNZ`, and `TBZ/TBNZ` predicate/target semantics.
- Upgraded x86 `Jcc` lifting to emit flag-based predicates (including composed `CF/ZF` and `SF/OF/ZF` forms) instead of constant-true branches, and added focused x86 control decoder tests for predicate shape/target handling.
- Added x86 semantic lifting for register-form `CMP/TEST` and base ALU/logical group (`ADD/SUB/AND/OR/XOR`), including explicit EFLAGS writes (`CF/OF/ZF/SF/PF`) so branch predicates consume real upstream flag dataflow.
- Added integration coverage proving `CMP`-produced `ZF` feeds `JNE` predicate construction in function-level lift output.
- Expanded x86 semantic coverage for immediate forms (`81/83/F7/A9`) and r/m memory operand forms (load/store-backed arithmetic/compare/test), and widened PF regression sampling to include register, immediate, and memory paths.
- Extended x86 semantic lifting with carry/shift/sign-sensitive op families (`ADC`, `SBB`, `INC`, `DEC`, `NEG`, `SHL`, `SHR`, `SAR`) including flag-write behavior and memory r/m paths.
- Extended x86 shift/address handling with `D3` (`* r/m, CL`) count lowering and `0x67` address-size override effective-address decoding in semantic lift paths.
- Extended x86 Group2 shift coverage with byte-sized forms (`C0`, `D0`, `D2`) using width-correct semantic lowering.
- Refined dynamic x86 shift semantics so runtime `count==0` preserves destination/`CF/ZF/SF/PF` via conditional writeback (instead of unconditional flag/result overwrite).
- Refined `0x67` address-size override semantics to compute effective addresses with 32-bit arithmetic (including disp/index/base composition) and then zero-extend to 64-bit for memory access.
- Extended x86 length decoding for shift-immediate opcode `C1` and added regressions for the new length rule.
- Added x86 length regressions for `D3` and `0x67` forms.
- Added x86 length regressions for byte shift forms (`C0`, `D0`, `D2`).
- Updated lifter ownership/structure documentation to match the new folder tree.

### Validation
- `cargo check -p fission-sleigh`
- `cargo test -p fission-sleigh`
- `cargo check -p fission-static --features native_decomp`
- `cargo test -p fission-static --features native_decomp select_nir_output_from_pcode_`
- `cargo test -p fission-sleigh`
- `cargo test -p fission-sleigh lifter::aarch64::control::tests::`
- `cargo test -p fission-sleigh lifter::x86::control::tests::`
- `cargo test -p fission-sleigh lifter::x86::semantic::tests::`
- `cargo test -p fission-sleigh lifter::tests::x86_cmp_flags_feed_jcc_predicate_path`
- `cargo test -p fission-sleigh lifter::x86::length::tests::`
- `cargo test -p fission-sleigh`
- `cargo check -p fission-cli --features native_decomp`
- `cargo test -p fission-cli --features native_decomp cfg_blocks_`
- `cargo test -p fission-cli --features native_decomp terminal_control_flow_only_stops_on_return_or_indirect_branch`
- `cargo run -p fission-cli --features native_decomp -- samples/hello --decomp 0x000100000460 --engine rust-sleigh --no-header`
- `fission_cli samples/hello --decomp 0x000100000460 --engine rust-sleigh --no-header`

## 2026-04-03

### Added
- Sleigh converter statement-level UserCall lowering to CALLOTHER.
- New Sleigh converter modules for export and user-call handling.
- Additional Sleigh language/spec assets for converter and lifter validation.
- Utility inventory reader script for benchmark support.

### Changed
- Extended LocalGoto relative branch handling for broader signed delta resolution.
- Improved NIR relative branch target resolution and related CFG behavior.
- Updated NIR structuring and normalization paths, including guarded-tail and linearization work.
- Updated CLI one-shot decompile path and rendering/common argument handling.
- Updated automation reporting and native decompiler integration plumbing.

### Fixed
- Restored multiple pcode/NIR tests after structural model drift in basic block fields.
- Improved converter expression/assignment handling robustness and edge-case behavior.
- Synced decompiler extraction paths and headers for native bridge changes.

### Validation
- Verified Sleigh converter crate tests are passing.
- Verified pcode and CLI build checks pass with current integration state.
