# Decompiler Change Proposal: Preserve Entry-Owned Loop-Carried Register Seeds

## 1. Baseline Row Anchor

- Binary: `math_gcc_O2.exe`
- Function: `gcd`
- Address: `0x1400016e0`
- Corpus row or benchmark command: local `fission-benchmark` holdout `core_c_pe_holdout`, `--function gcd`, Fission `local-bb907a58f`, caches disabled
- Current output summary: the signature contains `param_1, param_2`, but the loop reads an uninitialized `rdx`; the final body returns `param_1` instead of the Euclidean remainder/divisor state.
- Semantic cases passed / total: `2/6` for the six compiler variants; `gcc -O2` passed `2/6`, `clang -O2` passed `2/6`, `gcc-m32 -O2` timed out.
- Failure category: three assertion failures and one timeout in the focused external semantic harness.
- Relevant benchmark/static/readability observations: the raw p-code contains `test edx, edx` before any RDX definition, then the loop reads RDX before the backedge writes the remainder back to RDX. The baseline PreHIR instead emits `uVar6 = rdx`, divides by `rdx`, and never initializes that carrier from `param_2`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [x] Builder/materialize:
- [x] HIR presentation contract:
- [ ] Normalize:
- [ ] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
Raw p-code is complete and ABI evidence is present:

  IntAnd   flags <- RDX32, RDX32       ; test edx, edx at entry
  CBranch  ... <- flags
  Copy     RCX32 <- RDX32              ; first loop iteration reads RDX
  IntSDiv  ... <- dividend, RDX64
  IntSRem  ... <- dividend, RDX64
  SubPiece EDX32 <- remainder
  IntZExt  RDX64 <- EDX32               ; backedge update

The builder's loop-header missing-merge path records:

  relation = LoopHeaderMergeMissing
  missing incoming = entry default
  materialized output binding = rdx

The ABI slot is already proven as entry-owned (`param_2`) by the entry test,
but `live_register_lhs_name_for_safe_missing_merge` unconditionally selects the
hardware name for this loop-carried merge. The first read and the backedge
update therefore share `rdx` without an initializer from `param_2`.

The same row exposed a second, downstream correctness defect after the carrier
was restored: HIR presentation's formal-alias propagation treated `uVar6 = b`
as globally stable even though the loop later assigns the Euclidean remainder
back to `b`. The canonical presentation owner now admits only formals with no
body definitions as alias sources, preserving the carrier across that write.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
When a loop-header register merge has an entry-default incoming value and the
register's ABI/cspec slot is within the already-proven entry parameter arity,
the loop carrier must use that formal parameter as its seed. The merge may
still use a private/hardware binding when the slot is not entry-owned or when a
prior local definition proves that the register is an internal accumulator.
Register-slot membership comes from the ABI model; loop ownership comes from
the existing CFG/scalar-SSA merge proof.
```

ISA-agnostic check (ADR 0009):

- [x] The production condition uses ABI slot ownership plus the existing generic loop-header merge proof, not a function/address guard.
- [x] Register offsets and parameter names remain supplied by the cspec/register namer.
- [x] The synthetic test uses a generic entry-read/loop-backedge shape and does not identify the motivating binary.

Comparable coverage:

- Similar shape 1: a register parameter is tested at entry, read before its first loop-carried rewrite, and then updated on the backedge.
- Similar shape 2: an ABI-capable register is defined before use and must remain a private loop carrier rather than becoming a formal parameter.
- Synthetic invariant test: a Win64 RDX entry read followed by a loop-header remainder update uses `param_2` as the carrier; a prior-defined RDX scratch remains non-parameter.
- Synthetic presentation regression: a carrier copied from a formal that is
  reassigned in the loop is retained, while an alias from an unchanged formal
  is still folded.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `live_register_lhs_name_for_safe_missing_merge` in `materialize/register_join.rs`, called by the existing materialized-output binding path.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the missing-merge proof already establishes that the output is a scalar loop carrier. The change only chooses the already-proven formal name when the ABI slot is entry-owned; no new dataflow pass or printer behavior is needed.
- Possible interaction with existing normalize/structuring/materialize passes: parameter registers that are overwritten after entry may become mutable formal carriers. Prior local definitions and unproven ABI slots must retain their current private/hardware bindings.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: internal RDX/R8/R9 accumulators with a prior definition, unproven entry-default register slots, non-scalar/side-effectful merges, and existing pointer-cursor cases whose source register is not an entry-owned ABI slot.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode shared_loop_exit_uses_entry_alias_carrier_binding loop_body_parameter_passthrough_uses_source_formal_before_carrier_write loop_header_missing_merge_uses_entry_owned_parameter_binding does_not_fold_alias_from_formal_reassigned_in_loop folds_alias_from_unchanged_formal`
  - Expected signal: the old binding is not selected; the formal seed and loop update share one `param_2` carrier, and HIR does not substitute a mutable formal across its write.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no new failures beyond the three existing tracked lower-expression failures.
- [ ] Focused benchmark row:
  - Command: `fission-benchmark` holdout `core_c_pe_holdout --function gcd --decompilers fission --run-mode local --no-resume`, caches disabled
  - Expected row-level improvement: the x64 optimized gcd rows execute the Euclidean remainder path and pass more wrapper cases.
- [ ] Smoke or automation sample:
  - Command: broader holdout/core C smoke after the focused rerun
  - Expected no-regression signal: existing behavior statuses and case counts do not regress.
- [ ] Optional related checks:
  - Command: `cargo nextest run -p fission-emulator`, `cargo check`, `cargo fmt --all --check`, `git diff --check`, `cargo build -p fission-cli --release`
  - Expected signal: all pass, with known unrelated pcode failures recorded if still present.
- [x] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency is planned.
  - Expected signal: no boundary change.

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
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the existing register-join/materialization owner is extended.
