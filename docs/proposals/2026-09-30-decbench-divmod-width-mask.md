# DecBench wide multiplication in magic-number division recovery

## 1. Baseline Row Anchor

- Binary: `certtool`, DecBench sample-set revision `abee628fdb9f4b1622127bbdfc2ea6437fda288b`
- Function: `yyparse`
- Address: `0x253e0`
- Corpus row or benchmark command: Fission tree `cd55be5b36d8618f3574339af4a9c208c2f95bb4`; `fission_cli decomp <binary> --addr 0x253e0 --timeout-ms 45000 --engine rust-sleigh --layer both --json --no-db --no-warnings`
- Current output summary: the pinned release sample-set run returns assembly fallback after 45 seconds. A fresh debug CLI run reaches `recognize_magic_number_division` and panics at `div_mod.rs:783` with “attempt to shift left with overflow”.
- Semantic cases passed / total: Not supplied by this DecBench sample-set; it has GED/type scores, not a behavioral case oracle.
- Failure category: normalize panic while a u64 mask is shifted by a multiplication width above 63.
- Relevant benchmark/static/readability observations: GED is unscoreable for this row and type-match is `0.0`; the emitted result is not C.

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
Fresh debug CLI decompilation of the anchored row:
  crates/fission-midend-normalize/src/arith/div_mod.rs:783:17
  attempt to shift left with overflow

The expression is `(1u64 << bits) - 1`; the branch special-cases only width
64, so widths greater than 64 reach a shift that cannot be represented by
u64. The multiplier constant is stored as `i64` and converted to `u64`, so a
width of at least 64 needs an all-ones u64 mask.
```

The failure originates in the normalizer's magic-number division matcher. The same release row is not scoreable because its result is assembly fallback; there is no measured quality gain to claim before rerunning it.

## 3. Generality / Invariant Proof

Generalized rule:

```text
The magic-number recognizer stores the multiplier constant in `i64`, converts
it to `u64`, and computes with `u128` intermediates. For declared multiplication
widths below 64, mask the represented low bits. At widths of 64 or more, every
represented u64 bit is in range, so use `u64::MAX`; do not shift a u64 by the
declared width or truncate a wider multiply to a narrower mask.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] The condition is based on the integer operation width, not an ISA, calling convention, or row identity.
- [x] No architecture-specific control-structure rule is involved.
- [x] Synthetic coverage will exercise widths at and beyond the u64 boundary.

Comparable coverage:

- No second row with this same mask-width panic was present in the pinned failure bucket.
- Synthetic invariant test: a 128-bit multiply shape is handled without panic; low-width masks at 32 and 64 bits stay unchanged.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `fission-midend-normalize::arith::div_mod::recognize_magic_number_division`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [x] None; the input-width precondition belongs in the existing arithmetic recognizer.
- Why extending that owner is sufficient: the matcher already calculates a u64 mask and u128 quotient intermediates; a width-aware u64 mask matches those existing representations.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass, helper API, or metric is planned.
- Possible interaction with existing normalize/structuring/materialize passes: wide multiplication keeps using the existing u128 arithmetic; low-width cases must keep their current mask and result.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: None.
- Known cases that must not change: existing signed/unsigned 32-bit and 64-bit magic-number division rows and tests.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize magic_number_division_handles_multiplication_width_above_u64`
  - Expected signal: the width-128 case does not panic; low-width masks remain unchanged.
  - Result: passed.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no NIR/HIR regressions.
  - Result: passed (1,153 tests; 1 skipped).
- [x] Focused benchmark row:
  - Command: pinned DecBench row, `--engine rust-sleigh --layer both --timeout-ms 45000 --no-db`, followed by `FISSION_BENCHMARK_NO_CACHE=1` scoring.
  - Result: the row returned NIR/HIR C output in 3.38 seconds without a panic or timeout. GED remained a large approximate distance (496) and type-match remained 0%; no quality gain is claimed.
- [x] Full pinned sample-set regression:
  - Command: 250 DecBench sample-set functions, NIR/HIR, `--engine rust-sleigh --timeout-ms 45000 --no-db`; GED/type scoring with `FISSION_BENCHMARK_NO_CACHE=1`; package validation and Linux/amd64 GCC 13 compile-rate pass.
  - Result: 250/250 records, 0 fallbacks, 0 missing. NIR GED perfect count stayed 65 (65/233 after vs. 65/228 before); HIR stayed 66 (66/233 vs. 66/228). The new outputs made five GED rows scorable, but their large approximate distances raised the means (NIR 26.21→27.23, HIR 28.75→29.84); no quality gain is claimed. Structural/text type perfect counts stayed unchanged. GCC compile validity rose from 80/158 to 81/158 for NIR and 81/158 to 82/158 for HIR; byte-match perfect counts stayed 1/158 and 2/158. Both packages validated at 224/224 binaries and 250/250 functions with no warnings.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize`, `cargo check -p fission-pcode`, `cargo check -p fission-decompiler`, `cargo build -p fission-cli --release`, and `cargo fmt --all --check`.
  - Expected signal: clean.
  - Result: all passed.
- [x] Boundary audit, if a new pass/helper/dependency was added: not applicable; no new pass, helper API, metric, or dependency is planned.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in an external AI prompt: none.
- Redaction confirmed: no external implementation prompt was used.
- Ghidra guidance confirmed: not applicable; no output-style imitation is proposed.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run; the complete pinned sample set was used for regression checking, not as an unseen tuning pool.
  - Synthetic invariant test command/result: passed; 128-bit multiplication width does not panic.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed in the width-boundary guard and regression test.
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed; the row no longer panics but GED/type scores show no measured quality improvement.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; implementation adds only an input-width guard to the existing arithmetic owner.
