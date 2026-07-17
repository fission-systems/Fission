# Render / HIR presentation guide

Scope: `crates/fission-pcode/src/render/`

Canonical policy: [`docs/adr/0011-hir-presentation-contract.md`](../../../docs/adr/0011-hir-presentation-contract.md).

Phase 1 layout (ADR 0008 / 0011): presentation lives at **crate root** `render/`, not under `nir/`. Structured IR types remain in `nir/`; this module only consumes them.

## Layout

| File | Role |
|------|------|
| `layer.rs` | `PseudocodeLayer`, `PrintProfile`, `LayeredPseudocode` |
| `pipeline.rs` | `render_layered_pseudocode` — NIR from raw tree, HIR from **clone** + presentation |
| `hir_presentation.rs` | Readability-only tree polish (`apply_hir_presentation`) |
| `printer.rs` | C print; `PrintProfile::Hir` cast sugar only |

## Dependency direction

```text
nir (builder / normalize / structuring / types)
        │
        ▼  consume structured tree
     render (print + HIR presentation)
```

- Do **not** call normalize/structuring from here.
- `nir` orchestration may call `render` for dual-layer output (crate-local).
- Prefer `crate::render::…`. Compat alias: `crate::nir::render` re-exports this module.

## Rules (do)

1. Clone before `apply_hir_presentation`; never polish the tree used for NIR print.
2. Keep semantic recovery in normalize/structuring; keep sugar here.
3. Prefer structural invariants (def counts, purity, goto/label shape, truthiness).
4. Preserve **single evaluation** of calls/loads when folding `x = e; return x` or selects.
5. Add a focused `hir_presentation` / `layered_*` test for every new transform.
6. When a real binary motivated the change, verify with  
   `fission_cli decomp <bin> --addr … --layer both --no-header --no-warnings`  
   and report **actual** NIR/HIR in the PR.

## Rules (don’t)

1. Don’t branch on function name, address, binary path, or corpus row id.
2. Don’t alias-fold multi-def / loop-carried names.
3. Don’t inline multi-use calls/loads (would re-execute side effects).
4. Don’t drop required widening casts (e.g. wide mul).
5. Don’t retarget semantic oracle / primary benchmark `code` to HIR.
6. Don’t require full ADR 0006 proposal for pure presentation edits—but if you
   must change normalize/structuring for “readability,” ADR 0006 applies.

## Validation checklist

```bash
# Focused
cargo nextest run -p fission-pcode -- hir_presentation layered

# Crate
cargo nextest run -p fission-pcode

# Real surface (example O0 rows)
cargo build -p fission-cli --release
./target/release/fission_cli decomp \
  /path/to/advanced_patterns_gcc_O0.exe --addr 0x140001532 \
  --layer both --no-header --no-warnings
```

## Pass list owner

`apply_hir_presentation` documents the fixed-point order. Extend an existing
helper before inventing a parallel end pass.
