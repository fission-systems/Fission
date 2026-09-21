# Bubble-sort pointer surface aliases

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/math_gcc_O2.exe`
- Function: `bubble_sort`
- Address: `0x1400015d0`
- Corpus row or benchmark command:
  `fission-benchmark/runner/runner.py --corpus dev --function bubble_sort --decompilers fission --run-mode local --no-resume`
- Current output summary: the packed 8-byte SSE load and two 32-bit lane comparisons are present, but the pointer cursors are declared as `unsigned long long *` internally. The output assigns `rax = arr`, which is not valid C for the emitted declaration, and models the cursor as a 64-bit element pointer instead of preserving the source `int *` cursor surface.
- Semantic cases passed / total: 16 / 45 across the nine `bubble_sort` rows in the cache-disabled run; the motivating `gcc -O2` row passed 0 / 5 and failed at runtime after generated C compilation.
- Failure category: runtime/semantic failure; the focused aggregate had 5 runtime-error rows, 2 assertion-failure rows, and 2 passing rows.
- Relevant benchmark/static/readability observations: raw p-code is correct and contains the 4-byte cursor increment, 8-byte packed load, lane extraction, swap store, and end-pointer comparison. The first wrong surface fact is the local declaration/type overlay, not the lift or lane semantics.

### Measured follow-up

The same cache-disabled nine-row DecBench matrix was rerun after each owner
change. The alias/byte-stride work raised the aggregate from 16/45 to 24/45;
the signed lane and dual-layer fixes then raised it to 27/45 (0.6000), with
perfect rows changing from 2/9 to 5/9. The motivating `gcc -O2` row changed
from 0/5 runtime-error to 5/5, and `gcc -O3` changed from 0/5 runtime-error
to 5/5. The final row taxonomy is 5 `ok`, 1 `assertion_fail`, 1
`compile_error`, and 2 `runtime_error`.

The remaining failures are compiler/ABI-specific recovery gaps outside the
motivating x86-64 packed-SSE row: both x86-32 rows still need separate pointer
address recovery, and the Clang rows expose unrelated scalar/pointer recovery
problems. They are not used to claim that every matrix variant is solved.

The final cache-disabled run after `23e2095ee` is recorded in
`results/issue102_after_23e2095ee.json`. It remains 27/45 (0.6000) with 5/9
perfect rows; the motivating `gcc -O2` row remains 5/5 and `gcc -O3` remains
5/5. The result is unchanged by the final normalize-order fix, which closes a
synthetic wide-piece regression without changing the measured bubble-sort row.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [x] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [x] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
The motivating raw p-code already has:

  rax = rcx
  load 8 bytes from [rax]
  extract two 32-bit lanes for the comparison
  rax = rax + 4
  compare rax with the computed end pointer

Function-name type hints give the parameter a trusted `int *` surface type,
but the single-definition local aliases `rax` and `r10` receive no surface
type. The printer therefore declares them from their internal wide packed-load
type (`unsigned long long *`), making `rax = arr` invalid and exposing the
address cursor as a 64-bit array. No raw p-code or lane reconstruction change
is required to establish the failure.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a binding with a trusted pointer surface type flows through a unique local
definition whose right-hand side is provably address-preserving (direct copy,
pointer cast, pointer offset, or pointer arithmetic with one pointer operand
and one non-pointer operand), propagate the source surface type to that local.
Keep the internal NirType unchanged so a packed load can still retain its
observed width. Refuse ambiguous multi-pointer arithmetic, memory loads, and
multi-assigned/reused bindings.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data lives in the existing type/prototype and p-code layers;
      the alias rule is a generic def-use/type constraint.
- [x] Synthetic test states the pointer dataflow shape without requiring a
      compiler tuple or function name.

Comparable coverage:

- Similar shape 1: a local register cursor copied directly from a pointer parameter.
- Similar shape 2: an end cursor derived from a pointer parameter with a constant
  or integer offset.
- Synthetic invariant test: unique direct-copy and pointer-offset aliases inherit
  the pointer surface type; an unrelated pointer loaded from memory does not.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `apply_preview_type_hints`
  in `crates/fission-pcode/src/midend/builder/type_hints.rs`, which is already the
  canonical overlay for trusted parameter/local surface declarations.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] CFG / dominance / postdominance fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed:
  extend the existing type-hint owner with a conservative, fixed-point alias
  propagation helper. A new pipeline pass would duplicate the same surface-type
  ownership and would run after the declarations have already been selected.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass is needed; the helper consumes the already-built HIR
  def-use shape and only writes missing surface metadata.
- Possible interaction with existing normalize/structuring/materialize passes:
  the internal type and expression tree remain unchanged, so semantic lowering
  and structuring are unaffected. Only declarations and pointer-assignment
  surface compatibility can change.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: count propagated local surface hints in the existing
  `PreviewHintStats.local_surface_hits` field.
- Known cases that must not change: explicit parameter/local surface hints, packed
  load widths, non-pointer values, ambiguous pointer arithmetic, and bindings with
  more than one assignment.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: focused `fission-pcode` nextest filter for pointer surface alias propagation.
  - Expected signal: direct-copy and pointer-offset aliases inherit the source
    pointer surface type; unsafe aliases remain unchanged.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the known unrelated baseline failures.
- [x] Focused benchmark row:
  - Command: cache-disabled DecBench rerun for `bubble_sort` across the same nine
    dev rows.
  - Result: `results/issue102_after_23e2095ee.json`; the motivating `gcc -O2`
    row is 5/5 and the emitted HIR keeps an `int *` cursor with byte-stride
    arithmetic while retaining the packed 64-bit load/store. The aggregate is
    27/45 with 5/9 perfect rows, unchanged from the immediately preceding
    measured build.
- [ ] Smoke or automation sample:
  - Command: bounded dev smoke through the local benchmark runner.
  - Expected no-regression signal: no new failure in the smoke sample.
- [x] Optional related checks:
  - Command: `cargo check --workspace`, `cargo fmt --all --check`, `git diff --check`,
    `cargo nextest run -p fission-emulator`, and release CLI build.
  - Result: formatting, diff check, workspace check, emulator nextest (200
    passed / 3 skipped), and `cargo build -p fission-cli --release` pass; the
    full pcode nextest is 1077 passed / 3 failed / 1 skipped, with only the
    three pre-existing #103 failures.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: no boundary violations.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt:
  - [ ] Structural failure pattern only
  - [ ] Owner evidence only
  - [ ] Invariant candidates only
  - [ ] Validation matrix only
- Redaction confirmed:
  - [x] Function names removed from any implementation prompt (none sent)
  - [x] Addresses removed from any implementation prompt (none sent)
  - [x] Binary paths removed from any implementation prompt (none sent)
  - [x] Corpus row ids removed from any implementation prompt (none sent)
  - [x] Compiler tuple / row-identifying labels removed from any implementation prompt (none sent)
- Ghidra guidance confirmed:
  - [x] No Ghidra output-style request was made.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run; the focused DecBench matrix
    is the measured motivating-row gate for this change.
  - Synthetic invariant test command/result: targeted `fission-pcode` nextest
    filters passed, including the signed lane, HIR preservation, and dual-layer
    stitch regressions.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed

## 8. Final Owner Evidence

The raw p-code still contains the explicit 32-bit `IntZExt` operations for
each packed lane. The first semantic loss was in generic normalize cast
canonicalization: `u64(u32(signed_lane))` was collapsed to
`u64(signed_lane)`. The follow-up fix keeps a signedness-changing intermediate
cast through both normalize cast-elimination paths. A second, independent
surface bug was in dual-layer orchestration: `code_nir` was populated from the
scored tree's HIR string, so HIR cast sugar could reintroduce the same loss
after the NIR AST was correct. The stitch now keeps the scored NIR string, and
HIR cast cleanup only removes a widening cast when the underlying source is
known unsigned. The final normalize fix restores conservative behavior for an
unknown source signedness and recognizes wide integer piece recombination
top-down, before child cast cleanup can erase the high/low piece boundaries.

The fixes are generic signedness and layer-contract rules. They contain no
function, address, binary, architecture, or compiler-tuple guards.

Regression coverage includes:

- normalize idiom and wide-cast tests for preserving signedness-changing
  intermediate casts;
- scalar SSA packed-piece reassembly coverage;
- dual-layer NIR stitching coverage;
- HIR presentation coverage for signed-lane zero-extension.
