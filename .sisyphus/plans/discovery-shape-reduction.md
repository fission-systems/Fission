# Discovery/Shape Rejection Reduction Plan

## Objective

Reduce `discovery_rejected_noncanonical_layout_count` and the coupled `promotion_rejected_by_shape_count` in Fission's guarded-tail discovery pipeline using **algorithmic CFG / printed-order invariants** informed by:

- `vendor/ghidra/ghidra-Ghidra_11.4.2_build`
- `vendor/retdec-5.0`

This plan targets **one immediate workstream**: accepting structurally equivalent guarded-tail layouts that are currently rejected due to conservative discovery-time canonicalization limits.

## Scope

### In scope
- Guarded-tail discovery acceptance in `crates/fission-pcode/src/nir/structuring/guards.rs`
- Discovery-time canonicalization rules for lexically awkward but structurally equivalent layouts
- Targeted telemetry to reveal the next dominant unsplit discovery blocker if needed
- Regression tests in `crates/fission-pcode/src/nir/tests/structuring_misc.rs`
- Validation using both `nir-check --functions-limit 200` and `500`

### Out of scope
- Readability polish / printer-only improvements
- Type propagation / FID post-processing
- Broad UI / product-layer changes
- Heuristic binary-specific patches

## Current Evidence

Latest large-sample picture indicates:

- `discovery_rejected_noncanonical_layout_count` remains dominant
- `promotion_rejected_by_shape_count` mirrors discovery shape pressure
- direct shape subtype counters are effectively zero, implying remaining shape failures are mostly **canonicalization/discovery-side**
- alias-nonlocal and alias-not-fallthrough have already been meaningfully decomposed, so the next ROI is discovery acceptance rather than more blind alias work

## Target Hypothesis

The next reduction should come from **accepting discovery-time layouts that already satisfy common-follow / next-flow equivalence but fail because the canonicalized middle still looks lexically awkward**.

The likely next wins are in these categories:

1. pure/structurally neutral middle payloads that still block candidate formation
2. guarded-tail layouts with common-follow equivalence but awkward lexical label placement
3. discovery candidates rejected before promotion despite already satisfying the same printed front-path invariants later accepted downstream

## Reference-Guided Rules To Reuse

### From Ghidra
- Use **common follow / next flow** facts instead of lexical prettiness when deciding if a guarded shape is structurally valid.
- Prefer **front-leaf / printed-path equivalence** over raw label position.
- Allow `if-no-exit`-style terminal guarded shapes when structurally single-entry.
- Do not suppress explicit control transfer when printed order truly diverges.

### From RetDec
- Prefer structurally stable normalization before forcing higher-level reduction.
- Keep fallthrough-based simplifications tied to explicit CFG / successor equivalence, not pattern-only matching.

## Implementation Work Packages

### WP1 — Instrument the dominant remaining discovery blocker

**Goal**: avoid another opaque large bucket.

#### Actions
- inspect remaining `mark_noncanonical_layout_rejection()` call paths in `guards.rs`
- add one layer of subtype telemetry only if the blocker is still aggregated at that exact path
- keep subtype additions minimal and tied to a concrete reduction candidate

#### Expected outcome
- a measurable split of the still-dominant discovery rejection path into one or two actionable sub-buckets

#### QA
- inspect the added counters in code review and confirm they are wired through:
  - `crates/fission-pcode/src/nir/types.rs`
  - `crates/fission-pcode/src/nir/mod.rs`
  - `crates/fission-pcode/src/nir/builder/mod.rs`
  - `crates/fission-automation/src/report.rs`
- run `cargo check -p fission-pcode`
- if telemetry/report changes were added, run `cargo check -p fission-automation`
- expected result: new subtype fields compile cleanly and appear in exported build-stat reporting keys

---

### WP2 — Relax discovery acceptance for CFG-equivalent guarded middles

**Goal**: accept lexically awkward guarded middles when printed order / common follow is already equivalent.

#### Candidate patterns to target
- alias/middle regions with only pure value computation and no escaping control transfer
- front-path-equivalent label layouts where next emitted flow already reaches the same follow
- terminal discovery shapes whose follow equivalence is already proven by structure, not lexical adjacency

#### Files
- `crates/fission-pcode/src/nir/structuring/guards.rs`

#### Constraints
- no heuristic binary-specific rules
- no relaxing nested control-transfer cases unless successor/printed-path equivalence is explicit
- preserve conservative behavior for:
  - nested-after-label control transfers
  - nonlocal ownership ambiguity
  - loop/switch escape contamination

#### Expected outcome
- lower `discovery_rejected_noncanonical_layout_count`
- small but real increase in `promotion_candidate_count` and `promoted_region_count`

#### QA
- run only the targeted regression(s) added for the new acceptance path, for example:
  - `cargo test -p fission-pcode structuring_candidate_discovery_ -- --nocapture`
- confirm the newly accepted shape no longer increments the targeted discovery rejection counter
- confirm the paired unsafe variant still rejects
- expected result: exactly the intended discovery shape is accepted, while nested/nonlocal/escape variants remain conservative

---

### WP3 — Add regression coverage for accepted vs rejected discovery shapes

**Goal**: prevent accidental over-acceptance while opening the intended cases.

#### Files
- `crates/fission-pcode/src/nir/tests/structuring_misc.rs`

#### Add tests for
- newly accepted common-follow-equivalent noncanonical discovery shape
- newly accepted printed-front-path-equivalent guarded middle
- preserved rejection for nested/escaping/nonlocal versions of the same shape

#### Expected outcome
- direct proof that the new discovery acceptance is structural, not permissive noise

#### QA
- run the specific regression names added in `crates/fission-pcode/src/nir/tests/structuring_misc.rs`
- then run the full crate suite:
  - `cargo test -p fission-pcode`
- expected result: all new tests pass and no unrelated structuring regressions fail

---

### WP4 — Validate on expanded samples and re-rank the next bottleneck

**Goal**: ensure improvement survives beyond toy samples.

#### Commands
- prerequisite build step:
  - `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-pcode`
- `cargo check -p fission-pcode`
- `cargo check -p fission-automation` (if telemetry/report changes)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500`

#### Success criteria
- all tests/checks pass
- `discovery_rejected_noncanonical_layout_count` decreases on both 200 and 500 samples
- `promotion_candidate_count` and/or `promoted_region_count` increases on at least one large sample
- no meaningful regression in:
  - `canonicalization_failed_nested_tail_escape`
  - `canonicalization_failed_interleaved_join_uses`
  - `canonicalization_failed_alias_has_nonlocal_ref_count`

## Acceptance Criteria

The plan is considered successful when all of the following are true:

1. `cargo test -p fission-pcode` passes
2. `cargo check -p fission-pcode` passes
3. any added telemetry/reporting wiring compiles cleanly
4. both expanded `nir` samples (200 and 500) complete successfully
5. discovery rejection count decreases on at least one large sample without creating a larger new failure bucket elsewhere

## Risks and Guardrails

### Risks
- accepting lexically awkward layouts that are not truly front-path equivalent
- shifting failures from discovery into worse downstream mis-structuring
- introducing sample-local gains that do not survive the 500-function run

### Guardrails
- only accept when next-flow / common-follow equivalence is explicit
- keep nested / nonlocal / escape cases conservative
- prefer additional subtype telemetry over broad acceptance if the next blocker is still unclear

## Concrete First Change Recommendation

Start with the **single most common unsplit discovery rejection path in `guards.rs`**, not a broad rewrite.

Execution order:

1. identify exact discovery-only rejection branch still inflating `discovery_rejected_noncanonical_layout_count`
2. add minimal subtype telemetry if still aggregated
3. relax one CFG-equivalent case using Ghidra/RetDec invariants
4. add 2-3 tight regressions
5. run 200 + 500 samples and compare deltas

## Expected Next Bottleneck After This Plan

If this succeeds, the next likely frontier will be one of:

- `canonicalization_failed_interleaved_join_uses`
- `canonicalization_failed_nested_tail_escape`
- a newly revealed discovery subtype from the remaining noncanonical bucket
