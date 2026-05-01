# 2026-05-01 Changelog

## SLEIGH Raw P-code Constant Varnode Parity Restore

- Reverted the broad heuristic-purge regression and restored the canonical x86-64 raw P-code lane to perfect structural parity.
- Fixed the remaining `feature-fibonacci-lea-dec` failure at `0x14000148e` without instruction-family or architecture-specific fallback.
- Root cause: `.sla` `ConstTpl` produced a const-space value of `0xffffffffffffffff`; Fission rejected it as exceeding `i64`, while Ghidra emits the same bit pattern as constant `-1`.
- Change: const-space `VarnodeTpl` / `HandleTpl` offsets are now converted through the existing `Varnode::constant(i64)` representation by preserving the 64-bit bit pattern.
- Added a targeted runtime regression test for `8d 41 ff` LEA negative displacement emission.

## Validation

- `cargo check -p fission-sleigh`
- `cargo test -p fission-sleigh generated_runtime_decodes_lea_negative_displacement_const_without_decode_error -- --test-threads=1`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- Raw P-code gate:
  - Report: `benchmark/artifacts/raw_p_code_benchmark/const_signed_restore/aggregate_raw_pcode_parity_report.json`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 46`

## Notes

- No approximate P-code path was added.
- No mnemonic-family semantic emitter was added.
- No architecture-name branch was added for the fix.
- Benchmark artifacts and Ghidra project DB state remain uncommitted.

## SLEIGH Canonical Gate Audit

- Added reporting-only legacy path audit fields to raw P-code probe/report output.
- The audit records debt involvement without changing semantic success: `BoundOperand -> FixedHandle` fallback, no-export subtable fallback, legacy shared-token policy, direct token parser, compatibility template source, and source-line/opprint remap.
- Promoted the x86-64 canonical perfect gate as the regression command: `--require-perfect-canonical --expected-full-match 44`.
- Preserved the strict success rule: successful rows must still report real `.sla ConstructTpl` source and cannot count compatibility or approximate P-code as success.

## Canonical Gate Audit Validation

- Report: `benchmark/artifacts/raw_p_code_benchmark/canonical_gate_audit/aggregate_raw_pcode_parity_report.json`
- `full_match = 44`
- `average_similarity_score = 1.0`
- `average_parity_ratio = 1.0`
- `compat_emitter_used = 0`
- `fake_placeholder_op = 0`
- `invalid_pcode_shape = 0`
- `template_source_totals.sla_construct_tpl = 46`
- `legacy_path_audit_totals.bound_operand_fixed_handle_fallback = 12`
- `legacy_path_audit_totals.legacy_shared_token_policy = 46`
- `legacy_path_audit_totals.no_export_subtable_fallback = 14`

## Raw P-code Multi-binary Manifest Expansion

- Added `binaries[]` manifest support to `run_raw_pcode_parity.py`.
- The runner now flattens binary-level metadata into the existing row execution path, preserving the legacy `rows[]` manifest contract.
- Added suite filters for larger manifests: `--binary-id`, `--language-filter`, and `--max-rows-per-binary`.
- Added aggregate `binary_count`, `language_count`, `binary_totals`, and `language_totals` fields for suite-level reporting.
- Added `benchmark/raw_p_code_benchmark/multi_binary_smoke.json` as a checked-in example manifest.

## Multi-binary Validation

- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- `python3 -m json.tool benchmark/raw_p_code_benchmark/multi_binary_smoke.json`
- Filtered smoke report: `benchmark/artifacts/raw_p_code_benchmark/multi_binary_smoke_filtered/aggregate_raw_pcode_parity_report.json`
  - `row_count = 2`
  - `binary_count = 1`
  - `full_match = 2`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
- Schema smoke report: `benchmark/artifacts/raw_p_code_benchmark/multi_binary_smoke_filtered_schema/aggregate_raw_pcode_parity_report.json`
  - `row_count = 1`
  - `binary_count = 1`
  - `language_count = 1`
  - `full_match = 1`
- Canonical gate report: `benchmark/artifacts/raw_p_code_benchmark/multi_manifest_canonical_gate/aggregate_raw_pcode_parity_report.json`
  - `row_count = 17`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `template_source_totals.sla_construct_tpl = 46`

## Ghidra Parity Gap Audit

- Added `scripts/audit/ghidra_parity_audit.py` as a reporting-only owner-chain audit for Ghidra parity gaps.
- Added `docs/architecture/GHIDRA_PARITY_GAP_AUDIT.md` to track current SLEIGH, loader, and FID/signature implementation status against Ghidra 12.0.4 structural owners.
- The audit records gaps without changing semantics:
  - SLEIGH remains partial because `.sla ConstructTpl` execution is active, but legacy token cursor and BoundOperand-derived handle debt still appear in successful-row audits.
  - Loader remains partial because implemented executable loaders are Fission-owned, while lower-priority Ghidra loader families stay typed unsupported.
  - FID remains partial because raw `.fidbf` records decode through a DBHandle-style reader, while packed `.fidb` and complete program-seeker/hash input parity remain typed unsupported.
- Updated the raw P-code benchmark README to point at the Ghidra parity audit command.

## Ghidra Parity Audit Validation

- `python3 scripts/audit/ghidra_parity_audit.py --markdown`
- Raw P-code gate report: `benchmark/artifacts/raw_p_code_benchmark/ghidra_gap_audit/aggregate_raw_pcode_parity_report.json`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 46`
- Current audit snapshot:
  - `sleigh_native_model = partial`
  - `sleigh_token_cursor = legacy_debt`
  - `sleigh_handle_resolution = legacy_debt`
  - `sleigh_compatibility_sources = legacy_debt`
  - `loader_family_matrix = partial`
  - `loader_raw_binary = typed_unsupported`
  - `loader_postload_analyzers = legacy_debt`
  - `fid_raw_dbhandle = partial`
  - `fid_hash_and_match = partial`

## Architecture-organized Vendor Binary Corpus

- Reorganized `vendor/binaries/tests/*` into `benchmark/binary/<architecture>/vendor_binaries/<source-family>/` instead of a single flat vendor corpus copy.
- Copied only the architecture/test binary families from `vendor/binaries/tests`; excluded `vendor/binaries/.git`, `.github`, `tests_src`, `tests_data`, and other source/support-only trees.
- Preserved original source-family names under each architecture bucket so provenance remains explicit:
  - `x86_64 -> benchmark/binary/x86-64/vendor_binaries/x86_64`
  - `x86`, `i386 -> benchmark/binary/x86/vendor_binaries/*`
  - `aarch64 -> benchmark/binary/AARCH64/vendor_binaries/aarch64`
  - `armel`, `armhf -> benchmark/binary/ARM7_le/vendor_binaries/*`
  - MIPS, PowerPC, RISC-V, JVM/Dalvik, SuperH4, PA-RISC, m68k, s390x, alpha, and mixed corpus families were placed under their matching benchmark architecture buckets.

## Vendor Corpus Copy Validation

- Verified source/destination file counts for all copied families:
  - `25` source-family directories copied.
  - `949` source files matched `949` destination files.
- Size audit:
  - total copied corpus footprint is approximately `350M`.
  - no copied file exceeded `90M`, so the corpus does not hit GitHub's single-file size limit.

## Vendor Binary Multi-corpus Raw P-code Smoke

- Added `benchmark/raw_p_code_benchmark/vendor_binary_smoke.json` as a `binaries[]` manifest over newly organized vendor corpus samples.
- The smoke covers four entry-point rows:
  - x86-64 ELF: `benchmark/binary/x86-64/vendor_binaries/x86_64/fauxware`
  - x86-64 PE: `benchmark/binary/x86-64/vendor_binaries/x86_64/windows/not_packed_pe64.exe`
  - x86 ELF: `benchmark/binary/x86/vendor_binaries/i386/fauxware`
  - x86 PE: `benchmark/binary/x86/vendor_binaries/x86/windows/not_packed_pe32.exe`

## Vendor Binary Smoke Validation

- `python3 -m json.tool benchmark/raw_p_code_benchmark/vendor_binary_smoke.json`
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- Report: `benchmark/artifacts/raw_p_code_benchmark/vendor_binary_smoke/aggregate_raw_pcode_parity_report.json`
  - `row_count = 4`
  - `binary_count = 4`
  - `language_count = 2`
  - `full_match = 13`
  - `average_similarity_score = 0.8799006944444444`
  - `average_parity_ratio = 0.8125`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 16`
- Per-binary outcome:
  - `vendor-x64-elf-fauxware`: 4/4 full match.
  - `vendor-x64-pe-not-packed`: 4/4 full match.
  - `vendor-x86-elf-fauxware`: 4/4 full match.
  - `vendor-x86-pe-not-packed`: 1/4 full match, with remaining owner hints under `decode_length`, `handle_selector_resolution`, and `template_opcode_sequence`.

## Vendor x86 PE SLEIGH Operand Extent Fix

- Fixed the `vendor-x86-pe-not-packed-entry` stream desync at `0x4014e3` without adding table-name, mnemonic, source-line, or binary-specific mapping.
- Root cause: x86 `.sla` operand expressions for the PE32 entry stream were not fully decoded; `rel32` depended on Ghidra-style instruction-boundary pattern expressions, and the walker could under-consume the `C7 /0` absolute-store immediate slice.
- Added `.sla` pattern expression support for `start_exp`, `end_exp`, and `next2_exp` as native pattern-expression variants. `start_exp` and `end_exp` now feed the existing operand expression evaluator; unresolved `next2_exp` remains fail-closed.
- The `0x4014e3` row now decodes as a 10-byte `mov dword ptr [0x405034],0x0` and emits exact raw P-code: `COPY const(0,4) -> ram(0x405034,4)`.
- Replaced the old no-export subconstructor fallback with parent-template handle-reference validation. Referenced operands without an exported fixed handle now fail typed instead of receiving a dummy handle; guard-only operands are allowed only when the parent `ConstructTpl` does not reference that handle.
- Removed the canonical `fixed_handle_for_bound_operand` helper. Fixed handles used by this path now come from `.sla` token/expression/export metadata; `BoundOperand` remains display/debug DTO state.
- Added `vendor_x86_pe_c7_moffs_imm32_uses_sla_extents` as a targeted x86 regression test.

## Vendor x86 PE Validation

- `cargo check -p fission-sleigh`
- `cargo test -p fission-sleigh vendor_x86_pe_c7_moffs_imm32_uses_sla_extents -- --test-threads=1`
- `cargo test -p fission-sleigh generated_runtime_decodes_startup_call_rel32_without_compatibility_lift -- --test-threads=1`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py`
- Disassembly smoke: `not_packed_pe32.exe @ 0x4014e0`
  - `0x4014e0`: `sub ESP,0xc`, 3 bytes
  - `0x4014e3`: `mov dword ptr [0x405034],0x0`, 10 bytes
  - `0x4014ed`: `call 0x402200`, 5 bytes
- Vendor smoke report: `benchmark/artifacts/raw_p_code_benchmark/vendor_binary_smoke_no_manual_mapping/aggregate_raw_pcode_parity_report.json`
  - `full_match = 16`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 16`
- Canonical gate report: `benchmark/artifacts/raw_p_code_benchmark/canonical_gate_no_manual_mapping/aggregate_raw_pcode_parity_report.json`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 46`

## Vendor x86 PE Notes

- No approximate P-code path was added.
- No architecture-name semantic branch was added.
- No table-name or binary-specific rule was added for `C7 /0`, `rel32`, `addr32`, or `rm32`.
- `fixed_handle_for_bound_operand` and `fallback_binding_for_no_export_subtable` are absent from active SLEIGH compiler/runtime source outside audit-test string literals.

## GitHub Release Corpus Collection

- Added `scripts/corpus/collect_github_release_samples.py` for CLI-first GitHub release sample collection.
- The collector queries GitHub release metadata, filters assets by explicit include/exclude regexes, emits a URL list, and only downloads binaries when `--download` is provided.
- Downloaded assets are stored under `benchmark/binary/realworld/github` by default and are treated as local corpus artifacts, not source files.
- Generated manifest entries include SHA-256, size, repository, release tag, asset name, asset URL, content type, and source config index.
- Added `benchmark/config/benchmark_corpus/github_release_sources.example.json` as a non-binary example source config.
- Updated `.gitignore` so `benchmark/binary/realworld/**` stays out of git while `benchmark/binary/realworld/.gitkeep` can keep the local corpus root.
- Updated `benchmark/BENCHMARK_GUIDE.md` with the GitHub release collection command and the no-binary-staging policy.

## Loader Accuracy Gate

- Strengthened the loader route contract so only inputs with a concrete, format-proven load spec can become executable `LoadedBinary` instances.
- Removed the generic `unknown:LE:32:default` success path from raw binary, Intel HEX, Motorola S-record, MZ, NE, and Unix a.out loaders.
- Raw binary loading now remains fail-closed unless an explicit caller-provided raw load hint is supplied.
- Intel HEX, Motorola S-record, MZ, NE, and Unix a.out still validate exact headers/records, but return typed `LoadSpecNotFound` until a real load-spec source is available.
- Expanded exact container routing for XZ, 7z, RAR, and Unix ar archives. These inputs now return `ContainerRequiresExtraction(...)` instead of falling through to unknown/raw executable handling.
- Added `GzfLoader` to the known unsupported loader-family table using the exact Ghidra packed-file magic, returning typed `UnsupportedLoaderFamily(GzfLoader)`.
- Updated loader smoke classification with explicit `load_spec_not_found` and `malformed_header` buckets.

## Loader Accuracy Validation

- `cargo check -p fission-loader`
- `cargo test -p fission-loader --lib -- --test-threads=1`
  - `55 passed`
  - Note: the plain `cargo test -p fission-loader` command reached rustdoc/doc-test execution and did not terminate promptly in this workspace; unit tests were run with `--lib` to isolate loader behavior.
- `cargo check -p fission-core`
- `cargo check -p fission-cli`
- `cargo check -p fission-tauri`
- `cargo build --release -p fission-cli`
- `python3 -m py_compile scripts/benchmark/*.py scripts/corpus/*.py`
- GitHub release loader smoke:
  - Report: `benchmark/artifacts/realworld_suite/github_release_loader_accuracy_gate/loader_smoke_report.json`
  - `row_count = 9`
  - `loaded = 3`
  - `load_failed = 6`
  - `failure_bucket_counts.container_requires_extraction = 6`
  - `input_classification_counts.executable_loaded = 3`
  - `input_classification_counts.container:ZipArchive = 2`
  - `input_classification_counts.container:Xz = 4`
- Raw P-code canonical gate:
  - Report: `benchmark/artifacts/raw_p_code_benchmark/loader_accuracy_gate_canonical/aggregate_raw_pcode_parity_report.json`
  - `full_match = 44`
  - `average_similarity_score = 1.0`
  - `average_parity_ratio = 1.0`
  - `compat_emitter_used = 0`
  - `fake_placeholder_op = 0`
  - `invalid_pcode_shape = 0`
  - `template_source_totals.sla_construct_tpl = 46`

## Loader Accuracy Notes

- No heuristic MSI/container string detection was added.
- No approximate loader metadata or raw executable fallback was added.
- No downloaded binaries, benchmark output artifacts, or Ghidra project DB files are staged with this change.

## Ghidra Data Snapshot Vendoring

- Added `utils/ghidra-data` as a checked-in Ghidra data snapshot so future loader/signature/analyzer work can consume local data without runtime/build dependency on `vendor/ghidra`.
- Copied only data resources, not Java implementation code:
  - `Ghidra/Features/Base/data` no-return lists, no-return constraints, PE Rich product table, parser profiles, Win16/Win32/Win64 symbol hint/export data, and string n-gram model.
  - `Ghidra/Features/FileFormats/data/languages/*.opinion`.
  - `Ghidra/Processors/*/data/languages/*.opinion`.
  - `Ghidra/Processors/*/data/patterns/*`.
  - `Ghidra/Processors/*/data/extensions/*`.
  - Processor thunk metadata such as `aarch64-pltThunks.xml`.
- Preserved Ghidra `LICENSE`, `NOTICE`, and `DISCLAIMER.md` beside the snapshot.
- Added `utils/ghidra-data/MANIFEST.sha256.json` with source path, size, and SHA-256 for every vendored file.
- Snapshot size: `160` data/provenance files, approximately `29M`.

## Ghidra Data Snapshot Validation

- `python3 -m json.tool utils/ghidra-data/MANIFEST.sha256.json`
- Verified copied file count: `160`.
- Verified snapshot size: approximately `29M`.
- No Ghidra Java source files were copied into `utils/ghidra-data`.

## GitHub Release Corpus Validation

- `python3 -m py_compile scripts/corpus/collect_github_release_samples.py scripts/corpus/hash_and_manifest.py scripts/benchmark/run_loader_smoke.py scripts/benchmark/run_realworld_suite.py`
- `python3 -m json.tool benchmark/config/benchmark_corpus/github_release_sources.example.json`
- `python3 scripts/corpus/collect_github_release_samples.py --help`
- `git check-ignore -v benchmark/binary/realworld/github/example.bin`
- `git check-ignore -q benchmark/binary/realworld/.gitkeep`

## SLEIGH Specs Snapshot Migration

- Added `utils/sleigh-specs` as the checked-in SLEIGH spec snapshot location for compiler/runtime language discovery.
- Preserved the existing crate-local `crates/fission-sleigh/specs` tree as a legacy fallback while moving active resolution to repo-level utility data.
- Added `utils/sleigh-specs/MANIFEST.sha256.json` with source path, size, and SHA-256 for every copied spec resource.
- Added `utils/sleigh-specs/README.md` documenting snapshot ownership, refresh source, and resolution order.
- Updated `fission-sleigh` discovery to resolve specs in this order:
  - `FISSION_SLEIGH_SPEC_DIR`
  - repo-relative `utils/sleigh-specs`
  - legacy crate-local `crates/fission-sleigh/specs`
- Updated runtime registry manifest embedding to use `utils/sleigh-specs/ghidra_language_manifest.json`.
- Updated `fission-sleigh` README/AGENTS notes so SLEIGH spec ownership points at `utils/sleigh-specs`.

## SLEIGH Specs Snapshot Validation

- `cargo fmt -p fission-sleigh`
- `CARGO_TARGET_DIR=/tmp/fission-target-sleigh-specs cargo check -p fission-sleigh`
- `CARGO_TARGET_DIR=/tmp/fission-target-sleigh-specs cargo test -p fission-sleigh x86_64_entry_spec_exists_under_arch_tree -- --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/fission-target-sleigh-specs cargo test -p fission-sleigh infer_arch_from_entry_spec_uses_arch_subdirectory -- --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/fission-target-sleigh-specs cargo test -p fission-sleigh compile_frontend_for_entry_spec_collects_inventory -- --test-threads=1`
- `CARGO_TARGET_DIR=/tmp/fission-target-sleigh-specs cargo build --release -p fission-cli`
- `python3 -m json.tool utils/sleigh-specs/MANIFEST.sha256.json`
- Snapshot size: approximately `14M`.
- Snapshot file count: `698` files including the manifest.
