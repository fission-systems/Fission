# Wide Register Subpiece Projection

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `bubble_sort`
- Address: `0x4007f0`
- Corpus row or benchmark command: external cache-disabled x86 O2 DecBench plus focused `fission_cli decomp`/`raw-pcode` at the address above.
- Current output summary: the scalar `movd` lane reads after `movq`/`pshufd` become aggregate casts and an undefined `xmm1_qa`, producing `if ((uint)xmm1_qa < (fission_agg16)xVar23)` instead of a comparison between two 32-bit lanes.
- Semantic cases passed / total: NIR 0/5; HIR 0/5.
- Failure category: `compile_error`.
- Relevant benchmark/static/readability observations: NIR/HIR GED 34/35, type match 0.4, byte match 0.0. The preceding byte-offset repair restored pointer advances but did not change these row metrics.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
Raw p-code correctly represents the lane flow:
  LOAD      tmp:8       <- [rax]
  INT_ZEXT  xmm0:16     <- tmp:8
  ... pshufd writes four 4-byte lanes of xmm1 ...
  SUBPIECE  edx:4       <- xmm0:16, 0
  SUBPIECE  ecx:4       <- xmm1:16, 0
  INT_LESS  cf          <- ecx:4, edx:4

lower_subpiece_op currently lowers the entire 16-byte input before applying
the scalar cast. A full-width register without one full-width defining op
(xmm1 is assembled by four lane writes) falls back to the aggregate hardware
binding. A full-width zext definition (xmm0) lowers to an aggregate cast before
the subpiece. The builder has therefore already lost the scalar lane before
normalize and rendering.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
SUBPIECE of a register selects a scalar storage interval. Resolve that interval
against the most recent overlapping definition before lowering the full wide
register. An exact lane definition supplies the value directly. A covering
COPY/CAST/ZEXT/SEXT maps the requested interval back into its input when the
input covers that interval. Only fall back to whole-value shift/cast lowering
when no storage-level projection is provable.
```

ISA-agnostic check:

- [x] Production condition is based on p-code SUBPIECE and overlapping storage, not an x86 opcode or ISA enum.
- [x] Endianness comes from the existing lowering options; ISA-specific decode stays in SLEIGH.
- [x] Synthetic tests use generic wide register and scalar-lane p-code shapes.

Comparable coverage:

- Similar shape 1: a wide register assembled by independent scalar lane writes and then read with `SUBPIECE`.
- Similar shape 2: a wide register populated by widening copy/zext from a narrower scalar carrier and then projected.
- Synthetic invariant tests: exact partial-lane projection and covering-passthrough projection.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: builder `lower_subpiece_op`, `lookup_def_site`, and `project_alias_def_expr`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: a small owner-local helper can reuse the current block and lowering-site facts. No new pipeline pass or cross-owner analysis is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: the helper is local expression lowering for an existing opcode, not a new analysis or pass.
- Possible interaction with existing normalize/structuring/materialize passes: produces scalar expressions earlier; downstream constant folding and dead-temp cleanup may simplify them. Existing CDQ signed-subpiece behavior must remain unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: none.
- Known cases that must not change: unique-space subpieces, signed CDQ high-half extraction, non-covering or cross-block ambiguous definitions, and big-endian lane locations.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-pcode -E 'test(wide_register_subpiece)'`
  - Expected signal: exact lane and passthrough-covered lane lower to scalar values without aggregate casts.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: all tests pass, including CDQ/subpiece coverage.
- [x] Focused benchmark row:
  - Command: release rebuild; focused `bubble_sort @ 0x4007f0` semantic and DecBench rerun with caches disabled.
  - Expected row-level improvement: scalar lane comparison and swap values replace aggregate casts/undefined XMM binding; report semantic and metric movement exactly.
  - Measured result: the undefined `xmm1_qa` and all `fission_agg16` casts/typedefs disappeared. The comparison now consumes the low and high 32-bit lanes of the 64-bit pair load (`xVar23` and `xVar23 >> 32`). GED remained 34/35, type match 0.4, byte match 0.0, and semantic remained 0/5 `compile_error`; pair reassembly/store and void-return recovery are separate remaining owners.
- [x] Smoke or automation sample:
  - Command: external cache-disabled x86 O2 18-function NIR/HIR sample.
  - Expected no-regression signal: 18/18 output coverage, zero errors, and no perfect-count regression.
  - Measured result: NIR and HIR each produced 18/18 x86 O2 functions with zero errors and retained GED0 2, Type1 1, Byte1 0, Union 3.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode -p fission-decompiler && cargo build -p fission-cli --release`
  - Expected signal: clean compilation.
- [ ] Boundary audit, if a new pass/helper/dependency was added: no pass or dependency added; not required.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model implementation review was requested.
- Information exposed in an AI prompt: N/A.
- Redaction confirmed: N/A; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: reference measurement only.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: cache-disabled external x86 O2 18-function sample retained full coverage and identical perfect counts.
  - Synthetic invariant test command/result: two focused tests passed; full fission-pcode gate passed 990/990 with one pre-existing skipped test.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
