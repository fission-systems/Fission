# Decompiler Problem Inventory

- **Date:** 2026-07-10
- **Scope:** Shipped NIR/HIR decompile quality (x86/x86-64 primary)
- **Related:** [ADR 0008](../adr/0008-nir-substrate-and-owner-boundaries.md),
  [ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md),
  [ISA debt table](2026-07-10-isa-semantic-debt-inventory.md)

## Executive summary

Current quality gaps are dominated by **intra-function NIR owner bugs**
(materialize, terminator, structuring, normalize), not by missing static
FactStore depth or SMT solver coupling. Static/type facts and solver/emulator
help later ceilings (API types, path validation); they do not fix cmov/return/
binding-order failures by themselves.

## Priority ranking (ROI)

| Pri | Theme | Owner | Why high ROI now |
|-----|--------|--------|------------------|
| P0 | Distinct live values share one binding (param/sum/return collapse) | materialize / variable_merge | Wrong C that cannot compile or is semantically false |
| P0 | Stack-param → reg alias does not dominate first use | materialize (hoist is bandage) | Use-before-def; D1 emergency pass |
| P0 | Return value recovery on epilogue joins / cmov arms | terminator | Empty or wrong `return` |
| P1 | Flag/cmov chains not folded to relational values | materialize + flag recovery | classify_range / clamp noise |
| P1 | Control structure (goto, nested if, switch-like) | structuring | saturating_add / classify_range readability |
| P2 | Type/role (size_t vs pointer, signatures) | type recovery + static facts | checksum-style rows |
| P3 | Broader static FactStore fixed-point | fission-static + decompiler | Real-world API ceilings |
| P3 | Solver in decomp hot path | — | Prefer emulator/validation lane |

## What is *not* the main problem (for now)

- “We don’t depend on static enough” — partial only; static helps types/API, not
  local binding/control facts.
- “We need SMT in the decompiler core” — wrong layer for most residual rows;
  keep `fission-solver` with emulator / optional narrow queries.

## Concrete residual signals (m32 O2 control_flow)

Measured with release CLI against `control_flow_gcc-m32_O2.exe` (2026-07-10).

| Function | Status | Top defects |
|----------|--------|-------------|
| `count_bits` | Good | Loop + return OK |
| `clamp` | Partial | Semantic shape OK; flag temps (`of`/`sf`/`zf`) noise; hoist-dependent param order |
| `signum` | Partial | Returns 1 / -setnz path; extra block braces; return type uchar |
| `checksum` | Partial | Loop OK; `len` typed as `uchar *`; cast noise |
| `saturating_add` | Partial | **F1** stopped `eax+eax` collapse. Current main-shaped output: `ecx/edx` live, `if (ecx+edx < ecx) return INT_MAX`, but **`return eax` without `eax = sum`** (live primary return without dominating def). **INT_MIN cmovl** is a tail-of-block absolute CBranch (next BB start); helper added in `cfg.rs` (`same_block_forward` tail skip) — wiring via terminator classification still needs a non-regressing materialize change (attempt 2026-07-10 regressed INT_MAX to `eax < eax`). |
| `classify_range` | Bad | Flag soup; setnz/neg/cmovg not recovered as value classes |

## Problem families (canonical)

### F1 — Binding / value identity

**Invariant:** Distinct reaching definitions (params, sum, INT_MIN, INT_MAX) must
not be force-merged into one name when path-sensitive or when storage is reused
only by the ABI return register after a real intermediate value exists.

**Evidence:** saturating_add overwrites `eax` with both params then uses
`eax + eax`.

**Forbidden:** Row-specific renames; “always keep eax” without def-use.

### F2 — Dominating param aliases

**Invariant:** After recovering `stack_slot → param_N` into a register, the
assignment `reg = param_N` dominates first use of `reg` in the emitted HIR.

**Evidence:** clamp without `hoist_param_alias` → use-before-def on `edx`.

### F3 — Primary return at joins

**Invariant:** When RET’s p-code input is a return *address*, the returned
expression is the live primary return register (or equal pred exprs when safe).

**Evidence:** epilogue multi-pred join (partially fixed 2026-07-10).

### F4 — Relational recovery from flags/cmov

**Invariant:** Cmp+flag+cmov/setcc sequences lower to relational ops or select
forms without mandatory dead flag stores in final HIR.

**Evidence:** classify_range, clamp LE arm still dumps `of`/`sf`/`zf`.

### F5 — Structuring legality vs readability

**Invariant:** Acyclic diamonds and saturating branches become if/else without
spurious goto when CFG postdom facts allow; switch-like chains prefer value
ranges when legal.

### F6 — Type/role

**Invariant:** Scalar accumulators and length parameters are not stuck as
pointer types without address-producing uses.

## Improvement plan (this cycle)

1. **P0 F1:** Fix saturating_add-class binding collapse (distinct live values on
   return-reg timeline) — synthetic test + real decomp.
2. **P0 F2:** Materialize entry stack-param aliases in load order → retire hoist
   when real clamp clean.
3. **P1 F4/F5:** classify_range / residual saturating control structure.
4. Track debt D1/D3+ in ISA inventory; no new emergency passes without removal path.

## Non-goals this cycle

- Big-bang static FactStore rewrite
- SMT in normalize hot path
- New ISA breadth beyond x86/x86-64 measurement
