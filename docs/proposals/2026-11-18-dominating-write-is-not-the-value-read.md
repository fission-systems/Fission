# Proposal: a write that dominates a read is its value only when it is the one write reaching it

Status: implemented and measured; **not a quality claim, and not recommended
for merge as it stands** -- see "What it broke" in section 5. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

A repeating shape, read on the full local DecBench configs (`unoptimized`:
31,987 scored functions; `inlined` (`-O2`): 24,186), baseline
`fix/stack-protector-check-forms` at `a4358919e` (PR #903), caches disabled.

The builder answers "what does this read see" with the nearest write that
dominates it. That write reaches the read on every path, but it is the read's
value only if no other write joins on the way. When one does, value users fold
on one path's value:

| function | config | what the baseline prints | what the binary does |
|---|---|---|---|
| `pwdcmd` (dash) | `-O2` | the loop, then `sub_13f40("%s\n", ...)`; the `if (rbx)` block and its call are gone | `mov ebx,0; loop { ebx = eax }; test bl,bl` -- the test read `0` |
| `bashline_reset` (bash) | `-O2` | `goto block_99700;` forever; nothing after the loop | the loop exits on a byte it loads each time round |
| `crypto_sign_ed25519_ref_sc25519_mul_shortsc` (openssh, 9 binaries) | `-O2` | `do { stack[64] = 0; } while (&stack[68] != &stack[128]);` | a pointer walks `stack[64..128]` |
| bash `0x67931` | `-O0` | 35 lines, 0 of 24 call sites | 436 lines once the branch stops folding |
| `mux_master_read_cb` (openssh), `load_entry` (cronie, 3 binaries), `sparse_dump_file` (tar) | `-O2` | `sub_309a0(...)`, `sub_1(...)`, `sub_3(...)` -- a call to a fixed address | an indirect call through a table; `1` and `3` are not functions |

- Failure category: code deleted or a value replaced by a constant because a
  read took one reaching write for all of them.
- On a 16-binary sample (ten `-O2`, six `-O0`; 15,286 functions) the change
  below alters 1,438 functions.

Why the output is wrong, without a metric: it prints an infinite loop where
the program has a terminating one, drops calls the program makes, and names a
call target the program does not call.

## 2. Owner Proof

- [x] NIR materialization (builder):
  `crates/fission-pcode/src/midend/builder/expr/lower_expr.rs`,
  `lookup_def_site_at`.

The fallback there picks "the write in the deepest block that dominates the
use". The scalar SSA built beside it (`scalar_ssa.rs`) already knows whether
that write is the only one: it places a phi on the iterated dominance frontier
of every write, so a phi for an overlapping piece on the dominator chain
between the write and the read means another write joined. Nothing downstream
can recover this: by then the folded branch and the code behind it are gone.

Ghidra has no equivalent lookup -- a read is an SSA varnode with one
definition, and a merge is a `MULTIEQUAL` whose inputs are never read through.

## 3. Generality / Invariant Proof

Invariant: **a write is the value of a read only if it is the sole write
reaching it.** Stated on the CFG and SSA; no register, mnemonic, function or
address appears in it.

- `definition_is_sole_reaching` (`value_recovery.rs`) walks the dominator chain
  from the read's block up to the write's block and answers `false` at the
  first phi for an overlapping storage piece. It uses the dominator tree SSA
  placed its phis with (`heritage_idom`), not structuring's `ImmDomTree`,
  which gives blocks the entry cannot reach parents that form cycles.
- The lookup is split by what the caller wants. `DefinitionRole::Value`
  (every existing caller) gets `None` for a joined write.
  `DefinitionRole::Name` (`lookup_dominating_def_site`) keeps the old answer,
  for the two places that want the *name* a merge is emitted under: naming in
  `lower_varnode_inner` and loop seeds in `materialize/loop_carried/*`.
- `loop_exit_materialized_register_binding` now runs only when no sole write
  reaches the read; it rebuilt a merge from one edge's copy where there was
  none (`if (r15 < r15)` in sort `0xe260`).

Shape repeats: five programs above, both optimisation levels.

## 4. Risk And Ownership Check

- No new pass, metric, dependency or ISA-specific rule.
- The risk is what a read does once the lookup says "no single value". It
  falls to merge machinery that predates this change -- the explicit merge
  select, the last-materialized-register name, the bare live register -- and
  each of those is right only under its own unstated condition. Section 5
  shows this risk is real.

## 5. Validation Matrix

- [x] Targeted invariant tests: seven in `lower_expr_tests.rs`. Positive
  controls, each by breaking only the fix:

  | mutation | tests that fail |
  |---|---|
  | `definition_is_sole_reaching` returns `true` | joined write, loop write, branch-on-loop-read |
  | chain walk stops at the first parent | `a_join_above_the_read_counts_as_much_as_one_at_it` |
  | loop-exit gate removed | `a_copy_on_one_edge_does_not_rename_a_register_one_write_reaches` |
  | unreachable block answers `false` | `a_read_the_entry_cannot_reach_keeps_the_write_found_for_it` |

- [x] Crate gates: `cargo nextest run -p fission-pcode -p fission-midend-normalize -p fission-midend-structuring`
  debug 2,149 / release 2,148 pass; CI clippy line, `cargo fmt --all --check`,
  `scripts/check/owner_boundaries.sh` clean.
- [x] Sample (16 binaries), by call-instruction address: reachable call sites
  lost 0; regained 157 (`-O2`) and 121 (`-O0`) in 67 functions.
- [x] Full sweeps, same rows, caches disabled:

  | | `-O2` (`inlined`) | `-O0` (`unoptimized`) |
  |---|---|---|
  | rows scored on both sides | 24,186 | 31,986 |
  | total distance | 616,056 -> 619,173 (+0.51%) | 392,543 -> 391,284 (-0.32%) |
  | structure perfect | 7,409 -> 7,410 | 12,994 -> 12,993 (1 new, 2 lost) |
  | type / byte perfect | 624 / 500, unchanged | 3,383 / 780, unchanged |
  | Union | 7,858 -> 7,859 | 15,146 -> 15,145 |
  | rows better / worse | 203 (-2,169) / 399 (+5,286) | 123 (-2,215) / 76 (+956) |
  | texts changed | 3,683 | 1,170 |

  One `-O0` row is missing from the fix run: `gnutls certtool yyparse` hit the
  30 s render deadline under sweep load (15.6 s alone on the baseline, 16.0 s
  with the fix). The same row dropped out of an earlier unrelated sweep.

  Of the 399 worse `-O2` rows, 329 (+4,867) already had at least as many
  nodes as the source and gained more: mostly bodies the baseline had deleted
  (`print_factors` 47 -> 312 lines, `bashline_reset`, `pwdcmd`), scored against
  a source function that does not contain what `-O2` inlined into it.

### What it broke

Read, not inferred from the score:

- **A stale merge select.** `bash find_special_var` (`-O0`) lost its perfect
  score and is now wrong where the baseline was right:
  `r12 = d; if (!r12) { rax = strcmp(..); r12 = rax; } uVar96 = r12 ? d : rax;`
  -- the select re-reads `r12` after the arm reassigned it. The select was
  already synthesized on the baseline and discarded unused; the condition read
  it now because `lower_flag_tested_value` no longer finds a value.
  `tar lseek_or_error` (`-O0`) is the same construct; there the baseline was
  wrong too (`if ((uchar)rax)`, structure-perfect).
  Changed rows carry 178 more such assignments at `-O2` (117 rows) and 279
  more at `-O0` (126 rows). Not all are stale; none was checked for it.
- **A bare register nobody assigns.** A joined read whose naming write has no
  emitted name prints the register, while the writes reaching it went out
  under fresh temporaries. Sort `0x13230`: `param_2 << 5` became `rax << 5`.
  Sample lint "read but never assigned": 9,890 -> 10,058 (`-O2`), 3,900 ->
  3,908 (`-O0`).

Tried and not shipped: dropping a merge select whose operands an arm
assigns. The binding the select defined is also created from the write side,
so the read then bound to a name only one arm assigns. Emitting phi-operand
writes under the register's live name broke a loop-latch test and multiplied
`param_N = ...` lines by eight in sshd.

Conclusion: the lookup is fixed; the places a joined read falls to are not,
and they are where the regressions come from. The owner for those is
consistent naming of every write reaching a merge (SSA emission admission),
not another guard on this lookup.

## 6. AI Review / Prompt Firewall

No external model prompt was used.

## 7. Review Notes

- Not fixed and seen on the way: `xor r, r` counts as a read of `r` when
  deciding which phis are needed; a producer's input resolved at the consumer's
  site (sshd `0x7bde0`, `compat_banner`, `is_colored`); `ImmDomTree` parent
  cycles in regions the entry cannot reach.
