# Preserve aggregate buffers through byte writes and whole-value stores

## 1. Baseline Row Anchor

- Binary: DecBench sample-set `bin_011.elf`
- Function: stripped `sub_1740`, address `0x1740`
- Reproduction: release `fission_cli decomp bin_011.elf --addr 0x1740 --layer both --prehir --json --debug-decomp`
- The original HIR bound 16-byte stack values as `char *` and assigned them to `fission_agg16` output fields. Clang 22.1.8 rejected those pointer-to-integer assignments with `-Werror=int-conversion`.
- Disassembly shows four 16-byte loads from stack offsets `{0,16,32,48}` followed by stores to `r12 + {0,16,32,48}`. The evalkit row has no executable source cases; raw p-code and disassembly establish the copy widths and destinations.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [x] Normalize / type recovery
- [ ] Structuring
- [ ] Printer
- [ ] Benchmark/automation

The failure spans two existing owners:

1. **Builder stack-address resolution:** `r12 = rdi` is followed by later reuse of `rdi` as a stack address. Recursive resolution used the later store's site instead of the producer's site, so the first 16-byte output store was rendered as a write to a local. Carrying `LoweringSite` through recursive stack and constant resolution keeps each p-code input tied to its producer operation.
2. **Callsite type propagation:** raw HIR represents `local_58` and `local_48` as `fission_agg16` buffers. `strncpy(&local_58, ...)` was collected as if the value of `local_58` itself were passed, so the API's `char *` parameter surface was attached to the aggregate binding.
3. **Type constraint propagation:** a later narrow initialization could replace a fieldless aggregate binding with the initializer's scalar type. The collector did not account for the value also being stored through an aggregate `FieldAccess` of the same size.

Evidence from the real row:

```text
P-code:
  r12 = rdi
  later: rdi = stack-address values for calls
  16-byte LOADs from rsp + {0,16,32,48}
  16-byte STOREs to r12 + {0,16,32,48}

Raw HIR after producer-site resolution:
  fission_agg16 local_58;
  strncpy(&local_58, ...);
  *(fission_agg16 *)(32 + r12) = local_58;

Normalize before the fix:
  AddressOfLocal(local_58) received the callee's char* surface type;
  the fieldless aggregate was subsequently narrowed from its 16-bit initializer.
```

## 3. Generality / Invariant Proof

- Recursive p-code input resolution uses the producer operation's program point, including RSP and aliased constants.
- A pointer parameter contract for `AddressOfLocal(x)` describes the address view passed to the callee; it must not become the value declaration of `x`.
- A fieldless aggregate that is stored whole through a same-sized aggregate access keeps its width when a narrower assignment initializes part of its storage. Same-width scalarization remains valid when there is no whole-aggregate store use.
- None of these rules names a binary, function, address, ISA, or compiler.

Regression coverage:

- `recursive_stack_address_uses_producer_site` preserves an incoming output pointer across register reuse and checks that an RSP-derived value uses its producer-site delta.
- `api_pointer_contract_does_not_retype_addressed_aggregate_storage` covers an aggregate buffer passed by address to `strncpy`.
- `narrow_initializer_does_not_scalarize_whole_aggregate_store_value` covers a narrow initialization followed by a whole-value `FieldAccess` store. `fieldless_aggregate_constant_refines_binding_to_same_width_integer` protects the existing scalarization case.

## 4. Risk And Ownership Check

- Existing owners: `PreviewBuilder` stack-address def-use resolution, `callsite_type_prop.rs` argument binding, and `constraint.rs` assignment/type unification.
- Shared analysis candidates: reaching definitions and aggregate store-use facts; this change extends the current owners and adds no pass.
- Interactions: ABI stack-base recovery, `AddressOfLocal` calls with typed pointees, byte writes into aggregate storage, and whole-value copies.
- Conservative boundary: address-of-local arguments no longer apply the callee's pointer surface directly to the object's value binding. Separate pointee inference can be added when the object type is independently established.
- Telemetry impact: none.

## 5. Validation Matrix

- [x] Targeted regressions: producer-site, addressed aggregate, whole-value FieldAccess, and the existing same-width aggregate scalarization test.
- [x] `cargo nextest run -p fission-midend-normalize`: 434 passed.
- [x] `cargo nextest run -p fission-pcode`: 1133 passed, 3 failed, 1 skipped. The three failures also reproduce on the untouched root checkout at the same base commit: `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load`.
- [x] Release CLI on `bin_011.elf@0x1740`: PreHIR/NIR/HIR retain all four 16-byte aggregate locals and store those values into the output fields; none of the aggregate field stores receives a `char *` binding.
- [x] Clang 22 C2x check with `-Werror=int-conversion`: compilation succeeds after neutralizing unrelated declarations. One pointer-sign warning remains on a byte-buffer call; no aggregate pointer-to-integer errors remain.
- [x] Non-Docker CLI smoke on `bin_012.elf@0x2d50`, `bin_014.elf@0x1f800`, and `bin_029.elf@0x38b0`: all commands exited successfully through Rust-Sleigh without fallback. This confirms the real CLI path runs; it is not a comparative regression or type-quality result. `bin_014` still shows a separate pointer/field typing inconsistency, and its baseline was not compared.
- `cargo check -p fission-pcode`, `cargo fmt --all --check`, and `git diff --check` pass. `cargo build --release -p fission-cli` succeeds.

Docker-backed external benchmark validation is unavailable because the user removed Docker. No benchmark quality or readability claim is part of this correctness fix.

## 6. AI Review / Prompt Firewall

- No other model was asked for implementation advice.
- No external prompt or benchmark identity was shared.

## 7. Review Notes

- Production behavior is invariant-based and has no row-specific guards.
- Report the actual compiler and sample results; do not claim broad quality gains from this focused correctness repair.
