# Decompiler change proposal: CDQ / signed rem collapse

**Date:** 2026-07-18  
**Status:** implementing  
**Gate:** ADR 0006 + measurement-only

## 1. Row anchor

| Field | Value |
|-------|--------|
| Source | `math.c` `gcd` (`while (b) { t=b; b=a%b; a=t; }`) |
| Binary | `math_gcc_O0.exe` PE x64 |
| Address | `0x1400017fd` |
| Asm | `cdq; idiv dword [b]` → EDX remainder |

### Current NIR

```c
xVar15 = (ulonglong)(int)((longlong)param_10 >> 32) << 32 | (ulonglong)param_10;
param_18 = xVar15 % param_18;
```

Should be signed `param_10 % param_18` (same evaluation as `idiv` rem).

## 2. Owner

| Layer | Verdict |
|-------|---------|
| SLEIGH | Correct CDQ = Piece(sign_fill, low) + IntSRem |
| **Materialize / lower_binary** | **Primary** — treat CDQ piece dividend as the low half for SRem/SDiv |
| Normalize | Optional residual fold of bit-or sign-fill patterns |

## 3. Invariant

When dividend of `IntSRem`/`IntSDiv` is `Piece(H, L)` and `H` is an arithmetic right-shift sign-fill of `L` (CDQ-class), the semantic dividend is the signed value of `L` alone.

## 4. Validation

1. Unit: piece+srem synthetic → `%` on low half only.
2. Local remeasure `gcd` @ 0x1400017fd.
3. `cargo nextest run -p fission-pcode`.
