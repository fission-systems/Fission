# ADR 0013: Diagnostic `format_expr_key` vs dual-layer printer

**Status:** Accepted  
**Last verified:** 2026-07-17

## Context

Two different “print expression” surfaces exist after midend extraction:

1. **`fission_midend_core::format_expr_key`** (`util/print.rs`, formerly
   `print_expr`) — deterministic, dependency-light stringization of `HirExpr`
   for **keys, ordering, and diagnostics** inside normalize/structuring.
2. **`fission_pcode::render::print_expr`** (`render/printer.rs`) — dual-layer
   **C presentation** (NIR/HIR profiles) governed by
   [ADR 0011](0011-hir-presentation-contract.md).

They must not be confused: a normalize test that asserts C cast syntax belongs
in `render/`, not in midend-core’s diagnostic key formatter.

## Decision

| Surface | Crate | Use for |
|---------|--------|---------|
| `fission_midend_core::format_expr_key` | `fission-midend-core` | Sort keys, GVN/hash tie-breaks, log lines, unit diagnostics |
| `crate::render::print_*` / layered render | `fission-pcode` | Human-readable dual-layer pseudocode, oracle NIR text |

Rules:

1. **Normalize / structuring free functions** may only depend on midend-core’s
   diagnostic key formatter (or pure `Debug`) — never on `render/`.
2. **Presentation quality** (casts, CALLIND opaque form, spacing) is fixed in
   `render/` under ADR 0011, not by changing midend-core `format_expr_key`.
3. When a test needs dual-layer C output, place it under `render/` or
   `midend/tests` that call `print_hir_function` / layered APIs, not midend-core
   `format_expr_key`.

## Consequences

**Positive**

- midend-normalize can stay free of `fission-pcode` / render cycles.
- Diagnostic keys stay stable for deterministic passes.
- Distinct names (`format_expr_key` vs render `print_expr`) reduce accidental mixing.

**Costs**

- Diagnostic strings will not match Ghidra/C pretty-print; that is intentional.

## Follow-ups

- Keep ADR 0011 as the authority for dual-layer presentation.
