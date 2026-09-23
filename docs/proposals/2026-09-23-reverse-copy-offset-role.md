# Preserve scalar roles for reused reverse-copy offsets

## 1. Baseline Row Anchor

- Binary: `semantic_stress_clang_O2.exe` from the external dev corpus.
- Source: `corpus/dev/source/c/semantic_stress.c`, `overlap_move` (line 83).
- Function/address: `overlap_move@0x140001800`.
- Focused local command: `fission_cli decomp ... --addr 0x140001800 --layer hir --no-header --no-warnings --no-db`.
- Before runner command:

  ```bash
  FISSION_ENDPOINT=http://localhost:8007 FISSION_SOURCE=local \
  FISSION_SOURCE_FINGERPRINT=e6040a25ca0e635043740246a5e81a880ca0c6d2904ff71602caed67575f1184 \
  python runner/runner.py --corpus dev --function overlap_move \
    --decompilers fission --output /tmp/fission_issue71_before_cf08fcb.json \
    --no-resume --run-mode local
  ```

- The baseline had 9 rows: semantic mean `0.2917` over 8 usable rows, 2/8
  perfect, and compilation succeeded on 3/9. The `clang -O2` row was rejected
  by the adapter because its output was truncated at 8,000 characters; GCC
  `-O2` had a separate compile failure (`i[-1]` with `i` typed as `size_t`).
- The baseline HIR declared `xVar128` as `uint *` and emitted a negated
  integer-derived expression into it. Disassembly at `0x140001bb0` uses the
  corresponding register as an address displacement in `(%r10,%r11)` and
  `(%r9,%r11)`, decrements it by four, and compares it against a negative
  count.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [x] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Pointer-role recovery creates the incorrect binding type before rendering.
Two generic promotions can override this local's scalar role: transitive
address provenance follows every operand through derived address temporaries,
and equality with a pointer-typed peer is treated as pointer evidence. The real
HIR defines the value with integer `Neg`, copies it through an alias used in an
address `Add`, and decrements it in the reverse loop. The loop exit also
compares it with a pointer-cast machine value. Those weak pointer signals
overrode its scalar-induction role. The invalid pointer declaration and
pointer subtraction were downstream symptoms.

The owning module is `types/type_infer/pointer_roles.rs`, which owns the scalar
role override, transitive address override, and pointer-comparison-peer
promotion. Before the fix, type fixed-point diagnostics repeatedly reported
type changes in this function, consistent with scalar correction and pointer
promotion undoing one another.

```text
Before: xVar128 : uint *
Definition: xVar128 = -(unsigned long long)(address - 1)
Alias: xVar146 = xVar128
Address temp: xVar147 = xVar124 + xVar146
Loop update: xVar128 -= 4
Machine use: movl (%r10,%r11), %edi
             movl %edi, (%r9,%r11)
             addq $-4, %r11
             cmpq %r11, %rsi
Semantic role: integer displacement / induction value
After: xVar128 : long long
```

## 3. Generality / Invariant Proof

```text
An integer-negated binding with a negative constant self-update is scalar
induction evidence, even when simple copies feed it into an effective-address
expression or a pointer comparison. Propagate that evidence through simple
aliases. Transitive address provenance and pointer-comparison peers must not
promote it unless it is directly used as a memory-address root; actual memory
roots and pointer-valued locals remain pointers.
```

- No ISA, function, address, binary, compiler, or corpus guard is used.
- Existing coverage includes pointer-add-offset inference and preservation of
  direct memory-address bases.
- `copied_negated_induction_offset_is_not_promoted_by_pointer_context` models
  the negated definition, copy alias, derived address temporary, pointer-peer
  comparison, and `PtrOffset(-4)` loop update. It asserts the induction value
  remains integer and the actual address temporary remains a pointer. With the
  new scalar-role evidence disabled, it fails with `cursor: Ptr(...)`, proving
  it detects the old promotion.

## 4. Risk And Ownership Check

- Existing owner: `types/type_infer/pointer_roles.rs`; no new pass.
- Shared analysis: existing definition dependencies and pointer-role facts.
- The owner now recognizes integer-negation plus negative constant self-update
  (and simple copies), then prevents the transitive-address and pointer-peer
  heuristics from overriding that scalar induction role. A negative
  self-`PtrOffset` update alone no longer counts as a direct address use; direct
  loads/stores still protect real address roots.
- Risk: a machine binding can represent pointer and scalar values on separate
  paths. The classifier is restricted to integer-negated induction values,
  preserves direct memory-address roots, and leaves ordinary pointer
  arithmetic and pointer parameters unchanged.
- Owner-to-owner dependency: none. Telemetry: none.

## 5. Validation Matrix

- [x] Targeted regression:
  - `cargo nextest run -p fission-midend-normalize copied_negated_induction_offset_is_not_promoted_by_pointer_context`
  - Passed after the fix; the disabled-evidence control failed with a pointer
    type.
- [x] Normalize suite: `cargo nextest run -p fission-midend-normalize` — 412
  passed.
- [x] Pcode suite: `cargo nextest run -p fission-pcode --no-fail-fast` — 1,114
  passed, 1 skipped, 3 known unrelated builder failures:
  `diamond_join_lowers_copy_through_join_read_as_select`,
  `movzx_after_byte_add_zero_extends_unsigned`, and
  `x64_byte_add_movzx_does_not_double_add_load`.
- [x] External before/after DecBench: same 9-row focused run, with no resume.
  The initial after service was rebuilt from local Linux release CLI
  fingerprint `6a420f5d1abf4eac425125891c4340f84a96dcaeb7da5efe018765e74e35eb62`.
  - Exact `clang -O2` decomp output changed `xVar128` from `uint *` to
    `long long`; the reverse-loop update is scalar and the memory operands
    remain pointers.
  - Aggregate metrics and compile categories did not move. The motivating
    `clang -O2` row is an adapter error in both runs because output exceeds the
    8,000-character adapter limit, so it is not semantically scored. No
    DecBench score improvement is claimed.
- [x] Performance sanity: reusing scalar-induction evidence reduced the same
  9-row local runner elapsed time from 107.1s to 95.9s (single runs, so this is
  directional only, not a controlled benchmark). Type/GED/similarity scores
  and row categories stayed unchanged. Alias-role propagation now uses a
  source-to-target worklist rather than repeatedly rescanning all copies.
- [x] Current-source macOS release CLI built successfully. Its focused HIR
  still declares `xVar128` as `long long` and emits `xVar128 -= 4`; the address
  expressions remain pointer-typed. The issue's original unary-minus-on-pointer
  diagnostic is absent in the current whole-function Clang syntax check, but
  that whole function still has 20 unrelated type errors, so full-function
  compilability is not claimed.
- [x] Local external service rebuilt and health-checked with matching source
  fingerprints. Linux release CLI build completed through the script's Docker
  CD-target fallback.
- [x] `cargo check -p fission-pcode -p fission-decompiler`,
  `cargo fmt --all --check`, and `git diff --check` passed.

## 6. AI Review / Prompt Firewall

- Another AI model was not asked for implementation advice.
- The production condition contains no corpus or row identity.

## 7. Review Notes

- [x] No binary/function/address/corpus guards.
- [x] No semantic score gain is claimed; benchmark limitation is documented.
- [x] Extended the existing pointer-role owner; added no parallel pass.
