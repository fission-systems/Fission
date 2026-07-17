# ADR 0011: HIR presentation contract

**Status:** Accepted  
**Last verified:** 2026-07-17

## Context

Fission emits dual pseudocode surfaces from one structured tree:

- **NIR** — semantic-faithful mechanical C (oracle / quality-loop primary)
- **HIR** — human-readable presentation (dashboard readability, operator UX)

Early HIR work lived only as ad-hoc polish in
`crates/fission-pcode/src/render/hir_presentation.rs`. That improved real
O0 rows (`add_ints`, `count_bits`, `apply_binop`, …) but left no durable contract
for:

- what transforms are allowed,
- what must never change (evaluation count, NIR tree, oracle ownership),
- how HIR changes are validated without reusing ADR 0006’s full semantic gate.

[ADR 0006](0006-decompiler-quality-change-gate.md) remains the gate for
builder / materialize / normalize / structuring / type-data **semantic** fixes.
HIR presentation is readability sugar over an already-normalized tree; it needs
a **lighter, different** contract—not a copy of the semantic gate.

## Decision

### 1. Dual-layer ownership

| Surface | Owner | Mutates structured tree? | Default semantic / oracle role |
|---------|--------|---------------------------|--------------------------------|
| NIR print | `render` with `PrintProfile::Nir` on the **unpolished** tree | No | Primary `code` / benchmark semantic input |
| HIR print | `apply_hir_presentation` clone + `PrintProfile::Hir` | Yes, **clone only** | Readability / dual-layer `code_hir` |

Rules:

1. `render_layered_pseudocode` must clone before presentation; the caller’s
   `HirFunction` and the NIR string must not observe presentation side effects.
2. Do **not** add readability-only passes to normalize / structuring “because HIR
   needs it.” Fix structure for real control-flow recovery at those owners; keep
   sugar in `render/hir_presentation` and HIR printer knobs.
3. Do **not** promote HIR text as the semantic oracle or primary ranking input.

### 2. Meaning-preservation boundary

HIR transforms may change **form** only when observational behavior of the
emitted C is intended to match the NIR form under normal evaluation:

**Allowed (presentation-pure or single-eval preserving):**

- single-def pure var alias fold (`param_10 = param_1` homes),
- pure self-update fold (`x = a; x = x + b`),
- `x = e; return x` → `return e` (including call/select; **one** evaluation),
- shared `goto L; L: return e` → direct return,
- limited if-goto / while-goto recovery and `while (1) { if (!c) break; … }` → `while (c)`,
- dead pure flag / pure-intrinsic temp elimination,
- identity / nested same-type cast peel; printer-only widen peel that does not
  change wide-mul intent.

**Forbidden:**

- reordering or duplicating loads/calls with observable effects,
- inlining a multi-use call/load so the call/load runs more than once,
- alias-folding multi-def or loop-carried names,
- dropping stack writes solely because a name is unread when alias is possible,
- peeling sign/width casts that change arithmetic meaning (e.g. required
  `(longlong)a * (longlong)b` widening),
- function-name / address / binary / corpus-row special cases.

One-line contract:

> Evaluation count, order, and observable side effects stay the same; only
> presentation form may change.

### 3. Overfit firewall (shared with ADR 0006 / 0007)

HIR presentation code must not branch on function names, addresses, binary ids,
corpus rows, or compiler tuple labels. Conditions must be structural: def-use
counts, purity, label/goto shape, truthiness equivalence (`!p` vs `p == 0`), etc.

### 4. Pass discipline

- Presentation runs as a **bounded fixed-point** over a documented pass list in
  `apply_hir_presentation`.
- Prefer extending an existing presentation helper over a new end-of-pipeline
  special case.
- New HIR transform ⇒ synthetic invariant test **and** at least one real
  `fission_cli decomp --layer both` check on a motivating PE row when the gap
  was found from a binary.

### 5. Validation matrix (lighter than ADR 0006)

For HIR-only production changes:

1. Targeted test under `hir_presentation` / `render` (invariant, not row name).
2. `cargo nextest run -p fission-pcode` (or focused filter + full crate if
   shared render types change).
3. When motivated by a real binary: host CLI oneshot
   `decomp --layer hir|both --no-warnings` on that address; paste **actual** NIR
   and HIR in the PR—not an expected mockup alone.
4. Confirm NIR string / `code_nir` still carries mechanical form the change did
   not intend to alter.

Full ADR 0006 proposal is **not** required for pure presentation edits. If a
change must alter normalize/structuring to “make HIR nice,” it is a semantic
change and ADR 0006 applies.

### 6. Metrics

- Optional readability proxies may track HIR (`goto` count, `while (1)`, residual
  homes/temps).
- Do **not** retarget semantic case scores or official rankings onto HIR text.

## Consequences

**Positive**

- Clear split: NIR correctness vs HIR readability.
- Reviewers can reject HIR patches that risk double evaluation or oracle swap.
- Real-binary verification is part of the contract after dashboard/mockup
  confusion.

**Accepted costs**

- Some readability wins stay deferred until structural owners improve (e.g.
  irreducible CFG).
- Dual maintenance of pass docs and tests.

**Follow-up**

- `crates/fission-pcode/src/render/AGENTS.md` implements the operator checklist.
- Regression tests enforce single-eval of calls and NIR isolation from
  presentation.

## References

- [ADR 0002](0002-fission-pcode-canonical-semantics.md) — pcode owns semantics
- [ADR 0006](0006-decompiler-quality-change-gate.md) — semantic quality gate
- [ADR 0007](0007-ai-overfit-and-validation-firewall.md) — overfit firewall
- [ADR 0008](0008-nir-substrate-and-owner-boundaries.md) — substrate vs owners
- Repo policy: root `Agents.md` § NIR vs HIR
- Implementation: `crates/fission-pcode/src/render/` (`hir_presentation.rs`, `printer.rs`, …)
- Module boundary: presentation is crate-root `render`, not nested under `nir/` (Phase 1 of dual-layer ownership)
