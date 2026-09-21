# Decompiler Change Proposal

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/libc_types_gcc_O2.exe`
- Function: `count_spaces`
- Address: `0x1400014f0`
- Corpus row or benchmark command: `c_libc_types.json`, `count_spaces`, GCC `-O2`
- Current output summary: the short-string path increments a fresh `xVar156`, but the
  epilogue returns `xVar188`, which is only initialized by the SIMD predecessor.
- Semantic cases passed / total: to be measured with the DecBench `count_spaces`
  wrapper before and after the production change.
- Failure category: builder/materialize accumulator merge; an initialized scalar
  count is split across a multi-predecessor join.
- Relevant benchmark/static/readability observations: direct raw-HIR output contains
  `xVar156 = rdx + 1` in the scalar path and `rax = xVar188` at the return. The
  source returns the space count for every string, including short strings.

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
The raw p-code is semantically complete:

  0x140001686 seq 410: rdx:8 = IntAdd(rdx:8, 1:8)
  0x14000169b seq 450: rax:8 = Copy(rdx:8)

The materialization trace reports a JoinMergeMissing proof for the RDX family
with three predecessors (0x140001665, 0x14000168a, 0x1400016b8), no missing
incoming values, and conflicting VarOrConst/Arithmetic values. The current
materializer has a proof for the n-way case, but its emitted merge statement
requires exactly two predecessor values. It therefore assigns the vector
incoming value to xVar188, leaves the scalar update as xVar156, and returns
xVar188. The accumulator-input proof also rejects RDX solely because its
hardware name is ABI-capable, although entry-arity inference finds only the
first register parameter in this function.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a register-family definition reaches a proven multi-predecessor join,
all complete, scalar, side-effect-free incoming definitions must share one
materialization binding. A register name being present in the calling
convention's parameter-slot table is not sufficient to protect it as an
entry parameter: only an entry-owned slot (slot < inferred entry arity) is
parameter-owned. A later definition proven by the local def-use/CFG facts may
be an accumulator and may reuse the join binding.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is based on register-family, entry-arity, def-use,
      and CFG merge facts rather than a function/address guard.
- [x] ISA-specific parameter slots remain supplied by the ABI/cspec/register
      namer; the merge rule is shared.
- [x] Synthetic coverage will describe a three-way register merge with one
      incoming arithmetic update and an ABI-capable but non-entry-owned register.

Comparable coverage:

- Similar shape 1: a two-way scalar register merge must retain its existing
  select/binding behavior.
- Similar shape 2: an entry-owned ABI parameter must not be rebound as a
  loop/join accumulator.
- Synthetic invariant test: n-way accumulator binding plus entry-parameter
  protection/redefinition cases.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `builder/materialize`
  (`merge_policy.rs`, `cross_block.rs`, and accumulator input proofs).
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the existing merge proof already
  computes predecessor completeness, incoming value kinds, and the reaching
  definition. The fix should make the binding emission honor that proof for
  n-way incoming assignments without adding a new pass.
- If adding a new pass/helper/metric, why existing shared analysis cannot express
  the invariant: no new pass/helper is planned.
- Possible interaction with existing normalize/structuring/materialize passes:
  only the materialized name shared by incoming definitions changes; normalize
  and structuring remain downstream consumers.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none expected.
- Known cases that must not change: two-way merge selects, true entry-owned
  register parameters, loop-body operands with incomplete or unsafe incoming
  definitions, and post-loop values whose defining operation is only a
  one-iteration delta.

## 5. Validation Matrix

- [ ] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode --filter-expr 'test(count_spaces) or test(merge)'`
  - Expected signal: old binding split fails; fixed n-way/redefinition cases pass.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the known baseline failures.
- [ ] Focused benchmark row:
  - Command: DecBench local `runner.py --corpus dev --function count_spaces --decompilers fission`
  - Expected row-level improvement: the `count_spaces` wrapper no longer returns an
    undefined/wrong short-string count and the emitted return value has a defined
    accumulator binding.
- [ ] Smoke or automation sample:
  - Command: the existing libc-types smoke subset, after the focused row.
  - Expected no-regression signal: other `libc_types` variants retain their prior
    behavior and compile status.
- [ ] Optional related checks:
  - Command: `cargo check -p fission-pcode`, `cargo build -p fission-cli --release`,
    `cargo fmt --all --check`, `git diff --check`
  - Expected signal: clean checks/build.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass/helper/dependency.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Required
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed; benchmark evidence will be reported separately after rerun.
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the existing materialize merge owner is extended.
