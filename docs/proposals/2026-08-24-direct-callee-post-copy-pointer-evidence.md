# Preserve post-copy pointer evidence in direct-callee previews

## 1. Baseline Row Anchor

Measured on `main` at `f42085100` with the cache-disabled DecBench sample-set
NIR runner and the official upstream type scorer's fixed 109-function subset.
The current sample result is 6 type-perfect functions and total type distance
1,203.

Three independently decompiled internal helpers recover pointer formals in
their final NIR, but the same helpers recover no pointer for those slots when
used as an isolated direct-callee prototype preview:

| project / optimization | helper evidence in final NIR | isolated preview |
| --- | --- | --- |
| coreutils O2-noinline | two `char*` formals | neither formal is a pointer |
| openssh-portable O2-noinline | trailing `char*` formal in each of two helpers | corresponding slots are not pointers |
| tar O2-noinline | `char*` formal | only the immediate single-definition path is admitted; a reused temporary loses the source formal |

The affected callers render seven source pointer arguments as integer types.
The shape also repeats in locally compiled and stripped O0 builds of the same
three source functions: arguments are copied into stack/register locals before
the helper call, and final caller output recovers only the subset exposed to a
known call without that copy boundary.

The measured difference is an analysis-stage inconsistency, not a proposal to
copy the final text of one decompilation into another. The same decoded helper
and type context produce different parameter evidence solely because the
ordinary render path runs safe statement-run copy propagation and reapplies
callsite contracts after normalization, while the prototype preview stops
immediately after normalization.

The final fixed-denominator rerun keeps the perfect count at 6 and reduces
total type distance from 1,203 to 1,199. Two measured rows improve and none
regress. This is a type-distance improvement, not a perfect-rate or leaderboard
movement claim.

## 2. Owner Proof

- [x] Decompiler direct-callee prototype preview finalization
- [ ] SLEIGH/raw p-code semantics
- [ ] Builder/materialize
- [ ] Normalize type contract itself
- [ ] Structuring algorithm
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

The helper's raw p-code contains the relevant calls and copies. The normalize
owner already has exact API/direct-callee parameter contracts, conservative
single-definition backward propagation, and a run-local copy propagator that
stops at calls, labels, gotos, and nested control-flow boundaries. The normal
render path invokes that propagator after layout and reapplies the existing
callsite contract when it exposes a stable source binding. The isolated
prototype path invokes neither step before extracting its parameter list.

Recursive grandcallee preview is not the owner: a measured experiment using
it caused sample worker timeouts on two large functions and was rejected.

## 3. Generality / Invariant Proof

Generalized rule:

```text
An isolated prototype preview must preserve pointer evidence that follows from
the same function's stable, effect-free COPY chain. After ordinary normalize,
run the existing boundary-aware statement-run copy propagation on the isolated
function. Only if it changes the body, reapply the existing callsite contract
once before reading the formal parameter types.

Do not recursively decompile callees, do not relax multi-definition backward
typing, and do not infer through a call, label, goto, loop, or branch boundary.
```

- [x] No function, address, project, compiler, corpus, or ISA identity enters
      production logic.
- [x] The evidence repeats in at least three functions, three projects, and
      both O0 and O2 forms.
- [x] The change reuses existing semantic owners and adds no pass or parallel
      fact map.
- [x] The isolated host remains detached from the caller.

The output is better without reference to the metric because copying a value
into a temporary does not change whether it is a pointer, and an isolated
analysis of a helper should not contradict the helper's ordinary analysis
solely because one path stopped before an existing semantics-preserving copy
cleanup.

## 4. Risk And Ownership Check

- The copy propagator is already constrained to straight-line statement runs
  and clears state at effects and control-flow boundaries.
- The callsite pass retains its existing exact-width, surface-conflict,
  pointer-depth, single-definition, and self-reference guards.
- No full structuring or nested callee preview is added to the prototype path.
- Preview p-code and instruction caps remain unchanged.
- Candidate risk is that pre-structuring statement layout exposes fewer or
  different runs than the ordinary post-structuring path. Focused output and
  full-corpus audits are required; a no-op result will be closed rather than
  broadened heuristically.

## 5. Validation Matrix

- [x] Targeted test: stable copy exposed after normalize transports an exact
      pointer call contract to the source formal.
- [x] Targeted test: label/goto, effect, multi-definition, and self-reference
      boundaries remain closed.
- [x] Focused sample callers and standalone helpers remeasured. The
      openssh-portable caller recovers its trailing `char*`; the coreutils
      caller recovers three `char*` arguments. The tar sample's ambiguous
      multi-definition temporary remains rejected.
- [x] Official fixed-109 sample type scorer replay, including per-row
      regressions.
      Perfect 6 -> 6, distance 1,203 -> 1,199, pointer-miss rows 76 -> 76;
      two improved rows and zero regressed rows.
- [x] Full sample NIR and HIR output/goto/failure audit. Both layers decompile
      250/250 functions and 224/224 binaries with no failures. Exactly four
      files change on each layer (`bin_041`, `bin_078`, `bin_129`, and
      `bin_152`), all in pointer declarations or the cast required by one
      declaration change. NIR gotos remain 1,079; HIR gotos remain 1,077.
- [x] Stripped O0 callers remeasured as held-out regression evidence. The
      coreutils caller gains one `char*`, tar gains two `char*` formals, and
      openssh-portable is byte-identical. No held-out row loses pointer form.
- [x] The two sample functions that timed out under the rejected recursive
      grandcallee experiment complete in 8.7s and 1.8s.
- [x] The four affected HIR functions were rerun and are byte-identical,
      providing a focused determinism check.
- [x] `cargo nextest run -p fission-decompiler -p
      fission-midend-normalize`: 404 passed.
- [x] `cargo nextest run -p fission-pcode`: 1,015 passed, one skipped.
- [x] `cargo check -p fission-pcode -p fission-decompiler` and
      `cargo build --release -p fission-cli`.
- [x] Owner-boundary scan passed; benchmark-smell scan reported zero findings.
- [x] Cache-disabled external Docker fixed 40-row run (`--variant-limit 1`,
      `--no-resume`): NIR/HIR code, semantic, type, GED, recompilation, goto,
      AST, and readability fields are identical to the saved baseline. GED
      cache hits are zero. Semantic is 22/40 perfect (mean 0.6442), type 22/40
      (mean 0.8127), GED 15/40 (mean distance 5.225), with 22 ok / 9 compile
      errors / 9 assertion failures. This is local regression evidence only;
      the artifact is
      `fission-benchmark/results/local_postcopy_f42085100_dirty.json`.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Vendor code is not linked, invoked, or copied.
- Row identities occur only in this proposal and measurement artifacts.

## 7. Review Notes

- [x] Proposal recorded before production edits.
- [x] The existing direct-callee preview owner is extended.
- [x] No quality claim will be made unless measured real rows improve without
      regressions.
