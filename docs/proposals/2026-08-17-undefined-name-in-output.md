# Output reads names that are never defined

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O1.exe`
- Function/address: `list_sum` at `0x14000153a`
- Source: `corpus/dev/source/c/advanced_patterns.c:20`

```c
int list_sum(const Node *head) {
    int total = 0;
    const Node *cur = head;
    while (cur != NULL) { total += cur->value; cur = cur->next; }
    return total;
}
```

- Current output (identical in NIR and HIR):

```c
ulonglong list_sum(uint * ptr)
{
    ulonglong rax;
    ulonglong xVar11;
    ulonglong xVar16;          // never assigned

    if (ptr) {
        rax = 0;
        do {
            rax += *ptr;
            xVar11 = ptr->field_8;
            ptr = (uint *)(xVar11);
        } while (xVar11);
        return xVar16;         // <- should be `return rax;`
    }
    return 0;
}
```

The loop is correct: `rax` accumulates exactly what the source accumulates.
Only the return is wrong, and it is wrong in the strongest sense — compiling
this returns an uninitialized value.

## 2. Scope

Measured across 465 corpus functions, counting only names Fission itself
mints (`bVar`/`iVar`/`uVar`/`xVar` + digits, raw register names, `home_N`,
`local_N`) that are declared, read, and never assigned:

| | |
|---|---|
| functions affected | **93 / 465 (20%)** |
| occurrences | 157 |
| builder temps | 83 |
| raw registers | 55 |
| `home_N` stack slots | 15 |
| `local_N` | 4 |

By compiler: gcc 32, clang 27, go 13, gcc-m32 12, rustc 9. By optimisation:
-O0 37, -O2 17, -O1 9, -O3 8. It is not specific to a front end or a
pipeline mode.

Correlation with structural correctness, scored by fission-benchmark's
`runner/ged.py`: of the 60 affected functions that are also GED-scored, 47
fail exact structural match. That is a correlation and not the primary
argument. The primary argument is that the output is wrong.

## 3. What is established

`FISSION_PREVIEW_DIAG=1` on the anchor row prints return recovery three
times for the same block, on the same code path, with two different answers:

```text
[DIAG] return recovery: block=3 path=epilogue_join_live_primary expr=Var("rax")
[DIAG] return recovery: block=3 path=epilogue_join_live_primary expr=Var("rax")
[DIAG] return recovery: block=3 path=epilogue_join_live_primary expr=Var("xVar16")
```

`live_primary_return_register_expr`
(`builder/control/terminator.rs:1177`) lowers the primary return register
with `lower_wrapped_varnode`. Lowering is not a query — it mints names and
consults builder state — so the same varnode at the same site resolves
differently once merge synthesis has run in between. The last answer is the
one that reaches the output, and its binding was never written.

Also established:

- **Not caused by this session's work.** Byte-identical output before the
  frame-pointer spill fix (`949d92001`), at that commit, and after the
  AST-stage copy propagation (`a1e90d24a`).
- **Not a rendering problem.** NIR carries it too, so the defect is at or
  before the builder.
- `return rax;` is the correct output; nothing else in the function is wrong.

## 4. What was tried and did not work

Hypothesis: merge synthesis skips the phi because
`phi_output_is_consumed_in_block`
(`builder/materialize/cross_block.rs:1398`) only looks for consumers inside
the phi's own block, and `list_sum`'s consumer is the `return` one block
later. Widening the test to the whole function would then emit the missing
assignment.

Implemented as `phi_output_is_consumed_anywhere`, scanning all of
`scalar_ssa.operation_inputs`. **Zero effect** — output byte-identical on the
anchor row and on ten sampled affected functions. Reverted.

Instrumenting the phi loop shows why the hypothesis was wrong: block 2 has 17
phis, and `consumed_anywhere` is false for essentially all of them, including
both phis on the register-space slot the return uses (`sp4:off0x0:sz4`
appears twice, once true and once false). The phi output ids are simply not
present in `operation_inputs`, so neither the narrow nor the wide test can
see a consumer. Whatever route resolves the primary return register to
`xVar16` is not this gate.

## 4b. Second attempt: narrowed to the exact trigger, cause still unfound

Instrumenting `live_primary_return_register_expr` to dump `materialized_vns`
alongside each answer shows the flip precisely. The failing call is block 4
(the join), `ret_vn = sp4:off0x0:sz8` (RAX), and the entries for that varnode
across the three calls are:

```text
call 1: ["def@0x14000153f:10->rax"]                                   => Const(0)
call 2: ["def@0x14000153f:10->rax", "def@0x140001544:17->rax"]        => Const(0)
call 3: [... , "def@0x140001550:41->xVar16", ...]                     => Var("xVar16")
```

One entry appears between calls 2 and 3 — `def@0x140001550 seq 41 -> xVar16`
— and `lower_varnode` trusts it from then on. So the trigger is exact: **a
`materialized_vns` entry exists whose name has no binding written into the
body.** Block 2's own answer stays `Var("rax")` throughout; only the join
flips.

Who writes that entry is still unknown. Both production insert sites were
instrumented for this specific name and neither fired:

- `builder/mod.rs:888`, the name-minting path — no output.
- `structuring/host_impl.rs:36`, the isolated-lowering identity commit — no
  output (it only logs when the key is genuinely new, so an existing key
  passes silently).

`PreviewBuilder::clone()` gives an isolated fork the whole map, which is the
remaining candidate: a name minted inside a fork can reach the host without
either instrumented line reporting it.

## 5. Where to look next

**Stop guessing insert sites.** Three hypotheses were formed by reading the
code — the phi consumption gate, the minting path, the isolation commit — and
all three were wrong. The anchor reproduces in seventeen lines and the
trigger is now exactly known, so the missing information is *who writes the
entry*, and that should be observed rather than deduced: wrap
`materialized_vns` in a type that records every write with a backtrace, or
snapshot the map around each pass and diff. Either answers the question in
one run.

Once the writer is known, the fix is likely one of:

1. refusing to record a `materialized_vns` entry whose binding was never
   written, or
2. emitting the binding the entry promises, or
3. dropping fork-local entries at commit unless their statements came with
   them.

An invariant worth asserting once the cause is known, independent of the fix:
**every name that appears in rendered output has a definition in that
output.** It is cheap to check, it is unambiguous — unlike goto count or CFG
node count it needs no judgement about whether output got "better" — and this
proposal exists because nothing checks it today.

## 6. AI Review / Prompt Firewall

- No external model was asked for implementation advice.
- No vendor tree was consulted for this diagnosis.
- The failing hypothesis in section 4 is recorded deliberately: it is the
  obvious reading of the code and the next person will otherwise spend the
  same time on it.
