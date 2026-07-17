# ADR 0012: Midend rename and crate extraction roadmap

**Status:** Accepted  
**Last verified:** 2026-07-17

## Context

`fission-pcode`’s post-lift pipeline lived under a module named `nir`, which
collided with the **NIR print surface** (`PrintProfile::Nir` / dual-layer
oracle text). After extracting dual-layer print to crate-root `render/`
([ADR 0011](0011-hir-presentation-contract.md)) and structured IR types to
`midend/ir/`, the remaining rename is the pipeline package itself.

[ADR 0008](0008-nir-substrate-and-owner-boundaries.md) already lists future
extraction candidates (`fission-nir-normalize`, `fission-structuring`). Those
names should track the midend vocabulary.

## Decision

### 1. Module rename: `nir` → `midend`

| Path | Role |
|------|------|
| `crates/fission-pcode/src/midend/` | Post-lift owners (builder, normalize, structuring, ir, orchestrate) |
| `crates/fission-pcode/src/render/` | Dual-layer print / HIR presentation |
| `fission_pcode::midend` | **Preferred** public path |
| `fission_pcode::nir` | **Re-export alias** of `midend` during migration |

Re-export period goals:

- External crates (`fission-decompiler`, `fission-static`, …) keep compiling.
- New code prefers `midend`.
- Alias removal is a follow-up once call sites migrate (tracked in this ADR).

### 2. Compat cleanup (completed with this decision)

Removed intermediate aliases that no longer had callers:

- `midend::types` → use `midend::ir` (or flat `midend::Hir*` re-exports)
- `midend::render` / `nir::render` → use `crate::render`

### 3. Crate extraction roadmap (phased)

| Phase | Crate | Contents | Status |
|-------|--------|----------|--------|
| A | *(in-tree)* | `midend` rename + public owner modules | **Now** |
| B | `fission-midend-normalize` | Facade re-export of normalize surface | **Scaffolded** |
| C | `fission-midend-structuring` | Facade re-export of structuring surface | **Scaffolded** |
| D | Move implementation | Code leaves `fission-pcode` into facades | Future |
| E | Drop `fission_pcode::nir` alias | After workspace migration | Future |

Facade crates **do not** move source yet. They establish stable dependency
names so callers can switch before the heavy code move (ADR 0008: boundaries
before crates).

### 4. What must not change

- Semantic owners and telemetry contract (`NirBuildStats`) stay authoritative.
- Dual-layer oracle primary remains NIR print text.
- No benchmark identity in midend production rules (ADR 0006 / 0007).

## Consequences

**Positive**

- Names match architecture: midend vs print surface vs shared IR substrate.
- Extraction can proceed one owner at a time behind facade crates.

**Costs**

- Dual paths (`midend` / `nir`) until alias removal.
- Facade crates add workspace members without new logic initially.

## Follow-ups

1. Migrate `fission-decompiler` / `fission-static` imports to `midend`.
2. Promote facade crates from re-export to owned source trees.
3. Remove `pub use midend as nir` when greps are clean.
