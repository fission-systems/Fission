# Render / HIR presentation guide

Scope: `crates/fission-pcode/src/render/`

Canonical policy:
- [`docs/adr/0011-hir-presentation-contract.md`](../../../docs/adr/0011-hir-presentation-contract.md)
- [`docs/adr/0013-print-expr-vs-dual-layer-printer.md`](../../../docs/adr/0013-print-expr-vs-dual-layer-printer.md) — midend-core diagnostic `format_expr_key` is **not** this surface
- [`docs/adr/0008-nir-substrate-and-owner-boundaries.md`](../../../docs/adr/0008-nir-substrate-and-owner-boundaries.md)

## Layout

```text
render/
├── mod.rs           # owner surface + type bridge from nir
├── layer.rs         # PseudocodeLayer, PrintProfile, LayeredPseudocode
├── printer.rs       # C print (shared walk; profile sugar)
├── presentation/    # HIR-only tree polish (apply_hir_presentation)
│   └── mod.rs
├── layered.rs       # dual-layer print + global/aggregate decls
├── globals.rs       # pre-print global symbol access recovery
└── pipeline.rs      # thin facade re-exporting layered + globals
```

| Path | Role |
|------|------|
| `layer` | Dual-surface contracts |
| `printer` | NIR/HIR text emission |
| `presentation` | Readability-only tree polish before HIR print |
| `layered` | `render_layered_pseudocode` + print-time decls/stubs |
| `globals` | `recover_global_symbol_accesses` (address→name rewrites) |
| `pipeline` | Facade only — prefer importing from `crate::render` |

**Do not rename this module to `hir`.** It owns both NIR and HIR print surfaces.

## Dependency direction

```text
nir (types / builder / normalize / structuring / labels)
        │ consume structured tree + shared sentinels only
        ▼
     render
        presentation ──► printer
        layered ────────► printer
        globals ────────► (tree rewrite, then layered/print)
```

- Prefer `crate::render::…`. Compat alias `crate::nir::render` remains temporarily.
- Shared sentinels live in `nir/labels.rs`.
- Boundary scan: `scripts/check/owner_boundaries.sh`

## Rules (do)

1. Clone before presentation polish; never polish the tree used for NIR print.
2. Keep semantic recovery in normalize/structuring.
3. Structural invariants only (def counts, purity, goto/label shape, truthiness).
4. Preserve single evaluation of calls/loads.
5. Focused tests under `presentation` / `layered` / `globals` for every new transform.
6. Real-binary verification with `fission_cli decomp --layer both` when motivated by a PE row.
7. Dead pure-assign elim must use **whole-function** use counts (never subtree-local).
8. Rely on `presentation/invariants.rs` post-pass firewall; on violation polish is rolled back.

## Rules (don’t)

1. No function/address/binary/corpus special cases.
2. No multi-def alias fold; no multi-use call/load inline.
3. No required widening-cast peel.
4. No semantic oracle / primary benchmark `code` retarget to HIR.
5. No `nir::normalize` / `nir::structuring` imports from render.
6. No nested dead-elim that drops branch defs still live after the if/loop.

## Validation

```bash
scripts/check/owner_boundaries.sh
cargo nextest run -p fission-pcode -- layered globals presentation
cargo nextest run -p fission-pcode
```

Structural firewall codes: `use_without_def`, `call_count_increased`,
`load_count_increased`, `empty_if_shell` (see ADR 0011 §7).
