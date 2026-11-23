# A loop that exits from the middle of its body is a loop

Date: 2026-11-23. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Found while classifying the `-O2` / `-O2 -fno-inline` excess against Kuna on
the full-dataset baseline: rows where Fission prints a backward `goto` and
fewer loops than Kuna carry about a quarter of the excess distance on both
configs (`goto+fewer-loops`: 3,731 rows / 38.5k on `optimized`, 3,402 rows /
36.4k on `inlined`).

| function | config | machine shape | printed (base) |
|---|---|---|---|
| zlib `compress2` (`0xa940`) | `-O2 -fno-inline` | `jmp head; reload: mov eax,[..]; mov edx,[..]; head: ...; call; test eax,eax; jz reload` | first iteration peeled and printed twice, then `block_a9f0: ... goto block_a9f0;` |
| zlib `send_all_trees` (`0x96c0`) | `-O2 -fno-inline` | same rotated layout around the tree-emission loop | backward `goto block_9840` |
| dash `command` (`0x12010`) | `-O2` | `while (readtoken() == TNL)` -style loop with the reload block above the head | `block_12400: ... goto block_12400;` |
| libedit `history_arg_extract` | `-O2 -fno-inline` | rotated counting loop | 27 `goto`s |

Why the old output is wrong without a metric: the machine code is a single
loop. The old output either printed its first iteration as a second,
separate copy of the body (`compress2`: the same calls appear twice in the
text although the binary calls them from one place each) or spelled the back
edge as a `goto` to a label in the middle of straight-line code, so a reader
has to rebuild the loop from labels.

## 2. Owner Proof

- [x] Structuring, `fission-midend-structuring`:
  - `loops.rs`: `try_lower_while` needs the exit test at the head and
    `try_lower_multiblock_dowhile` needs it at the latch. A natural loop whose
    back edge comes from an **unconditional** block (a "loop and a half", or a
    latch that only reloads loop-carried values) matches neither, and
    `try_lower_multiblock_infloop` only takes loops with **no** exit.
  - `sese_discovery.rs` (`prune_children_overlapping_non_contiguous_owner`):
    GCC lays such loops out rotated -- the reload block sits *above* the head.
    The SESE tree can give that block to an earlier contiguous sibling; the
    pruning only dropped *later* children overlapping a non-contiguous owner,
    so the block got two owners and the whole region failed at final
    reconstruction (`duplicate block ownership`), falling back to linear.

```text
[DIAG] try_lower_while reject: idx=3 block=0xa9f8 reason=exit_target_mismatch true=0xaa0f false=Some(43516) exit=0xaa40
[DIAG] structuring rule finish: rule=do-while block=3 ... outcome=none
[DIAG] structuring rule finish: rule=infloop block=3 ... outcome=none
```

## 3. Generality / Invariant Proof

Generalized rule (CFG facts only):

```text
LoopBody(head) has exactly one exit target E (= its canonical exit), and the
body occupies exactly the block range [first, E) with first <= head.
Blocks in [first, head) -- a rotated layout -- each have one successor and
are not loop heads themselves, so no other rule can have built a region from
them. Lower the body through the existing loop-body subgraph lowering with
break = E, continue = head; require that the lowered body reaches its break.
Emit `while (1) { ... }` and claim [first, head) as extra members.
```

Ownership: a non-contiguous child keeps its blocks against siblings on
**either** side (earlier contiguous siblings overlapping it are dropped and
their blocks re-lowered from the raw CFG); between two non-contiguous owners
the earlier one still wins.

A tier-1 filter that rejected any candidate sharing a block with a surviving
child was tried and dropped: on u-boot `handle_dollar` it rejected a rotated
loop, the next candidate in rule order took a range whose lowering did not
contain the `${...}` scanning loop, and the call at `0x6081902c` vanished from
the output (found by comparing call sites by instruction address).

ISA-agnostic check (ADR 0009):

- [x] No calling convention / ISA condition: the rule reads `LoopBody`,
      successor counts and block ranges.
- [x] Nothing ISA-specific added.
- [x] Synthetic test states the CFG shape only.

Comparable coverage:

- Similar shape 1: zlib `compress2`, `send_all_trees`, `uncompress2` (`-O2 -fno-inline`).
- Similar shape 2: dash `command` (`-O2`), libedit `history_arg_extract` (`-O2 -fno-inline`).
- Synthetic invariant test: `rotated_mid_exit_loop_lowers_to_while_with_break`
  (fails without the rule: no loop is printed), with the acyclic twin
  `acyclic_twin_of_rotated_loop_stays_loop_free` as the negative control;
  `prunes_earlier_contiguous_child_overlapping_rotated_owner` and
  `keeps_earlier_non_contiguous_owner_over_later_one` for the pruning.

## 4. Risk And Ownership Check

- Existing owner extended: the `InfLoop` collapse rule (after the single-block
  and no-exit forms) and the existing SESE child pruning; body lowering is the
  shared `lower_loop_body_subgraph`.
- Shared analysis: CFG / loop body / block ownership facts only.
- New owner-to-owner dependency: none.
- Known cases that must not change: loops the `while` / `do-while` rules
  already take (they run first and win), multi-exit loops (declined).

## 5. Validation Matrix

See the PR for the full-dataset sweep tables (base `main-1f10b8c78`).

- [x] Targeted invariant tests: `cargo nextest run -p fission-pcode -E 'test(rotated_mid_exit) | test(acyclic_twin)'`
- [x] Crate gates: `cargo nextest run -p fission-midend-structuring` (396 passed),
      `cargo nextest run -p fission-pcode` (all passed).
- [x] Patch-validation pool (`control_shapes.c`, clang `-O0`/`-O2`): output
      byte-identical before/after (go; no signal).
- [x] Correctness evidence on changed functions: call sites by instruction
      address compared on each changed function in a focused slice; raw
      hunks read (PR body).

## 6. AI Review / Prompt Firewall

- [x] No other model was asked for implementation advice.

## 7. Review Notes

- [x] No binary/function/address/corpus guards in production code.
- [x] Quality claims only from the measured sweep.
- [x] No duplicated owner.
