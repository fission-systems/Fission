# Architecture Proposal: P-code-rooted DIR Product Refactor

Date: 2026-07-26
ADR: `docs/adr/0014-dir-product-vs-prestructure-substrate.md`

## Baseline

This is an architecture and contract refactor. It is not motivated by a
benchmark row and makes no decompiler-quality claim.

Current shipped facts:

- `PreHirFunction` is the static pipeline's flat pre-structure AST.
- The static pipeline mutates that value through normalize/structuring and
  converts it into `HirFunction`.
- The former `fission-verify` implementation compares snapshots from one
  static pipeline. It does not produce an independent reconstruction rooted
  in p-code.
- Emulator execution is rooted in lifted p-code/machine state, while symbolic
  verification currently lowers pre-structure/HIR ASTs rather than the p-code
  foundation.
- The terms DIR and NIR are overloaded across substrate, product, and print
  surfaces.

## Owner Proof

- P-code semantic foundation: `fission-pcode`
- Static pre-structure AST: current `fission-midend-prehir`
- Static normalization/structuring: `fission-midend-normalize`,
  `fission-midend-structuring`
- Validated reconstruction product: former `fission-verify`, renamed and
  refactored as `fission-dir`
- CLI routing/reporting: `fission-cli`

No printer, UI, or benchmark surface owns this refactor.

## Invariants

1. The executable semantic foundation is a validated, immutable
   `PcodeFunction` snapshot captured before candidate-only optimization.
2. The static pre-structure types are renamed to `PreHir*`; they remain an
   internal static-pipeline substrate and carry no assurance or emulator state.
3. The DIR product owns independent candidate and assurance contracts.
4. A HIR or PreHIR value may seed a DIR candidate, but neither is the semantic
   oracle.
5. Validation compares an explicit observation contract against the p-code
   foundation: live-outs, memory effects, exit kind, return, and traps/calls as
   admitted by scope.
6. Verdict and evidence strength are separate. Unsupported or inconclusive
   work is never reported as equivalent.
7. Existing CLI/JSON compatibility shims may remain, but new names expose
   pre-structure data as PreHIR and validated output as DIR product.
8. The default HIR path remains unchanged and does not depend on the expensive
   DIR product.

## Scoped Implementation

- Mechanically rename the static substrate crate and public `Dir*` types to
  `PreHir*`.
- Rename `fission-verify` to the `fission-dir` product crate.
- Preserve the existing PreHIR/HIR differential tiers as explicit transition
  diagnostics, not as the product oracle.
- Add immutable p-code foundation identity, candidate source, observation
  scope, validation verdict, evidence strength, assumptions, budgets,
  unresolved effects, and counterexample contracts.
- Add a first foundation admission path that validates p-code shape and emits
  an unverified/inconclusive product report until a supported equivalence
  backend supplies stronger evidence.
- Add the first executable `PcodeNative` candidate slice for contiguous,
  side-effect-free single-block integer regions. The candidate owns a typed
  expression rather than treating rendered pseudocode as executable meaning.
- Validate the original selected operations with `fission-emulator`, prove or
  refute the candidate with `fission-solver`, and replay solver witnesses in
  the emulator before exposing a counterexample.
- Reject memory, calls, control flow, wide values, overlapping/subregister
  locations, unsupported observation scopes, and unenforceable solver timeout
  budgets instead of approximating them.
- Require callers to identify admitted non-memory value-space IDs and retain
  that classification as a `Memory` assumption, so solver-universal evidence
  is reported as conditionally proven until foundation metadata owns the space
  classes.
- Route CLI verification through the new product owner while preserving the
  existing command surface.

## Non-goals

- No whole-program proof.
- No second complete structuring implementation in this slice.
- No claim that PreHIR/HIR agreement proves p-code equivalence.
- No use of rendered NIR/HIR text as an execution oracle.
- Keep `--dir` as a compatibility alias for canonical `--prehir`; reserve new
  JSON vocabulary for `code_prehir`.

## Validation

- Targeted contract tests for foundation identity and assurance derivation.
- End-to-end native-region tests for universal equivalence, solver-found
  counterexamples, comparison outputs, observation-scope rejection, memory
  rejection, and overlapping-varnode rejection.
- Existing PreHIR/HIR differential, emulator, and symbolic tests.
- `cargo nextest run -p fission-midend-prehir`
- `cargo nextest run -p fission-dir`
- `cargo check -p fission-pcode`
- `cargo check -p fission-decompiler`
- `cargo check -p fission-cli`
- `cargo check -p fission-automation`
- NIR boundary scan and staged diff hygiene.
