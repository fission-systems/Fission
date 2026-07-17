# Render / HIR presentation guide

Scope: `crates/fission-pcode/src/render/`

Canonical policy:
- [`docs/adr/0011-hir-presentation-contract.md`](../../../docs/adr/0011-hir-presentation-contract.md)
- [`docs/adr/0008-nir-substrate-and-owner-boundaries.md`](../../../docs/adr/0008-nir-substrate-and-owner-boundaries.md)

## Layout

```text
render/
├── mod.rs           # owner surface + type bridge from nir
├── layer.rs         # PseudocodeLayer, PrintProfile, LayeredPseudocode
├── printer.rs       # C print (shared walk; profile sugar)
├── presentation/    # HIR-only tree polish (apply_hir_presentation)
│   └── mod.rs
└── pipeline.rs      # layered render + global-name recovery helpers
```

| Path | Role |
|------|------|
| `layer` | Dual-surface contracts |
| `printer` | NIR/HIR text emission |
| `presentation` | Readability-only tree polish before HIR print |
| `pipeline` | `render_layered_pseudocode` orchestration |

**Do not rename this module to `hir`.** It owns both NIR and HIR print surfaces.

## Dependency direction

```text
nir (types / builder / normalize / structuring / labels)
        │ consume structured tree + shared sentinels only
        ▼
     render
```

- Prefer `crate::render::…`. Compat alias `crate::nir::render` remains temporarily.
- Shared sentinels (e.g. switch fallthrough) live in `nir/labels.rs`, not duplicated here.
- Boundary scan: `scripts/check/owner_boundaries.sh`

## Rules (do)

1. Clone before presentation polish; never polish the tree used for NIR print.
2. Keep semantic recovery in normalize/structuring.
3. Structural invariants only (def counts, purity, goto/label shape, truthiness).
4. Preserve single evaluation of calls/loads.
5. Focused tests under `presentation` / `pipeline` for every new transform.
6. Real-binary verification with `fission_cli decomp --layer both` when motivated by a PE row.

## Rules (don’t)

1. No function/address/binary/corpus special cases.
2. No multi-def alias fold; no multi-use call/load inline.
3. No required widening-cast peel.
4. No semantic oracle / primary benchmark `code` retarget to HIR.
5. No `nir::normalize` / `nir::structuring` imports from render.

## Validation

```bash
scripts/check/owner_boundaries.sh
cargo nextest run -p fission-pcode -- presentation layered hir_presentation
cargo nextest run -p fission-pcode
```
