# Recover ARM32 incoming stack parameters from the cspec frame contract

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/scale/binaries/O2-noinline/cleanflight/cleanflight_DALRCF405.elf`
- Function: `uartOpen`
- Address: `0x800b76a`
- Corpus row or benchmark command: first 200 functions from the ARM ELF, measured with `target/debug/fission_cli decomp ... --all --limit 200 --json --layer nir --no-db --no-warnings`
- Current output summary: DWARF declares six formal parameters, but the baseline Fission output emits three register parameters and renders the two entry-stack loads at `[sp,#0x60]` and `[sp,#0x64]` as `local_60` and `local_64`; `param_5`, `param_6`, and `param_7` occur zero times in the 200-function window. After the fix, the same cache-free window has `param_5 = 3`, `param_6 = 1`, and `param_7 = 0`; `uartOpen` is emitted with all six formals, including `uchar param_5` and `uchar param_6`.
- Semantic cases passed / total: not scored by the available local ARM scale window; this is an ABI/type-recovery anchor.
- Failure category: builder ABI/type recovery — incoming ARM32 stack slots are rejected before the existing entry-memory-SSA proof can classify them.
- Relevant benchmark/static/readability observations: the binary's Thumb disassembly has `push {r4,r5,r6,r7,r8,r9,lr}`, `sub sp,#0x44`, then `ldrb.w r6,[sp,#0x60]` and `ldrb.w r9,[sp,#0x64]`; these are entry-SP offsets `0` and `4`. The ARM `.cspec` declares four integer register slots and `stack_arg_base = 0`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder ABI, scalar memory SSA, and stack-slot classification
- [ ] Normalize/type recovery
- [ ] Structuring
- [ ] HIR presentation/printer
- [ ] Benchmark/automation

Evidence:

```text
AbiState::incoming_stack_parameter_index_from_entry_offset() contained the
already-proven entry-SP-relative calculation, but rejected every 32-bit ABI.
AbiState::incoming_stack_argument_index() separately rejects every ABI except
X86_32. PreviewBuilder::classify_stack_slot_origin() only invoked the
entry-memory-SSA path under `options.is_64bit`, so ARM32 entry-owned loads fell
through to ordinary stack locals even though ARM.cspec supplied the base. The
ARM prologue also materializes an immediate in a unique temporary before
subtracting it from SP; scalar pointer proof therefore needed to follow a
bounded Copy/Cast/ZExt/SExt constant chain as well.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
For any ABI whose resolved cspec has ordered integer register parameter slots,
a stack parameter base, and a nonzero pointer/slot size, an exact entry-SP-
relative stack load is formal parameter

    register_parameter_count + (entry_offset - stack_arg_base) / slot_size

only when the address is aligned, at or above the cspec base, scalar pointer
SSA proves the address in the entry-SP coordinate, and scalar memory SSA
proves that every reaching memory value is function-entry input. The pointer
proof may follow a bounded copy/cast/extension chain for a materialized
constant, but not an unknown arithmetic value. Stores performed by the
current function, unknown/bounded addresses, and misaligned offsets remain
ordinary stack storage.
```

- [x] The production condition is based on cspec ABI slots and scalar memory-SSA ownership, not an ARM function/address guard.
- [x] ISA-specific data remains in cspec/register models: ARM contributes four register slots, base zero, and pointer size four.
- [x] Synthetic coverage exercises direct entry ownership, a materialized
  prologue immediate, and overwritten-slot rejection without a compiler tuple
  or function name.

Comparable coverage:

- Similar shape 1: SysV AMD64 entry-owned stack load becomes `param_7`.
- Similar shape 2: Win64 shadow/home space remains below the cspec stack-argument base and is not promoted.
- Synthetic invariant test: ARM32 entry-owned `sp` load becomes `param_5`; a store to the same slot prevents promotion.

## 4. Risk And Ownership Check

- Existing owner: `AbiState` plus the canonical builder stack-slot classifier.
- Shared analysis/substrate candidate: existing scalar memory SSA and exact entry-SP pointer proof; no new fact map or pass.
- Why extending that owner is sufficient: all required facts already exist; the defect is the x64/X86_32 consumer gate and helper naming.
- Possible interaction with existing normalize/structuring/materialize passes: formal binding insertion changes only the recovered signature and the load's source binding; CFG and statement structure should remain unchanged.
- Known cases that must not change: SysV x64, Win64 home slots, x86-32 frame-relative incoming arguments, overwritten/unknown stack memory, and non-stack ARM locals.
- Outgoing call stack arguments: audit separately. The current non-64-bit early return in call recovery is intentionally retained unless an AAPCS call-store proof is added; merely making the index arithmetic available must not enable a broad local-store scan.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode --filter-expr 'test(arm32_incoming_stack_load_becomes_fifth_formal_parameter) | test(arm32_incoming_stack_load_follows_a_materialized_prologue_immediate) | test(arm32_overwritten_entry_stack_slot_does_not_become_formal_parameter) | test(x64_incoming_stack_slots_follow_cspec_register_and_frame_layout)'`
  - Result: 4 tests passed; ARM32 emits `param_5` for direct and materialized-prologue entry loads, while an overwritten entry slot remains non-parameter.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1,060 tests passed, 2 skipped.
- [x] Focused benchmark row:
  - Command: repeat the exact 200-function cache-free CLI measurement above and inspect `uartOpen` plus all changed ARM outputs.
  - Result: `param_5` 0 -> 3, `param_6` 0 -> 1, `param_7` remains 0. `uartOpen` changes from three visible register parameters plus two locals to six formals, with the two byte stack loads as `uchar param_5` and `uchar param_6`.
- [ ] Smoke or automation sample:
  - Command: external local ARM scale smoke, when the local benchmark runner is available.
  - Expected no-regression signal: existing ARM outputs without proven incoming stack loads stay unchanged.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode -p fission-decompiler` and `cargo build --release -p fission-cli`
  - Result: both `cargo check` and the release CLI build passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Row identities occur only in this local proposal; production code and synthetic tests use ABI/frame invariants only.

## 7. Review Notes

- [x] Production code contains no hardcoded binary/function/address/corpus guards.
- [x] The justification is metric-independent: an accessed entry-owned incoming stack slot is part of the formal ABI interface, not a local.
- [x] The implementation extends the existing ABI/stack-slot owner and does not add a parallel semantic layer.
