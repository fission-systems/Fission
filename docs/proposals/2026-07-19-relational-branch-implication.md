# Decompiler Change Proposal — Relational branch implication folding in conditional_const

Date: 2026-07-19

## 1. Baseline Row Anchor

- Binary: `corpus/dev/binaries/c/control_flow_gcc_O0.exe` (fission-benchmark dev corpus)
- Function: `saturating_add`
- Address: `0x140001660`
- Corpus row or benchmark command: dev smoke run `26cfe8f2` (`results/dev_latest.json`,
  2026-07-17); local re-observe at HEAD via
  `python3 scripts/quality/local_decomp_observe.py baseline --binary … --addr 0x140001660 --label satadd-x64-o0-head`
- Current output summary: semantic 1.0 (5/5 cases) but structuring goto elimination
  tail-duplicates the second guard region into both arms of the first `if`. In the
  duplicated copy dominated by `param_2 > 0`, the branch `param_2 >= 0` is re-tested,
  leaving an unreachable `else` subtree (`xVar36 = -2147483648;` INT_MIN arm) inside
  the positive-`param_2` region. NIR baseline: 61 lines, if_count high, duplicated
  join subtree appears twice.
- Semantic cases passed / total: 5/5 (no semantic regression allowed)
- Failure category: readability / dead duplicated branch (not semantic failure)
- Relevant benchmark/static/readability observations: benchmark row (old build)
  showed goto=6; HEAD structuring removed gotos via duplication, leaving the dead
  re-test as the residual readability defect.

## 2. Owner Proof

- [x] Normalize (`fission-midend-normalize/src/global_opt/conditional_const.rs`)

Evidence:

```c
if (param_2 > 0) {
    if (param_1 <= __gvn_join_12) {
        __gvn_join_10 = local_4;
        if (param_2 >= 0) {          // always true under dominating param_2 > 0
            xVar36 = __gvn_join_10;
        } else {
            ...                       // unreachable INT_MIN arm survives
        }
    } else { xVar36 = 2147483647; }
}
```

`conditional_const` already owns branch-scoped constraint propagation but only
extracts `Eq`/`Ne` constant bindings; it has no relational (`<`, `<=`, `>`, `>=`)
constraint tracking, so a branch condition that is fully decided by a dominating
branch condition on the same variable is never folded. `simplify_empty_and_constant_ifs`
(cleanup) already folds `if (Const)` and runs in `cleanup_conditional_const`
immediately after this pass, so folding the condition to a constant is sufficient.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Within a structured HIR branch scope, each dominating `if` condition of the shape
(var CMP const) with CMP in {SLt, SLe, SGt, SGe, Eq, Ne} constrains the signed
interval of `var` on the then/else paths. A nested `if` condition of the same
shape on the same, unmodified variable whose truth value is fully decided by the
inherited interval folds to Const(1)/Const(0). Writes to the variable invalidate
its interval; loop bodies drop intervals for any variable written inside the
loop (same invalidation discipline the pass already uses for Eq/Ne bindings).
```

ISA-agnostic check (ADR 0009):

- [x] Production condition is a structured-HIR dataflow/dominance fact (branch
      scope nesting), not gated on any ISA/CC enum.
- [x] No ISA-specific data involved.
- [x] Synthetic test states the HIR shape (`if (x > 0) { if (x >= 0) … }`)
      without compiler tuple or function name.

Comparable coverage:

- Similar shape 1: `if (x < c) { … } else { if (x >= c) … }` re-tests after
  duplicated join tails (any O0 short-circuit `&&`/`||` lowering).
- Similar shape 2: `if (x == c) { if (x != c) … }` (already partially covered by
  Eq/Ne env; interval subsumes and extends it).
- Synthetic invariant test: new unit tests in `conditional_const.rs` test module /
  midend tests covering implied-true, implied-false, invalidation-by-write, and
  loop-write invalidation.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `conditional_const`
  (extended in place); `sccp` handles constants only, not relational ranges;
  `simplify_empty_and_constant_ifs` is the existing dead-arm remover.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact (scope-local invalidation, reused
        pattern already in the pass)
- Why extending that owner is sufficient: the pass already walks branch scopes
  with constraint envs and write invalidation; adding a signed-interval env is a
  strict extension, no new pass.
- Possible interaction with existing normalize/structuring passes: folded conds
  feed the existing `cleanup_conditional_const` block (`cleanup_func_stmt_list`
  + `constant_folding_pass`), which removes the dead arm. Conservative guards:
  only `If` conditions fold (never loop conditions); a fold is skipped when the
  discarded branch contains a `Label` (jump target safety).
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none (pass-level `changed` reporting unchanged).
- Known cases that must not change: `signum`, `clamp`, `fibonacci` rows (already
  clean at HEAD); Eq/Ne constant propagation behavior of the same pass.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize -E 'test(conditional)'`
  - Expected signal: new implied-condition tests pass
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode` (+ `-p fission-midend-normalize`)
  - Expected signal: no new failures
- [ ] Focused benchmark row:
  - Command: `local_decomp_observe.py after --session satadd-x64-o0-head`
  - Expected row-level improvement: dead `param_2 >= 0` re-test and unreachable
    INT_MIN arm removed from the `param_2 > 0` region; NIR line count drops;
    no goto/label introduced
- [ ] Smoke or automation sample:
  - Command: re-observe `signum`, `clamp`, `fibonacci` sessions
  - Expected no-regression signal: unchanged output
- [ ] Optional related checks:
  - Command: `cargo check -p fission-decompiler`, `cargo build -p fission-cli --release`
  - Expected signal: clean

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed (readability/dead-branch cleanup; semantic cases must stay 5/5)
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed (extends `conditional_const` in place)
