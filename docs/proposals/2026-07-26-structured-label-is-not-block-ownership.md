# Decompiler Change Proposal — Structured Labels Are Not Block Ownership

Date: 2026-07-26

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/memory_layouts_gcc_O0.exe`
- Function: `matrix_multiply`
- Address: `0x1400015b8`
- Corpus row or benchmark command:
  `FISSION_ENDPOINT=http://localhost:8007 uv run python runner/runner.py
  --corpus dev --function matrix_multiply --decompilers fission,ghidra
  --run-mode local
  --output results/local_b8442d08_matrix_multiply_fission_ghidra.json`
- Current output summary: the GCC O0 row loses the output-matrix store, the
  middle-loop induction update, and the outer-loop induction update after
  graph-only SESE reconstruction. The third parameter consequently has no
  pointer use and is emitted as `ulonglong`, so the semantic harness cannot
  compile the call. With `FISSION_COLLAPSE_LOOP=0`, the outer-loop update
  returns but the output store and middle-loop update are still absent.
- Semantic cases passed / total: 0/5
- Failure category: `compile_error`
- Relevant benchmark/static/readability observations:
  - Fission HEAD/source fingerprint:
    `b8442d08` /
    `7618cb1c0dad470c29a4a0bd19162fe069e472fc677eb87704d26f71bd9c139a`
  - Ghidra 12.0.4 emits a direct, goto-free function for the same row, but the
    current benchmark wrapper reports `runtime_error` 0/5 for that Ghidra row;
    Fission movement is therefore judged against the original-binary oracle,
    not by assuming the Ghidra harness result is green.
  - Fission `a07a64c1` retained the output store and both loop updates and
    reached 1/5; the regression is between `a07a64c1` and `b8442d08`.
  - Diagnostic reconstruction skips residual block indices 1, 2, 3, 5, and 7
    as "graph-owned"; the missing output store/update path is among the
    skipped residual blocks.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
HEAD default NIR:
    inner accumulation loop
block_14000168a:
    while (...) {
        local_c = 0;
        local_10 = 0;
        goto block_140001656;
    }
block_14000169a:
    while (...) { ... }

Missing from HEAD but present at a07a64c1:
    *xVar84 = xmm0_da;
    local_8 = local_8 + 1;
    local_4 = local_4 + 1;

FISSION_PREVIEW_DIAG=1:
    final reconstruction: skip graph-owned residual block idx=1 ...
    final reconstruction: skip graph-owned residual block idx=2 ...
    final reconstruction: skip graph-owned residual block idx=3 ...
    final reconstruction: skip graph-owned residual block idx=5 ...
    final reconstruction: skip graph-owned residual block idx=7 ...
```

`reconstruct_sese_final_body` currently builds a function-wide set of every
label textually defined anywhere in any structured child body. It then
suppresses a residual CFG block when its label appears in that set, even if the
block index is not in the child's proven `[start, skip_to)` ownership range.
A label can be surfaced for control transfer without the corresponding block's
statements being materialized. The first wrong fact is therefore created by
SESE final reconstruction, before type recovery observes the output store.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A CFG block body is consumed by a structured node only when its block index is
contained in that node's structurally proven ownership set/range. The textual
presence of Label(block_K) inside a structured statement proves only that a C
jump target was surfaced; it does not prove that block K's statements and
terminator were materialized. Label-definition deduplication and CFG-body
ownership are separate decisions.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition uses only structured-region proof and CFG block
      identity, not an ISA or calling-convention enum.
- [x] No ISA-specific data is introduced.
- [x] Synthetic coverage states label-vs-block ownership without a compiler
      tuple, function name, address, or binary identity.

Comparable coverage:

- Similar shape 1: nested loops where an inner region surfaces a label for an
  exit/latch block whose side-effecting statements remain residual.
- Similar shape 2: structured conditional/switch bodies containing a landing
  label for a residual join block with assignments before its terminator.
- Synthetic invariant test: a structured child textually defines a residual
  block label but owns only its own `[start, skip_to)` range; the residual block
  body must still be selected for materialization while its duplicate label can
  be suppressed independently.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
  `fission-midend-structuring::sese_driver::reconstruct_sese_final_body`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the function already has the
  `active_child_map` region proof and its `[start, skip_to)` ownership ranges.
  No new pass is required.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass or metric; a small pure ownership predicate may be
  extracted only to make the negative invariant directly testable.
- Possible interaction with existing normalize/structuring/materialize passes:
  restoring a residual body reintroduces its side effects and type witnesses;
  existing label cleanup must continue to prevent duplicate C label
  definitions without deleting statements.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change:
  - GCC O1/O2 `matrix_multiply` rows currently at 5/5.
  - Existing exclusive-emission nested-loop tests.
  - Duplicate-label cleanup and orphan-goto validity.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command:
    `cargo nextest run -p fission-midend-structuring -E
    'test(structured_label_does_not_own_residual_block)'`
  - Expected signal: label presence alone cannot suppress a non-tombstoned
    residual block.
- [x] Existing structuring regression:
  - Command:
    `cargo nextest run -p fission-pcode -E
    'test(nested_loops_fn) | test(structuring)'`
  - Result: the new cleanup test preserves unique residual statements while
    deduplicating the label. The complete pcode comparison recovered six
    existing preview tests.
- [x] Crate-level gate:
  - Command:
    `cargo nextest run -p fission-pcode && cargo check -p fission-pcode`
  - Result: `cargo check -p fission-pcode` passed. Current `b8442d08` baseline
    was 906/930 passed with 24 failures; the change is 912/930 passed with the
    remaining 18 failures identical after the later type refinement.
- [x] Focused benchmark row:
  - Command: repeat the baseline runner command against a freshly baked local
    Fission bundle with no stale service cache.
  - Result artifact:
    `results/local_b8442d08_label_ownership_after_matrix_multiply_fission_ghidra.json`.
    GCC O0 moved from `compile_error` 0/5 to compiled `assertion_fail` 1/5;
    the output store and both induction updates returned.
- [x] Smoke or automation sample:
  - Command:
    focused `matrix_multiply` all-variant Fission/Ghidra run followed by the
    external parity smoke.
  - Result: all-variant reruns kept GCC O1/O2 and Clang O0 at 5/5. An isolated
    three-subject parity smoke produced 15/15 matches across assembly, p-code,
    CFG, function discovery, and IR invariants.
- [x] Optional related checks:
  - Command:
    `cargo check -p fission-decompiler`
  - Result: clean.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal: baseline unchanged.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model implementation review was used.
- Information used locally:
  - [x] Structural failure pattern only
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction confirmed for any future external prompt:
  - [x] Function names removed
  - [x] Addresses removed
  - [x] Binary paths removed
  - [x] Corpus row ids removed
  - [x] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Synthetic label-vs-ownership test plus all-variant focused regression run.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
