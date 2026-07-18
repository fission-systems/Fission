# Decompiler change proposal: collapse pure temp self-square chains

**Date:** 2026-07-18  
**Status:** measured / landed  
**Gate:** ADR 0006 + measurement-only

## 1. Row anchor

| Field | Value |
|-------|--------|
| Source | `math.c` `power` (`base *= base`) |
| Binary | `math_gcc_O0.exe` PE x64 |
| Address | `0x140001832` |

### Baseline HIR

```c
xVar23 = param_10;
xVar23 *= xVar23;
param_10 = xVar23;
```

Should read as `param_10 = param_10 * param_10` (or `param_10 *= param_10`).

## 2. Owner

| Layer | Verdict |
|-------|---------|
| **cleanup/temp_var** | **Primary** — pure temp self-square chain |
| Presentation | Secondary compound-assign polish only |

## 3. Invariant

When a linear sequence is:
1. `t = a` (pure copy, `t` trivial temp)
2. `t = t * t`
3. `a = t`
and `t` has no other uses in the linear scope, replace with `a = a * a`.

Also: rebuild pure-copy use counts per nested scope so multi-use predicates
(`iVar = exp; if (iVar == 0 \|\| iVar < 0)`) can inline.

## 4. Validation

1. Unit: square chain collapse; pure-copy multi-use predicate inline.
2. Remeasure power @ 0x140001832.
3. Remeasure gcd @ 0x1400017fd (no CDQ regression).
4. `cargo nextest run -p fission-midend-normalize`.


## After (measured)

NIR/HIR:
```c
param_10 *= param_10;
```
gcd @ 0x1400017fd unchanged: signed `%`.
nextest fission-midend-normalize: 262 passed.
