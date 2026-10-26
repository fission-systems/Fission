# ADR 0015: One output surface — HIR over an accuracy-selected tree

**Status:** Accepted  
**Last verified:** 2026-10-26  
**Amends:** [ADR 0011](0011-hir-presentation-contract.md) §1 (dual-layer ownership), §6 (metrics)

## Context

Fission printed two surfaces per function: **NIR** (mechanical C, the scored
`code`) and **HIR** (presented C). The split was meant to separate accuracy from
readability, and in production it had grown into two different *structurings*:
`dual_layer_structuring` (on by default in `render_finish.rs`) re-ran the
pipeline with `SelectionAxis::Jumps` for HIR, so HIR was not "NIR plus
presentation" but a different tree chosen for fewer jumps.

Measured on the 250-row DecBench sample set
(`scripts/quality/decbench_sample_set_gate.py`, both layers from one run, 233
scored functions, release CLI, 2026-10-26):

| | GED perfect | Σ GED distance |
|---|---|---|
| NIR | 64 | 6,351 |
| HIR, own jump-minimising structuring (as shipped) | 66 | 6,945 (+9.4%) |
| HIR, same tree as NIR (`FISSION_DUAL_LAYER=0`) | 64 | 6,328 (−0.4%) |

The perfect-match count ranked the shipped HIR *ahead* of NIR while the distance
sum put it 9.4% behind: 38 functions got worse (Σ +801), 29 better (Σ −207).
Opening one of the regressions showed an unconstrained jump-minimising
structuring producing a condition on a flag variable that is never assigned.
The count could not see this; the distance sum did.

Presenting the *accuracy-selected* tree is both at least as accurate as NIR and
free of that regression class, so the second surface bought nothing the first
did not already have.

## Decision

1. There is **one output surface**: the structured tree, selected on
   `SelectionAxis::NodeEstimate`, after `apply_hir_presentation` on a clone.
   `NirDecompileOutput.code`, the CLI `code` field and plain-text output are that
   surface.
2. `dual_layer_structuring` and the second pipeline run are removed. Nothing in
   the pipeline selects `SelectionAxis::Jumps` on its own; it remains an explicit
   option on `NirRenderOptions`.
3. `code_nir`, `code_hir`, `layer`, `line_mappings_nir/_hir` and the
   `PseudocodeLayer` / `LayeredPseudocode` types are removed from the public
   surface. `fission_cli decomp --layer` is accepted, hidden and ignored (with a
   warning unless `--no-warnings`), so existing scripts keep running instead of
   failing — but a script that asked for `nir` no longer gets it.
4. NIR remains the **internal** representation and the owner of semantics
   (ADR 0008). `PrintProfile::Nir` survives as the mechanical print that unit
   tests compare the final surface against.
5. ADR 0011's presentation contract is unchanged and now load-bearing: clone-only,
   same evaluation count and order, and the structural firewall (§7) that
   restores the pre-polish tree on a violation. Because the presented text is now
   what is scored, a presentation bug is a scored bug.

## Consequences

- The "NIR is the semantic oracle / scored `code`" role passes to the final
  surface. Quality claims still require the measurement rule in `AGENTS.md`;
  they are now measured on this one surface, with total distance reported next to
  the perfect count (see *Fitting the metric instead of the row*).
- Renders one surface instead of two: no NIR print, and no second structuring
  run.
- The readability objective (fewer jumps) is no longer a separate product. If a
  readability-only structuring is wanted again it must come back through
  `SelectionAxis` with its own measured justification, not as a hidden second
  tree.
- Open: `SelectionAxis::Jumps` and its tests are kept; whether to delete them is
  a separate decision. The golden-corpus snapshot was regenerated for the new
  surface.
