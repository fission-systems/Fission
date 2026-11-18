# Proposal: only a straight-line tail is copied to more than one place

Status: implemented; results below. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Not one row: a repeating shape, read on the full local DecBench configs
(`unoptimized`: 31,987 scored functions; `inlined` (`-O2`): 24,186), baseline
`origin/main` at `1c10edab0`, caches disabled.

`tail_dup` copies a function's shared tail into each place that reaches it. It
admitted any tail of at most six statements, including one that contains an
`if` or a loop. Three functions, three programs, both optimisation levels:

| function | config | what `main` prints | what the source has |
|---|---|---|---|
| `*_tree_RB_REMOVE` (openssh, 16 rows in 6 binaries) | `-O0` | `if (!color) REMOVE_COLOR(...); return old;` in four places | once, under `color:`, reached by `goto color;` (`sys/tree.h`) |
| `ifs_firstchar` (bash) | `-O0` | `if (lenp) *lenp = len; return ret;` at the end of both arms | once, after the `if`/`else` |
| `sdram_command` (libopencm3, 3 binaries) | `-O2` | a `do { } while (busy)` wait, a store and the `return` in eight places | once, after the `switch` |

- Failure category: duplicated decision. The copies are correct; the defect is
  that code the program has once is printed N times.
- The shape is not rare: the change below alters 166 `-O0` and 228 `-O2`
  functions, 80 and 119 distinct function names over 18 and 26 projects.

Why the output is wrong, without a metric: a reader who meets the same `if`
four times has to compare the four copies to learn they are one piece of code,
which the binary and the source both already say. A compiler merges a
function's `return`; it does not merge four separately written `if`s, so
"un-merging" a tail that decides something restores nothing the source had.

## 2. Owner Proof

- [x] Structuring (cleanup): `crates/fission-midend-structuring/src/cleanup/tail_dup.rs`.

The module documents its reference as angr's `ReturnDuplicatorHigh`. That pass
duplicates only what `_is_simple_return_graph` accepts: a chain of
single-successor blocks holding nothing but assignments, side effects and the
`return` ("will only duplicate return-only blocks"). Fission's two admission
points -- the gotoless split (`split_fallthrough_return_tails`) and the
goto-driven clone (`duplicable_region_at`) -- bound the tail by statement count
only, so a tail with a branch passes.

Evidence that the copies come from this pass and nothing upstream: with the
admission below, the three anchor functions print the tail once and nothing
else in them changes.

```text
# main                                # this change
if (!local_30) { ...; return x; }     if (!local_30) { goto block_ddb5; }
...                                   ...
if (!local_30) { ...; return x; }     block_ddb5:
...                                   if (!color) { REMOVE_COLOR(...); }
if (!color) { REMOVE_COLOR(...); }    return x;
return x;
```

## 3. Generality / Invariant Proof

```text
A region that would end up in more than one place must be straight-line: no
statement in it has a child sequence (if / loop / switch), looking through
plain blocks.

One branch does not count: the stack-protector check. It is recognised by the
callee name the existing stack-protector pass already uses, in the shapes it
has when tail_dup runs (two-armed, the failing arm being the handler call and
at most a return; or the settled one-armed guard). It is the compiler's, it
sits in front of every return of a protected function, and the last cleanup
pass deletes it -- so a return behind it is still "the function's return".

A single-reference, non-fallthrough region that is moved (not copied) is
exempt: it still appears once.
```

The rule reads only the statement tree. No ISA, calling convention, compiler
or function identity is consulted; the anchors are x86-64 `-O0` and ARM `-O2`.

- [x] Not gated on an ISA / calling convention.
- [x] No ISA data involved.
- [x] Synthetic tests state the statement shape only.

Comparable coverage:

- Similar shape 1: `ssh_hmac_init` (openssh, 6 binaries): the closing
  `if (copy_state(...) < 0) return -1; return 0;` is no longer copied in front
  of the `if (key != NULL)` body.
- Similar shape 2: `read_byte` (gzip), `__xfrm_algo_print` (iproute2; the source
  writes `goto fin;`), `cleanUpAndFail` (bzip2 `-O2`; the `setExit(); exit();`
  ending printed five times).
- Synthetic invariant tests: nine in `tail_dup.rs`, two in `stack_protector.rs` (section 5).

## 4. Risk And Ownership Check

- Existing owner: `tail_dup` (admission) and `stack_protector` (what the canary
  check looks like). Both extended in place; no new pass.
- Shared substrate: [x] none; owner-local rule on the statement tree.
- Why the stack-protector exemption is needed -- measured, not assumed. Without
  it the same rule loses on both configs, and every loss is a canary tail:

  | variant vs `main` | `-O0` distance / perfect | `-O2` distance / perfect |
  |---|---|---|
  | split only, no exemption | +301 / -25 | +737 / -26 |
  | split + clone, no exemption | -856 / -15 | +2,971 / -27 |
  | split + clone + exemption (this change) | -1,444 / +18 | -1,675 / +1 |

  The first attempt at the exemption reused `is_stack_protector_guard` and did
  nothing: that predicate matches the settled one-armed guard, and when
  `tail_dup` runs the check is still two-armed. `stack_protector_pass_arm`
  matches both.
- Interaction: `sink_spans_into_if_arms`, `invert_forward_guard_gotos` and
  `relocate_jump_only_joins` run in the same round and now see a `goto` where
  they saw a copy. Their own admission is unchanged.
- New owner-to-owner dependency: [x] none new (`tail_dup` -> `stack_protector`,
  both in `cleanup`, `pub(super)`).
- Telemetry impact: none.
- Must not change: a tail that is only assignments, calls and the `return`
  (tested); the canary epilogue of a protected function (tested, and measured:
  0 lost perfects).

## 5. Validation Matrix

- [x] Targeted invariant tests: `cargo nextest run -p fission-midend-structuring`
  -- 335 pass. Each new test was checked against the change it guards by
  breaking that one line and confirming that test, and only the intended
  tests, fail:

  | broken | failing test |
  |---|---|
  | split admits a branch | `gotoless_split_leaves_a_tail_that_decides_in_one_place` |
  | clone admits a branch | `clone_leaves_a_tail_that_decides_in_one_place` |
  | canary check never recognised | `gotoless_split_sees_through_...`, `clone_sees_through_...`, `the_pass_arm_is_found_...` |
  | moved region treated as copied | `a_single_reference_tail_that_decides_is_still_moved` |
  | pass arm not inspected | `a_branch_inside_the_canary_pass_arm_still_counts` |
  | failing arm may do more work | `an_arm_that_does_more_than_fail_is_not_the_check` |

- [x] Crate-level gate: `cargo nextest run -p fission-midend-structuring -p fission-pcode`,
  debug and `--release`; `cargo clippy --workspace --locked -- -D warnings -A clippy::style -A clippy::complexity`;
  `cargo fmt --all --check`.
- [x] Full-config measurement (not the scored sample set), same rows, caches off:

  | | `-O0` main | `-O0` change | `-O2` main | `-O2` change |
  |---|---|---|---|---|
  | total structure distance | 394,148 | 392,704 | 621,358 | 619,683 |
  | structure perfect | 12,976 | 12,994 | 7,378 | 7,379 |
  | Union / type / byte perfect | 15,131 / 3,383 / 780 | 15,146 / 3,383 / 780 | 7,827 / 624 / 500 | 7,828 / 624 / 500 |
  | rows better / worse | | 112 (-1,796) / 40 (+352) | | 178 (-1,946) / 27 (+271) |
  | new / lost perfect | | 18 / 0 | | 1 / 0 |

  A second axis that does not see CFG shape -- recompile and compare bytes:
  total byte distance 2,162,792 -> 2,160,155 (`-O0`; 96 better, 8 worse) and
  1,404,115 -> 1,404,001 (`-O2`; 68 better, 35 worse); compilable count
  unchanged (17,170 / 11,531).

  What the score does not show, in the changed functions only: lines
  39,192 -> 37,701 (`-O0`) and 32,727 -> 31,052 (`-O2`); `goto`s
  1,429 -> 1,695 and 1,732 -> 2,063. Each added `goto` replaces a copy of a
  tail that decides something.

- [x] Sample-set gate (the scored 233 rows): perfect 67 -> 67, total distance
  7,603 -> 7,611. One row moves: gnutls `print_list` (105 -> 113), kind 4
  below. So on the scored rows this change is a small loss; the gain is on the
  rows the benchmark does not score.

Rows that got worse were read, not just counted. Four kinds:

1. The source reaches the tail by `goto` and the output now does too, but the
   distance rises: `sshauthopt_merge` (+24; the output now has the source's
   `alloc_fail:` / `fail:` pair), `get_token` (`goto after_token;`),
   `globextend`.
2. The source falls into the tail from an `if` the structurer did not recover,
   so the one copy is behind a `goto`: `f_open`, `echo::main`, `realpath::main`.
3. The source itself repeats the tail through a macro (`BZ_SETERR` in
   `BZ2_bzWrite`, +52). The old copies matched it; the single copy does not.
4. The canary check in a shape neither this change nor the stack-protector
   pass recognises -- `if (!diff) { return v; } __stack_chk_fail();` -- is now
   printed once behind a `goto` instead of at every return: zlib `inflate`,
   `start_header`, `prime2_p`. At `-O2` the same shape improves 47 rows and
   worsens 12. See "Not fixed".

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

- `if (!diff) { return v; } __stack_chk_fail();` survives to the output: the
  stack-protector pass removes the guard only when the handler call is inside
  the `if`. That is a gap in `stack_protector.rs`, independent of this change.
- A label can be printed directly before a closing brace (`block_x:` then `}`),
  which C before C23 rejects. `main` already prints it in 74 (`-O0`) and 118
  (`-O2`) functions; this change adds three (`sdram_command`).
- `MAX_DUPLICATED_STMTS` (160) is unchanged; its doc comment already says
  nothing spends it.
