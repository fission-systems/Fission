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

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [ ] Production condition is not gated only on one calling convention / ISA enum
      (for example `X86_32`) when the fact is really register/loop/join/CFG.
- [ ] ISA-specific data lives in cspec / register namer / CC tables / SLEIGH, not
      a forked copy of the control-structure rule.
- [ ] Synthetic test states the CFG/dataflow shape without requiring a compiler
      tuple or function name.

Comparable coverage:

- Similar shape 1:
- Similar shape 2:
- Synthetic invariant test:

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior:
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed:
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant:
- Possible interaction with existing normalize/structuring/materialize passes:
- New or changed owner-to-owner dependency:
  - [ ] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below:
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
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Expected signal:

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [ ] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt:
  - [ ] Structural failure pattern only
  - [ ] Owner evidence only
  - [ ] Invariant candidates only
  - [ ] Validation matrix only
- Redaction confirmed:
  - [ ] Function names removed
  - [ ] Addresses removed
  - [ ] Binary paths removed
  - [ ] Corpus row ids removed
  - [ ] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [ ] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result:
  - Synthetic invariant test command/result:

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [ ] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [ ] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [ ] Confirmed
