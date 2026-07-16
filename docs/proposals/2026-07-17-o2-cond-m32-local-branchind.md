# Decompiler Change Proposal: O2 test-cond freeze + m32 local rescue + cdecl restage

## 1. Baseline (apply_binop, local docker / host)

| Variant | Status | Symptom |
|---|---|---|
| gcc -O0 | 6/6 | done (prior cycle) |
| gcc -O2 | 0/6 runtime_error | `if (param_2)` should be `if (param_1)`; args already (param_2,param_3) |
| gcc-m32 -O0 | compile_error | undeclared `local_0` |
| gcc-m32 -O2 | runtime_error | `if (param_3)`; empty `(*)()`; ebp param-slot restage ignored |

## 2. Owners (separated)

1. **Branch predicate value lowering** (terminator): `test reg` after `mov reg, src; redef src` must lower the **snapshot at the copy**, not live redef of `src`.
2. **Undeclared binding rescue** (normalize cleanup): `local_*` stack-home names must be rescue candidates (not only `uVar*` / flags).
3. **x86_32 call_recovery**: BranchInd/CallInd tail-call may restage cdecl args via **frame-pointer-relative Stores** to `[ebp+8..]`, not only ESP pushes.

## 3. Invariants

```text
1) For X86 branch predicates EqZero/NeZero/… whose tested value is a register
   defined by Copy/Cast/ZExt/SExt from another register, lower the source at the
   defining op's site so later writes to that source are invisible
   (mov rax,rcx; mov ecx,edx; test rax,rax → value is original rcx / param_1).

2) Any body name matching stack-local surface `local_<hexdigits>` that is used
   but not declared is a rescue candidate (declare with inferred type).

3) On X86_32, when recovering tail-call args for CallInd/BranchInd, after
   ESP-push recovery, also collect same-block (and single-pred) Stores to
   frame-positive param slots [ebp+8 + 4*i] as contiguous stack args.
```

## 4. Validation

- Unit: O2-shaped `test rax` after rcx clobber; local_0 rescue; m32 BranchInd ebp restage
- `cargo nextest run -p fission-pcode` → 1221 passed
- Local docker apply_binop (quality-loop only):

| Variant | Before | After |
|---|---|---|
| gcc -O0 | 6/6 | **6/6** |
| gcc -O2 | 0/6 runtime | **6/6** |
| gcc-m32 -O0 | compile_error (`local_0`) | runtime_error (declared; cdecl CallInd args residual) |
| gcc-m32 -O2 | runtime_error (empty call / wrong cond) | host shape correct `if(param_1) return fp(param_2,param_3)`; docker still 0/6 (uint harness ABI / residual) |

## 5. Forbidden

- No function/address/binary-specific guards
- Do not fold m32 local rescue into x64 BranchInd owner
- Do not treat ebp restage as a general stack-slot rename pass

## 6. Residual (next cycle)

- m32-O0 cdecl CallInd: stack push arg recovery + call-result → eax binding
- m32 semantic harness vs `uint` signatures (host verifies ulonglong form 6/6; pure uint form crashes harness)
