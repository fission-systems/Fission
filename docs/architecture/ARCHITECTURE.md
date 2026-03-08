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
- `fission-tauri`: Tauri 2.x + React 19 desktop GUI (backend commands + frontend). 소스는 `crates/fission-tauri/`이며 Rust 백엔드는 `src-tauri/` 하위에 있음.
- `fission-cli`: entrypoints, one-shot/interactive CLI (`fission_cli` 바이너리), TUI binaries

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

#### Per-binary decompiler preparation (single entry point)

All work that must be done before decompiling a given binary (load binary image, register memory sections, IAT/global symbols, symbol provider, known functions, FID DB, GDT when configured, etc.) is performed in **one place only**. The single entry point is `fission_analysis::analysis::decomp::prepare_native_decompiler_for_binary`. Both the CLI (oneshot) and the GUI (Tauri) create a decompiler instance and then call this function only.

**GDT, timeout, and errors** are controlled only from this entry point and from config. Callers pass `PrepareOptions` (e.g. `gdt_path` from `PATHS.get_gdt_path(binary.is_64bit)`, `timeout_ms` from `Config::default().decompiler.timeout_ms`). GDT is applied in prepare when a path is provided; timeout is reserved for when the native layer exposes it. Failure reporting stays as `last_error` → Rust `Result`; any step-specific error refinement should be done in this path so both CLI and GUI benefit.

**Prepare initialization cost** is measured per step when using `--benchmark`; the CLI adds `_meta.prepare_timings` (load_binary_ms, symbols_ms, symbol_provider_ms, sections_ms, known_functions_ms, fid_ms, gdt_ms) to the JSON output. Use this breakdown to drive optimization (e.g. skip empty work, limit FID count).

#### Decompiler performance optimization priorities

Priorities are ordered by impact, measurement data (`prepare_timings`), and implementation cost. Update this section when the order or items change.

- **Required safeguard (not a fundamental performance fix)**: **Timeout** — Prevents unbounded wait or excessive time per decompilation; config `timeout_ms` should be applied in the native/FFI path so that one slow function does not block the process or UI. This does not make decompilation faster; it bounds the damage when something is slow or stuck.

- **Performance priority 1**: **Reduce prepare init cost (e.g. FID)** — `prepare_timings` often shows FID loading as dominant. Options: skip empty work, limit FID paths/count, avoid retry on failure. This directly shortens init time.

- **Performance priority 2**: **GUI prefetch** — Connect `enable_prefetch` / `prefetch_count` to decompilation: prefetch nearby functions into cache so that scrolling or selection feels faster.

- **Performance priority 3**: **Batch/benchmark stability** — Use timeout and init metrics so that batch scripts and large binaries (e.g. putty) have predictable behavior; adjust per-step limits if needed.

- **Performance priority 4**: **C++ engine options** — Expose or document quality-vs-speed knobs in the native pipeline; upstream constraints may limit changes.

Error message refinement is a usability/debugging concern rather than performance; handle it separately (e.g. in the same prepare path for consistency).

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

## Decompiler Logging and Errors

- **Control surface**: Decompiler diagnostic logging is controlled only by `[decompiler].log_verbose` and `[decompiler].log_file` in config (see [fission.toml](../../fission.toml)). CLI overrides with `--verbose` (effective log = config `log_verbose` OR CLI `--verbose`).
- **Errors**: Failures are always reported via `last_error` on the C++ context and exposed as Rust `Result` / `FissionError::decompiler(...)`. This path is separate from the diagnostic log stream.
- **C++ contract**: When `log_verbose` is false, the native decompiler uses `log_output()` (null stream); when true, it uses stderr (and optionally the file set by `log_file`). `DecompilerNative::set_log_verbose` / `set_log_file` apply this at context creation or before use.
- **Clients**: CLI and GUI (Tauri) should both read `Config::default().decompiler.log_verbose` and `log_file`, and call `set_log_verbose` / `set_log_file` after creating the decompiler. CLI additionally uses `OutputSilencer` when not verbose so that any remaining stderr from the process is suppressed.

## Native Code Boundary Rule

`ghidra_decompiler/decompile` is upstream source.  
Modify wrappers/integration code under `ghidra_decompiler/src/*` and Rust crates, not upstream `decompile` internals.
