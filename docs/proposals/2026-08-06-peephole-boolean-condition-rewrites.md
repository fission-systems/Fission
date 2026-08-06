# Decompiler Change Proposal: Peephole Boolean-Condition Rewrites

Date: 2026-08-06

## 1. Context

Third idea ported from angr's decompiler this session, and the smallest/
lowest-risk of the batch surveyed earlier
(`peephole_optimizations/`): pure boolean-expression pattern rewrites, with
no evaluation-order or side-effect implications, so they fit directly into
Fission's existing ADR-0011-constrained presentation layer.

Fission already has exactly this kind of rewrite -- `rewrite_presentation_condition_form`
in `render/presentation/mod.rs`, called from every node of a whole-tree
walk (`canonicalize_conditions_in_expr`) inside the presentation fixed-point
loop. It already did `!!e -> e`, `!(x==0) -> x!=0`, `!(x!=0) -> x==0`, and
const-left comparison commuting. Three more angr rules slot into the same
function as three more `if let` blocks. A fourth angr rule
(`CoalesceSameCascadingIfs`) was skipped -- it operates on angr's own
`ConditionalJump`-with-`ITE`-valued-target AIL shape, which has no
counterpart in Fission's structured `HirStmt::If`.

## 2. The three rules added

- **De Morgan's push**: `!(A && B)` -> `!A || !B`, `!(A || B)` -> `!A && !B`,
  gated on the outer `Not`'s result type being `Bool` (same safety gate the
  existing `!!e` rule already uses, since `!x` for non-bool `x` isn't
  necessarily `x`).
- **Bitwise-or-to-logical-or**: `(a | b) == 0` -> `(a == 0) && (b == 0)`;
  `(a | b) != 0` -> `(a != 0) || (b != 0)`. Turns a flags-mask compare into
  boolean logic. Only fires with the zero constant already on the right
  (the existing const-left-flip rule, earlier in the same function,
  guarantees that ordering by the time this one gets a chance to run).
- **Redundant ITE comparison removal**: `Select(cond, a, b) == a` -> `cond`;
  `== b` -> `!cond` (and negated for `!=`), restricted to `a`/`b` both being
  the *exact constant* being compared against. Constants have no side
  effects, so dropping their (and the select's) evaluation can't change
  observable behavior -- deliberately not generalized beyond constants,
  since that would need a separate purity proof for arbitrary `a`/`b`.

All three preserve evaluation count exactly: each operand appears exactly
once before and after in the first two rules (nothing is duplicated or
dropped), and the third only fires where the dropped operands are provably
side-effect-free constants.

## 3. Verification

- Three new direct expression-level unit tests (bypassing the full
  statement pipeline to avoid interaction with unrelated folding passes),
  each constructing the exact before-shape and asserting the exact
  after-shape: De Morgan's push composing with the existing `!(x==0)` peel
  in the same fixed-point loop, the bitwise-or-to-logical-or split, and both
  directions of the redundant-ITE removal.
- `cargo nextest run -p fission-pcode -p fission-midend-structuring -p fission-midend-normalize -p fission-midend-core -p fission-midend-prehir`:
  1393/1393 passed.
- No panics across all 72 dev-corpus binaries (`--layer hir`).
- Corpus-wide `git`-stash A/B (isolating only this change): all 72 files
  differ (expected -- this is a readability-facing rewrite, unlike the
  prior two structural fixes this session, which showed zero visible
  corpus effect). Confirmed genuine, correct De Morgan's applications
  firing across many real functions, e.g.:
  `!(param_1 != 0 && param_1 < 0 == 0) ? ... : ...` ->
  `(param_1 == 0 || param_1 < 0 != 0) ? ... : ...`, and
  `if (!(zf || xVar18 < 0 != 0))` -> `if (!zf && xVar18 < 0 == 0)` --
  every sampled case verified by hand to be the exact De Morgan's
  transform, not a coincidental match. Line count and
  `goto`/`while`/`return`/`break` counts are identical in every file --
  confirming these are pure boolean-form rewrites, not restructuring. The
  bitwise-or-to-logical-or and redundant-ITE rules didn't happen to fire
  visibly in this specific corpus (no `(a|b)==0`-shaped or
  constant-arm-select-comparison-shaped code present), but are verified
  correct in isolation via the unit tests above.
