# Fission Architecture Documentation

## Workspace Structure (`crates/*`)

Fission is a Cargo workspace with strict crate boundaries:

- `fission-core`: shared configuration, error types, common models, plugin trait types
- `fission-loader`: binary loading/parsing (PE/ELF/Mach-O), symbol/language metadata extraction
- `fission-disasm`: disassembly abstraction (iced-x86 based)
- `fission-pcode`: P-code IR and optimizer pipeline
- `fission-signatures`: API/signature and relation databases
- `fission-ffi`: unsafe boundary to native decompiler and pcode C ABI
- `fission-analysis`: analysis logic (CFG/xref/decomp wrapping/debug/unpacker/plugin/script)
- `fission-tauri`: Tauri 2.x + React 19 desktop GUI (backend commands + frontend)
- `fission-cli`: entrypoints, one-shot/interactive CLI, TUI binaries

## Dependency Direction

Primary dependency flow:

`fission-core` -> (`fission-loader`, `fission-signatures`, `fission-disasm`) -> `fission-pcode` -> `fission-analysis` -> (`fission-tauri`, `fission-cli`)

Native integration flow:

`ghidra_decompiler/src/*` <-> `fission-ffi` <-> `fission-analysis`

Notes:

- `fission-ffi` owns unsafe/native boundary.
- `fission-analysis` consumes `fission-ffi` through feature gates (`native_decomp`).
- UI/CLI should remain consumers (presentation/orchestration), not business-logic owners.

## Runtime Layers

### 1) Static Analysis Layer

- Loader: parse binary bytes and construct `LoadedBinary`
- Disassembly: instruction decoding and textual rendering
- P-code/CFG/XRef: IR-based analysis and graph modeling
- Signatures: API/function identification and relation checks

### 2) Decompilation Layer

- Native decompiler integration is feature-gated (`native_decomp`)
- `fission-analysis::analysis::decomp` provides safe high-level wrapper/cache
- `fission-ffi` provides ABI + safe wrappers for Rust callers

### 3) Dynamic Analysis Layer

Two distinct domains exist in `fission-analysis`:

- `debug/`: interactive debugging (attach/step/register/memory/ttd scaffolding)
- `unpacker/`: runtime memory extraction/reconstruction (IAT rebuild, dump/fix)

`unpacker` is not a general interactive debugger; it is purpose-built for extraction and reconstruction workflows.

### 4) Presentation Layer

- `fission-tauri`: Tauri backend (`commands/`) + React 19 frontend (`src/panels/`, `src/components/`)
- `fission-cli`: CLI arguments, one-shot analysis commands, interactive REPL/TUI

## Feature Gates

Important workspace-level features:

- `native_decomp`: enables native decompiler path (`fission-ffi` + analysis integration)
- `gui` / `cli` / `tui`: binary/runtime surface selection

Use `#[cfg(feature = "native_decomp")]` for native decompiler dependent code paths.

## Error Handling Policy

- Prefer `fission_core::errors::FissionError` and `fission_core::Result<T>`
- Core and analysis logic should propagate errors (`?`) rather than panic
- CLI/UI handlers should report/log errors instead of crashing

## Concurrency & Performance

- Zero-copy and shared ownership patterns are used for large binary data (`DataBuffer`, `Arc`)
- Caching used for decompilation and repeated analyses
- `rayon` is available for CPU-bound analysis tasks

## Native Code Boundary Rule

`ghidra_decompiler/decompile` is upstream source.  
Modify wrappers/integration code under `ghidra_decompiler/src/*` and Rust crates, not upstream `decompile` internals.
