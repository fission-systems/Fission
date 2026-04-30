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

## Commit Scope Notes

- Benchmark output artifacts and generated Ghidra DB state are not commit material.
- Existing unrelated dirty files in SLEIGH/NIR/runtime areas are outside this changelog scope.
