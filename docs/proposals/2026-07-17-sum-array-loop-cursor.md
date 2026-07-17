# Decompiler change proposal: loop pointer cursor (sum_array)

**Date:** 2026-07-17  
**Status:** implemented (local remeasure + nextest green; official ranking pending)  
**Gate:** ADR 0006 + Agents.md Core Rule 10 (measurement-only)

## 1. Row anchor (measured)

| Field | Value |
|-------|--------|
| Source | `fission-benchmark/corpus/dev/source/c/data_structures.c` `sum_array` |
| Binary | `data_structures_gcc-m32_O2.exe` |
| Address | `0x4015b0` (`_sum_array`) |
| Oracle | PE wine semantic wrapper |
| Evidence run | CI artifact `29594922653` `results/dev_latest.json` |
| Status | `fail_category=timeout`, `semantic_score=0.0`, `gotos=1` (HIR path) |
| Local remeasure | `fission_cli decomp … --layer both` @ commit workspace `main` |

### Assembly (ground truth)

```text
mov eax, [ebp+8]      ; cursor = items
lea ecx, [eax+4*edx]  ; end = items + len
xor edx, edx          ; sum = 0
L:
  add edx, [eax]      ; sum += *cursor
  add eax, 4          ; cursor += 4
  cmp eax, ecx
  jnz L
```

### Current NIR (local)

```c
uVar1 = param_1;
ecx = uVar1 + param_2;
edx = 0;
do {
    edx += *uVar1;   // BUG: frozen base, never advances
    eax += 4;        // BUG: eax not seeded to param_1 / not same as load base
} while (eax != ecx);
```

### Failure mode

Infinite loop / wrong loads → oracle **timeout** (measured), not adapter_error.

## 2. Owner proof

| Layer | Verdict |
|-------|---------|
| SLEIGH / raw p-code | Unlikely — disasm is a textbook load/add/cmp loop on EAX |
| **Materialize / loop-carried** | **Primary** — load address not bound to loop-carried EAX MultiEqual; preheader copy used for load |
| Structuring | Secondary (do-while shape OK) |
| HIR presentation | **Worsens** local HIR: drops `uVar1`/`ecx` defs, leaves `eax`/`ecx` use-without-def |
| Printer | Not owner |

## 3. Invariant (ISA-agnostic)

For a natural loop body that:

1. **Loads** through address expression *A*,
2. **Updates** a register/slot *R* by a constant stride (or scaled pointer step),
3. **Compares** *R* to a loop-invariant end pointer,
4. Where preheader defines *R₀* as the same base used to form the end pointer,

then the load address *A*, the update of *R*, and the compare of *R* must refer to the **same loop-carried binding** (phi of *R*), not a frozen preheader copy of the base that is never updated in the body.

Forbidden: load from frozen `uVar = base` while updating a distinct unbound `eax` used only in the exit compare.

## 4. Validation matrix

1. Synthetic pcode/HIR test: m32-style pointer scan loop → single cursor for load+add+cmp.
2. `cargo nextest run -p fission-pcode` (loop_carried / materialize filters).
3. Local remeasure: `fission_cli decomp data_structures_gcc-m32_O2.exe --addr 0x4015b0 --layer both`.
4. Focused benchmark row: `sum_array` gcc-m32 -O2 semantic score (timeout → pass).
5. Regression: other loop-carried tests; smoke if available.

## 5. Non-goals

- HIR `while→for` sugar without this semantic fix.
- Binary/address special cases.
- Changing oracle timeouts to hide infinite loops.

## 6. Next implementation step

Inspect MultiEqual / loop-carried materialize for EAX in the loop body LOAD path; ensure LOAD uses the phi binding that INT_ADD updates.
