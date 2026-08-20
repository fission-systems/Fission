# Short-circuit branches as a structuring quality axis

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_146.elf`
- Function/address: `sub_fad0` at `0xfad0`
- Command: release `fission_cli decomp .../bin_146.elf --layer nir --json
  --no-header --no-warnings --addr 0xfad0`
- Current measured output: 0 gotos, 2,743 short-circuit operators, 233 lines.
  Parsed to a CFG it is 4,721 nodes / 7,199 edges, against a corpus median of
  9 nodes.
- References on the same corpus: DecBench reports Fission structure GED
  mean 318.3 / median 17 (18.7x). Every other scored tool sits between 1.8x
  and 3.4x; Glaurung is mean 30.2 / median 10.
- Semantic cases/static score: unchanged by this proposal. Rejecting a
  candidate returns the function to the structuring it already had, which is
  the same program with explicit transfers.

Whole-corpus measurement, 250 functions over 224 binaries, all parsed
(0 parse failures, so none of this is a broken-output artifact):

```text
corr(decompiled CFG nodes, short-circuit operators) = 0.954
corr(decompiled CFG nodes, statement count)         = 0.233
share of total node+edge mass in functions with &&/||  = 97%
sum(nodes) + sum(edges)                             = 87,073
DecBench GED mean 318.3 x 250 functions             = 79,575  (109%)
```

The corpus's entire structural distance from source is accounted for by
short-circuit operators this pipeline emits. The DecBench maintainer reached
the same conclusion independently from the source side: *"the repeated
`bVar291 && !bVar292 && ...` guard expressions ... inflate the CFG well past
the source shape."*

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

`structuring_quality` is the owner. It decides which candidate structuring is
kept, and it is the only place that can see the trade being made. The drivers
are not at fault: `reaching_driver` produces a correct description of the
region, and the comparator accepts it because none of its axes can tell what
the description costs.

`guard_formula_size` counts expression nodes, so `a && b` and `a + b` are both
three nodes. The distinction the comparator needs is that one of them is a
conditional branch and the other is arithmetic.

The defect is recorded in the owner's own source. `max_guard_budget`'s comment
raises `MAX_GUARD_NODES_PER_REMOVED_GOTO` from 4 to 512 and names its evidence:
`bin_009` 39 jumps to 0 and `bin_146` 38 to 0. Those are the first and fourth
worst functions in the structure-distance tail.

## 3. Generality / Invariant Proof

Generalized invariant:

```text
A structuring may add at most as many short-circuit operators as it removed
transfers. Both are conditional branches, so the total number of branches in
the emitted body never increases.
```

This is a rate of one, which makes it an invariant rather than a tuned bound.
It admits the legitimate fold -- `if (a) if (b) X;` becoming `if (a && b) X;`
spends one term for the one transfer it removes -- and refuses a
reaching-condition formula, which spends hundreds.

ISA-agnostic check:

- [x] Uses only the emitted statement tree. No CFG, dominance, address, or
      binary identity is consulted.
- [x] No binary/function/address/compiler/ISA identity in production logic.
- [x] No runtime or build dependency on any vendored decompiler.

The gate is a new conjunct in `improves_on`, so it is monotone: it can only
reject candidates that were previously accepted, never accept one that was
previously rejected. A rejected candidate leaves the established structuring
in place, which is why no semantic risk attaches to the change.

Required negative cases:

- a fold that removes no transfers is already refused by the goto axis and
  must not be reached by this one;
- a guard that is wide in expression nodes but carries no branches must stay
  affordable, so the size axis keeps its own meaning;
- a candidate reaching zero gotos must still be refused when its predicate
  terms outnumber the jumps it removed.

## 4. Risk And Ownership Check

- Extend `structuring_quality`; no new pass, no change to any driver.
- `MAX_GUARD_FORMULA_SIZE` and `MAX_GUARD_NODES_PER_REMOVED_GOTO` are
  untouched. They bound downstream cost, which is a different quantity and
  still real: the 160,423-node formula that cost 45 seconds is excluded by
  size regardless of its branch count.
- A per-guard ceiling on branch terms was implemented and measured as an
  alternative, and is dominated: capping one guard at 16 terms gives 1,040
  jumps and 763 terms, against this rate at 2 giving 1,030 and 537 -- worse on
  both axes, because rejecting a candidate for its largest guard discards all
  of its jump removals at once. It was removed rather than kept as a second
  knob.
- The goto count regresses and this is the intended trade, not a side effect.
  See section 5.

## 5. Validation Matrix

- [x] Targeted Rust test for the new axis
      (`branches_are_budgeted_against_the_jumps_they_replace`), covering both
      the admitted fold and the refused reaching-condition shape.
- [x] The two existing guard-size tests rebuilt on an arithmetic-wide guard so
      the size axis is exercised without branches, which is what made them
      pass for the wrong reason before.
- [x] `cargo nextest run -p fission-midend-structuring` (250 passed).
- [x] `cargo nextest run -p fission-pcode` (1,001 passed, 1 skipped).
- [x] `cargo nextest run --workspace --no-fail-fast`: 2,580 passed, 7 failed,
      1 timed out. The 7 failures are the established `fission-emulator`
      baseline. The timeout,
      `fission-dir::phase2_corpus_ground_truth::corpus_decompilations_match_real_machine_code`,
      was verified to time out identically at `HEAD` with the change stashed,
      so it is pre-existing and unrelated.
- [x] Per-function 224-binary/250-function sample-set sweep in both layers, at
      seven budget values, with no claim taken from aggregate totals alone.
- [x] Decompiled CFGs re-parsed through the benchmark's own
      `extract_decompiled_cfgs` at four of those values, so the structural
      claim is measured on CFGs rather than on the short-circuit proxy.

Measured result. Budget sweep on the sample-set, counting both kinds of
branch:

```text
per-goto   gotos   short-circuit   total branches   CFG nodes+edges   mean/med
       0   1,297              85            1,382                 -          -
       1   1,123             228            1,351            13,191       2.7x
       2   1,030             537            1,567            14,172       2.8x
       4     963           1,347            2,310                 -          -
       8     936           1,983            2,919            18,895       3.3x
      32     729          11,570           12,299                 -          -
     512     617          27,866           28,483            87,073      15.3x
```

Adopted: 1.

- Structural mass falls from 87,073 node+edge to 13,191, and the mean/median
  ratio from 15.3x to 2.7x -- inside the 1.8x-3.4x band every other scored
  decompiler occupies. The largest single function falls from 4,721 CFG nodes
  to 281, and the count of functions large enough to take DecBench's
  approximate-GED path from 54 to 18.
- Gotos rise from 617 to 1,123 in NIR and to 1,120 in HIR. This is the trade
  being made deliberately. The previous 617 was reached by converting
  transfers into predicate terms at 55 branches spent per branch removed; the
  goto counter could not see the terms, so it scored the conversion as a win.
  Counting both, the old setting carried 28,483 branches and the new one
  1,351.
- Output does not grow: 34,811 NIR lines to 33,364.
- No claim is made here about goto count against other tools on this corpus.
  The existing Ghidra/angr comparison is measured on the 204-file benchmark
  intersection, which is a different corpus, and has not been rerun.

## 6. AI Review / Prompt Firewall

- An external model produced an architecture review of the repository during
  this cycle. Its causal account of the structure-distance tail (partial
  collapse failure leaving residual transfers) is contradicted by the
  measurement above: the tail functions have zero or near-zero gotos, and the
  worst of them is a candidate the comparator accepted. No implementation
  advice was taken from it.
- The DecBench maintainer's independent observation is quoted in section 1 as
  corroboration of a result already measured locally, not as its source.
- The vendored sample-set was used as a measurement corpus. Production code
  contains no binary, function, or address identity from it; the constants are
  justified in the source by the shape of the sweep, and the row names appear
  only in prose.
