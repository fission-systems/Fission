# Decompiler Change Proposal: Keep primary-return register materializations

## 1. Baseline

- Binary: `control_flow_gcc-m32_O2.exe`
- Function: `saturating_add`
- Symptom: `if (ecx + edx < ecx) return INT_MAX; return eax;` with **no**
  `eax = ecx + edx` — sum inlined into compare only; epilogue returns bare `eax`

## 2. Owner

- [x] Builder/materialize: `output_replacement_is_complete` + complete-plan early
  `return Ok(None)` (representative_downgrade)

## 3. Invariant

```text
A write to the ABI primary return register must be materialized as an HIR
binding even when same-block p-code consumers can fully inline the RHS.
SLEIGH Return inputs are control/stack targets, so consumer analysis does not
see the ABI live-out use of the return register.
```

## 4. Validation

- Synthetic: lea/add into primary return reg + later Return join keeps `eax = …`
- Real: saturating_add shows `eax = param_1 + param_2` (or ecx+edx) before uses
- Benchmark: fission-benchmark local docker runner (see docs/BENCHMARK_DOCKER.md)
