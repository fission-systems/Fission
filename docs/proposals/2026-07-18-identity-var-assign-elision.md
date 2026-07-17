# Decompiler change proposal: elide pure identity var assigns

**Date:** 2026-07-18  
**Status:** implemented (local remeasure + nextest; official oracle pending)  
**Gate:** ADR 0006 + Agents.md Core Rule 10 (measurement-only)

## 1. Row anchor (measured)

| Field | Value |
|-------|--------|
| Binary | `math_gcc_O0.exe` (PE x64) |
| Address | `0x140001530` (also iterative loop / saturating paths) |
| Local remeasure | after call-result successor fix (`0ab783d0`) |
| Pattern count | 5 pure `name = name;` lines in NIR inside an outer `{ }` block |

### Snippet (local)

```c
uVar1 = __gvn_join_6;
uVar1 = uVar1;   // pure identity (inside Block)
…
rbx = rax;
rbx = rbx;       // pure identity
```

## 2. Owner proof

| Layer | Verdict |
|-------|---------|
| Materialize | Partial — can elide at emit time |
| **Normalize `eliminate_redundant_var_assigns`** | **Primary** — already removed top-level `x = x`, but **did not recurse into nested Block/If/loop** bodies (O0 structured wrap) |
| HIR presentation | Do **not** add aggressive identity scrub here without guarding alias rewrites (`rbx = rax` then `rax += rbx` must not become `rax += rax`) |
| Printer | Not owner |

## 3. Invariant

`eliminate_redundant_var_assigns` must apply the same self-assign / adjacent-duplicate rules in every nested statement list (Block, If arms, loops, switch cases), not only the top-level body vector.

## 4. Validation

1. Unit: nested Block containing `uVar1 = uVar1` is cleaned.
2. Existing top-level self-assign unit still passes.
3. Local remeasure: self-assign count → 0; call results still summed via saved register.
4. HIR must not rewrite `rax += rbx` into `rax += rax`.
5. `cargo nextest run -p fission-pcode` (+ normalize crate tests).

## 5. Non-goals

- Casting self-assigns `x = (T)x`.
- Presentation-only alias folding fixes (separate if needed).
