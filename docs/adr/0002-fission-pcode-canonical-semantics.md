# ADR 0002: `fission-pcode` owns canonical decompiler semantics

**Status:** Accepted  
**Last verified:** 2026-05-02

## Context

Multiple crates consume lifted IR and structuring results. Duplicating normalization rules across CLI, desktop, or printer layers produces drift and brittle snapshots.

## Decision

**`crates/fission-pcode`** owns canonical pcode → NIR/HIR normalization, structuring, and rendering contracts surfaced externally.

Fix incorrect behavior **here**, not in downstream UI layers ([`AGENTS.md`](../../AGENTS.md) core rules).

## Consequences

- Telemetry keyed by [`NirBuildStats`](../../crates/fission-pcode/src/nir/types.rs) is authoritative; automation rolls it up ([`crates/fission-automation/AGENTS.md`](../../crates/fission-automation/AGENTS.md)).
- Printer tweaks that mask semantic bugs are rejected unless accompanied by pcode fixes.

## Rename note (2026-05)

Workspace crate **`fission-decompiler`** (`fission_decompiler`) now hosts application-layer orchestration and Rust-Sleigh glue previously carried by **`fission-decompiler-core`**, while **`fission-pcode`** remains the semantic owner described above. Downstream code should prefer **`fission_decompiler::`** when consuming both IR re-exports and orchestration APIs.
