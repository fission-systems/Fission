# A flag is a snapshot, and SLEIGH's temporary flags are flags

Filled from `docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`.

## 1. Baseline

Thumb `subs r0, #0; it ne; movne r0, #1` is how a Cortex-M compiler returns a
boolean. Fission printed the predicate as a branch on a named flag and kept the
flag writes inside it:

```c
uint isSomeStatEnabled(void)          // betaflight_STM32F405, -O2
{
    r0 = *(uint *)(tmp_8052aa4 + 20);
    tmpzr = r0 == 0;
    if (!tmpzr) {
        r0 = 1;
        tmpzr = tmpov;
    }
    return r0;
}
```

Measured on the full `inlined` (`-O2`) DecBench config, 26,012 functions, on
`main` at 1c10edab0:

| | functions |
|---|---|
| mention `tmpng` / `tmpzr` / `tmpcy` / `tmpov` | 1,511 |
| branch on one of them | 664 |

They sit in nine projects (betaflight 496, crazyflie 330, cleanflight 248,
u-boot 220, libopencm3 112, chibios 44, nuttx 39, freertos 15, riot-os 7). The
shape is not an optimisation artefact: an earlier `unoptimized` (`-O0`) run of
the same pipeline has it in 9,027 of 33,868 functions across eight projects.

Ranked by excess structure distance over Kuna on that config, the predicated
`if` was the largest single class (12.6x).

## 2. Owner

Normalize, in three places, found in this order.

1. **`recovery/flag_recovery.rs::flag_family`.** ARM's SLEIGH computes the
   condition codes into `tmpNG`/`tmpZR`/`tmpCY`/`tmpOV` and copies them into
   `NG`/`ZR`/`CY`/`OV` only for the flag-setting forms. The family listed the
   architectural names and not the temporaries, so neither the substitution of
   a flag into its condition nor the dead-flag cleanup looked at them.
2. **`arith/conditional_move.rs`, the default/override merge.** With the flag
   gone the body is `x = d; if (x != 0) x = 1;`. The merge turned every
   `x = d; if (c) x = o;` into `x = c ? o : d`, which evaluates `c` *before*
   the default is stored. When `c` or `o` reads `x` that is a different
   program. It was already wrong on `main`, on x86:

   ```c
   rax = rax ? rax + 1 : strrchr(param_1, 47);     // sysvinit `mybasename`
   ```

3. **`recovery/flag_recovery.rs`, the substitution itself.** Reading the
   changed output of (1) turned up a condition that tested a register the
   previous statement had just replaced. The walk records `flag -> defining
   expression` and substitutes the expression at each later use; nothing
   removed a record when one of the expression's operands was written. The
   same hole was open for `zr` and the x86 flags before this change:

   ```c
   r12 += param_2;
   if (__scarry(r12, param_2)) {                   // coreutils `dd`, on main
   ```

   (1) would have widened it to roughly 3,000 more flag writes, so it is fixed
   first.

## 3. Invariant

- **A flag definition is a snapshot.** Substituting its expression at a use
  re-evaluates it there. That is the flag's value only while nothing the
  expression reads has been written: not a variable it names, not memory if it
  loads, not the flag itself on some path. A statement list containing a label
  can be entered without passing the definition at all. A loop body and a
  switch arm can run after themselves, so what they write is in effect at
  their own entry. A definition that calls is not re-evaluated.
- **The whole-function single-definition fallback has no position**, so it
  takes only definitions that read nothing the function ever assigns.
- **`x = d; if (c) x = o;` is `x = c ? o : d` only when the reordering is
  invisible**: `c` and `o` do not read `x`, `d` has no effect to lose, and `c`
  has none that could change what `d` reads.
- **`x = d; if (x != 0) x = 1;` is `x = (d != 0)`.** `d` is still evaluated
  exactly once, so this needs none of the above.
- **A flag is scratch.** An instruction writes it whether or not anything
  tests it; its value matters only where something observable reads it. A flag
  an instruction carries through itself (`of = keep ? of : new`) has a reader
  for every write and no test. The unsound substitution used to break such
  cycles by accident; `cleanup/scratch_liveness.rs` already retires closed
  cycles by reachability from observable roots, and now counts flag variables
  among the names it may retire.

ISA-agnostic check (ADR 0009):

- [x] No condition on an ISA enum. The only ISA data is four more register
      names in the flag family; the rules are def-use and evaluation-order facts.
- [x] The synthetic tests state statement shapes, not a compiler tuple.

Comparable coverage: the snapshot rule is exercised on ARM (`tmpzr`, `zr`) and
x86 (`cf`, `of`, `zf`) rows of the same sweep; the default/override rule on
x86-64 (`strrchr`, `fileno` chains) and Thumb.

## 4. Risk and ownership

- Existing owners extended; no new pass. `collect_written_vars`
  (`conditional_const`), `expr_reads_memory` / `stmt_may_write_memory`
  (`condexe`) and `collect_address_taken_locals` (`defuse`) are reused rather
  than restated.
- Shared substrate: def-use / reaching-definition fact.
- Interaction: a flag that can no longer be substituted stays a named boolean
  (`of = __scarry(a, b); a += b; if (of)`). That is the cost of the snapshot
  rule and it is the correct program.
- Telemetry: none.
- Not addressed, seen while reading output:
  - `cleanup_after_constant_folding_init` drops a stack store that is read
    only through the address of its neighbour (`buf[1]` of a two-byte buffer
    passed as `&buf[0]`). It runs only when constant folding changed
    something; with the temporary flags gone it runs less often, so some of
    those stores now survive. That is a side effect, not a fix.
  - Two predicates on one compare that write the same register
    (`movne r0, #1; moveq r0, #0`) keep the flag as a variable instead of
    becoming one `if`/`else`.
  - A dead default/override pair directly followed by the select that
    recomputes it: 9 occurrences in the sweep, 2 before.

## 5. Validation

- [x] Targeted tests, each with a positive control (the guard disabled, the
      test fails):
  `cargo nextest run -p fission-midend-normalize flag_recovery conditional_move scratch_liveness`
- [x] Crate gates: `cargo nextest run -p fission-midend-normalize -p fission-pcode`
      in the debug and the release profile.
- [x] Focused row: the anchor prints `return (uint)(*(uint *)(tmp_8052aa4 + 20) != 0);`.
- [x] Full `inlined` config, same 24,186 measured rows on both sides, caches off:

  | | main | this change |
  |---|---|---|
  | total structure distance | 621,358 | 620,160 (-1,198) |
  | perfect | 7,378 | 7,452 (+74) |
  | Union | 7,827 | 7,894 |
  | type perfect / byte perfect | 624 / 500 | 622 / 500 |
  | ARM projects: distance / perfect | 178,504 / 4,623 | 177,328 / 4,700 |
  | x86 projects: distance / perfect | 442,854 / 2,755 | 442,832 / 2,752 |
  | ARM functions naming a temporary or architectural flag | 1,569 | 239 |
  | x86 flag-write statements | 16,356 | 14,556 |

  Twelve rows lose a perfect score. Read one by one: eight had been perfect
  with wrong text (five read-before-definition selects as above, two that
  returned `v ? 1 : r0` for a `v` that is always 0, one `if` whose whole body
  was two dead flag writes), three now print a stack store `main` dropped, and
  one duplicates a `return 0` into each arm.
- [x] Sample-set gate (233 rows, mixed optimisation levels), same two builds:
      perfect 67 -> 67, total distance 7,603 -> 7,625 (+22), types 17/222 ->
      17/222. Four rows moved, all up, all read. One is a second call the
      source really makes: gnutls `buffer_append` expands
      `xtimes(xsum(capacity, 1), 2)`, a macro that evaluates its argument
      twice, and `main` printed `xsum` once because the merge above folded
      `rax = xsum(); if (rax >= 0) rax = xsum();` into a select with two equal
      arms. The others are a dead `zf` write removed with the branch shape it
      happened to supply, and two flag tests printed inline.
- [ ] `unoptimized` full config: not measured.

The reason the output is better without the metric: the old text tested a
value on the far side of the write that changed it, and named registers the
machine never tests.

## 6. AI review / prompt firewall

- No external model was asked for implementation advice.

## 7. Review notes

- [x] No binary, function, address or corpus guard in production code.
- [x] No claim rests on a dashboard-only edit.
- [x] No new pass, helper or metric duplicating an owner.
