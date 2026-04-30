# 2026-05-01 Changelog

## SLEIGH canonical path purge

- Removed the canonical x86 subtable-name cursor policy from `decode_metadata`; generated schema compatibility fields remain, but raw P-code cursor progression no longer uses architecture/table-name policy bits.
- Removed direct opcode-prefix, opcode-length, ModRM/SIB, RIP-relative, and shared-token decoding from the canonical compiled-table token path.
- Removed raw P-code fixed-handle synthesis from `BoundOperand`; operands now need `.sla` token/value/varnode metadata or exported fixed handles, otherwise the instruction fails closed.
- Removed no-export subtable fallback binding. `BUILD` now requires child constructor traversal to materialize an exported handle unless the exact `.sla` ownership can prove no parent handle dependency.
- Removed the `construct_tpl` section-less fallback that selected an arbitrary template when main/named section identity was unresolved.
- Added audit coverage over the split compiled-table files so forbidden compatibility symbols are checked in the actual implementation files, not only the include wrappers.

## Raw P-code gate

Report path:

- `benchmark/artifacts/raw_p_code_benchmark/sla_1to1_no_heuristics/aggregate_raw_pcode_parity_report.json`

Bucket totals after the purge:

- `full_match`: 21
- `unsupported_template`: 10
- `missing_fission_instruction`: 14
- `ghidra_decode_error`: 1
- `both_decode_error_or_padding`: 1

Safety invariants:

- `compat_emitter_used`: 0
- `fake_placeholder_op`: 0
- `invalid_pcode_shape`: 0
- successful rows reported `template_source=sla_construct_tpl`

Similarity/parity after the purge:

- `average_similarity_score`: 0.45588235294117646
- `average_parity_ratio`: 0.45588235294117646

The coverage drop is intentional for this wave: rows that previously depended on legacy token/manual-handle fallback now fail closed instead of producing approximate P-code.

## Validation

- `cargo check -p fission-sleigh` passed.
- SLEIGH audit tests passed:
  - `compiled_table_policy_symbols_stay_architecture_neutral`
  - `canonical_template_executor_has_no_compatibility_success_entrypoints`
  - `canonical_template_executor_does_not_materialize_from_bound_operand_helpers`
- `cargo build --release -p fission-cli` passed.
- `python3 -m py_compile benchmark/raw_p_code_benchmark/*.py` passed.
- Full `cargo test -p fission-sleigh -- --test-threads=1` was stopped after `compiles_all_checked_in_entry_specs` ran for more than 15 minutes at 100% CPU with no progress output. Earlier compiler/SLA native identity tests in that run had passed.

## Remaining owners

- Exact `.sla` operand cursor metadata must replace any constructor that now fails with `legacy_*` unsupported reasons.
- Subconstructor export handle coverage must be completed from decoded `OperandSymbol`/`HandleTpl` identity.
- Full all-entry-spec generation/test runtime still needs a separate determinism/performance owner.
