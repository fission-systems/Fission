# ADR 0008: NIR substrate and owner boundaries

**Status:** Accepted
**Last verified:** 2026-07-17

## Context

`fission-pcode` has grown from a p-code/NIR substrate into the place where most
decompiler quality work lands: builder/materialize, normalize, type recovery,
structuring, rendering, telemetry, and tests. Some growth is expected in Rust,
but unbounded growth inside one crate makes fixes harder to review and makes
benchmark-specific patches easier to hide.

The next split should not start with physical crates. The first boundary is an
architectural one: each quality change must enter through a stable owner and
shared analysis surface before code is added.

## Decision

`fission-pcode` remains the crate that owns shipped decompiler semantics, but
its internal role is narrowed:

- **Substrate:** IR/HIR types, p-code-facing lowering contracts, telemetry,
  action-pipeline framework, shared CFG/def-use/type/alias facts.
- **Owner layers:** builder/materialize, normalize, type/data recovery,
  structuring, and render/printer.
- **Future extraction candidates:** `fission-nir-analysis`,
  `fission-nir-normalize`, and `fission-structuring`.

New semantic fixes must prefer reusable analysis or an existing pass registry
over adding one-off logic inside large files. If a fix needs a new pass, helper,
or metric, the proposal must show why the invariant cannot be expressed through
existing dataflow, def-use, type-constraint, calling-convention, CFG, or alias
facts.

Module dependency direction is now part of the design:

- substrate modules may not depend on owner layers;
- builder may use substrate, but must not call normalize, structuring, or render
  policy directly;
- normalize/type recovery may use substrate and action pipeline, but must not
  call builder, structuring, or render policy;
- structuring may use substrate CFG facts and typed HIR, but must not repair
  expression semantics through normalize or builder helpers;
- render/printer consumes structured HIR only and must not perform semantic
  recovery.

**Physical progress (in-crate, not yet multi-crate):**

- Dual-layer print lives at crate-root `src/render/` (not nested under `nir/`).
- HIR polish is `render/presentation/`; shared sentinels in `nir/labels.rs`.
- Structured IR substrate is `nir/ir/` (compat alias `nir::types`).
- Preview entrypoints live in `nir/orchestrate.rs`.
- Smoke scan: `scripts/check/owner_boundaries.sh`.

Existing cross-edges are migration debt, not precedent. They should be retired
by moving shared facts into substrate modules such as CFG, def-use, type
constraints, calling-convention facts, or alias analysis.

## Consequences

- `fission-pcode` can remain one crate while behaving like multiple bounded
  components.
- Future crate splits become mechanical because dependency direction is already
  documented and audited.
- Reviews can reject “just add code in p-code” changes unless they land at the
  correct owner or shared analysis surface.
- Boundary scan reports can track migration debt without blocking unrelated
  semantic fixes.
