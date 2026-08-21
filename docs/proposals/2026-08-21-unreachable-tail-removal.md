# Dropping the exit block nothing reaches

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/scale/binaries/O0/base-passwd/update-passwd`
- Function/address: `unlink_file` at `0x4ebe`
- Command: release `fission_cli decomp <binary> --layer nir --addr 0x4ebe
  --no-header --no-warnings`
- Current measured output:

```c
if (local_8) {
    ...
    return 0;
} else {
    return 1;
}
return xVar20;          // reached by nothing; xVar20 is never assigned
```

- Reference: the function's published source CFG has 3 nodes and 2 edges;
  ours has 4 and 2. The extra node has no predecessor.

Whole-binary measurement, 40 functions scored against the corpus's published
source CFGs with the benchmark's own `compute_ged`:

```text
node/edge delta      functions
(+0, +0)  exact             12   (30%)
(+1, +0)  one dead node      9   (22%)
everything else             19
```

Every one of the nine is wrong by this and nothing else. DecBench's graph edit
distance charges `1 + in-degree + out-degree` to delete a node, so an isolated
one costs exactly 1 -- which is why nine functions scored a GED of exactly 1
and the binary's median GED was 1 rather than 0.

Statement content is never priced by that metric: substitution costs
`|d_in| + |d_out|` and nothing else. The whole of the structure score is how
many basic blocks are emitted and how they are wired, which is what makes a
single unreachable statement worth as much as it is.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code · [ ] Builder · [ ] Normalize · [x] Structuring
- [ ] Type/data recovery · [ ] Printer · [ ] Benchmark/automation

`cleanup` owns it. A structured region whose every path returns is still
followed by the function's exit block, and nothing was looking at whether the
body in front of it could fall through. This is not a printer concern -- the
statement is in the tree, and a printer that skipped it would be re-deciding
structure downstream of the owner, which the architecture forbids.

## 3. Generality / Invariant Proof

```text
A statement list is truncated after the first statement that provably leaves
it, unless a later statement defines a label something still jumps to.
```

Divergence is proven, never assumed: `return`, `goto`, `break` and `continue`
leave; an `if` leaves only when both arms exist and both leave; a `switch`
only when it has a default and every case leaves. Loops are never treated as
diverging, since proving that would mean reasoning about the condition.
Everything unproven falls through, so a body is never truncated on a guess.

C labels are function-scoped, so reachability by jump is a whole-function
fact. A tail defining a referenced label is kept and scanning resumes from it,
however dead it looks in statement order.

ISA-agnostic check:

- [x] Reads only the statement tree.
- [x] No binary/function/address/compiler/ISA identity in production logic.

Required negative cases: an arm that falls through keeps the tail; an empty
arm is a fall-through path; a loop does not prove divergence; a tail behind a
referenced label survives; the trim applies inside nested arms too.

## 4. Risk And Ownership Check

Placed after the layout fixpoint, never inside it. Run inside, it changed what
the next round matched: terminal-tail duplication and guard inversion read the
shape of the body they are handed, and a tail this removes is part of that
shape. Reachability is a property of the finished body, so the finished body is
where it is decided.

## 5. Validation Matrix

- [x] Seven targeted tests covering each arm of the divergence rule and the
      label-reachability exception.
- [x] `cargo nextest run -p fission-midend-structuring` (257 passed).
- [x] `cargo nextest run -p fission-pcode` (1,001 passed, 1 skipped).
- [x] `cargo nextest run --workspace --no-fail-fast` against the established
      baseline.
- [x] Same 40 functions re-scored against their published source CFGs, before
      and after.
- [x] Whole DecBench sample-set rerun for regressions.

Measured result on the anchor binary, same functions both times:

```text
            mean    median   exact
before      10.9         1      12  (30%)
after       10.7         0      21  (52%)
```

Nine improved, **zero regressed**, nine newly exact. Widened to 15 binaries
and 259 functions, 108 are now exact (42%) and the `(+1, +0)` signature no
longer appears among the common shapes.

On the DecBench sample-set the change is quiet, as expected of something that
only deletes unreachable statements: 250 of 250 still decompile, gotos 1,123 ->
1,115, short-circuit terms unchanged, output 33,364 -> 33,254 lines.

### Two tests that were asserting on unreachable output

Both failed here, and both were right to. They had been passing on text that
nothing reached:

```c
// diamond_join_lowers_branch_local_register_defs_as_select
if (!tmp_80) { return 10; } else { return 20; }
return tmp_80 ? 20 : 10;        // the string the test asserted on

// preview_supports_instruction_local_unconditional_branch_targets_backward
return 9;
while (1) { }                   // the string the test asserted on
```

Both now assert on reachable output. Each also names a real defect this
uncovered, neither introduced here and neither fixed here:

1. The diamond select is built correctly and then destroyed -- structuring
   copies the join's return into both arms, leaving the select stranded in the
   exit block. A reader has never seen it. It is also the better structure:
   `return c ? 20 : 10` is one CFG node against four.
2. A backward branch whose loop is unreachable from the entry was emitted
   anyway, behind the return. Whether the fixture's loop *should* be reachable
   is a separate question this does not answer.

## 6. AI Review / Prompt Firewall

- No external model was consulted.
- The defect was found by scoring decompiled CFGs against the corpus's own
  published source CFGs, not by reading a benchmark row. Production code names
  no binary, function or address from that corpus.
