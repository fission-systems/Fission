# Byte Pointer Offset Contract

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `bubble_sort`
- Address: `0x4007f0`
- Corpus row or benchmark command: external DecBench local evaluation, x86 O2 `math.c`, with `DECBENCH_NO_CACHE=1`; focused decompilation via `target/release/fission_cli decomp ... --addr 0x4007f0 --layer both --json --debug-decomp --no-warnings`.
- Current output summary: both NIR and HIR change the two machine-level pointer advances from 4 bytes to 32 bytes (`(uint8_t *)(arr) + 32` and `(uint8_t *)(rax) + 32`). The SIMD pair load/compare remains low quality, and the generated C does not compile in the semantic harness.
- Semantic cases passed / total: NIR 0/5; HIR 0/5.
- Failure category: `compile_error`.
- Relevant benchmark/static/readability observations: focused DecBench scores are GED 34 (NIR), GED 35 (HIR), type match 0.4, and byte match 0.0. Ghidra/angr GED on the same row are 7/8. This change is scoped to the proven byte-offset corruption, not the separate SIMD-lane or CFG gaps.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
0x4007f5 raw p-code:  INT_ADD(unique:8) <- RDI:8, const(4:8)
0x40085e raw p-code:  INT_ADD(RAX:8)    <- RAX:8, const(4:8)

The normalized output instead contains:
  xVar1 = (uint8_t *)(arr) + 32;
  rax = (ulonglong *)((uint8_t *)(rax) + 32);

PreHirExpr::PtrOffset documents and prints its offset in bytes. During a later
recover_in_expr walk, ptr_arith.rs currently treats every positive PtrOffset
smaller than an inferred pointee size as an element index and multiplies it.
With the temporary/internal `ulonglong *` type, the already-byte offset 4 is
therefore multiplied by 8. Raw lifting is correct; normalize first creates the
wrong fact.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
PreHirExpr::PtrOffset.offset is a byte offset by IR contract. Once an address
expression has this representation, later type refinement may change the
pointee but must never reinterpret or rescale the offset. Element-count to byte
conversion must occur only while lowering a Binary pointer operation that
still carries an explicit unit witness; it cannot be inferred from a bare
PtrOffset after provenance has been erased.
```

ISA-agnostic check:

- [x] Production condition is not gated on an ISA or calling convention.
- [x] ISA-specific data remains in SLEIGH/register/calling-convention models.
- [x] Synthetic test uses only the IR byte-offset contract and a later pointee type.

Comparable coverage:

- Similar shape 1: a byte advance smaller than a subsequently inferred scalar pointee size.
- Similar shape 2: a loop-carried cursor whose load width differs from its source-level element type.
- Synthetic invariant test: `PtrOffset(Var("p"), 4)` remains 4 when `p` is inferred as `Ptr(u64)`.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `apply_ptr_arith_recovery_pass` and its `recover_in_expr` traversal.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: removing the ambiguous second scaling step restores the existing `PtrOffset` contract. No new pass or helper is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: none added.
- Possible interaction with existing normalize/structuring/materialize passes: pointer and aggregate recovery may refine the base type after `PtrOffset` formation; they must preserve its byte unit. Binary pointer arithmetic recovery remains unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: none.
- Known cases that must not change: typed Binary pointer addition rescaling, byte-cast recovery tests, aggregate field access recovery, and wide-stride array-of-aggregate recovery.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize -E 'test(ptr_offset_byte_unit_survives_later_pointee_refinement)'`
  - Expected signal: offset remains 4 and the pass reports no rewrite for the existing byte-offset node.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode && cargo nextest run -p fission-midend-normalize`
  - Expected signal: all tests pass.
- [x] Focused benchmark row:
  - Command: rebuild release CLI; rerun `bubble_sort @ 0x4007f0` and external DecBench with caches disabled.
  - Expected row-level improvement: both pointer advances remain 4 bytes; NIR/HIR GED and semantic status are reported honestly whether improved, unchanged, or regressed.
  - Measured result: both advances changed from the incorrect 32 bytes to the raw-p-code value of 4 bytes. GED stayed 34/35, type match stayed 0.4, byte match stayed 0.0, and both layers stayed at 0/5 semantic cases with `compile_error` because the separate SIMD-lane and return-type failures remain.
- [x] Smoke or automation sample:
  - Command: external cache-disabled x86 O2 DecBench sample plus `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/release/fission_cli --functions-limit 200` when practical.
  - Expected no-regression signal: no lost outputs or aggregate/type regressions.
  - Measured result: cache-disabled external x86 O2 DecBench produced 18/18 functions with zero errors for both NIR and HIR. Each retained GED0 2, Type1 1, Byte1 0, and Union 3, identical to baseline. The optional automation command was also attempted but could not start because the configured `benchmark/binary/x86-64/window/small/binary/c/test_functions.exe` fixture is absent from this checkout.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize && cargo check -p fission-pcode && cargo build -p fission-cli --release`
  - Expected signal: clean compilation.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not required.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model implementation review was requested.
- Information exposed in an AI prompt: N/A.
- Redaction confirmed: N/A; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: reference measurement only.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: external cache-disabled x86 O2 18-function sample retained identical perfect counts and zero errors for NIR/HIR.
  - Synthetic invariant test command/result: focused test passed; combined normalize/pcode gate passed 1284/1284 tests with one pre-existing skipped test.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
