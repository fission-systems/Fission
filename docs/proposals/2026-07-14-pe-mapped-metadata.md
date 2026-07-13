# PE Mapped Metadata Recovery

## Baseline And Owner

- Structural failure: the loader exposes PE sections and base relocations, but
  the immutable program snapshot omits the mapped image headers. MinGW pseudo
  relocation discovery can also mistake a `_refptr_*` symbol for the list
  itself and emit a false use-site.
- Owner: PE loader facts and program metadata assembly. Neither issue belongs
  to NIR, structuring, type recovery, or the printer.

## Invariants

- A PE image has a mapped read-only header range only when a non-zero image base,
  the first section virtual address, and the minimum non-zero section raw offset
  establish a non-overlapping range. Its size is derived from the section table,
  never from a fixed corpus value.
- A MinGW pseudo-relocation list must be identified by its complete linker symbol
  identity after permitted leading underscore decoration. A reference-pointer
  symbol containing the list name is not the list.
- PE `ABSOLUTE` base-relocation entries are alignment padding, not relocation
  application sites. They remain loader-format evidence but are not semantic
  relocation parity targets.

## Reuse And Risk

- Extend `ProgramSnapshot` memory assembly and the existing MinGW pseudo-reloc
  scanner; add no analysis pass.
- Fail closed when the PE address and raw-file ranges are inconsistent.
- Preserve section-derived block IDs deterministically after inserting the
  header block.
- Do not synthesize Ghidra labels, relocation addresses, or benchmark-specific
  entries.

## Validation

- Synthetic PE header mapping test in `fission-analysis-db`.
- Synthetic `_refptr_*` rejection test in `fission-loader`.
- Existing analysis-db, static discovery, loader, and CLI tests.
- Local metadata parity against Ghidra, with function entries, mapped block
  topology, and non-padding relocation addresses compared independently.

## AI Firewall

Production conditions use only documented PE section, symbol, and relocation
semantics. They contain no binary name, function name, address, corpus row,
compiler tuple, or Ghidra output-style branch.
