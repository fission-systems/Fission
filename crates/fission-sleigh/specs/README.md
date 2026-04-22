# fission-sleigh specs

This directory is the canonical home for SLEIGH language specs used by `fission-sleigh`.

- Primary path: `crates/fission-sleigh/specs/languages/`
- Source of truth: `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Processors/*/data/languages/`
- Import policy: mirror each Ghidra `Processor/data/languages` directory into the same
  Processor-named subdirectory here; do not treat `vendor/rsleigh/slaspec` as the
  canonical import source.
- Processor layout now mirrors Ghidra 1:1.
  - Example: `crates/fission-sleigh/specs/languages/AARCH64/`
  - Example: `crates/fission-sleigh/specs/languages/ARM/`
  - Example: `crates/fission-sleigh/specs/languages/PowerPC/`
  - Example: `crates/fission-sleigh/specs/languages/x86/`
- Current mirror coverage:
  - `38` processors
  - `146` `.slaspec` variants
- Canonical checked-in manifest:
  - `crates/fission-sleigh/specs/ghidra_language_manifest.json`
- Language lookup remains recursive under `specs/languages/`, so callers resolve by entry
  stem, derived language id, or compatibility alias instead of hardcoding a directory.

Migration note:
- This is a compatibility-first migration step.
- Legacy spec call sites have been switched to this directory.
- This directory is now the single maintained source for local SLEIGH specs.

Compiler-only front-end note:
- The clean-room compiler wave now targets every checked-in `.slaspec` variant under `crates/fission-sleigh/specs/languages/<Processor>/`.
- Deterministic generated output is checked in under `crates/fission-sleigh/generated/<Processor>/<entry-spec-stem>/`.
- The all-variant manifest is `crates/fission-sleigh/generated/compiler_manifest.json`.
- Regenerate the compiler-only artifacts with:
  - `cargo run -p fission-sleigh --example generate_sleigh_frontends`
- Generated artifacts are compiler products consumed by the new runtime registry.
- Runtime p-code template execution is still incomplete; unsupported variants must fail closed rather than falling back to a hand-lifter.
