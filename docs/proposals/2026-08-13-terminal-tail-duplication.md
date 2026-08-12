# Shared terminal-tail duplication

## 1. Baseline row anchor

- Binary: DecBench sample-set `bin_126.elf`
- Function: `sub_33b0` region, address `0x33b0`
- Command: release `fission_cli decomp` at the address in `nir` and `hir`,
  plus the 224-binary / 250-function sample-set runner
- Current NIR output for the anchor file: 3 gotos
- Aggregate baseline: NIR 2,089 gotos / 38,681 lines; HIR 1,983 gotos /
  35,715 lines
- Failure category: a terminal tail reached from several predecessors is
  emitted once, and every other predecessor reaches it by explicit `goto`

Measured excerpt (three-way comparator):

```c
if (rax < xVar0) goto block_3398;
if (xVar0 < rax) goto block_33a0;
...
block_3398:
    return -1;
block_33a0:
    return 1;
```

## 2. Owner proof

- [x] Structuring

The tails above are *terminal*: each ends in `return`, so control provably
never leaves the region by falling through. Structuring has no schema that
copies such a tail into its jump sites, so a shared epilogue, a shared
error/abort handler, or a shared cleanup-and-return block keeps one emitted
copy and N-1 gotos.

Reference invariants, read as controls rather than implementation
dependencies:

- Ghidra `ActionReturnSplit` (`blockaction.hh`): splits the epilog,
  introducing RETURN operations for individual branches flowing to it,
  gated by `isSplittable`.
- angr SAILR `ReturnDuplicatorHigh` (Basque et al., USENIX Security 2024):
  the *gotoless* duplicator that splits a shared return block into its
  predecessors so early-return guards recover their source shape.
- kuna `ActionReturnDup` / `ActionGotoReduce`
  (`p8_structure/kuna_returndup.rs`, `kuna_gotoreduce.rs`): a port of the
  angr pass; rewrites `if (cond) goto T` where `T` is a small chain ending
  in `return`, bounded by per-function split and in-degree caps.

## 3. Generality / invariant proof

Generalized rule:

```text
A Label(L) region is duplicable into its goto sites iff the region ends in
Return (control cannot fall out of it), contains no Break/Continue anywhere
(those bind to the nearest enclosing loop, which may differ between L and a
goto site), contains no nested Label or Goto (a copied Label would be a
duplicate function-scoped definition; a copied Goto multiplies edges this
rule has not proven terminal), L is defined exactly once, and the region and
its reference count stay inside the growth bounds. Each Goto(L) is replaced
by a verbatim copy. The original Label(L) and its region are removed only
when the preceding sibling is a total transfer, proving ordinary control
cannot fall into L.
```

- [x] The rule uses AST shape only: statement kind, label definition and
  reference counts, and fallthrough reachability.
- [x] No function, address, binary, compiler, mnemonic, or rendered-name
  guard is used.
- [x] Synthetic coverage uses hand-built statement trees.

Comparable coverage (unit tests in `cleanup/tail_dup.rs`):

- Motivating shape: unreachable shared return tail, duplicated, original
  dropped.
- Retained shape: label reachable by fallthrough, tail copied to the goto
  site and the original kept — the reference owners' "duplicate into each
  predecessor but one".
- Negative: region containing `Break`.
- Negative: non-terminal region that falls into the next label.
- Negative: protected (LSDA landing-pad) label.
- Negative: reference count over the cap.

## 4. Risk and ownership check

- Existing owner: `fission-midend-structuring::cleanup`, alongside
  `eliminate_nonfallthrough_label_aliases`.
- New pass: no new stage. Runs once at the existing post-structure cleanup
  point in `orchestrate.rs`, immediately after the alias pass so aliased
  labels are already canonicalized to a single target.
- State safety: this is an AST-to-AST rewrite on the finalized structured
  body. It does not call into the builder, so it cannot perturb type
  inference, materialization, or naming — the failure mode that reverted the
  earlier recursive complex-arm experiments.
- Growth: bounded by `MAX_TAIL_STMTS` (6), `MAX_TAIL_REFS` (8), and
  `MAX_DUPLICATED_STMTS` (160) per function.
- Known cases that must not change: `bin_103`'s
  `uint sub_72f1(uint param_1)` type-pollution sentinel; `bin_039`.

## 5. Validation matrix

- [x] Targeted unit tests: `cargo nextest run -p fission-midend-structuring
  tail_dup` — 6 passed.
- [x] Crate gates: `cargo nextest run -p fission-midend-structuring
  -p fission-pcode` — 1,139 passed, 1 skipped.
- [x] Ground-truth semantics: `fission-dir`
  `count_bits_matches_real_machine_code` (drives the real emulator against
  decompiled output) — passed.
- [x] Full sample-set rerun, both layers, release CLI.
- [x] Sentinel inspection: `bin_103` parameter type unchanged; `bin_039`
  head unchanged.

## 6. AI review / prompt firewall

- [x] Reference decompilers were read as cleanroom control-structure
  references only; no vendor code is copied or linked.
- [x] Production code contains no benchmark identity or compiler tuple.
- [x] A synthetic invariant test plus a full-corpus regression run back the
  measured claim.

## 7. Measured result

Full DecBench sample set, release CLI, 224/224 binaries and 250/250
functions in both layers:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 2,089 | 2,030 | **-59 (-2.82%)** | 38,681 | 38,747 (+66) |
| HIR | 1,983 | 1,959 | **-24 (-1.21%)** | 35,715 | 35,758 (+43) |

Per-file movement: NIR 25 files improved, **0 regressed**; HIR 13 improved,
**0 regressed**. Three NIR files reached zero gotos (`bin_126`, `bin_016`,
`bin_026`, `bin_221`). Largest single-file gains: `bin_220` -7,
`bin_007` -6, `bin_083`/`bin_166` -4.

Line growth is +66 NIR / +43 HIR against 59 and 24 removed jumps, i.e. the
duplication bound is doing its job; this is a control-flow readability
claim, not a size claim.

Anchor row after:

```c
if (rax < xVar0) {
    return -1;
}
if (xVar0 < rax) {
    return 1;
}
```

One pre-existing test, `test_switch_skips_to_exit`, asserted the previous
`goto block_5030` shape. The switch's shared `return rax` exit is a terminal
tail, so the `default` arm now returns directly instead of jumping backward
into `case 2`'s body — which also removes the `block_5030:` label that had
been emitted *inside* a switch case. The assertion was updated to pin the
new shape after inspecting both outputs.
