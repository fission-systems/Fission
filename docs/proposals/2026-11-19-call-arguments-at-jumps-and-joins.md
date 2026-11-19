# A call's arguments are the registers it reads where it is made

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Found with the printed-tree verifier (`fission_cli verify --tier
ground-truth`, #927) and the double-call count from #929's sweep. Every row
below is a call printed with arguments the machine does not pass, or a call
the machine does not make:

| function | config | printed | machine |
|---|---|---|---|
| bash `check_dev_tty` (`0x41fa0`) | `-O2` | `return sub_30e20();` | `mov edi, eax; jmp` -- one argument |
| bash `check_dev_tty` | `-O2` | `rax = open();` | `rdi` set a block earlier, `esi = 0x802` |
| bash `print_select_command_head` (`0x46150`) | `-O2` | `... return sub_44ad0();` | no third call; the function is void |
| bash `sh_unset_nodelay_mode` (`0x41ec0`) | `-O2` | `fcntl(param_1)` | `fcntl(fd, 3, 0)`; the one "argument" was `push rbp` |
| bash `0x49cc0` | `-O2` | `return sub_30e20();` twice | `xor edi, edi; jmp` / `mov edi, ebp; jmp` after `dup2(fd, 0)` |
| bash `check_dev_tty`, after the `open` fix | `-O2` | `open(0, 2050)` | `open(ttyname(..), 2050)` |
| dash `0xe530` | `-O2` | `mempcpy()` / `mempcpy(param_3, ..)` | `mov rdi, rdx` a block earlier / `mov rdi, rbx; pop rbx; jmp` with `rbx` recomputed |

Why each is wrong, without a metric: a call with an argument missing or
invented passes the callee something else; a call the machine never makes
runs the callee again; `open(0, ..)` opens a different path.

## 2. Owner Proof

All in the builder's call-argument recovery, except the last, which is in
normalize:

- [x] `control/terminator.rs`, `recover_tail_call_expr_from_target_expr`:
  arguments of `jmp f` were recovered only when the *whole function* had two
  blocks or fewer. (`recover_known_external_tail_call_expr` had already
  dropped the same gate.)
- [x] `control/terminator.rs`, `lower_return_terminator`: when no evidence
  gives the function a return value, calls are emitted without a result
  binding; the epilogue-join path then lowered the result register's
  reaching definition -- the call -- a second time.
- [x] `calls/call_recovery.rs`, `exact_call_register_arity`: only
  `locked_exact_arity` proved that the callee reads its leading slots; a
  variadic declaration's named parameters are as certain.
- [x] `calls/call_recovery.rs`, `entry_register_arg_at_exact_call`: "no
  single reaching definition" was read as "never written", so a slot written
  differently on each arm of a join printed the caller's parameter.
- [x] `calls/call_recovery.rs`, `recover_call_stack_args_from_block`: a
  prologue `push` of a callee-saved register lands where a first stack
  argument would.
- [x] `calls/call_recovery.rs`, `call_arg_carrier_assignments`: carriers
  written before an earlier call in the same block were counted, although
  that call clobbered them.
- [x] `calls/call_recovery.rs`, tail-call "prefer source": the first input of
  *any* definition was taken as its source (`xor edi, edi` gave `edi`); the
  source was named by its entry alias even after being written, or lowered
  at the jump, after `pop rbx` had restored the caller's value.
- [x] `calls/call_recovery.rs`, `exact_call_register_arity`: a declaration
  was looked up for `call` but not for `jmp f`.
- [x] Normalize, `cleanup/temp_var.rs` (`inline_single_use_temps`,
  `collapse_adjacent_pure_copy_into_if`): an `if` is an allowed inline target
  when the temp is used anywhere in it, but only the statements *before* the
  `if` were checked for writes to the copy's inputs, not those ahead of the
  use inside the arm. The builder's PreHIR was right (`xVar18 = rax; if (rax)
  { rax = 0; open(xVar18) }`); this pass made it `open(0)`. The new `open`
  arguments exposed it.

## 3. Generality / Invariant Proof

- [x] Calling-convention slots come from the ABI model; nothing is gated on
  an ISA, a mnemonic or a function.

Invariants:

1. A tail call's arguments are staged at the jump or in the single
   predecessor that falls into it -- a property of the call site.
2. Every call instruction is emitted once; a `return` never re-lowers one.
   A tail call's origin is a jump, so `return f(x)` from `jmp f` stays.
3. A declaration proves the named slots it reads, variadic or not, and
   whether it is reached by `call` or `jmp`.
4. A slot holds its entry value at a call only if no path from the entry to
   the call writes it (through any view) or makes a call.
5. A push of a preserved register still holding its entry value is a save,
   not an argument.
6. Registers written before an earlier call are not arguments of a later one.
7. Only a copy has a source to prefer, and the source is read where the copy
   reads it.
8. A use in an `if`'s arm reads its inputs after the statements ahead of it
   in that arm; a write there (or a store, for a value that reads memory)
   keeps the copy. Loops, switches and labels in the arm are refused outright.

## 4. Risk And Ownership Check

- More arguments print where recovery used to give up. Each comes from a
  write the callee reads; where the evidence is a join with no single value,
  the slot stays empty rather than guessing.
- `return;` where a value-less function printed `return f();`. The decision
  that the function returns nothing is the existing one (calls were already
  emitted without a result for lack of return evidence).

## 5. Validation Matrix

- [x] Targeted tests (each fails with only its fix reverted):
  `a_jump_to_a_function_keeps_its_arguments_in_a_branching_function` (1),
  `a_return_after_a_call_in_another_block_does_not_call_again` (2),
  `variadic_named_parameters_recover_a_dominating_predecessor_register_slot` (3),
  `exact_call_arity_does_not_read_the_entry_value_of_a_register_written_on_every_path` (4),
  `a_callee_saved_push_is_not_a_stack_argument` (5),
  `an_argument_written_before_an_earlier_call_is_not_passed_to_a_later_one` (6),
  `a_jump_after_zeroing_an_argument_register_passes_zero` (7, both halves),
  `a_jump_passes_a_register_copied_before_it_is_restored` (7),
  `exact_call_arity_recovers_a_tail_call_slot_from_the_preceding_block` (3),
  `a_copy_is_not_inlined_into_an_if_arm_past_a_write_to_its_source` and
  `a_load_is_not_inlined_into_an_if_arm_past_a_store` (8, both passes).
  Tests 3, 4, 5, 7 and 8 carry in-test controls.
- [x] Crate gate: `cargo nextest run -p fission-pcode -p fission-decompiler
  -p fission-midend-normalize` (1,972 passed); CI clippy line clean.
  `x64_branchind_register_fp_tail_call` now prints its first argument as
  `(unsigned long long)param_2` (read through the zero-extension of the
  copied `edx`, which is written again before the jump); its assertion
  accepts the cast.

### Measured

`fission_cli verify --all --tier ground-truth`, 16 sample binaries, 15,280
functions, current `main` (`b62197539`) vs. this branch:

| verdict | main | this branch |
|---|---|---|
| equivalent | 5,571 | 5,775 |
| diverged | 948 | 829 |
| unsupported | 8,439 | 8,349 |
| harness | 322 | 327 |

Transitions: diverged → equivalent 132, unsupported → equivalent 78,
harness → equivalent 3. Out of equivalent, 9:

- 7 → unsupported: newly printed arguments are string literals or stack
  addresses (`"wb"`, `&stack_frame`), which the verifier's interpreter has
  no address for. The output gained arguments the machine passes.
- 2 → diverged, both pre-existing defects the old argument-less print hid:
  bash `strvec_sort` now prints `qsort(p, n, 8, cmp)`, and the loop-carried
  `mov rsi, rax` that sets `n` is not printed (main: `qsort()`);
  `_malloc_unblock_signals` prints `sigprocmask(2, param_2, 0)`, which is what
  the machine passes, but its signature declares `param_2` without
  `param_1`, so the verifier feeds its first argument to the wrong slot.

Decompile sweep over the same binaries: `return f();` re-lowering a call
also made with arguments, 151 → 109 functions.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`): perfect 67/233,
total distance 7,482 before and after, every row identical. Arguments are
not CFG nodes. Not a quality claim.

The earlier `rb` verify baseline (before #929-#931 landed) showed bash
`-O0` `0x607f5` returning an unassigned `uVar111`; it is already
equivalent on current `main`.

## 6. AI Review / Prompt Firewall

No external model was consulted.
