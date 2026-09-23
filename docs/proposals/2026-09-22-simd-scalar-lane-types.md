# Preserve scalar types for wide SIMD values used by integer p-code

## 1. Baseline Row Anchor

- `libc_types_gcc_O2.exe:count_spaces@0x1400014f0`
- `crypto_gcc_O2.exe:rc4_init@0x140001530`
- `semantic_stress_clang_O2.exe:overlap_move@0x140001800`
- Corpus: external `/Users/sjkim1127/fission-benchmark`, `dev` profile, local
  Fission endpoint, caches disabled with `--no-resume`.

The focused baseline was recorded in:

```text
results/issue72_before_count_spaces.json
results/issue72_before_rc4_init.json
results/issue72_before_overlap_move.json
```

The issue-specific output has a 16-byte byte-array aggregate on an integer
expression path. For `rc4_init` the raw p-code is a wide XMM copy followed by
four-byte lane additions, but the output is:

```c
fission_agg16 tmp_140004010;
uint xmm3_qa;
xmm3_qa = tmp_140004000;
xmm3_qa += tmp_140004010;
```

For `count_spaces`, the same aggregate representation reaches integer shifts
and bitwise operations, including the result of the `psraw` p-code call. For
`overlap_move`, the wide XMM values are retained as aggregate stores; that is
the important negative case and must not be scalarized merely because the
storage width is 16 bytes.

Focused external baseline:

```text
count_spaces: 0/6 semantic cases on gcc -O2, compile_error
rc4_init:     0/5 semantic cases on gcc -O2, compile_error
overlap_move: 0/6 semantic cases on gcc -O2, compile_error
```

The whole generated projects also contain unrelated prelude/global declaration
failures, so the #72 measurement is anchored to the function rows and to the
invalid aggregate/scalar expressions, not to the project-wide error count.

Focused external after-runs used the same `dev` corpus rows, the final local
release bundle (`git_sha=3ea5688bf-dirty`, fingerprint
`8ed1772bbeceffa990b1f546d5647cf743a4b72982f1a74d5eb0debc838adf16`), and
`--no-resume`:

```text
results/issue72_final_count_spaces.json
results/issue72_final_rc4_init.json
results/issue72_final_overlap_move.json
```

Measured outcome:

```text
count_spaces: 0/6 on every semantic compiler row before and after. The O2 C
              output now uses a valid GNU `__int128` spelling rather than the
              invalid `int128`; remaining pointer conversion and undeclared
              SIMD-alias errors still prevent semantic execution.
rc4_init:     headline counts unchanged (gcc O0 4/5, gcc-m32 O0/O2 3/5;
              gcc O2 0/5). The O2 bare-C compiler now accepts the scalar
              `fission_agg16` typedef after diagnostic fixups, whereas before
              it rejected struct-to-integer assignment and struct arithmetic.
              Semantic execution next fails on undeclared `xmm1_wh`, tracked
              separately by #60.
overlap_move: no regression; gcc O0 and gcc-m32 O0 remain 6/6, gcc -Os 2/6,
              and aggregate-only stores still emit no lane projection.
```

The motivating C type/operator defect is removed in generated output, but
these rows do not show a DecBench semantic-score increase because their
remaining compile and semantic failures are independently owned. No broader
quality claim is made from this focused measurement. Final checks: seven
targeted `fission-pcode` tests passed; `cargo check -p fission-pcode
-p fission-decompiler`, formatting, diff checks, and the release CLI build
passed. The full `fission-pcode` suite ran 1,116 tests: 1,113 passed, one
skipped, and three unrelated baseline failures remained
(`diamond_join_lowers_copy_through_join_read_as_select`,
`movzx_after_byte_add_zero_extends_unsigned`, and
`x64_byte_add_movzx_does_not_double_add_load`).

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder expression lowering and opaque-value C representation
- [x] Renderer C spelling for 128-bit integer values
- [ ] Normalize
- [ ] Structuring
- [ ] Printer
- [ ] Benchmark/automation

The raw p-code is already correct. The direct builder repro produces
`fission_agg16 xmm0; fission_agg16 xmm4; xmm8 = xmm0 & xmm4;`: the opcode
establishes a 128-bit integer use, but the opaque operands still lower as a
byte-array struct. The real lane repro follows the same path through a wide
register `Copy`; its low 32-bit alias is lowered back to the full-sized
global name.

`NirType::Aggregate { fields: [] }` is the builder's opaque byte-storage
fallback, not a recovered C struct. The owner boundary is builder expression
lowering: integer p-code uses must obtain an explicitly typed scalar view of
that storage, while ordinary copies/stores retain the full-width opaque value.
The render layer represents an otherwise opaque 16-byte value as an
`unsigned __int128` typedef with byte alignment and GNU `may_alias`, preserving
its 16-byte extent while making scalar operations valid C. Narrow p-code uses
project their byte lane with an endian-aware shift and cast; a printer-only
cast of a struct would still be invalid C and would not select the right lane.
The separate `NirType::Int { bits: 128 }` C spelling must also be a compiler
type; emitting the undeclared token `int128` prevented the row from reaching
the benchmark's semantic cases even after the aggregate/scalar mismatch was
removed.

## 3. Generality / Invariant Proof

```text
A value with no recovered aggregate fields remains opaque for copies,
arguments, and stores. At an integer p-code use, lowering projects the exact
operand width from that storage: a 16-byte operation uses the 128-bit value,
while a narrower alias shifts to the corresponding endian-correct lane and
casts to the p-code operand type. Recovered aggregates with fields and
float-class operations do not take this path. Whole-value copies/stores remain
whole-value assignments; the byte-aligned typedef preserves their size and
alignment.
```

This is based on p-code opcode, width, and def-use evidence. It does not
depend on an ISA, compiler, binary, function name, or address. The negative
coverage must keep a wide value used only by aggregate stores as
`fission_agg16`.

## 4. Risk And Validation Matrix

The change must preserve:

- aggregate layout and field recovery for memory-backed structs and arrays;
- wide copy/store output when no scalar operator proves a bit-vector view;
- existing 32/64-bit signedness and float metatype refinement;
- wide-register lane projection and big-endian offset handling;
- opaque/unsupported p-code call results that are not consumed by integer
  operators.

Validation:

1. Add synthetic builder tests for a 16-byte integer bitwise operation,
   little- and big-endian wide-register lane projection, and an aggregate-only
   store; keep recovered fieldful aggregate declarations as structs. Add a
   printer test for signed/unsigned 128-bit C type spellings.
2. Run the focused builder tests and the complete `fission-pcode` suite.
3. Rebuild the release CLI and rerun the three external rows with
   `--no-resume`.
4. Compare issue-specific invalid aggregate/scalar expressions, semantic
   status, and compile status; report any remaining failures separately from
   the unrelated project-prelude failures.
5. Run the decompiler/decompiler release checks and formatting gates.

No new pass, dependency, or renderer-specific workaround is required.
