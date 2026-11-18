# The emission plan is declined for names it does not own

Date: 2026-11-18. Status: **implemented and measured; not a quality claim,
and not recommended for merge as it stands** -- see section 5. Filled
from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).
Follows [`2026-11-18-dominating-write-is-not-the-value-read.md`](2026-11-18-dominating-write-is-not-the-value-read.md),
whose conclusion was that a joined read needs every write reaching the merge
emitted under one name.

## 1. Baseline Row Anchor

That naming already exists: the scalar SSA emission plan
(`builder/ssa_emission.rs`, [`2026-10-05-ssa-value-emission.md`](2026-10-05-ssa-value-emission.md))
gives a phi and its operands one name and places the edge copies. It is
all-or-nothing per function, and on a 16-binary sample (ten `-O2`, six `-O0`;
15,301 functions) it is accepted for very few of the functions it has a plan
for:

| | `-O2` (7,171) | `-O0` (8,130) |
|---|---|---|
| no admissible storage | 4,505 | 7,552 |
| CFG precondition fails | 642 | 28 |
| plan built, then declined by the initialization check | 1,620 | 387 |
| plan accepted | 404 | 163 |

Functions the plan is declined for stay wholly on the legacy path, and that is
where the dominating-write defect lived: of the functions that fix changed at
`-O2`, 595 were declined plans and 22 were accepted ones.

Three declined functions, read:

| function | config | what is printed | what the binary does |
|---|---|---|---|
| `find_special_var` (bash) | `-O0` | `r12 = d; if (!r12) { rax = strcmp(..); r12 = rax; } r = r12 ? d : rax; if (!r)` | tests `r12` after the branch; the select re-reads `r12` after the arm assigned it |
| dash `0x4d30` | `-O2` | a list walk that returns `xVar138`, the head, from inside the loop | returns the previous node, which the loop updates each round |
| gzip `0x44a0` | `-O2` | correct either way | (control: the plan changes the names and nothing else) |

- Failure category: a merged register read resolved by legacy merge machinery
  although a validated single name for it existed.

Why the output is wrong, without a metric: it tests a value the branch has
already overwritten, and returns the first element of a list where the program
returns the one before the match.

## 2. Owner Proof

- [x] Builder/materialize: `builder/ssa_emission.rs`
  (`ssa_emitted_bindings_are_initialized`), `materialize/cross_block.rs`
  (`synthesize_explicit_merge_bindings_for_block`),
  `materialize/accumulator_inputs.rs`
  (`rewrite_block_entry_accumulator_input_expr`).

Two separate reasons the plan was thrown away, both seen on
`find_special_var` with `FISSION_PREVIEW_DIAG`:

1. **Ownership compared by exact key.** The plan owns `r12` as two 4-byte
   storages (`0xa0/4`, `0xa4/4`), because `r12 = zext(r12d)` writes both
   halves. The legacy merge-select synthesis skips a storage the plan owns by
   looking up the op's *output* key, `0xa0/8`, which is not one of them. It
   therefore built `xVar110 = uVar4 != 0 ? uVar4 : uVar5` over the plan's own
   names, and `rewrite_block_entry_accumulator_rhs_with_live_gpr` then
   replaced the plan's correct read (`r12`) with that select's binding. The
   select reads `uVar5`, assigned only in the arm, so the initialization check
   declined the whole plan.
2. **The check answers for every temporary.** With (1) fixed the plan was
   still declined, at the loop head, for `xVar129 = xVar125 * cf`: the x86
   shift semantics read the carry flag, which this function has not yet
   written on the entry path. `cf` is not a name the plan emits. The design
   note for the check says "check every emitted temporary read"; the
   implementation tracks `self.temps`, which is every temporary of the
   function.

## 3. Generality / Invariant Proof

Generalized rules:

```text
(a) A storage belongs to the plan if any part of it is emitted. A wider or
    narrower view of an emitted storage is the same value, and legacy merge
    machinery must not answer for it a second time.
(b) The plan must leave no temporary read before it is assigned -- except a
    binding named after a register the plan does not emit. That name is the
    register; read before the function writes it, it holds what the caller
    left there. This is the legacy path's own reading of a live-in and the
    plan neither introduces nor changes it.
```

(b) keeps three things tracked: every name the plan emits; every numbered
temporary, whoever emitted it; and a register name any of whose accesses
overlaps an emitted storage (a read that still prints `r12` when the plan
carries `r12`'s value under another name would be reading a variable the plan
never assigns).

- [x] Not gated on a calling convention or ISA: overlap of storage keys, and
  "is this binding's name a register name" asked of the register namer.
- [x] ISA data comes from the SLEIGH register map (`sla_hw_name`) and the
  cspec return registers.
- [x] Synthetic tests state the CFG/dataflow shape only.

Shape repeats: declined plans are 1,620 of 7,171 `-O2` and 387 of 8,130 `-O0`
sample functions across all sixteen binaries.

## 4. Risk And Ownership Check

- No new pass, metric or dependency. Two existing skip conditions change from
  exact key to overlap; one validator input narrows.
- The risk is exposure: the emission path has been printing ~4% of functions
  and now prints ~10% of them. Its output is more verbose than the legacy
  path's for the same code (see section 5), and any defect it has is now
  visible three times as often.
- Not relaxed, deliberately: a numbered legacy temporary read before
  assignment still declines the plan. Relaxing that too takes acceptance from
  1,051 to 1,838 on the `-O2` sample, but numbered names are not comparable
  between a lowering with the plan and one without, so "the plan did not
  cause it" cannot be shown per name.

## 5. Validation Matrix

- [x] Targeted invariant tests, three in `materialize/mod_tests.rs`, each
  with a positive control made by breaking only the fix:

  | mutation | tests that fail |
  |---|---|
  | ownership back to the exact storage key | all three |
  | merge-select synthesis ignores ownership | all three |
  | accumulator rewrite ignores ownership | `a_register_name_that_touches_emitted_storage_stays_checked` |
  | every temporary tracked again | all three |
  | every register name exempt, overlapping or not | `a_register_name_that_touches_emitted_storage_stays_checked` |
  | only the plan's own names tracked | `a_live_in_register_..`, `a_register_name_that_touches_..` |

- [x] `cargo nextest run -p fission-pcode -p fission-midend-normalize
  -p fission-midend-structuring -p fission-decompiler`: 2,255 passed (debug),
  2,254 passed (`--release`).
- [x] CI clippy line, `cargo fmt --all --check`, `scripts/check/owner_boundaries.sh`.

### Sample (16 binaries, same build but for this change, 10-minute render limit)

Plan outcome, before -> after:

| | `-O2` | `-O0` |
|---|---|---|
| plan accepted | 404 -> 1,059 | 163 -> 354 |
| declined by the initialization check | 1,620 -> 969 | 387 -> 197 |

Output, compared with temporaries' numbers masked (bash `-O0` `yyparse`
excluded: it takes seven minutes alone on either build):

| | `-O2` (7,165) | `-O0` (8,130) |
|---|---|---|
| functions whose text changed | 663 | 149 |
| call sites lost / gained (by call-instruction address) | 0 / 1 | 0 / 0 |
| locals read but never assigned | 10,134 -> 9,999 | 3,914 -> 3,912 |
| `x = c ? a : b;` statements | 1,850 -> 1,795 | 3,172 -> 3,131 |
| of those, `c` reads a name the branch just assigned | 122 -> 121 | 296 -> 279 |
| lines in the changed functions | 85,955 -> 94,096 | 16,053 -> 18,876 |
| `goto` in the changed functions | 6,132 -> 6,250 | 1,010 -> 1,174 |

Read, not inferred:

- `find_special_var` is right again: `r12 = ..; if (!..) { ..; r12 = ..; }
  if (!r12)`.
- dash `0x4d30` returns the loop-updated node (`r12`) where it returned the
  list head.
- sshd `0x42b21` (`-O0`): the old output had lost a mask
  (`field_4 = field_4`); the new one computes it. It is also 51 lines longer,
  has three `goto`s and no recovered loop -- see below.
- sshd `0xa4f30` (`-O2`): eleven nested `if`s with the cleanup tail copied
  onto each exit became a chain of `if (r12) goto out;` into one cleanup
  block. More `goto`s, one copy of the tail.

What it costs, and this is not small: **the emission path prints more than
the legacy path for the same code.** Every emitted definition is a statement,
so `esi = esi - 1; rsi = zext(esi)` prints as `uVar0 = rsi - 1; { rsi =
(uint)uVar0; }` where the legacy path printed `i--`, and a load followed by
two width changes prints as three assignments. In gzip `0x44a0` that is the
whole difference (19 -> 31 lines, same semantics). Where the extra statements
land in a loop header the loop is not recovered (sshd `0x42b21`). 871 of the
added lines are bare `{`/`}` around a multi-piece definition.

The stale select this work started from is only partly gone. It predates the
dominating-write change -- the unchanged baseline has 113 (`-O2`) and 250
(`-O0`) on this sample -- and what remains is on storage the plan cannot own:
`rax` and the other call-clobbered registers in a function that makes calls
(`tar lseek_or_error` is unchanged and still wrong).

### Full sweep (against the dominating-write change alone, same rows, caches off)

| | `-O2` (`inlined`) | `-O0` (`unoptimized`) |
|---|---|---|
| rows scored on both sides | 24,186 | 31,986 |
| total distance | 619,173 -> 620,049 (+0.14%) | 391,284 -> 390,933 (-0.09%) |
| structure perfect | 7,410 -> 7,411 (2 new, 1 lost) | 12,993 -> 12,949 (6 new, 50 lost) |
| type / byte perfect | 624 / 500, unchanged | 3,383 -> 3,380 / 780 |
| Union | 7,859 -> 7,860 | 15,145 -> 15,127 |
| rows better / worse | 262 (-1,313) / 324 (+2,189) | 44 (-734) / 82 (+383) |
| function texts changed | 1,876 | 406 |

Against the baseline before both changes (`a4358919e`): `-O2` distance
616,056 -> 620,049 with perfect 7,409 -> 7,411; `-O0` distance 392,543 ->
390,933 with perfect 12,994 -> 12,949.

The 50 lost `-O0` perfects, read:

- **30 are one shape and a real regression.** A table search --
  `log_level_name` and `log_facility_name` (twelve openssh binaries each),
  `fmt_multistate_int`, `lookup_opcode_name` -- was `while (tbl[i].name) { if
  (..) return ..; i++; }` and is right in both outputs. With the plan
  accepted, the loop header's address computation is three emitted
  assignments instead of one condition, the header is copied to the entry and
  the latch, and the loop comes out as a label and a `goto`.
- **20 were scored perfect while wrong.** `is_directory`, `dev_ino_compare`,
  `perm_related_errno` and others: `if (a != 13) t = a; return t != 1 ? 0 :
  1;` reads `t` unassigned when `a == 13`. They are now `a == 13 || a == 1`
  spelled as nested `if`s with three returns, which is correct and costs 10.

The largest `-O2` loss is `sshkey_drop_cert` in twelve binaries (+40 each,
480 of the 2,189): every `case` of its switch used to jump to the join with
its assignment gone; the cases now assign 0, 1, 3, 8, 10, 12 and the default
2. New perfects include `find_special_var` and `compress_block` (five zlib
binaries).

**Conclusion.** The plan's names are right where the legacy path's were not,
and the measurement is again mixed: restored code and corrected values on one
side, a lost loop on the other. Not a quality claim, and not recommended for
merge as it stands. The lost loop has one cause -- the emission path makes a
statement of every definition it names, including one read exactly once in
the next op -- and that is what to fix before the plan is accepted this
widely.

## 6. AI Review / Prompt Firewall

No external model prompt was used.

## 7. Review Notes

- Not fixed and seen on the way: the low piece of an `IntZExt` is never
  coalesced with its source (`scalar_ssa.rs` copy coalescing handles `Copy`
  and `Cast` with equal piece counts), which is where `uVar0 = rsi - 1; rsi =
  (uint)uVar0;` comes from; a multi-piece definition is wrapped in a
  `PreHirStmt::Block` that prints as bare braces; the plan cannot own a
  call-clobbered register in a function that makes calls, which is where the
  remaining stale selects are.
- bash `-O0` `yyparse` takes about seven minutes alone on either build and is
  a 30-second-limit failure in every sweep; its output is identical with and
  without this change.
