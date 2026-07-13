# Program Metadata Preflight For Canonical Decompilation

## Baseline And Owner

- Structural failure: canonical `decomp` and `list` commands build the shared
  immutable `ProgramSnapshot`, but static function discovery is skipped unless
  the user explicitly selects a discovery profile.
- Consequence: loader facts reach NIR/type/name recovery, while conservative
  executable thunk and call-target facts can remain inventory-only.
- Owner: CLI program-analysis preflight before `FactStore` construction.

## Invariant

Canonical decompilation and function listing run the existing conservative
SLEIGH discovery profile once after loading and before any shared `FactStore` is
frozen. Explicit balanced or aggressive profiles override the default. Batch
decompilation continues to create one shared snapshot after that preflight.

## Reuse And Risk

- Reuse `discover_functions_with_runtime`; add no scanner, semantic pass, or
  binary-specific rule.
- The conservative profile remains fail-closed when a SLEIGH runtime is not
  available.
- Info, section, import, string, and other loader-only commands do not pay the
  discovery cost.
- Library callers retain explicit control by supplying a prepared binary or
  shared `FactStore`.

## Validation

- Canonical CLI parser tests prove conservative defaults for `decomp` and
  `list`, plus explicit profile override.
- Full `fission-cli`, `fission-static`, and `fission-analysis-db` tests.
- Focused local decompilation confirms the discovered program snapshot is used
  without changing semantic output.

## AI Firewall

The default is a named, reusable analysis profile. It does not depend on a
function name, address, binary path, corpus row, compiler tuple, or benchmark
score.
