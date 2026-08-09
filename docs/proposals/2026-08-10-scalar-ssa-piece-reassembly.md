# Scalar SSA Piece Reassembly

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/.cache/decbench-local-eval/x86_O2/math.elf`
- Function: `bubble_sort`
- Address: `0x4007f0`
- Corpus row or benchmark command: external DecBench local evaluation, x86 `O2`, Fission NIR/HIR with caches disabled
- Current output summary: the swapped 64-bit pair store is emitted as `*rax = (ulonglong)xmm0_da`, retaining only one recovered 32-bit lane.
- Semantic cases passed / total: NIR `0/5`; HIR `0/5`
- Failure category: `compile_error`
- Relevant benchmark/static/readability observations: NIR GED `34`, HIR GED `35`, type score `0.4`, byte score `0.0`. The x86 O2 18-function sample is `18/18` completed with zero runner errors, GED0 `2`, Type1 `1`, Byte1 `0`, Union3 `3`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
145..190: four 4-byte lane definitions cover register XMM0
191: COPY unique:d500:8 <- register:1200:8
192: STORE [rax] <- unique:d500:8

Current NIR/HIR:
xmm0_da = iVar121 + iVar124;
*rax = (ulonglong)xmm0_da;

Scalar SSA records the 8-byte input at op 191 as an ordered, disjoint cover of
the two reaching 4-byte definitions. Builder lookup instead selects one
overlapping definition and loses the other lane.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When one scalar register input is wider than every reaching definition and
scalar SSA proves that two or more disjoint operation-defined pieces exactly
cover the input, reconstruct its value from those definitions in physical-byte
order. Decline unless the cover is complete and every piece has an exact
operation output, so input and phi pieces continue through existing paths.
```

ISA-agnostic check:

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific byte order comes from builder options rather than a forked control rule.
- [x] Synthetic test states the def-use shape without a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: two adjacent partial scalar-register writes consumed by one wide copy/store.
- Similar shape 2: four exact lane writes followed by an 8-byte scalar read; only the covered low two lanes participate.
- Synthetic invariant test: adjacent 4-byte writes followed by an 8-byte read reconstruct both halves.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `PreviewBuilder::lower_varnode_inner` and scalar Heritage.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: scalar SSA already owns overlap partitioning and dominance validation; expression lowering only needs a fail-closed consumer helper.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no pass or metric is added; the helper consumes the existing typed contract.
- Possible interaction with existing normalize/structuring/materialize passes: reconstructed shifts/ORs may later simplify, but NIR preserves both pieces before those passes.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
- Telemetry impact, if any: none.
- Known cases that must not change: incomplete covers, entry inputs, phi-defined pieces, pieces produced by wider operation outputs, memory spaces, values wider than 64 bits, and exact single-definition reads.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode scalar_ssa_piece_reassembly`
  - Expected signal: passed `1/1`; both 32-bit constants occur in the reconstructed 64-bit expression.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: passed `991/991`, with one skipped test.
- [x] Focused benchmark row:
  - Command: local Fission decompilation plus external DecBench and semantic verification for `bubble_sort @ 0x4007f0`, caches disabled
  - Expected row-level improvement: the store changed from one 32-bit lane to `(ulonglong)low | (ulonglong)high << 32`. Semantic remained NIR/HIR `0/5` compile error at the next pointer-type mismatch. DecBench remained NIR GED/type/byte `34/0.4/0.0`, HIR `35/0.4/0.0`.
- [x] Smoke or automation sample:
  - Command: external DecBench x86 O2 18-function sample
  - Expected no-regression signal: NIR and HIR each produced `18/18`, zero runner errors, GED0 `2`, Type1 `1`, Byte1 `0`, Union `3`; unchanged from baseline.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode -p fission-decompiler` and `cargo build -p fission-cli --release`
  - Expected signal: both checks and the release CLI build passed.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not required; no new pass or dependency.
  - Expected signal: n/a.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: none.
- Redaction confirmed: not applicable; no external model prompt.
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: external x86 O2 18-function sample after implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
