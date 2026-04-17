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

## Structuring Model

The active structuring path is a hard-cutover Ghidra-style CFG owner model.

- `StructureGraph` is the internal collapsed overlay owner.
- `CollapseDriver` applies deterministic collapse rules.
- `RegionProof` and rewrite execution decide whether a region may be promoted.
- `linear` is an explicit fallback surface, not a late semantic repair layer.

The implementation still parses legacy engine names for compatibility, but active execution resolves to the graph/collapse path.

The active rule flow is:

1. Build a `StructureGraph` from CFG/basic-block facts.
2. Produce `RegionProof` and replacement/readiness evidence for candidate regions.
3. Collapse only proof-complete, replacement-complete, emit-ready regions.
4. Surface final HIR from the collapsed graph.
5. Fall back to explicit unstructured or goto-based output when legality is incomplete.

Printer and postprocess must not reconstruct structure after this point.

## Benchmark / Telemetry Contract

- Canonical telemetry owner: `NirBuildStats`
- Benchmark/report layers project canonical counters only
- Row regression reasons should be derived from canonical structuring/materialization families, not from downstream heuristics

## Non-Goals

- `fission-cli` and `fission-tauri` are not semantic repair layers.
- `fission-static` should not regain decompiler policy ownership.
- printer or postprocess should not recreate structure when proof is absent.
