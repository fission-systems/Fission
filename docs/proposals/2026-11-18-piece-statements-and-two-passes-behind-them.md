# Piece statements, and three places that were wrong behind them

Date: 2026-11-18. Status: **implemented and measured** -- see section 5 for
what the measurement does and does not support. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).
Third of three on one branch, after
[the dominating-write change](2026-11-18-dominating-write-is-not-the-value-read.md)
and [the plan-acceptance change](2026-11-18-ssa-plan-declined-for-names-it-does-not-own.md).

## 1. Baseline Row Anchor

The plan-acceptance change lost 50 structure-perfect `-O0` rows. Thirty were
one shape, read on the full local DecBench `unoptimized` config:

| function | binaries | before | after the plan was accepted |
|---|---|---|---|
| `log_level_name`, `log_facility_name` | 12 openssh each | `while (tbl[i].name) { if (..) return ..; i++; }`, 16 lines | 55 lines, the header copied to the entry and the latch, a label and a `goto` |
| `fmt_multistate_int`, `lookup_opcode_name` | 3 each | same | same |

Fixing that exposed three older defects, all read on real functions:

| function | config | what is printed | what the binary does |
|---|---|---|---|
| tar `0x1f625`, tar `0x20b9f` | `-O0` | `p->field_10 = (uchar)rax` and `p->field_8 = param_2` are absent | stores them |
| sshd `0x42b21` | `-O0` | the `\|= 4096` store after a list walk is absent | stores it |
| bash `_rl_nchars_available`, mirai `util_local_addr`, openssh `ssh_krl_file_contains_key` | `-O0` | `uint *slot_0 = (uint *)local_14; .. __errno_location(); *slot_0 = 0;` | `*__errno_location() = 0` |
| libedit `history_def_curr`, bash `uconvert`, bash `bash_complete_filename_internal` | `-O0`, `-O0`, `-O2` | `if (c <= 0) { rdx = 5; } *ev = rdx;` -- the other arm's `rdx = 8` is absent | assigns on both paths |

- Failure category: a loop not recovered; stores deleted; a write through a
  pointer computed once from the wrong value; one arm of a branch dropped.

Why the output is wrong, without a metric: it omits writes the program makes
and writes `errno` through an uninitialized local.

## 2. Owner Proof

- [x] Builder/materialize: `materialize/mod.rs`, `lower_block_ops_range`.
- [x] Normalize: `global_opt/dead_store.rs`, `memory/slots.rs`.
- [x] Structuring host: `structuring/linear/mod.rs`,
  `is_trivial_forwarding_block`.

1. **The wrapper.** An op whose output the emission plan names in pieces
   (`rax = zext(eax)` is two 4-byte storages) lowers to several assignments,
   returned as one `PreHirStmt::Block` because the caller takes one statement
   per op. The pre-structuring passes that fold a temporary into its one
   reader do not look inside it, so a loop header kept three assignments its
   condition would have held, and the header was then copied. Experiment on
   `log_level_name`: unwrapping alone gives the 16-line `while`; relaxing the
   plan's `TempPreserved` bindings alone changes nothing.
2. **Dead store elimination.** `FISSION_NORM_DUMP`-style pass tracing on sshd
   `0x42b21`: the store `*(local_8_3 + 4) = rdx | x` is present before
   `dead_store_elimination` and absent after. `classify_base_object`
   (`memory/partition.rs`) classes any `Var` whose name starts `local_` as the
   stack slot. Here `local_8_3` is a variable holding a pointer.
3. **Slot aliases.** `collect_single_var_aliases` counted an assignment only
   when its right side was a variable, so `p = q` followed by `p = p + 16` in
   a loop left `p` "an alias of `q`"; the alias `slot_4 = q + 4` is a
   declaration initializer, evaluated once. With that corrected the base
   stays `p`, which `is_surface_stable_slot_display_base` accepts by name and
   which the initializer would read before the body assigns it.
4. **Forwarding blocks.** Found by reading the lost perfects of the first
   sweep of (1)-(3). `history_def_curr`'s builder output has `rdx = 8` in one
   arm block and `rdx = 5` in the other; the structured body has only the
   second. The arm is a copy, an extension and a branch, which
   `is_trivial_forwarding_block` classes by opcode as a block nothing is
   printed for, and `conditionals/short_circuit.rs` then steps over it. That
   is true of the legacy path, where a copy's reader looks the value up, and
   false under the plan, where the copy is the assignment.

## 3. Generality / Invariant Proof

```text
(a) Statements an op lowers to are statements of its block.
(b) A name the function assigns is a variable. A write through it goes where
    its value points, not to the slot the name came from.
(c) An alias of a variable is a name assigned exactly once, by a copy.
(d) A declaration's initializer runs before the body, so it may read no name
    the body assigns.
(e) A block that makes an assignment the plan emits is not a block that
    merely forwards.
```

- [x] No ISA, calling convention, function or address appears in any of them.
- [x] Synthetic tests state the statement shape only.

Shape repeats: (a) thirty rows in two optimisation-independent table-search
functions plus the sample below; (b) 151 more store statements on the `-O0`
sample and 9 on `-O2`; (c)/(d) declaration-initialized `slot_N` pointers fall
3,648 -> 3,334 (`-O0`) and 794 -> 680 (`-O2`) on the sample.

## 4. Risk And Ownership Check

- No new pass, metric or dependency. One shared helper,
  `collect_assigned_names`, in `analysis/defuse.rs`.
- (b) only withholds a deletion and (c)/(d) only withhold a rewrite; neither
  can remove code. What they can cost is a store that really was dead, or a
  slot alias that was sound, staying as plain pointer arithmetic.
- (b) is applied in the pass that deletes, not in `classify_base_object`,
  which also feeds memory heritage. The same misclassification is still
  there for that consumer; it is not known to print anything wrong.

## 5. Validation Matrix

- [x] Tests, each with a positive control made by breaking only the fix:

  | test | mutation that fails it |
  |---|---|
  | `a_definition_named_in_pieces_joins_its_block_as_statements` (`materialize/mod_tests.rs`) | the wrapper kept |
  | `dead_store_elimination_keeps_a_write_through_a_variable_named_after_a_slot` (with an in-test control: the same store with nothing assigning the name is still removed) | the assigned-name check removed |
  | `a_cursor_the_body_advances_gets_no_slot_alias` (control: `a_pointer_assigned_once_gets_a_slot_alias_rooted_at_its_source`) | only copies counted; and, separately, an assigned base allowed |
  | `a_block_of_copies_the_plan_names_is_not_stepped_over` (in-test control: the same block without the plan is still a forwarding block) | the plan check removed |

- [x] `cargo nextest run -p fission-pcode -p fission-midend-normalize
  -p fission-midend-structuring -p fission-decompiler`: 2,260 passed (debug),
  2,259 (`--release`). CI clippy line, `cargo fmt --all --check`,
  `scripts/check/owner_boundaries.sh`.

### Sample (16 binaries, against the dominating-write change alone, 10-minute render limit; changes (1)-(3), before (4))

| | `-O2` (7,165) | `-O0` (8,131) |
|---|---|---|
| functions whose text changed | 903 | 625 |
| call sites lost / gained (by call-instruction address) | 0 / 1 | 0 / 2 |
| `goto` in the changed functions | 9,354 -> 9,415 | 2,722 -> 2,721 |
| lines in the changed functions | 132,555 -> 137,259 | 63,657 -> 63,912 |
| memory store statements, all functions | 20,388 -> 20,397 | 29,055 -> 29,206 |

### Full sweep (same rows, caches off)

Against the dominating-write change alone:

| | `-O2` (`inlined`) | `-O0` (`unoptimized`) |
|---|---|---|
| rows scored on both sides | 24,177 | 31,986 |
| total distance | 614,467 -> 614,705 (+0.04%) | 391,284 -> 390,686 (-0.15%) |
| structure perfect | 7,410 -> 7,410 (2 new, 2 lost) | 12,993 -> 13,012 (43 new, 24 lost) |
| type / byte perfect | 624 / 500 -> 624 / 499 | 3,383 -> 3,399 / 780 |
| Union | 7,859 -> 7,859 | 15,145 -> 15,176 |
| rows better / worse | 271 (-1,532) / 273 (+1,770) | 109 (-1,115) / 97 (+517) |

All three changes together, against the baseline before them (`a4358919e`):

| | `-O2` | `-O0` |
|---|---|---|
| total distance | 611,354 -> 614,705 (+0.55%) | 392,543 -> 390,686 (-0.47%) |
| structure perfect | 7,409 -> 7,410 | 12,994 -> 13,012 |
| Union | 7,858 -> 7,859 | 15,146 -> 15,176 |
| rows better / worse | 457 (-3,614) / 655 (+6,965) | 229 (-3,320) / 171 (+1,463) |

The `-O2` run scored nine fewer rows than the earlier ones (24,177 of
24,186): nine large functions hit the 30-second render limit where three
usually do, during a sweep that shared the machine with other work. The
comparison is over the rows both sides scored.

Change (4) alone, measured as the difference between two sweeps that differ
only in it: 14 `-O0` texts changed (10 better, -67; 3 worse, +12; 4 new
perfects) and 5 `-O2` texts (2 worse, +6). The three worse rows that were read
(`normalize_filename_x`, `uconvert`, `bash_complete_filename_internal`) each
gained the arm that had been dropped.

Read, not inferred:

- The thirty table-search rows are perfect again.
- The 24 `-O0` perfects still lost: sixteen at GED 6 are the
  `a == 13 || a == 1` returns that were scored perfect while reading an
  unassigned temporary (`dev_ino_compare` read here, `is_directory` and
  `perm_related_errno` on the previous sweep; the rest by name only);
  `ssh_krl_file_contains_key` (five binaries) and `util_local_addr` used the
  wrong `errno` pointer and now use the right one with the two returns merged
  into one; `u32tochar` has its terminating store and `return` copied into
  three branches where it had one join -- a structural loss on output that
  was right before; `make_cond_command` was not read.
- No row lost a real callee against the baseline. The names that disappear
  are flag intrinsics (`__carry`, `__scarry`, `__sborrow`, `__popcount`).
- `-O2` distance is up 0.55% over the baseline with perfect up by one. The
  rows read from the worse side of the first two sweeps were restored code
  (`sshkey_drop_cert`: every `case` had lost its assignment); of the worse
  rows of this last step, most were already at or over the source's node
  count and grew, which is that same pattern, and they were not individually
  read.

**What this supports.** At `-O0` every measure moved the right way against
the baseline -- distance, perfect, Union, type -- and the lost perfects that
were read are corrections but for one. At `-O2` the distance is worse and
the perfect count flat; the explanation read on the largest rows is code that
used to be deleted, and that explanation was not checked on every row.

### Rebased on `main` (`26dff6bcc`, after #899-#903, #918 and #919 merged)

The branch rebased without conflict. Both sides rebuilt and swept back to
back on an otherwise idle machine; two rows hit the render limit on each
side of each config.

| all three changes, against `main` | `-O2` (`inlined`) | `-O0` (`unoptimized`) |
|---|---|---|
| rows scored on both sides | 24,188 | 31,989 |
| total distance | 616,516 -> 619,863 (+0.54%) | 391,603 -> 389,747 (-0.47%) |
| structure perfect | 7,484 -> 7,488 (5 new, 1 lost) | 13,105 -> 13,119 (43 new, 29 lost) |
| type / byte perfect | 622 / 501 -> 622 / 500 | 3,385 -> 3,400 / 792 |
| Union | 7,926 -> 7,930 | 15,199 -> 15,224 |
| rows better / worse | 454 (-3,542) / 642 (+6,889) | 223 (-3,139) / 160 (+1,283) |

The same shape as before the rebase. Of the 29 `-O0` perfects lost, the ones
not already listed above are `should_dereference` (three binaries) and
`all_digits`, both read: the old output returned an unassigned temporary on
one `case`, and dropped the upper bound of a digit test; the new output has
both right. `lseek_or_error` is the stale select the first proposal records.
Rows that lose a callee name other than a flag intrinsic lose a fixed-address
call that was a wrong devirtualization (`istrsenvisx`: `sub_26320()` becomes
a call through the pointer it was loaded from).

With #901 on `main`, sshd `0x42b21` prints its mask.

## 6. AI Review / Prompt Firewall

No external model prompt was used.

## 7. Review Notes

- Not fixed and seen on the way: sshd `0x42b21`'s loop is still a label and
  a `goto`. `u32tochar` (above). The emission path is still more
  verbose than the legacy path on straight-line code (`uVar0 = rsi - 1; rsi
  = (uint)uVar0;`), because the low piece of an `IntZExt` is not coalesced
  with its source. `classify_base_object`'s name-based stack class still
  feeds memory heritage. The stale select on call-clobbered registers
  (`tar lseek_or_error`) is untouched.
