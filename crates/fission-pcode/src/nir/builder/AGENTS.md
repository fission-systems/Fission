# Builder Area Guide

Scope: `crates/fission-pcode/src/nir/builder/`

## Role

Lift P-code blocks into HIR under `PreviewBuilder`: control flow, calls, memory, and unsupported surfaces. Telemetry for this stage flows into `NirBuildStats` (`nir/types.rs`); do not invent parallel counters.

## Indirect control surfaces

- `emit_unsupported_control_surface` materializes indirect call/branch/dispatcher surfaces when a target expression exists (or opaque `CallInd`), incrementing `indirect_surface_preserved_count` when the surface is preserved rather than collapsed to a single unsupported stub.
- Refinement of **which** targets are legal belongs with CFG facts (dominance, successor sets) and typed evidence — not ad hoc binary-specific naming.

## Materialization

- Prefer single-definition, dominance-respecting lowering in `materialize.rs` / terminators; large exploratory search must stay bounded and reflected in stats.

## Validation

```bash
cargo test -p fission-pcode
cargo check -p fission-pcode
```
