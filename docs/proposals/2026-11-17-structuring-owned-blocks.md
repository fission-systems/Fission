# Proposal: a recovered statement owns only the blocks it prints

Status: implemented; `-O0` and `-O2` measured. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Not one row: four places where the output loses code or reads the wrong
value, found while reading the output of a different change (cutting flow
behind a call that never returns), and all present on the baseline
`a4358919e` (PR #903). Read on a 16-binary sample of the local DecBench
configs (8 `-O0`, 8 `-O2`; 15,290 functions), caches disabled.

A structuring rule returns a statement and the block index the walk resumes
at. The walk treats every block in between as printed.

| defect | function | config | what the baseline prints |
|---|---|---|---|
| switch drops its block's statements | gnulib `hash_insert` (`sort`, `tar`) | `-O0` | `switch (local_14)` on a variable never set; the `hash_insert_if_absent` call is gone |
| | gnulib `gettext_quote` (`sort`, `tar`) | `-O2` | `switch (rax->field_0 & ~0x20)` on a pointer never set; the `locale_charset` call is gone |
| | `no_options` (bash) | `-O0`, `-O2` | 2 of its 4 calls |
| switch steps over an arm that only sets a register | `rsa_hash_alg_oid` (9 openssh programs) | `-O0` | `default: return rax;` on a variable never set, where the source returns `-10` |
| | `rl_set_keymap_from_edit_mode` (bash) | `-O2` | `case 0: return rl_editing_mode;` -- the arm's store to `_rl_keymap` is gone |
| merge variable replaces a read behind the tail's own write | `rl_history_search_forward`, `_backward`, `rl_history_substr_search_forward`, `_backward` (bash) | `-O0` | `jle` after `cmp count, 0` printed with the overflow flag of an earlier pointer compare: `zf \|\| bVar56 != (count < 0)` |

- Failure category: lost statements; wrong value in a condition.
- Measured by the call instructions that reach the output (section 5): the
  baseline prints 112,188 call sites on the sample and misses 55 that this
  change prints, in 34 functions of 5 programs at both optimisation levels.

Why the output is wrong, without a metric: `hash_insert` is one call and a
three-way return, and the baseline output has no call in it. A reader is shown
a `switch` on a variable nothing assigns.

## 2. Owner Proof

- [x] Structuring: `crates/fission-midend-structuring/src/switch.rs`,
  `linear_body.rs`, `guarded_tail/bodies.rs`.

Each is the first place the fact goes wrong:

- **Switch head.** `try_lower_switch` and
  `try_lower_direct_dispatcher_switch` return `PreHirStmt::Switch`. Their three
  callers (`sese_driver::apply_collapse_rule`, the linear walk in
  `linear_multiblock.rs`, the loop-body reducer in `loops.rs`) push that
  statement and resume at `skip_to`; none lowers the dispatching block's
  statements. Every `if` rule does (`cond_prefix`), which is why the same
  block printed as an `if` keeps them.
- **Switch skip.** With no shared join the two functions resume at
  `max_skip`, the furthest block any arm reached. Nothing checks that the
  blocks below it belong to an arm.
- **Switch arm.** `canonicalize_switch_target` follows an arm through
  "trivial forwarding blocks" to the block behind them and takes that as the
  arm. A forwarding block may hold register copies (`mov eax, -10; jmp exit`);
  the arm then starts behind them and they are printed nowhere.
- **Later compare blocks.** A compare chain is several blocks. The statements
  of every one but the first were dropped with the first's.
- **One-armed `if`.** `lower_conditional_tail` returns the arm's own resume
  position. An arm that returns from a block past the join reports a position
  past the join.
- **Guarded tail.** `collect_guarded_tail_exported_bindings` records a read
  site per *statement*, and `apply_guarded_tail_replacement_read` replaces the
  name throughout that statement. The loop already stops at the first
  top-level statement that redefines the name; a block or a branch that
  redefines it and then reads it is one statement.

```text
# baseline, hash_insert                 # this change
switch (local_14) {                     uVar0 = sub_11bdd(param_1, param_2, &local_10, param_2);
    case -1:                            local_14 = uVar0;
block_11fa8:                            ...
        return uVar0;
    case 0:
        goto block_11fa8;
    default:
        goto block_11fa8;
}
```

## 3. Generality / Invariant Proof

```text
A statement returned with a resume position owns the blocks from its own up
to that position: they are printed inside it or not at all.

1. A switch replaces a block's terminator. The block's statements go in
   front of it. For a compare chain, a later compare block's statements run
   only when the earlier compares failed, and a switch has no place for them.
   They may be dropped only if that changes nothing: the block writes
   registers and temporaries only (no store, no call), and no path from it
   reads one of the names it assigns before assigning it again. Otherwise the
   chain is not a switch.
2. A switch with no join owns [idx, max_skip) only if every block in it is
   entered through the switch (idx dominates it) and no goto inside the
   statement names a block of the range the statement does not define.
3. An `if` with an empty arm reaches the join through that arm, so it never
   resumes past a join ahead of it. It may resume earlier.
4. An arm starts at the block the dispatch jumps to. A forwarding block in
   front of it is stepped over only if it writes nothing.
5. A read site may be rewritten to the merge variable only if every read in
   the statement runs before the statement writes the name. An assignment
   does; a block, branch or loop that writes the name may not, and the
   promotion is rejected.
6. A jump-table switch replaces the indirect branch *and* what computed its
   target. Of the statements rule 1 puts in front of it, the trailing run
   that only finds the target is not printed: an assignment to a name, whose
   right side calls nothing (a flag helper over call-free operands counts as
   computing), which the selector does not read, and which no path from the
   block reads before assigning it again. The run ends at the first statement
   that fails any of these; everything before it is kept. Applied only to a
   block that stores nothing and calls nothing.
```

All six read the CFG (dominance, block order), the block's p-code, or the
statement tree. No ISA,
calling convention, compiler or function identity is consulted.

- [x] Not gated on an ISA / calling convention.
- [x] No ISA data involved.
- [x] Synthetic tests state the CFG / statement shape only.

Rule 1 was first written as "a chain with any statement in a later compare
block is not a switch". Measured on the full configs that was wrong in the
other direction: at `-O0` every compare re-loads its operand and sets the
flags, so 171 functions stopped being switches (distance 392,625 -> 393,372,
perfect 12,994 -> 12,981). The statements were the compare itself. The rule
above keeps those switches and still refuses a chain that does something
between its compares.

The search for a reader in rule 1 works on names in lowered statements, on a
copy of the lowering state (`lower_observed`), so that looking at a block
ahead of its turn does not fix what it lowers to. It stops after 128 blocks
and answers "read". It can refuse a switch that would have been fine -- when
the real lowering resolves the read to the value instead of the name -- and
the chain then prints as `if`s, which is correct.

Rule 6 exists because rule 1 alone was measured to print too much. A block
that ends in `jmp [table + index*8]` lowers to the bounds test's leftovers,
the scaled index, the table load and the flag helpers of the compare; the
`switch (selector)` already says all of it. One `-O2` function (`fx2txt`,
four programs) printed 13 such statements in front of its switch, and the
table load through its first parameter made the type pass call that
parameter a pointer (type perfect 624 -> 620 over the config). The reader
search is the one rule 1 uses, with the same budget and the same failure
mode: when it cannot tell, the statement is kept.

Rule 5 rejects more than it has to: a statement whose reads all sit *behind*
its own write needs no merge at all. Telling the two apart needs a
path-sensitive walk of the statement; rejecting is sound and its cost is
measured below.

## 4. Risk And Ownership Check

- Existing owners, extended in place. No new pass, crate or metric.
- `ParsedSwitch` gains `compare_blocks` (the chain's blocks in order), so the
  caller can ask for their statements.
- `cleanup::collect_referenced_labels` becomes `pub(crate)` for rule 2.
- `StructuringHost` gains `block_writes_nothing` (no op of the block has an
  output), so rule 4 can be asked without lowering the block.
- Rule 1 reuses `head_has_only_discardable_pure_ops`, the for-loop path's
  test for "no store, no call".
- `StructuringHost` gains `block_only_computes` (no `Store`, `Call`,
  `CallInd` or `CallOther` in the block) for rule 6, which reuses the
  guarded-tail module's `guarded_tail_call_target_is_known_pure_helper` and
  `suffix_expr_contains_call` for "calls nothing".
- Interaction: a switch that used to be recovered and is now refused lowers
  through the `if` rules. A guarded-tail promotion that is now refused leaves
  the `goto` it would have removed.
- Interaction: an arm that used to be stepped over can leave the arms without
  a shared exit; the chain then lowers through the `if` rules.
- Must not change: a switch whose head block has no statements; a chain with
  none between its compares; a chain whose later compares only compute what
  nothing reads; a promotion whose tail reads the name only in simple
  statements. All tested.

## 5. Validation Matrix

- [x] Targeted tests, each checked against the line it guards by breaking
  that line and confirming the test fails:

  | broken | failing test |
  |---|---|
  | chain: head statements not carried | `a_switch_chain_keeps_what_its_first_block_does` |
  | jump table: head statements not carried | `a_jump_table_switch_keeps_what_its_block_does_first` |
  | chain: later compare blocks never count as doing work | `a_chain_that_does_work_between_its_compares_is_not_a_switch`, `a_later_compare_that_sets_what_an_arm_returns_is_not_a_switch` |
  | chain: later compare blocks always count as doing work | `a_later_compare_that_leaves_a_register_nobody_reads_is_still_a_switch` |
  | chain: the reader search always answers "not read" | `a_later_compare_that_sets_what_an_arm_returns_is_not_a_switch` |
  | arm: a forwarding block that writes is stepped over | `a_default_that_only_sets_the_return_value_keeps_it` (prints `default: return rax;`, the baseline's output) |
  | chain: ownership not checked | `a_block_between_the_arms_that_the_switch_does_not_hold_is_still_printed` |
  | `if`: a join behind counted as ahead | `a_join_behind_the_if_leaves_the_reported_position_alone`, `multiblock_loop_rewrites_all_exits_to_breaks` |
  | `if`: always resumes at the join | `an_arm_that_stops_short_of_the_join_leaves_the_rest_to_the_walk` |
  | `if`: rule off | `a_one_armed_if_resumes_at_the_join_ahead_of_it` |
  | guarded tail: compound statement rewritten | `a_read_behind_the_tail_s_own_definition_keeps_its_name` |
  | guarded tail: an assignment no longer reads first | `a_read_in_front_of_the_tail_s_definition_takes_the_merged_value` |
  | jump table: nothing dropped from the head | `what_only_finds_the_jump_target_is_not_kept` |
  | jump table: reads behind the block ignored | `a_name_read_after_the_block_is_kept_with_all_before_it`, `a_jump_table_switch_keeps_a_value_its_block_sets_for_later` |
  | jump table: the selector's reads ignored | `what_the_selector_reads_is_kept` |
  | jump table: every right side counts as computing | `a_call_is_kept_and_so_is_what_feeds_it`, `a_flag_helper_on_the_result_of_a_call_is_kept` |
  | jump table: a flag helper's operands not looked at | `a_flag_helper_on_the_result_of_a_call_is_kept` |

  The reader search itself is a pure function over blocks with eleven unit
  tests (a read on one of two paths, a write that ends the search, a write
  inside a branch that does not, the terminator as the reader, a loop, a
  block that cannot be looked at, the budget).

  Not covered by a failing test: the ownership check at the jump-table and
  terminator call sites (removing either fails nothing; the predicate itself
  has four unit tests); the three call sites of the one-armed-`if` rule (the
  rule is a pure function with five unit tests); and the "no store, no call"
  precondition of rule 1 (removing it fails nothing: a call is already
  refused because it is not an assignment, and no test builds a store to a
  stack slot in a compare block); and rule 6's "stores nothing, calls
  nothing" gate on the block (removing it fails nothing: the statement-level
  test already refuses a call, and a store is not an assignment to a name).

  Rule 6 is tested on the pure function that picks the run
  (`statements_ahead_of_the_target_computation`) and by one end-to-end
  function. An end-to-end test that the target computation is *not* printed
  was written and thrown away: it passed with the rule switched off, because
  in a hand-built block the table load is already dead before structuring.

  `a_later_compare_that_sets_what_an_arm_returns_is_not_a_switch` checks that
  the chain is refused, not that accepting it would print a wrong value: in
  that synthetic function the lowering resolves the arm's read to the value,
  so the switch would have been right. No function was found where dropping
  a later compare block's write changed a printed value.

- [x] Crate-level gate: `cargo nextest run -p fission-midend-structuring -p fission-pcode -p fission-midend-normalize`,
  debug (2,178) and `--release` (2,177);
  `cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity`;
  `cargo fmt --all --check`; `scripts/check/owner_boundaries.sh`.

- [x] Call-site coverage on the sample. Every call expression carries the
  address of the instruction it came from; a temporary probe printed, per
  function, the set of those addresses present in the final output.

  | | `-O0` (8,129 functions) | `-O2` (7,161 functions) |
  |---|---|---|
  | call sites printed, baseline | 54,972 | 57,216 |
  | call sites printed, this change | 55,003 | 57,240 |
  | printed by the baseline and not by this change | 0 | 0 |
  | printed by this change and not by the baseline | 31 (18 functions) | 24 (16 functions) |

  Switches printed on the same sample: baseline 215 (`-O0`) / 173 (`-O2`),
  this change 239 / 166.

  The probe was run on rules 1-5. Rule 6 removes only assignments that call
  nothing, and the full configs below were checked for lost callees with
  rule 6 in.

- [x] Full-config measurement, `-O0` (`unoptimized`, 33,899 functions),
  against the baseline build, caches off. Four large functions (`yyparse`,
  `inflate`, `poly1305_auth` x2) produced no output in this run and are left
  out of both columns; run alone, each prints in 7-16 s and the same text as
  before rule 6, so they ran out of time under the sweep's load. With them
  (measured before rule 6) the distance row reads 392,625 -> 392,115 and
  every other row is the same.

  | | baseline | this change |
  |---|---|---|
  | functions scored for structure in both | 31,983 | 31,983 |
  | structure distance, total | 392,261 | 391,751 |
  | structure perfect | 12,994 | 13,068 |
  | type perfect | 3,383 | 3,381 |
  | type distance over the 49 functions whose type score changed | 137 | 109 |
  | byte perfect | 780 | 792 |
  | Union | 15,146 | 15,158 |
  | functions whose text changed | | 570 |
  | score better / worse | | 194 (-1,604) / 117 (+1,094) |
  | new perfect / lost perfect | | 88 / 14 |
  | changed functions that lost a callee name | | 0 (5 lose the `__sborrow` intrinsic, the corrected flag merge of section 1) |

  Rule 6 changes two `-O0` functions (`simple_strtod_fatal`,
  `selinux_set_callback`: a parameter prints as `uint`, not `int`, and the
  bounds test in front of the switch loses its cast). No score moves.

  Read, not only scored:

  - 142 functions print one more `switch` (distance -938). Their arms are one
    block that sets the return value and jumps to the exit. The baseline
    stepped over every arm to the same exit block, found one target and
    printed `if`s; the arms are now distinct. `sig2name`, `mode_to_filetype`
    and 3 other rows were perfect as `if` chains and are not as switches
    (+60, +30, ...): the two forms mean the same and the binary does not say
    which the source used.
  - 35 functions print one `switch` fewer (distance +341). In the ones read
    (`rsa_hash_alg_oid` x9, `parse_absolute_time` x12; 21 of the 35) the baseline's
    `default:` returned a variable nothing set; the default sets its value in
    a block that jumps past the arms' join, the arms then share no exit, and
    the chain prints as `if`s with the right value. 9 of the 14 lost perfects
    are `rsa_hash_alg_oid`, perfect on the baseline while switching on an
    unset variable.
  - 3 of the 88 new perfects are not right: `should_dereference` (cp,
    ginstall, mv) returns `!command_line_arg ? 0 : 1` whatever the switch
    chose. The baseline's output for it is wrong too, differently; see "Not
    fixed".

  - The two type perfects lost net are `is_colored` (4 programs lost, 2
    other functions gained). The baseline switched on a variable nothing set
    and printed no load at all; the head now prints, and its load address is
    wrong (`rax = 176256; rax = *(unsigned long long *)(352512);` for
    `color_indicator[type].len`). See "Not fixed".

- [x] Full-config measurement, `-O2` (`inlined`, 26,012 functions, 24,186
  scored for structure), against the baseline build, caches off:

  | | baseline | rules 1-5 | rules 1-6 (this change) |
  |---|---|---|---|
  | structure distance, total | 616,056 | 616,032 | 616,032 |
  | structure perfect | 7,409 | 7,409 | 7,409 |
  | type perfect | 624 | 620 | 624 |
  | byte perfect | 500 | 500 | 500 |
  | Union | 7,858 | 7,854 | 7,858 |
  | functions whose text changed | | 195 | 190 |
  | score better / worse | | 39 (-243) / 28 (+219) | 39 (-243) / 28 (+219) |
  | new perfect / lost perfect | | 0 / 0 | 0 / 0 |
  | changed functions that lost a callee name | | 0 | 0 |

  At `-O2` this is not a measured improvement: the distance moves by 24 in
  616,056 and nothing else moves. The type score changes in 3 functions, all
  up (distance 23 -> 19).

  Read, not only scored:

  - `fallible_read_node` (u-boot, +20): the baseline lost the function's
    first call and switched on a register nothing set; the call and the test
    of its result are now printed.
  - `gnutls-cli-debug::main` (+25): the baseline printed a second copy of a
    block behind `return 0;`, where nothing reaches it. It is gone.
  - `groups::main`, `no_options`: an `if` chain becomes a `switch` with the
    same meaning.
  - `fx2txt` (4 programs), `pred_newerXY`: baseline text again (rule 6).
  - `sshkey_size` (9 openssh programs, distance 18 -> 18) is **worse**: it is
    now a `switch`, and one arm tests `rdi`, which nothing assigns; the
    baseline read `param_1->field_8` there. See "Not fixed".

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? [x] No external prompt.
- Unseen or synthetic validation evidence: the full configs, of which the
  scored sample set is 233 rows; the synthetic tests above.

## 7. Review Notes

- [x] No binary/function/address/corpus guards in production code.
- [x] No claim from dashboard-only edits.
- [x] No new pass, helper crate or metric.

## Not fixed

- **A default that leaves past the arms' join.** `default: x = -10; goto
  exit` where the cases meet one block earlier (`x = 0; exit: return x`) has
  no shared exit as the rules compute it, so the chain prints as `if`s. It
  could be a `switch` whose default returns. 35 `-O0` functions.
- **A return value merged on the wrong condition.** `hash_insert` returns
  one of three values and prints one; `should_dereference` prints a ternary
  on the last condition tested. The lowered `Return` resolves the return
  register to one reaching value. Builder (register join), not structuring.
- **A block's statements lowered after its terminator name a register
  differently.** The builder caches a block's terminator and its statements
  separately. When the terminator is lowered first, a register it reads is
  named after the incoming value, and the statements lowered afterwards
  assign another name; the output then reads a register nothing assigns.
  On the sample (15,296 functions) 1,115 functions read a never-assigned
  register-named local on the baseline and 1,110 with this change: 7 gone
  and 2 new, both from switches that are now recovered (`sshkey_size` at
  `-O2`, `if (rdi)`; one `-O0` function of `sshd`, `if (rdx)`, where the
  baseline was a correct `if` chain). Two fixes were measured and neither
  shipped: lowering the statements first inside `lower_block_terminator`
  changes 7,503 of the sample's functions (1,110 -> 1,039, 105 switches
  lost, 19 gained), far outside this change; lowering each arm's first block
  before the switch changes 464, fixes the `-O2` function and breaks the
  `-O0` one differently. Builder, own change.
- **A copy read after its source is overwritten.** `mov rdx, rax; lea rax,
  [table]; mov rax, [rdx + rax]` prints as `rax = 176256; rax = *(352512)`:
  the copy is resolved by name to the register's later value. The baseline
  has it in 4 `-O0` functions (among them `compat_banner`'s loop head); the switch
  heads rule 1 now prints add `is_colored` (4 programs), where the baseline
  printed no head. Whether this is the defect above or a second one was not
  established. Builder.
- **Rule 5 is wider than it has to be** (section 3).
- On the sample, 333 functions miss a reachable call site on the baseline
  and with this change alike. Not looked at.
- In one synthetic CFG a call in a return tail copied into two arms was not
  printed, with and without this change. Not reproduced on a binary, not
  chased.
