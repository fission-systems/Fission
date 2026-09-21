# Decompiler Change Proposal: Ignore Self-Clearing Register Inputs in ABI Arity Inference

## 1. Baseline Row Anchor

- Binary: `string_utils_gcc_O2.exe`
- Function: `find_substring`
- Address: `0x140001560`
- Corpus row or benchmark command: `fission-benchmark` dev corpus, `runner.py --function find_substring --decompilers fission --run-mode local --no-resume`, local build `3d2e396ee`
- Current output summary: x64 output declares phantom `param_3`/`param_4`; the inner loop compares against the first needle byte and returns `param_4` instead of the match index.
- Semantic cases passed / total: gcc-O2 `0/6`; across the nine available variants `27/54` cases passed.
- Failure category: gcc-O2 `compile_error` in the external wrapper; the direct decompilation is semantically wrong before compilation.
- Relevant benchmark/static/readability observations: raw p-code has the correct `R8 = needle[j]` load and `R9 = 0; R9++` index carrier. PreHIR instead exposes `param_3`/`param_4`, freezes the first byte as the comparison value, and leaves the zero seed implicit. Baseline external row output was 9 variants in 100.7 seconds.

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
Raw p-code is semantically complete:
  IntXor R9D <- R9D, R9D                 ; zero the outer match index
  Copy R8D <- R10D                       ; seed the inner needle byte
  Load R8D <- [RDX + RAX]                ; reload needle[j]
  IntAdd R9 <- R9, 1                     ; increment the match index

The current PreHIR is:
  do {
      uVar24 = *param_2;
      rax = 0;
      while (*(param_1 + rax) == uVar24) {
          rax++;
          param_3 = *(param_2 + rax);
          if (!uVar24) return param_4;
      }
      param_1++;
      param_4++;
  } while (*param_1);

`infer_entry_register_param_arity` counts the input side of `IntXor R9D,R9D`
as evidence that the incoming R9 slot is a parameter. That raises the inferred
entry arity to four; subsequent loop-carrier binding can then claim R8/R9 as
formal parameters even though both are defined before their semantic use.
The wrong fact is created in builder entry/loop binding, before normalize or
rendering.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Entry-register parameter arity may use a register input as evidence only when
the operation actually preserves information from the incoming register. A
self-clearing register definition (for example IntXor x,x or IntSub x,x) does
not read an incoming ABI value, even though p-code represents the operation
with that register in its input list. The same CFG/def-use rule applies to all
ABIs; register offsets and parameter slots remain supplied by the register
namer/cspec.
```

ISA-agnostic check (ADR 0009):

- [x] Production condition is not gated on one calling-convention or ISA enum.
- [x] ABI-specific register mapping remains in `RegisterNamer`; the rule uses
      p-code output/input identity and operation semantics.
- [x] Synthetic test states the self-clearing def-use shape without a binary
      address or compiler tuple.

Comparable coverage:

- Similar shape 1: `xor`-zeroed loop/accumulator registers in x86-64 and x86-32.
- Similar shape 2: a register parameter that is overwritten by a zero-producing
  operation before any later use must not enlarge the formal parameter list.
- Synthetic invariant test: Win64 R9 self-xor followed by a use must infer only
  the genuinely read preceding parameter slots.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `infer_entry_register_param_arity` in `midend/abi.rs`, consumed by `PreviewBuilder::new_with_binary`, and loop-carried binding selection in `materialize/loop_carried`.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: correcting entry arity removes the false formal slots before loop materialization. The same materialize owner must also preserve the existing scalar-SSA phi-latch proof when a backward lowering order reaches the latch before its preheader definition has been materialized. That proof may reserve the canonical temp binding for an ABI slot outside the proven entry arity and reuse it when the preheader definition and later merge are materialized; proven formal slots remain excluded. No normalize or printer cleanup is needed.
- Possible interaction with existing normalize/structuring/materialize passes: functions that currently rely on a register's self-zero input being counted as a parameter should lose only an unsupported formal slot; genuine read-before-write and read-modify-write cases must remain parameter evidence.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: genuine entry reads, read-modify-write updates, explicit register aliases, ARM32 incoming-parameter recovery, and functions with an actually live R8/R9 value before a definition.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode infer_entry_register_param_arity`
  - Expected signal: self-clearing register inputs do not enlarge inferred arity; genuine entry reads still do.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures; three existing lower-expression failures are tracked separately.
- [x] Focused benchmark row:
  - Command: external `fission-benchmark` runner for `find_substring` with caches disabled across all dev variants.
  - Expected row-level improvement: gcc-O2 no longer emits the phantom parameters/frozen first-byte comparison; semantic cases and/or wrapper compilability improve.
- [ ] Smoke or automation sample:
  - Command: broader smoke manifest after the focused rerun.
  - Expected no-regression signal: existing rows retain their behavior status and case counts.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode`, `cargo build -p fission-cli --release`, `cargo fmt --all --check`, `git diff --check`.
  - Expected signal: clean build and formatting.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency.
  - Expected signal: no boundary change.

## 8. Follow-up Owner Proof: Preheader Binding Reservation

The arity correction is necessary but not sufficient for this row. The
existing loop-head phi-latch proof already identifies the exact scalar-SSA
phi whose latch value is read at the loop head. On the real row, the proof
reaches the correct R8 and R9 phis but rejects them because the entry
definitions were not yet visited in the materialization order. The general
repair is to reserve the same canonical temporary binding that the ordinary
materializer will use for an entry definition when its ABI slot is not within
the proven entry arity, and let later merge binding selection reuse that
reservation. It does not infer a parameter, inspect an address, or rename a
binary-specific register.

Required regression shape: entry definition -> loop-head phi -> latch
reload/update -> loop-head read.

The focused test must fail when the latch gets a fresh hardware binding and
pass when both the preheader seed and latch use the one phi-carried binding.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in the AI prompt: none.
- Redaction confirmed: not applicable.
- Ghidra guidance confirmed: not applicable.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending implementation.
  - Synthetic invariant test command/result: pending implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the existing ABI arity owner is extended.
