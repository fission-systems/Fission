# Fission Architecture

Updated: 2026-04-15

## Ownership

- Semantic owner: `fission-pcode`
- Structuring owner: `fission-pcode::nir::structuring`
- Orchestration owner: `fission-decompiler-core`
- Facts and native preparation owner: `fission-static`
- Printer and postprocess: consume-only

## Decompiler Layers

### `fission-pcode`

`fission-pcode` owns the canonical decompiler semantics:

- P-code to NIR/HIR lowering
- normalization and recovery
- structuring legality
- `StructureGraph`, `RegionProof`, and related telemetry
- `NirBuildStats` and `NirHintStats`

Structuring decisions must be made here. Downstream crates must not reconstruct semantic policy or region legality.

### `fission-decompiler-core`

`fission-decompiler-core` owns application-layer orchestration:

- request/result contracts
- engine selection
- routing between legacy and NIR paths
- type-context assembly from facts
- worker execution and render orchestration
- fallback policy and postprocess sequencing

It consumes canonical semantic policy from `fission-pcode`. It does not redefine legality or quality counters.

### `fission-static`

`fission-static` is a service/provider crate:

- fact extraction and provenance
- native decompiler preparation
- binary-derived static helpers

It does not own decompiler semantics, region legality, or postprocess policy.

## Structuring Migration

The current migration path is dual-engine:

- `LegacyScored`
- `GraphCollapseV1`

`GraphCollapseV1` moves Fission toward a Ghidra-style CFG owner model:

1. Build a `StructureGraph` from the CFG.
2. Produce `RegionProof` values for candidate regions.
3. Collapse only proof-complete and emit-ready regions.
4. Surface final HIR from the collapsed graph.

When legality is incomplete, the structuring layer must preserve explicit unstructured or goto-based output instead of relying on printer-side recovery.

## Benchmark / Telemetry Contract

- Canonical telemetry owner: `NirBuildStats`
- Benchmark/report layers project canonical counters only
- Row regression reasons should be derived from canonical structuring/materialization families, not from downstream heuristics

## Non-Goals

- `fission-cli` and `fission-tauri` are not semantic repair layers.
- `fission-static` should not regain decompiler policy ownership.
- printer or postprocess should not recreate structure when proof is absent.
