# Fission Loader Guide

Generated: 2026-05-23
Scope: `crates/fission-loader/`

## Overview

`fission-loader` is the binary file parsing and metadata recovery crate. It is responsible for identifying formats, selecting architecture specifications, mapping memory blocks (RVAs/VAs), resolving relocations, classifying symbols (imports, exports, code, data, thunks), mapping virtual headers, and producing the canonical `LoadedBinary` wrapper consumed by the decompiler engine.

## Structure

```text
crates/fission-loader/
├── src/
│   ├── lib.rs                 # Crate entrypoint & exports
│   ├── prelude.rs             # Shared error & result types
│   ├── detector/              # Magic number and format routing detector
│   └── loader/                # Individual format loaders & pipeline
│       ├── mod.rs             # Registry and loader trait definitions
│       ├── pipeline.rs        # Main detection and loader routing dispatcher
│       ├── reader.rs          # Bounds-checked byte reader wrapper
│       ├── types.rs           # Core loader types (LoadedBinary, RelocationsDb, InferredTypeInfo)
│       ├── function_view.rs   # Canonical function, import, and export views
│       ├── pe/                # PE executable loader (imports, delay-load, Rich Header decryption, CRT entry)
│       ├── elf/               # ELF executable loader (dynamic versioning symbols, RELRO permissions mapping)
│       ├── macho/             # Mach-O executable loader
│       ├── coff/              # COFF and MS-COFF object loaders
│       ├── formats/           # Secondary and helper format loaders
│       │   ├── te.rs          # UEFI Terse Executable (TE) loader
│       │   ├── mz_ne.rs       # MZ / NE loaders
│       │   ├── hex.rs         # Intel / Motorola HEX loaders
│       │   ├── aout.rs        # Unix a.out loader
│       │   └── raw.rs         # Fallback Raw Binary loader
│       ├── identity/          # Evidence-backed BinaryIdentityReport generator
│       ├── types/             # Virtual structure and type builder helpers
│       ├── dwarf/             # DWARF debug symbols extractor
│       └── analyzers/         # Post-load enrichments (C++ RTTI, Go pclntab, Rust vtable)
```

## Loader Pipeline

All binary loads must traverse the canonical loader pipeline:
1. **`detect/route`**: Identify format or fail-closed on unsupported containers/formats.
2. **`probe/load-spec`**: Select architecture and compiler/ABI specification.
3. **`map`**: Construct virtual memory spaces, sections, and access permissions.
4. **`relocate`**: Parse and populate base relocations into the `RelocationsDb`.
5. **`symbols`**: Classify exports, imports, code, data, thunks, and debug symbols.
6. **`virtual-types`**: Register header structures (e.g. `ELF_HEADER`, `IMAGE_NT_HEADERS`, `EFI_TE_IMAGE_HEADER`) as virtual structs in `InferredTypeInfo`.
7. **`finalize`**: Build the `LoadedBinary` payload.

## Core Rules

1. **Fail-Closed Policy**: If a binary is corrupt, lacks minimal trustworthy facts, or matches a known unsupported loader family, it must return a typed loader error (e.g., `UnsupportedFormat`, `UnsupportedLoaderFamily`, `ContainerRequiresExtraction`). Best-effort speculation is prohibited.
2. **Pure Safe Rust**: All parsers must be written in safe Rust using bounds-checked byte reading. Do not bind or link to native C++ tools.
3. **Relocations Database**: Relocation logic must be decoupled and mapped into `RelocationsDb` for fast static analysis lookups by virtual address.
4. **Virtual Type Mapping**: Map binary structural headers and sections as inferred virtual types during finalization. This allows symbol annotation and downstream recovery layers to parse headers naturally.
5. **Deterministic Sections**: Ensure section offsets, alignment, and raw data mapping match target platform conventions exactly (e.g., TE stripped-headers file offset adjustment: `PointerToRawData - StrippedSize + sizeof(EFI_TE_IMAGE_HEADER)`).
6. **Analysis Snapshot Boundary**: `LoadedBinary` is the loader fact payload. Cross-layer typed IDs, provenance normalization, and program-wide references belong to `fission-analysis-db`; do not make the loader own downstream analysis state.

## Anti-Patterns

- Do not use file extensions to guess or route binary formats.
- Do not bypass `LoaderPipeline` to parse formats directly in UI or CLI commands.
- Do not hardcode absolute path configs or local resources inside loader logic.
- Do not let analyzers own format mapping, segment permissions, or default entry points.

## Validation

```bash
# Run loader unit and integration tests
cargo nextest run -p fission-loader

# Validate downstream decompiler compilation
cargo check -p fission-decompiler
cargo check -p fission-cli
```
