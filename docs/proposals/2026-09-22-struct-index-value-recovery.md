# Decompiler Change Proposal

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/data_structures_gcc_O1.exe`
- Function: `find_pair_value`
- Address: `0x140001596`
- Corpus row or benchmark command:
  `runner.py --corpus dev --function find_pair_value --decompilers fission --run-mode local --no-resume`
- Current output summary: raw p-code and PreHIR preserve the match load at
  `base + 4`, but the final NIR renders it as `pairs[1]` after the debug
  `Pair*` type overlay. `pairs[1]` is an element-sized `Pair` advance, not the
  `value` field at byte offset 4.
- Semantic cases passed / total: 5/5 for `gcc -O0`; 0/5 for `gcc -O1`, with
  the latter blocked by generated-source compile errors unrelated to this
  field-shape defect. Across the focused nine-variant run, the current output
  is correct on the runtime-scored O0 row but the optimized rows expose the
  bad final expression.
- Failure category: optimized rows are reported as `compile_error`; the
  direct decompilation defect is a type/data recovery mismatch.
- Relevant benchmark/static/readability observations: current direct output
  contains `pairs->key` for offset 0 and `pairs[1]` for offset 4; the matching
  raw p-code is `LOAD(base + 4)`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
raw p-code match path:
  IntAdd unique:8 <- reg0x8, const(4:8)
  Load temp:u32 <- space3, unique:8
  Copy RAX:u32 <- temp

PreHIR:
  rax = param_1[1];

Final NIR:
  uVar6 = pairs->key;
  if (uVar6 == key) {
      rax = pairs[1];
  }
```

The raw address and PreHIR are correct. `apply_debug_struct_promotions` in
`crates/fission-pcode/src/midend/builder/type_hints.rs` rewrites `Load` and
`Deref` nodes at known debug-struct offsets, but its `Index` branch only
recurses. The preceding normalize pass has already represented the scalar
`base + 4` load as `Index(base, 1)`, so the field promotion is skipped and the
surface aggregate type changes the meaning of the surviving index.

## 3. Generality / Invariant Proof

Generalized rule:

```text
When an explicit one-level debug struct hint proves that a pointer base is a
record, a constant scalar index must be interpreted using the pre-promotion
element width. If index * sizeof(the existing element type) equals a declared
struct field offset and the access width fits that field, promote the access to
that field. Do not reinterpret the scalar index using the newly promoted
aggregate stride.
```

ISA-agnostic check:

- [x] Production condition is independent of ISA, function name, address, and
      compiler tuple.
- [x] No architecture-specific rule is added; the existing debug type hint
      and normalized expression widths provide the required facts.
- [x] Synthetic tests state only the pointer/index/type shape.

Comparable coverage:

- Similar shape 1: existing `sum_point` debug-struct promotion tests cover
  direct loads at offsets 0 and 4.
- Similar shape 2: `kv_lookup` in the same corpus has the same `key`/`value`
  layout and emits the same optimized `base + 4` match load.
- Synthetic invariant test: a scalar `uint32_t *` `Index(..., 1)` promoted by a
  `Pair { int key; int value; }` hint must become `FieldAccess(offset=4)` and
  print `p->value`, while a variable index remains an index.

## 4. Risk And Ownership Check

- Existing pass/owner that already owns this behavior: debug-struct type/data
  recovery in `builder/type_hints.rs`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the pass already owns the proof that
  a surface binding names a one-level struct and already converts constant
  `Load`/`Deref` offsets to named fields. Supporting the equivalent normalized
  `Index` form closes a representation gap without adding a new pass.
- Possible interaction with existing normalize/structuring/materialize passes:
  field promotion runs after normalize and before rendering; variable or
  aggregate indices must remain unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none expected.
- Known cases that must not change: genuine array indexing, variable indices,
  multi-level pointers, and accesses whose width exceeds the hinted field.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(type_hints_function_hints)'`
  - Result: 56 passed. The new regression renders `p->y` and rejects `p[1]`.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1088 passed, 1 skipped, 3 pre-existing failures in unrelated
    `lower_expr` tests (`diamond_join...`, `movzx_after_byte_add...`, and
    `x64_byte_add_movzx...`).
- [x] Focused benchmark row:
  - Command: local DecBench `dev` runs for `find_pair_value` and `kv_lookup`
    with `FISSION_BENCHMARK_NO_CACHE=1`, a commit-specific
    `BENCHMARK_IMAGE_ID_FISSION`, and a force-recreated local Docker service.
  - Result: `find_pair_value` semantic perfect variants 2/9 -> 7/9 and
    `kv_lookup` 2/9 -> 7/9. The optimized rows changed from `pairs[1]` /
    `items[1]` to `pairs->value` / `items->value`. Remaining compile errors
    are the separate missing `Pair`/`Kv` declaration issue in the benchmark
    translation-unit harness.
- [x] Smoke or automation sample:
  - Command: `cargo nextest run -p fission-emulator` and release CLI build.
  - Result: emulator 200 passed, 3 skipped; `cargo build -p fission-cli --release`
    passed.
- [x] Optional related checks:
  - Command: `cargo fmt --all --check && git diff --check`
  - Result: passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or
  benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
