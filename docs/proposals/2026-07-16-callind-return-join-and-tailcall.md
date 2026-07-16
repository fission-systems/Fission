# Decompiler Change Proposal: CallInd return join + x64 fp tail-call

## 1. Baseline

- `apply_binop` advanced_patterns
- gcc -O0: call form OK, **return wrong** (`!zf ? arg : 0` not call result) → assertion 2/6
- gcc -O2 / m32 -O2: **`jmp reg` tail-call** (BranchInd) → `__fission_branchind` compile_error

## 2. Owner

- [x] Builder terminator (BranchInd tail-call recovery)
- [x] Builder lower_varnode / diamond incoming (CallInd result vs last EAX write)
- [x] Builder call_recovery prefer_source peel for staged tail-call args

## 3. Invariants

```text
1) When reading the primary return register from a predecessor of a join, if that
   predecessor contains a Call/CallInd with a call-result binding, use that
   binding — not a pre-call write to EAX/RAX that only set up arguments.

2) BranchInd whose target is a register-held function pointer (param/reg after
   copy chain) is a tail call on x86/x64, not only ARM: recover
   return ((T(*)())fp)(args) with ABI arg recovery from the BranchInd block.

3) When prefer_source recovers a tail-call arg from Copy/ZExt of an ABI slot,
   peel same-block pure casts to the staged source register (mov ecx,edx;
   movsxd rcx,ecx → arg is edx/param_2, not ecx/param_1).
```

## 4. Validation

- Unit: diamond CallInd return prefers call binding; x64 BranchInd tail-call
  with staged (param_2, param_3)
- Docker: apply_binop variants; crate nextest 1220; CI Fast/Heavy

## 5. Cause separation (post-fix)

| Variant | Category | Owner residual |
|---|---|---|
| gcc -O0 | semantic **6/6** after return-join | done |
| gcc -O2 | runtime_error (compiles) | null-check cond may bind wrong param (`if (param_2)` vs `param_1`); args fixed to (param_2,param_3) |
| gcc-m32 -O0 | compile_error | undeclared `local_0` (stack slot materialize without decl) + cdecl stack arg shuffle |
| gcc-m32 -O2 | runtime_error | BranchInd/arg staging + cond on m32 cdecl |

Do not merge m32 `local_0` into the x64 tail-call owner; separate stack-home path.
