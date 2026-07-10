# Decompiler Change Proposal: Disjoint merge respects co-occurrence (F1)

## 1. Baseline

- Family **F1** (binding collapse): m32 O2 `saturating_add`
- Symptom: `eax = param_1; eax = param_2; if (eax + eax < eax)` — a, b, and sum
  forced onto one name
- Intermediate GT dump still had `ecx` and sum distinct before merge-type-loop

## 2. Owner

- [x] Normalize / `variable_merge` disjoint live-range step
- Copy-alias step already consults `collect_cooccurring_var_pairs`; disjoint
  step did not

## 3. Invariant

```text
Two locals that co-occur in the same expression (e.g. binary operands, or
def and use in one assign) must not be coalesced by speculative disjoint
live-range merge. Distinct hardware GPRs must not be coalesced into each
other by that step either (they name ABI/storage identity, not free temps).
```

## 4. Validation

- Synthetic HIR: `ecx = p1; edx = p2; eax = ecx + edx; return eax` survives
  `apply_variable_merge_pass` with three distinct names
- Real: saturating_add no longer emits `eax + eax`
