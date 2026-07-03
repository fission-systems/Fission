# Decompiler Change Proposal

Use this before adding production code for semantic decompiler-quality changes
in builder, materialize, normalize, structuring, or type/data recovery. Keep the
answers short, evidence-backed, and specific enough for review.

## 1. Baseline Row Anchor

- Binary:
- Function:
- Address:
- Corpus row or benchmark command:
- Current output summary:
- Semantic cases passed / total:
- Failure category:
- Relevant benchmark/static/readability observations:

## 2. Owner Proof

Select the owner and cite the evidence. If multiple owners are involved, list
the first owner that creates the wrong fact.

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Paste the smallest p-code, NIR/HIR, CFG, output, or benchmark excerpt that
proves the owner.
```

## 3. Generality / Invariant Proof

The production condition must not depend on a function name, address, binary id,
corpus row, or row-identifying compiler artifact.

Generalized rule:

```text
Describe the ABI/ISA rule, p-code semantic, CFG/dominance/postdominance fact,
def-use fact, type constraint, calling-convention fact, memory alias fact, or
documented binary-format rule.
```

Comparable coverage:

- Similar shape 1:
- Similar shape 2:
- Synthetic invariant test:

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
- Why extending that owner is sufficient, or why a new pass/helper is needed:
- Possible interaction with existing normalize/structuring/materialize passes:
- Telemetry impact, if any:
- Known cases that must not change:

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command:
  - Expected signal:
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal:
- [ ] Focused benchmark row:
  - Command:
  - Expected row-level improvement:
- [ ] Smoke or automation sample:
  - Command:
  - Expected no-regression signal:
- [ ] Optional related checks:
  - Command:
  - Expected signal:

## 6. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [ ] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [ ] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [ ] Confirmed
