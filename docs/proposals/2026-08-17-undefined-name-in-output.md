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

## 7. Fixed so far, and what the remainder is

Two branches closed:

| | |
|---|---|
| `447832f0a` | one merge name per varnode — `explicit_merge_bindings` is keyed by `(block, varnode)` and the hardware-name promotion only fires on a loop head, so a join minted a second name for the same register and registered it without an assignment |
| `8113d39dd` | a run-local read count retired a definition that an enclosing `for` header reads — **a regression this document's own author introduced in `a1e90d24a`** |

Measured across 465 corpus functions:

| | undefined fns | occurrences | bare compile |
|---|---|---|---|
| before `a1e90d24a` | 57 | 90 | 363 |
| with the regression | 89 | 142 | 371 |
| now | **53** | **84** | 370 |

Note what that table says about section 2's numbers. This document originally
reported 93 functions as a pre-existing defect and set priorities from it; 32
of those were a regression committed the previous day. The correct
pre-existing figure was 57. Comparing against the immediately preceding state
before calling anything "pre-existing" is not optional.

The remaining 84 occurrences are **not one cause**:

| kind | count | note |
|---|---|---|
| callee-saved / frame registers | 22 | `rsp` 13 (all Go), `r14` 8 (all Go), `rbp` 7, `ebp` 4 |
| argument-passing registers | 15 | `rdi`, `r9`, `r8`, `edx` |
| `home_N` stack slots | 15 | all rustc -O0 |
| other registers | 18 | |
| builder temps | 10 | |
| `local_N` | 4 | |

Read context: 70 on the right-hand side of an assignment, 9 in a condition,
4 in a return, 1 in a `for` header.

### Anchor for the argument-register branch

- Binary: `corpus/dev/binaries/c/advanced_patterns_gcc-m32_O0.exe`
- Function: `bounded_checksum` at `0x4016ef`, 25 lines, one bad name (`edx`)
- Source: `size_t n = len < max_take ? len : max_take;`

```asm
0x4016fc  mov EDX, [EBP+0x10]   ; max_take -> param_3
0x4016ff  mov EAX, [EBP+0xc]    ; len      -> param_2
0x401702  cmp EDX, EAX
0x401704  cmovbe EAX, EDX
```

```c
uVar7 = param_2;
xVar14 = param_2 <= edx && edx - param_2 != 0;   // edx never assigned
if (!xVar14) { uVar7 = edx; }
```

Both loads are cdecl stack arguments and both parameters are recovered — the
signature is `(uchar *param_1, int param_2, uint param_3)`. `[EBP+0xc]` folded
into its uses as `param_2`; `[EBP+0x10]` left `edx` raw and dropped the
assignment that would have defined it. `incoming_stack_argument_index`
(`midend/abi.rs:213`) computes index 2 for `+0x10` correctly, so the defect is
not classification but whatever connects a promoted parameter to the register
that loaded it. NIR carries it, so it is at or before the builder, and it
predates every change in this session.

The Go `rsp`/`r14` group and the rustc `home_N` group are almost certainly
separate again — Go uses its own register ABI and `r14` is its goroutine
pointer. Each branch needs its own observation.

### What the anchor's observation established

`param_3` is a fully recovered parameter — `func.params` carries
`("param_3", ParamIndex(2))` — so parameter classification is not the defect.
The `[EBP+0xc]` load folds into its uses as `param_2` and the `[EBP+0x10]`
load does not fold, leaving `edx` raw:

```c
uVar7 = param_2;                                  // eax load folded
xVar14 = param_2 <= edx && edx - param_2 != 0;    // edx left raw
if (!xVar14) { uVar7 = edx; }
```

The two loads differ in what consumes them. `cmovbe EAX, EDX` puts EDX inside
a **guarded cmov body**, and `materialize/mod.rs` refuses replacement there on
purpose, twice:

- `output_replacement_is_complete` returns `false` for any op inside a
  same-block-forward CBranch skip range;
- `build_replacement_value_plan` returns `incomplete` with
  `ConsumerRequiresStableRepresentative` for the same range.

Both carry the same reasoning: completing the replacement would let later uses
see the taken-path value unconditionally (the x64 clamp case, `cmovle` into R8
then `cmovge` from R8 collapsing to `max(lo, value)`). That reasoning is about
the cmov's *output*. Here the unresolved name is an **input** to the cmov,
defined before the guard, and refusing to resolve it does not protect
anything — it just leaves the reader an undefined name.

**That hypothesis is wrong.** Instrumenting both refusal sites: only one
fires, `plan_incomplete` at `0x401704` (the `cmovbe` itself) for
`sp4:off0x0:sz4` — EAX, the cmov's *output*, which is exactly what the
refusal is for. EDX never reaches either site.

Instrumenting the two loads instead shows they are perfectly symmetric, and
both resolve:

```text
[LOAD] addr=0x4016fc op_idx=18 out=sp4:off0x8:sz4 rhs=Var("param_3")   EDX
[LOAD] addr=0x4016ff op_idx=21 out=sp4:off0x0:sz4 rhs=Var("param_2")   EAX
```

So promotion, classification, and resolution all work for EDX exactly as they
do for EAX: the builder knows the load's right-hand side is `param_3`. The
assignment is lost somewhere after that, and the cmov path is not involved.

Instrumenting `maybe_materialize_output_stmt`'s return value closes the
builder half:

```text
[EMIT] addr=0x4016fc op_idx=18 -> Assign { lhs: Var("edx"),   rhs: Var("param_3") }
[EMIT] addr=0x4016ff op_idx=21 -> Assign { lhs: Var("uVar7"), rhs: Var("param_2") }
```

**The builder emits `edx = param_3;` correctly.** Both statements exist when
it hands the body on. In the final output `uVar7 = param_2;` survives and
`edx = param_3;` is gone, so a normalize pass removes it.

The asymmetry that decides which one survives is the **name**: EAX's
materialization took a fresh builder temp (`uVar7`), EDX's took the raw
hardware name (`edx`). A pass that retires assignments to raw register names
would drop one and keep the other, which is exactly the observed shape.

So the defect has two candidate owners and they need different fixes:

1. the naming choice — why does one materialization pick a temp and the other
   a hardware register name for two symmetric parameter loads?
2. the normalize pass that drops `edx = param_3` while its value is still
   read.

Ruled out by measurement, not by reading: parameter classification,
parameter promotion, RHS resolution, and the guarded-cmov refusal. The
remaining question is which normalize pass removes the statement, and the
same anchor answers it — snapshot the body before and after normalize and
diff, rather than reading passes and guessing which one it is.

Note that a false lead was ruled out on the way: the
`[DIAG] param_pointer_roles` line lists only parameters with an inferred
*role*, so `param_3` missing from it means nothing about whether it is a
parameter. Reading that line as a parameter list costs an hour.

## 8. Third branch closed, and one that is not a Fission defect

`363ef6369` closed the copy-chain branch: `copy_propagation_pass` admitted
`edx <- param_3` and `uVar8 <- edx` together, removed every entry's defining
copy, then substituted once — so replacing `uVar8` with `edx` reintroduced a
name whose definition had just been deleted. `resolve_copy_chains` walks each
target to its root first; cycles are dropped rather than resolved arbitrarily.

Running total on 464 corpus functions:

| | undefined fns | occurrences |
|---|---|---|
| session start (true pre-existing) | 57 | 90 |
| after the `a1e90d24a` regression | 89 | 142 |
| now | **50** | **80** |

### The Go group is a corpus address problem, not this defect

`rsp` (13), `r14` (8) and `rdi` (5) — 26 of the remaining 80 — are all Go
rows, and they are not Fission reading an undefined value. **Nine of the
fifteen Go rows decompile a different function than the manifest names:**

```text
manifest go_add_ints   -> runtime.duffzero
manifest (8 more rows) -> syscall.compileCallback
```

`runtime.duffzero` is a Go runtime fragment entered at a computed offset with
`rdi` (destination) and `xmm15` (zero) already established by its caller.
Reading those without defining them is what the routine *is*; there is no
defect in the output. The defect, if any, is that the manifest address
resolves to a runtime helper instead of the user function it names, which is
a corpus/function-discovery question and belongs to whoever owns the Go rows.

Excluding the Go rows, the remaining undefined names are roughly: `rbp` 7 and
`ebp` 4 (frame pointer, the other half of what `949d92001` fixed), `home_N`
15 (all rustc -O0), `r9`/`r8`/`rax` 10, builder temps 8, `local_N` 4. The
frame-pointer group is the most likely next branch to be a real single cause.

### The frame-pointer group is my own pass declining, correctly but too bluntly

`rbp` 7 + `ebp` 4 are the same `local_N = rbp` shape `949d92001` was written
for, and that pass *sees* them: on `matrix_multiply` at gcc -O0 it reports
`undefined_regs=["rbp"]`, finds the candidate, and judges `dead=false`.

The reason is its own conservatism. `dst_fate_in_stmt`'s catch-all arm returns
`Read` for any statement that mentions the destination anywhere, including
inside a nested construct. Here the overwrite is nested:

```text
[SPILL] local_8: read-verdict from
  While { cond: Const(1, Bool), body: [ ... local_8 = 0 ... ] }
```

At normalize time — before structuring turns this into
`for (local_8 = 0; ...)` — the initialiser sits inside a `While`, so "this
construct mentions `local_8`" is true and the pass declines rather than
reasoning about whether the mention is a read or a write.

That conservatism is correct as written: a nested construct may read the
value on one path and write it on another, and treating it as a write would
delete a live definition. Extending it safely means asking whether the
construct's *first* touch of the destination is a write on every path into
it, which is a real analysis rather than a match-arm tweak. Worth doing —
it is 11 of the remaining 54 non-Go occurrences — but it is not the one-line
change the shape suggests.

## 9. A measurement error underneath all of this

Checking every row's decompiled signature against the name its manifest gives
— the check that exposed the Go group — turns out to matter beyond Go, but
most of what it flags is not a defect. Of 464 rows, 90 name something other
than the manifest, and they fall into three kinds:

| kind | rows | verdict |
|---|---|---|
| stripped binaries (`+strip` variants): gcc 27, gcc-m32 22 | 49 | **correct behaviour** — no symbol to recover |
| wrong manifest address: go 15, g++ 13, rustc 13 | 41 | **corpus defect** |
| clang, gcc-elf, gcc-aarch64 | 0 | clean |

The 49 gcc/gcc-m32 rows are all `-O0+strip` / `-O2+strip`, where
`FUN_0x140001530` / `sub_4015b0` is the right answer. Comparing names on a
stripped binary measures nothing.

The 41 go/g++/rustc rows are a real corpus problem, and the symbol table
settles which side is wrong:

```text
manifest        rust_add_ints @ 0x140001920
symbol table    rust_add_ints @ 0x140001840
                rust_cstr_len @ 0x140001900 - 0x1400019b0
```

`0x140001920` is inside `rust_cstr_len`. Fission resolved the address to the
function containing it, which is correct; the manifest address is wrong.
Those rows measure something other than the function they name, in
fission-benchmark rather than in Fission.

Of the 50 rows counted as reading an undefined name, 21 are wrong-function
rows, so the genuine figure is **29**, not 50.

This document has now been wrong about its own headline number twice — 93
when 32 were a regression I had just committed, then 57 counting rows that
were not measuring their named function. A row is evidence only if it
decompiled the function it claims to, and that filter belongs in every
per-row axis.

The check needs the same care as the thing it checks. Two false readings
happened while building it: 32-bit PE prefixes an underscore, so a naive
comparison called all 104 gcc-m32 rows mismatched when 22 are; and the first
example printed for gcc was a stripped row, which made a clean `-O0` result
look like a naming failure.
