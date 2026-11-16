# Proposal: the stack-protector check is removed in every form it is laid out in

Status: implemented; results below. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).
Stacked on
[`2026-11-16-tail-dup-straight-line-tails.md`](2026-11-16-tail-dup-straight-line-tails.md),
whose "Not fixed" list this closes the first item of.

## 1. Baseline Row Anchor

Not one row: a repeating shape, read on the full local DecBench configs
(`unoptimized`: 31,987 scored functions; `inlined` (`-O2`): 24,186), baseline
`0d3eb0cf2` (the tail-dup change above), caches disabled.

`-fstack-protector` puts a check in front of every return of a function with a
local buffer. The existing pass removed it in one layout only -- the handler
call alone inside a one-armed `if`. The baseline still prints
`__stack_chk_fail` in 44 `-O0` and 1,071 `-O2` functions. Read by what stands
in front of the call, the checks among them have four layouts:

| layout | example (baseline output) | function | config |
|---|---|---|---|
| a label inside the guard | `if (x) { block_f: __stack_chk_fail(); }` -- printed without the label when nothing refers to it | `plain_key_blob` (openssh), `rl_insert` (libedit) | `-O2` |
| the handler after the check | `if (!x) { return v; } __stack_chk_fail();` | `decode_line_length` (coreutils), `inflate` (zlib), `start_header` (tar) | `-O0` |
| | | `tilde_expand_filename` (openssh, 12 binaries), `_generate_request` (gnutls) | `-O2` |
| the check jumps | `if (x) goto block_f;` ... `block_f: __stack_chk_fail();` | `do_ipnh` (iproute2; seven checks), `unary_operator` (bash) | `-O2` |
| the handler past the end of the arm the check is in | `if (c) { ...; if (!x) { return v; } } else { ... } __stack_chk_fail();` | `lwt_print_encap` (iproute2), `env_modprobe_options_append` (kmod), `mergefiles` (coreutils) | `-O2` |

- Failure category: compiler scaffolding printed as program logic. The output
  is correct; it shows a branch and an abort call the source does not have.
- The shape is not rare: the change alters 42 `-O0` and 708 `-O2` functions
  (29 and 430 distinct names, 16 and 25 projects). Every one of them printed
  the handler before; no function without it changes.

Why the output is wrong, without a metric: the reader is shown a decision --
"if this value is non-zero, abort" -- in front of each return, and has to
recognise it as the canary and discard it. The source has no such decision and
neither does an unprotected build of the same function. The pass that exists
to remove it already agrees; it only failed to recognise the layout.

## 2. Owner Proof

- [x] Structuring (cleanup): `crates/fission-midend-structuring/src/cleanup/stack_protector.rs`.

The pass runs once on the finished layout and matched `If { then: [call], else: [] }`
(either polarity). Observed on the anchors, at the pass's own input:

- `plain_key_blob`: the arm is `[Label, call]`. An earlier clone left the
  label; nothing refers to it by the time the pass runs, so it is not printed
  and the output looks exactly like the form the pass claims to remove.
- `decode_line_length`: the layout put the handler block after the `if`, so the
  arm that fails is the one that is not there.
- `do_ipnh`: one block calls the handler and every other check reaches it by
  `goto`.

All three are the same branch the pass already removes, in a layout decided by
block order. Nothing upstream is wrong: the branch is real in the binary.

```text
# baseline (decode_line_length)           # this change
if ((uint)rax) {                          if (!(uint)rax) {
    if ((uint)rax == 1) {                     xVar6 = local_8 - *ptr;
        rax = 0;                              return rax;
        goto block_7054;                  }
    }                                     if ((uint)rax == 1) {
    rax = -1;                                 rax = 0;
}                                             xVar6 = local_8 - *ptr;
block_7054:                                   return rax;
if (!(local_8 - *(...)(fs_offset + 40))) {  }
    return rax;                           rax = -1;
}                                         xVar6 = local_8 - *ptr;
__stack_chk_fail();                       return rax;
```

With the check gone the tail is two straight-line statements, which is what
`tail_dup` copies to each return (the previous proposal's rule, unchanged).

## 3. Generality / Invariant Proof

```text
A branch exactly one side of which does nothing but reach the stack-protector
handler is the compiler's check. It is replaced by its other side.

"Reach the handler" is: the handler call (behind any labels, followed by at
most returns); a jump to a label standing on the call, or on such a jump; or,
for an arm that is not there, falling out of the `if` into the call. Falling
out of the last statement of an `if` arm or a plain block continues with what
follows the enclosing statement; falling out of a loop body or a switch case
does not.

A branch both of whose sides only reach the handler has no passing side and
stays.

After a check is removed, a handler block that nothing reaches -- the
statement before it leaves, and no jump names a label on it -- is removed.
Only the handler: no other statement is swept. A jump that is left pointing at
a removed label is replaced by the call it stood for.
```

The rule reads the statement tree and one callee name -- the handler names
`stack_protector.rs` already owned. No ISA, calling convention, compiler or
function identity is consulted.

- [x] Not gated on an ISA / calling convention.
- [x] No ISA data involved.
- [x] Synthetic tests state the statement shape only.

Deliberately not a check, and left alone: a handler that control runs into
from ordinary code.

```text
sub_1f410();            // the compiler put nothing after this call
__stack_chk_fail();
```

The compiler laid the handler out behind a call it knew not to return. Fission
does not know that, so here the handler call is the only thing that ends the
path; removing it would make the code run on into the next label. Most of the
529 `-O2` functions that still print the handler keep it for this reason
("Not fixed"). That is a no-return-analysis gap, a different owner.

`tail_dup` uses the same definition (`stack_protector_pass_arm` now takes the
statement and what follows it), so "a return behind the check is still the
function's return" holds for the handler-after layout too.

Comparable coverage:

- Similar shape 1: `key_sig_algorithm` (openssh `-O2`): one check that jumps
  and one with the handler after it, in the same function.
- Similar shape 2: `jit_exec` (grep `-O2`): two checks that jump, plus one
  fall-through into the handler behind a call -- the checks go, the
  fall-through stays.
- Synthetic invariant tests: 15 new in `stack_protector.rs`, 1 in `tail_dup.rs` (section 5).

## 4. Risk And Ownership Check

- Existing owner: `stack_protector` (what the check is), `tail_dup` (reads that
  definition). Both extended in place; no new pass. The post-loop tail of
  `finalize_post_layout_body_with` is now `settle_finished_body`, so tests can
  run the three passes in their real order.
- Ordering change: the check is removed before
  `unwrap_else_after_diverging_then`, not after. The handler is a call that
  never returns; left in, an arm ending in it unwrapped an `else` the source
  wrote as one (tested).
- Shared substrate: [x] none; owner-local rule on the statement tree.
- A mistake made and removed, recorded because the score rewarded it. The
  first version re-ran `drop_unreachable_tails` to a fixpoint after removing a
  check. That deleted dead code which had nothing to do with the canary and was
  kept only by jumps from other dead code: `list_disks` (sysvinit `-O0`) went
  from 371 lines to 13 and `do_one_pass` (e2fsprogs `-O2`) from 1,820 to 1,563. On
  `-O2` that build measured **1,593 better** than the one proposed here
  (614,463 against 616,056). It was found by reading the largest line drops,
  not by the total. The replacement sweeps the handler only, and decides
  "leaves" by the last statement of each arm -- `unreachable_tail` uses "any
  statement", which is wrong for an arm entered again at a label (tested).
- New owner-to-owner dependency: [x] none new (`unreachable_tail::is_diverging_call`
  becomes `pub(super)`; same module tree).
- Telemetry impact: none.
- Must not change: a function without the handler (measured: none changes); a
  handler control still runs into (tested, and visible in the 529); code
  that is dead for reasons other than the check (tested).

## 5. Validation Matrix

- [x] Targeted invariant tests: `cargo nextest run -p fission-midend-structuring`
  -- 351 pass. Each rule was checked against its tests by breaking that one
  line and confirming those tests, and only those, fail:

  | broken | failing tests |
  |---|---|
  | an absent arm never fails | `a_check_with_the_handler_after_it_goes`, `a_check_sees_the_handler_past_...`, `a_branch_with_no_passing_side_stays`, `the_pass_arm_is_found_when_the_handler_follows_the_check`, `the_handler_does_not_unwrap_...`, `clone_sees_through_a_check_laid_out_before_the_handler` |
  | the end of an arm never continues past the `if` | `a_check_sees_the_handler_past_the_end_of_the_arm_it_is_in` |
  | a loop body continues past the loop | `a_loop_body_does_not_run_into_what_follows_the_loop` |
  | labels in the failing arm not skipped | `a_label_on_the_handler_does_not_hide_the_check`, `a_check_that_jumps_...`, `a_jump_that_was_not_a_check_...` |
  | a jump is never the failing side | `a_check_that_jumps_to_the_handler_goes`, `a_label_on_a_jump_to_the_handler_is_followed` |
  | fail labels collected in one pass | `a_label_on_a_jump_to_the_handler_is_followed` |
  | jumps left dangling | `a_jump_that_was_not_a_check_gets_the_call_it_stood_for` |
  | every jump to a fail label rewritten | `a_jump_to_a_handler_that_is_still_there_stays_a_jump` |
  | a branch with two failing sides picks one | `a_branch_with_no_passing_side_stays` |
  | check removed after the `else` unwrap | `the_handler_does_not_unwrap_an_else_the_source_wrote` |
  | unreached handler never swept | `a_check_with_the_handler_after_it_goes`, `a_check_sees_the_handler_past_...`, `a_label_on_a_jump_...`, `the_handler_does_not_unwrap_...` |
  | sweep ignores jumps to the handler | `a_jump_to_a_handler_that_is_still_there_stays_a_jump` |
  | sweep ignores falling into the handler | `a_handler_control_still_runs_into_stays`, `an_arm_that_leaves_and_is_entered_again_...`, three more |
  | sweep takes any dead statement | `only_the_handler_is_swept_not_other_dead_code` |
  | sweep runs once | `a_label_on_a_jump_to_the_handler_is_followed` |
  | "leaves" decided by any statement of an arm | `an_arm_that_leaves_and_is_entered_again_does_not_hide_the_handler` |
  | anything may follow the handler in the failing arm | `an_arm_that_does_more_than_fail_is_not_the_check` |
  | `tail_dup` looks at the `if` alone | `clone_sees_through_a_check_laid_out_before_the_handler` |
  | a two-armed check needs an empty arm | `a_two_armed_check_becomes_its_passing_arm`, `the_pass_arm_is_found_in_either_arm_...`, `gotoless_split_sees_through_...` |

- [x] Crate-level gate: `cargo nextest run -p fission-midend-structuring -p fission-pcode`,
  debug and `--release`; `cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity`;
  `cargo fmt --all --check`.
- [x] Full-config measurement (not the scored sample set), same rows, caches off:

  | | `-O0` base | `-O0` change | `-O2` base | `-O2` change |
  |---|---|---|---|---|
  | total structure distance | 392,704 | 392,625 | 619,683 | 616,056 |
  | structure perfect | 12,994 | 12,994 | 7,379 | 7,409 |
  | Union / type / byte perfect | 15,146 / 3,383 / 780 | 15,146 / 3,383 / 780 | 7,828 / 624 / 500 | 7,858 / 624 / 500 |
  | rows better / worse | | 26 (-184) / 15 (+105) | | 508 (-4,806) / 158 (+1,179) |
  | new / lost perfect | | 0 / 0 | | 30 / 0 |
  | functions printing the handler | 44 | 3 | 1,071 | 529 |

  No row's score moved on unchanged text, in either config.

  A second axis that does not see CFG shape -- recompile and compare bytes --
  on the rows whose text changed: `-O0` +76 (22 better, 9 worse), `-O2`
  -3,385 (369 better, 111 worse). The config totals (2,160,155 -> 2,161,065;
  1,403,771 -> 1,400,780 on the common rows) also contain 7 `-O0` rows and 1
  `-O2` row whose byte distance moved (+834, +394) on **identical** text, so
  the `-O0` total is harness noise, not this change. Compilable: 17,170 ->
  17,170 and 11,531 -> 11,533 (three gained, one lost:
  `send_userauth_info_request`; the two texts give the same diagnostics under
  a local syntax check, so the cause is not established).

  What the score does not show, in the changed functions only: lines
  37,718 -> 37,631 (`-O0`) and 182,014 -> 178,231 (`-O2`); `goto`s
  2,298 -> 2,262 and 14,763 -> 14,370. No output has a `goto` to a label that
  is not there, before or after, and the count of labels printed directly
  before a closing brace is unchanged (211 / 136).

- [x] Sample-set gate (the scored 233 rows): perfect 67 -> 67, total distance
  7,611 -> 7,592. Seven rows move: three better (-42 counting the row the
  sample lists twice), four worse (+23), all four of the kind described next.

Rows that got worse were read, not just counted. In every one read, the diff
is the check and nothing else. By what moved:

1. **The output already had fewer CFG nodes than the source, and now has fewer
   still**: all 15 `-O0` rows and 151 of the 158 `-O2` rows (+105, +1,020).
   The check's two nodes were standing in for nodes Fission fails to recover
   elsewhere in the function -- zlib `inflate` (source 416 nodes, output
   366 -> 363), `example::main` (11; 9 -> 7), `unary_operator` (22; 19 -> 15).
   The distance was better for a reason that had nothing to do with the
   source. The improving rows are the mirror image: of the 508, 433 go from
   over the source's node count towards it, 44 cross it, 30 become isomorphic
   and one was under already.
2. **A lower bound replaced by a computed distance**: `factor_using_pollard_rho2`
   (+141, from a three-line change). The baseline's 222 was the scorer's
   `size_lower_bound` approximation; the new 363 is `vj_ged`. Not comparable.
3. Six `-O2` rows (+18 in total) where the output was over the source's node
   count and the distance still rose by 1-5.

The gains have the same asymmetry to keep in mind: 30 new perfects, 12 of them
one function (`tilde_expand_filename`) in 12 openssh binaries.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? [x] No external prompt.
- Unseen or synthetic validation evidence: the measurement is on the full
  configs, of which the scored sample set is 233 rows; synthetic tests above.

## 7. Review Notes

- [x] No binary/function/address/corpus guards in production code. The only
  name consulted is the stack-protector handler, already owned by
  `stack_protector.rs`.
- [x] No claim from dashboard-only edits.
- [x] No new pass, helper crate or metric.

## Not fixed

- The handler that code runs into. Of the 529 `-O2` functions still printing
  it: directly after a statement, a call not known to be no-return such as
  `fatal_exit()` or `error(1, ...)` (438 functions; 1 at `-O0`); after a block
  one path of which runs out of it -- an arm ending in such a call, a tail
  call printed as `__fission_branchind`, a `switch` with no default, a
  `while (1)` (79; 46 of the sites have the tail call in front). Correct to
  keep until the code in front is known to diverge.
- A handler nothing reaches in a function where no check was recognised --
  after a `goto` with no label (11 `-O2` functions; in a twelfth an
  unconditional jump targets it), or after a block ending in
  `return` whose check was already gone when the pass ran (2 `-O0`). The sweep
  runs only after a removal; who removed those checks and left the handler is
  not established.
- The canary arithmetic stays as a dead assignment
  (`xVar6 = local_8 - *(...)(fs_offset + 40);`), as the module has always
  documented. It is straight-line; removing it is dead-store elimination, not
  this pass.
- The combination with the flag-snapshot change (#900) was not swept.
