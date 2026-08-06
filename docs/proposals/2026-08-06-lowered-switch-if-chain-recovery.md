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

## 4. Follow-up round: binary-search decision trees

Added `recover_switch_from_select_decision_tree`, targeting exactly the
`gcc -O0` shape found above: a `Select` tree already collapsed by
`fold_if_else_pure_returns_to_select`, where GCC's binary-search-style
lowering mixes equality splits (real case boundaries) with range splits
(`x > c`, pure search-tree narrowing, no case value of their own).

`classify_direct_split` only ever recurses into a `Select` as "still inside
the decision tree" when its condition is a **direct** comparison of the
switch variable against a constant (`x CMP c`, or the bare zero-check idiom
`!x`) -- never through arithmetic on the variable (`x - 1 > 2`), and never
into a condition on an unrelated variable. Anything else is treated as an
opaque leaf, full stop, even if it happens to itself be a `Select` -- this
is the load-bearing safety property: a case body's own unrelated ternary
must never be mistaken for another tree split and decomposed. Every leaf the
walk bottoms out at that isn't a matched `==` case is required to be
*structurally identical* (`HirExpr`'s derived `PartialEq`) to every other
such leaf before accepting a single `default:` -- these compiled trees
routinely reach the same default value through several differently-shaped
leaves (see `classify_range`'s three separate `param_1 < 0 ? -1 : 3`
occurrences below), so "first default leaf found" would be a guess, not a
proof.

**Verification**: three new tests (a 3-case binary-search tree recovers
correctly; inconsistent default leaves correctly decline; an unrelated
ternary living inside a case's own result value survives untouched, proving
the leaf/split boundary is drawn correctly) -- all against hand-built
`Select` trees, since constructing a genuinely ambiguous or corruptible
case through the full compile pipeline isn't practical. `cargo nextest run`
across the same five crates: 1399/1399 passed. No panics across the full
corpus. Corpus-wide A/B: **still 224 total `switch (` occurrences, no
change** -- re-checked `classify_range` directly and it still doesn't
convert, because its case-1/2/3 split uses a *biased* comparison
(`param_1 - 1 > 2`, not a direct `param_1 CMP const`), which
`classify_direct_split` deliberately declines to walk through: proving an
arithmetic transform like `x - k` is injective and order-preserving over
the relevant domain (so the implied case-value set can be safely derived)
is a meaningfully harder, higher-stakes correctness problem than anything
attempted so far in this feature, and getting it wrong would silently
produce a wrong case boundary rather than just missing an optimization.

## 5. Third round: the biased-subtraction range check, resolved

Gap (1) above -- `classify_range`'s actual case-1/2/3 blocker,
`param_1 - 1 > 2` -- turned out tractable with a real correctness argument,
not just a heuristic. Confirmed directly against the corpus's own NIR
(`--layer nir`) that the compiled comparison is genuinely **unsigned**:
`uVar36 = param_1; uVar36--; if (2 < uVar36) goto default;` (Fission's own
`uVar` naming already reflects `NirType::Int { signed: false }`, and the op
is the unsigned `Gt`, not `SGt`). This is GCC/Clang's standard "bias and
compare" range-check idiom -- `(unsigned)(x - lo) <= (hi - lo)` for
`lo <= x <= hi`, folding the lower-bound check into the upper-bound one via
deliberate unsigned wraparound when `x < lo`. Subtracting a constant is an
exact bijection over any fixed-width integer domain, so once the comparison
is confirmed unsigned, `(x - bias) CMP bound` translates to an *exact*
`[range_lo, range_hi]` on `x` -- not an approximation.

Added `DirectSplitOnVar::Range` to `classify_direct_split`, gated tightly:
only fires for an *explicit* `Sub` on the switch variable (a bare
`x CMP const` stays on the existing `NotEq` path, which recurses
unconditionally into both arms and was already correct for that shape --
see the bug below for why conflating the two paths is unsafe), only for the
four genuinely unsigned ops (`Gt`/`Ge`/`Lt`/`Le`, never the signed
variants), and capped at `MAX_RANGE_SPLIT_EXPANSION` (64) values to avoid
flooding an unrelated large bounds check into a wall of case labels. The
in-range arm only expands into per-value cases when it resolves to one flat
leaf with no further splits of its own (verified the same way
`collect_select_tree_leaves` already required for range-in-general);
otherwise the whole split is declined, not guessed at. Case values that
end up sharing one result expression (the normal outcome of a range
expansion) are now grouped into one `HirSwitchCase { values: [1, 2, 3], .. }`
rather than three separate `case 1: .. break; case 2: .. break;` blocks.

**A second bug found and fixed while wiring this in**: the existing
`NotEq` handling recursed into *both* arms of every non-equality split
unconditionally, which had been fine when `NotEq` only ever led to further
splits or flat constants -- but `classify_range`'s conditional default,
`param_1 < 0 ? -1 : 3`, is itself a `NotEq`-shaped split (`SLt`) that
recurring unconditionally shattered into two separately-tracked leaves
(`-1` and `3`), which the "every default leaf must be identical" check then
correctly (but unhelpfully) rejected as inconsistent -- silently declining
the whole tree. Fixed with `subtree_has_real_case_boundary`: only descend
into a `NotEq`/`Range` arm if it actually leads to a real case boundary
(an `==` split, or a `Range` split whose in-range arm is a flat leaf)
somewhere inside; an arm with no real boundary is the default's own
internal logic and must stay intact as one leaf, exactly like the
already-existing "unrelated case-body ternary" protection.

**Verification**: a new test reproduces `classify_range`'s decision tree
node-for-node (same range-check shape, same four-times-repeated
`x < 0 ? -1 : 3` default, same nesting) and asserts full recovery.
`cargo nextest run` across the same five crates: 1400/1400 passed. No
panics across the full corpus. Corpus-wide A/B: **exactly +1** total
`switch (` occurrence (224 → 225), isolated to precisely one file
(`control_flow_gcc_O0.txt`) -- confirmed directly:

```c
int classify_range(int param_1)
{
    switch (param_1) {
        case 10:
            return 2;
        case 0:
            return 0;
        case 1:
        case 2:
        case 3:
            return 1;
        default:
            return param_1 < 0 ? -1 : 3;
    }
}
```

Semantically identical to the original source (case order differs, which
doesn't matter for a `switch`). Every other file in the corpus shows *zero*
change in `switch`/`goto`/`return`/`break` counts -- the fix is precisely
targeted, not incidentally touching anything else. `return` count in the
one changed file moved from 106 to 109 (+3), exactly matching one
`return <nested ternary>;` being replaced by four `return`s (one per case
arm + default) -- the expected, correct shape of this exact restructuring.

## 6. Remaining known limitations

Two gaps remain, both larger, separate undertakings:

1. **Range comparisons without any equality splits** (`gcc -O2`'s shape for
   the same function: `param_1 <= 3` mixed with flag computation, no bare
   `==` case boundaries at all) -- a different codegen strategy this pass's
   "equality split = case, range split = grouped cases" model doesn't cover
   on its own (though the underlying `Range` machinery built this round is
   at least a plausible foundation for it).
2. **Raw, unstructured multi-target `goto` range checks** (`clang -O0`'s
   shape) -- never even reaches structured `If`/`Select` form, so no
   presentation-layer pass can see it; would need `recover_if_else_from_gotos`
   itself extended, a materially different and larger undertaking given
   that pass's own scope and this session's earlier experience with how
   easily adjacent-pass changes in this exact fixed-point loop can misfire
   (see `docs/proposals/2026-08-06-cross-block-merge-binding-scalar-ssa-fallback.md`'s
   two-iteration debugging story for the general shape of that risk).

This feature is now well past the "few days" original estimate and close
to angr's actual ~940-line scope, but has landed a real, verified,
corpus-confirmed win. Flagging (1) and (2) as follow-ups rather than
attempting them in this round.
