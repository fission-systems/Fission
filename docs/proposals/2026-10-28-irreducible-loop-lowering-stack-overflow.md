# Irreducible back edges seed loops that lower each other without bound

Status: candidate B implemented and measured in this worktree (uncommitted); A not started. Base: main `2bd9720a9`.
This is a robustness and consistency fix. It makes no decompiler-quality claim; the numbers
in section 4 describe how widespread the condition is, not a gain.

## 1. Baseline row anchors (measured)

Both functions abort the decompiler process (`fatal runtime error: stack overflow, aborting`,
exit 134) when decompiled alone, on main, on the #182 candidate and on the discovery-unified
build. Found by the O2 sweep of the rebuilt 2026-10-09 binaries.

| Function | Binary | Address | CFG | Stack needed (default is 32 MB) | Output when it completes |
| --- | --- | --- | --- | --- | --- |
| `echocmd` (bltin/printf.c) | dash O2 | 0x14b50 | 12 blocks, 193 bytes | between 32 and 40 MB | 1,116 characters, 211 p-code ops |
| `main` (src/uniq.c) | coreutils uniq O2 | 0x2720 | 93 blocks | between 64 and 96 MB | 15,428 characters |

`FISSION_RUST_DECOMP_STACK_MB=128` makes both finish with identical time at 128 and 256 MB, so
the recursion is deep but finite; the 8..256 MB clamp in `workers::resolve_decomp_stack_size_bytes`
is the only reason it ever fails.

Build statistics of the small function show the repetition directly:
`loop_while_subgraph_lowered_count` 5,552, `loop_multi_exit_break_count` 11,104,
`promotion_rejected_by_gate_count` 5,552, `rejected_external_entry` 5,552,
`structuring_reason_region_legality_count` 5,552, for a function whose final output has no loop.

Harness consequence: one such function erases every result of its binary unless the harness
isolates it. The sweep tooling now halves the address list on a process crash and records
`process_crashes` in `run.json`; that is measurement tooling, not a repair.

## 2. Owner proof

- [ ] SLEIGH/raw p-code  - [ ] Builder/materialize  - [ ] Normalize
- [x] **Structuring** (loop identification, then loop lowering)  - [ ] Type recovery  - [ ] Printer

Call stack at the fault (symbolized `profiling` build, lldb, top 60 frames), identical shape for both:

```text
try_lower_while(idx=5) -> try_lower_while_impl -> {closure#1} -> lower_loop_body_subgraph
  -> try_lower_while(idx=4) -> try_lower_while_impl -> {closure#1} -> lower_loop_body_subgraph
  -> try_lower_while(idx=5) -> ...                      (dash: heads 5,4,5,4,...)
                                                        (uniq: heads 1,4,1,4,...)
```

Why the cycle exists. `echocmd` has this CFG (successor lists):

```text
0:[1,11] 1:[2,10] 2:[3,6] 3:[4] 4:[5,8] 5:[6,8] 6:[7,9] 7:[5,8] 8:[] 9:[4] 10:[2] 11:[3]
```

Blocks 4,5,6,7,9 form one cycle with **two entries**, 3->4 and 2->6, so it is irreducible. Its two
DFS back edges, 9->4 and 7->5, are both in `irreducible_edges` (the head does not dominate the tail).
`LoopBody::identify_loops` still builds a loop from each of them:

```text
head=4 tails=[9] body=[4,9,6,2,5,1,10,0,7,11,3]     <- contains head 5, and the entry block 0
head=5 tails=[7] body=[5,7,6,2,1,10,0,9,11,3,4]     <- contains head 4, and the entry block 0
```

`lower_loop_body_subgraph(body(4))` tries `try_lower_while` on block 5, whose body contains 4, and so on.
The only thing that stops it is the global work counter
(`structuring_total_work_budget_exceeded`); there is no check that a head is already being lowered
on the current path. The depth reached at the budget decides how much stack is needed.

Ordering fact in `builder/init.rs`: `identify_loops` runs first and the irreducible edges are removed
from the successor/predecessor lists only afterwards, so a `LoopBody` outlives the edge that defined
its tail. `identify_loops` step 1 groups tails from every `EdgeClass::Back` edge without testing
`irreducible_edges`, although `find_base`/`extend` do skip them when walking predecessors.

The two defects are separate:
1. First wrong fact (data): a "loop" whose head does not dominate its body. A natural loop whose head
   is not the entry block cannot contain the entry block.
2. Missing guard (control): re-entering a head that is already being lowered.

## 3. Generality / invariant

```text
A LoopBody is a natural loop: its closing edge is a back edge whose head dominates its tail, and the
head dominates every block of the body. An edge that fails the dominance test is irreducible; it
is a goto edge and seeds no loop. Independently, loop lowering never starts on a head that is
already being lowered on the current call path.
```

Pure CFG/dominance facts; no ISA, name, address, compiler or corpus condition (ADR 0009). The repository
already treats these edges as non-loop edges everywhere downstream (init.rs removes them), so this
removes an inconsistency rather than adding a rule.

Comparable coverage (two programs, one binary each, same optimization level for the anchors; the
condition itself appears at O0 as well):

- dash O2 `echocmd`, coreutils O2 `uniq::main` (anchors).
- Population probe over the sweeps' targets: an irreducible back edge seeds a loop in 2,729 of 26,083
  O2 functions (10.5%) and 235 of 34,165 O0 functions (0.7%); in 1,138 (O2) and 81 (O0) two of the
  loops contain each other; in 2,714 (O2) and 207 (O0) a loop with a non-entry head contains the
  entry block, which no natural loop can.

## 4. How widespread, and what it does not prove

Same size band, structure-perfect rate (GED == 0) of O2 functions with and without a loop seeded by an
irreducible back edge (24,147 scored rows):

| Source CFG nodes | Seeded: rows / GED==0 / mean GED | Not seeded: rows / GED==0 / mean GED | Kuna 1.515 GED==0, seeded vs not |
| --- | --- | --- | --- |
| 1-5 | 447 / 28.2% / 31.0 | 12,004 / 58.1% / 6.8 | 33.6% vs 60.8% |
| 6-10 | 345 / 0.6% / 42.4 | 3,871 / 5.3% / 16.5 | 7.4% vs 10.4% |
| 11-20 | 552 / 0.0% / 57.7 | 3,256 / 0.4% / 27.2 | 1.1% vs 3.1% |

Kuna is also worse on these functions, so part of the difference is that irreducible CFGs are harder.
The gap between Fission and Kuna is larger on the seeded group (1-5: 5.4 vs 2.7 points; 6-10: 6.8 vs 5.1),
which is consistent with, but does not prove, the bogus loops costing Fission something. Not a quality claim.

## 5. Candidate changes and risks

| | Change | Effect | Risk |
| --- | --- | --- | --- |
| B | Track the heads being lowered on the current path; `try_lower_while*` and the subgraph lowering decline (return `None`, falling back to the existing goto form) for a head already on the path | Removes the recursion for every shape, bounded by the number of heads, not by the work budget | Only changes functions that currently recurse into themselves (mutual pairs: 1,138 O2 / 81 O0); their output may change |
| A | `identify_loops` skips back edges in `irreducible_edges`, or restricts bodies to blocks the head dominates | Removes the bogus loops at their source; B becomes defence in depth | Changes the loop structure of every function with an irreducible back edge (2,729 O2 / 235 O0); needs a full measured comparison |
| C | Raise the default decompile stack | Both anchors finish at 128 MB | Rejected: it moves the threshold, costs memory per worker, and leaves the cause |

Recommendation: B first, alone, because it is the smallest invariant-based change that makes the
process unable to overflow on this path; then A as a separate, measured change. Interactions to check:
`lower_loop_body_via_sese` (the `structure_body_members` path), `try_lower_rotated_while`, and
any other caller of `lower_loop_body_subgraph` (lines 999, 1320, 1587, 2167 of `loops.rs`).

Known cases that must not change under B: every function whose loop-lowering never re-enters an
active head. Under A: functions with no irreducible back edge (all but 10.5% of O2 and 0.7% of O0).

## 6. Validation matrix

- [ ] Synthetic CFG test: the 12-block `echocmd` graph (and a two-head mutual-containment graph) lowers
      inside an 8 MB thread without overflow; asserts the active-head guard fires.
- [ ] Invariant test for A (if taken): for every `LoopBody`, the head dominates every body block; no
      non-entry head has the entry block in its body.
- [ ] Both anchors decompile at the default 32 MB stack.
- [ ] `cargo nextest run -p fission-midend-structuring -p fission-pcode -p fission-decompiler`, both profiles.
- [ ] Text comparison on all O0 (34,165) and O2 (26,083) targets against the same build without the change:
      B must change only functions in the mutual-pair set; list every changed function.
- [ ] Score comparison with `rebuild/run_all.sh` / `full_sweep.sh` on the O0 and O2 sweeps and the
      sample-set: Union, structure/type/byte perfect counts, total GED distance next to them,
      zero lost perfect matches; per-row listing of every changed score.
- [ ] No new `process_crashes` entries in `run.json`.

## 7. Evidence on disk (local, untracked)

`benchmark/artifacts/decbench/recovered-union-pipeline/runs/full-inlined-disc3/crash_investigation/`:
lldb stacks for both functions, the ordered CFG/loop dump of `echocmd`, the population probe and its summary.

## 8. Candidate B: implementation and measurement

Change (9 files, uncommitted, branch `investigate/loop-lowering-stack-overflow`): `StructuringHost` gains
`enter_loop_lowering` / `leave_loop_lowering`; `PreviewBuilder` keeps `active_loop_heads: Vec<usize>`;
`lower_loop_body_subgraph` and `lower_loop_body_via_sese` run under `with_active_loop_head`, which declines
(returns `None`, i.e. the existing goto fallback) when the head is already being lowered on the current
path. A new `NirBuildStats.loop_lowering_reentry_declined_count` records each decline.

Tests: `irreducible_two_head_cycle_does_not_lower_loops_into_each_other` builds the 12-block `echocmd` CFG.
Without the guard it fails with `loop lowering repeated 5553 times (re-entries declined: 0)`; with it,
8 lowerings and a non-zero decline count. `cargo nextest run -p fission-midend-core -p fission-midend-structuring
-p fission-pcode -p fission-decompiler`: 1,657 passed, 1 skipped. `cargo fmt --all --check` clean.

Anchors at the default 32 MB stack, release build: both finish (`rc=0`, no overflow).
dash `echocmd` 1,774 bytes, GED 13; coreutils `uniq::main` 18,169 bytes, GED 150.

Sweeps (stripped inputs, same harness; baseline = the same tree without the guard, i.e. the discovery-unified
build `disc3`; O0 on the July binaries, O2 on the rebuilt 2026-10-09 binaries):

| | O0 (34,165 functions) | O2 (26,083 functions) |
| --- | --- | --- |
| Process crashes (harness-isolated) | 0 -> 0 | **2 -> 0** |
| Functions where the guard declined a re-entry | 2 | 32 |
| Functions whose text changed | 7 (1 caused by the guard) | 29 (25 where the guard fired) |
| Union | 14,832 -> 14,832 | 7,774 -> 7,774 |
| Perfect matches lost, any metric | 0 | 0 |
| Type perfect / mean | unchanged | unchanged |
| GED mean on common rows | 12.2809 -> 12.2804 | 25.2845 -> 25.2946 |
| Rows with a different GED | 1 better | 20 rows = 8 distinct functions: 5 worse (+225), 3 better (-140), net +85 |

Reading it:

- The rows that appear only in the new run are not all the guard's: 6 of the 8 newly scored O2 functions and 6 of 7
  changed O0 functions had a 30 s render timeout (`preview_timeout`) in the baseline and none in the new run, with
  zero guard declines in them. Wall-clock timeouts differ between runs (O0 43 -> 37, O2 42 -> 36). That is load
  noise, not an effect of the guard, and is excluded from the reading above.
- The two anchors are the guard's real gain: a crash that erased results is now a scored row.
- On the functions where the guard changes the structure, the result is mixed and slightly negative: the old
  unbounded lowering sometimes produced a better structure before the work budget stopped it (`pkcs12_info`
  203 -> 335, `sys_child_open_for_compress` 26 -> 81), and sometimes a worse one (`readtoken1` 143 -> 48 is better).
  Net +85 distance over 8 distinct functions out of 24,147 scored is not a quality claim in either direction.
- Byte-match values moved on 13 (O0) and 18 (O2) functions whose text is identical; that is the evaluator's
  run-to-run noise (already seen in the 4- vs 8-worker comparison), not an effect.

Conclusion: B removes the crash and the unbounded recursion without losing a perfect match anywhere, and its
score effect is within noise except for 8 functions that moved in both directions. Whether the structure the old
recursion reached is worth keeping, or whether A (no loop from an irreducible back edge) does better, is what the
measured comparison of A should answer. Not done.

Evidence: `runs/full-unopt-guard`, `runs/full-inlined-guard` and `runs/full-inlined-guard/ab_patches/` under the
local pipeline directory (`guard.patch`, `discovery.patch`).
