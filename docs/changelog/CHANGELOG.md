# Changelog

All notable changes to the Fission project (November 2025 – Present).

This file is the public-facing English changelog.  
The previous detailed Korean historical notes are preserved in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md).

---

## 2026-03-20

### P5G - Focused PDB Function-Facts Ingestion

This round moved PDB handling from “source presence is visible” into real function-level fact ingestion for the Fission NIR pipeline.

Instead of building a full PDB parser, the loader now performs a narrow sidecar-driven ingest for function-scoped facts that directly affect decompilation quality:

- function names
- return types
- parameter names
- parameter types

These facts now flow into the existing Rust facts pipeline rather than staying trapped as loader metadata.

#### Added

- focused PDB sidecar ingestion in the loader
  - PE CodeView / RSDS / NB10 metadata is now used to locate and open matching `.pdb` sidecars
  - module symbol streams are scanned narrowly for function-scoped facts instead of attempting broad PDB database coverage
- function-level PDB facts in `FactStore`
  - `FactProvenance::PdbMetadata`
  - `FunctionFacts.pdb_info`
  - `FactStore::preferred_debug_function(...)` now falls back from DWARF to PDB-backed function info
- inventory explicit surfacing for PDB-derived facts
  - `explicit_fact_breakdown.pdb_type_count`
  - `explicit_breakdown_totals.pdb_type_count`
  - inventory row names now prefer the chosen resolved fact name when available

#### Changed

- preview / postprocess debug fact consumption
  - preview function hints can now use PDB-backed function info when DWARF is absent
  - Rust-side postprocess also consumes preferred debug function info instead of assuming DWARF-only availability
- diagnosis quality after PDB source detection
  - the pipeline can now distinguish:
    - `PDB source present and actually surfaced`
    - `PDB source present but still not surfaced`
    - `native inferred facts are still filling the gap`

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-loader loads_focused_pdb_function_facts_from_repo_sample -- --nocapture`
- inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `test-pdb.exe`
  - `fauxware.exe`

#### Observed Effect

- `test-pdb.exe`
  - `source_presence_counts.pdb = 6`
  - `provenance_surface_totals.pdb_nonzero_rows = 5`
  - `strict_explicit_candidate_count = 4`
- `fauxware.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 16`
  - `strict_explicit_candidate_count = 6`
- `has_pdb.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 7`

This means the repository now has both sides of the diagnostic split:

- samples where PDB-derived function facts genuinely surface into inventory rows,
- and samples where PDB source presence is truthful but surfaced explicit rows are still being supplied by native inferred facts.

## 2026-03-19

### P5F2 - Preview-Stage Block Split And First Narrow Unblock

This round moved preview-side diagnosis from “generic unknown failure cleanup” into the first real unblock patch for the Fission NIR path.

The work happened in two steps:

- first, preview-stage failures were split so that pcode/frontend acquisition failures stopped polluting the real preview block bucket,
- then a single recoverable `unsupported_indirect_branch_target` shape was patched without broadening indirect control-flow support.

#### Added

- preview block signature reporting in inventory-backed rows
  - rows now carry:
    - `preview_block_signature`
    - `preview_block_detail`
- finer preview-stage diagnosis buckets
  - `preview_frontend_reject` is now separated from genuine preview CFG failures
  - diagnosis summaries can aggregate preview block signatures directly
- narrow instruction-local relative branch target support in the Fission NIR pcode path
  - recoverable constant-space pcode branch targets are now resolved by exact target block index
  - duplicate-start blocks can now be distinguished through synthetic target keys / labels instead of collapsing into one canonical start address

#### Changed

- preview inventory / diagnosis interpretation
  - `native_pcode_failure`-like cases that previously looked like preview unknowns are now surfaced as frontend rejection rather than preview-stage block
- preview control-flow lowering
  - branch and cbranch lowering now use resolved target block indices for the supported instruction-local relative-target shape
- structuring path label/target handling
  - duplicate-start block targets are preserved narrowly enough to support the recovered branch shape without enabling broad indirect branch handling

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-pcode preview_supports_instruction_local_conditional_branch_targets -- --nocapture`
- `cargo test -p fission-pcode preview_supports_instruction_local_unconditional_branch_targets -- --nocapture`
- inventory smoke reruns:
  - `GetProcAddress.exe --functions-limit 20`
  - `putty.exe --functions-limit 10`

#### Observed Effect

- `GetProcAddress.exe`
  - before:
    - `direct_success_count = 16`
    - `preview_frontend_reject = 3`
    - `preview_unsupported_cfg = 1`
    - dominant preview-side signature: `unsupported_indirect_branch_target`
  - after:
    - `direct_success_count = 17`
    - `preview_failure_count = 3`
    - remaining failures are all `preview_frontend_reject`
    - the representative blocked row at `0x140001190` now becomes `preview_direct_success = true`
- `putty.exe`
  - 10-function smoke rerun stayed stable with:
    - `direct_success_count = 10`
    - `preview_failure_count = 0`

This means the first real preview-side unblock is now in place: one recoverable `unsupported_indirect_branch_target` class has moved onto the success path without widening support to general indirect branch control flow.

### P5F1 - Provenance Completeness For Function Facts Inventory

This round refined the inventory from “provenance-aware” toward “provenance-complete enough to guide the next core patch.”

The main improvement is that inventory output can now distinguish between:

- sources that carry PDB-style debug provenance,
- function rows that actually surface explicit facts,
- and cases where surfaced explicit rows are still being supplied by native inferred facts rather than by PDB-derived facts.

#### Added

- provenance fact breakdown in function inventory rows
  - rows now include `provenance_fact_breakdown` with:
    - `dwarf_type_count`
    - `pdb_type_count`
    - `native_type_count`
    - `loader_type_count`
- provenance surface totals in inventory summaries
  - summaries now report:
    - `dwarf_nonzero_rows`
    - `pdb_nonzero_rows`
    - `native_nonzero_rows`
    - `loader_nonzero_rows`
- function snapshot provenance helpers
  - `FunctionFacts` now exposes:
    - `dwarf_type_fact_count()`
    - `pdb_type_fact_count()`
    - `native_type_fact_count()`
    - `loader_type_fact_count()`

#### Changed

- PDB source presence detection
  - `fact_sources_present.pdb` is no longer a placeholder
  - inventory now treats `.pdb` sidecars and embedded PE `RSDS` / `.pdb` markers as real PDB source presence signals
- diagnosis interpretation
  - inventory-guided diagnosis can now distinguish:
    - `pdb source present but no pdb-surfaced explicit rows`
    - `native inferred facts are currently covering the explicit surface gap`

#### Validation

- `cargo test -p fission-static snapshot_counts_dwarf_type_facts_from_function_info -- --nocapture`
- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- smoke inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `source_presence_counts.pdb = 10`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 5`
  - diagnosis now shows that PDB provenance is present, but surfaced explicit rows are still coming from native inferred facts

This means the next preview-side or facts-side patch can target real remaining gaps without provenance confusion.

### P5D / P5E - Inventory-Guided Diagnosis And Function-Level Facts Surfacing

This round stopped treating explicit-facts scarcity as a vague benchmark problem and turned it into a concrete inventory diagnosis plus a core data-path patch.

The key result is that aligned sources no longer have to stay stuck in a blanket `inventory_surface_gap` bucket. Inventory-backed diagnosis identified where provenance existed but explicit rows still stayed at zero, and the inventory export now promotes function-level native inferred facts into the explicit surface instead of leaving them hidden behind generic provenance flags.

#### Added

- inventory-guided diagnosis runner
  - added `scripts/test/batch_benchmark/diagnose_function_inventory.py`
  - classifies aligned binaries into:
    - `source_facts_absent`
    - `factstore_or_inventory_surface_gap`
    - `preview_stage_block`
    - `mixed_or_inconclusive`
  - emits a per-binary diagnosis plus a recommended next patch direction
- function snapshot helpers for type-fact provenance
  - `FunctionFacts` now exposes separate counts for:
    - native type facts
    - loader type facts

#### Changed

- function inventory explicit surfacing
  - inventory export now ingests function-level native inferred types during whole-binary row generation
  - `explicit_fact_breakdown` now includes `native_type_count`
  - `explicit_fact_total` now counts surfaced native function facts in addition to DWARF param/local/return facts
- inventory surface-gap interpretation
  - `inventory_surface_gap` is no longer triggered by image-wide loader metadata alone
  - the gap signal now focuses on per-function/debug provenance that should realistically surface as explicit facts
- strict explicit candidate detection in inventory
  - strict candidate evaluation now uses the surfaced inventory explicit total rather than only the DWARF-only count

#### Validation

- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- inventory smoke reruns:
  - `has_pdb.exe`
  - `putty.exe`
- inventory-guided diagnosis rerun:
  - `GetProcAddress.exe`
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `explicit_fact_nonzero_count`: `0 -> 5`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`
- `putty.exe`
  - `explicit_fact_nonzero_count`: `0 -> 7`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`

This moves the project past “why are explicit facts missing?” into a narrower question: which remaining aligned binaries are still blocked by preview-stage issues, and which ones still need more supply-path surfacing.

### P5A / P5B / P5C - Function Facts Inventory, Inventory-Backed Corpus Selection, And Provenance-Aware Analysis

This round changed the benchmark/corpus workflow from probe-first scanning to inventory-first filtering.

The key architectural shift is that benchmark scripts no longer need to treat address-targeted preview scans as the canonical source of truth. Instead, the CLI can now export whole-binary function facts as a structured inventory, and corpus generation can filter that inventory into strict explicit, heuristic, aligned, and blocked views.

#### Added

- whole-binary function facts inventory export
  - added hidden CLI mode `--emit-function-facts-inventory`
  - emits row-level JSONL plus summary JSON from a single binary load / decompiler preparation pass
- inventory row metadata for corpus selection
  - rows now carry function-level facts, preview admission results, pcode size, and structured row failure fields in one place
- Python inventory reader helper
  - added `scripts/test/batch_benchmark/grand_finale_support/inventory_reader.py`
  - centralizes:
    - running the Rust inventory export
    - loading inventory JSONL rows
    - loading summary JSON
- provenance-aware inventory fields
  - inventory rows now include:
    - `fact_sources_present`
    - `explicit_fact_breakdown`
    - `admission_block_stage`
    - `inventory_surface_gap`
  - summary output now includes:
    - `source_presence_counts`
    - `explicit_breakdown_totals`
    - `inventory_surface_gap_count`
    - `aligned_with_zero_explicit_count`

#### Changed

- benchmark/corpus scripts now consume inventory rows
  - `refine_preview_quality_corpus.py` now builds corpus outputs from function facts inventory rows instead of address-probe scan results
  - `grand_finale_support/corpus_candidates.py` now treats the Rust inventory export as the default candidate source
- provenance-aware blocked/aligned interpretation
  - blocked and aligned candidate reports now carry provenance fields through from the inventory rows
  - corpus refinement now emits aggregated inventory provenance counters alongside blocked explicit summaries
- corpus outputs derived from the same canonical source
  - `preview_quality_corpus.json`
  - `preview_explicit_blocked_candidates.json`
  - `preview_explicit_aligned_candidate_report.json`
  are now designed to be generated from the same inventory-backed function row source

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- function facts inventory smoke
  - `putty.exe --emit-function-facts-inventory --functions-limit 3`
  - verified row JSONL and summary JSON emission
- inventory-backed corpus smoke
  - `refine_preview_quality_corpus.py` against `GetProcAddress.exe`
  - verified generation of:
    - candidates JSON
    - aligned candidate report
    - blocked explicit report
    - curated corpus JSON
- provenance-aware inventory smoke
  - `GetProcAddress.exe --emit-function-facts-inventory --functions-limit 5`
  - verified:
    - row-level provenance fields
    - summary-level provenance counters
    - blocked report inventory summary totals

#### Current State

- address-targeted scans remain useful, but they are now probe/debug tooling rather than the preferred canonical data source
- strict explicit / heuristic / blocked / aligned analysis can now be driven from one whole-binary inventory export
- inventory rows now expose whether explicit-fact scarcity appears to come from missing source facts, inventory surface gaps, or preview-stage rejection

## 2026-03-18

### P4.8 / P4.8.2 - Explicit-Facts PE Source Expansion

This round focused on finding PE samples that can actually exercise the new explicit preview hint paths without weakening the meaning of the strict explicit corpus.

The main result was diagnostic rather than cosmetic:

- the strict `quality_explicit_facts` corpus remains intentionally empty,
- blocked explicit candidates are now tracked separately,
- and the remaining bottleneck is clearly sample scarcity plus lack of direct-preview overlap, not corpus/refinement logic.

#### Added

- explicit source inventory metadata
  - expanded the PE candidate pool with LLVM, `samples/other`, and other debug-info-rich Windows binaries
  - recorded per-source metadata including:
    - `toolchain`
    - `debug_info_kind`
    - `has_loader_types`
    - `priority`
    - `notes`
- blocked explicit candidate tracking
  - added a dedicated blocked-candidate artifact instead of weakening the strict explicit corpus

#### Changed

- explicit corpus discipline
  - kept `quality_explicit_facts` strict rather than filling it with provisional fallback seeds
  - continued to require:
    - `explicit_fact_total >= 2`
    - `preview_direct_success == true`
    - `has_indirect_control_flow == false`
    - `pcode_op_count <= 800`
- blocked-candidate reporting
  - normalized blocked explicit candidates under the current taxonomy
  - preserved raw fallback information where the engine still reports only coarse `preview_unsupported` results
  - added summary counts for:
    - blocked-reason distribution
    - newly scanned zero-explicit sources
    - newly scanned timeout sources

#### Current State

- strict explicit corpus: still empty by design
- blocked explicit candidates:
  - `main-debug.exe`
  - `addr.exe`
- dominant blocked reason:
  - `preview_non_success_unknown`

This means the benchmark/reporting pipeline is no longer the limiting factor. The next step is better fact-rich PE source acquisition, not provisional promotion of blocked seeds.

### v104 - 3-Way Benchmark Expansion (`pyghidra` vs `legacy` vs `preview`)

This round expanded the public benchmarking story from two separate comparisons into a consistent 3-way model:

- `pyghidra` as the Python-host baseline,
- `legacy` as the native FFI / Ghidra core baseline,
- `preview` as the Rust-owned decompiler pipeline.

The main goal was not a single blended score, but a benchmark shape that shows where overhead, fallback behavior, and readability improvements come from.

#### Added

- shared resource monitor helper for benchmark scripts
  - added `scripts/test/batch_benchmark/grand_finale_support/resource_monitor.py`
  - reused the same optional `psutil`-based RSS / CPU sampling model in both benchmark modes
- function-level 3-way artifact shape
  - `compare_legacy_preview.py` now emits `pyghidra`, `legacy`, and `preview` together
  - added `three_way_delta` and `winner_summary` per function
- whole-binary 3-way raw outputs
  - now writes `legacy_full.json`, `preview_full.json`, and `ghidra_full.json`

#### Changed

- fixed-seed function-level comparison
  - promoted `compare_legacy_preview.py` into the main 3-way fixed-seed comparison path
  - kept existing `legacy` / `preview` fields for backward compatibility
  - added engine-level summaries and pairwise deltas:
    - `pyghidra_vs_legacy`
    - `legacy_vs_preview`
    - `pyghidra_vs_preview`
- timing and resource metrics
  - added shared timing stats with `p95_ms`
  - added best-effort per-run resource summaries:
    - `max_rss_mb`
    - `avg_rss_mb`
    - `avg_cpu_pct`
    - `max_cpu_pct`
- whole-binary benchmark summary
  - replaced the old 2-way summary with explicit engine buckets:
    - `pyghidra`
    - `legacy`
    - `preview`
  - added pairwise quality/similarity sections and a public-ready summary line
- benchmark documentation
  - updated `scripts/test/batch_benchmark/README.md` to describe both benchmark modes and the 3-way engine model

#### Validation

- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/compare_legacy_preview.py`
  - `scripts/test/batch_benchmark/full_decomp_benchmark.py`
  - `scripts/test/batch_benchmark/grand_finale_support/*.py`
- `cargo build -p fission-cli --features native_decomp`
- function-level 3-way smoke
  - `test_control_flow_x64_O0.exe 0x140001010`
  - artifact:
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.json`
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.md`
- whole-binary 3-way smoke
  - `test_control_flow_x64_O0.exe --limit 1`
  - artifact:
    - `/tmp/v104_full_smoke2/benchmark_summary.json`
    - `/tmp/v104_full_smoke2/benchmark_summary.md`

## 2026-03-17

### Repository Licensing + CLA Setup

The public repository license was fixed to AGPL-3.0, and a Contributor License Agreement was added to support a future open-core operating model. The intent is to keep the core engine open under AGPL-3.0 while preserving a clean legal boundary for accepting outside contributions.

#### Added

- root license file
  - added the full GNU AGPL-3.0 text to `LICENSE`
- Contributor License Agreement
  - added `CLA.md`
- GitHub pull request template
  - added a PR template with an explicit CLA acknowledgement checkbox

#### Changed

- README public metadata
  - declared the repository license as AGPL-3.0
  - added a CLA reference
- Rust package metadata
  - added `license = "AGPL-3.0-or-later"` across public workspace `Cargo.toml` files
- CONTRIBUTING guide
  - documented the CLA requirement
  - fixed the source-header policy around repository-level licensing plus optional SPDX short headers

### Private AI Layer Repository Boundary Cleanup

The repository boundary was tightened by removing `fission-ai` from the public workspace and Git tracking. The goal was to keep the core decompiler and analysis engine open while keeping future AI product/API orchestration layers outside the public repository scope.

#### Changed

- public workspace scope
  - removed `crates/fission-ai` from the workspace members
  - removed the `fission-ai` dependency and re-export from `fission-analysis`
- public Git tracking scope
  - added `crates/fission-ai/` to `.gitignore`
  - removed `crates/fission-ai/*` from Git tracking so it would no longer be published on GitHub

#### Validation

- `cargo build -p fission-analysis --features native_decomp`

### v75-v78 - Preview-First Retirement Prep + Type Absorption Expansion + ARM64 Detection Scaffolding

This span focused on three themes:

1. making preview-first the real product policy while shrinking `legacy` toward compat/fallback only,
2. expanding Rust-side type absorption for hard x64 and x86 cases,
3. laying the first PE/Windows ARM64 detection groundwork and widening cross-image propagation to `ida76sp1/plugins`.

#### Added

- legacy-needed benchmark/report artifacts
  - separate binary/global summaries for successful functions that still are not preview-direct
- x86 decimal index field-replacement regression coverage
  - validates decimal surfaces such as `register[24]` as field-replacement candidates
- cross-image propagation scope coverage for `plugins/`
  - smoke validation that `ida76sp1/plugins/hexrays.dll` is actually included
- Windows ARM64 spike note
  - recorded current blockers and bring-up checklist in `docs/benchmark/windows_arm64_spike.md`
- synthetic PE ARM64 loader test
  - validated `IMAGE_FILE_MACHINE_ARM64 -> AARCH64:LE:64:v8A`

#### Changed

- preview-first retirement prep
  - removed `legacy` from normal GUI workflow
  - kept CLI `--engine legacy` as a hidden compatibility mode
  - fixed fallback taxonomy around `preview_timeout`, `preview_unsupported`, `native_pcode_failure`, `legacy_fallback`, and `assembly_fallback`
- x64/x86 shared type absorption
  - kept metadata-first inferred-type merge
  - extended line-local pointer-offset alias substitution
  - widened `register[offset]` field replacement candidates to decimal as well as hex surfaces
- x86 hard-case surfacing
  - prevented decimal and stack-like index surfaces from dropping out of common postprocess on cases such as `WinMergeU.exe 0x407050` and `EverPlanet_KR.exe 0xa918d0`
- cross-image propagation phase 2, step 1
  - expanded sibling scanning to include DLLs under `plugins/`
  - widened weak-name detection to include `sub_`, `FUN_`, `func_`, `Ordinal_`, `j_`, `thunk_`, `nullsub_`, `loc_`, and `LAB_`
- Windows PE loader / CLI architecture surfacing
  - recognized PE ARM64 as `AARCH64:LE:64:v8A`
  - surfaced ARM64 as `arm64` / `ARM64 (64-bit)` instead of `x86_64`

#### Improved

- `putty.exe 0x140006380`
  - reduced leftover `unique0x... = register + offset` alias residue
  - increased `register[offset]` surfacing
- x86 hard-case observability
  - hard-case summaries now expose `unique_surface_count`, `field_access_count`, and `offset_index_count`
- legacy deprecation observability
  - reports now show which functions still depend on legacy/native fallback outcomes
- `ida76sp1`
  - propagation scope now includes `plugins/hexrays.dll`, making sibling-based auto rename practical across the plugin layout

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-static --features native_decomp field_offset_replacement -- --nocapture`
- `cargo test -p fission-loader test_parse_synthetic_pe -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-tauri`
- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/grand_finale_support/metrics.py`
  - `scripts/test/batch_benchmark/grand_finale_support/summary.py`
  - `scripts/test/batch_benchmark/grand_finale_support/report_md.py`

#### Notes

- On `EverPlanet_KR.exe 0xa918d0` and `WinMergeU.exe 0x407050`, `unique0x` residue was already near zero in legacy output; the real goal in this round was improving x86 `[]` / field-style surfacing.
- The Windows ARM64 spike is still only a bring-up track. There is no real Windows ARM64 PE sample in the repository yet, so fixed-seed baseline JSON/Markdown artifacts were deferred.

### v69-v74 - x64 Timeout Closure + Portable Multi-DLL Symbol Propagation

This span closed two major threads:

1. reducing the last branch/readability residue in giant x86/x64 functions while turning long-running preview cases into explicit fallback outcomes through subprocess isolation,
2. introducing the first cross-image symbol propagation pass for portable multi-DLL layouts using only sibling EXE/DLL import-export-thunk relationships.

#### Added

- stronger x86 branch-condition recovery
  - reconstructs exact `TEST` / `CMP` boolean trees directly in terminator lowering
- preview render subprocess worker
  - runs heavy preview rendering in a separate worker process
  - kills and falls back explicitly on timeout
- `ida76sp1` fixed-seed watchlist artifacts
  - `ida64.exe`
  - `idat64.exe`
  - `ida64.dll`
  - `ida.dll`
  - `plugins/hexrays.dll`
- Tauri cross-image propagation service
  - same-folder sibling `*.exe` / `*.dll` scan
  - import/export/thunk-based rename candidate resolution
  - in-memory rename provenance tracking

#### Changed

- non-float scalar self-equality / boolean simplification
  - `x == x -> true`
  - `x != x -> false`
  - removed residual expressions such as `if (!reg && reg == reg)`
- stronger dead flag-intrinsic cleanup
  - removes unused `__carry/__scarry/__sborrow` assignments
- converted two `ida76sp1` watchlist timeouts to explicit subprocess-isolated `preview_timeout` fallback
  - `ida64.dll 0x101fa177`
  - `hexrays.dll 0x17088330`
- fixed `hexrays.dll 0x170057f0` to end in a non-empty assembly fallback instead of ambiguous empty preview output
- after `open_file`, scans the current binary parent folder and merges sibling import/export/thunk-based rename candidates directly into `renamed_functions`
- ensured manual/project-loaded renames always outrank auto-propagated renames

#### Improved

- `EverPlanet_KR.exe 0xa918d0`
  - removed `if (!reg && reg == reg)` and `reg == reg` residue
  - reduced code length further
- `ida76sp1` baseline closure
  - `ida64.exe`: direct preview `4/5`
  - `idat64.exe`: direct preview `4/5`
  - `ida64.dll`: direct preview `4/5`, timeout case converted to explicit fallback
  - `ida.dll`: direct preview `4/5`
  - `hexrays.dll`: direct preview `3/5`, remaining cases explicit legacy/assembly fallback
- `ida64.dll 0x101fa177` and `hexrays.dll 0x17088330` no longer remain as 20-second hangs
- sibling scan produced non-zero propagated renames on real `ida76sp1/ida64.dll` smoke runs
- existing regression targets held
  - `putty.exe 0x140006260`: `LPRECT param_2`, `RECT local_3c`, `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: direct preview retained
  - `WinMergeU.exe` x86 and `EverPlanet_KR.exe` x86 direct preview retained

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-static --features native_decomp preview_worker_ -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --bin fission_preview_worker --features native_decomp`
- `cargo build -p fission-tauri`
- compare/watchlist reruns across `ida76sp1` watchlist binaries and retained regression samples

### v63-v68 - C++ Corpus Expansion + x86 Preview Readability Uplift

This span expanded the real-world validation set and then used the new coverage to fix x86-specific preview bottlenecks and readability problems.

#### Added

- new Windows sample corpus coverage
  - `WinMergeU.exe` x64 / x86
  - `SumatraPDF-3.5.2-32.exe`
  - `cmake.exe`
  - `EverPlanet_KR.exe`
- x86 `CallInd` trap-like target recovery
  - surfaces `INT3` producers as opaque callees like `((code *)swi(3))`
- additional x86 readability tests
  - register naming bootstrap
  - large-body cheap slot surfacing
  - dead local / dead flag-intrinsic cleanup
- EverPlanet x86 fixed-seed stress corpus

#### Changed

- added budgeted fallback to x86 `try_lower_while()`
- restored real x86 register names (`eax`, `ecx`, `edx`, etc.)
- allowed cheap slot surfacing to continue in large HIR bodies
- removed write-only non-temp local clobber
- added x86 flag-temp canonicalization and stronger dead intrinsic cleanup

#### Improved

- `SumatraPDF-3.5.2-32.exe`: all 5 fixed seeds `mlil_preview`, fallback 0
- `WinMergeU.exe` x86: all 5 fixed seeds `mlil_preview`, fallback 0
- `EverPlanet_KR.exe`: all 5 fixed seeds `mlil_preview`, fallback 0, while legacy timed out on the selected seeds
- major readability improvement on `EverPlanet_KR.exe 0xa918d0`
  - residue score `207 -> 169 -> 11`
  - temp surface count `182 -> 144 -> 11`
  - code length `18435 -> 15459 -> 9462`
  - `__carry/__scarry/__sborrow` `68/68/19 -> 33/68/18 -> 0/0/0`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- reran compare/fixed-seed coverage for `SumatraPDF`, `WinMerge`, `EverPlanet`, `putty`, and `everything`

### v62 - Warning Cleanup + Fixed-Seed Benchmark Closure

This round removed the last dead warnings after the second major `nir` refactor and re-closed fixed-seed compare results for `putty`, `everything`, `notepad++`, and `7zr`.

#### Changed

- removed two dead warnings
  - `MlilPreviewOptions::is_pe_x64()`
  - unused `VN_SIZE` inside `PcodeFunction::to_flat_bytes()`

#### Improved

- `cargo test` / `cargo build --release` passed cleanly without additional warnings
- reconfirmed fixed-seed compare closure
  - `putty.exe 0x140006260`: `mlil_preview`, fallback 0, preserved `LPRECT param_2` / `RECT local_3c` / `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: `mlil_preview`, fallback 0
  - `7zr.exe` selected seeds: all `mlil_preview`, fallback 0
  - `notepad++.exe` selected seeds: all `mlil_preview`, fallback 0

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v59-v61 - x86 Conditional Structuring Stabilization + Second `nir` Refactor

This span stabilized long-running x86 `try_lower_if()` paths on heavy `7zr.exe` seeds and then reorganized the growing `nir` implementation into a more maintainable module tree.

#### Added

- x86-only conditional structuring budget/cache
- join/follow-gated plain `if` candidate pre-checks
- second-stage `nir` module tree split under `builder/`, `structuring/conditionals/`, and `tests/`

#### Changed

- made x86 pathological CFG handling more conservative
- prioritized short-circuit chains before plain `if` recovery when they close on the same join
- split `builder/mod.rs` and promoted `structuring/conditionals.rs` into a directory module

#### Improved

- `7zr.exe 0x401804` and `0x402778` no longer time out due to long-running `try_lower_if()`
- retained direct preview on `putty.exe 0x140006260` and `everything.exe 0x140183590`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v36-v58 - `putty` Aggregate Copy Closure + x86 Timeout Diagnosis

This stretch had two goals:

1. remove the last aggregate transit temp from `putty.exe 0x140006260` until preview reached `RECT local_3c; *param_2 = local_3c;`,
2. determine whether heavy x86 `7zr.exe` timeouts came from Rust NIR or native extraction.

#### Added

- dead temp cleanup for aggregate transit temps
- prepare/native/preview diagnostic logging
- finer structuring-phase diagnostic logging

#### Changed

- recovered `LPRECT param_2`, `RECT local_3c`, and `*param_2 = local_3c;` for `putty.exe 0x140006260`
- removed dead aggregate transit temps like `xVar32`
- instrumented native prepare, preview p-code extraction, and Rust structuring boundaries

#### Improved

- closed the x64 aggregate-copy/type-surface target on `putty.exe 0x140006260`
- narrowed heavy x86 `7zr.exe` timeouts to Rust `structuring`, especially `try_lower_if()`

#### Validation

- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- regression/diagnostic reruns for `putty`, `everything`, `notepad++`, and `7zr`

---

## 2026-03-14

### v26-v35 - Preview Coverage Recovery + `putty` Type-Surface Recovery

The goals in this span were:

1. restore direct `mlil-preview` coverage on real large functions,
2. bring the type surface back up on `putty.exe 0x140006260` after direct preview had been recovered.

#### Added

- more detailed preview/native coverage diagnostics
- x86 preview bootstrap regression guard
- stack-slot naming recovery for direct preview
- stronger indirect import / Win64 argument recovery
- site-sensitive lowering infrastructure inside the builder

#### Changed

- reduced p-code extraction work in giant dispatcher cases
- added linear fallback caching and fast paths to Rust NIR structuring
- relaxed builder lowering carefully to recover `putty.exe 0x140001160`
- extended wide aggregate copy recovery with lane matching and prior-def lowering
- improved pointer-deref printing quality

#### Improved

- `putty.exe 0x140001160`: direct preview recovered
- `everything.exe 0x140183590`: direct preview retained
- `7zr.exe 0x401000`: direct preview retained
- `putty.exe 0x140006260`: recovered `LPRECT param_2`, `GetClientRect(...)`, `local_3c`, and whole-object assignment path progression

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- `cargo check -p fission-tauri`

### v25 - NIR Module Tree Refactor

This round was about maintainability rather than new algorithms. The growing `nir` core was split into `builder / normalize / structuring / tests` subsystems to reduce future edit and regression costs.

#### Changed

- reorganized `crates/fission-pcode/src/nir/` into:
  - `builder/`
  - `normalize/`
  - `structuring/`
  - `tests/`
- narrowed `nir/mod.rs` to entrypoint/wiring responsibilities
- split normalize responsibilities into arithmetic/boolean normalization, cleanup, slot/table surfacing, and bitstream helpers
- split structuring responsibilities into conditionals, loops, switch, and linear fallback

### v24 - Preview Coverage Recovery First, x64 + x86 in Parallel

This round focused on restoring direct preview output on real x64 functions again while also bringing up the first real x86 preview bootstrap path.

#### Added

- finer preview unsupported-reason diagnostics
- PE x86 preview bootstrap path

#### Changed

- relaxed branch-target recovery to improve x64 large-function direct preview coverage
- made region builder more aggressive about trivial forwarding/cleanup/tail-return absorption
- canonicalized identical-input `MULTIEQUAL`
- preserved slot-family / bitstream helper / loop-body compaction while fixing the application order around coverage-first goals

#### Improved

- `putty.exe 0x140006260`: direct preview recovered again
- `everything.exe 0x140183590`: direct preview recovered again
- at least one fixed-seed `7zr.exe` function reached direct preview, confirming the first real x86 bootstrap success

### v16 - Preview Type Surface Quality + Direct `putty 0x140006260` Output

This round pushed preview beyond “structured pseudocode exists” toward more natural known-signature type surfaces. The main target was direct preview of `putty.exe 0x140006260` with `LPRECT`, `RECT`, and whole-object assignment style output.

#### Added / Changed

- known-signature-based type surface context in preview
- preview binding type hints
- stronger p-code JSON opcode alias parsing
- layout-based fallthrough analysis for preview CFG recovery
- direct preview understanding of `goto(target, cond)` style real p-code branches
- containment so preview optimizer panic would not collapse the whole path

#### Improved

- `putty.exe 0x140006260 --engine mlil-preview` could directly surface:
  - `LPRECT param_2`
  - `RECT local_3c`
  - whole-object assignment style output

### v15 - Preview Quality Uplift + Low-Risk Function Promotion

The target here was not higher legacy success, but making `mlil-preview` the better path on lower-risk functions.

#### Added / Changed

- canonical `switch` reconstruction in preview
- preview-only surface cleanup
- centralized `engine_used` source of truth in `fission-static`
- widened `auto` preview eligibility on stable multi-block functions

#### Notes

- Preview coverage and structuring improved significantly, but preview type surface quality still lagged legacy on representative cases such as `putty.exe 0x140006260`.

### v14 - Legacy `type` Failure Removal + 90/90 Closure

This round focused on removing the remaining legacy `type` failures and restoring benchmark closure without counting `mlil-preview` rescue as equivalent success.

#### Improved

- removed the last known legacy `type` failures for that benchmark round
- retained preview direct output on representative targets

### v13 - MLIL Preview Structuring / Readability Uplift

This round strengthened the preview path around:

- canonical multi-block `if`, `if/else`, `while`, and `do-while`
- short-circuit boolean chains
- `PIECE` / `SUBPIECE` recombination
- cast-density reduction and lower-level residue cleanup

### v10-v12 - Experimental Fission MLIL/NIR Path Integrated Into Product Surfaces

This was the point where `mlil-preview` stopped being a CLI-only experiment and became a real engine mode exposed in both CLI and Tauri.

#### Added

- `legacy | mlil-preview | auto` engine modes
- engine selector in the Tauri decompiler options UI
- engine/fallback badges in the decompile view
- Rust-owned preview NIR/HIR + printer path

#### Changed

- adopted lightweight p-code extraction before the full native action pipeline when possible
- fixed wrapped negative constant parsing
- expanded multi-block canonical `if/if-else` lowering
- added conservative `MULTIEQUAL`, `PIECE`, and `SUBPIECE` lowering

#### Improved

- preview generated direct output across real smoke samples instead of remaining an isolated prototype path

---

## Historical Milestones (Late 2025 – Early 2026)

The repository history before the current architecture convergence includes several major milestones. The detailed Korean notes remain available in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md); the summaries below capture the public-facing highlights.

### Multithreaded Performance Breakthrough (157s -> 10s)

- introduced global Sleigh, GDT, and data-section scan caches
- added a core-level fail-fast timeout tripwire for monster functions
- reduced large batch decompilation wall-clock time dramatically on `putty.exe`

### Decompiler Performance + Success-Rate Uplift

- improved one-shot CLI throughput and overall decompilation success rate
- instrumented postprocess timing and removed major bottlenecks
- fixed recursive decompilation and duplicate-variable-piece failure classes
- built the first fair batch benchmark runner against PyGhidra baselines

### Security Policy / CI Gate Hardening

- added `docs/build/SECURITY_ADVISORIES.md`
- restored security checks as a CI quality gate
- documented advisory baselines and review policy

### Stabilization / Portability / Phase 2–4 Refactors

- removed panic-prone `unwrap/expect` paths across loader/analysis/ffi/tauri code
- converted pass pipelines toward `Cow<str>`-based no-op fast paths
- removed hardcoded local build paths in favor of environment-based discovery

### Postprocess Modularization

- split the large `postprocess.rs` implementation into focused modules
- separated naming, structure, arithmetic, and shared condition utilities
- added dedicated postprocess module documentation and tests

### Major Decompiler Quality Round + v4 Benchmark System

- fixed four large-quality bugs in postprocessing and structure handling
- introduced the v4 benchmark system with multi-platform suites
- significantly improved benchmark scores across ARM64, x64, Linux, and Windows

### x86 / MinGW / Type Propagation Expansion

- added MinGW-focused type propagation improvements
- brought in x86 benchmark suites and comparison binaries
- improved call propagation, loop conversion, and x86 normalization quality

### P-code Optimizer / Constant Substitution / RTTI / Listing / CFG Work

- introduced the early p-code optimization pipeline
- added context-aware constant substitution
- expanded listing, RTTI recovery, CFG analysis, and disassembly support

### Tauri Migration and Desktop Product Surface

- completed the move from the older egui UI to Tauri 2.x + React / TypeScript
- added large portions of the desktop workflow:
  - function navigation
  - assembly/decompile views
  - CFG views
  - project save/load
  - debugger surfaces
  - timeline/TTD experiments
- removed the legacy `fission-ui` egui codebase after the migration

### Analysis Pipeline / Data-Section Scan Consolidation

- unified batch analysis context and analysis-pass entrypoints
- consolidated data-symbol scanning and registration
- expanded FFI surface for function and prototype configuration

### Loader / Function Discovery

- added linear-sweep function discovery for stripped code
- improved function recovery on x86 and x64 binaries

### Early Core Capabilities Established

By this point Fission had already accumulated the foundations that still shape the current system:

- PE / ELF / Mach-O loading
- static analysis and disassembly
- Ghidra native decompiler integration
- Rust-side orchestration
- benchmarking infrastructure
- desktop UI foundations
- the first steps toward a Fission-owned decompiler core
