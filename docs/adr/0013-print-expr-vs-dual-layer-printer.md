# ADR 0013: Diagnostic `print_expr` vs dual-layer printer

**Status:** Accepted  
**Last verified:** 2026-07-17

## Context

Two different “print expression” surfaces exist after midend extraction:

1. **`fission_midend_core::print_expr`** (`util/print.rs`) — deterministic,
   dependency-light stringization of `HirExpr` for **keys, ordering, and
   diagnostics** inside normalize/structuring.
2. **`fission_pcode::render::print_expr`** (`render/printer.rs`) — dual-layer
   **C presentation** (NIR/HIR profiles) governed by
   [ADR 0011](0011-hir-presentation-contract.md).

They must not be confused: a normalize test that asserts C cast syntax belongs
in `render/`, not in midend-core’s diagnostic printer.

## Decision

| Surface | Crate | Use for |
|---------|--------|---------|
| `fission_midend_core::print_expr` | `fission-midend-core` | Sort keys, GVN/hash tie-breaks, log lines, unit diagnostics |
| `crate::render::print_*` / layered render | `fission-pcode` | Human-readable dual-layer pseudocode, oracle NIR text |

Rules:

1. **Normalize / structuring free functions** may only depend on midend-core’s
   diagnostic printer (or pure `Debug`) — never on `render/`.
2. **Presentation quality** (casts, CALLIND opaque form, spacing) is fixed in
   `render/` under ADR 0011, not by changing midend-core `print_expr`.
3. When a test needs dual-layer C output, place it under `render/` or
   `midend/tests` that call `print_hir_function` / layered APIs, not midend-core
   `print_expr`.

## Consequences

**Positive**

- midend-normalize can stay free of `fission-pcode` / render cycles.
- Diagnostic keys stay stable for deterministic passes.

**Costs**

- Two similar names (`print_expr`) — prefer fully qualified paths in new code.
- Diagnostic strings will not match Ghidra/C pretty-print; that is intentional.

## Follow-ups

- Optionally rename midend-core helper to `format_expr_key` to reduce confusion.
- Keep ADR 0011 as the authority for dual-layer presentation.
