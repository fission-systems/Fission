# Decompiler change proposal: call-result observation across successors

**Date:** 2026-07-18  
**Status:** implemented (local remeasure + nextest green; official oracle pending)  
**Gate:** ADR 0006 + Agents.md Core Rule 10 (measurement-only)

## 1. Row anchor (measured)

| Field | Value |
|-------|--------|
| Source | `fission-benchmark/corpus/dev/source/c/math.c` recursive integer function |
| Binary | `math_gcc_O0.exe` (PE x64) |
| Address | `0x140001530` |
| Evidence | `results/dev_latest.json` smoke matrix (fission): `semantic_score=0.5`, `cases=3/6`, `fail_category=assertion_fail` |
| Local remeasure | `fission_cli decomp … --addr 0x140001530 --layer both` |

### Assembly pattern (ground truth)

```text
call  <self>
mov   ebx, eax          ; save first return
… reload arg …
call  <self>
add   eax, ebx          ; second return + first return
```

### Current NIR (local)

```c
uVar1 = n - 1;
fibonacci((ulonglong)uVar1);   // result discarded
rbx = uVar1;                   // pre-call arg, not return
uVar7 = n - 2;
fibonacci((ulonglong)uVar7);   // result discarded
rax = uVar7 + rbx;             // wrong: args, not returns
```

### Failure mode

Semantic oracle assertion_fail (3/6 cases) — measured, not adapter_error.

## 2. Owner proof

| Layer | Verdict |
|-------|---------|
| SLEIGH / raw p-code | OK — CALL is block terminator; next BB uses EAX |
| **Materialize `call_result_is_observed`** | **Primary** — only scans same-block ops after CALL; terminator CALL never sees successor `mov ebx, eax` |
| live_call_result binding (cross-block read) | Already correct **if** binding is recorded |
| Structuring / HIR / printer | Not owner |

## 3. Invariant (ISA-agnostic)

When a `Call` / `CallInd` does not redefine the ABI primary return register later in the same block, and a successor block **uses** that return register (or a subregister alias) as an input **before** redefining it, the call result is **observed**.

Then materialize must bind the call as an assignment to the call-result binding (not a bare expression), so successor reads resolve via existing live-call-result logic.

Forbidden: discard call results and reuse pre-call argument temps that still occupy the same storage family.

## 4. Validation matrix

1. Unit: same-block observe still true; clobber-before-use still false.
2. Unit: CALL as last op + successor `Copy(EBX, EAX)` → observed true.
3. Synthetic lower: two recursive-style calls → results summed, not arg temps.
4. `cargo nextest run -p fission-pcode` (full crate regression).
5. Local remeasure at anchor address + `local_decomp_observe` session.
6. Official semantic oracle (docker) still required for quality claim close-out.

## 5. Non-goals

- Printer-only patches.
- Function/address special cases.
- Changing oracle cases to hide wrong results.

## 6. Implementation step

Extend `call_result_is_observed` with a successor-block use-before-redef scan after the existing same-block scan.
