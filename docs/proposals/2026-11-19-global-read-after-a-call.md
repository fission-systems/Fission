# A global read after a call is a load

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

bash `-O0` `print_group_command`-style counter (`0x49f6a`): the function
increments a global on entry, makes six calls, then reads the global again
and decrements it:

```text
mov eax, [0x173918] ; add eax, 1 ; mov [0x173918], eax   ; entry
call ... (x6)
mov eax, [0x173918] ; sub eax, 1 ; mov [0x173918], eax   ; exit
```

With #928's merge barrier applied, the builder output for the exit read was
`uVar63 = uVar36` -- the value stored on entry -- so the printed decrement
computed `entry_value - 1` whatever the callees did to the counter. On main
the same read is hidden by `variable_merge` folding the copy onto the global.

The same forwarding also happens one layer later. A synthetic
`g = 5; f(); return g;` printed `f(); return 5;` on main: presentation's
`inline_single_use_pure_assigns` treated `g` as a temporary, inlined the
assignment into the read after the call and deleted the store.

Why the output is wrong, without a metric: the callee can change the global,
so the program returns or stores a value the C does not.

## 2. Owner Proof

- [x] Builder: `builder/expr/lower_expr.rs` (`lookup_def_site_for`),
  `builder/expr/value_recovery.rs` (new `call_may_write_global_between`).
- [x] Printer/presentation: `render/presentation/mod.rs`
  (`inline_single_use_pure_assigns`).

Global accesses are RAM-space varnodes, and the builder's reaching-definition
lookup treats them like registers: a p-code `Call` has no output, so nothing
between the store and the load "defines" the storage. Presentation takes the
global set (`apply_hir_presentation_with_globals`) for its dead-assignment
pass but did not pass it to the inlining pass.

## 3. Generality / Invariant Proof

- [x] Not gated on an ISA or calling convention.

Invariant: a call may write any memory at a fixed address (Ghidra gives a
call an INDIRECT effect on every address-tied storage). So:

1. In the builder, a definition of RAM storage is neither the value nor the
   name of a read when a call can run on some path between them
   (`call_may_write_global_between`). The read falls through to a load of the
   global. Calls in the definition's own instruction do not count -- a
   temporary can live across a call inside one instruction (ARM `blx`), and
   no instruction stores a global and then calls.
2. Which space is RAM is decided per function: id 3 is RAM in the Rust-Sleigh
   numbering and unique space in the legacy one, so it counts as memory only
   when the function's p-code uses Rust-Sleigh register/unique ids
   (`fixed_memory_space_id`). Treating every id-3 varnode as memory turned a
   tail call's argument temporary into a "global" (dash `0xf350`:
   `memcpy(.., sub_8e00)`), which is how this was found.
3. In presentation, an assignment to a global is not inlined away, and an
   expression that reads a global is moved only across statements that write
   no memory.

## 4. Risk And Ownership Check

- Exposes a pre-existing defect it does not fix: a condition built from a
  global's value *before* a read-modify-write (`sub [g], 1; jnz`) is now
  printed as a read of the global *after* the store
  (`g--; if (g != 1)`), where main printed neither the decrement nor the
  right test (`g = g; if (g)`). Same family as gzip `send_bits`. Measured
  below by `verify`, not by reading.

## 5. Validation Matrix

- [x] Targeted test:
  `lower_expr::tests::a_global_read_after_a_call_is_a_load_not_the_value_stored_before_it`
  (one block and three blocks). Positive controls: disabling the builder
  check fails it; removing the presentation guard fails it.
- [x] Crate gate: `cargo nextest run -p fission-pcode -p fission-decompiler
  -p fission-midend-normalize`.

### Measured

Stacked on #928, whose merge barrier this change needs: without it, the
fresh global loads this produces are copy-merged back onto the global and
print as stores the program never makes (dash `0x16f10` on main alone:
`tmp_21f88 = 0 + tmp_21f8c;`).

`fission_cli verify --all --tier ground-truth` (verifier from #927), 16
sample binaries, 15,280 functions, #928 vs. #928 + this change:

| verdict | #928 | + this change |
|---|---|---|
| equivalent | 5,471 | 5,480 |
| diverged | 1,043 | 1,033 |
| unsupported | 8,444 | 8,445 |
| harness | 322 | 322 |

Transitions: diverged → equivalent 9, equivalent → anything else **0**.
diverged → unsupported 1 (bash `0x607f5`): its loop now reads `tempvar_list`
and `tvlist_ind` back after the calls instead of constants, and the
`uVar111` it returns -- previously set to 0 inside a malformed loop -- is now
never assigned; the return value recovery for that function is a separate
gap.

(For reference, the same run measured #928 against #927 alone: diverged →
equivalent 5, unsupported → equivalent 1, equivalent → anything else 0.)

`decomp --all` on the same binaries against main: 630 functions change.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`), #928 vs. #928 +
this change: perfect 67/233 both; total distance 7,530 → 7,525 (three rows:
134 → 126, 33 → 32, 120 → 124). Too small to call a quality change; the
claim is correctness, measured by `verify`.

## 6. AI Review / Prompt Firewall

No external model was consulted.
