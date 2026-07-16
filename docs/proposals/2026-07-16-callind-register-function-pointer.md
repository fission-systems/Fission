# Decompiler Change Proposal: Register CallInd as function-pointer call

## 1. Baseline Row Anchor

- Binary: `advanced_patterns_gcc_O0.exe`
- Function: `apply_binop` @ `0x1400014fc`
- Semantic: **0/6** (`compile_error`)
- Asm: `call R8` after null-check of function pointer in RCX/home
- Bad output: `xVar19(a, b, fp);` / undeclared `xVar19` / return ignores call result

## 2. Owner Proof

- [x] Builder / call lowering + call-result materialize
- Raw p-code correct: `CallInd unique←R8` with ABI args in RCX/RDX
- First wrong fact: CallInd target Var lowered as **callable symbol name** instead of
  **function-pointer expression**; R8 (target carrier) also recovered as 3rd arg;
  call result not bound to primary return live-out

## 3. Invariant Proof

```text
CallInd whose target does not resolve to a known constant/IAT/function symbol
must lower as an opaque function-pointer call: (*(fp))(args…), where fp is the
lowered target expression.

Argument recovery for CallInd must not treat the register family that carries
the call target as a callee parameter slot.

When CallInd leaves the ABI primary return register live (no later redefinition
in-block), materialize `ret_tmp = (*(fp))(args)` so epilogue return recovery
can read the call result binding.
```

ISA-agnostic: CallInd p-code + ABI param slots + primary return registers.

## 4. Validation

- Unit: synthetic x64 `call r8` / null-check shape
- Docker local: `apply_binop` all variants; no clamp/signum regression
