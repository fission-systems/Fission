# Decompiler Change Proposal: Retire `hoist_param_alias_copies` (D1)

**Status: aborted 2026-07-10 — hoist retained.**

## Investigation result

- Synthetic clamp HIR (already-ordered `edx = param_1`) passes without hoist.
- Real `control_flow_gcc-m32_O2.exe` `_clamp` **regresses** without hoist:

```text
// without hoist (broken)
uVar2 = param_3;
ecx = param_2;
of = __sborrow(edx, uVar2);  // use-before-def
...
edx = param_1;               // late

// with hoist (ok)
uVar2 = param_3;
ecx = param_2;
edx = param_1;
of = __sborrow(edx, uVar2);
```

- Root owner remains **builder/materialize**: first stack load of the value
  register is not emitted as a dominating `edx = param_1` before the first
  compare that uses `edx`. Other stack params (`param_2`/`param_3`) do get
  early aliases. Late reinjection of `edx = param_1` is what hoist reorders.

## Decision

Keep `hoist_param_alias_copies` as temporary debt (ADR 0009 §6) until materialize
emits dominating stack-param→register aliases for all loaded params in entry
order. Do not delete the pass based on synthetic-only evidence.

## Follow-up (not this change)

- Materialize entry stack loads: when `Load`+`Copy` recovers `ParamIndex` into a
  GP register, emit `reg = param_N` at the load site before any consumer.
- Then re-run real clamp without hoist; remove pass only if still clean.