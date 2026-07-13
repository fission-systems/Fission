# Program Metadata Pipeline Integration

## Baseline And Owner

- Baseline: each decompilation context reconstructs `FactStore` from
  `LoadedBinary`, cloning loader type and debug maps and re-reading raw function,
  import, and symbol tables.
- Owner: `fission-analysis-db` owns immutable program metadata;
  `fission-static::FactStore` owns mutable decompilation overlays;
  `fission-decompiler` assembles the per-function type context.
- Failure mode: program facts have parallel representations, and structuring
  hints are stored as synthetic DWARF records, which loses provenance.

## Invariant

For one loaded binary, all decompilation functions consume the same immutable
`ProgramSnapshot`. Analysis-produced names, types, calling conventions, and
structuring hints remain overlays and never mutate or impersonate loader/debug
facts. Call-target recovery reads the canonical snapshot rather than rebuilding
parallel indexes from raw loader fields.

No condition depends on a benchmark function, address, binary identity,
compiler tuple, or ISA-specific register name.

## Ownership And Risk

- Extend existing owners; add no NIR pass or output heuristic.
- Keep `LoadedBinary` access where bytes, identity reports, or on-demand lifting
  are still required.
- Keep `fission-pcode` independent of program databases.
- Preserve existing name priority and debug-hint precedence.
- Do not import Ghidra vendor data into runtime resources unless a measured
  metadata category is absent from the existing `utils` bundle.

## Validation

- `cargo nextest run -p fission-analysis-db`
- `cargo nextest run -p fission-static`
- `cargo nextest run -p fission-decompiler`
- `cargo check -p fission-decompiler`
- Rebuild the local benchmark adapter and rerun metadata parity plus a focused
  decompilation smoke row.
- Confirm benchmark/static audit scans remain clean.

## AI Firewall

The implementation request was expressed as architecture boundaries and
metadata ownership only. Production code contains no corpus row, function name,
address, binary path, compiler tuple, or Ghidra output-style condition.
