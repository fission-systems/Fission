# NIR Structuring Guide

Generated: 2026-03-27
Scope: `crates/fission-pcode/src/midend/structuring/`

## Ownership (ADR 0012)

Free-function owners live in `fission-midend-structuring` and take
`&mut impl StructuringHost`. Production host is `PreviewBuilder`
(`host_impl.rs`).

| Area | Owner crate | Notes |
|------|-------------|-------|
| CFG facts, cleanup, regions, graph, admission | `fission-midend-structuring` | pure / host-free where possible |
| Collapse-loop edge virtualization | `fission-midend-structuring::collapse_loop` | host free fns |
| Conditionals (`try_lower_if*`, short-circuit) | `fission-midend-structuring::conditionals` | host free fns; thin wrappers here |
| Loops (`try_lower_while/for/…`, subgraph) | `fission-midend-structuring::loops` | host free fns; thin wrappers here |
| Switch (`try_lower_switch`, compare-chain) | `fission-midend-structuring::switch` | host free fns; thin wrappers here |
| LinearExit / LoweredTerminator / budget / outcomes | `fission-midend-structuring::linear_types` | re-exported via support |
| Region linear recovery | `fission-midend-structuring::linear_recovery` | free fns |
| Linear body (`lower_linear_body*`, exits, cond tails) | `fission-midend-structuring::linear_body` | free fns; cache via host |
| Multiblock linear fallback | `fission-midend-structuring::linear_multiblock` | free |
| P-code trivial opcode tables | residual on `PreviewBuilder` | `PcodeOpcode`/`Varnode` host residual |
| Guarded-tail pure HIR rewrites | `fission-midend-structuring::guarded_tail_pure` | free fns |
| Guarded-tail types + promote entry | `fission-midend-structuring::guarded_tail` | free promote/discover |
| Guarded-tail pure HIR (`pure_hir`) | `fission-midend-structuring::guarded_tail::pure_hir` | free pure helpers |
| Guarded-tail canonicalize/execute bodies | `fission-midend-structuring::guarded_tail::bodies` | free fns + residual host hooks |
| Guarded-tail telemetry mark_* | residual on `PreviewBuilder` | `StructuringTelemetry` bumps only |
| Driver admission / region scaffold pure helpers | `fission-midend-structuring::driver_pure` | free fns |
| Collapse-rule dispatch (SESE tier-1) | `fission-midend-structuring::sese_driver` | free `apply_collapse_rule` |
| SESE collapse loop + final scan | `fission-midend-structuring::sese_driver` | free `build_sese_region_body` |
| SESE discovery / tree structure | `fission-midend-structuring::sese_discovery` | free `structure_cfg_via_sese` |
| Orphan-goto repair | `fission-midend-structuring::orphan_repair` | free `try_repair_orphan_gotos` |
| Guarded-tail suffix pure + with-diag + callee pure | `fission-midend-structuring::guarded_tail::{pure_hir,suffix_window}` | free; host residual supplies call-effect / binary facts |
| Host data adapters | `host_impl.rs` | binary / type_context / inventory bridges only |
| SESE / collapse entry | call `fission_midend_structuring::{structure_cfg_via_sese,build_sese_region_body}` | do not add new pcode thin wraps |
| Graph types | `fission_midend_structuring::graph` | re-exported from `structuring/mod.rs` |

Prefer new work as `fn try_lower_*(host: &mut impl StructuringHost, ...)`.

Orchestration (`midend/orchestrate.rs`) calls **`fission_midend_normalize`**
directly for normalize; do not reintroduce `crate::midend::normalize::*` for
new stage wiring.

## Overview

This directory owns CFG-based reconstruction from flattened NIR/HIR into structured control flow. It is the main algorithmic hotspot for decompiler quality work.

## Structure

```text
structuring/
├── driver.rs        # Discovery / promotion orchestration
├── guards.rs        # Guarded-tail canonicalization + promotion logic
├── linear.rs        # Linear body lowering
├── loops.rs         # While / do-while / loop control rewrites
├── switch.rs        # Switch recovery
├── cfg_analysis/    # Dom/postdom/SCC/edge facts (mod, dom, postdom, scc, edge, util)
├── cleanup.rs       # Label/layout cleanup
└── conditionals/    # Plain if / if-else / short-circuit lowering
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Guarded-tail discovery / promotion | `guards.rs`, `driver.rs` | Biggest rejection buckets live here |
| Shape facts | `cfg_analysis/` | Prefer these over lexical shortcuts |
| Conditional lowering | `conditionals/` | Shared follow / plain-if / short-circuit |
| Loop normalization | `loops.rs` | Break/continue rewriting and reducers |
| Layout cleanup | `cleanup.rs` | Canonical labels before discovery |

## Conventions

- Prefer common-follow / next-flow / dom/postdom invariants over lexical position.
- Add both positive and negative regressions for any acceptance change.
- If a new rejection bucket appears repeatedly, subtype it before broadening acceptance.
- Keep behavior deterministic; metrics and snapshots depend on stable output.
- Structuring may consume substrate CFG facts and typed HIR only. If a fix needs builder or normalize knowledge, move the reusable fact into substrate instead of adding a direct dependency.

## Anti-Patterns

- Do not “fix” a guarded-tail failure by forcing a prettier printer shape.
- Do not relax nested/nonlocal/loop/switch cases without explicit structural proof.
- Do not broaden acceptance before measuring on 200/500-function automation samples.
- Do not patch expression semantics inside structuring. Fix the owner that produced the expression or add a shared analysis fact structuring can consume.

## Validation

```bash
cargo test -p fission-pcode structuring_candidate_discovery_ -- --nocapture
cargo test -p fission-pcode structuring_ -- --nocapture
cargo test -p fission-pcode
cargo check -p fission-pcode
```
