# Decompiler Change Proposal: Recover `switch` from a Lowered If-Chain

Date: 2026-08-06

## 1. Context

Fourth idea ported from angr's decompiler this session, from the switch-
recovery survey: angr's `LoweredSwitchSimplifier` (documented in the SAILR
paper, USENIX 2024) undoes "switch lowering" -- GCC/Clang sometimes compile
a `switch` with too few or too sparse cases to an `if`/`else if` chain
instead of a jump table. Fission's own switch recovery
(`fission-midend-structuring`'s jump-table-based rule) has nothing to key
off when there's no jump table, so this class of switch surfaces as nested
`if`/`else` instead.

angr's version is ~940 lines, operating on its own post-structuring graph
region representation with a stable-hash equality check, multi-shape case
detection (jump tables, cascading ifs, reused blocks), and Linux-only
gating. This is a much narrower, HIR-presentation-layer port: detect a
literal `if (x == c1) {..} else if (x == c2) {..} else if (x == c3) {..}
else {..}` chain (structural equality on `x`, not a stable hash) and rebuild
it as `HirStmt::Switch`.

## 2. The fix

`recover_switch_from_lowered_if_chain` (new, `render/presentation/mod.rs`):
walks the tree looking for an `If` whose condition is `x == c` (or `c ==
x`), then follows a single-statement `else_body` chain as long as each link
is another `If` comparing the *same* `x` (via `HirExpr`'s derived
`PartialEq`) against a distinct constant. Requires:

- `expr_is_presentation_pure(x)` -- a `switch` evaluates its expression
  exactly once; an if-chain evaluates it up to once per untaken arm.
  Collapsing multiple evaluations into one is only safe when repeating or
  dropping them can't be observed (rejects `Load`, general `Call`, etc. --
  the same purity check `fold_if_else_pure_same_var_assign` and friends
  already use for the identical reason).
- At least `MIN_LOWERED_SWITCH_CASES` (3) matched arms -- two reads fine as
  plain `if`/`else if`; the switch only pays for itself once the repeated
  `x == ` noise piles up.
- Distinct case values (a real C `switch` can't have duplicate labels).

The printer already auto-inserts `break;` at the end of any case body that
doesn't end in `Break`/`Return`/`Goto` (`printer.rs:1418-1424`), so case
bodies are just the original `then_body`s moved verbatim -- no new control
flow to get wrong.

**Pass ordering matters here**: placed *before*
`fold_if_else_pure_same_var_assign`/`fold_if_else_pure_returns_to_select` in
the fixed-point loop. Both of those want the same 2-arm `if`/`else` shape a
3+-arm chain also matches one level at a time; letting them run first
collapses the chain into nested ternaries before this pass ever sees it as
a chain (caught during testing: an early version placed after those folds
failed its own positive test for exactly this reason).

## 3. Verification

- Three new tests: a 3-arm chain recovers to a `switch` with the printer
  auto-inserting `break;`s; a 2-arm chain (below threshold) stays
  `if`/`else`; an impure (`Load`-based) 3-arm chain declines to convert.
- `cargo nextest run -p fission-pcode -p fission-midend-structuring -p fission-midend-normalize -p fission-midend-core -p fission-midend-prehir`:
  1396/1396 passed.
- No panics across all 72 dev-corpus binaries (`--layer hir`).
- Corpus-wide `git`-stash A/B: total `switch (` occurrence count is
  **identical** before and after (224 in both), across every one of the 72
  files -- this pass never fired on this corpus at all. Traced why
  directly: the corpus's one real small-case switch,
  `classify_range` in `control_flow.c` (5 cases, sparse values 0/1-3/10 --
  exactly the shape switch-lowering targets), compiles to three genuinely
  different shapes across the tested compilers/opt-levels, **none** of
  which is the linear equality chain this pass recognizes:
  - `gcc -O0`: a binary-search-style nested ternary
    (`param_1 == 10 ? 2 : param_1 > 10 ? ... : ...`), already collapsed by
    Fission's own `fold_if_else_pure_returns_to_select` before this pass's
    turn (it runs first in the pass order specifically to *prevent* that
    for the shapes it does catch, but this particular tree isn't a chain
    of single-`If`-per-`else_body` links, so it was never a candidate --
    reordering wouldn't have helped here).
  - `clang -O0`: still raw `goto`-based range checks
    (`if (!uVar10) goto L1; ...; if (uVar17 < 3) goto L2; ...`), never
    even reached structured `If` form -- `recover_if_else_from_gotos`'s own
    scope doesn't cover this specific multi-target-range-check shape.
  - `gcc -O2`: range comparisons (`param_1 <= 3`) mixed with flag
    computation, not equality comparisons at all.

  None of these are the "clean cascading equality chain" this MVP targets.
  The mechanism itself is verified correct (unit tests, including the
  positive case), but this specific corpus's only switch-lowering candidate
  doesn't exercise it -- a real, honest gap, not a hidden bug.

## 4. Known limitation / follow-up

Catching `classify_range`'s actual shapes would need substantially more
than this round's scope: recognizing binary-search-tree-shaped nested
ternaries/comparisons (not just linear chains), range comparisons
(`x > c`, not just `x == c`), and possibly extending
`recover_if_else_from_gotos` to structure the raw multi-target-goto shape
before any switch-recovery pass could even see it as a candidate. This is
much closer to angr's actual ~940-line scope than the "few days" original
estimate assumed. Flagging as a real, larger follow-up rather than
attempting it in this round.
