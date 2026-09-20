# DecompFacts Pipeline Wiring

## Baseline / issue anchor

- Issue: #36
- Reproducer: `render_mlil_preview_with_binary_and_context_output` accepts
  `Option<&mut dyn DecompFacts>` and immediately discards it.
- Current behavior: the normalize and structuring action-pipeline executors
  construct `PassCtx { decomp_facts: None, .. }`, so a caller-owned
  `DecompContext` cannot receive facts from a pass.
- This is a mechanical API-contract repair. No benchmark quality claim is
  made from synthetic tests alone.

## Owner proof

The canonical owner is midend orchestration plus the existing action-pipeline
execution boundaries. `DecompFacts` is already defined in the shared midend
substrate and `PassCtx` already exposes the intended callback slot. The defect
is that `orchestrate.rs`, normalize's pipeline driver, and structuring's
pipeline driver do not carry the reference across their existing boundaries.

## Invariant

When a render caller supplies a mutable `DecompFacts` sink, every normalize or
structuring pass executed for that render receives the same sink through its
`PassCtx`; when no sink is supplied, behavior remains unchanged. The sink is
borrowed and reborrowed, never cloned or stored beyond the render call.

The dual-layer path forwards the sink to the canonical scored render once and
keeps the optional readable second render observation-only, avoiding duplicate
fact publication from two independent structurings.

## Scope and risks

- Extend the existing normalize and structuring pipeline entrypoints with an
  optional `DecompFacts` borrow.
- Preserve the current no-facts wrappers for existing callers and tests.
- Add a focused plumbing test using a recording sink at the action-pipeline
  boundary; do not invent new fact heuristics as part of this change.
- No new dependency, pass, telemetry field, or architecture-specific rule.

## Validation

- Targeted pipeline plumbing tests.
- `cargo nextest run -p fission-pcode`.
- `cargo check -p fission-pcode`.
- `cargo check -p fission-decompiler`.
- `cargo fmt --all --check` and `git diff --check`.
- The change is reported as mechanical unless a real corpus row demonstrates
  a downstream fact-feedback improvement.
