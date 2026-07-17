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
| `fission_pcode::nir` | **Removed** — call sites use `midend` |

Migration goals met for the alias path:

- External crates (`fission-decompiler`, `fission-static`, bins) use `midend`.
- Prefer `midend` / facade crates in new code.

### 2. Compat cleanup (completed with this decision)

Removed intermediate aliases that no longer had callers:

- `midend::types` → use `midend::ir` (or flat `midend::Hir*` re-exports)
- `midend::render` / `nir::render` → use `crate::render`

### 3. Crate extraction roadmap (phased)

| Phase | Crate | Contents | Status |
|-------|--------|----------|--------|
| A | *(in-tree)* | `midend` rename + public owner modules | **Done** |
| B | `fission-midend-normalize` | Facade re-export of normalize surface | **Expanded** (`normalize_hir_function`, wave stats, API sigs) |
| C | `fission-midend-structuring` | Facade re-export of structuring surface | **Expanded** (owner + shared IR types) |
| D0 | Decouple reverse edges | `wave_stats` at midend root; all callers use `midend::wave_stats`; reverse `is_known_api_signature` via midend root | **Done** |
| D | `fission-midend-core` + move | Shared substrate facade scaffolded; physical `ir`/`action_pipeline`/`wave_stats` move still future | **Scaffolded** |
| E | Drop `fission_pcode::nir` alias | After workspace migration | **Done** |

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

1. ~~Migrate `fission-decompiler` / `fission-static` imports to `midend`.~~ **Done**
2. ~~Remove `pub use midend as nir` when greps are clean.~~ **Done** (Phase E)
3. ~~Scaffold `fission-midend-core` and route facade IR types through it.~~ **Done**
4. **Physical midend-core move:** lift `ir/`, `action_pipeline/`, and
   `wave_stats` out of `fission-pcode` into `fission-midend-core` (break
   pcode→core cycle by inverting ownership; keep builder/vsa in pcode or
   co-extract as needed).
5. Promote normalize/structuring facades from re-export to owned source trees;
   then have decompiler/static depend on facades for owner entrypoints.
