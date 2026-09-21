# Preserve Byte Units Across Affine Pointer Recovery

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/data_structures_gcc_O2.exe`
- Function: `reverse_in_place`
- Address: `0x140001560`
- Corpus row or benchmark command: external DecBench local evaluation, `dev` corpus, focused on `reverse_in_place`, Fission local endpoint, `FISSION_BENCHMARK_NO_CACHE=1`; baseline artifact `results/issue104_before_62fea7840_recreated.json` with server fingerprint `c0059ccc3ab6804cdc8a7c94dd095cc9357b0db6c5b745912fba63a0c3e373c6`.
- Current output summary: the reverse cursor is rendered as `addr = (int *)(items + len - 4)`. The machine address is `items + len * sizeof(int) - sizeof(int)`; the output therefore loses the byte scale on both the dynamic span and the final element offset. After the fix the same row renders the equivalent `(uint8_t *)(items + len) - 4` address and preserves the swap cursor.
- Semantic cases passed / total: focused `gcc -O2` row `1/5`; across the nine `reverse_in_place` variants, `18/45` cases (`0.40` mean pass rate) and `3/9` perfect rows.
- Failure category: `assertion_fail` for `gcc -O2`, `gcc -O1`, and `gcc-m32 -O2`; the remaining variants include separate compile/runtime failures and are retained in the same before/after matrix.
- Relevant benchmark/static/readability observations: baseline row metrics for `gcc -O2` are source similarity `0.237`, GED `9`, type match `0.75`, recompilation `0.3667`, and `1/5` semantic cases. The output loses the distinct last-element address and produces the wrong swap endpoint. After the fix, that row is `5/5` semantic cases with no failure; the nine-row matrix improves from `18/45` to `27/45` cases and from `3/9` to `5/9` perfect rows (`0.40` to `0.60` mean pass rate).

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [x] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Raw p-code for the entry address is semantically correct:

  INT_ADD(unique, const(-4), RCX)
  INT_MULT(unique, RDX, const(4))
  INT_ADD(unique, previous, scaled_len)
  COPY(RDX, result)

The raw builder's pre-normalize form preserves the same byte-affine address:

  xVar1 = -4 + param_1;
  xVar2 = param_2 * 4;
  xVar3 = xVar1 + xVar2;
  xVar4 = xVar3;

After `memory::ptr_arith` normalization it becomes:

  xVar4 = (int *)(param_1 + param_2 - 4);

`recover_const_offset_as_typed_pointer_add` interprets the scalar `-4` as an
element count after the base has been inferred as `int *`. The later dynamic
`param_2 * 4` term is then also recovered as a typed-pointer index, so the
normalized address no longer denotes the raw byte expression. The builder and
raw p-code therefore create the correct fact; normalize is the first owner that
creates the wrong unit.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Machine-level scalar address arithmetic remains byte-addressed until an
explicit pointer-element operation or an access-specific stride witness proves
an element-count unit. A typed pointer binding alone is not sufficient to
reinterpret an arbitrary scalar affine expression as C pointer arithmetic.
When a byte-affine expression contains a dynamic term whose stride is already
the pointee size, normalize the complete affine tree in bytes first and only
then expose an element index or PtrOffset with the equivalent byte offset.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is not gated only on an ISA, calling convention,
      function, address, binary, or compiler tuple.
- [x] The rule is about IR units, pointer provenance, and affine address
      structure; ISA-specific scale facts remain in raw p-code/SLEIGH.
- [x] Synthetic coverage will state only the base pointer, dynamic byte stride,
      constant byte displacement, and accessed element type.

Comparable coverage:

- Similar shape 1: `base + index * sizeof(T) - sizeof(T)` for a last-element
  load/store.
- Similar shape 2: `base + index * sizeof(T) + field_byte_offset` where the
  dynamic term and residual constant have different units.
- Synthetic invariant test: a byte-affine load from `base + i*4 - 4` keeps a
  four-byte stride and resolves to the previous `int` element, rather than
  `base + i - 4`.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `apply_ptr_arith_recovery_pass`, specifically `try_recover_ptr_arith_tree`
  and `try_recover_ptr_arith` in `memory/ptr_arith.rs`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed:
  the existing pointer-arithmetic owner already recognizes affine add trees and
  owns `PtrOffset` byte units. The fix should make unit evidence explicit in
  that owner rather than add a later printer or binary-specific correction.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass is planned; a small owner-local helper or an
  extension of the existing affine-tree recovery is sufficient.
- Possible interaction with existing normalize/structuring/materialize passes:
  typed pointer element-offset tests, byte-cast recovery, aggregate field
  recovery, and wide-stride array recovery must retain their current behavior.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact, if any: none expected.
- Known cases that must not change: explicit C-style typed pointer additions,
  `(uint8_t *)p + byte_offset`, `PtrOffset` byte-unit preservation, constant
  indexed loads/stores, and aggregate field offset recovery.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize -E 'test(preserves_scalar_add_const_as_byte_offset) or test(recovers_reversed_scalar_pointer_add_as_byte_offset) or test(recovers_byte_affine_last_element_offset)'`
  - Expected signal: the synthetic byte-affine expression retains the correct
    byte offset/element index and fails on the current implementation.
  - Measured result: `3/3` focused tests passed; the complete normalize crate
    passed `395/395`.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize && cargo nextest run -p fission-pcode`
  - Expected signal: no normalize regressions; known unrelated pcode failures,
    if still present, are reported separately.
  - Measured result: normalize `395/395`; pcode `1069/1072` passed, `1`
    skipped, and the same three pre-existing builder failures remained:
    `diamond_join_lowers_copy_through_join_read_as_select`,
    `movzx_after_byte_add_zero_extends_unsigned`, and
    `x64_byte_add_movzx_does_not_double_add_load`.
- [x] Focused benchmark row:
  - Command: rebuild the local Linux CLI, recreate the local DecBench container,
    and rerun the exact nine-row `reverse_in_place` matrix with caches disabled.
  - Expected row-level improvement: the last-element address and swap dataflow
    use the raw byte-affine expression; semantic cases improve without changing
    unrelated variants.
  - Measured result: local container health reported `git_sha=723aecd96` and
    source fingerprint `7b3da503281d6010236691ff8c7de4bde4849f322fcc2f2996f6e297b61d83f2`.
    The nine rows moved `18/45 -> 27/45` semantic cases; `gcc -O2` and
    `gcc-m32 -O2` each moved `1/5 -> 5/5`, and `gcc -O1` moved `1/5 -> 2/5`.
    Existing `gcc -O3`/`clang -O2` compile failures and `gcc-m32 -O0` runtime
    failure remained outside this address-unit fix.
- [x] Smoke or automation sample:
  - Command: `FISSION_BENCHMARK_NO_CACHE=1 .venv/bin/python runner/runner.py --corpus dev --limit 20 --variant-limit 1 --decompilers fission --run-mode local --no-resume --output results/issue104_smoke_after_723aecd96.json`
  - Expected no-regression signal: no lost requested-function outputs or compile
    regressions outside the motivated affine-address shape.
  - Measured result: `20/20` requested outputs, zero adapter/boundary errors,
    `16/20` perfect rows, and mean semantic pass rate `0.84`.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize -p fission-pcode` and
    `cargo build -p fission-cli --release`
  - Expected signal: clean compilation.
  - Measured result: workspace `cargo check --workspace`,
    `cargo fmt --all --check`, `git diff --check`, and release CLI build all
    passed; emulator nextest passed `200/200` with `3` skipped.
- [x] Boundary audit, if a new pass/helper/dependency was added: not required
  unless the implementation introduces one.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in an AI prompt: none.
- Redaction confirmed: not applicable; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: reference/correctness use only; no output-style
  mimicry request.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: external cache-disabled DecBench
    smoke over 20 dev functions produced `20/20` clean requested outputs and
    no adapter/boundary errors.
  - Synthetic invariant test command/result: focused byte-unit tests `3/3`
    passed; normalize crate `395/395` passed.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
