# ADR 0020: Evidence contracts live in a leaf crate; the analysis kernel sits above the engines

**Status:** Accepted
**Last verified:** 2026-11-26
**Related:** issue #1368 (Unified Analysis Kernel; this is its M0), #1002
(trace-guided decompilation), #1087 (fact graph and invalidation),
[ADR 0010](0010-typed-program-metadata-substrate.md) (immutable program
snapshots), [ADR 0014](0014-dir-product-vs-prestructure-substrate.md)

## Context

Static analysis, the decompiler, the emulator, the debugger, the solver and
the verification tools each learn things about a binary. There is no shared way
to say which code such a thing is about, so moving it between layers means
passing bare virtual addresses. A bare address is wrong as soon as the image
is relocated, remapped, patched or unpacked, and a fact passed that way has no
source or scope.

### Dependency audit

Workspace edges relevant to the placement (normal dependencies, at
`bb7cb1d89`):

| Crate | Workspace dependencies | Owns |
|---|---|---|
| `fission-core` | none | resource paths, config, logging, shared utilities; most crates depend on it |
| `fission-analysis-db` | `loader` (which pulls `core`, `signatures`) | immutable `ProgramSnapshot` with snapshot-local row ids |
| `fission-analysis-protocol` | none | CLI JSON payload shapes |
| `fission-pcode-ir` | none | p-code IR |
| `fission-solver` | none | SMT/SAT |
| `fission-ttd` | `core` | execution snapshots |
| `fission-static` | `analysis-db`, `core`, `cspec`, `loader`, `midend-core`, `pcode-ir`, `signatures`, `sleigh` | static facts (`FactStore`) |
| `fission-emulator` | `loader`, `pcode-ir`, `sleigh`, `solver`, `static`, `ttd` | execution |
| `fission-dynamic` | `core`, `emulator`, `loader`, `plugin`, `sleigh`, `ttd` | debugger backends |
| `fission-decompiler` | `analysis-db`, `analysis-protocol`, `core`, `loader`, `midend-*`, `pcode`, `signatures`, `sleigh`, `static` | decompilation |

Candidates for the shared contracts:

- **`fission-analysis-db`.** It depends on the loader, so every producer of
  evidence would inherit binary parsing. Its contract is also the opposite of
  evidence: an immutable snapshot rebuilt as a whole, keyed by row ids that die
  with it, against append-only claims that must outlive it.
- **`fission-core`.** It has no workspace dependency, but it owns paths,
  configuration and logging, and nearly everything depends on it: a contract
  change would rebuild the workspace and widen an already broad owner.
- **`fission-analysis-protocol`.** It describes command output. Evidence
  identity carries validation and normalization rules, which that crate
  deliberately does not.
- **A new leaf crate.** No workspace dependency, three external ones the
  workspace already builds (`serde`, `serde_json`, `blake3`, plus
  `thiserror`).

## Decision

### 1. Placement

- `fission-evidence` is a new **leaf** crate holding the contracts below. It
  reads no binary, runs no analysis and persists nothing. A test keeps its
  manifest free of workspace dependencies.
- `fission-analysis-kernel` (M1) is a thin orchestration crate above the engine
  facades, with optional backend features. Only surfaces (CLI, scripting) may
  depend on it. Engines never depend on it, and gain no dependency on one
  another to exchange evidence: an engine exposes plain values through its own
  API and the kernel turns them into claims.
- Existing boundaries stand. No canonical decompiler crate depends on
  `fission-dynamic`. `ProgramSnapshot` stays immutable. `fission-project` keeps
  persisting analyst decisions only; evidence is stored apart from it.
- Command output shapes stay in `fission-analysis-protocol`.

### 2. Identity

| Contract | Fields | Rule |
|---|---|---|
| `ImageIdentity` | file content digest (BLAKE3), address space, `SpecContext { language_id, compiler_spec_id }` | path and load address are not identity. The language id carries the architecture |
| `DecodeContext` | SLEIGH context register override, in a canonical text form | the same bytes in another mode are other code |
| `CodeVersion` | length, digest of the bytes | part of every code location |
| `CodeLocation` | image, image-relative offset, version, decode context | survives relocation and snapshot rebuilds; stops applying when bytes or context change |
| `FunctionKey` | image, entry offset, decode context | natural identity. `FunctionId` and other snapshot row ids never leave their snapshot |

The image-relative offset is the address minus the image base the loader
reports.

The compiled `.sla` digest is not yet part of `SpecContext`. A SLEIGH spec
change can alter a lift without altering identity; M3 (dependency
fingerprints) must add it to what derived results depend on.

### 3. Runtime locations

- `RuntimeLocation` = run, process, thread, mapping epoch, virtual address.
  `RunId` is derived by the producer from what was run and observed, with no
  clock or random part.
- `MappingTable` is one process's mapping history. Entries are validated on
  insertion (non-empty, in range, no overlap in a shared epoch), so a lookup
  has at most one answer. Unmapping ends an entry; a remap is a new entry.
- `attribute` turns a runtime address into a `CodeLocation` **only** when a
  mapping live in that epoch shows this image *and* the observed bytes equal
  the image's bytes at the computed offset. Otherwise:
  - bytes that differ, and code in anonymous memory, become
    `RuntimeCodeLocation`, whose name includes the version, so generations of
    a JIT or unpacked buffer stay distinct and are never merged with the file;
  - an unmapped address, unreadable bytes, a range crossing a mapping or a
    mapping of another image stay `Unattributed` with the reason.

Subtracting a base is never sufficient.

### 4. Evidence

A claim (`Evidence`) is immutable: entity, predicate, value, source, scope,
producer and version, dependencies (other claims or artifact records) and
assumptions. Its `EvidenceId` is the digest of those fields.

- **Source** kinds are distinct and never promoted into one another: declared
  metadata, static inference, runtime observation, user assertion, symbolic
  inference, hypothesis.
- **Scope**: a runtime observation holds in a run context and is rejected if it
  claims the whole image. Runtime code and unattributed addresses may appear
  only in claims scoped to their own run.
- **Predicates are existential.** An observed target proves that the edge
  occurred in that scope. No predicate defined here states that a target set is
  complete, so no consumer may prune an edge because evidence lacks it. A
  closed set would need a new predicate with a proof behind it.
- **Lifecycle status** (active, withdrawn with a reason) and **verification
  status** (unchecked, observed agreement, replay validated, proven under
  assumptions, refuted, unknown, unsupported) are two separate axes, recorded
  as appended `StatusEvent`s. Claims are never edited or deleted. No
  confidence number substitutes for either.

`EvidenceLedger` is the append-only collection. Inserting the same claim again
changes nothing, a new run adds claims, serialization is independent of
insertion order, and loading re-derives every id and refuses a file that does
not match. The claim encoding is pinned by a test: changing it renames stored
claims and needs a new ledger schema.

### 5. What M1 adds on these contracts

One slice, opt-in: the emulator reports indirect transfers as plain values to
its existing observer interface (selected at translation time, so a run that
does not ask carries no callback); the kernel records them as a runtime trace,
imports a trace into a ledger, and joins the ledger with the p-code the
decompiler itself lifts. Ordinary decompilation reads no evidence.

## Consequences

- Engines can exchange facts without new edges between them, at the cost of a
  conversion step in the kernel.
- Code locations carry their image identity in full. Stored ledgers repeat it;
  M2, which chooses the persistent store, may intern it.
- Not decided here: the persistent store and concurrent writers (M2),
  contradiction tracking and withdrawal of dependents (M2/M3), path conditions
  as a scope (M4), and dependency fingerprints (M3).
