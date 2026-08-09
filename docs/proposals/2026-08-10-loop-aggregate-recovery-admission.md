# Loop Aggregate Recovery Admission

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/advanced_patterns_gcc_O0.exe`
- Function: `list_sum`
- Address: `0x140001530`
- Corpus row or benchmark command: `target/release/fission_cli decomp <binary> --addr 0x140001530 --profile nir --json --debug-decomp`
- Baseline output summary: parameter was rendered as `int *`; the second object member was `*(ulonglong *)(xVar22 + 2)` and no aggregate fields were recovered.
- Semantic cases passed / total: N/A for this type-parity row.
- Failure category: type/data recovery admission.
- Relevant benchmark/static/readability observations: type parity is `field_layout` mismatch with `ref_fields=2`, `cand_fields=0`, and `type_token_jaccard=0.0`. Telemetry reports `aggregate_fields_skipped_by_admission_count=1`, `memory_fact_prefilter_skip_count=2`, and zero object-root/object-shape promotions.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
run_stage_memory_recovery computes:

memory_fact_prefilter_allows_full(func) && !body_has_loopish_shapes(&func.body)

The row has a pointer parameter and constant-offset loads at object offsets
0 and 8, but its `for` loop makes this admission false. After opening that
admission in the focused invariant test, `can_upgrade_binding_to_aggregate`
still rejects the already inferred `Ptr(Int32)` even though the collected
access widths are heterogeneous (4 bytes at offset 0, 8 bytes at offset 8).
The real row then exposes one more owner-native split: offset 0 is accessed
through `local_10`, while offset 8 is accessed through the copy alias
`xVar22`; telemetry records two object roots but zero object shapes. The
existing shared `DefinitionDependencyMap` already owns definition provenance,
so an identity-preserving subset can project an alias access back to its
tracked pointer roots without inventing a parallel alias analysis. The first
post-change real-row measurement also exposed an offset-unit bug: the surviving
C pointer expression `int_ptr + 2` was recorded as byte offset 2 instead of
object-layout byte offset 8.
The existing collector already recursively visits While/DoWhile/For bodies
and its fact pass is a bounded AST walk; the loop shape does not make those
memory facts invalid. Heterogeneous constant-offset access widths are record
evidence that a homogeneous scalar array does not explain.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Constant-offset typed accesses inside structured loop bodies are valid object
shape evidence. Aggregate-field admission depends on pointer/memory-surface
interest and the existing function-size budget, not on the mere presence of a
loop. A pointer with a previously inferred scalar pointee may be upgraded only
when the object facts contain at least two distinct access widths; homogeneous
constant accesses remain scalar/array-like. Expensive stack-slot surfacing may
retain its separate loop admission.

Pointer-identity-preserving definition dependencies impose compatible
object-shape constraints. Attribute a typed access to tracked roots reachable
through exact copies, casts, and selects; do not traverse pointer arithmetic,
loads, calls, aggregate copies, or other provenance barriers. Object-layout
facts use byte offsets: normalize a direct concrete `T * +/- constant` element
count by `sizeof(T)`, while leaving byte-based `PtrOffset` unchanged. Apply the
same unit witness in field-access rewriting, using the expression's pointer type
because the carrier binding may already have been promoted to an aggregate.
```

ISA-agnostic check:

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data remains in register/calling-convention models.
- [x] Synthetic test uses a structured loop and constant memory offsets only.

Comparable coverage:

- Similar shape 1: linked-list traversal reading fields at offsets 0 and pointer-size.
- Similar shape 2: record traversal in a `while`, `do-while`, or `for` body with heterogeneous field widths.
- Synthetic invariant test: memory recovery admits aggregate facts across loop-carried pointer aliases, including a direct `int * + 2` element offset normalized to byte offset 8.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `run_stage_memory_recovery` admission, `apply_aggregate_fields_pass`, typed object facts, and shared `DefinitionDependencyMap` provenance.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [x] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: the existing collector and aggregate pass already support nested loop statements, and the existing shared dependency map supplies alias-root evidence. No parallel alias pass is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass, helper, or metric is added.
- Possible interaction with existing normalize/structuring/materialize passes: aggregate recovery runs after pointer arithmetic and dead/redundant memory cleanup as before; only loop-bearing functions with pointer/memory interest become eligible.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: existing skip and object/shape promotion counters change naturally; no new telemetry.
- Known cases that must not change: loop-free aggregate recovery, non-pointer functions, homogeneous scalar arrays, and heritage/slot-surfacing loop admission.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize -E 'test(memory_recovery_admits_aggregate_facts_inside_structured_loop)'`
  - Expected signal: parameter becomes `Ptr(Aggregate)` with fields at offsets 0 and 8.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: all fission-pcode tests pass.
- [x] Focused benchmark row:
  - Command: rerun the exact `list_sum` row with caches disabled/new release bundle.
  - Expected row-level improvement: aggregate promotion counters become non-zero and raw offset dereferences become field accesses; no semantic regression.
  - Measured result: the same row now emits a 16-byte aggregate with fields at byte offsets 0 and 8, and the access is `xVar22->field_8`. Existing object-shape and surface-promotion counters move from 0 to 4. External type parity changes from `field_layout` mismatch (`ref_fields=2`, `cand_fields=0`) to match (`ref_fields=2`, `cand_fields=2`, layout Jaccard 1.0).
- [x] Smoke or automation sample:
  - Command: external `type_parity --corpus dev --limit 20` plus cache-disabled DecBench core local evaluation.
  - Measured signal: type parity remains 2/20 matches (the two `list_sum` O0 compiler rows), aggregate field-layout evidence is non-zero on 8/20 rows, and DecBench produces all 165/165 target functions with zero errors.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize && cargo check -p fission-pcode && cargo build -p fission-cli --release`
  - Expected signal: clean compilation.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not required.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model implementation review was requested.
- Information exposed in an AI prompt: N/A.
- Redaction confirmed: N/A; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: reference evidence only.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: external 20-row type sample moved from 1 to 2 matches; O1/O2/Os/O3 and most `kv_lookup` rows remain unresolved.
  - Synthetic invariant test command/result: focused loop-alias, pointer-unit, and homogeneous-array tests pass.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
