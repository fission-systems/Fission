# Changelog: DIE Loader Detection and GUI Metadata Surfacing

**Date:** 2026-04-30
**Scope:** `fission-loader` DIE detection resources/parser, Tauri GUI detection metadata

## Summary

Moved Detect-It-Easy based loader enrichment toward a Fission-owned, vendored-resource model and surfaced the resulting static detections in the desktop UI. This does not make DIE the binary format owner and does not execute DIE JavaScript/DSL. Loader format identity remains owned by Fission parsers; DIE detections are metadata enrichment only.

Strict policy remains:

- no runtime/build dependency on `vendor/Detect-It-Easy-master`
- no external DIE binary/library/process execution
- no YARA execution in this wave
- unsupported DIE DSL constructs are ignored or counted as unsupported metadata, not treated as successful evidence
- detection metadata does not alter load-spec selection, SLEIGH decode, or raw P-code semantics

## Implementation Notes

### Vendored DIE Resources

Added the checked-in DIE resource mirror under:

```text
utils/signatures/die/detect-it-easy/
```

Included resource families:

- `db`
- `db_extra`
- `db_custom`
- `yara_rules`
- `LICENSE`

The active code resolves this checked-in `utils/signatures` tree and does not read directly from the vendor checkout.

### DIE Parser / Matcher

Expanded the static `.sg` primitive parser to cover more exact DIE patterns without interpreting the full DSL:

- compact byte patterns such as `60BE........8DBE`
- wildcard bytes using `.`, `?`, and `$`
- quoted ASCII byte pattern fragments such as `"'UPX!'"`
- `Binary.compare(...)`
- `PE/ELF/MACH/MSDOS.compare(...)`
- `PE/ELF/MACH/MSDOS.compareEP(...)`
- static literal offsets
- EOF/file-size anchored offsets when statically recoverable
- overlay presence and overlay byte compares
- section count and section numeric predicates
- section and overlay entropy predicates

Unsupported dynamic helper calls, loops, arithmetic-heavy offsets, and other non-static DSL constructs are not promoted to detection success. They are counted as unsupported rule metadata.

### Module Boundary

Started splitting the larger DIE engine into clearer owner modules:

- `detector/die_engine/rules.rs`: static rule model and selector/comparison types
- `detector/die_engine/resources.rs`: vendored DIE resource discovery and `.sg` file enumeration
- `detector/die_engine.rs`: database load/merge, parser extraction, matcher, and tests

The public detector API remains unchanged.

### GUI Surfacing

Added detection metadata to the Tauri binary info DTO:

- detection type
- name
- version
- confidence
- details/provenance

The Explorer sidebar now shows a read-only `Detections` panel, and the status bar shows the detection count. This is display-only metadata and does not change loader or decompiler behavior.

## Validation

Completed:

```text
cargo test -p fission-loader die_engine -- --test-threads=1
cargo check -p fission-tauri
cargo check -p fission-cli
npm run build
```

Audit:

```text
rg "vendor/Detect-It-Easy-master|Detect-It-Easy-master" crates utils Cargo.toml Cargo.lock -g'!*detect-it-easy/*'
```

Result: no active references outside the checked-in DIE resource mirror.

Observed warnings during Rust checks are existing workspace warnings in downstream crates and are not introduced by this loader metadata change.

## Remaining Work

- Split the remaining parser/matcher logic out of `die_engine.rs` into dedicated parser and matcher modules.
- Add CLI JSON exposure for DIE detection details if needed for non-GUI workflows.
- Add fixture-level GUI/CLI parity tests for detection metadata.
- Keep YARA execution as a separate future analyzer wave; current `yara_rules` are stored resources only.
- Continue treating DIE signatures as loader metadata enrichment, not as format/load-spec authority.

## Raw P-code PuTTY Installer Check

Ran the requested raw P-code probe against:

```text
benchmark/binary/x86-64/window/commercial_binary/binary/PuTTY_V0.83.exe
```

Result: the file is not a PE executable image. It is a Windows Installer MSI stored as a Compound Document File:

```text
sha256: d816fba5750e95ae5f845ad22bd165e19ebefbbf298f453abc1db2ef7655e4b8
magic: d0 cf 11 e0 a1 b1 1a e1
file: Composite Document File V2 Document, MSI Installer
```

Fission loader result:

```text
UnsupportedFormat: unknown binary format
```

Ghidra raw P-code probe at `0x0` produced a typed no-instruction result, and the Fission raw probe was stopped because the input has no executable PE instruction stream to compare. This is an input/loader-family classification issue, not a SLEIGH raw P-code parity regression.

Artifact directory:

```text
benchmark/artifacts/raw_p_code_benchmark/putty_v083_installer_msi_attempt
```

Follow-up: run the raw P-code lane on an actual extracted or downloaded PuTTY PE executable, not the MSI installer container.

## Exact Container Classification

Added the implementation plan for container-aware loader routing without string
or name heuristics. Fission now treats container inputs as a distinct pre-loader
classification owner rather than falling through to generic unknown executable
loading.

Implementation policy:

- executable loaders remain PE/COFF/ELF/Mach-O/HEX/MZ/NE/a.out only
- Compound Document, ZIP, gzip, and Cabinet route to typed container failures
- Compound Document detection validates the CFB header shape and does not infer MSI from strings
- benchmark loader smoke records `input_classification` and `next_action`
- realworld suite orchestration skips raw/full lanes when the same manifest fails loader preflight as non-executable

## Raw P-code SQLite DLL Smoke

Ran a temporary six-row raw P-code smoke against:

```text
benchmark/binary/x86-64/window/commercial_binary/binary/sqlite3.dll
```

Rows were selected from loader-discovered PE x86-64 entry/export function seeds
and run with Ghidra `--disassemble-missing` so the oracle materializes
instructions at the requested export addresses.

Result:

```text
report: /tmp/sqlite3_raw_pcode_smoke_disassemble/aggregate_raw_pcode_parity_report.json
row_count: 6
full_match: 48
average_similarity_score: 1.0
average_parity_ratio: 1.0
compat_emitter_used: 0
fake_placeholder_op: 0
invalid_pcode_shape: 0
template_source_totals: sla_construct_tpl=48
```

An initial run without `--disassemble-missing` produced Ghidra
`no instruction` rows while Fission decoded branch thunks. The corrected run
confirms this was oracle materialization setup, not a Fission raw P-code
semantic mismatch.

Expanded the same DLL check to the first 100 loader-discovered entry/export
function seeds:

```text
report: /tmp/sqlite3_raw_pcode_export100/aggregate_raw_pcode_parity_report.json
row_count: 100
full_match: 800
average_similarity_score: 1.0
average_parity_ratio: 1.0
compat_emitter_used: 0
fake_placeholder_op: 0
invalid_pcode_shape: 0
template_source_totals: sla_construct_tpl=800
fission_wall_clock_sec: 327.68184033373836
ghidra_wall_clock_sec: 321.9895864216378
```

This keeps the current x86-64 `.sla ConstructTpl` raw P-code path exact across a
larger real-world PE DLL export surface, not just the synthetic canonical rows.

## Raw P-code Timing Breakdown

Split Fission raw P-code benchmark timing so performance reports now separate
process/harness startup, binary loading, frontend loading, and checked
decode/lift execution.

New Fission timing fields:

```text
process_startup_sec
binary_load_sec
frontend_load_sec
decode_lift_sec
rust_probe_sec
decode_lift_instructions_per_sec
decode_lift_pcode_ops_per_sec
```

The Rust `raw_pcode_probe` example now records binary load, SLEIGH frontend
load, and decode/lift timings directly. The Python benchmark harness subtracts
the Rust probe runtime from wall clock time to expose process startup/harness
overhead instead of folding it into SLEIGH execution.

Validation used the same temporary sqlite3 DLL six-row smoke:

```text
report: /tmp/sqlite3_raw_pcode_timing_smoke/aggregate_raw_pcode_parity_report.json
row_count: 6
full_match: 48
average_similarity_score: 1.0
average_parity_ratio: 1.0
compat_emitter_used: 0
fake_placeholder_op: 0
invalid_pcode_shape: 0
template_source_totals: sla_construct_tpl=48
process_startup_sec: 12.839296834027973
binary_load_sec: 12.647225417000001
frontend_load_sec: 5.239065835
decode_lift_sec: 0.00176879
```

This confirms the current sqlite3 smoke wall-clock gap is not in checked
`.sla ConstructTpl` execution. Native backend policy is unchanged: native code
remains candidate acceleration only, and final raw P-code success still requires
common checked `.sla ConstructTpl` execution.

## Raw P-code EverPlanet x86 Smoke

Ran a temporary raw P-code smoke against:

```text
benchmark/binary/x86/window/commercial_binary/binary/EverPlanet_KR_v1842_U_DEVM.exe
```

Loader classification:

```text
format: PE
arch: x86
bits: 32
entry: 0xc2cc03
functions: 1
imports: 336
exports: 0
sections: 6
```

The first entry-window run used one row with `count=32`:

```text
report: /tmp/everplanet_raw_pcode_entry_smoke/aggregate_raw_pcode_parity_report.json
row_count: 1
bucket_totals: unsupported_template=1, missing_fission_instruction=31
first_error: UnsupportedPcodeTemplate: x86: operand_template_resolution_failed: subtable rel32 did not export a handle
compat_emitter_used: 0
fake_placeholder_op: 0
invalid_pcode_shape: 0
```

To avoid treating the first fail-closed instruction as 31 cascade misses, the
same Ghidra-disassembled entry window was split into 32 one-instruction rows:

```text
report: /tmp/everplanet_raw_pcode_entry_split/aggregate_raw_pcode_parity_report.json
row_count: 32
bucket_totals:
  unsupported_template: 15
  fission_decode_error: 14
  input_varnode_mismatch: 2
  pcode_opcode_mismatch: 1
  pcode_op_count_mismatch: 1
  length_mismatch: 1
average_similarity_score: 0.07565398391812866
average_parity_ratio: 0.0
compat_emitter_used: 0
fake_placeholder_op: 0
invalid_pcode_shape: 0
template_source_totals: sla_construct_tpl=3
owner_hint_totals:
  unsupported_template: 15
  decode_length: 15
  handle_selector_resolution: 1
  varnode_identity: 1
  template_opcode_sequence: 1
```

This is an x86 32-bit SLEIGH runtime coverage gap, not a loader failure and not
an approximate P-code regression. The direct next owner is `.sla` operand export
handle resolution for 32-bit subtables such as `rel32`, `rel8`,
`check_Reg32_dest`, `check_rm32_dest`, and `xrelease`, plus 32-bit stack
`VarnodeTpl` size handling.

## Commit Scope Notes

- Benchmark output artifacts and generated Ghidra DB state are not commit material.
- Existing unrelated dirty files in SLEIGH/NIR/runtime areas are outside this changelog scope.
