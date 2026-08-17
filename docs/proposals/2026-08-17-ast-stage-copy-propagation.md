# AST-stage copy propagation and scratch-liveness pruning

## 1. Baseline Row Anchor

- Binary: `vendor/../fission-benchmark/corpus/dev/binaries/c/semantic_stress_clang_O2.exe`
- Function/address: `overlap_move` at `0x140001800`
- Source: `corpus/dev/source/c/semantic_stress.c:83` — a 12-line byte-copy loop.
- Command: release `fission_cli decomp <bin> --layer hir --json --no-header
  --no-warnings --addr 0x140001800`
- Current measured output: 511 lines, 144 local declarations.
- CFG comparison against the original source, scored with fission-benchmark's
  `runner/ged.py` (a port of DecBench's `metrics/ged.py`):

| | source | Fission | angr |
|---|---|---|---|
| CFG nodes | 12 | **1176** | 69 |
| CFG edges | 15 | 1773 | 94 |
| GED | 0 | **2922** | 136 |

- Current driver evidence (`FISSION_PREVIEW_DIAG=1`, `preview_build_stats`):

```text
structuring start: blocks=59 edges=94 force_linear=false
CFG-DIAG start entry=0x140001800 op_count=1360 instr_count=256
preserved_temp_copyprop_skip_count = 126
condition_fold_rejected_side_effect = 74
```

The machine CFG is small (59 blocks) and structuring is admitted with no budget
tripped. The blowup is entirely statement volume inside blocks: clang -O2
vectorised the loop into 256 instructions / 1,360 p-code ops, and copy
propagation then declined to fold 126 of the resulting copies.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize *(diagnosed as the wrong owner — see below)*
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

The 126 skips come from `copy_propagation_pass`
(`recovery/phi_recovery.rs`), which drops a copy when **either** side is a
`TempPreserved` binding:

```rust
copy_map.retain(|name, source| {
    !should_skip_copyprop_for_preserved_name(name, &preserved_temps)
        && !should_skip_copyprop_for_preserved_name(source, &preserved_temps)
    ...
```

`TempPreserved` is set by the builder via `should_preserve_materialized_expr`
(`builder/materialize/same_block.rs`), which returns `true` for everything
except `Var`/`AddressOfGlobal`/`Const`.

**Attacking the marker was measured and rejected.** Narrowing
`should_preserve_materialized_expr` so pure arithmetic is not preserved was
implemented and measured on 464 corpus functions: lines 20,001 → 15,336
(−23%), gotos 38 → 36, `overlap_move` 511 → 303 lines, skips 126 → 4. It was
reverted because it also produced:

- bare compile 363 → 358 (six functions newly broken, all with type errors:
  `array subscript is not an integer`, `invalid operands to binary expression
  ('void *' and 'int')`);
- seven failing invariant tests, every one in the materialisation machinery —
  `no_consumer_materialization_decision_keeps_preserved_rhs`,
  `arithmetic_predicate_proof_marks_low_bit_and_one`,
  `low_bit_mask_predicate_proof_marks_arithmetic_origin`,
  `same_block_replacement_keeps_nonleaf_representatives`,
  `single_consumer_predicate_proof_marks_compare_zero_same_guard`,
  `m32_popcount_loop_carries_add_and_shr`,
  `for_loop_non_last_update_failure`;
- a 334/464 (72%) blast radius.

The marker is load-bearing for predicate proofs and loop-carried recovery.
The owner is therefore not the builder's marking but **where the folding runs**.

Reference comparison — Glaurung (`vendor/glaurung-master/src/ir/copy_prop.rs`,
Apache-2.0, Rust, ranked #3 on DecBench sample-set at 32.8%, ahead of Hex-Rays
29.2% and angr 28.8%):

- Its copy propagation runs **on the structured AST**, not during lifting. Its
  own module doc names the target: *"which inflates the control-flow graph the
  GED metric compares against ground truth."*
- Its soundness argument is positional, not a marker: *"Copies do not cross
  control-flow edges (the active set is cleared at `if`/`while`/`switch`,
  labels, gotos, and calls), so the transform is sound without dataflow
  analysis."*
- It is not one pass with one veto but ~8 narrow transforms, each carrying its
  own proof obligation (`propagate_switch_entry_copies` never enters a loop;
  `propagate_adjacent_promoted_values` rejects every
  `Store { addr: Reg(local_*) }` because adjacency cannot prove whether that
  shape is a frame-slot assignment or a write through a pointer held in the
  slot).
- One pair is decisive for pass ordering: `propagate_adjacent_promoted_values`
  and `propagate_adjacent_typed_promoted_values` are the *same* transform, and
  the second is admissible only once type recovery has proved the destination
  is scalar. Safety is a function of pipeline position.

Fission runs all four `copy_propagation` invocations inside
`normalize_hir_function`, i.e. **before** `run_structuring_pipeline`
(`midend/orchestrate.rs:220-230`). At that point control flow is still
gotos and labels, there is no bounded "linear run" to scope an active set to,
and the pass compensates with global def-counts plus the `TempPreserved` veto.

## 3. Generality / Invariant Proof

Proposed invariant for a new AST-stage group, running after
`run_structuring_pipeline` at the existing `eliminate_redundant_var_assigns`
hook (`orchestrate.rs:236`):

```text
1. The pass runs only on a structured body: control flow is If/While/DoWhile/
   For/Switch nodes plus residual Label/Goto.
2. An active copy set is scoped to one statement list. It is cleared on entry
   to and exit from any nested construct, and at Label, Goto, and any statement
   containing a Call. Nested bodies are processed recursively with their own
   empty set.
3. A copy `x = <pure>` is admitted only when the source is a Var, a Const, or a
   Cast over those, and `x` is not self-referential.
4. `TempPreserved` does NOT veto at this stage. The marker exists to hold a
   materialisation point for builder-stage consumers -- predicate proofs,
   loop-carried initializer seeding, cross-block replacement. Those have all
   run and consumed it before structuring begins.
5. Liveness pruning computes reachability backwards from semantic roots --
   conditions, store addresses and values, call targets and arguments, return
   values, and writes to non-temp bindings -- and retains only temp assignments
   in the transitive dependency closure. A definition whose RHS contains a Call
   is itself a root even if its result is unread.
6. `For` header statements (init/step) are treated as wholly observable; only
   the body is pruned.
7. Any statement form the pass does not model aborts pruning for that function
   rather than deleting dataflow across unknown semantics.
```

Point 4 is the load-bearing claim and it is falsifiable: every test that broke
in the rejected experiment is a builder-stage test. An AST-stage pass that
leaves `should_preserve_materialized_expr` untouched must leave all seven
green. If any of them moves, the claim is wrong and the design is wrong.

Point 5 is the piece Fission cannot express today. Its def-count approach
cannot retire loop-carried residue of the form `a = b; ... b = a`: both names
have a reader even when the graph only feeds itself. Backwards liveness from
semantic roots retires the whole closed graph.

ISA-agnostic check:

- [x] Uses only AST shape, binding origin, and expression purity.
- [x] No binary/function/address/compiler/ISA identity in production logic.
- [x] No runtime or build dependency on `vendor/glaurung-master`.

Required negative cases:

- a copy must not survive across a Label or Goto;
- a copy must not survive a Call;
- a copy must not be carried into or out of a loop body;
- a temp read only by another temp that is itself unread must be retired, and
  a temp reaching a store address must not be;
- a `For` init/step assignment must never be pruned;
- an unmodelled statement form must abort pruning for that function.

## 4. Risk And Ownership Check

- Do not touch `should_preserve_materialized_expr` or any builder-stage
  marking. That path is measured and rejected in section 2.
- Add a new pass group; do not widen the existing pre-structuring
  `copy_propagation_pass`. Its global-def-count contract is what makes it safe
  where it currently runs.
- Fold expressions, not just aliases, only behind a *single-read* proof, and
  recount reads after each pruning round -- a first round routinely retires a
  dead flag chain that was the only thing making a read non-unique.
- Bound the fixpoint (Glaurung uses 8 rounds).
- Type-dependent variants (Fission's analogue of
  `propagate_adjacent_typed_promoted_values`) are out of scope for the first
  slice; land the untyped, positionally-proved core first.

## 5. Validation Matrix

Every slice, in this order, and stop at the first regression:

- [ ] Targeted unit tests per invariant in section 3, each verified to fail
      with the pass stubbed out.
- [ ] `cargo nextest run -p fission-midend-normalize -p fission-pcode` --
      the seven builder-stage tests from section 2 must stay green.
- [ ] `cargo nextest run --workspace --no-fail-fast` -- 2,514 passing, only the
      7 pre-existing `fission-emulator` SLEIGH-decode failures.
- [ ] fission-benchmark corpus, 465 functions, per function: gotos, lines, and
      bare compile via the benchmark's own `runner/bare_compile.py`. Bare
      compile must not regress; today's baseline is 363/465.
- [ ] GED against original source via `runner/ged.py` on the same corpus.
      Today's baseline is 93/368 exact (25.3%), and specifically 11/129 (8.5%)
      at -O2 and 4/36 (11.1%) at -O3.
- [ ] Execution differential (`fission-dir` corpus ground truth) after every
      slice -- this is the tier that catches semantics-preserving claims that
      are not.
- [ ] DecBench sample set: 224/224 binaries, 250/250 functions, gotos, and
      per-file diff. 67% of that corpus is O2, which is what this proposal
      targets.

No claim from aggregate totals alone; per-function comparison only.

## 6. AI Review / Prompt Firewall

- No external model was asked for implementation advice.
- `vendor/glaurung-master` was read to identify owner invariants, soundness
  arguments, and pass ordering. It is Apache-2.0; no code is copied, and
  nothing in the build or at runtime depends on it.
- Production tests describe AST shapes; production code contains no corpus row
  identity.

## Appendix: measured priority

Of the 150 unmatched -O2/-O3 functions in the corpus, the node-count delta has
median 0 and mean +13: ten catastrophic blowups carry the mean, and every one
is this defect. angr on the same functions:

| function | source | Fission | angr |
|---|---|---|---|
| `overlap_move` clang -O2 | 12 | 1176 | 69 |
| `overlap_move` gcc -O3 | 12 | 331 | 37 |
| `reverse_in_place` gcc -O3 | 6 | 187 | 20 |
| `sum_array` gcc -O3 | 4 | 38 | 8 |

Vectorisation is not the obstacle; folding is. A separate group of 41
functions already has the right node count and differs only in statement
content (GED 2-12) -- smaller, and not addressed here.

## 7. Measured outcome

Landed as two passes in `fission-midend-normalize/src/cleanup/`, wired after
`run_structuring_pipeline` in `orchestrate.rs`:

```text
prune_unobservable_scratch      (scratch_liveness.rs)
  -> constant_folding
  -> propagate_copies_in_runs   (run_copy_prop.rs)
  -> constant_folding
  -> propagate_copies_in_runs
  -> prune_unobservable_scratch
```

Measured on 465 corpus functions against the commit before them:

| axis | before | after |
|---|---|---|
| bare compile | 363 | **371 (+8, zero newly broken)** |
| output lines | 20,363 | **15,655 (-23%)** |
| gotos | 38 | 37 |
| `overlap_move` clang -O2 | 511 lines | 318 lines |
| execution differential | 26/26 | 26/26 |
| workspace | — | 2,533 passing |
| the seven builder-stage tests | green | **green** |
| **GED exact structural match** | 93/368 | **93/368 (+0)** |

**Section 3 point 4 held.** The seven builder-stage invariant tests stayed
green with `should_preserve_materialized_expr` untouched, which is what the
rejected experiment could not do. Folding at AST stage rather than at the
marker is the whole difference: same -23% line reduction, but bare compile
went +8 instead of -5.

**The appendix's priority claim was wrong.** It argued the ten catastrophic
blowups carried the mean and were all this defect. The blowups did shrink --
`overlap_move` 511 -> 318 lines, `classify_range` 27 -> 22 CFG nodes -- and
exact structural match moved by zero, at every optimisation level. Fewer nodes
is not the same property as matching the source's nodes. `signum`
gcc-aarch64 -O2 went 5 -> 1 nodes and its GED got *worse*, 8 -> 14: folded
past the source rather than toward it.

Kept anyway, on the other axes: eight more functions compile, output is a
quarter shorter, semantics are unchanged, and nothing regressed. No claim is
made that this moves the leaderboard's structural axis, because it was
measured and it does not.

Three things the design did not anticipate, each caught by measurement:

1. A resolved global symbol is registered with `origin: Temp`. `is_temp_like()`
   alone would have deleted a store to `0x2000` as a dead value; the scratch
   set is now restricted to names `next_temp_name` actually mints
   (`bVar`/`iVar`/`uVar`/`xVar` + digits).
2. A pointer-typed temp carries type information its defining expression does
   not: folding `xVar12: uchar *` into `(ulonglong)xVar12` makes the cast look
   redundant, and dropping it yields `ptr + ptr`. Expression folding is
   restricted to non-pointer temps -- the untyped half of what
   `propagate_adjacent_typed_promoted_values` splits in two.
3. Copy propagation exposes dead stores, exactly as Glaurung's `dead_store_runs`
   documents, and constant folding then exposes new aliases -- hence the second
   propagation pass.
