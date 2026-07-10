# X86-32 Loop-Carried Self-Update Binding

## 1. Baseline Row Anchor

- Binary: `corpus/dev/binaries/control_flow_gcc-m32_O2.exe`
- Function: `count_bits`
- Address: `0x4015b0`
- Failure: semantic timeout / wrong popcount (2/6) from non-updating loop
- Observed HIR (before):

```c
do {
    ecx = param_1;
    ecx &= 1;
    edx = ecx;              // lost IntAdd self-read
} while (param_1 >> 1);     // lost IntRight store
```

- Expected shape:

```c
do {
    ecx = param_1;
    ecx &= 1;
    edx += ecx;
    param_1 >>= 1;
} while (param_1);
```

## 2. Owner Proof

- [x] Builder / materialize (`loop_carried`)
- [ ] Normalize
- [ ] Structuring
- [ ] Printer

Evidence (pre-normalize dump):

```text
edx = Add(Const(0), ecx)     // pre-loop xor inlined into self-read
uVar7 = Shr(param_1, 1)      // induction write not bound to param_1
```

Raw p-code is correct (`IntAdd edx,edx,ecx` and `IntRight eax,eax,1`). Failure is binding identity for confirmed loop-carried register updates when no prior materialized seed exists (x86-32 stack-param load into EAX; unmaterialized xor-zero of EDX).

## 3. Generality / Invariant Proof

```text
When a register write is a confirmed loop-carried update
(self-loop / backedge reaches tail, and the op or earlier loop ops
read the same register), the materializer must bind the write to a
stable identity and scope RHS lowering so self-reads use that identity
instead of inlining the pre-loop definition.
```

Fallbacks when no prior seed name exists:

1. Incoming stack-parameter load seed → `param_N` (x86-32 induction).
2. Hardware register name (EDX/EAX/…) so accumulators keep `x = x + y`.

Comparable shapes: x64 O2 `count_bits` (already correct via ABI register params); any O2 popcount/shift-reduce loop on x86-32.

## 4. Risk And Ownership Check

- Owner: existing `loop_carried` materialize path.
- No new pass/metric.
- x86-64 external-seed / explicit-merge HW naming extended to `X86_32` only.
- Must not promote ABI scratch to `param_N` (covered by existing test, relaxed to allow HW name).

## 5. Validation Matrix

- [x] Targeted unit test: `m32_popcount_loop_carries_add_and_shr`
- [x] Existing `loop_carried` suite green
- [x] Real binary decomp: m32 O2 `count_bits` shows `edx += ecx` / `param_1 >>= 1`
- [x] Regression spot-check: m32 O0 `count_bits`, x64 O2 `count_bits`
- [ ] Optional: full m32 control_flow smoke (local docker; do not promote latest)
