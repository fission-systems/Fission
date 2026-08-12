# Nested fallthrough goto cleanup

## 1. Measured row anchors

Release `fission_cli` output from the 224-binary / 250-function DecBench sample
set initially exposed five NIR and six HIR rendered candidates of this shape:

```c
if (cond) {
    ...
    goto block_join;
}
block_join:
...
```

The goto is the last statement of one or more nested `Block`/`If` scopes and
the target label is the next statement in the first enclosing sequence. The
measured rows are:

- Initial NIR candidates: `bin_014`, `bin_029`, `bin_077`, `bin_111`,
  `bin_213`.
- Initial HIR candidates: the same rows plus `bin_193`.
- Current aggregate baseline: NIR 2,115 gotos; HIR 2,009 gotos.

The full AST-grounded rerun found two additional safe rows (`bin_126` and
`bin_138`) and conservatively retained the apparent candidates in `bin_193`
and `bin_213`, whose structural boundary did not prove the enclosing sequential
successor invariant.

This proposal uses those rows only as measurement anchors. Production logic
must use AST control-scope and label identity, never binary or address identity.

## 2. Owner and reference proof

- Canonical Fission owner:
  `fission-midend-structuring::cleanup::eliminate_redundant_gotos`.
  Its empty-jump rule currently operates only within one statement list and
  does not carry the next enclosing fallthrough label into nested scopes.
- Ghidra `CollapseStructure::ruleBlockCat` concatenates a single-exit chain
  when the successor has one incoming edge, and its printer suppresses a goto
  when the destination is the next flow block.
- Vendored kuna records the same `BlockGoto::gotoPrints` rule: a trailing goto
  is not printed when it targets `nextFlowAfter`.
- Snowman's `DefinitionGenerator` similarly returns no goto when the target is
  the next basic block.

No runtime or build dependency on a vendor tree is introduced.

## 3. Invariant

```text
Within a sequential Block or either arm of an If, a trailing Goto(L) is
equivalent to fallthrough when L is the next statement in the nearest enclosing
sequence. Propagate that successor label only through Block and If scopes.
Do not propagate it through While, DoWhile, For, or Switch boundaries because
their body/case fallthrough has loop-iteration or case semantics.
```

The rule is ISA-agnostic and changes neither expression evaluation nor side
effect order. Label cleanup remains responsible for removing an unreferenced
label only after all goto rewrites complete.

## 4. Validation matrix

- Positive: nested `Block` and nested `If` tail goto to the enclosing next
  label are removed.
- Negative: the same textual target at the end of a loop body or switch case is
  retained.
- Focused release rerun of all measured rows in NIR and HIR.
- `cargo nextest run -p fission-midend-structuring`, full `fission-pcode`, crate
  checks, and owner-boundary scan.
- Full DecBench NIR/HIR rerun before claiming readability improvement.

## 5. Measured result

The release full-corpus rerun completed 224/224 binaries and 250/250 functions
on both layers:

| Layer | Gotos | Logical lines | Bytes |
|---|---:|---:|---:|
| NIR baseline | 2,115 | 38,736 | 1,065,829 |
| NIR after | 2,108 | 38,719 | 1,065,457 |
| HIR baseline | 2,009 | 35,769 | 965,428 |
| HIR after | 2,002 | 35,756 | 965,171 |

All seven removed gotos occur in `bin_014`, `bin_029`, `bin_077`, `bin_111`,
`bin_126`, and `bin_138` (two arms are removed in `bin_029`). `bin_073` and
the type-pollution sentinel `bin_103` remained byte-identical on both layers.
HIR byte-level comparison outside the goto-delta rows is not a deterministic
oracle because the existing local-name allocator can renumber names across
identical reruns; goto counts and logical-line counts remained stable measures.
