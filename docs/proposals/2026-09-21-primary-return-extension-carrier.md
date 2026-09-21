# Decompiler Change Proposal: Preserve Register Value Snapshots Across Calls

Date: 2026-09-21
Issue: #115

## 1. Baseline Row Anchor

- Binary: `fission-benchmark/corpus/dev/binaries/c/libc_types_gcc_O2.exe`
- Function: `main`
- Address: `0x140002cf0`
- Corpus row / command: `libc_types.c`; release `fission_cli decomp` with
  `--addr 0x140002cf0 --project --json --no-db --no-warnings`.
- Current output: both `code_nir` and `code_prehir` lose the unconditional
  `count_spaces("a b c")` contribution and render the value as
  `rax ? tm_year_of(rax) : count_spaces("a b c")`.
- Failure category: builder/control-condition and builder/materialization
  register-snapshot correctness.
- Relevant observation: raw p-code is correct. The sequence is `Call
  _localtime64`, `RBX <- RAX`, `Call count_spaces`, `RDX <- RAX`,
  `EAX <- EAX xor EAX`, `RAX <- zext(EAX)`, `test RBX,RBX`, then the
  conditional call and join arithmetic. Two independent lowered-state errors
  followed: the branch predicate was lowered through the source name of the
  `RBX <- RAX` copy after `RAX` had been cleared, and a successor read of
  `RDX` was lowered before the predecessor's copy binding existed, so it
  reused the newer `RAX` name.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/control condition lowering
- [x] Builder/expr lowering
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
raw p-code: Copy RBX <- RAX; IntAnd unique <- RBX,RBX;
             IntEqual ZF <- unique,0; CBranch target,ZF
old predicate lowering: Eq(rax, 0), not Eq(rbx, 0)
old materialization: RDX <- RAX was later read as rax after RAX was reused
old join: rax + rax (the unconditional count_spaces value disappeared)
```

The p-code and the flag definition are correct. The first wrong branch input
is created by `lower_flag_tested_value`: it used to recursively lower the
source of an exact register copy, which made a value snapshot depend on the
source register's later name reuse. The earliest wrong arithmetic value is
created by cross-block expression lowering: the `RDX <- RAX` definition had
not yet been materialized when the successor's `RDX` use was visited, so the
source ABI name was reused instead of allocating/stabilizing the destination
binding. The full-width extension also had to claim the primary-return surface
after an observed call; otherwise a later cross-block `RAX` read could select
the stale call carrier rather than the extension result.

## 3. Generality / Invariant Proof

Generalized rules:

```text
An exact register Copy/Cast/extension is a value snapshot. A condition or
successor use must prefer the destination definition's stable binding over
re-lowering its source register.

When a full-width ABI primary-return register is rebuilt from a narrower
same-offset register after an observed call result, and the rebuilt value
reaches another block, preserve the full-width ABI surface. A narrow binding
does not write back the full-width call carrier.
```

The proof is based on register aliasing, call-result observation, and CFG
non-local liveness, and the def/use block relation. It does not inspect a
function name, address, binary, or compiler tuple.

ISA-agnostic check:

- [x] The condition uses the register model's primary-return role and pointer
  width, not a function or address guard.
- [x] Exact register-copy predicate recovery uses the p-code destination
  snapshot, not an ISA mnemonic or branch address.
- [x] Successor-before-predecessor lowering seeds the normal materialization
  binding for register passthroughs; it does not flush all state or fall back
  to the interpreter.
- [x] Existing ISA-specific register facts remain in the register namer/cspec;
  the materialization rule consumes those facts.
- [x] The synthetic tests express the copied-register predicate and the call,
  partial write, widening extension, and successor-before-predecessor use
  directly.

Comparable coverage:

- Similar shape 1: existing x64 call-result carrier tests for partial return
  register reads and cross-block redefinitions.
- Similar shape 2: existing full-width extension/cmov tests that require the
  ABI return surface for a later guarded write.
- Synthetic invariant test:
  `full_width_return_extension_does_not_reuse_partial_call_carrier`.
- Synthetic invariant test:
  `x64_test_branch_preserves_register_copy_snapshot`.

## 4. Risk And Ownership Check

- Existing owners: `PreviewBuilder::lower_flag_tested_value`,
  `PreviewBuilder::lower_varnode_inner`, and
  `PreviewBuilder::maybe_materialize_output_stmt`/
  `full_width_primary_return_surface_name`.
- Shared analysis candidate: def-use / CFG liveness facts already exposed by
  `output_has_nonlocal_use`; no new pass is needed.
- Extending the existing owners is sufficient because the defects are all
  value-snapshot/binding decisions already owned by condition lowering and
  materialization; no new pass or synchronization layer is needed.
- Known cases that must not change: local full-width extensions, the RC4
  low-byte truncation shape, same-block cmov joins, and call carriers that are
  already killed by a later full-width definition.
- Main risks: unnecessary primary-return surface bindings in a narrow set of
  call/partial-register shapes, and over-stabilizing a passthrough whose
  source is not a register value. The non-local-use/observed-call proof and
  exact register-input check keep the changes out of local-only extensions
  and memory/unique expressions.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Commands: `cargo nextest run -p fission-pcode -E
    'test(full_width_return_extension_does_not_reuse_partial_call_carrier) or
    test(x64_test_branch_preserves_register_copy_snapshot)'`
  - Old behavior: the branch test lowered `Eq(rax, 0)` and the materialization
    test could not establish a stable copied destination under successor-first
    lowering.
  - Fixed behavior: the tests pass with `Eq(rbx, 0)`, a stable copied
    destination, and a full-width `rax` binding.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1058 tests run: 1056 passed, 2 failed, 1 skipped. The only two
    failures were the pre-existing
    `movzx_after_byte_add_zero_extends_unsigned` and
    `x64_byte_add_movzx_does_not_double_add_load` tests. The new tests and the
    loop-exit regression passed.
- [x] Focused benchmark row:
  - Command: release CLI decompilation of the same `libc_types` function with
    caches disabled where applicable.
  - Before: branch recovery selected the wrong value and the final join lost
    the unconditional count, effectively producing `rax + rax`.
  - After: output contains `xVar1 = count_spaces("a b c")` and
    `rbx = (int)(rax + xVar1)`; the conditional fallback is zero.
- [x] Smoke or automation sample:
  - Commands: `cargo check`, `cargo build -p fission-cli --release`,
    `cargo nextest run -p fission-emulator`, and the existing materialize /
    call-carrier regression tests.
  - Result: workspace check, release build, emulator (200 passed, 3 skipped),
    and focused materialization/condition tests passed.
- [x] Optional related checks:
  - Commands: `cargo fmt --all --check`, `git diff --check`.
  - Result: passed.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Redaction confirmed:
  - [x] No external implementation prompt was used.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from synthetic tests alone:
  - [x] Confirmed; the real-binary row is remeasured separately.
- No new pass/helper/metric duplicates an existing owner:
  - [x] Confirmed
