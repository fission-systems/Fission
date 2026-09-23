# Decompiler Change Proposal: API Pointer Returns on Call Carriers

## 1. Baseline Row Anchor

- Binary: `libc_types_gcc_O2.exe` in the external DecBench dev corpus
- Function: `open_reader`
- Address: `0x1400014b0`
- Corpus row or benchmark command:
  `python runner/runner.py --corpus dev --function open_reader --decompilers fission --output /tmp/fission_issue70_before_be66635.json --no-resume --run-mode local`
- Current output summary: incoming `path` and `FILE* open_reader(...)` are recovered, but the generated project prelude defines `FILE` as `unsigned long long` and declares `fopen` / `setvbuf` with guessed `unsigned long long` returns. The function's call-result copy chain therefore has a pointer/integer mismatch and does not compile against the host `<stdio.h>`.
- Semantic cases passed / total: 0/5 for each of 7 compiler rows; all 7 are `compile_error`.
- Failure category: compilation failure. The generated scalar `FILE` typedef conflicts with the platform's stdio definition; the function body also has a pointer/integer mismatch on the call-result copy chain.
- Relevant benchmark/static/readability observations: aggregate row mean semantic pass rate 0.0, type-match accuracy 0.5, and GED perfect on 6/7 rows (mean GED 0.8571). The x86-64 `-O2` output has `FILE* rax`, `unsigned long long rbx`, then `rax = fopen(...)`, `rbx = rax`, and `return rbx`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [ ] Structuring:
- [x] Type/data recovery:
- [x] Printer / translation-unit declaration closure:
- [ ] Benchmark/automation:

Evidence:

```text
callsite_type_prop resolved the informative fopen API return contract but did
not apply it to the pointer-width integer call carrier or its safe aliases.
The API resource has the signature
`fopen|FILE*|__filename:char*,__modes:char*`.
The renderer then inferred a scalar typedef for FILE and guessed duplicate
stdio prototypes from those scalar call types instead of relying on stdio.h.
The separately inspected function body and returned source parameter were
already recovered on the clean baseline, so the measured defect is the broken
type/declaration closure that made all seven rows fail compilation.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
An informative API pointer return is a return-type contract. Apply it to the
call expression and to a pointer-width integer receiver only when that
receiver is a stable single-definition, non-self-referential call result and
has no conflicting surface type. Pointer argument contracts may promote a
scalar alias only when its exact-width, bit-preserving local-copy chain ends
at a binding already known to be a pointer; arbitrary pointer-width integers
remain integers. Strip only exact-pointer-width integer cast round trips on
such pointer copies. Remove a call-result store only when structured
liveness proves it dead and the call has a known API signature; preserve the
call side effect and leave opaque/indirect calls intact. Liveness treats
terminal returns as non-fallthrough, while address-taken and stack-backed
locals remain protected. At the translation-unit boundary, use the standard
header that owns a recognized standard-library type/symbol rather than
inventing a conflicting scalar typedef or duplicate guessed prototype.
Reused register names and conflicting declarations remain untouched.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is not gated on a calling-convention or ISA enum.
- [x] No ISA-specific data is needed; the pointer width comes from the function ABI width.
- [x] Synthetic test will express the call-result/copy def-use shape without a function name or address.

Comparable coverage:

- Similar shape 1: typed API pointer return assigned to a pointer-width scalar call carrier.
- Similar shape 2: stable copy alias of that call carrier returned from the caller.
- Synthetic invariant test: exact typed API return plus single-definition copy; negative reused/multi-definition receiver.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: normalize `callsite_type_prop`; `type_flow` owns propagation across safe copies; layered rendering owns project prelude/type closure.
- Shared analysis/substrate candidate:
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
- Why extending that owner is sufficient: the pass already resolves informative API return types, computes definition counts/self-reference, and the existing type-flow fixed point propagates safe copy facts. Layered rendering already owns named-type prelude declarations and called external declarations, so it can include `<stdio.h>` and defer stdio symbols to that header. No new pass or dependency is needed.
- Possible interaction with existing normalize/structuring/materialize passes: pointer return evidence improves aliases; layered output must not synthesize a second `FILE` definition or redeclare stdio functions with guessed return types. Multi-definition/reused carriers must not be rewritten.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none expected.
- Known cases that must not change: reused call receivers, conflicting surface-typed scalar bindings, non-pointer scalar API returns, and non-API calls.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-midend-normalize callsite_type_prop`
  - Result: API pointer return updates a stable result carrier and pointer-copy aliases; exact-width pointer cast round trips are removed; an unknown indirect call receiver is preserved. Related liveness and dead-result cases are covered by focused tests including `api_pointer_return_survives_a_dead_reused_register_result`, `pointer_width_round_trip_casts_are_removed_only_for_pointer_copies`, `api_pointer_parameter_promotes_carrier_and_elides_pointer_round_trip`, and `unknown_indirect_call_receiver_is_not_removed_from_machine_state`.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize`
  - Result: 427 passed, 0 skipped.
- [x] Focused benchmark row across compiler variants:
  - Command: `python runner/runner.py --corpus dev --function open_reader --decompilers fission --output /tmp/fission_issue70_final_96c8ca.json --no-resume --run-mode local` with the final local Fission build fingerprint.
  - Result: all 7 compiler rows passed 5/5 semantic cases (0/7 to 7/7 perfect rows; semantic mean 0.0 to 1.0); no compile failures. Type-match remained 0.5 and mean GED remained 0.8571, so no improvement is claimed for those metrics.
- [x] Smoke / regression sample:
  - Result: the external patch-validation pool has no canonical runnable oracle/entrypoint in the available benchmark checkout. The complete normalize suite, full pcode suite, and all seven focused compiler variants were used instead. The pcode suite has three unrelated failures also present on baseline `be66635f7`: `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load`.
- [x] Optional related checks:
  - Command: `cargo check -p fission-midend-normalize`, `cargo check -p fission-pcode`, `cargo build -p fission-cli --release`, `cargo fmt --all --check`, `git diff --check`.
  - Result: all passed. Host cross-target `cargo zigbuild` lacked the configured Rust target, then the documented local Docker build completed successfully; the benchmark service reported the same final build fingerprint.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- Information exposed in the AI prompt:
  - [x] Not applicable
- Redaction confirmed:
  - [x] Not applicable
- Ghidra guidance confirmed:
  - [x] Not applicable
- [x] Unseen or synthetic validation evidence:
  - Patch validation pool command/result: unavailable; no canonical pool runner/oracle was present in the external benchmark checkout.
  - Synthetic invariant tests: covered by normalize and pcode regression tests listed above; these complement, not replace, the measured real-binary DecBench row.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; reuse the existing callsite/type-flow owners.
