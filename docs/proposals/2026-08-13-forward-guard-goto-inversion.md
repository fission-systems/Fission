# Forward guard-goto inversion

## 1. Baseline row anchor

- Binary: DecBench sample-set `bin_212.elf`
- Function: `main`, address `0x6de7`
- Command: release `fission_cli decomp` at the address in `nir` and `hir`,
  plus the 224-binary / 250-function sample-set runner
- Aggregate baseline (post terminal-tail duplication, `7e83d848a`):
  NIR 2,030 gotos / 38,747 lines; HIR 1,959 gotos / 35,758 lines
- Failure category: a forward guard stays in jump form when the statements it
  guards are already sitting in the same sequence

Measured excerpt:

```c
if (xVar430) {
    goto block_6ea2;
}
local_64 = 2;
goto block_71a3;
block_6ea2:
```

The span between the guard and `block_6ea2` is exactly "what runs when
`xVar430` is false", but it is expressed as a jump over that span.

## 2. Owner proof

- [x] Structuring

Triage of all 2,030 remaining NIR gotos by target-region shape found 1,337
guards of the form `if (cond) { goto L; }`. Of those, 448 are backward, 769
have a label inside the span (disqualifying — see below), and **154 are
forward with a label-free span**, i.e. directly invertible. 103 of the 154
have spans of ten statements or fewer.

## 3. Generality / invariant proof

Generalized rule:

```text
`if (cond) { goto L; } SPAN; L:` may be rewritten to `if (!cond) { SPAN }`
when L is a forward label in the same statement sequence, SPAN is non-empty,
SPAN declares no label at any nesting depth, and SPAN stays inside the size
bound. The condition is wrapped in a logical negation, never rewritten by
inverting the comparison operator.
```

Two constraints carry the correctness argument:

**No label in the span.** A label inside the span would end up inside the new
`if` body, making every `goto` that reaches it a jump *into* a conditional
block. That is neither expressible in structured form nor semantics-
preserving, so a label at any depth disqualifies the candidate.

**Negation, never operator inversion.** `PreHirBinaryOp` does not distinguish
integer from floating-point comparisons — p-code `IntLess` and `FloatLess`
both lower to `PreHirBinaryOp::Lt` (`midend/support/pcode_util.rs:38`).
Rewriting `!(a < b)` to `a >= b` is valid for integers but **wrong for
floats**, where a NaN operand makes both forms false. The pass therefore
always uses `negate_expr` and accepts the `!(...)` rendering. `negate_expr`
still collapses `!(!x)` to `x`, so the common `if (!rax) goto L` case reads
as `if (rax) { ... }`.

Statements moving into the `if` body keep their meaning: wrapping in a
conditional introduces no loop, so any `break`/`continue` in the span still
binds to the same enclosing loop.

- [x] The rule uses AST shape only.
- [x] No function, address, binary, compiler, mnemonic, or rendered-name guard.
- [x] Synthetic coverage uses hand-built statement trees.

Comparable coverage (unit tests in `cleanup/guard_invert.rs`):

- Motivating shape: forward guard, span absorbed, label pruned.
- `!(!x)` collapse.
- Negative: label in the span (top level, and nested inside a `Block`).
- Negative: backward target.
- Negative: empty span (owned by `eliminate_redundant_gotos`).
- Negative: guard with a non-empty `else`.
- Negative: protected (LSDA landing-pad) label.
- Negative: span over the size bound.
- `break` inside an absorbed span keeps its loop binding.

## 4. Risk and ownership check

- Existing owner: `fission-midend-structuring::cleanup`, next to
  `eliminate_nonfallthrough_label_aliases` and `duplicate_terminal_tails`.
- New pass: no new stage. AST-to-AST on the finalized structured body, so it
  cannot perturb builder type inference, materialization, or naming.
- Label hygiene: inversion can orphan the target label, and structuring's own
  `finalize_structured_body` has already run by this point, so
  `cleanup_redundant_labels_protecting` is invoked afterwards rather than
  shipping a dangling label.
- **Known interaction, measured:** the HIR presentation layer already has a
  *richer* recovery of the same family
  (`render/presentation/mod.rs`, `if_is_single_goto`), which turns
  `if (C) goto Lelse; THEN; goto Lend; Lelse: ELSE; Lend:` into a full
  if/else, gated on the label having exactly one global reference. This pass
  can consume a candidate before that recovery sees it. That is why the HIR
  gain is smaller than NIR's and why one HIR file regresses; see §6.
- Known cases that must not change: `bin_103`'s
  `uint sub_72f1(uint param_1)` type-pollution sentinel.

## 5. Validation matrix

- [x] Targeted unit tests: `cargo nextest run -p fission-midend-structuring
  guard_invert` — 10 passed.
- [x] Crate gates: `cargo nextest run -p fission-midend-structuring
  -p fission-pcode` — 1,149 passed, 1 skipped, no expectation changes needed.
- [x] Ground-truth semantics: `count_bits_matches_real_machine_code` passed.
- [x] Workspace suite: only the 7 pre-existing unrelated `fission-emulator`
  SLEIGH-decode failures.
- [x] Full sample-set rerun, both layers, release CLI.

## 6. Measured result

224/224 binaries and 250/250 functions in both layers:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 2,030 | 1,921 | **-109 (-5.37%)** | 38,747 | 38,570 (**-177**) |
| HIR | 1,959 | 1,909 | **-50 (-2.55%)** | 35,758 | 35,665 (**-93**) |

Unlike tail duplication, this transform shrinks the output on both axes:
the jump and its now-orphaned label both disappear.

Per-file: NIR **51 improved, 0 regressed**; HIR 23 improved, **1 regressed**
(`bin_064`, 2 → 3). Largest NIR gains: `bin_103` -13, `bin_212` -12,
`bin_209`/`bin_039`/`bin_033` -6 each.

Anchor row after:

```c
if (!xVar430) {
    local_64 = 2;
    goto block_71a3;
}
```

### Span bound, measured rather than assumed

`MAX_SPAN_STMTS = 400` (effectively unbounded) was measured against 24:

| Cap | NIR gotos | NIR files regressed | HIR gotos | HIR files regressed |
|---|---:|---:|---:|---:|
| 24 | 1,921 (-109) | 0 | 1,909 (-50) | 1 |
| 400 | 1,917 (-113) | 0 | 1,909 (-50) | **2** |

The larger cap buys four more NIR gotos and costs a second HIR regression
(`bin_084`, 0 → 2), because a larger span is more likely to be the richer
if/else shape the HIR presentation layer recovers better. The tighter bound
is kept so those candidates are left to the owner that handles them properly.

The single remaining HIR regression is the same interaction. It is reported
rather than hidden: the aggregate is -50 HIR gotos across 23 improved files,
and both forms are semantically correct — one simply keeps a jump the other
structures.

## 7. Follow-up slice: full if/else recovery (2026-08-13)

The residual HIR interaction was resolved by *implementing* the richer shape
here rather than declining it. Declining would have handed those candidates
back to HIR (good for HIR, 2 jumps -> 0) but left NIR with 2 jumps -> 2,
a NIR regression. Recovering the shape at PreHIR level is strictly better for
both layers, and leaves HIR's own recovery nothing to race for.

```text
if (C) { goto Lelse; } THEN...; goto Lend; Lelse: ELSE...; Lend:
    becomes
if (C) { ELSE } else { THEN }   Lend:
```

Both jumps retire: the guard's, and the then-arm's jump to the join. `Lelse`
disappears with its single reference; `Lend` stays because other predecessors
may still target it. Admission mirrors the HIR owner's proven conditions --
`Lelse` must have exactly one function-wide reference, and neither THEN nor
ELSE may declare a label -- plus this pass's own recursive label check and
size bound. It is tried before the plain inversion, since it retires two
jumps rather than one.

Reference counts are snapshotted once per function. The rewrites only ever
remove `Goto`s, so a count can go stale high but never low, and a stale-high
count only makes the single-reference test decline -- the safe direction.

Measured against a clean rebuild of the committed parent, 224/224 binaries
and 250/250 functions in both layers:

| Layer | Gotos before | Gotos after | Delta | Files improved | Files regressed |
|---|---:|---:|---:|---:|---:|
| NIR | 1,921 | 1,916 | -5 | 3 | **0** |
| HIR | 1,909 | 1,905 | -4 | 2 | **0** |

An interim measurement appeared to show three NIR regressions; that baseline
was confounded (it had been generated at `MAX_SPAN_STMTS = 400` rather than
the committed 24). Regenerating the baseline from the committed parent showed
zero regressions in either layer. Interim numbers are only comparable against
a baseline built from the same configuration.

Validation: 3 new unit tests (recovery, single-reference requirement,
label-in-else rejection) for 13 total in this module; 1,152 crate tests
passed; `count_bits_matches_real_machine_code` passed.
