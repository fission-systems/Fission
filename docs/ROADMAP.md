# Roadmap

This document tracks the current **medium-term priorities** for Fission.

Detailed idea notes can live under `docs/idea/*` and deeper analysis can live under `docs/analysis/*`, but priority decisions should follow this document.

## Current Direction

The current direction of Fission is:

1. **Keep the legacy path stable**
   - native Ghidra decompilation plus Fission postprocess
   - retained as the fallback / compatibility baseline
2. **Expand the `mlil-preview` path**
   - Ghidra p-code input with Fission-owned NIR/HIR and a Rust printer
   - move this toward the default product path over time
3. **Establish the Session Fact Store**
   - converge rename / FID / cross-image / inferred type / debug facts into one aggregation layer
   - make decompile paths and UI consume the same fact source
4. **Clean up docs / benchmarks / metrics**
   - measure preview / native / assembly fallback separately
   - track quality improvement and legacy retirement criteria numerically

## Near-Term Priorities

### 1. Legacy Deprecation Inventory + Preview-First Routing

The current goal is not to delete `legacy`, but to make preview-first the default product policy and keep `legacy` only as an explicit fallback / compatibility mode.

Key items:

- stop presenting `legacy` as a normal GUI workflow choice
- keep CLI `--engine legacy` only as a hidden compatibility mode
- fix fallback taxonomy around:
  - `preview_timeout`
  - `preview_unsupported`
  - `native_pcode_failure`
  - `legacy_fallback`
  - `assembly_fallback`
- keep an explicit artifact-level inventory of corpora/functions that still require legacy baselines

Current legacy intervention points:

- preview skip/error -> native decompile fallback in `fission-static select_preview_output()`
- explicit legacy compatibility mode and preview rescue / assembly fallback paths in CLI oneshot decompile
- preview failure -> native decompile or explicit assembly/native failure surface in Tauri
- benchmark/compare scripts that collect both legacy baselines and fallback taxonomy

Conditions before legacy can shrink further:

- fixed-seed corpora reproducible with preview direct or explicit native/assembly fallback only
- `putty`, `everything`, `WinMerge`, `EverPlanet`, and `ida76sp1` watchlist functions terminate without hangs
- the set of functions that still need a legacy baseline remains explicitly tracked in benchmark artifacts

### 2. Type Recovery / Type Failure Reduction

The clearest current quality bottleneck is still type-related failure, not only structuring.

Representative hard cases:

- `putty 0x1400052b0`
- `putty 0x140006380`
- `cmkr 0x140002cc0`

Next-round goals:

- reduce legacy type failures
- preserve fallback quality
- improve type recovery without destabilizing the preview path

### 3. `mlil-preview` Coverage Expansion

Preview has become meaningful, but it is not yet a full replacement.

Priority areas:

- more direct handling of multi-block functions
- stronger loop/header normalization
- lower label/goto fallback ratios
- wider `switch` and complex-CFG coverage

### 4. Preview Quality Improvement

Areas where preview already performs well:

- short-circuit folding
- loop lowering
- cast canonicalization
- `PIECE` / `SUBPIECE` recombination

Next improvement areas:

- type-aware expression quality
- better aggregate handling
- broader preview-owned idiom recognition
- better large-function readability

## Medium-Term Direction

### 1. Strengthen the Fission-Owned Decompiler Stack

The long-term direction is to strengthen the following structure:

- Ghidra: lift / CFG / baseline type recovery / fail containment
- Session Fact Store: symbol / type / name aggregation
- Fission NIR: normalization / stack abstraction / temp coalescing
- Fission HIR: structured pseudocode
- Rust printer: final output

The goal is not “a better postprocessor for Ghidra output,” but a **Fission-owned decompiler built on top of Ghidra as the lower engine**.

### 2. Make Preview the Default Product Path

Medium-term goals:

- higher preview adoption
- more stable preview output quality
- keep `legacy` only as explicit fallback / compatibility

### 3. Reorganize Rewrite Ownership

New rules should live in one of three ownership tiers only:

- canonicalization
- idiom recovery
- polish

That means new work should move toward NIR/HIR or structured-preview ownership rather than continuing to accumulate string-level postprocess rules.

## GUI / Product

### Keep Tauri as the Main GUI

The current GUI source of truth is Tauri.

Remaining priorities:

- keep preview-first decompile UX stable
- keep native / assembly fallback surfaces explicit and understandable
- continue dynamic debug / timeline work on a separate track

### Clean Up Legacy egui Docs

- `docs/gui/GUI_GUIDE.md` is not the current source of truth
- long-term, the documentation set should converge on Tauri-first docs while older egui-era notes are shrunk or archived

## Docs / Benchmark

### Documentation Hierarchy

Top-level public documents:

- [`README.md`](../README.md)
- [`docs/README.md`](./README.md)
- [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md)
- [`docs/changelog/CHANGELOG.md`](./changelog/CHANGELOG.md)

### Benchmark Management

Principles to retain:

- separate preview / native / assembly fallback metrics
- keep raw JSON as artifacts and summaries as checked-in docs
- preserve regression sets around `everything`, `putty`, and `cmkr`

Additional corpus focus:

- `ida76sp1`
  - x64 multi-DLL C++ / plugin corpus
  - useful for large C++ GUI + shared DLL + plugin ecosystem regression
  - future use for cross-image symbol/type propagation experiments

## Out Of Scope For Now

Not current priorities:

- broad semantic renaming
- large-scale xref sync / GUI interaction polish overhauls
- another full-system performance-optimization re-entry
- large invasive modifications to Ghidra core

## Related Docs

- [`docs/FEATURES.md`](./FEATURES.md)
- [`docs/analysis/KNOWN_ISSUES.md`](./analysis/KNOWN_ISSUES.md)
- [`docs/benchmark/grand_finale_summary.md`](./benchmark/grand_finale_summary.md)
