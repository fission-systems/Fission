# Decompiler Change Proposal: m32 CallInd args/result + host semantic ABI

## 1. Baseline

| Variant | Symptom |
|---|---|
| gcc-m32 -O0 apply_binop | CallInd wrong args; `return eax` unbound; local_0 was staging |
| gcc-m32 -O2 apply_binop | host shape OK; docker 0/6 — host harness truncates `uint` fp |

## 2. Owners (separated)

1. **Builder materialize** `call_result_registers`: include X86_32 (EAX) so CallInd binds result.
2. **Builder call_recovery**: recover cdecl **ESP-staged** Stores `[esp+4*i]` (offset ≥ 0) before CallInd; mark as recovered call-args.
3. **fission-benchmark semantic harness**: host-ABI adaptation for 32-bit surface — do not truncate function pointers to `uint` when recompiling on host (no `-m32` on arm64).

## 3. Invariants

```text
1) X86_32 primary return register is EAX; Call/CallInd results observed
   as ABI live-out must bind like other ISAs.

2) cdecl CallInd args may be staged as Stores to [esp+k] with k≥0 before
   the call (not only push-below-ESP). Recover by stack index k/ptr_size;
   skip constant return-address stores on the call instruction.

3) Host semantic harness runs native width. When decompiled formals use
   32-bit integer types for values that wrappers pass as host pointers
   (m32 surface), adapt formals to ulonglong for host recompilation only
   — do not change decompiler 32-bit surface output.
```

## 4. Validation

- Unit: x86_32 CallInd esp-staged args + eax result binding
- Host semantic: m32 O2 shape with uint formals via harness adapt → 6/6
- Docker apply_binop m32 variants
- CI Fast/Heavy on Fission; benchmark harness change is local fission-benchmark
