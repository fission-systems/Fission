# Single-reference tail threading

## 1. Baseline row anchor

- Binary: DecBench sample-set `bin_078.elf`
- Command: release `fission_cli decomp` in `nir` and `hir`, plus the
  224-binary / 250-function sample-set runner
- Aggregate baseline (commit `9809e45d0`): NIR 1,916 gotos / 38,567 lines;
  HIR 1,905 gotos / 35,665 lines
- Failure category: a block with exactly one predecessor, unreachable by
  fallthrough, is emitted in place and reached by a jump — so the jump *into*
  it and the jump *out of* it both survive

Measured excerpt:

```c
    goto block_3700;
    ...
block_3700:
    if (!0) { goto block_3868; }
    rax = dcgettext(0, "failed to change ownership ...", 5, 14108);
    xVar42 = rax;
    goto block_36ba;
```

Two jumps (`goto block_3700` and the region's own `goto block_36ba`) where
one suffices.

## 2. Owner proof

- [x] Structuring

Triage of the 1,916 remaining NIR gotos by target-region shape put **66% in
the "region ends in another goto" bucket** — by far the largest and, until
now, entirely untouched. Within it, **352 labels** have exactly one
reference, are unreachable by fallthrough, and end in a jump. Each is worth
one goto at *zero* statement cost, because the rewrite is a move rather than
a copy.

An earlier count of 536 was wrong: it treated a preceding `}` as proof that
control could not fall into the label. A closing brace ends a block that may
well fall through, so `}` is not a total transfer. Restricting to genuine
total transfers (`return`/`goto`/`break`/`continue`) gave the real 352.

## 3. Generality / invariant proof

This extends `duplicate_terminal_tails` rather than adding a pass, because
the admission is the same shape with one axis widened:

```text
A region following Label(L) is relocatable when it ends in a *total transfer*
rather than specifically a Return. A Return tail may be cloned into any
number of jump sites. A Goto tail may only be moved to a single site: cloning
it would clone its trailing jump N ways and win nothing. So the Goto case
additionally requires refs(L) == 1 and L unreachable by fallthrough, making
the rewrite a move whose net effect is exactly one jump retired.
```

The pre-existing constraints carry over unchanged: no `Label` in the region
(a copied label would be a duplicate function-scoped definition), no
`Break`/`Continue` (they bind to the nearest enclosing loop, which can differ
between the label site and the jump site), and the size/count bounds. Interior
`Goto`s become admissible only in the relocating case, where they move rather
than multiply. A region whose trailing jump targets its own label is refused —
threading it would delete the very jump the rewrite needs to find.

- [x] AST shape only; no binary, compiler, or name guard.
- [x] Synthetic coverage uses hand-built statement trees.

## 4. Correctness fix surfaced by the new tests

Allowing goto-terminated regions exposed a latent bug in the original pass.
`drop_unreachable_tail_definitions` decided how many statements to delete
behind a label using the length recorded at collection time, and it dropped
the original on the assumption that every reference had been rewritten.
Neither holds once regions can contain jumps:

- Replacing a `Goto` *inside* a region changes that region's length, so the
  recorded length no longer matches what sits behind the label. The extent is
  now re-derived from the current tree.
- A `Goto` inside freshly spliced content is skipped by the replacement scan
  (the scan steps over what it just inserted), so a label can still have a
  live reference after replacement. Dropping it stranded that jump — a
  dangling `goto` with no label. Removal is now gated on **recomputed**
  reference counts being zero.

The first synthetic threading test failed on exactly this, which is how it
was caught rather than shipped.

## 5. Validation matrix

- [x] Unit tests: 4 new (threading; fallthrough-reachable refusal;
  multi-reference refusal; self-targeting refusal) for 10 in this module.
- [x] Crate gates: `cargo nextest run -p fission-midend-structuring
  -p fission-pcode` — 1,156 passed, 1 skipped, no expectation changes.
- [x] Ground-truth semantics: `count_bits_matches_real_machine_code` passed.
- [x] Full sample-set rerun, both layers, release CLI, against a baseline
  regenerated from the committed parent.

## 6. Measured result

224/224 binaries and 250/250 functions in both layers:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 1,916 | 1,782 | **-134 (-6.99%)** | 38,567 | 38,294 (**-273**) |
| HIR | 1,905 | 1,771 | **-134 (-7.03%)** | 35,665 | 35,391 (**-274**) |

Per-file: NIR **53 improved, 0 regressed**; HIR **52 improved, 0 regressed**.
Largest gains: `bin_048` -9, `bin_095`/`bin_072`/`bin_075` -6 each.

Both layers improve identically and lines fall, as expected for a transform
that moves code instead of copying it.

Anchor row after:

```c
    if (!0) { goto block_3868; }
    rax = dcgettext(0, "failed to change ownership ...", 5, 14108);
    xVar42 = rax;
    goto block_36ba;
```

## 7. Cumulative

Across the three 2026-08-13 slices (terminal-tail duplication, forward guard
inversion + if/else recovery, and this):

| Layer | Session start | Now | Delta |
|---|---:|---:|---:|
| NIR | 2,089 | 1,782 | **-307 (-14.7%)** |
| HIR | 1,983 | 1,771 | **-212 (-10.7%)** |
