# Fission Architecture

Updated: 2026-04-15

## Ownership

- Semantic owner: `fission-pcode`
- Structuring owner: `fission-pcode::nir::structuring`
- Orchestration owner: `fission-decompiler-core`
- Facts and native preparation owner: `fission-static`
- Printer and postprocess: consume-only

Ghidra parity gaps are tracked separately in
`docs/architecture/GHIDRA_PARITY_GAP_AUDIT.md`. That audit is reporting-only:
it must not be used as a semantic repair layer or as justification for
approximate P-code success.

## Decompiler Layers

### `fission-pcode`

`fission-pcode` owns the canonical decompiler semantics:

- P-code to NIR/HIR lowering
- normalization and recovery
- structuring legality
- `StructureGraph`, `RegionProof`, and related telemetry
- `NirBuildStats` and `NirHintStats`

Structuring decisions must be made here. Downstream crates must not reconstruct semantic policy or region legality.

### `fission-decompiler-core`

`fission-decompiler-core` owns application-layer orchestration:

- request/result contracts
- engine selection
- routing between legacy and NIR paths
- type-context assembly from facts
- worker execution and render orchestration
- fallback policy and postprocess sequencing

It consumes canonical semantic policy from `fission-pcode`. It does not redefine legality or quality counters.

### `fission-static`

`fission-static` is a service/provider crate:

- fact extraction and provenance
- native decompiler preparation
- binary-derived static helpers

It does not own decompiler semantics, region legality, or postprocess policy.

### `fission-loader`

`fission-loader` owns binary format loading and metadata provenance. Its
canonical pipeline follows the Ghidra Loader owner chain:

1. `detect`: identify PE executable, COFF object, ELF, Mach-O, or Mach-O fat.
2. `probe/load-spec`: select architecture and load specification from format metadata.
3. `map`: build file-offset/RVA/VA memory blocks and permissions.
4. `symbols`: classify code/data, imports, exports, thunks, undefined externals, and debug-only symbols.
5. `finalize`: build `LoadedBinary`, `FunctionInfo`, imports, exports, and canonical function views.

PE/COFF/ELF/Mach-O parsing is Fission-owned through bounds-checked byte readers.
`object` is not a loader decision owner; it may be used only as fixture/debug
inspection support. `gimli` and `pdb` remain specialized DWARF/PDB metadata
readers rather than primary binary loaders.

Ghidra loader family coverage is staged. The implemented executable-loader group is
`PeLoader`, `CoffLoader`/`MSCoffLoader`, `ElfLoader`, `MachoLoader`,
`BinaryLoader` (explicit raw hint only), `IntelHexLoader`, `MotorolaHexLoader`,
`MzLoader`/`NeLoader`, and `UnixAoutLoader`. Lower-priority or separate-wave
families are `DyldCacheLoader`, `PefLoader`, `SomLoader`, `OmfLoader`,
`Omf51Loader`, `DbgLoader`, `DefLoader`, `MapLoader`, `GdtLoader`, `GzfLoader`,
and XML/debug helper loaders. Known but unsupported families must fail closed
with a typed loader message such as `UnsupportedLoaderFamily(<name>)`. Raw binary
loading is never an automatic fallback for unknown bytes because that would hide
malformed or unsupported formats.

Container inputs are not executable loaders. Archive/file-system inputs such as
Compound Document, ZIP, gzip, and Cabinet are classified before executable
loading and fail closed with `ContainerRequiresExtraction(<name>)` until an exact
extractor/file-system owner is implemented. Compound Document detection validates
the CFB header shape; MSI classification is not inferred from strings or names.
Raw P-code and full benchmark lanes must skip these rows unless an extracted
executable child is explicitly provided.

Loader provenance is a public contract shared by CLI and GUI surfaces.
`FunctionInfo.origin`, `kind`, `is_import`, `is_export`, `is_thunk_like`,
`external_library`, and `source_section` classify whether a record is code,
entry, export, true import, import thunk, undefined external, debug-only symbol,
or data-derived metadata. User-facing and decompile-seed function lists must go
through `loader::function_view`; CLI and GUI must not reconstruct independent
function/import/export filtering rules.

Language/runtime analyzers live outside format parsing. `loader/analyzers`
contains post-load enrichment such as C++ RTTI, Go pclntab/type metadata, and
Rust vtable scanning. These analyzers may add functions or inferred types, but
they do not own format detection, load-spec selection, or memory mapping.

## Structuring Model

The active structuring path is a hard-cutover Ghidra-style CFG owner model.

- `StructureGraph` is the internal collapsed overlay owner.
- `CollapseDriver` applies deterministic collapse rules.
- `RegionProof` and rewrite execution decide whether a region may be promoted.
- `linear` is an explicit fallback surface, not a late semantic repair layer.

The implementation still parses legacy engine names for compatibility, but active execution resolves to the graph/collapse path.

The active rule flow is:

1. Build a `StructureGraph` from CFG/basic-block facts.
2. Produce `RegionProof` and replacement/readiness evidence for candidate regions.
3. Collapse only proof-complete, replacement-complete, emit-ready regions.
4. Surface final HIR from the collapsed graph.
5. Fall back to explicit unstructured or goto-based output when legality is incomplete.

Printer and postprocess must not reconstruct structure after this point.

## Benchmark / Telemetry Contract

- Canonical telemetry owner: `NirBuildStats`
- Benchmark/report layers project canonical counters only
- Row regression reasons should be derived from canonical structuring/materialization families, not from downstream heuristics

## Non-Goals

- `fission-cli` and `fission-tauri` are not semantic repair layers.
- `fission-static` should not regain decompiler policy ownership.
- printer or postprocess should not recreate structure when proof is absent.
