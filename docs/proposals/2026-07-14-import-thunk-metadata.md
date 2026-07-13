# Import Thunk Metadata Recovery

## Baseline And Owner

- Structural failure: executable stubs ending in an indirect jump through an
  import slot are validated by static discovery, but their target, import name,
  library, and `import_thunk` kind are discarded.
- Consequence: the program snapshot exposes the IAT slot but not the executable
  function unit that callers target.
- Owner: SLEIGH-backed static function discovery and program metadata assembly.

## Invariant

When the first decoded instruction of an executable function candidate is a
terminal jump whose normalized reference resolves to an authoritative import
slot, record a non-import thunk function at the executable address. Preserve the
IAT address as `thunk_target`, and derive symbol/library identity from the import
table. This is an instruction-flow and loader-fact invariant, not a byte pattern,
function-name rule, fixed address, or benchmark condition.

## Reuse And Risk

- Extend the existing `terminal_import_thunk` proof; add no new scanner or NIR
  pass.
- Keep IAT slot records and executable thunk records distinct.
- Canonical function inventory contains executable function units. A non-code
  IAT slot remains an import symbol and is not duplicated as a function record.
- Program metadata inventory runs conservative static discovery before freezing
  the immutable snapshot. Unsupported SLEIGH runtimes remain fail-closed.
- Do not target Ghidra-generated local labels; label parity is a separate symbol
  namespace and not function-discovery evidence.

## Validation

- Focused synthetic import-thunk test in `fission-static`.
- `cargo nextest run -p fission-static`.
- `cargo nextest run -p fission-cli` and `cargo check -p fission-cli`.
- Local metadata parity rerun against Ghidra; inspect function entry recall and
  candidate precision separately.
- Focused semantic row must remain passing.

## AI Firewall

Production logic receives only decoded flow, normalized references, executable
ranges, and authoritative import-table facts. It contains no benchmark identity,
address, binary path, compiler tuple, or reference-output style rule.
