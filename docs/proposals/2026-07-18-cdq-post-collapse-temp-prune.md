# Decompiler change proposal: prune unused temps after CDQ collapse

**Date:** 2026-07-18  
**Status:** measured / landed  
**Gate:** ADR 0006 + measurement-only

## 1. Row anchor

| Field | Value |
|-------|--------|
| Source | `math.c` `gcd` |
| Binary | `math_gcc_O0.exe` PE x64 |
| Address | `0x1400017fd` |

### Baseline NIR (after CDQ signed-% fix)

```c
ulonglong home_0;   // intentional NIR home (layered policy)
longlong xVar15;    // declared, never assigned/used after CDQ collapse
param_18 = (int)param_10 % (int)param_18;
```

`xVar15` is residual noise from wide dividend temp before collapse.

### After

```c
// no xVar15 decl
param_18 = (int)param_10 % (int)param_18;
```
`home_0` remains in NIR (layered home-slot policy).

## 2. Owner

| Layer | Verdict |
|-------|---------|
| CDQ collapse | Correct — rem is signed low % |
| **Pipeline stages order** | **Primary** — `prune_unused_*` runs *before* CDQ; no re-prune after |
| Printer | Must not invent decl cleanup only |

## 3. Invariant

After a late normalize pass removes the only uses of a trivial temp name (`xVar*`, etc.), the binding must be pruned from `func.locals` in the same pipeline tail (or a subsequent prune pass).

## 4. Validation

1. Unit: function with unused `xVar15` binding after CDQ-style body → prune removes it.
2. Local remeasure gcd: no `xVar15` in NIR decls.
3. `cargo nextest run -p fission-midend-normalize -p fission-pcode`.
4. Do not remove NIR `home_0` (layered policy keeps home slots in NIR).
