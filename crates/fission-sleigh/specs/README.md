# fission-sleigh specs

This directory is the canonical home for SLEIGH language specs used by `fission-sleigh`.

- Primary path: `crates/fission-sleigh/specs/languages/`
- Initial contents were migrated from `ghidra_decompiler/languages/`.

Migration note:
- This is a compatibility-first migration step.
- Legacy `ghidra_decompiler/languages/` is intentionally kept for now.
- After all call sites are switched and validated, legacy C++/Ghidra coupled paths can be retired.
