# A call's result is the value a `ret` after it returns

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Found by `fission_cli verify --tier ground-truth` (#927), confirmed against
the disassembly. Three shapes, one cause:

| function | config | printed | binary |
|---|---|---|---|
| gzip `safe_write` (`0xf254`) | `-O0` | `write(fd, buf, n); return write();` | `call write; leave; ret` |
| gzip `0x125c5` (`xreallocarray`-like) | `-O0` | `if (!n) { n = 1; s = 1; return 1; } ... return reallocarray(..);` | the `n == 0` arm falls into the call |
| bash `xtrace_print_for_command_head` (`0x49b6e`) | `-O0` | `if (xtrace_fp) return xtrace_fp; else return stderr;` | stores the stream, then makes three calls |

In the sample outputs (16 binaries), `return f();` where `f` is also called
with arguments in the same function: **384** sites on main.

Why the output is wrong, without a metric: the first runs `write` twice, the
second skips a call the program makes, the third drops a store and three
calls.

## 2. Owner Proof

- [x] Builder/materialize: `materialize/call_results.rs`
  (`call_result_is_observed`), `control/terminator.rs`
  (`is_epilogue_style_return_join_block`, `lower_return_terminator_impl`).

A raw p-code `Call` has no output varnode, and the `Return`'s input is the
return address. Two consequences, both in the builder:

1. `call_result_is_observed` scans the call's block for a read of the result
   register; a `Return` there reads nothing, so `call f; leave; ret` gave the
   call no binding. Return recovery then read the result register, whose
   reaching definition is the call, and lowered the call a second time
   without its arguments (`lower_call(op, None, ..)`). The successor scan in
   the same function already counts a `Return` as a read when the function
   has primary-return evidence; the same-block scan did not.
2. `is_epilogue_style_return_join_block` decides that a multi-predecessor
   `ret` block is a frame-only epilogue when nothing in it after the last call
   defines the result register -- and the search starts *after* the last
   call. A join holding a call therefore always passed, and the return value
   was taken from the arms: `return 1;` in `0x125c5`, and in bash `0x49b6e`
   both arms became returns that made the join (and its calls) unreachable.
   `predecessor_primary_return_expr` takes the same shortcut on the path
   before it (bash `0x49b6e` again, once the epilogue rule was fixed:
   `return __stream;` after the last call).

## 3. Generality / Invariant Proof

- [x] Not gated on a calling convention or ISA: "a call replaces the ABI
  primary return register" is the calling-convention fact the builder already
  uses in `last_primary_return_def_after_barrier`.
- [x] Synthetic tests state the CFG shape.

Invariant: at a `ret`, the primary return register holds the last call's
result unless an op after that call redefines it. So (a) a `Return` after a
call in the same block observes the call's result when the function returns
a value, and (b) a `ret` block containing a call does not return a value the
predecessors left in the register. The call barrier is now one helper,
`last_call_barrier`, shared by both rules and by
`last_primary_return_def_after_barrier`.

## 4. Risk And Ownership Check

- A void function ending in `call; ret` whose other paths write the result
  register now gets `rax = f(..); return rax;`-shaped output instead of a
  bare call; the evidence rule is the existing one, not a new one.
- No new pass, no new metric.

## 5. Validation Matrix

- [x] Targeted tests in `builder/expr/lower_expr_tests.rs`:
  `a_call_returned_from_its_own_block_is_printed_once`,
  `a_join_that_calls_does_not_return_the_value_an_arm_left_in_the_register`,
  `a_join_that_calls_does_not_return_what_each_arm_left_in_the_register`.
  Positive controls: disabling rule (a) fails the first two; disabling the
  epilogue half of (b) fails the second and third. The predecessor half of
  (b) is not reached by these fixtures; its evidence is bash `0x49b6e`
  (`return __stream;` without it, the call's result with it).
- [x] Crate-level gate: `cargo nextest run -p fission-pcode
  -p fission-decompiler -p fission-midend-normalize`.

### Measured

16 sample binaries, `decomp --all`, main vs. this branch:

| | |
|---|---|
| functions changed | 283 |
| `return f();` with `f` also called with arguments | 384 → 151 |
| functions printing fewer calls (the duplicate gone) | 175 |
| functions printing more calls (a call the join held, restored) | 31 |
| same number of calls (e.g. a `void` function no longer returns an unset `rax`) | 75 |
| fell back to disassembly in one build only (batch time budget) | 2 |

The 151 left are almost all `-O2` (bash 79 + 29, iproute2 18): a tail call
by `jmp` (bash `print_for_command_head`), whose arguments are not recovered
-- a different path, not this one.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`): **perfect
67/233, total distance 7,530 before and after; all 233 rows identical.**
Not a quality claim; GED does not count calls.

`fission_cli verify --all --tier ground-truth` (from #927, built together
with this change) on the same 16 binaries, 14,603 functions, against the
same verifier on main:

| verdict | main | this branch |
|---|---|---|
| equivalent | 4,666 | 4,724 |
| diverged | 1,110 | 1,056 |
| unsupported | 8,397 | 8,393 |
| harness | 430 | 430 |

Transitions: diverged → equivalent 54, unsupported → equivalent 4,
equivalent → anything else **0**. Four functions went unsupported →
diverged: their tree used to not finish (4,096-call budget, an undeclared
variable) and now runs far enough to reach a different, pre-existing defect
-- e.g. bash `0xf9a5b`, a `while (count--)` whose decrement is lost, which
now returns `rl_end_undo_group()` where main returned an unset local. Four
went diverged → unsupported (the `xtrace_print_*` family): the restored
calls include a variadic `fprintf` the interpreter does not model.

## 6. AI Review / Prompt Firewall

No external model was consulted.
