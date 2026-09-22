# Call floating-point return recovery

## Measured row

DecBench `dev` row `libc_types_gcc_O2.exe::days_between` at `0x1400016e0`
currently emits a discarded `_difftime64` call followed by a division of the
stale `xmm0_qa` binding. The focused seven-row `libc_types` rerun at the
`149df3a71` baseline measured `0/35` semantic cases and `compile_error` on all
rows; the x64 row also emitted `extern unsigned long long _difftime64(...);`,
which is not a valid C11 declaration.

The raw p-code is:

```text
Call        - <- _difftime64
FloatDiv    XMM0_Qa <- XMM0_Qa, RAM[0x140004030]
FloatTrunc  EAX     <- XMM0_Qa
IntZExt     RAX     <- EAX
```

The call has no output varnode by design. Its result is the ABI-owned floating
return carrier `XMM0_Qa`, but call-result priming only considers the integer
`primary_return_registers()` list and therefore resolves the later read to a
pre-call value.

## Invariant and owner

The canonical owner is the cspec-driven call-result machinery in
`midend/builder/materialize/call_results.rs` and the register model in
`midend/cspec/register_model.rs`. An observed call result may use either the
integer or floating-point ABI return carrier; the carrier must be selected from
the actual dependency use, and a floating carrier must retain its scalar float
type through the call expression and binding.

Unknown-arity called externs must also remain valid C11 declarations. The
renderer currently writes `f(...)` without a named parameter, which C rejects;
the generic unspecified-parameter declaration `f()` is the compatible fallback
when no signature is known.

## Validation plan

1. Add focused cspec/register and synthetic call-result regressions for integer
   and floating return carriers.
2. Re-render the anchored `days_between` row and verify the call result is
   assigned to `xmm0_qa`, divided as a `double`, and converted to the integer
   return.
3. Re-run the same seven-row DecBench slice with caches disabled.
4. Run pcode tests, workspace checks, formatting, release build, and the
   existing decompilation smoke checks.

No function name, address, binary, or ISA-specific guard is part of the fix.
