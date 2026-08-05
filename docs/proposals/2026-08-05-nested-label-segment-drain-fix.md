# Decompiler Change Proposal: Fix Dangling `goto` from Nested-Label Segment Drain

Date: 2026-08-05

## 1. Context

User asked to start closing the measured Fission-vs-Ghidra gap. The most
recent official benchmark run (dev corpus, 216 functions,
`fission-benchmark/results/latest-summary.json`, 2026-08-03) shows:

- `avg_correctness`: Fission `0.6263` vs Ghidra `0.7256`
- `semantic_pass_pct`: Fission `52.86%` vs Ghidra `72.56%`

Breaking Fission's 105/216 non-`ok` rows down by `fail_taxonomy`:

| taxonomy | count | character |
|---|---|---|
| `assertion_fail` | 45 | semantically wrong, long tail across ~20 functions, no dominant pattern |
| `compile_error` | 27 | decompiled C does not even compile -- mechanical, deterministic |
| `timeout` | 16 | concentrated in a handful of functions, mostly `-O2` (matches existing backlog item on SESE structuring's algorithmic bottleneck) |
| other | 17 | misc |

`compile_error` was chosen as the starting point: it's binary (compiles or
it doesn't), several of the 27 rows are the *same* source function recurring
across multiple compiler variants (`dot_product_stride` at `gcc -O0`,
`gcc -O2`, `gcc-m32 -O0`; `manipulate_bitfields` at `gcc -O0`,
`gcc-m32 -O0`), so one root cause can plausibly resolve several rows at
once, and each is deterministically reproducible outside the benchmark
harness via `fission_cli decomp`.

## 2. Root cause: `single_pred_label_inline_flat`'s dead-zone check is not recursive

`crates/fission-midend-normalize/src/cleanup/control_flow.rs`'s
`single_pred_label_inline_flat` collapses the pattern
`Goto(L); <dead-zone>; Label(L);` down to nothing, when `L` is referenced by
exactly one `Goto` in the whole function (this one) and the dead zone is
genuinely unreachable any other way. That's a legitimate transform *when*
nothing inside `<dead-zone>` is itself a jump target still referenced from
outside the zone.

Its existing safety check (`external_ref_found`) scanned `segment.iter()`
for direct `PreHirStmt::Label` elements only:

```rust
let external_ref_found = segment.iter().any(|s| {
    if let PreHirStmt::Label(l) = s { ... } else { false }
});
```

`segment` is the flat, top-level slice of statements between the `Goto` and
`Label(L)`. If that segment contains a compound statement (`While`/`For`/
`If`/`DoWhile`/`Switch`) whose *body* defines a label, that label is
invisible to this shallow scan -- it's nested one level down, not a direct
element of `segment`. If a `Goto` to that nested label exists *outside* the
segment (a real, still-live jump into what looks like "dead" code), the
whole segment -- including the label it targets -- gets deleted anyway,
because the safety check never saw it. The surviving `Goto` is left
dangling: a jump to a label that no longer exists anywhere in the function,
which is not valid C.

### Concrete repro: `dot_product_stride` (`gcc -O0`, x64)

Traced via a temporary env-gated dump of the pre-pass statement shape
(`FISSION_SPLI_TRACE`, added and removed for this investigation). The
top-level body was:

```
[0] Block (prologue)
[1] Assign
[2] Goto(block_1400016d6)
[3] Label(block_1400016c7)
[4] For
[5] Assign
[6] While { body: [ If, Label(block_140001671), Assign, Goto(block_1400016c7) ] }
[7] Return
[8] Label(block_1400016d6)
[9] If { then: [ Goto(block_140001671) ] }
[10] Return
```

`Goto(block_1400016d6)` at `[2]` is referenced exactly once, matching
`Label(block_1400016d6)` at `[8]`. The segment `[3..8)` was drained --
including the `While` at `[6]`, whose body defines
`Label(block_140001671)`. That label is *still* referenced by the
surviving `If` at `[9]`'s `Goto(block_140001671)`. Result: a `goto
block_140001671;` with no matching label anywhere in the function --
exactly the `bare.c: error: label ... used but not defined`-class failure
the benchmark harness recorded.

## 3. Fix

Added `collect_defined_labels` (`cleanup/utils.rs`), a recursive collector
mirroring the existing recursive `collect_stmt_referenced_label_counts` --
it walks into every compound body (`Block`/`While`/`DoWhile`/`For`/`If`/
`Switch`) and collects every `Label` name defined anywhere within, not just
ones that are direct elements of the slice passed in.

`external_ref_found` now checks every label `collect_defined_labels(segment)`
finds (recursively), not just `segment`'s direct top-level elements, against
the same total-vs-internal reference-count comparison (and the existing
`PROTECTED_LSDA_LABELS` carve-out) as before. A segment is only drained when
*none* of the labels it defines anywhere -- at any nesting depth -- has a
reference from outside itself.

## 4. Verification

- New unit test
  `single_pred_label_inline_keeps_dead_zone_with_externally_referenced_nested_label`
  (`cleanup/passes_tests.rs`) reproduces the minimal shape (`Goto`, a
  `While` whose body defines a label, `Label`, a `Goto` to the nested
  label after it) and asserts every referenced label still has a matching
  definition after the pass runs.
- `cargo nextest run -p fission-midend-normalize`: 289/289 passed.
- `cargo nextest run --workspace --no-fail-fast`: 2299/2306 passed, same 7
  pre-existing unrelated `fission-emulator` baseline failures, no new
  regressions.
- Real repro, before/after, `dot_product_stride @ 0x140001639` in
  `advanced_patterns_gcc_O0.exe`: before, ends in a dangling
  `goto block_140001671;` with the label missing entirely; after, every
  `goto` has a matching label. Extracted the after-fix function body into a
  standalone `.c` file with typedefs for the surface types
  (`uint`/`ulonglong`/`longlong`) and confirmed with `gcc -c -w -O0`: **compiles
  clean**. Wrote a driver calling it with the same arguments `main()` uses
  in the corpus source (`dot_product_stride(row_a={1,2,3,4},
  row_b={5,6,7,8}, n=2, stride=2)`) and confirmed the runtime result is
  `70`, matching the hand-computed expected value from the original C
  source (`i,j` nested loop, `idx = i*stride+j`, `acc += a[idx]*b[idx]`) --
  the fix is both a compile fix and semantically correct.
- Also reproduced and confirmed fixed for the `gcc-m32 -O0` variant of the
  same function (same dangling-`goto` shape, same fix resolves it).
- Full corpus sweep: decompiled every discovered function (5,057 total)
  across every `.exe` in `fission-benchmark/corpus/dev/binaries/c`, scanning
  each function's output text for any `goto X;` with no matching `X:`
  label anywhere in the same output. 88 dangling-goto occurrences remain,
  but collapse to exactly 3 distinct function names when deduplicated:
  `__tmainCRTStartup`, `___tmainCRTStartup`, `__pei386_runtime_relocator`
  -- all MinGW CRT startup/relocator internals, never part of the
  benchmark's source-compared, scored function set. None of the 27
  `compile_error` rows' source functions (`dot_product_stride`,
  `manipulate_bitfields`, `rc4_init`, `matrix_multiply`, etc.) appear in
  the post-fix dangling list.

## 5. Known-separate, NOT fixed by this change

While investigating the 27 `compile_error` rows, found this is a bucket of
at least 3-4 *distinct* root causes, not one. Confirmed these are
unrelated to the dangling-goto bug (their generated code has no
label/goto mismatch at all) and left for future work:

- `dot_product_stride @ gcc -O2`: fully structured (no goto/label), but a
  stride parameter is mistyped as `int *` and used in pointer arithmetic
  where an integer stride was intended -- a type-inference bug, separate
  investigation needed.
- `manipulate_bitfields @ gcc -O0` / `gcc-m32 -O0`: raw x86 condition-code
  state (`of`/`sf`/`zf`/`pf`) leaking directly into the decompiled output
  instead of being resolved into proper boolean expressions -- a much
  larger "flags modeling" gap, out of scope for a single-pass fix.
- `rc4_init @ gcc -O2`: SSE/AVX-vectorized loop (`xmm0`/`xmm1`/...
  registers, `fission_agg16` synthetic aggregate types) -- Fission does not
  yet recognize compiler-vectorized idioms; a materially bigger feature
  gap, not a bug in an existing pass.

These, along with the `assertion_fail` (45, long-tail semantic correctness)
and `timeout` (16, concentrated -- ties to the existing open item on SESE
structuring's algorithmic bottleneck) buckets, are documented here as the
next candidates rather than pursued further this round.
