# A global's value is read where the machine reads it

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).
Stacked on #930 (itself on #928).

## 1. Baseline Row Anchor

A global read prints as the global's name, and a name is a load wherever the
expression lands. Three shapes print a load at a point after the global was
written:

| function | config | printed | binary |
|---|---|---|---|
| gzip `send_bits` (`0x3a6c`) | `-O0` | `outcnt = outcnt + 1; *(outcnt + outbuf) = c;` | `outbuf[outcnt++] = c` -- the old index |
| dash `0x4e60` (with #930) | `-O2` | `g--; if (g != 1)` | `sub [g], 1; jnz` -- tests the new value against 0 |
| `x = g; f(); return x;` in a callee-saved register | (synthetic, see tests) | `f(); return g;` | the value read before the call |

Why the output is wrong, without a metric: the first writes the byte one
slot too far, the second takes the wrong branch when `g` was 1 or 2, the
third returns what a callee left in `g`.

## 2. Owner Proof

The value was right at each layer's input and lost in the layer itself:

- [x] Builder, `builder/expr/lower_expr.rs`: a read of a global whose
  reaching definition is a store to that global rebuilt the store's
  expression (`g - 1`) at the read, re-reading `g` after the store.
- [x] Builder, `builder/materialize/mod.rs` (`maybe_materialize_output_stmt`):
  a register definition whose value names a global was inlined at its uses
  even when the global is written, or a call runs, before them.
- [x] Normalize, `cleanup/run_copy_prop.rs` (`writes_of`): a store to a
  global is spelled `*&g = v` while reads are `g`, so the run-scoped copy
  propagation carried `x = g` across the store (`send_bits`: the builder's
  PreHIR was right, `iVar60 = outcnt; *&outcnt = iVar60 + 1;
  outbuf[iVar60] = c`, and this pass undid it).
- [x] Presentation, `collapse_trivial_assign_returns` and
  `fold_redundant_select_return_join`: `g = e; return g;` folded to
  `return e;`, deleting the store. The invariant check caught it and
  restored the whole pre-polish tree, so the effect was lost polish rather
  than a wrong store -- but only because the check happened to fire.

## 3. Generality / Invariant Proof

- [x] Not gated on an ISA or calling convention; global memory is the
  materializer's existing `output_is_mapped_global_memory`.

Invariant: a load observes memory at the point it is evaluated, so a value
read from a global must be printed at a point where the global still holds
it -- or be kept in a binding taken where the machine read it.

1. Read after a read-modify-write of the same global (`g = g op x`): read
   memory (`global_memory_read`), don't rebuild the update's expression. A
   store of a constant or a register keeps forwarding its value: those inputs
   still hold it at the read, and reading memory there instead changed names
   and lost a store on bash (`0xaab00`, `0xd3a59`) in a first, wider version.
2. A definition whose value names a global, with a write to that global or a
   call (in a later instruction) before one of its uses -- same block,
   including narrower views of the register (`mov edx, eax` after
   `rax = zext(eax)`), or any use in another block with such a write
   reachable: keep it as a binding.
3. A store to a global is a write of that global for copy propagation.
4. Presentation never folds an assignment to a global away.

Stores through pointers are not counted as writes to a global: nearly all are
stack writes, and telling the rest apart needs alias facts the builder does
not have.

## 4. Risk And Ownership Check

- More bindings where a global is read and then written or a call follows:
  `x = g; ...` instead of a bare `g` at the use. That is the program's
  dataflow, not an extra variable.

## 5. Validation Matrix

- [x] Targeted tests:
  `lower_expr::tests::a_global_read_before_a_call_is_kept_as_a_snapshot`
  (rule 2), `..._after_storing_it_reads_memory_not_the_stored_expression`
  (rule 1), `run_copy_prop::tests::a_store_to_a_global_invalidates_a_copy_of_it`
  (rule 3, with in-test control), `presentation::tests::return_folds_keep_a_store_to_a_global`
  (rule 4, with in-test control). Positive controls: disabling each rule
  fails exactly its test.
- [x] Crate gate: `cargo nextest run -p fission-pcode -p fission-decompiler
  -p fission-midend-normalize`.

### Measured

`fission_cli verify --all --tier ground-truth` (verifier from #927), 16
sample binaries, 15,280 functions, #930 vs. #930 + this change:

| verdict | #930 | + this change |
|---|---|---|
| equivalent | 5,480 | 5,491 |
| diverged | 1,033 | 1,025 |
| unsupported | 8,445 | 8,442 |
| harness | 322 | 322 |

Transitions: diverged → equivalent 9, unsupported → equivalent 2 (one is
dash `0x4e60`, the `sub [g], 1; jnz` row), **equivalent → anything else 0**.
unsupported → diverged 1 (bash `0x91e40`): its tree used to stop on an
undeclared `r12` and now runs far enough to show a call missing from both
versions' output; the set of calls printed is unchanged.

A first version applied rule 1 to every store; it turned seven equivalent
functions diverged (bash `0x71fb9` returned a re-read global through a
mistyped 8-byte `pgrp_pipe`, `0xaab00` renamed a global into a local,
`0xd3a59` lost a store). Restricting it to read-modify-write removed all
seven.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`): perfect 67/233,
total distance 7,525 before and after, all rows identical. A read's position
is not a CFG node. Not a quality claim.

## 6. AI Review / Prompt Firewall

No external model was consulted.
