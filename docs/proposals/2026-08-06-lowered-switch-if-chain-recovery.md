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

## 7. Fourth round: normalize-level flag-value simplification (partial progress on gap 1)

Investigated gap (1) directly: `classify_range` under `gcc -O2` computes each
branch's result via raw x86 EFLAGS arithmetic (`zf`/`sf` materialized into a
0/1 or -1/0 value, e.g. `-(uint)zf`), not a tree of direct comparisons. That
turned out to be three separate, narrower gaps in the PreHir-level
`fission-midend-normalize` crate (upstream of, and unrelated to, the
presentation-layer switch-recovery machinery from rounds 1-3), all now fixed:

1. **`canonicalize_condition_expr`'s `(a - b) != 0 -> a != b` identity was
   condition-only.** It's actually a pure value-level identity (`Sub`/`Xor`
   is zero iff its operands are equal, in *any* consuming context, not just a
   branch test) but was only ever invoked from `normalize_condition_expr`,
   never from the generic bottom-up `normalize_expr` that runs on every
   expression position. Split it out into its own `canonicalize_sub_xor_zero_compare`
   (`arith/flags_cond.rs`) and wired it into `normalize_expr`'s fixed-point
   chain (`pipeline/run.rs`). This alone fixed the `-O2` else-branch:
   `(uint)(param_1 - 10 != 0) + 2` -> `(uint)(param_1 != 10) + 2`.
2. **`Select`'s `cond` wasn't condition-normalized.** `normalize_expr`'s
   `Select` case called plain `normalize_expr(cond)` instead of
   `normalize_condition_expr(cond)`, even though a ternary's `cond` is a
   truthiness test with exactly the same semantics as an `if`/`while`
   condition. Switched it to `normalize_condition_expr`, and additionally
   taught `normalize_condition_expr` to recurse into `&&`/`||`/`!` operands
   (`normalize_condition_logical_operands`) -- those operands are themselves
   always boolean-context truthiness tests (that's what makes them valid
   logical-connective operands), so the same condition-only rewrites are
   equally safe on each one, not just on the connective as a whole. Without
   this, `!zf && (x < 0) == 0` never simplified because `canonicalize_condition_expr`
   only ever looked at the top-level AND node, which doesn't itself match any
   rewrite pattern.
3. **Flag-var reads outside a branch condition were never substituted at
   all.** `flag_recovery.rs`'s reaching-definition walker
   (`recover_in_stmts_with_reaching_defs`) only ever called `recover_in_cond`
   on actual `If`/`While`/`For` conditions; a plain `x = -(uint)zf;` (reading
   a flag as an ordinary value) was invisible to it, and multiply-defined
   flags (like `zf`, redefined per-branch here) don't qualify for the
   separate whole-function single-definition fallback either -- so the flag
   stayed a permanently-opaque `Var` reference. Added `recover_flag_value_use`
   and wired it into every value-producing statement form the walker visits
   (`Assign` RHS and non-`Var` LHS subexpressions, `Expr`, `Return`,
   `Switch`'s scrutinee, `VaStart`), using the same per-branch reaching-def
   map already threaded through for conditions.

Net effect on `classify_range` (`gcc -O2`, `--layer hir`): the `then` branch
went from an opaque `!zf && xVar15 < 0 == 0 ? 1 : xVar16` (with `xVar16`
itself an un-recovered `-(uint)xVar15` flag materialization) to
`xVar15 = uVar11 != 0; xVar16 = -(uint)xVar15; xVar15 = uVar20 <= 0; if (!xVar15) rax = 1;`
-- i.e. every flag is now a direct, correctly-signed comparison
(`uVar11 != 0`, `uVar20 <= 0`) instead of a raw EFLAGS variable. The `else`
branch fully collapsed to `return (uint)((uint)(param_1 != 10) + 2);`.

**Not yet a switch**: the `then` branch is still an `if (!xVar15) rax = 1; return rax;`
default-override shape with some interleaved dead stores (`rax = 0;` etc.)
between the real default assignment and the override -- `conditional_move.rs`'s
"Default-Override" fold requires the assign/if pair to be *exactly* adjacent,
so the leftover dead code in between blocks it. Getting this the rest of the
way to a recovered switch needs either a DCE pass to run first (to make the
pair adjacent) or loosening that fold's adjacency requirement -- left as a
further follow-up rather than attempted this round, since the value here
(untangling raw flag-arithmetic into direct comparisons) is a general
normalize-quality improvement independent of whether this specific function
ever becomes a switch.

**Verification**: full workspace test suite green (only the 7 pre-existing,
unrelated `fission-emulator` failures remain, same baseline as prior rounds);
one existing unit test's expected string updated (`!uVar0 || uVar0 < esi`
instead of `uVar0 == 0 || uVar0 < esi`) since it was pinning the pre-fix
`==0` style that this round's condition-recursion fix now also applies one
level deeper -- a legitimate style consequence, not a masked bug, since the
codebase already applied this exact `==0 -> !x` preference at the top level
everywhere else. Corpus-wide git-stash A/B diff (209 binaries, 11,757
functions): function count identical before/after (no functions gained or
lost), total decompiled line count dropped ~0.8% (more dead flag stores now
prunable once their reads are substituted), 148/209 files changed --
spot-checked several diffs, all copy-propagation/flag-recovery quality
improvements, no operator or value changes. Ran `fission_cli verify --tier
ground-truth` (real-machine-code-oracle equivalence) across every function in
two of the most heavily-diffed binaries (`advanced_patterns_gcc_O0.exe` and
`c/advanced_patterns_clang_O0.exe`): pass/fail sets are
byte-identical before vs. after in both (2 pre-existing divergences in the
first, unrelated to this change and present identically on both sides; 0 in
the second). `classify_range` itself also has a pre-existing ground-truth
divergence on negative/`INT_MAX` inputs (confirmed present identically before
and after this round's changes, so not introduced here) -- a separate,
unrelated bug worth its own investigation, not chased down in this round.

## 8. Fifth round: closing the "not yet a switch" gap from round 4

Directly addressed round 4's "not yet a switch" blocker: generalized
`conditional_move.rs`'s Default-Override fold (`var = default; if (cond) { var
= override; }`) to also match when exactly one single-use predicate-defining
assignment sits between the default and its guarding `if` -- the common
`t = cond_expr; if (t) {...}` / `if (!t) {...}` shape GCC/Clang routinely
produce, which defeated the fold's strict positional adjacency even though
nothing about the underlying shape differs. Safety bar: the predicate's whole-
function use count (via a `DefUseMap` built once per `apply_conditional_move_pass`
call) must be exactly 1, its definition must be pure
(`!expr_has_side_effects`), and only the two unambiguous cond shapes (`pred`
bare or `!pred`) are inlined -- never a compound condition. `classify_range`
(`gcc -O2`) now collapses fully: `return uVar20 > 0 ? 1 : xVar16;` for the
`then` branch, matching the `else` branch's earlier full collapse -- both
branches are now a single `Return(Select(..))`, the exact shape
`recover_switch_from_select_decision_tree` (round 2) already knows how to
turn into a `switch`. (A residual gap remains before that fold can actually
fire: interior dead stores like `uVar11 = param_1;` survive in the branch
body alongside the collapsed return, so `fold_if_else_pure_returns_to_select`
-- which requires each branch to be *exactly* `[Return(expr)]` -- still
declines. That's a normalize-level dead/uninlined-copy cleanup gap, not a
switch-recovery one; flagging as a further follow-up rather than chasing it
this round.)

**Verification**: full workspace test suite green, same 7 pre-existing
`fission-emulator` failures as every prior round, no new failures. Corpus-wide
git-stash A/B diff (209 binaries): function count identical (11,757 before
and after), only 15/209 files changed (much narrower than round 4's 148, as
expected for a positionally-scoped fold generalization), total line count
dropped slightly (more full return-collapses now DCE-eligible). Ran
`fission_cli verify --tier ground-truth` across every function in two x86
binaries that showed this diff (`control_flow_gcc_O1.exe`,
`control_flow_gcc_O2.exe`): zero new divergences in either, and in *both*
binaries one additional function moved from `Unsupported` to `Equivalent`
(the fold's inlining made an unsigned-overflow-checked min/max comparator
simple enough for the verifier's sampler to model) -- net evidence in favor
of correctness, not just neutral. One diff on an aarch64 binary
(`control_flow_gcc-aarch64_O0`, `fde_unencoded_compare`) showed a constant
changing its printed form from `-1` to `4294967295` inside the newly-merged
`Select`; traced this to the transform doing an exact `.clone()` of the
original `PreHirExpr::Const` node (same underlying value, unrelated
downstream type-inference/printer logic evidently renders the same bit
pattern differently once it's a `Select` operand vs. a bare statement RHS) --
ground-truth verification wasn't available for this specific function
(pointer-arg sampling unsupported), so this is reasoned-through rather than
directly proven; flagging as a loose end worth a direct look if it recurs
elsewhere, not a confirmed bug.
