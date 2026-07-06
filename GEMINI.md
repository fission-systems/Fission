# Fission Gemini Instructions

For decompiler-quality work, follow [`SKILL.md`](SKILL.md) first.

Key constraints:

- Treat benchmark rows, Ghidra diffs, and AI suggestions as evidence only.
- Translate every semantic fix into an owner-native invariant before editing.
- Prefer shared CFG, def-use, type-constraint, calling-convention, or alias facts
  over another narrow pass.
- Do not add function/address/binary/corpus-specific production branches.
- Do not copy or depend on `vendor/` reference code.
