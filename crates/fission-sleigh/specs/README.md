# fission-sleigh specs

This directory is the canonical home for SLEIGH language specs used by `fission-sleigh`.

- Primary path: `crates/fission-sleigh/specs/languages/`
- Source of truth: `vendor/ghidra/ghidra_12.0.4_PUBLIC/Ghidra/Processors/*/data/languages/`
- Import policy: mirror Ghidra language directories directly into the matching
  architecture subdirectory here; do not treat `vendor/rsleigh/slaspec` as the
  canonical import source.
- Architecture layout:
  - `crates/fission-sleigh/specs/languages/aarch64/`
  - `crates/fission-sleigh/specs/languages/arm32/`
  - `crates/fission-sleigh/specs/languages/mips/`
  - `crates/fission-sleigh/specs/languages/powerpc/`
  - `crates/fission-sleigh/specs/languages/riscv/`
  - `crates/fission-sleigh/specs/languages/x86/`
- Language lookup is recursive under `specs/languages/`, so callers still resolve by language
  name rather than hardcoding a subdirectory.
- Initial contents were migrated from a legacy external language-spec tree.

Migration note:
- This is a compatibility-first migration step.
- Legacy spec call sites have been switched to this directory.
- This directory is now the single maintained source for local SLEIGH specs.

Compiler-only front-end note:
- The first clean-room compiler wave targets `crates/fission-sleigh/specs/languages/x86/x86-64.slaspec`.
- Deterministic generated output is checked in under `crates/fission-sleigh/generated/x86/`.
- Generated artifacts are compiler products only; they are not yet the canonical runtime decoder path.
