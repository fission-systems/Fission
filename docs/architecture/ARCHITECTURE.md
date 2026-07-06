# Fission Architecture

Updated: 2026-04-15

## Ownership

- Semantic owner: `fission-pcode`
- Structuring owner: `fission-pcode::nir::structuring`
- Orchestration owner: `fission-decompiler`
- Facts and native preparation owner: `fission-static`
- Printer surfaces: consume-only

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

Internally, `fission-pcode` is not a single semantic dumping ground. Treat it as
a substrate plus owner-layer stack:

- **Substrate:** IR/HIR types, p-code/NIR contracts, telemetry, action-pipeline
  framework, shared CFG facts, def-use facts, type constraints,
  calling-convention facts, and alias facts.
- **Owner layers:** builder/materialize, normalize, type/data recovery,
  structuring, and render/printer.
- **Extraction candidates:** `fission-nir-analysis`,
  `fission-nir-normalize`, and `fission-structuring`.

New quality fixes should first ask whether the invariant belongs in shared
analysis. Repeated special cases must be absorbed into dataflow, def-use,
type-constraint, calling-convention, CFG, or alias analysis rather than added as
another narrow pass. A new owner, pass, helper, or metric is acceptable only when
the proposal shows that no existing owner or shared analysis surface expresses
the invariant.

### `fission-decompiler`

`fission-decompiler` owns application-layer orchestration:

- request/result contracts
- engine selection
- routing between legacy and NIR paths
- type-context assembly from facts
- worker execution and render orchestration
- Rust-Sleigh decode → NIR pipeline (`rust_sleigh`)

It **re-exports** the `fission_pcode` IR surface for convenience and consumes canonical semantic policy from `fission-pcode`. It does not redefine legality or quality counters.

### Cargo layering note

`fission-sleigh` depends only on `fission-pcode` (IR types). Orchestration therefore lives in `fission-decompiler`, which depends on both `fission-pcode` and `fission-sleigh`, avoiding a workspace dependency cycle.

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

**Binary identity (`loader::identity`).** After `LoaderPipeline::load`, Fission attaches an optional structured `BinaryIdentityReport` on the `LoadedBinary` wrapper (entropy, overlay tail hints, PE-oriented section/import signals, optional `/utils` resource summaries, bounded DIE JSON primitive subset telemetry + matches, PE TLS/debug-directory hints, MSVC CRT pattern hits near entry, WinAPI catalog coverage counts, evidence lists). This augments loader provenance for CLI JSON (`fission_cli info --identity`) and benchmark attribution; it is **not** a decompiler repair layer and does not alter parsing or IR. Flat rule/DiE-style detections remain available via `detector::detect` (`info --detections`).

PE/COFF/ELF/Mach-O parsing is Fission-owned through bounds-checked byte readers.
`object` is not a loader decision owner; it may be used only as fixture/debug
inspection support. `gimli` and `pdb` remain specialized DWARF/PDB metadata
readers rather than primary binary loaders.

Ghidra loader family coverage is staged. The implemented executable-loader group is
`PeLoader`, `CoffLoader`/`MSCoffLoader`, `ElfLoader`, `MachoLoader`,
`TeLoader`, `BinaryLoader` (explicit raw hint only), `IntelHexLoader`,
`MotorolaHexLoader`, `MzLoader`/`NeLoader`, and `UnixAoutLoader`. Lower-priority or separate-wave
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

**Loader Parity and Enrichment Features.** To achieve strict parity with Ghidra's binary analysis ecosystem, `fission-loader` implements advanced metadata enrichment and validation layers:
- **PE Delay-Load Imports**: Classifies delayed import directories (`ImgDelayDescr`) and generates placeholder/import symbols to feed downstream call analysis.
- **Rich Header Decryption**: Recovers and decrypts MSVC compiler telemetry headers, validating checksum matches against section/PE hashes to verify compiler/linker provenance.
- **ELF Version Symbols**: Parses GNU/SVR4 dynamic version symbols (`.gnu.version`, `.gnu.version_r`) to map version-dependent dynamic symbols to their precise external libraries/libraries requirements.
- **ELF RELRO Write Protection**: Detects and enforces Read-Only Relocations (`PT_GNU_RELRO`) block bounds and classifies section/segment write protection permissions accordingly.
- **Relocations Database**: Maintains a decoupled, indexed database of all base relocation entries to support rapid lookup of relocation-backed pointers during static analysis.
- **Virtual Header Structure Mapping**: Mapped structures (like ELF, PE, and TE headers) are registered as virtual structures in `InferredTypeInfo`, allowing symbol annotation and type context recovery.

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

## Pass Pipeline Architecture

Fission uses an explicit `Pass` pipeline framework (`nir::action_pipeline`) for all HIR transformation stages. Each stage is registered as a named `Pass` with a declared `GhidraActionConcept`, and is executed by a `PassManager` (`Pipeline` + `ActionGroup`) that owns fixed-point iteration and budget.

**Structuring stage** (`nir::structuring::passes`) is wired into this framework via `run_structuring_pipeline`, called from `render_mlil_preview_with_binary_and_context` after `normalize_hir_function`.

**Enforcement rules (architectural)**:

1. Every new transformation must be expressed as a `NirPass` implementation — not as an inline patch inside `build_sese_region_body`, `CollapseDriver`, or any other internal loop.
2. A `Pass` may only read/write through `PassCtx { func: &mut HirFunction, ... }`. It must not capture `PreviewBuilder` internal state or address-specific constants.
3. `AnalysisKey` dependencies (Dominance, PostDom, LoopBody, ...) must be declared via `fn requires() -> &[AnalysisKey]` so `PassManager` can enforce sane ordering.
4. `PassOutcome::changed: bool` must be accurate — returning `Changed` when nothing changed causes unnecessary fixed-point rounds; returning `Unchanged` when something changed silently breaks convergence.
5. Binary-specific or address-specific guard conditions inside a `Pass` body are **forbidden**. All admission conditions must derive from CFG properties (block count, SCC shape, dominance, etc.).

**Anti-patterns (will fail code review)**:
- `if func.name == "fibonacci" { ... }` inside a Pass body
- `if address == 0x140001470 { skip_rule() }` timer skips in Pass
- Adding a new `CollapseRule` variant without registering a corresponding `Pass`
- Round-about patches that bypass `PassCtx` by reaching into `builder::state`

## Decompiler Quality Firewall

Decompiler-quality changes are accepted at the architecture boundary, not at the
CI/dashboard boundary. A benchmark row, AI review prompt, Ghidra comparison, or
validation-pool result may motivate a change, but none of those surfaces own the
semantic decision.

The architectural admission path is:

1. Identify the canonical owner: SLEIGH/raw p-code, builder/materialize,
   normalize, structuring, type/data recovery, or printer.
2. State the invariant in owner-native terms: p-code semantics, ABI/ISA rule,
   CFG/dominance/postdominance fact, def-use fact, type constraint, calling
   convention fact, or memory-alias fact.
3. Implement the rule inside the existing owner/pass by default. New passes,
   helpers, metrics, or validation knobs require proof that no existing owner
   covers the invariant.
4. Validate with targeted invariant tests plus representative rows. Row
   improvement is evidence, not the acceptance condition.

The following are forbidden architectural dependencies for production semantic
code:

- benchmark function names,
- concrete addresses or row ids,
- binary paths or corpus names,
- compiler tuple labels used only to identify a row,
- AI prompt output that is not restated as an owner-native invariant,
- Ghidra presentation quirks such as comma-in-condition style when a clearer
  equivalent is possible.

The AI overfit firewall therefore lives in the same layer as the pass pipeline:
every AI-suggested or benchmark-motivated change must be translated into an
owner-native invariant before implementation. Static scans and CI jobs may help
find violations, but they are mirrors of this architecture contract, not the
source of the contract.

## Benchmark / Telemetry Contract

- Canonical telemetry owner: `NirBuildStats`
- Benchmark/report layers project canonical counters only
- Row regression reasons should be derived from canonical structuring/materialization families, not from downstream text matching

## Non-Goals

- `fission-cli` and `fission-tauri` are not semantic repair layers.
- `fission-static` should not regain decompiler policy ownership.
- printer or postprocess should not recreate structure when proof is absent.
