# Proposal: a remembered path condition ends where the value it read can change

Status: implemented; `-O0` and `-O2` measured. A soundness fix, not a quality
claim: see section 5. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Not one row: a repeating shape, read on the full local DecBench configs
(`unoptimized`: 31,987 scored functions; `inlined` (`-O2`): 24,186), baseline
the stack-protector branch at `a4358919e`, caches disabled.

`condexe` folds a test whose outcome is already known on the path that
reaches it: inside `if (c) { ... }` a second `if (c)` is replaced by its
`then` arm. It remembered `c` until a *variable* `c` reads was assigned, and
for nothing else. Four functions, four programs, both optimisation levels:

| function | config | what the baseline prints | what the binary does |
|---|---|---|---|
| `sshpkt_disconnect` (openssh, 4 binaries) | `-O0` | three calls in a row, each storing its result in `r`, then one `if (r)` | tests `r` after each call and returns on the first failure |
| `check_ocsp_response` (gnutls, 2 binaries) | `-O0` | `ret = import(...); ret = check_crt(...); if (ret >= 0) ...` | tests `ret` after the import and exits; the `ret < 0` arm after `check_crt` prints one of two messages |
| `BZ2_bzRead` (bzip2) | `-O2` | `*bzerror = x;` unguarded in six places, behind a label that is also entered with `bzerror == NULL` | `if (bzerror != NULL) *bzerror = x;` |
| `init` (dash) | `-O2` | `while (1) { ...; continue; }` with no way out | leaves the loop when the next `*envp` is null |

- Failure category: a decision the program makes is missing from the output.
  Two forms: a guard deleted (the guarded code runs always), and a loop exit
  deleted (the loop is printed as endless).
- The shape is not rare for its size: the change alters 26 scored `-O0`
  functions (18 names, 10 projects) and 37 `-O2` functions (20 names, 12
  projects).

Why the output is wrong, without a metric: `r = f(); if (r) return r;
r = g(); if (r) return r;` and `r = f(); r = g(); if (r) return r;` are
different programs. The second calls `g` after `f` failed.

How it was found: not from a score. A separate change (making a call to a
no-return function end its block) moved `rest` in `if (c) { exit(); } rest`
under the `else`, and a second `if (c)` inside `rest` -- behind a store that
changes what `c` reads -- vanished with its call. The call-site coverage
comparison named the function; a per-pass probe named `condexe`. The same
probe on the unchanged baseline showed the fold firing there too.

## 2. Owner Proof

- Owner: `crates/fission-midend-normalize/src/cleanup/condexe.rs`,
  `fold_conditions`. It is the only place that keeps the `true_conds` /
  `false_conds` lists and the only reader of them.
- The lists are carried through a statement list and into nested bodies. The
  one existing way a condition left the list was `invalidate_variable`, called
  for each variable the current statement assigns.
- Three ways a remembered condition stops being true that this did not cover:
  1. **Memory.** The condition reads memory (`*slot == 0`, a field, a stack
     slot printed as a load) and a later statement stores to memory or calls.
     `-O0` code keeps locals in stack slots, so `r = f(); if (r)` is a store
     and a load, not an assignment to a name.
  2. **A label.** Code behind a label is reached by every `goto` to it, from
     anywhere in the function. The conditions collected on the fall-through
     path say nothing about those entries.
  3. **A condition with an effect.** `if (f()) { if (f()) ... }`: the two
     calls are two calls. Recording the first as "known true" deletes the
     second.
- Not the owner: the printer and structuring only show what this pass left.
  The builder is not the owner either -- the tests exist in the tree that
  enters this pass (pass-level probe: present before, absent after).

## 3. Generality / Invariant Proof

One invariant: **a remembered condition may be used at a statement only if
every path that reaches the statement passed the test and nothing since could
have changed a value the condition reads.** The pass enforced the second half
for named variables only, and the first half not at all.

Rules, all in `fold_conditions`:

1. After each statement, if it may write memory (a store, a call -- the
   existing `stmt_may_write_memory`), drop every remembered condition that
   reads memory (`expr_reads_memory`). No alias analysis: any write ends any
   memory-reading condition.
2. After each statement, if it is a label or holds one at any depth, drop
   every remembered condition.
3. A condition with a side effect (`expr_has_side_effects`) is not recorded
   for the arms below it.
4. Entering a loop, the conditions carried into the body are filtered by what
   the *whole body* may change (rules 1 and 2 applied to every body
   statement first), because the body's later statements run before its
   earlier ones on every iteration after the first. The existing code already
   did this for assigned variables.

No function name, address, compiler or ISA appears. The rules are stated over
the statement tree; rule 1 is the same on any target whose locals live in
memory.

What is deliberately coarse:

- Rule 1 has no notion of "which memory". A store to an unrelated global ends
  a condition on a stack slot. A sound refinement needs a may-alias answer the
  pass does not have.
- Rule 2 drops conditions at every label, including a label reached only from
  inside the arm that established the condition. Keeping those would need
  every `goto` to the label to lie inside the arm and in front of the label,
  with no backward jump able to run a later statement first. No row read for
  this change shows a test kept only for this reason; see section 5.

## 4. Risk And Ownership Check

- The change only removes folds. Every fold the pass still performs it
  performed before, so the pass cannot produce a statement it did not produce
  on the baseline; it can only leave a test in place.
- Downstream passes see more `if`s. Read on the changed rows: no call site is
  lost in either config (callee-name comparison: 0 of 63 changed rows lose a
  name), and the type and byte scores do not move on any row.
- No new pass, helper crate, metric or dependency. Three existing predicates
  are reused; one small one is added (`stmt_defines_a_label`).
- Reference: Ghidra's conditional-execution rule compares SSA values, so
  "the same condition" is the same value by construction: a load behind a
  store is a different value from the load in front of it, and a join has a
  phi. This pass works on a statement tree with no versions, so those two
  boundaries have to be stated explicitly. (Stated from the published
  algorithm; the local `vendor/` tree was not readable in this checkout, so
  the source was not re-read for this change.)

## 5. Validation Matrix

- [x] Targeted tests in `cleanup/passes_tests.rs`, each checked by breaking
  the line it guards and confirming it fails (the positive-control test
  `condexe_folding_drops_a_test_already_decided_on_the_way_in` proves the
  fixture folds when nothing intervenes, so the "keeps" tests are not
  vacuous):

  | broken | failing test |
  |---|---|
  | rule 1: memory writes ignored | `condexe_folding_keeps_a_test_of_memory_written_since_the_first`, `..._of_memory_behind_a_call`, `..._of_memory_the_loop_around_it_writes` |
  | rule 2: labels ignored | `condexe_folding_keeps_a_test_behind_a_label` |
  | rule 2: a label nested in an `if` not seen | `condexe_folding_keeps_a_test_behind_a_label` |
  | rule 3: a calling condition recorded | `condexe_folding_keeps_a_second_call_in_a_nested_condition` |
  | rule 4: the loop body's writes ignored | `condexe_folding_keeps_a_test_of_memory_the_loop_around_it_writes` |

- [x] Crate gate (`fission-midend-normalize`, `fission-midend-structuring`,
  `fission-pcode`): debug 2,148 pass, release 2,147 pass.
- [x] CI lint line (`cargo clippy --workspace --locked -- -D warnings
  -A clippy::style -A clippy::complexity`), `cargo fmt --all --check`,
  `scripts/check/owner_boundaries.sh`.
- [x] Full local DecBench configs, caches disabled, same harness and GED
  backend for both sides, same scored rows on both sides (31,987 and 24,186;
  no row dropped):

  | | `-O0` baseline | `-O0` this change | `-O2` baseline | `-O2` this change |
  |---|---|---|---|---|
  | total structure distance | 392,625 | 392,507 | 616,056 | 616,066 |
  | structure perfect | 12,994 | 12,996 | 7,409 | 7,409 |
  | type perfect | 3,383 | 3,383 | 624 | 624 |
  | byte perfect | 780 | 780 | 500 | 500 |
  | Union | 15,146 | 15,148 | 7,858 | 7,858 |
  | functions whose text changed | | 26 | | 37 |
  | score better / worse (rows) | | 15 / 11 | | 9 / 19 |
  | score better / worse (distance) | | -181 / +63 | | -91 / +101 |
  | new / lost perfect | | 2 / 0 | | 0 / 0 |

  **This is not a quality result.** `-O2` distance rises by 10 and more
  `-O2` rows score worse than better. The claim is only that the listed
  outputs were wrong and are no longer.

- [x] Changed output read, not only scored. Every `-O0` row that scores
  worse and the `-O2` rows named here were opened:

  | row | score | what changed |
  |---|---|---|
  | `sshpkt_disconnect` x4 (`-O0`) | 13 -> 19 | the two early returns are back; the source writes them as one `\|\|` chain, so nested `if`s cost more than the wrong straight line did |
  | `get_link_kind` (`-O0`) | 19 -> 35 | baseline replaced `if (dlh) { snprintf; dlsym; ... }` after a fresh `dlopen` result was stored with `rax = 0`; the block is back |
  | `append_history` (`-O0`) | 11 -> 14 | `p = f(); if (!p) { errno...; goto fail; }`: the `if` is back |
  | `sparse_copy` x3, `execute_in_subshell` (`-O0`) | +3, +10 | a test of a stack slot behind stores is back |
  | `check_ocsp_response` x2 (`-O0`) | 32 -> 0 | both error exits are back |
  | `emfat_init_entries` (`-O0`, `-O2`) | 16 -> 17 | `if (!i) return 0;` behind a label is back |
  | `BZ2_bzRead` (`-O2`) | 171 -> 115 | `if (bzerror)` back around six stores |
  | `ubifs_tnc_locate` (`-O2`) | 71 -> 85 | a branch that picks between two callees is back; baseline called one of them unconditionally |
  | `init` (dash), `sshkey_advance_past_options` x5, `mux_client_read`, `database` (`-O2`) | 0 to +2 | `continue;` at the end of `while (1)` becomes `if (c) continue; break;` |
  | `gzclearerr` x5, `freerrset` x3 (`-O2`) | +8, +4 | `if (p) free(p)`-style guards are back; see the note below |
  | `test_deflate` x3 (`-O2`) | 17 -> 17 | a `continue` is printed as a `goto` to a label in front of the loop; same control flow, uglier |

  Note on `gzclearerr`, `freerrset`, dash `init`: the restored test prints as
  `if (param_1)` where the binary tests a value loaded into the same register
  later (`state->msg`, the next `*envp`). That is a naming defect in front of
  this pass -- a register reused for a new value keeps its parameter name
  without a printed assignment -- and it is why the baseline folded these
  tests: by name, the condition had not changed. The test is real; its
  operand is misnamed. Not fixed here.

- [x] Sample cross-check outside the scored rows: 16 binaries (ten at `-O2`,
  six at `-O0`, from bash, dash, dpkg, gzip, iproute2, sshd, tar, sort,
  certtool and useradd), about 14 `-O2`
  and 7 `-O0` functions change. Larger differences seen under machine load
  were rerun alone and are identical.

## 6. AI Review / Prompt Firewall

No external model prompt. The defect was found by a call-site coverage
comparison and a per-pass probe, not from benchmark rows; the rows above were
read after the fix to check it.

## 7. Review Notes

- A fold the baseline made that is now refused can leave a test whose
  outcome a reader can see is fixed. None was found among the rows read, but
  rule 1 and rule 2 are both coarser than they need to be, and the place to
  recover precision is a versioned (SSA) form, not more special cases here.

## Not fixed

- A register reused for a new value keeps the parameter's name with no
  assignment printed (`if (param_1)` for a test of a freshly loaded value).
  Seen in `gzclearerr`, `freerrset`, dash `init`. Builder/naming, in front of
  this pass.
