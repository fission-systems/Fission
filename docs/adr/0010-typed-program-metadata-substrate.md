# ADR 0010: Typed program metadata substrate

**Status:** Accepted
**Last verified:** 2026-07-13

## Context

Fission's loader already recovers sections, functions, symbols, relocations,
debug facts, and architecture selection. Those facts currently remain in
parallel vectors and maps on `LoadedBinary`, while downstream analysis copies
selected values into service-specific stores. That makes provenance, identity,
and ownership difficult to preserve and encourages CFG, type recovery, and NIR
passes to invent local metadata channels.

Ghidra's advantage is not Java object allocation by itself. Its analyses share
a program database with stable addresses, symbols, references, functions,
memory blocks, source provenance, and transactions. Fission needs the same
contract without adopting mutable global object graphs.

## Decision

`fission-analysis-db` owns Fission's typed, immutable whole-program metadata
view. The dependency direction is:

```text
fission-loader             parses format-specific facts
        |
        v
fission-analysis-db        canonicalizes IDs, provenance, and references
        |
        v
static analysis / decompiler / automation consume read-only snapshots
```

The initial `ProgramSnapshot` covers binary identity, memory blocks, functions,
symbols, and relocations. Every row has a deterministic typed ID and explicit
provenance. Snapshot table order is canonical and must not depend on hash-map or
discovery insertion order.

Memory blocks distinguish mapped size from format-declared virtual size and
file-backed size. For PE sections, mapped size includes initialized raw bytes
when `SizeOfRawData` exceeds `VirtualSize`; the source sizes remain available
for format diagnostics.

The following ownership rules apply:

- Loaders continue to own byte parsing, address mapping, and format semantics.
- The analysis database does not parse files and does not run NIR passes.
- Static and semantic analyses may propose future typed fact deltas; they must
  not mutate loader maps or encode facts in unrelated DWARF/PDB structures.
- NIR, normalization, and structuring consume read-only analysis views. They do
  not become program metadata stores.
- CLI and benchmark surfaces serialize the canonical snapshot; they do not
  independently reconstruct equivalent metadata schemas.

Ghidra comparison is a reference audit, not an implementation dependency. Exact
agreement is expected for facts that both tools can prove from the same binary
(mapped ranges, permissions, symbol/relocation addresses, function entries).
Source labels and heuristic confidence are compared separately because analysis
policy can legitimately differ.

## Consequences

- Program-wide metadata has a canonical owner outside `fission-pcode`.
- Rust ownership remains explicit: immutable snapshots replace a JVM-style
  mutable object graph rather than attempting to emulate it.
- Existing `LoadedBinary` and `FactStore` consumers can migrate incrementally;
  this ADR does not require a flag-day crate split.
- New metadata facts require source and confidence instead of another parallel
  `HashMap<u64, _>`.
- Direct Fission/Ghidra metadata parity can expose missing input facts before
  CFG or decompiler passes are blamed.
