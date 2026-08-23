# Preserve proven direct-callee pointer parameter types in call summaries

## 1. Baseline Row Anchor

Measured on `main` at `7c23d3cf3` with the official DecBench
`type_match.py` scorer, the sample-set relabeling path, and the fixed set of
109 functions whose DWARF-bearing originals are present locally. The current
baseline is 6 type-perfect functions and total type distance 1,214.

The caller and its direct internal callee were then decompiled independently
from the same stripped sample binary. These rows repeat one ownership defect:

| caller row | opt | caller parameter | current caller | isolated callee proof |
| --- | --- | --- | --- | --- |
| `get_string` / cronie | O0 | `FILE *file` | `ulonglong param_3` | `get_char(FILE* param_1)` |
| `print_line_tail` / grep | O0 | `const char *line_color` | `ulonglong param_3` | two callees take `uchar *` |
| `xheader_list_append` / tar | O2-noinline | `char *kw`, `char *value` | two `ulonglong` parameters | `assign_string(char* param_1)` |
| `wall` / sysvinit | O2-noinline | `char *text` | `ulonglong` | direct callee takes `uchar *` |
| `restore` / libacl | O2-noinline | `FILE *file` | `longlong` | two direct callees take `FILE*` |
| `describe_change` / coreutils | O2-noinline | four `char *` parameters | four `ulonglong` parameters | direct callee takes two `char*` arguments |

The same diagnostic found `void*` callees. Those are deliberately not proof
of a caller's declared pointee type and form a negative admission case.

This is a type-contract change. No structure or recompilation improvement is
claimed before remeasurement.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Decompiler type-context/callee prototype facts
- [x] Normalize/type recovery
- [ ] Structuring
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

`fission-decompiler::facts::build_preview_callee_summaries` already decodes
each direct internal callee in isolation, but records only its inferred arity.
`NirCallPrototypeSummary` therefore carries no parameter type facts.
`fission-pcode::seed_callee_summaries_from_type_context` compensates by filling
every `PrototypeSummary::param_lattices` entry with `Unknown`.

The consumer already exists:
`types/callsite_type_prop.rs::apply_callsite_type_prop_pass` reads non-unknown
internal `param_lattices`. The missing owner contract is transport of the
callee's independently proven parameter types and surface declarations, not a
printer rewrite or a new inference heuristic in the caller.

```text
isolated callee: get_char(FILE*)
  -> NirCallPrototypeSummary: arity=1, parameter type discarded
  -> caller CallSummary: param_lattices=[Unknown]
  -> get_string(ulonglong file)
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
A direct, non-import callee at an exact function start may export an ABI-slot
pointer parameter type after isolated lowering and type normalization when
the parameter is either a concrete integer-pointee pointer or has a specific
source declaration such as `char*` or `FILE*`. The caller may consume that
fact at the same argument position and backward through plain-copy aliases
only when the stable chain reaches a real caller parameter under the existing
single-definition/non-self-reference proof. Existing caller surface
declarations remain locked. Generic void pointers, numeric-width pointer
spellings, scalar types, missing parameters, recursive self-summaries,
temporary-only chains, caller-owned pointer-depth evidence, and conflicting
facts do not promote a caller declaration.
```

- [x] The rule uses direct-call identity, ABI argument position, isolated
      callee types, and existing def-use safety only.
- [x] It contains no ISA, compiler, source function, address, or corpus guard.
- [x] Evidence spans six projects, O0 and O2-noinline, `char*` and `FILE*`.
- [x] Generic `void*` is a measured negative case rather than an accepted
      substitute for a concrete source pointee.

## 4. Risk And Ownership Check

- Existing owners: `fission-decompiler` constructs immutable binary-derived
  callee facts; `fission-midend-normalize` consumes prototype constraints.
- Extend the typed `NirCallPrototypeSummary` / `PrototypeSummary` contract;
  do not create a parallel fact map.
- Isolated callee analysis uses a fresh PreHIR function. Raw-HIR construction
  discards its register-origin thread-local state on exit, so preview facts do
  not mutate the caller's lowering state.
- Only pointer parameter facts are exported in this slice. Return/scalar and
  aggregate-name recovery remain out of scope.
- The pointer-width override applies only to a width-only ABI scalar at the
  matching argument slot; a differently-sized or already surfaced caller
  binding is not rewritten.
- Stripped `sub_<address>` identities may transport a type fact only when the
  direct target resolves to an exact internal function start. They do not
  enable effect-summary propagation or exact-arity argument deletion.
- Typed preview is capped at 2,000 decoded p-code operations. Effect summary
  construction remains available above the cap, but the nonlinear lowering
  and normalization work is not admitted into an interactive caller build.
- Principal risks were circular summaries, generic-pointer overreach,
  pointer-depth inflation, normalize-state contamination, and per-function
  latency. The admission guards and whole-sample changed-signature audit cover
  each observed failure mode.

## 5. Validation Matrix

- [x] Typed transport tests cover concrete `char*`, surfaced `FILE*`, generic
      `void*` and numeric-width surface rejection, existing surface lock,
      wrong-width rejection, caller pointer-depth evidence, and temporary-only
      chains. The producer also rejects the current function address before
      attempting a recursive preview.
- [x] Focused NIR/HIR reruns covered all six measured caller/callee pairs.
- [x] Official type scorer replay on the same 109-function subset: perfect
      count remains 6, total distance improves 1,214 -> 1,206, pointer-miss
      rows improve 79 -> 76, with seven improved rows and zero regressions.
- [x] Full NIR sample sweep: 224/224 binaries and 250/250 functions; goto count
      remains 1,079. Sixteen output files changed and all were audited.
- [x] Full HIR sample sweep: 224/224 binaries and 250/250 functions; goto count
      remains 1,077. The same sixteen output files changed.
- [x] The measured type edits are: `wall` (two variants, one edit each),
      `print_line_tail` (one), `get_string` (one), `restore` (one),
      `xheader_list_append` (one), and `array_patsub` (two).
- [x] `cargo nextest run -p fission-signatures -p fission-midend-normalize
      -p fission-pcode`: 1,435 passed and one skipped.
- [x] `cargo check -p fission-decompiler -p fission-pcode`
- [x] `cargo build --release -p fission-cli`
- [x] External Docker `corpus/dev`, cache disabled, fixed 40-row denominator:
      semantic perfect 22/40 and mean 0.6442; type perfect 22/40 and mean
      0.8127; GED perfect 15/40 and mean 5.225. All are exactly unchanged from
      the saved baseline, with 40/40 coverage and the same 22 ok / 9 compile
      error / 9 assertion-fail distribution. This local run is regression
      evidence only and is not publishable leaderboard data.
- [x] `scripts/check/owner_boundaries.sh`
- [x] `python3 scripts/audit/nir_boundary_scan.py --root .`: zero findings.

## 6. AI Review / Prompt Firewall

- No external or cross-model implementation advice was requested.
- Row identities occur only in this local evidence document and tests use
  synthetic type/def-use facts.
- Production code will contain only direct-callee, ABI-slot, type-lattice,
  surface-type, and copy-safety invariants.

## 7. Review Notes

- [x] The justification is metric-independent: a caller should not redeclare a
      value as an integer after the independently analyzed callee proves that
      the same ABI argument slot is dereferenced or consumed as a concrete
      pointer.
- [x] No new runtime/build dependency or vendor implementation is introduced.
- [x] The existing fact producer and type consumer are extended; no duplicate
      type pass is added.

## 8. Measured Outcome

The output is better independently of DecBench scoring: seven callers now
declare values as pointers because their exact direct callees independently
prove that those ABI slots carry `char*`/`uchar*` or `FILE*`; no caller loses
an existing declaration, gains an unsupported pointer depth, or changes
control flow. The official type-distance reduction is supporting evidence,
not the reason for accepting the change.

This slice does not add a type-perfect function, does not change structure,
and does not claim a recompilation improvement.
