# Propagate aggregate use-site constraints back to bindings

## 1. Baseline Row Anchor

- Binaries: DecBench sample-set `bin_012.elf`, `bin_014.elf`, and
  `bin_029.elf` (representative rows from a measured 10--12-function class).
- Functions: `sub_2d50`, `sub_1f800`, and `sub_38b0`.
- Addresses: `0x2d50`, `0x1f800`, and `0x38b0`.
- Corpus command: `run_fission.py` on the stripped eval-kit binaries, followed
  by the x86-64 `rawerr.py results-hir` container command.
- Current output: a 16-byte aggregate binding receives a string or zero; an
  aggregate element is assigned to an `int`; mirror-image rows declare a
  scalar pointer and later use `->field_*` on it.
- Semantic cases passed / total: not the failure surface; recompilation fails
  before behavior comparison.
- Failure category: incompatible aggregate/scalar assignment and member access
  on a non-aggregate binding.
- Baseline gate: HIR GED 60/232 perfect, NIR GED 59/232; HIR and NIR types
  14/222 perfect. Compile rate is 101/158 measurable.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
fission_agg16 local_170; local_170 = "[unknown]";
fission_agg16 *addr; *addr = 0;
int uVar18; uVar18 = rax[19];
uint *ptr; ptr->field_e8 = 0;
```

`aggregate_fields.rs` recovers aggregate use-site facts, while
`type_flow.rs` is the fixed-point owner that propagates load/store/copy facts
to declarations. The invalid declaration/use pairs already exist before the
renderer; a print-boundary cast would only hide them.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A binding's declaration type and every direct aggregate load/store/member use
must agree through the existing type-flow constraints. A wide storage access
is width evidence, not permission to replace a pointer or scalar value with an
opaque aggregate. Conversely, a field access supplies aggregate-pointee
evidence to a safely refinable carrier binding.
```

ISA-agnostic check:

- [x] The rule is def-use/type-constraint based, not ISA gated.
- [x] No ISA-specific data or compiler tuple enters production code.
- [x] Synthetic coverage will state only declaration/use and flow shapes.

Comparable coverage:

- Similar shape 1: a scalar/string value stored through an address whose
  access width is 16 bytes.
- Similar shape 2: a scalar-declared pointer used through a recovered aggregate
  field.
- Synthetic invariant test: both directions converge without overwriting
  independent value types with access-width aggregate evidence.

## 4. Risk And Ownership Check

- Existing owner: `types/type_flow.rs`, fed by aggregate facts from
  `memory/aggregate_fields.rs`.
- Shared substrate: type constraints and def-use facts.
- Extend the existing solver/pass; do not add a rendering fallback or a new
  end-of-pipeline pass.
- Interactions: pointer arithmetic and aggregate-field recovery run in the same
  normalize fixed point, so convergence and multi-definition guards must stay
  intact.
- New owner dependency: none.
- Telemetry impact: none expected.
- Must not change: homogeneous scalar arrays, locked surface types,
  multi-definition register names, or value types that merely share a storage
  width with an aggregate.

## 5. Validation Matrix

- [ ] Targeted invariant tests in the existing normalize owner.
- [ ] `cargo nextest run -p fission-midend-normalize`.
- [ ] Focused decompilation of the representative sample rows.
- [ ] `rawerr.py results-hir`: zero incompatible aggregate assignments and
  zero member requests on non-aggregate bindings.
- [ ] Full `decbench_sample_set_gate.py check`: no GED or type-perfect drop;
  type perfect flat or higher.
- [ ] `cargo fmt --all --check`.

## 6. AI Review / Prompt Firewall

- [x] No separate model was asked for implementation advice.
- [x] Production code will contain no binary/function/address/corpus guards.
- [x] Validation includes synthetic invariants plus the measured corpus rows.

## 7. Review Notes

- The change is a type-inference repair, not a compile-only rendering repair.
- A compile-rate gain accompanied by a type-perfect drop is a failed result and
  will not be retained.
