# Decompiler Change Proposal: Loop-Carried Pointer Advance Silently Dropped

Date: 2026-08-05

## 1. Context

Continuing the Fission-vs-Ghidra gap investigation, moved to the benchmark's
`timeout` bucket (16 rows), concentrated in a handful of functions --
`list_sum`, `kv_lookup`, `find_pair_value`, `state_machine_score`,
`linear_search` -- mostly at `-O2`.

**First, a correction to the standing assumption behind this bucket.** The
backlog item this was tracked under ("SESE 구조화의 근본 알고리즘 병목
분석" -- SESE structuring's algorithmic bottleneck) assumed `timeout` meant
Fission's decompiler itself was slow. Reading the benchmark oracle's source
(`fission-benchmark/docker/oracle/server.py`) shows this is wrong: `timeout`
is set when *executing the recompiled decompiled code* against test cases
times out (`execute_program` hits a subprocess timeout) -- not when
decompiling times out. Confirmed directly: `list_sum @ advanced_patterns_
gcc_O2.exe` decompiles in 0.79s. There is no decompiler performance problem
in this bucket at all; every one of these functions was decompiling into
code that **infinite-loops when actually run**.

## 2. The bug

`list_sum`'s decompiled output:

```c
int list_sum(int * param_1)
{
    ...
    rax = 0;
    if (param_1) {
        do {
            rax += *param_1;
            xVar16 = *(ulonglong *)(param_1 + 2);
        } while (xVar16);
    }
    return rax;
}
```

Source: `while (cur != NULL) { total += cur->value; cur = cur->next; }`.
The decompiled loop reads `cur->next` into `xVar16` every iteration but
**never assigns it back to `param_1`** -- the loop condition and pointer
never change, so this infinite-loops on any list with more than one node.

## 3. Root cause

Traced via two temporary env-gated traces (added and removed for this
investigation): one dumping `normalize_hir_function`'s input shape (to
confirm the bug predates all `normalize` cleanup passes -- it does, so the
bug is in HIR/materialize construction, not a later dead-code pass), and
one inside `materialize/mod.rs`'s `maybe_materialize_output_stmt` dumping
`loop_carried_lhs_name` per op.

The trace showed `loop_carried_lhs_name` for the pointer-advance op *did*
correctly resolve to `Some("param_1")` -- `prove_loop_carried_register_update`
(`materialize/loop_carried/shape.rs`) correctly proved this definition
(a `Copy` writing the reloaded next-pointer back into the same register as
`param_1`, inside a self-looping single-block `do`/`while`) is the value
carried across the loop's back-edge. The proof and name resolution were
never the problem.

The problem is 12 lines further down in `maybe_materialize_output_stmt`:

```rust
if self.materialized_lhs_conflicts_with_load_address_role(&lhs_name, &rhs) {
    lhs_name = self.bind_materialized_output_to_fresh_temp(...);
}
```

`materialized_lhs_conflicts_with_load_address_role` fires when: (1) the name
was previously used as a load *address* (`self.load_address_bindings` --
true here, since `*param_1` is read earlier in the same loop body), (2) the
RHS is load-derived (true -- it's `*(param_1 + 8)`), and (3) the RHS's
p-code-inferred type is *not* `Ptr` (true -- raw p-code just says "loaded 8
bytes"; nothing has recognized `Node *` as a pointer type yet at this stage
of the pipeline). All three held, so this check overrode the correctly
*proven* loop-carried name and forced a fresh, unused temp -- which
`xVar16 = ...; xVar17 = xVar16;` (`xVar17` unused) confirms: the fresh temp
was never read by anything, so it silently vanished from the final output
with nothing to indicate the pointer update never happened.

The check exists for a real, different, *unproven* case (a name used as a
load address elsewhere getting silently repurposed for an unrelated
raw-integer value), and has no way to know a rigorous, narrowly-scoped proof
already established this exact definition as the genuine loop-carried
update -- it's a much blunter heuristic than
`prove_loop_carried_register_update`, and firing after that proof succeeded
overrides the more trustworthy answer with the less trustworthy one.

## 4. Fix

`maybe_materialize_output_stmt` now tracks whether `lhs_name` came from the
proven loop-carried path (`lhs_name_is_proven_loop_carried`) and skips the
load-address-role conflict check when it did:

```rust
if !lhs_name_is_proven_loop_carried
    && self.materialized_lhs_conflicts_with_load_address_role(&lhs_name, &rhs)
{
    lhs_name = self.bind_materialized_output_to_fresh_temp(...);
}
```

Scoped as narrowly as possible: only bypasses this one heuristic, only for
names that already survived the much stricter loop-carried proof, and
doesn't touch the heuristic's behavior for any other naming path (`merge_
lhs_name`, `direct_successor_merge_lhs_name`, the `live_register_lhs_name_
for_*` family, etc.).

## 5. Verification

- New unit test `loop_carried_pointer_advance_not_hijacked_by_load_address_
  role` (`materialize/loop_carried/tests.rs`): replicates the minimal shape
  (self-loop, `*param_1` read establishing the load-address binding, then
  `*(param_1+8)` reloaded and copied back into `param_1`) and asserts the
  advance is materialized under the stable `param_1` name.
- `cargo nextest run -p fission-pcode`: 969/969 passed (968 prior + 1 new).
- `cargo nextest run --workspace --no-fail-fast`: 2300/2307 passed, same 7
  pre-existing unrelated `fission-emulator` baseline failures.
- Real repro, `list_sum @ 0x140001550` in `advanced_patterns_gcc_O2.exe`:
  before, `param_1` never reassigned; after, `param_1 = (int *)(xVar16);`
  appears exactly where expected. Extracted the fixed function into a
  standalone `.c` file with a real 3-node linked list and confirmed with
  `gcc -w -O0`: **compiles, runs, terminates, returns `6`** (`1+2+3`,
  matching the hand-built list) -- before the fix, the equivalent extraction
  hung (`timeout 5` killed it, confirming the exact runtime-infinite-loop
  failure mode the benchmark's oracle records as `timeout`).
- git-stash A/B on seven real corpus binaries (`advanced_patterns_gcc_O2`,
  `control_flow_gcc-m32_O0`, `memory_layouts_clang_O0`, `math_gcc-m32_O2`,
  `memory_layouts_gcc_O0`, `crypto_gcc_O2`, `data_structures_gcc_O2`),
  decompiling every function in each: every diff is a previously-dropped
  register/pointer update now correctly appearing (`param_1 = (int *)
  (xVar16);`, `rbx--;`, `rbx = (uint *)(xVar17);`, an `if (*(uint*)(eax+8))
  { eax = *(uint*)(eax+8); } else {...}` branch replacing a broken always-
  same-path version) -- this bug affected multiple MinGW CRT-internal
  functions too, not just the benchmark's own source functions. No
  unexpected or unsafe-looking change in any of the seven diffs.

## 6. Known-separate, NOT fixed by this change

`kv_lookup` and `find_pair_value` (both `-O2`, both in the `timeout`
bucket) share a *different*, still-unresolved bug: the array pointer
advance (`param_1 += 2;`, `i++;`) is correctly materialized now, but the
generated control flow puts it **outside** a `while (1) { ... continue; }`
loop that a `continue` inside jumps back into -- meaning `continue` never
reaches the increment at all, and a non-matching first entry infinite-loops
regardless of this fix:

```c
uVar0 = 0;
goto block_x;
...
while (1) {
block_x:
    uVar16 = *param_1;
    if (uVar16 != param_3) {
        continue;   // loops back to block_x, never reaches the code below
    }
    return uVar0;
}
uVar0++;             // unreachable via `continue`
param_1 += 2;
if (xVar10 != uVar0) goto block_x;
return -1;
```

Confirmed via a standalone extraction + `timeout 3`: hangs (exit 124). This
is a goto/`continue`-target structuring bug, unrelated to loop-carried
naming -- left as a separate, documented follow-up rather than pursued in
this change.
