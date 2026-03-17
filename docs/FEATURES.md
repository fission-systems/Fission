# Features

This document summarizes the features that Fission currently provides.

For the public repository, Fission should still be understood as an **early prototype**. The items below describe the current implemented or experimentally usable surface, not a mature end-user product feature list.

Use [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md) for the architectural source of truth and [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md) for recent change history.

## Decompilation Engines

Fission currently has two decompilation paths.

### `legacy`

Native Ghidra decompilation plus the Fission postprocess pipeline.

This is still the most stable path and the current quality baseline for serious use.

Key capabilities:

- WinAPI-signature-driven type promotion
- `CONCAT` / piece residue cleanup
- goto reduction and CFG-oriented cleanup
- switch clustering
- temp inlining
- stack / piece access normalization

### `mlil-preview`

The forward path that takes Ghidra p-code and emits pseudocode through Fission-owned NIR/HIR plus a Rust printer.

Current support:

- PE x64
- bootstrap-level PE x86 support on selected seeds
- stack-slot recovery
- multi-block `if`
- multi-block `if/else`
- short-circuit `&&` / `||`
- multi-block `while`
- multi-block `do-while`
- cast canonicalization
- `PIECE` / `SUBPIECE` recombination
- preview-owned label/goto fallback

Current limits:

- general output quality may still lag `legacy`
- some large functions or type-heavy functions still require fallback
- semantic renaming and aggressive member-name guessing are intentionally not enabled

## Binary / Architecture Support

Supported formats:

- PE
- ELF
- Mach-O

Supported architectures in the broader workspace:

- x86
- x86-64
- ARM64 / AArch64

Current first-class `mlil-preview` scope remains focused on **PE x64**, with limited bootstrap-level x86 preview coverage on selected cases.

## Analysis / Recovery Capabilities

Core static-analysis capabilities include:

- function discovery
- imports / exports / strings / sections
- disassembly
- xref / CFG-oriented analysis
- p-code optimization
- signature / type DB loading
- FID-based symbol identification

## Type / Signature Features

Current type/signature infrastructure in-tree:

- Windows signature DBs in `fission-signatures`
- WinAPI prototype injection
- structure / pointer type promotion
- GDT loading
- baseline type propagation

Representative strong cases today:

- WinAPI structure pointer surfacing such as `LPRECT`, `RECT`, and `LPMSG`
- parameter / structure cleanup on the legacy path

## CLI Features

The CLI currently provides:

- binary info
- function list
- strings
- disassembly
- single-function decompilation
- batch decompilation
- benchmark mode
- engine selection

Key options:

- `--profile balanced|quality|speed`
- `--engine legacy|mlil-preview|auto`
- `--timeout-ms`
- `--benchmark`
- `--ghidra-compat`

## Desktop GUI Features

The current Tauri GUI exposes:

- function list / filtering
- assembly tabs
- decompile tabs
- decompiler options dialog
- engine selector (`legacy`, `mlil_preview`, `auto`)
- engine-used / fallback badges
- strings / imports / exports / search / CFG-adjacent panels

Important note:

- [`docs/gui/GUI_GUIDE.md`](./gui/GUI_GUIDE.md) is an older egui-era document, not the source of truth for the current Tauri UI

## Benchmark Snapshot

Checked-in benchmark summary:

- [`docs/benchmark/grand_finale_summary.md`](./benchmark/grand_finale_summary.md)

Current observed direction:

- `legacy` remains the stable baseline
- `mlil-preview` coverage and structuring quality are improving quickly
- preview benchmark work has been steadily reducing goto and temporary-surface residue on covered functions

## Known Limits

Important current limits:

- `mlil-preview` is not yet a full replacement for `legacy`
- some `type`-heavy functions remain hard cases even on the legacy path
- semantic renaming, advanced field naming, and broad high-level idiom recovery are still in progress

## Related Docs

- [`docs/README.md`](./README.md)
- [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
- [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)
- [`docs/ROADMAP.md`](./ROADMAP.md)
