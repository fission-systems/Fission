# Share selected immutable SLEIGH constructors

Mechanical performance validation, 2026-10-16. **Kuna-level speed is not achieved.** No semantic/readability, metric, ranking or guest-execution claim.

## Source and invariant

Baseline main `2091f9038f9337260174bb7b619e67759b956d35`, immutable release SHA256 `6f39d552d6dee077a45272ef89cc9821091eb4b327ca1d984b97472a9d080762`. Candidate release SHA256 `cad83688200d2aeca0f5b5b559d4df5ebf0b1c2a3cf218e7c83aa14b3fce20a4`. [Compact evidence](constructor-ownership-performance.json) includes exact source tree/patch/file hashes, inputs, timing ranges and output checks. All nine source files match the frozen measured snapshots.

`CompiledParserWalker::walk` copied seven immutable constructor metadata fields into every runtime state and allocated source/mnemonic strings for constructor diagnostic frames. Existing native profiles repeatedly show binding and state/handle allocation/destruction; bounded native probing counted 22,937 states across 5,576 linear root instructions (not full-function execution or allocator/live-byte accounting). These observations motivate ownership work; sample counts are not phase wall-time shares.

Existing compiled subtables now own executable constructors through Arc. Selection borrows the exact canonical Arc, returned states retain one clone, and constructor diagnostic frames retain the same metadata instead of two string allocations. Instruction-specific handles, context values, lengths, selection identity, operands, template execution and error ordering are unchanged. Context-change descriptions are shared; context changes still execute per instruction. No new production dependency, program-fact map, global cache, worker/timeout/instruction-limit change, CLI schema or DecBench adapter change.

### Rust construction API

`CompiledSubtableDefinition.constructors` changes to `Vec<Arc<CompiledExecutableConstructor>>`. A `RuntimeConstructState` supplies a complete `constructor: Arc<CompiledExecutableConstructor>` instead of its former seven metadata fields. Read-only metadata field access is available through Deref; mutation requires explicit `Arc::make_mut(&mut state.constructor)`, with another make_mut for nested Arc fields. No DerefMut permits accidental mutation of another bound state. Callers building states/subtables need source migration; this is not full Rust source compatibility. The logical Debug field order/content is preserved.

Serde JSON and bincode values/bytes are unchanged against the legacy owned-constructor representation in both directions for all x86-64 subtables. The compiled frontend cache uses bincode. Current rkyv roundtrip and shared reference preservation are tested; compatibility with old raw rkyv archive layout is not claimed (no persisted CompiledFrontend-rkyv consumer was found).

## Controls

[Frozen qualification corpus](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489), 250 stripped functions / 45 cells (Linux 108, Windows 97, macOS 45). Reverified all 45 stripped input hashes; sample SHA256 `f432c528e3071efa60fdaecc132aafa8c351e386bf7203d252e2875d6de02620`. Sources/debug oracles are not supplied to the producers.

Native M4 Pro, 14 logical CPUs, 24 GiB; release LTO, codegen-units=1. Fresh CLI processes, same requested addresses, resources and cwd, `--no-db`, `DECBENCH_NO_CACHE=1`, 30-second function limit. Internal runtime/resource caches remain enabled; no OS cache flush, desktop scheduling uncontrolled. Other observed build/test work finished before timing. No own build/test/profiler overlapped timing. The fresh full replay uses two concurrent processes only for output validation, not speed claims.

## Large-input baseline/candidate

Six functions in one large Windows input. Three alternating pairs, default render/preparation workers. Predeclared first gate: >=2% separate-median latency reduction.

| Measurement | Baseline | Candidate | Change |
|---|---:|---:|---:|
| Wall seconds | 12.261268 | 11.990471 | -2.21% |
| CPU seconds | 107.449972 | 100.362175 | -6.60% |
| Peak RSS bytes | 2,428,911,616 | 2,396,962,816 | -1.32% |

An independent five-pair confirmation starts candidate first (inverted initial order), giving eight total pairs with four starts/tool. Predeclared confirmation gate: >=2% median paired latency reduction and no >2% median paired CPU regression.

| Confirmation | Baseline median | Candidate median | Median paired change |
|---|---:|---:|---:|
| Wall seconds | 12.311457 | 11.741746 | -5.03% |
| CPU seconds | 108.482351 | 99.322057 | -7.21% |
| Peak RSS bytes | 2,427,551,744 | 2,402,271,232 | +0.27% |

Confirmation separate-median wall change is -4.63%, CPU -8.44%. Full individual results/ranges are retained. Confirmation wall ranges: baseline [12.262113125063479, 13.758468084037304], candidate [11.692337499931455, 12.183332583867013]. Do not interchange median paired ratios with ratios of separate medians. **No meaningful memory reduction is claimed.**

Small inputs: five alternating pairs, requested render worker=1 (preparation/FID retain normal parallelism):

| Cell | Baseline seconds | Candidate seconds | Change |
|---|---:|---:|---:|
| 03 | 1.981661 | 1.970254 | -0.58% |
| 27 | 0.847778 | 0.852653 | +0.57% |
| 40 | 0.950200 | 0.955585 | +0.57% |

Two small inputs are slightly slower; all differences are within about 0.6%. No meaningful small-input speed gain is claimed.

## Fixed Kuna 1.808 comparison

Four fixed batches / 22 addresses, three alternating runs per tool/batch/setting; requested jobs/render workers 1 and 14 (14 lane also fixes Rayon=14). Each tool chooses its own faster observed median separately; Fission render=1 does not serialize FID/preparation.

| Cell | Fission 1 | Kuna 1 | Fission 14 | Kuna 14 | Faster-setting ratio |
|---|---:|---:|---:|---:|---:|
| 03 | 1.9766 | 0.1966 | 1.3574 | 0.3360 | 6.90x |
| 22 | 13.9113 | 2.8445 | 12.3100 | 5.5547 | 4.33x |
| 27 | 0.8711 | 0.1137 | 0.6768 | 0.2462 | 5.95x |
| 40 | 1.0121 | 0.3481 | 0.9512 | 0.6929 | 2.73x |

Fission remains **2.73–6.90x slower**. These controls do not establish equal quality, latest Kuna, whole-binary throughput or a cross-version ranking. Previous ranges came from separate host runs and must not be treated as paired Fission before/after changes. Fixed Kuna executable SHA256 `46cd0f8109557f8fe1e8bae53e191bd6b92a7d0410ee2b279a37621c5a2c3e05`.

## Validation and bounds

- **13 targeted nextest tests pass**: exact metadata pointer identity and lifetime after frontend removal, clone/explicit copy-on-write isolation, Send+Sync, legacy serialization and current archive roundtrip, complete bind/lift/error and FID controls.
- **2,209 related nextest tests pass, 9 skipped** across sleigh/signatures/static/pcode/decompiler/CLI/emulator. Nextest reports one leaky pass; no competing build/test/decompiler process remained before timing. Crate checks, CI-scoped Clippy, release CLI, workspace fmt and owner scan pass. Unrelated existing example warnings remain.
- The 1,024-record four-ISA matrix preserves complete logical raw-state Debug, full successful lift tuples and complete errors: 746 successes, 278 errors, 27 parser traces; 41,939,354 uncompressed bytes, SHA256 `5721b8f3f02547208b90aec3889566b5bfb33dab046fcc95a0509c0890bba9b7`. Host backtraces are excluded from error comparison; tests ran with RUST_BACKTRACE=1.
- FID matrix: 512 inputs, 383 eight-instruction extents, 473 successful hashes/two resolver policies; hashes and resolver callback order identical, SHA256 `a771dbb8a6b4ef303ab310b1e62ca5f5a7d8a596cf45ef7fe53a51ed618bc6fe`. Existing context-commit, invalid display/commit/length, delay-slot and FID vector controls pass. No dedicated new crossbuild fixture is claimed.
- Fresh **250/250 complete function records unchanged**, excluding recursively only seven durations: `build_duration_ms`, `render_duration_ms`, `structuring_duration_ms`, `normalize_duration_ms`, `decomp_sec`, `postprocess_sec`, `total_time_ms`. Every other existing code/type/variable/PC/map/error/fallback/evidence/counter field is compared. This does not add line mapping support from another draft.
- Focused sets repeat: large 8 pairs; small 5 pairs each; all 24 Fission worker-lane runs match the fresh full replay. Both tools return exact requested address sets and error-free nonempty code; Kuna code/variable/map digests repeat. The full 250 population was regenerated once; no complete-function execution, recompilation, external semantic benchmark or score rerun.
- Five earlier prototypes (state vector pool, display streaming, global space-name sharing, local fixed-space sharing, derived operands) were rejected for no useful/reproducible latency gain or regressions. Their code is absent; patches/binaries/rejections remain archived.

## Why the larger Kuna gap remains

Fission's FID preparation still iterates binary functions and builds raw lifted CFGs beyond a small requested address set. Small-process profiles also expose repeated IR/type fixed-point and use-count traversals. Its recursive owned binding tree still allocates dynamic vectors and boxed descendants. This patch removes immutable metadata copying, not these larger costs.

The inspected local Kuna source at `b637463a36b07c6a031804ec0ae0cf3af09c794f`, `decompiler/crates/kuna-sleigh/src/sleigh.rs`, uses indexed constructor-state arenas and recycles parser contexts. This is structural reference evidence; it is **not proven to be the source of the measured v1.808 executable**. Kuna also commits image-wide discovery analysis; do not describe it as doing only requested-function work. No matched phase timing or Kuna CPU profile apportions the total observed gap yet. Next owner investigations are shared/demand-aware analysis preparation with dependency preservation, instruction-owned checked reusable state storage, and repeated IR traversals. Do not skip facts or diagnostics merely to match latency.

Full artifacts: `benchmark/artifacts/performance/2026-10-16-constructor-ownership/`: immutable binaries, frozen sources/patch/tree, pre-implementation proposal, all native commands/JSON/stderr, driver scripts, validation logs and input hashes. Reproduce local controls with `controlled.sh` after the listed build/checks; it freezes the release binary then gates large controls/confirmation before small controls/replay/Kuna. Source paths in the experimental drivers are local; production resource resolution remains unchanged.
