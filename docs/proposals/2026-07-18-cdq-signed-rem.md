# Decompiler change proposal: CDQ / signed rem collapse

**Date:** 2026-07-18  
**Status:** measured / landed  
**Gate:** ADR 0006 + measurement-only

## 1. Row anchor

| Field | Value |
|-------|--------|
| Source | `math.c` `gcd` (`while (b) { t=b; b=a%b; a=t; }`) |
| Binary | `math_gcc_O0.exe` PE x64 |
| Address | `0x1400017fd` |
| Asm | `cdq; idiv dword [b]` → EDX remainder |

### Baseline NIR (before)

```c
xVar15 = (ulonglong)(int)((longlong)param_10 >> 32) << 32 | (ulonglong)param_10;
param_18 = xVar15 % param_18;
```

### After (measured)

```c
param_18 = (int)param_10 % (int)param_18;
```

(NIR + HIR; dead `xVar15` decl residual only.)

## 2. Owner

| Layer | Verdict |
|-------|---------|
| SLEIGH | Real form is `IntSExt` → `SubPiece` → `(ZExt(hi)<<32)\|ZExt(lo)` → `IntSRem` (not only Piece) |
| **Materialize / lower_binary** | **Primary** — collapse CDQ-class wide dividends on SRem/SDiv |
| Normalize | Residual fold + sequential live map (free-var kill) for across-temp uses |

## 3. Invariant

When dividend of `IntSRem`/`IntSDiv` is CDQ-class wide form of `L`:
- `Piece(H, L)` with `H` = `IntSRight` / `IntSExt` / `SubPiece(IntSExt(L), |L|)`
- or `IntOr(IntLeft(H, k), ZExt(L))` with the same `H`

the semantic dividend is the signed value of `L` alone. Reject bare `Copy`/`IntZExt`/`IntRight` as sign-fill.

## 4. Validation

1. Unit: Piece+SRem and SLEIGH Or/SubPiece+SRem → `%` on low half only.
2. Normalize: adjacent temp, multi-def kill, free-var kill, logical Shr reject.
3. Local remeasure `gcd` @ 0x1400017fd → signed `%` (measured).
4. `cargo nextest run -p fission-pcode -p fission-midend-normalize` (1152 passed).
