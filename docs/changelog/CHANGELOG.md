# Changelog

All notable changes to the Fission project (November 2025 – Present).

This file is the public-facing English changelog.  
The previous detailed Korean historical notes are preserved in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md).

---

## 2026-04-20 (latest)

### `0x140008090` single-consumer call RHS proof tracing

This wave stayed diagnostic-only. It does not widen single-consumer replacement, change stable-representative policy, or alter the default release path. The goal was to take the large `DisallowedSingleConsumer -> RhsHasCall` bucket on `0x140008090` and split it by call target/effect provenance using the existing call-effect summary infrastructure.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries call-RHS proof vocabulary:
  - `SingleConsumerCallRhsFamily`
    - `KnownPureIntrinsic`
    - `PreviewCalleeAnalysisUnsafe`
    - `UnknownInternalCall`
    - `ImportCall`
    - `CallOther`
    - `IndirectCall`
    - `UnknownCall`
  - `SingleConsumerCallRhsProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `single_consumer_call_rhs_family`
    - `single_consumer_call_rhs_effect_source`
    - `single_consumer_call_rhs_consumer_kind`
    - `single_consumer_call_rhs_downstream_opcode`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - recursive discovery of the first call expression inside a disallowed single-consumer RHS
  - `describe_single_consumer_call_rhs_proof(...)`
  - a narrow intrinsic allowlist for diagnostics only:
    - `__popcount`
    - `__carry`
    - `__scarry`
    - `__sborrow`
  - classification that distinguishes:
    - known pure intrinsics
    - preview-summary unsafe internal callees
    - import / callother / indirect / unknown call surfaces
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `single-consumer-call-rhs-proof output=... def_block=... def_op_seq=... consumer_op_seq=... call_target=... family=... call_effect_source=... writes_memory=... may_call_unknown=... may_exit=... return_used=... consumer_kind=... downstream_opcode=...`
  - repartition summary families for the new call-RHS histograms
  - the new proof is only emitted when the existing `disallowed-single-consumer` reason is `RhsHasCall`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode single_consumer_call_rhs_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed intent:

- direct intrinsic-looking call RHS sites such as `__popcount`, `__carry`, `__scarry`, and `__sborrow` are now separated from broad unknown/unsafe call RHS cases
- preview-summary unsafe internal callees remain explicit fail-closed stops in diagnostics instead of being conflated with pure intrinsic families
- the default release path remains unchanged; this wave only improves owner attribution for the next policy decision
### `0x140008900` parity-chain regression attribution

This wave stayed diagnostic-only. It does not widen parity materialization, change the default release path, or promote the parity-chain env gate. The goal was to explain why the env-gated `PopCount -> IntAnd(mask=1) -> CompareZero` slice regressed `0x140008900` even though the local proof closed on `0x140008090`.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries parity-chain regression reporting vocabulary:
  - `ParityChainConsumerContext`
    - `CompareZero`
    - `CompareNonZero`
    - `CompareOne`
    - `CompareNotOne`
  - `MaterializeOwnerRepartition` now also tracks:
    - `parity_chain_regression_role`
    - `parity_chain_regression_before_event`
    - `parity_chain_regression_consumer_context`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `describe_parity_chain_final_hir_expr(...)`
  - a debug-only formatter for the final parity compare expression that the env-gated path surfaces
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `parity-chain-regression-attribution output=... role=... popcount_op_seq=... intand_op_seq=... compare_op_seq=... before_materialized=... after_materialized=false before_event=... after_event=parity_chain_materialized final_hir_expr=... consumer_context=...`
  - summary families for:
    - `parity_chain_regression_role`
    - `parity_chain_regression_before_event`
    - `parity_chain_regression_consumer_context`
- [`mod.rs`](../../crates/fission-pcode/src/nir/builder/materialize/mod.rs) now computes a side-effect-free fallback replacement plan preview before the env-gated parity shortcut emits attribution traces. This keeps regression reporting aligned with the default release path without mutating counters or rejection summaries.

Validation:

- `cargo fmt --all`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008900 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008900 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008900 FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION=1 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008900 --engine nir --profile nir --ghidra-compat`

Observed intent:

- env-off:
  - parity-chain attribution is absent; default release behavior stays unchanged
- env-on:
  - parity-chain attribution now states whether the default path would have kept a `materialized_binding`, `inline_suppressed`, or `representative_downgrade` event before the env-gated shortcut erased the binding
  - the final surfaced parity expression and consumer context are emitted at the exact site that changed

### `0x140008090` parity chain materialization trial

This wave is env-gated policy only. It does not change the default release path. The goal was to take the now fully isolated `PopCount -> IntAnd(mask=1) -> CompareZero` family on `0x140008090` and let the builder treat it as a parity intrinsic chain when, and only when, the full same-block proof closes.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries parity-chain policy vocabulary:
  - `ParityChainRole`
    - `PopCountInput`
    - `PopCountResult`
    - `IntAndResult`
  - `ParityChainKeepReason`
    - `PopCountHasMultipleConsumers`
    - `IntAndMaskNotOne`
    - `IntAndHasMultipleConsumers`
    - `FinalConsumerNotCompare`
    - `CompareConstUnsupported`
    - `InterveningSideEffect`
    - `RhsNotLowCost`
    - `RhsHasLoad`
    - `RhsHasCall`
  - `ParityChainProof`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `parity_chain_materialization_enabled(...)`
  - `describe_parity_chain_proof(...)`
  - narrow same-block proofing for:
    - original value feeding `PopCount`
    - `PopCount` result feeding `IntAnd(mask=1)`
    - `IntAnd(mask=1)` result feeding `IntEqual`/`IntNotEqual`
  - intervening side-effect screening between chain members
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `parity-chain-materialized output=... role=... popcount_op_seq=... intand_op_seq=... compare_op_seq=... compare_opcode=... compare_const=... chain_low_cost=... chain_side_effect_free=...`
  - `parity-chain-kept output=... reason=...`
- [`mod.rs`](../../crates/fission-pcode/src/nir/builder/materialize/mod.rs) now consults the new proof before other env-gated restart experiments, but only when:
  - `FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION=1|true|yes`
  - default release behavior remains unchanged when the flag is unset

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode parity_chain_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 FISSION_ENABLE_PARITY_CHAIN_MATERIALIZATION=1 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py ... --limit 50 ...` (same-axis putty limit50 trial; see artifact section in the wave report)

Observed intent:

- env-off:
  - no release-path behavior change
- env-on:
  - only parity chains that stay same-block, low-cost, side-effect-free, and end in `IntEqual`/`IntNotEqual` against `0`/`1` are allowed to skip materialized representatives
  - non-parity or unstable chains stay fail-closed with explicit `parity-chain-kept` reasons

This wave is a trial, not a release promotion. The next decision depends on same-axis `putty limit50` results.

### `0x140008090` PopCount IntAnd chain proof tracing

This wave stayed diagnostic-only. It does not widen intrinsic replacement, alter stable-representative policy, or enable any env-gated path. The goal was to take the already isolated `PopCount -> IntAnd` arithmetic chain on `0x140008090` and determine whether it is a general arithmetic sink or a parity-like slice with a stable final consumer.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries PopCount-IntAnd chain vocabulary:
  - `PopCountIntAndMaskKind`
    - `AndOne`
    - `AndByteMask`
    - `AndPowerOfTwoMinusOne`
    - `AndNonPowerOfTwoMask`
    - `UnknownMask`
  - `PopCountIntAndDownstreamUseFamily`
    - `FeedsPredicate`
    - `FeedsCompareZero`
    - `FeedsCompareConst`
    - `FeedsArithmetic`
    - `FeedsStoreOrCall`
    - `FeedsUnknown`
  - `PopCountIntAndChainProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `popcount_intand_mask_kind`
    - `popcount_intand_downstream_use`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `classify_popcount_intand_mask_kind(...)`
  - `classify_popcount_intand_downstream_use_family(...)`
  - `describe_popcount_intand_chain_proof(...)`
  - same-block/cross-block downstream use classification for `PopCount -> IntAnd(mask)` chains
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `popcount-intand-chain-proof output=... popcount_input_rhs=... popcount_result=... def_block=... def_op_seq=... consumer_op_seq=... intand_op_seq=... intand_mask=... intand_mask_kind=... intand_result_consumer=... downstream_consumer_opcode=... chain_low_cost=... chain_side_effect_free=...`
  - summary families for:
    - `popcount_intand_mask_kind`
    - `popcount_intand_downstream_use`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode popcount_intand_chain_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `popcount_consumer_result_use`
  - `PopCountFeedsArithmetic=443`
- `popcount_consumer_downstream_opcode`
  - `IntAnd=443`
- `popcount_intand_mask_kind`
  - `AndOne=443`
- `popcount_intand_downstream_use`
  - `FeedsCompareZero=443`

Representative traces:

- `output=space:3 off:0xe100005000202700 size:8 popcount_input_rhs=Binary { op: And, lhs: Var("rsp"), rhs: Const(255, Int { bits: 64, signed: false }), ty: Int { bits: 64, signed: false } } popcount_result=space:3 off:0xe100005000202708 size:8 def_block=0x140008090 def_op_seq=38 consumer_op_seq=39 intand_op_seq=40 intand_mask=0x1 intand_mask_kind=AndOne intand_result_consumer=FeedsCompareZero downstream_consumer_opcode=IntEqual chain_low_cost=true chain_side_effect_free=true`
- `output=space:3 off:0xe100005000203640 size:4 popcount_input_rhs=Binary { op: And, lhs: Var("uVar71"), rhs: Const(255, Int { bits: 32, signed: false }), ty: Int { bits: 32, signed: false } } popcount_result=space:3 off:0xe100005000203648 size:4 def_block=0x140008090 def_op_seq=89 consumer_op_seq=90 intand_op_seq=91 intand_mask=0x1 intand_mask_kind=AndOne intand_result_consumer=FeedsCompareZero downstream_consumer_opcode=IntEqual chain_low_cost=true chain_side_effect_free=true`
- `output=space:3 off:0xe100005000203d60 size:1 popcount_input_rhs=Binary { op: And, lhs: Var("xVar111"), rhs: Const(255, Int { bits: 8, signed: false }), ty: Int { bits: 8, signed: false } } popcount_result=space:3 off:0xe100005000203d68 size:1 def_block=0x1400080e8 def_op_seq=15 consumer_op_seq=16 intand_op_seq=17 intand_mask=0x1 intand_mask_kind=AndOne intand_result_consumer=FeedsCompareZero downstream_consumer_opcode=IntEqual chain_low_cost=true chain_side_effect_free=true`

Conclusion:

- the `PopCount -> IntAnd` arithmetic chain is no longer broad
- on the live row it collapses to a parity-like family:
  - `popcount(x & 0xff)`
  - `& 1`
  - compared against zero
  - low-cost and side-effect-free under current proof
- the next owner is therefore a narrow parity intrinsic chain candidate rather than generic PopCount or arithmetic-consumer handling

### `0x140008090` PopCount consumer proof tracing

This wave stayed diagnostic-only. It does not widen single-consumer replacement, alter stable-representative policy, or enable any env-gated path. The goal was to take the now-isolated `UnknownConsumerKind -> PopCount=443` slice on `0x140008090` and determine whether the `PopCount` consumer behaves like a predicate-only intrinsic, an arithmetic dataflow consumer, or another downstream use family.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries PopCount proof vocabulary:
  - `PopCountResultUseFamily`
    - `PopCountFeedsPredicate`
    - `PopCountFeedsArithmetic`
    - `PopCountFeedsCompareZero`
    - `PopCountFeedsCompareConst`
    - `PopCountFeedsStoreOrCall`
    - `PopCountResultUnused`
    - `UnknownPopCountUse`
  - `PopCountConsumerProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `popcount_consumer_result_use`
    - `popcount_consumer_downstream_opcode`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `classify_popcount_result_use_family(...)`
  - `describe_popcount_consumer_proof(...)`
  - same-block and cross-block downstream-use classification for `PopCount` outputs
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `popcount-consumer-proof output=... def_block=... def_op_seq=... consumer_op_seq=... input_width=... output_width=... rhs_kind=... rhs=... rhs_has_call=... rhs_has_load=... rhs_low_cost=... popcount_result_used_by=... downstream_consumer_opcode=...`
  - summary families for:
    - `popcount_consumer_result_use`
    - `popcount_consumer_downstream_opcode`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode popcount_consumer_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `unknown_consumer_kind_reason`
  - `Unknown=443`
- `unknown_consumer_kind_opcode`
  - `PopCount=443`
- `popcount_consumer_result_use`
  - `PopCountFeedsArithmetic=443`
- `popcount_consumer_downstream_opcode`
  - `IntAnd=443`

Representative traces:

- `output=space:3 off:0xe100005000202700 size:8 def_block=0x140008090 def_op_seq=38 consumer_op_seq=39 input_width=8 output_width=8 rhs_kind=Arithmetic rhs=Binary { op: And, lhs: Var("rsp"), rhs: Const(255, Int { bits: 64, signed: false }), ty: Int { bits: 64, signed: false } } rhs_has_call=false rhs_has_load=false rhs_low_cost=true popcount_result_used_by=PopCountFeedsArithmetic downstream_consumer_opcode=IntAnd`
- `output=space:3 off:0xe100005000203640 size:4 def_block=0x140008090 def_op_seq=89 consumer_op_seq=90 input_width=4 output_width=4 rhs_kind=Arithmetic rhs=Binary { op: And, lhs: Var("uVar71"), rhs: Const(255, Int { bits: 32, signed: false }), ty: Int { bits: 32, signed: false } } rhs_has_call=false rhs_has_load=false rhs_low_cost=true popcount_result_used_by=PopCountFeedsArithmetic downstream_consumer_opcode=IntAnd`
- `output=space:3 off:0xe100005000203d60 size:1 def_block=0x1400080e8 def_op_seq=15 consumer_op_seq=16 input_width=1 output_width=1 rhs_kind=Arithmetic rhs=Binary { op: And, lhs: Var("xVar111"), rhs: Const(255, Int { bits: 8, signed: false }), ty: Int { bits: 8, signed: false } } rhs_has_call=false rhs_has_load=false rhs_low_cost=true popcount_result_used_by=PopCountFeedsArithmetic downstream_consumer_opcode=IntAnd`

Conclusion:

- the `PopCount` blind spot is no longer a generic intrinsic unknown
- on the live row it collapses to one arithmetic chain:
  - arithmetic rhs
  - `PopCount`
  - downstream `IntAnd`
  - no predicate-only or compare-only slice was observed
- the next owner is therefore not predicate-oriented `PopCount` normalization but narrower arithmetic/intrinsic consumer modeling for `PopCount -> IntAnd`

### `0x140008090` unknown consumer kind subtyping

This wave stayed diagnostic-only. It does not widen single-consumer replacement, alter stable-representative policy, or enable any env-gated path. The goal was to take the remaining `DisallowedSingleConsumer -> UnknownConsumerKind=443` slice on `0x140008090` and determine whether it still hides multiple consumer families or has already collapsed to one concrete blind spot.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries unknown-consumer-kind proof vocabulary:
  - `UnknownConsumerKindReason`
    - `ConsumerOpcodeUnhandled`
    - `ConsumerHasMultipleMatchedInputs`
    - `ConsumerInputRoleUnknown`
    - `ConsumerIsIndirectUse`
    - `ConsumerIsAddressComputation`
    - `ConsumerIsSubpieceOrCast`
    - `ConsumerIsControlLike`
    - `Unknown`
  - `UnknownConsumerKindProof`
  - `DisallowedSingleConsumerProof` now records `matched_input_indices`
  - `MaterializeOwnerRepartition` now also tracks:
    - `unknown_consumer_kind_reason`
    - `unknown_consumer_kind_opcode`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `classify_unknown_consumer_kind_reason(...)`
  - `describe_unknown_consumer_kind_proof(...)`
  - unknown-consumer classification for unhandled same-block single-consumer sites
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `unknown-consumer-kind output=... def_block=... def_op_seq=... consumer_block=... consumer_op_seq=... consumer_opcode=... matched_input_indices=... rhs_kind=... reason=...`
  - summary families for:
    - `unknown_consumer_kind_reason`
    - `unknown_consumer_kind_opcode`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode unknown_consumer_kind_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `disallowed_single_consumer_reason`
  - `ConsumerIsPredicate=567`
  - `RhsHasCall=479`
  - `RhsHasLoad=64`
  - `UnknownConsumerKind=443`
- `disallowed_single_consumer_consumer_kind`
  - `Predicate=603`
  - `UnknownConsumerKind=500`
  - `OtherData=450`
- `unknown_consumer_kind_reason`
  - `Unknown=443`
- `unknown_consumer_kind_opcode`
  - `PopCount=443`

Representative traces:

- `output=space:3 off:0xe100005000202700 size:8 def_block=0x140008090 def_op_seq=38 consumer_block=0x140008090 consumer_op_seq=39 consumer_opcode=PopCount matched_input_indices=[0] rhs_kind=Arithmetic reason=Unknown`
- `output=space:3 off:0xe100005000203640 size:4 def_block=0x140008090 def_op_seq=89 consumer_block=0x140008090 consumer_op_seq=90 consumer_opcode=PopCount matched_input_indices=[0] rhs_kind=Arithmetic reason=Unknown`
- `output=space:3 off:0xe100005000203d60 size:1 def_block=0x1400080e8 def_op_seq=15 consumer_block=0x1400080e8 consumer_op_seq=16 consumer_opcode=PopCount matched_input_indices=[0] rhs_kind=Arithmetic reason=Unknown`

Conclusion:

- the broad `UnknownConsumerKind` bucket is no longer broad on the live row
- it collapses almost entirely to a single consumer-opcode blind spot:
  - `PopCount` consumer
  - same-block single matched input
  - arithmetic rhs
  - still no safe release-policy conclusion
- the next owner is therefore not generic unknown-consumer handling but a narrower `PopCount` consumer classification / intrinsic-consumer modeling wave

### `0x140008090` low-bit mask predicate proof tracing

This wave stayed diagnostic-only. It does not widen predicate normalization, alter stable-representative policy, or enable any env-gated policy path. The goal was to take the now-isolated `LowBitAndOne=459` arithmetic predicate slice on `0x140008090` and determine whether it behaves like boolean-flag extraction or plain integer bit testing.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries low-bit-mask proof vocabulary:
  - `LowBitMaskPredicateFamily`
    - `BooleanFlagMask`
    - `IntegerBitTest`
    - `MaskFromCompareResult`
    - `MaskFromArithmeticValue`
    - `UnknownLowBitMask`
  - `LowBitMaskInputOriginKind`
    - `Compare`
    - `BoolOp`
    - `Arithmetic`
    - `Load`
    - `Call`
    - `Unknown`
  - `LowBitMaskPredicateProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `low_bit_mask_predicate_family`
    - `low_bit_mask_input_origin_kind`
    - `low_bit_mask_feeds_only_predicate`
    - `low_bit_mask_input_is_boolean_like`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `describe_low_bit_mask_predicate_proof(...)`
  - low-bit mask input extraction for `(x & 1)` style rhs
  - input origin classification
  - boolean-like input proof
  - narrow family classification for low-bit mask predicate consumers
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `low-bit-mask-proof output=... rhs=... mask_input=... consumer_guard=... feeds_only_predicate=... input_is_boolean_like=... input_origin_kind=... stable_required_reason=...`
  - summary families for:
    - `low_bit_mask_predicate_family`
    - `low_bit_mask_input_origin_kind`
    - `low_bit_mask_feeds_only_predicate`
    - `low_bit_mask_input_is_boolean_like`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode low_bit_mask_predicate_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `arithmetic_predicate_shape`
  - `LowBitAndOne=459`
  - `UnknownArithmetic=9`
- `arithmetic_predicate_stable_reason`
  - `ArithmeticMask=459`
- `low_bit_mask_predicate_family`
  - `IntegerBitTest=459`
- `low_bit_mask_input_origin_kind`
  - `Unknown=459`
- `low_bit_mask_feeds_only_predicate`
  - `true=459`
- `low_bit_mask_input_is_boolean_like`
  - `false=459`

Representative traces:

- `output=space:3 off:0xe100005000202710 size:8 rhs=Binary { op: And, lhs: Var("xVar31"), rhs: Const(1, Int { bits: 64, signed: false }), ty: Int { bits: 64, signed: false } } mask_input=Var("xVar31") consumer_guard=CompareZero feeds_only_predicate=true input_is_boolean_like=false input_origin_kind=Unknown stable_required_reason=ArithmeticMask`
- `output=space:3 off:0xe100005000203650 size:4 rhs=Binary { op: And, lhs: Var("uVar77"), rhs: Const(1, Int { bits: 32, signed: false }), ty: Int { bits: 32, signed: false } } mask_input=Var("uVar77") consumer_guard=CompareZero feeds_only_predicate=true input_is_boolean_like=false input_origin_kind=Unknown stable_required_reason=ArithmeticMask`
- `output=space:3 off:0xe100005000203d70 size:1 rhs=Binary { op: And, lhs: Var("xVar117"), rhs: Const(1, Int { bits: 8, signed: false }), ty: Int { bits: 8, signed: false } } mask_input=Var("xVar117") consumer_guard=CompareZero feeds_only_predicate=true input_is_boolean_like=false input_origin_kind=Unknown stable_required_reason=ArithmeticMask`

Conclusion:

- the dominant `LowBitAndOne` slice does not currently look like boolean-origin flag extraction
- in the live row, it behaves as:
  - single predicate consumers only
  - compare-zero consumers
  - integer-looking masked inputs with no boolean-like proof
  - stable-representative requirement still explained by `ArithmeticMask`
- the next owner is therefore not low-bit boolean normalization but either:
  - refining var/input provenance so `Var(...)` mask inputs are no longer opaque, or
  - keeping this family fail-closed as integer bit-test consumers and moving to the next materialize owner
  

### `0x140008090` arithmetic mask predicate proof tracing

This wave stayed diagnostic-only. It does not widen predicate replacement, change stable-representative policy, or enable any new env-gated path. The goal was to take the dominant `DisallowedSingleConsumer -> ConsumerIsPredicate -> UnknownPredicate` slice on `0x140008090` and separate arithmetic bit-mask predicate shapes from the remaining classifier blind spot.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries arithmetic-predicate proof vocabulary:
  - `ArithmeticPredicateShape`
    - `LowBitAndOne`
    - `PowerOfTwoMask`
    - `NonPowerOfTwoMask`
    - `ShiftAndMask`
    - `UnknownArithmetic`
  - `ArithmeticPredicateStableReason`
    - `PredicateSensitive`
    - `ArithmeticMask`
    - `ConsumerCompare`
    - `NonCanonicalPredicate`
  - `ArithmeticPredicateProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `arithmetic_predicate_shape`
    - `arithmetic_predicate_consumer_guard`
    - `arithmetic_predicate_boolean_width`
    - `arithmetic_predicate_stable_reason`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `describe_arithmetic_predicate_proof(...)`
  - arithmetic mask-shape classification for `BitAnd`-based rhs
  - boolean-width detection for low-bit extraction shapes
  - stable-representative reason mapping for arithmetic predicate consumers
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `arithmetic-predicate-proof output=... rhs=... mask_kind=... mask_value=... consumer_guard=... boolean_width=... low_cost=... stable_required=... stable_required_reason=...`
  - summary families for:
    - `arithmetic_predicate_shape`
    - `arithmetic_predicate_consumer_guard`
    - `arithmetic_predicate_boolean_width`
    - `arithmetic_predicate_stable_reason`

Validation:

- `cargo test -p fission-pcode arithmetic_predicate_proof_ --lib -- --test-threads=1`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `single_consumer_predicate_family`
  - `UnknownPredicate=468`
  - `CompareZero=51`
  - `DirectFlag=22`
  - `ComposedPredicate=17`
  - `CompareOtherVar=9`
- `single_consumer_predicate_guard_family`
  - `CompareZero=443`
  - `CompareOtherVar=48`
  - `NegatedFlag=42`
  - `ComposedPredicate=18`
  - `CompareNonZero=16`
- `single_consumer_predicate_same_guard`
  - `false=567`
- `single_consumer_predicate_requires_stable`
  - `true=536`
  - `false=31`
- `arithmetic_predicate_shape`
  - `LowBitAndOne=459`
  - `UnknownArithmetic=9`
- `arithmetic_predicate_consumer_guard`
  - `CompareZero=443`
  - `CompareNonZero=16`
  - `CompareOtherVar=9`
- `arithmetic_predicate_boolean_width`
  - `true=468`
- `arithmetic_predicate_stable_reason`
  - `ArithmeticMask=459`

Conclusion:

- the dominant former `UnknownPredicate` slice is no longer opaque:
  - almost all of it is now `(... & 1)`-style low-bit extraction
  - almost all of it feeds `CompareZero` predicate consumers
  - almost all of it still requires a stable representative for arithmetic-mask reasons
- this remains a proof/classifier refinement wave, not an acceptance wave
- the next owner is now narrower:
  - either predicate-family normalization for low-bit arithmetic masks, or
  - explicit proof for why this family must remain stable-representative and not be inlined

## 2026-04-19

### `0x140008090` single-consumer predicate proof tracing

This wave stayed diagnostic-only. It does not widen predicate inlining, alter stable-representative policy, or enable any new env-gated replacement path. The goal was to take the already isolated `DisallowedSingleConsumer -> ConsumerIsPredicate` slice on `0x140008090` and partition it by predicate-family/guard-family proof instead of treating the whole bucket as a single future policy target.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries a dedicated predicate-only vocabulary for this builder family:
  - `SingleConsumerPredicateFamily`
  - `SingleConsumerPredicateProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `single_consumer_predicate_family`
    - `single_consumer_predicate_guard_family`
    - `single_consumer_predicate_same_guard`
    - `single_consumer_predicate_requires_stable`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `describe_single_consumer_predicate_proof(...)`
  - predicate-side family classification from the producer rhs
  - consumer-side guard-family classification from the predicate consumer op
  - `same_guard_as_consumer`
  - `requires_stable_representative`
  - `low_cost_if_predicate`
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `single-consumer-predicate-proof output=... def_block=... def_op_seq=... consumer_block=... consumer_op_seq=... consumer_opcode=... rhs_kind=... rhs=... predicate_family=... guard_family=... same_guard_as_consumer=... requires_stable_representative=... low_cost_if_predicate=... has_call=false has_load=false`
  - summary families for:
    - `single_consumer_predicate_family`
    - `single_consumer_predicate_guard_family`
    - `single_consumer_predicate_same_guard`
    - `single_consumer_predicate_requires_stable`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode single_consumer_predicate_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `disallowed_single_consumer_reason`
  - `ConsumerIsPredicate=567`
  - `RhsHasCall=479`
  - `RhsHasLoad=64`
  - `UnknownConsumerKind=443`
- `single_consumer_predicate_family`
  - `UnknownPredicate=468`
  - `CompareZero=51`
  - `DirectFlag=22`
  - `ComposedPredicate=17`
  - `CompareOtherVar=9`
- `single_consumer_predicate_guard_family`
  - `CompareZero=443`
  - `CompareOtherVar=48`
  - `NegatedFlag=42`
  - `ComposedPredicate=18`
  - `CompareNonZero=16`
- `single_consumer_predicate_same_guard`
  - `false=567`
- `single_consumer_predicate_requires_stable`
  - `true=536`
  - `false=31`

Conclusion:

- the live predicate slice does not currently expose a same-guard narrow acceptance candidate
- the dominant real shape is now clear:
  - `IntEqual(output, 0)` consumers over arithmetic `(... & 1)`-style rhs
  - classified today as `predicate_family=UnknownPredicate`, `guard_family=CompareZero`
  - almost always still requiring a stable representative
- the next owner is therefore not a broad predicate-restart policy but either:
  - refining predicate-family classification for these arithmetic flag-mask rhs shapes, or
  - explicitly proving why they must remain stable-representative consumers

### `0x140008090` `DisallowedSingleConsumer` proof subtyping

This wave stayed diagnostic-only. It does not widen same-block replacement, change representative stability policy, or alter env-gated materialization experiments. The goal was to take the now-isolated `DisallowedSingleConsumer=1553` slice on `0x140008090` and partition it by actual consumer/rhs proof instead of leaving it as a single alias-unsafe bucket.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries dedicated diagnostic-only proof vocabulary for this family:
  - `DisallowedSingleConsumerConsumerKind`
  - `DisallowedSingleConsumerRhsKind`
  - `DisallowedSingleConsumerReason`
  - `DisallowedSingleConsumerProof`
  - `MaterializeOwnerRepartition` now also tracks:
    - `disallowed_single_consumer_reason`
    - `disallowed_single_consumer_consumer_kind`
    - `disallowed_single_consumer_rhs_kind`
- [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs) now exposes:
  - `describe_disallowed_single_consumer_proof(...)`
  - proof construction stays local to the existing same-block single-consumer hazard:
    - consumer opcode/input-position classification
    - rhs shape classification
    - low-cost/load/call proof bits
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now:
  - emits:
    - `disallowed-single-consumer output=... def_block=... def_op_seq=... consumer_block=... consumer_op_seq=... consumer_opcode=... consumer_kind=... rhs_kind=... rhs_low_cost=... rhs_has_load=... rhs_has_call=... reason=...`
  - records per-function summary families for:
    - `disallowed_single_consumer_reason`
    - `disallowed_single_consumer_consumer_kind`
    - `disallowed_single_consumer_rhs_kind`

Validation:

- `cargo fmt --all`
- `cargo test -p fission-pcode disallowed_single_consumer_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `disallowed_single_consumer_reason`
  - `ConsumerIsPredicate=567`
  - `RhsHasCall=479`
  - `UnknownConsumerKind=443`
  - `RhsHasLoad=64`
- `disallowed_single_consumer_consumer_kind`
  - `Predicate=603`
  - `UnknownConsumerKind=500`
  - `OtherData=450`
- `disallowed_single_consumer_rhs_kind`
  - `Arithmetic=902`
  - `CallLike=479`
  - `BinaryBoolean=77`
  - `LoadLike=64`
  - `VarOrConst=31`

Conclusion:

- this wave intentionally narrows the next owner from broad `DisallowedSingleConsumer` to concrete consumer/rhs proof buckets
- the dominant live slices are now clearly:
  - single predicate consumers over arithmetic/bool rhs
  - call-bearing rhs that should stay fail-closed
  - a smaller unknown-consumer bucket that needs its own proof
- no policy decision should be made from the old aggregate count alone anymore
- any next release-safe candidate should target only one of those now-separated slices instead of the whole family

### `0x140008090` materialize owner repartition tracing

This wave stayed diagnostic-only. It does not widen materialization policy, change representative stability rules, or add new `NirBuildStats` fields. The goal was to re-partition the live `0x140008090` builder/materialize owner after the `materialize` module split and after the `0x140006c20` loop-boundary path had already been narrowed into a larger modeling problem.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries a diagnostic-only per-function summary container:
  - `MaterializeOwnerRepartition`
- [`state.rs`](../../crates/fission-pcode/src/nir/builder/state.rs) now keeps that repartition state on `PreviewBuilder`
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now:
  - records owner-family counts while existing `EMIT-TRACE` diagnostics fire
  - emits end-of-build summary lines in the form:
    - `materialize-owner-repartition family=... values=[...]`
- [`mod.rs`](../../crates/fission-pcode/src/nir/builder/materialize/mod.rs) now records active builder rejection-family counts for:
  - `AliasUnsafe`
  - `MissingMergeBinding`
  - `ConsumerRequiresStableRepresentative`
- [`builder/mod.rs`](../../crates/fission-pcode/src/nir/builder/mod.rs) now flushes the summary once per traced function after HIR body construction

Validation:

- `cargo fmt --all`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`

Observed state on `0x140008090`:

- `alias_unsafe_hazard_kind`
  - `UnknownNoConsumerFound=2137`
  - `UnknownMalformedDefUseWindow=1480`
  - `DisallowedSingleConsumer=1553`
  - `MultipleSameBlockConsumers=581`
  - `SameBlockStore=49`
  - `UnknownUnhandledConsumerKind=7`
- `materialization_rejection_reason`
  - `AliasUnsafe=5807`
  - `MissingMergeBinding=1066`
  - `ConsumerRequiresStableRepresentative=960`
- `malformed_def_use_window_relation`
  - `ConsumerInDifferentBlock=1089`
  - `RedefinitionBeforeConsumer=293`
  - `TerminatorMissing=98`
- `cross_block_consumer_relation`
  - `LoopBackedge=508`
  - `OrdinaryDataConsumer=303`
  - `JoinBlock=246`
  - `SuccessorBlock=32`
- `cross_block_redefinition_relation`
  - `RedefinedInDefBlockAfterDef=1089`
- `same_block_overwrite_shape_kind`
  - `OverwriteAtLoopUpdate=508`
  - `OverwriteAtPredicateProducer=222`
  - `OverwriteAtCopy=190`
  - `OverwriteBeforeBranch=169`
- `loop_carried_value_kind`
  - `BooleanFlag=406`
  - `UnknownLoopCarried=102`
- `loop_boolean_guard_family`
  - `DirectFlag=210`
  - `NonPredicate=196`

Conclusion:

- `0x140008090` is not dominated by the same narrow loop-header guard-refresh family that previously drove `0x140006c20`
- the live primary owners are now clearly partitioned across:
  - `AliasUnsafe`
  - malformed cross-block def/use windows
  - loop-backedge overwrite families
  - merge/stable-representative rejection paths
- the next policy wave should target one of those now-isolated families instead of broad builder-wide retunes

### Loop-boundary missing binding correlation tracing

This wave stayed diagnostic-only. It does not synthesize loop-boundary bindings, widen loop-carried replacement, or change stable-representative policy. The goal was to test whether the remaining `LoopBackedge x OverwriteAtLoopUpdate` boolean family on `0x140006c20` actually overlaps with the builder's active `MissingMergeBinding` or `ConsumerRequiresStableRepresentative` rejection path.

- [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs) now carries a dedicated loop-boundary correlation contract:
  - `LoopBoundaryBindingFamily`
  - `LoopBoundaryBindingCorrelation`
- [`loop_carried.rs`](../../crates/fission-pcode/src/nir/builder/materialize/loop_carried.rs) now exposes:
  - `describe_loop_boundary_binding_correlation(...)`
  - correlation is only produced when the active output already proves as:
    - `CrossBlockConsumerRelation::LoopBackedge`
    - `LoopCarriedValueKind::BooleanFlag`
  - the traced family stays intentionally narrow:
    - `BoolNegate`
    - `IntNotEqual`
    - `OtherBooleanFlag`
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `loop-boundary-binding-correlation output=... loop_header=... family=... missing_merge_binding=... stable_representative_required=... merge_block=... candidate_binding=... existing_binding=...`
- [`mod.rs`](../../crates/fission-pcode/src/nir/builder/materialize/mod.rs) now wires that trace only at the existing active rejection sites:
  - `MissingMergeBinding`
  - `ConsumerRequiresStableRepresentative`

Validation:

- `cargo fmt --all`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the live `0x140006c20` row still emits the previously added loop diagnostics:
  - `loop-boolean-flag-proof`
  - `loop-guard-refresh-dominance`
- the new `loop-boundary-binding-correlation` trace does not appear on the live row
- for the current active builder path, the remaining loop boolean slice is therefore not currently closing through:
  - `MissingMergeBinding`
  - `ConsumerRequiresStableRepresentative`

Conclusion:

- the next owner is not a local representative restart and not yet a directly proven missing-merge path on the current live row
- the loop boolean family still behaves like a broader loop-boundary ownership problem
- any future binding work should be designed as explicit loop/merge-boundary synthesis, not inferred from the current local restart proofs

### Loop guard refresh dominance proof tracing

This wave stayed diagnostic-only. It does not widen loop-carried replacement, add a loop-guard restart policy, or change merge-boundary handling. The goal was to take the already isolated `BoolNegate + same_guard_as_exit=true` slice from `0x140006c20` and explain why the existing builder still reports `redef_dominates_backedge=false`.

- [`loop_carried.rs`](../../crates/fission-pcode/src/nir/builder/materialize/loop_carried.rs) now exposes a dedicated dominance proof for the loop-header guard-refresh slice:
  - `LoopGuardRefreshDominanceReason`
  - `LoopGuardRefreshDominanceProof`
  - `describe_loop_guard_refresh_dominance_proof(...)`
- [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs) now emits:
  - `loop-guard-refresh-dominance loop_header=... def_block=... redef_block=... redef_op_seq=... backedge_block=... backedge_edge=... exit_edge=... redef_before_backedge_branch=... all_backedge_paths_covered=... header_predicate_uses_redef=... reason=...`
- the proof stays intentionally narrow:
  - it only fires for `loop-boolean-flag-proof` cases where `same_guard_as_exit=true` and `consumer_is_loop_header_predicate=true`
  - it distinguishes:
    - `ProvedBySingleBackedge`
    - `RedefAfterBackedgeBranch`
    - `RedefInNonBackedgeBlock`
    - `MultipleBackedgeBlocks`
    - `HeaderPredicateUsesIntermediate`
    - `MissingBackedgeTerminator`
    - `UnknownDominance`
- synthetic unit coverage now pins:
  - a single-backedge proved case
  - a multiple-backedge rejection case

Validation:

- `cargo test -p fission-pcode loop_boolean_flag_proof_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode loop_guard_refresh_dominance_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the `BoolNegate` loop-header guard slice on `0x140006c20` now emits both:
  - `loop-boolean-flag-proof`
  - `loop-guard-refresh-dominance`
- the live `BoolNegate` slice does not currently look like a missing local dominance refinement
- it repeatedly closes to:
  - `reason=RedefInNonBackedgeBlock`
  - `redef_before_backedge_branch=false`
  - `all_backedge_paths_covered=false`
  - `header_predicate_uses_redef=true`

Conclusion:

- the next owner is not generic loop guard restart
- for the current live row, the `BoolNegate` slice behaves like a non-backedge redef / loop-boundary ownership issue, not a simple local guard-refresh dominance miss
- any future policy experiment should stay behind an env gate and only happen after backedge ownership is tightened further

### `materialize.rs` thin-façade module split

This wave is a behavior-preserving refactor. It does not widen replacement/materialization policy, change telemetry, or retune any benchmark gate. The goal was to stop growing one multi-owner builder file and split the existing logic by semantic owner while keeping the external `builder::materialize` module path stable.

- the old flat [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) implementation was converted into the folder-backed module [`materialize/mod.rs`](../../crates/fission-pcode/src/nir/builder/materialize/mod.rs)
- the new layout keeps `mod.rs` as a thin orchestration façade and moves helper families into focused siblings:
  - [`contracts.rs`](../../crates/fission-pcode/src/nir/builder/materialize/contracts.rs)
  - [`trace.rs`](../../crates/fission-pcode/src/nir/builder/materialize/trace.rs)
  - [`same_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/same_block.rs)
  - [`no_consumer.rs`](../../crates/fission-pcode/src/nir/builder/materialize/no_consumer.rs)
  - [`cross_block.rs`](../../crates/fission-pcode/src/nir/builder/materialize/cross_block.rs)
  - [`loop_carried.rs`](../../crates/fission-pcode/src/nir/builder/materialize/loop_carried.rs)
  - [`scans.rs`](../../crates/fission-pcode/src/nir/builder/materialize/scans.rs)
- shared unit-test scaffolding that was previously trapped in the monolithic file now lives in [`test_support.rs`](../../crates/fission-pcode/src/nir/builder/materialize/test_support.rs), and owner tests were moved into the owning modules instead of staying in one giant local test block
- ownership boundaries are now explicit:
  - `contracts.rs` owns shared internal vocabulary
  - `trace.rs` owns `EMIT-TRACE` formatting only
  - `same_block.rs`, `no_consumer.rs`, `cross_block.rs`, and `loop_carried.rs` own their semantic proof/classifier families
  - `scans.rs` owns low-level def/use scanning helpers
- default behavior remains unchanged:
  - existing env gates are still the only active switches:
    - `FISSION_ENABLE_NO_CONSUMER_SUPPRESSION`
    - `FISSION_ENABLE_COPY_OVERWRITE_RESTART`
    - `FISSION_ENABLE_PREDICATE_REFRESH_RESTART`
  - no new trace vocabulary or `NirBuildStats` fields were introduced

Validation:

- `cargo test -p fission-pcode loop_carried_overwrite_provenance_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode loop_boolean_flag_proof_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode predicate_overwrite_refresh_proof_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode predicate_refresh_restart_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode def_window_restart_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- targeted live trace parity checks on:
  - `0x140006c20`
  - `0x140008090`
  - `0x140006fe0`

Observed state:

- the `materialize` split preserved the existing trace families on the live rows:
  - `0x140006c20` still surfaces `loop-carried-overwrite`, `loop-boolean-flag-proof`, and `predicate-overwrite-proof`
  - `0x140008090` still surfaces `alias-unsafe-shape`, malformed def/use, and no-consumer diagnostics
  - `0x140006fe0` still surfaces the pre-existing guarded-tail/materialization diagnostics
- `cargo test -p fission-pcode` remains red only in the already-known guarded-tail family; the split did not introduce a new materialize-side regression family

Conclusion:

- `builder::materialize` now has explicit internal owners without changing release behavior
- future waves can target one family at a time without re-expanding a monolithic file
- the guarded-tail red suite remains a separate known issue and was not altered by this refactor
- no release policy changed in this wave

### Loop boolean flag ownership proof tracing

This wave stayed diagnostic-only. It does not widen loop-carried replacement, synthesize loop bindings, or relax merge-boundary handling. The goal was to take the already-isolated `LoopBackedge x OverwriteAtLoopUpdate = 156` boolean slice on `0x140006c20` and prove whether those values behave like loop exit/latch guards or true carried loop state.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated boolean loop-carried proof trace on top of the existing `loop-carried-overwrite` signal:
  - `loop-boolean-flag-proof output=... loop_header=... def_block=... def_op_seq=... redef_op_seq=... redef_rhs=... consumer_block=... consumer_op_seq=... consumer_opcode=... exit_edge=... backedge_edge=... guard_family=... same_guard_as_exit=... old_def_has_pre_redef_use=... redef_dominates_backedge=... consumer_is_loop_header_predicate=...`
- the builder-local proof stays intentionally narrow:
  - only `LoopCarriedValueKind::BooleanFlag` cases participate
  - it records the loop-header consumer opcode and whether it feeds the header terminator predicate
  - it separates the loop header's exit edge and backedge edge using CFG reachability from the header successors
  - it classifies the consumer-side boolean family as:
    - `DirectFlag`
    - `NegatedFlag`
    - `EqZero`
    - `NeZero`
    - `NonPredicate`
- synthetic unit coverage now pins both sides of the split:
  - same-guard loop-header predicate refresh (`BoolNegate -> CBranch`)
  - non-predicate carried-state style header use (`Copy` with unrelated loop branch)

Validation:

- `cargo test -p fission-pcode loop_boolean_flag_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the live `0x140006c20` boolean loop-carried family no longer appears monolithic
- after deduplicating repeated trace emissions, the row splits into two concrete header families:
  - `loop_header=0x140006c40`
    - `consumer_opcode=BoolNegate`
    - `guard_family=NegatedFlag`
    - `same_guard_as_exit=true`
    - `consumer_is_loop_header_predicate=true`
    - `5` unique sites
  - `loop_header=0x140006c55`
    - `consumer_opcode=IntNotEqual`
    - `guard_family=NonPredicate`
    - `same_guard_as_exit=false`
    - `consumer_is_loop_header_predicate=true`
    - `8` unique sites
- all observed sites still share the same conservative facts:
  - `old_def_has_pre_redef_use=false`
  - `redef_dominates_backedge=false`

Conclusion:

- the remaining `0x140006c20` loop-carried boolean owner is not one uniform class
- one slice now looks like loop-header guard refresh (`BoolNegate`, same-guard-as-exit)
- the other slice looks like boolean composition/update (`IntNotEqual(..., other_pred)`) rather than a direct guard refresh
- the next owner should be narrower than generic loop-carried binding synthesis:
  - loop guard representative handling for the `BoolNegate` slice
  - separate merge-boundary / composed-predicate treatment for the `IntNotEqual` slice
- no release policy changed in this wave

### Loop-carried overwrite binding provenance

This wave stayed diagnostic-only. It does not widen cross-block replacement, synthesize merge bindings, or restart def windows for loop-carried values. The goal was to isolate the remaining `LoopBackedge x OverwriteAtLoopUpdate = 156` slice on `0x140006c20` and prove what kind of carried value it actually is before any merge-boundary policy work.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated loop-carried overwrite trace when a cross-block consumer falls through `LoopCarriedRedefinition + OverwriteAtLoopUpdate`:
  - `loop-carried-overwrite output=... def_block=... def_op_seq=... redef_op_seq=... redef_rhs=... loop_header=... backedge_block=... consumer_block=... consumer_op_seq=... has_multiequal=... phi_input_count=... induction_like=... carried_value_kind=...`
- the provenance is intentionally builder-local and diagnostic:
  - it reads the redef op from the actual backedge/redef block rather than the original def block
  - it reports whether the loop header block contains any `MULTIEQUAL`
  - it classifies the carried value into:
    - `BooleanFlag`
    - `CounterIncrement`
    - `PointerAdvance`
    - `Accumulator`
    - `UnknownLoopCarried`
- unit coverage now pins both ends of the classifier:
  - boolean loop-carried refresh without `MULTIEQUAL`
  - pointer-advance loop-carried refresh with unrelated header `MULTIEQUAL`

Validation:

- `cargo test -p fission-pcode loop_carried_overwrite_provenance_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the targeted `0x140006c20` trace no longer treats the loop-carried slice as a generic cross-block bucket
- all `156` observed `LoopBackedge x OverwriteAtLoopUpdate` cases currently collapse into the same live family:
  - `carried_value_kind = BooleanFlag`
  - `has_multiequal = false`
  - `phi_input_count = 0`
  - `induction_like = false`
- the live loop headers only split across two addresses:
  - `0x140006c55` (`96` cases)
  - `0x140006c40` (`60` cases)
- representative live examples look like boolean refreshes rather than counter/pointer updates:
  - `redef_rhs=[space:3:0xa8600018:s8,const(0x1:s8)]`
  - `redef_rhs=[space:3:0xa8600000:s1,const(0x0:s1)]`

Conclusion:

- the remaining large `0x140006c20` owner is not broad loop arithmetic
- it is a loop-carried boolean flag family across two loop headers
- the next owner is merge/loop-boundary boolean representative design, not direct cross-block propagation
- no release policy changed in this wave

### Same-guard predicate refresh restart env-gated trial

This wave added a narrow, opt-in restart trial for the `OverwriteAtPredicateProducer` slice. It does not change the default release path. The trial only targets the already-proved `PostDominatorBlock + BoolNegate + same_guard_family=true` family on `0x140006c20`, while leaving `SuccessorBlock + IntNotEqual(output, other_pred)` fail-closed.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now exposes:
  - `FISSION_ENABLE_PREDICATE_REFRESH_RESTART=1|true|yes`
- the active trial path is intentionally narrow:
  - `relation = PostDominatorBlock`
  - `overwrite_shape = OverwriteAtPredicateProducer`
  - `same_guard_family = true`
  - `old_def_has_pre_redef_use = false`
  - `redef_dominates_predicate = true`
  - predicate consumer opcode must be `BoolNegate`
  - no `Call`/`CallInd`/`CallOther`/`Store`/`Load` between redef and terminator
- when enabled, the builder emits:
  - `def-window-restarted-at-predicate-refresh ...`
- the broader diagnostic trace remains active regardless:
  - `predicate-overwrite-proof ...`

Validation:

- `cargo test -p fission-pcode predicate_refresh_restart_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode predicate_overwrite_refresh_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_ENABLE_PREDICATE_REFRESH_RESTART=1 FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`
- `FISSION_ENABLE_PREDICATE_REFRESH_RESTART=1 python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --ghidra-dir vendor/ghidra/ghidra-Ghidra_11.4.2_build --fission-bin target/debug/fission_cli --output-dir artifacts/batch_benchmark/putty-predicate-refresh-restart-limit50 --baseline-dir artifacts/batch_benchmark/putty-builder-provenance-wave --limit 50 --pairwise-similarity-mode shared-full --ghidra-cache-dir artifacts/ghidra_cache_copy_overwrite --use-ghidra-cache`

Observed state:

- the targeted trace confirms the trial hits only the intended same-guard family:
  - `def-window-restarted-at-predicate-refresh` fired `12` times on `0x140006c20`
  - the `PostDominatorBlock + BoolNegate` cases restart
  - the `SuccessorBlock + IntNotEqual(output, other_pred)` cases remain trace-only
- the same-axis `putty limit50` run did not produce a quality gain:
  - `avg_normalized_similarity` stayed at `38.74`
  - row gate remained failed for:
    - `0x140008090`
    - `0x140006c20`
    - `0x140006fe0`
  - row deltas stayed effectively unchanged from the default path:
    - `0x140006c20: 40.52 -> 40.40`
    - `0x140008090: 35.63 -> 35.28`
    - `0x140006fe0: 34.76 -> 33.97`

Conclusion:

- the same-guard predicate refresh family is now isolated well enough to trial safely
- but the current restart rule is not yet release-positive
- it remains an env-gated experiment only
- the next owner is likely not generic predicate refresh anymore, but the narrower distinction between:
  - postdominating boolean negation refresh that is semantically neutral
  - and broader predicate composition/update families that still need stable representatives

### Predicate overwrite refresh proof tracing

This wave stayed diagnostic-only. It did not widen cross-block replacement, relax stable-representative requirements, or re-enable the copy-overwrite restart path. The goal was to isolate the smaller `OverwriteAtPredicateProducer = 24` slice on `0x140006c20` and prove whether it behaves like a same-guard refresh family or a real predicate update family.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated predicate-overwrite trace when a cross-block consumer falls through `RedefinedInDefBlockAfterDef + OverwriteAtPredicateProducer`:
  - `predicate-overwrite-proof output=... def_op_seq=... redef_op_seq=... redef_rhs=... predicate_consumer_block=... predicate_consumer_op_seq=... predicate_rhs=... same_guard_family=... old_def_has_pre_redef_use=... redef_dominates_predicate=... consumer_relation=...`
- the proof is intentionally narrow and builder-local:
  - it only fires for `OverwriteAtPredicateProducer`
  - it records whether the redefinition still dominates the predicate consumer
  - it records whether the old definition had any pre-redef use
  - it classifies the consumer as same-guard-family only when the consumer is a trivial booleanized use of the refreshed output (`BoolNegate`, `IntEqual/IntNotEqual` with `0/1`, or branch-on-output)
- unit coverage now pins both sides of the classification:
  - a `BoolNegate` consumer is treated as same-guard-family
  - a plain `Copy` consumer is not

Validation:

- `cargo test -p fission-pcode predicate_overwrite_refresh_proof_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the targeted `0x140006c20` predicate overwrite slice stays split into the same two live shapes already implied by the cross-block provenance trace:
  - one `PostDominatorBlock` consumer with `consumer_opcode=BoolNegate`
  - one `SuccessorBlock` consumer with `consumer_opcode=IntNotEqual`
- the proof split is now explicit instead of inferred:
  - the `PostDominatorBlock + BoolNegate` family records `same_guard_family=true`
  - the `SuccessorBlock + IntNotEqual(output, other_pred)` family records `same_guard_family=false`
- both live families currently still record:
  - `old_def_has_pre_redef_use=false`
  - `redef_dominates_predicate=true`
- both cases are now explicit predicate-proof targets instead of being buried under the broader overwrite histogram
- this is enough to decide the next owner based on proof distribution rather than on overwrite shape names alone

Conclusion:

- `OverwriteAtPredicateProducer = 24` is now a real proof family instead of a coarse histogram bucket
- the next algorithmic decision is whether the observed predicate overwrite cases are mostly:
  - same-guard refresh / boolean canonicalization
  - or true predicate state updates that still require stable representatives
- `OverwriteAtLoopUpdate = 156` remains out of scope for this wave and continues to belong to the loop-carried / merge-boundary owner

### Copy overwrite def-window restart rollback to env-gated experiment

This wave did implement the narrow `OverwriteAtCopy = 12` restart policy on `0x140006c20`, but it did not hold up against the same-axis `putty limit50` quality gate. The policy now remains available only as an opt-in experiment while the proof and trace infrastructure stay in the default path.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) still carries the narrow restart proof and trace vocabulary:
  - `overwrite-copy-proof ...`
  - `def-window-restarted-at-copy-overwrite ...`
- the active restart policy is now gated behind:
  - `FISSION_ENABLE_COPY_OVERWRITE_RESTART=1|true|yes`
- default release behavior no longer restarts the replacement window at copy overwrite
- this keeps the mechanical proof available for future narrowing, while removing the release-path regression risk

Validation:

- `cargo test -p fission-pcode def_window_restart_ --lib -- --test-threads=1`
- `cargo test -p fission-pcode copy_overwrite_restart_proof_marks_same_value_and_no_pre_redef_use --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --ghidra-dir vendor/ghidra/ghidra-Ghidra_11.4.2_build --fission-bin target/debug/fission_cli --output-dir artifacts/batch_benchmark/putty-copy-overwrite-restart-env-default-limit50 --baseline-dir artifacts/batch_benchmark/putty-builder-provenance-wave --limit 50 --pairwise-similarity-mode shared-full --ghidra-cache-dir artifacts/ghidra_cache_copy_overwrite --use-ghidra-cache`

Observed state:

- with the restart policy enabled, the targeted `OverwriteAtCopy` slice did fire and downgraded the old def in all 12 observed cases on `0x140006c20`
- but the same-axis `putty limit50` run did not improve release quality:
  - `avg_normalized_similarity: 38.82 -> 38.74`
  - row gate remained failed for:
    - `0x140008090`
    - `0x140006c20`
    - `0x140006fe0`
  - `0x140006c20` specifically moved from `40.52` to `40.40`
- because the patch was mechanically correct but not release-positive, the restart path is now treated like the earlier broad no-consumer suppression experiment:
  - proof kept
  - trace kept
  - default-off in release path

Conclusion:

- `OverwriteAtCopy = 12` is still a real, narrow proof family, but not yet a release-safe active policy
- the next owner remains:
  - `OverwriteAtPredicateProducer = 24` for predicate refresh
  - `OverwriteAtLoopUpdate = 156` for loop-carried / merge-boundary handling
- any future return to copy-overwrite restart should be driven by a tighter acceptance slice, not by re-enabling the current broad `OverwriteAtCopy` path in default builds

### Copy overwrite def-window restart tracing

This wave stayed diagnostic-only. It did not restart replacement windows, relax malformed def/use handling, or widen cross-block replacement. The goal was to prove whether the narrow `OverwriteAtCopy = 12` slice on `0x140006c20` is a real def-window restart candidate or just another unsafe overwrite family.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated trace for the `OverwriteAtCopy` subset:
  - `overwrite-copy-proof output=... def_op_seq=... redef_op_seq=... redef_rhs=... consumer_block=... consumer_op_seq=... same_value=... redef_dominates_consumer=... old_def_has_pre_redef_use=...`
- the proof is intentionally narrow and builder-local:
  - it only fires for `RedefinedInDefBlockAfterDef + OverwriteAtCopy`
  - it checks whether the redef is a copylike equivalent representative, whether the redef still dominates the cross-block consumer, and whether the original def had any use before the overwrite
- unit coverage now fixes the basic synthetic restart shape:
  - a copy overwrite with no pre-redef use and a downstream cross-block consumer produces `same_value=true`, `redef_dominates_consumer=true`, `old_def_has_pre_redef_use=false`

Validation:

- `cargo test -p fission-pcode copy_overwrite_restart_proof_marks_same_value_and_no_pre_redef_use --lib -- --test-threads=1`
- `cargo test -p fission-pcode cross_block_redefinition_marks_def_block_after_def --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the `OverwriteAtCopy` family is no longer just "copy-shaped". On the targeted row it is fully uniform:
  - `count = 12`
  - `same_value = true` for all 12
  - `redef_dominates_consumer = true` for all 12
  - `old_def_has_pre_redef_use = false` for all 12
- the concrete live shape is stable across every observed sample:
  - `def_block = 0x140006c40`
  - `def_op_seq = 7`
  - `redef_op_seq = 25`
  - `redef_rhs = [const(0x0:s1)]`
  - `consumer_block = 0x140006c55`
  - `consumer_op_seq = 13`
- this means the entire `OverwriteAtCopy` slice on `0x140006c20` is currently behaving like a restartable shadowed-def family, not like a loop-carried or merge-boundary value family

Conclusion:

- `OverwriteAtCopy = 12` is now the narrowest credible release-safe policy target on `0x140006c20`
- the next algorithmic step is no longer more tracing for this slice
- the likely next patch is a narrow def-window restart policy for pure copy overwrite, while:
  - `OverwriteAtPredicateProducer = 24` remains a predicate refresh owner
  - `OverwriteAtLoopUpdate = 156` remains a loop-carried / merge-boundary owner

### Same-block overwrite window refinement

This wave stayed diagnostic-only. It did not widen cross-block replacement, relax alias safety, or change the active materialization policy. The goal was to split the now-dominant `RedefinedInDefBlockAfterDef` family into concrete same-block overwrite shapes so `0x140006c20` could be judged on a real def-window owner instead of a single redefinition label.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now extends the existing `cross-block-redefinition` trace with overwrite-local detail:
  - `def_op_seq`
  - `redef_opcode`
  - `redef_rhs_kind`
  - `overwrite_shape`
  - `terminator_idx`
  - `def_to_redef_gap`
  - `redef_to_terminator_gap`
- same-block overwrite shape is now classified into a deterministic internal vocabulary:
  - `OverwriteBeforeBranch`
  - `OverwriteAtPredicateProducer`
  - `OverwriteAtLoopUpdate`
  - `OverwriteAtCallResult`
  - `OverwriteAtLoadResult`
  - `OverwriteAtCopy`
  - `OverwriteUnknown`
- overwrite RHS kind is now surfaced alongside the shape:
  - `CopyLike`
  - `Predicate`
  - `Arithmetic`
  - `Load`
  - `Call`
  - `Unknown`

Validation:

- `cargo test -p fission-pcode cross_block_redefinition_marks_def_block_after_def --lib -- --test-threads=1`
- `cargo test -p fission-pcode cross_block_redefinition_marks_consumer_block_before_use --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140006c20` still resolves every live redefinition-backed cross-block case to the same owning family:
  - `RedefinedInDefBlockAfterDef = 192`
- but that family is no longer opaque. The same-block overwrite split is now explicit:
  - `OverwriteAtLoopUpdate = 156`
  - `OverwriteAtPredicateProducer = 24`
  - `OverwriteAtCopy = 12`
- the overwrite RHS split shows the same pattern from a second angle:
  - `Predicate = 132`
  - `Unknown = 48`
  - `CopyLike = 12`
- relation and overwrite shape now line up deterministically on the targeted row:
  - `LoopBackedge x OverwriteAtLoopUpdate = 156`
  - `SuccessorBlock x OverwriteAtPredicateProducer = 12`
  - `SuccessorBlock x OverwriteAtCopy = 12`
  - `PostDominatorBlock x OverwriteAtPredicateProducer = 12`
- a representative successor case is now concrete instead of generic:
  - `relation=SuccessorBlock`, `redef_opcode=Copy`, `redef_rhs_kind=CopyLike`, `overwrite_shape=OverwriteAtCopy`, `def_to_redef_gap=18`, `redef_to_terminator_gap=9`

Conclusion:

- `0x140006c20` is not primarily blocked by generic cross-block consumers anymore
- the dominant owner is same-block overwrite inside the defining block
- the next release owner is not broad cross-block replacement
- the likely next algorithmic split is:
  - loop-carried / merge-boundary ownership for `OverwriteAtLoopUpdate`
  - predicate refresh / guard-boundary ownership for `OverwriteAtPredicateProducer`
  - only after that, reconsider whether the small `OverwriteAtCopy` slice is a safe def-window refinement target

### Redefinition-aware cross-block provenance

This wave stayed diagnostic-only. It did not widen cross-block replacement, relax redefinition guards, or synthesize merge bindings. The goal was to split `no_redefinition_before_consumer=false` into deterministic provenance families so `0x140006c20` could be judged on a concrete redefinition owner instead of a single boolean.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now adds a dedicated redefinition trace below the existing proof trace:
  - `cross-block-redefinition output=... def_block=... consumer_block=... relation=... redef_block=... redef_op_seq=... redef_relation=... consumer_op_seq=...`
- the redefinition vocabulary is now explicit:
  - `RedefinedInDefBlockAfterDef`
  - `RedefinedOnEdge`
  - `RedefinedInConsumerBlockBeforeUse`
  - `RedefinedInSiblingPredecessor`
  - `PhiRedefinition`
  - `LoopCarriedRedefinition`
  - `UnknownRedefinition`
- unit coverage now pins the two lowest-risk synthetic owners:
  - def-block post-definition redefinition
  - consumer-block pre-use redefinition

Validation:

- `cargo test -p fission-pcode cross_block_redefinition_marks_def_block_after_def --lib -- --test-threads=1`
- `cargo test -p fission-pcode cross_block_redefinition_marks_consumer_block_before_use --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140006c20` no longer has an ambiguous redefinition owner:
  - `RedefinedInDefBlockAfterDef = 192`
  - no observed `RedefinedOnEdge`, `RedefinedInConsumerBlockBeforeUse`, `RedefinedInSiblingPredecessor`, `PhiRedefinition`, or `LoopCarriedRedefinition` cases in the targeted run
- the outer cross-block families stay the same:
  - `LoopBackedge = 156`
  - `SuccessorBlock = 24`
  - `PostDominatorBlock = 12`
- but every live redefinition-backed case resolves to a same-block overwrite inside the defining block:
  - `LoopBackedge x RedefinedInDefBlockAfterDef = 156`
  - `SuccessorBlock x RedefinedInDefBlockAfterDef = 24`
  - `PostDominatorBlock x RedefinedInDefBlockAfterDef = 12`
- representative examples on `0x140006c20` are concrete:
  - `relation=SuccessorBlock`, `redef_block=0x140006c40`, `redef_op_seq=25`, `redef_relation=RedefinedInDefBlockAfterDef`
  - `relation=PostDominatorBlock`, `redef_block=0x140006c40`, `redef_op_seq=26`, `redef_relation=RedefinedInDefBlockAfterDef`
  - `relation=SuccessorBlock`, `redef_block=0x140006c40`, `redef_op_seq=27`, `redef_relation=RedefinedInDefBlockAfterDef`

Conclusion:

- `0x140006c20` is not blocked by an edge/sibling/consumer-block ambiguity
- the next owner is not broad cross-block acceptance
- the real blocker is same-block overwrite inside the defining block, so the next algorithmic choice is:
  - redefinition-aware block splitting / def-window refinement for `SuccessorBlock` and `PostDominatorBlock`
  - or leaving these cases fail-closed and shifting focus to merge/loopback ownership elsewhere

### Cross-block replacement proof tracing

This wave stayed diagnostic-only. It did not enable cross-block propagation, synthesize merge bindings, or relax malformed def/use handling. The goal was to layer a narrow replacement-proof trace on top of the existing `ConsumerInDifferentBlock` provenance so `0x140006c20` could be judged as a real single-successor candidate or rejected on explicit evidence.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a second trace for cross-block malformed windows:
  - `cross-block-replacement-proof output=... def_block=... consumer_block=... relation=... def_successor_count=... consumer_predecessor_count=... dominates_consumer=... consumer_opcode=... rhs_low_cost=... preserve_materialization=... no_redefinition_before_consumer=... merge_phi=... narrow_candidate=...`
- the new proof layer is still consumer-only. It does not change the replacement plan or alias safety decision.
- unit coverage now pins the proof surface for two core fixtures:
  - `MergePhiConsumer` stays `narrow_candidate=false`
  - `SuccessorBlock` can be recognized as `narrow_candidate=true` in the synthetic single-successor case

Validation:

- `cargo test -p fission-pcode cross_block_consumer_provenance_prefers_merge_phi_consumer --lib -- --test-threads=1`
- `cargo test -p fission-pcode cross_block_consumer_provenance_marks_single_successor_data_consumer --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140006c20` still has the same relation histogram inside `ConsumerInDifferentBlock`:
  - `LoopBackedge = 156`
  - `SuccessorBlock = 24`
  - `PostDominatorBlock = 12`
- however, the new proof layer shows that all currently observed cross-block cases remain non-accepting:
  - `narrow_candidate=false = 192`
  - no live `narrow_candidate=true` sites were observed in the targeted trace
- the blocking proof fields are concrete, not heuristic:
  - `def_successor_count=2` on the live `SuccessorBlock` cases
  - `no_redefinition_before_consumer=false` on both `SuccessorBlock` and `PostDominatorBlock`
  - representative examples:
    - `consumer_opcode=IntNotEqual`, `relation=SuccessorBlock`, `rhs_low_cost=true`, `preserve_materialization=false`, `def_successor_count=2`, `no_redefinition_before_consumer=false`
    - `consumer_opcode=BoolNegate`, `relation=PostDominatorBlock`, `rhs_low_cost=true`, `preserve_materialization=true`, `def_successor_count=2`, `no_redefinition_before_consumer=false`

Conclusion:

- `0x140006c20` is cleaner than `0x140008090`, but it is still not a real single-successor replacement candidate on the current owner boundary
- the next owner is not broad cross-block propagation
- if this row is pursued next, the likely split is:
  - successor/postdom cases with redefinition-aware block splitting
  - merge/loopback cases under merge-binding / CFG-boundary ownership

### Cross-block consumer materialization provenance

This wave stayed diagnostic-only. It did not widen cross-block replacement, synthesize merge bindings, or relax alias safety. The goal was to split the dominant `ConsumerInDifferentBlock` slice of `UnknownMalformedDefUseWindow` into CFG-shaped provenance families so the next release wave can choose between single-successor replacement and merge/join handling on evidence instead of guesswork.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated cross-block trace whenever malformed def/use analysis resolves to `ConsumerInDifferentBlock`:
  - `cross-block-consumer output=... def_block=... consumer_block=... consumer_op_seq=... consumer_opcode=... relation=... def_successors=[...] def_successor_count=... consumer_predecessors=... consumer_is_multiequal=... immediate_successor=... consumer_is_join=... redefined_before_consumer=...`
- the provenance relation is now classified into a deterministic internal vocabulary:
  - `SuccessorBlock`
  - `JoinBlock`
  - `LoopBackedge`
  - `PostDominatorBlock`
  - `UnreachableOrUnclassified`
  - `MergePhiConsumer`
  - `OrdinaryDataConsumer`
- unit coverage now fixes two basic CFG families:
  - `MergePhiConsumer`
  - `SuccessorBlock`

Validation:

- `cargo test -p fission-pcode cross_block_consumer_provenance_prefers_merge_phi_consumer --lib -- --test-threads=1`
- `cargo test -p fission-pcode cross_block_consumer_provenance_marks_single_successor_data_consumer --lib -- --test-threads=1`
- `cargo test -p fission-pcode malformed_def_use_window_relation_marks_terminator_missing --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140008090` is not primarily a simple immediate-successor issue. The current cross-block distribution is:
  - `LoopBackedge = 508`
  - `OrdinaryDataConsumer = 303`
  - `JoinBlock = 246`
  - `SuccessorBlock = 32`
- the dominant consumer opcodes on `0x140008090` are:
  - `IntEqual = 424`
  - `IntZExt = 280`
  - `CBranch = 265`
  - `IntNotEqual = 112`
  - `CallInd = 8`
- concrete examples on `0x140008090` show three distinct owners already:
  - immediate successor predicate use:
    - `consumer_block=0x1400080e3`
    - `consumer_opcode=CBranch`
    - `relation=SuccessorBlock`
  - merge/join compare use:
    - `consumer_block=0x140008113`
    - `consumer_opcode=IntEqual`
    - `relation=JoinBlock`
  - non-successor data consumer:
    - `consumer_block=0x140008108`
    - `consumer_opcode=IntZExt`
    - `relation=OrdinaryDataConsumer`
- `0x140006c20` is materially simpler:
  - `LoopBackedge = 156`
  - `SuccessorBlock = 24`
  - `PostDominatorBlock = 12`
- the dominant consumer opcodes on `0x140006c20` are:
  - `IntNotEqual = 120`
  - `BoolNegate = 72`
- representative examples on `0x140006c20` are clean:
  - `consumer_block=0x140006c55`, `consumer_opcode=IntNotEqual`, `relation=SuccessorBlock`
  - `consumer_block=0x140006cad`, `consumer_opcode=BoolNegate`, `relation=PostDominatorBlock`

Conclusion:

- `ConsumerInDifferentBlock` is now clearly a mixed family, not one release owner
- `0x140008090` is dominated by loopback/join/ordinary cross-block consumers, so its next owner is not broad single-successor replacement
- `0x140006c20` is a better narrow candidate for a future single-successor or postdom-aware replacement experiment
- if the next wave wants maximum release leverage, it should likely split:
  - `JoinBlock`/`MergePhiConsumer` into merge-binding ownership
  - `SuccessorBlock`/`PostDominatorBlock` into narrow cross-block replacement ownership

### MalformedDefUseWindow invariant tracing

This wave stayed diagnostic-only. It did not relax alias safety, widen representative downgrade, or change the release path for `UnknownMalformedDefUseWindow`. The goal was to split that family into concrete def/use-window relations so the next policy wave can target a real owner instead of a catch-all label.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now adds a dedicated malformed-window trace:
  - `malformed-def-use-window output=... def_block=... def_op_seq=... def_op_idx=... terminator_idx=... consumer_count=... first_consumer_block=... first_consumer_idx=... first_consumer_op_seq=... relation=... rhs_kind=...`
- the trace classifies malformed windows into deterministic relations:
  - `DefAfterTerminator`
  - `ConsumerBeforeDef`
  - `ConsumerAfterTerminator`
  - `ConsumerInDifferentBlock`
  - `TerminatorMissing`
  - `OpIndexMissing`
  - `BlockMismatch`
  - `RedefinitionBeforeConsumer`
  - `UnknownWindow`
- the diagnostic path is builder-owned and only fires for the existing `AliasUnsafeHazardKind::UnknownMalformedDefUseWindow` family
- unit coverage now fixes three core relation cases:
  - `TerminatorMissing`
  - `ConsumerInDifferentBlock`
  - `RedefinitionBeforeConsumer`

Validation:

- `cargo test -p fission-pcode malformed_def_use_window_relation_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140008090` is not a single malformed family. The dominant owner is now explicit:
  - `ConsumerInDifferentBlock = 1089`
  - `RedefinitionBeforeConsumer = 293`
  - `TerminatorMissing = 98`
  - dominant RHS kinds inside this family:
    - `Binary = 928`
    - `Call = 250`
    - `Const = 222`
    - `Var = 80`
- `0x140006c20` is simpler and does not currently show the missing-terminator variant:
  - `ConsumerInDifferentBlock = 192`
  - `RedefinitionBeforeConsumer = 144`
  - dominant RHS kinds:
    - `Binary = 204`
    - `Call = 84`
    - `Const = 48`
- the practical owner conclusion is now clearer than before:
  - both rows are primarily merge/cross-block def-use window problems, not generic malformed indexing
  - only `0x140008090` still shows a secondary `TerminatorMissing` slice

Conclusion:

- `UnknownMalformedDefUseWindow` should not be treated as one policy family anymore
- the next release owner should target `ConsumerInDifferentBlock` first, because it dominates both active rows and points at merge/dominance/materialization-window handling rather than broad alias relaxation
- `TerminatorMissing` is a secondary builder indexing/boundary owner and should be handled separately after the cross-block family

### NoConsumer suppression regression attribution

This wave changed the release path back to fail-closed while keeping the new regression-attribution diagnostics alive. The earlier `UnknownNoConsumerFound` suppression policy turned out to be too broad for release quality: it removed many dead-looking `UNIQUE` representatives, but same-axis `putty` limit50 still regressed and even pulled `0x140008900` into the failed row gate set. The active path now leaves those candidates materialized by default again unless an explicit env gate is enabled for experiment-only runs.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now treats the no-consumer suppression policy as opt-in:
  - `FISSION_ENABLE_NO_CONSUMER_SUPPRESSION=1|true|yes` is required to actually suppress
  - default release-path behavior is to keep the binding and trace it as a suppression candidate
- the builder now emits a dedicated attribution line for every release-path suppression candidate:
  - `no-consumer-suppression-detail output=... rhs_kind=... block_position=... output_kind=... applied=...`
- the keep vocabulary gained an explicit rollback reason:
  - `SuppressionDisabled`
- this preserves the regression triage surface without leaving the over-broad suppression policy active on `main`

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_ENABLE_NO_CONSUMER_SUPPRESSION=0 FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008900 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008900 --engine nir --profile nir --ghidra-compat`
- `FISSION_ENABLE_NO_CONSUMER_SUPPRESSION=0 FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --limit 50 --pairwise-similarity-mode shared-full --fission-bin target/debug/fission_cli --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC --output-dir artifacts/batch_benchmark/putty-no-consumer-regression-attribution-limit50 --baseline-dir artifacts/batch_benchmark/putty-builder-provenance-wave`

Observed state:

- the active path is no longer suppressing `UnknownNoConsumerFound` residues by default:
  - `0x140008900`:
    - `suppression_candidate=1785`
    - `no-consumer-suppression-detail=1785`
    - `no-consumer-kept=2207`
    - no live `no-consumer-suppressed` events on the default path
  - `0x140008090`:
    - `suppression_candidate=1686`
    - `no-consumer-suppression-detail=1686`
    - `no-consumer-kept=2137`
    - no live `no-consumer-suppressed` events on the default path
- the new regression-attribution dimensions show that the reverted suppression family is still dominated by trivial temp materialization, not named/local surfaced values:
  - `0x140008900` suppression candidates:
    - `rhs_kind`: `Const=1734`, `Var=43`, `Cast=8`
    - `block_position`: `PreBranch=1621`, `PredicateAdjacent=98`, `MergeAdjacent=50`, `Local=16`
    - `output_kind`: `TempOnly=1785`
  - `0x140008090` suppression candidates:
    - `rhs_kind`: `Const=1665`, `Var=21`
    - `block_position`: `PreBranch=1588`, `PredicateAdjacent=56`, `MergeAdjacent=42`
    - `output_kind`: `TempOnly=1686`

Benchmark result:

- same-axis `putty` limit50 remains below the accepted baseline, but the rollback does remove the extra `0x140008900` regression introduced by the broad suppression wave
- current aggregate:
  - `avg_normalized_similarity = 38.74` vs baseline `38.82`
- row gate still fails for:
  - `0x140008090 = 35.28` vs baseline `35.63`
  - `0x140006c20` vs baseline `40.52`
  - `0x140006fe0` vs baseline `34.76`
- rows now back above baseline or non-worse on this axis include:
  - `0x140001160 = 32.39`
  - `0x140008900 = 23.97`
  - `0x140007da0 = 34.60`
- semantic guardrails stayed aligned with the accepted shape:
  - `unsupported_indirect_control_count = 1`
  - `dispatcher_shape_recovered_count = 12`

Conclusion:

- the broad `UnknownNoConsumerFound` suppression policy should remain off on the release path
- its diagnostic surface is still useful, and the new attribution proves the dominant reverted family is mostly constant/temp-only pre-branch materialization
- the next release owner is not broad no-consumer suppression; it is row-level representative/materialization stability on `0x140008090`, `0x140006c20`, and `0x140006fe0`

### NoConsumerFound dead materialization suppression

This wave changed behavior. It no longer kept every `UnknownNoConsumerFound` representative materialized by default. The policy was narrowed to suppress only builder-local dead-ish residues whose replacement stop was already `AliasUnsafe`, but whose live diagnostic profile showed no same-block use, no cross-block use, no merge use, no debug use, no legacy-inline eligibility, no preservation requirement, no side-effectful RHS, and a `UNIQUE`-space output.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now classifies `UnknownNoConsumerFound` into a policy decision:
  - `Suppress`
  - `Keep(<reason>)`
- suppression is deliberately fail-closed and only happens when all of the following are true:
  - `same_block_consumers == 0`
  - `cross_block_consumers == 0`
  - `has_later_block_use == false`
  - `has_phi_merge_use == false`
  - `has_debug_use == false`
  - `legacy_inline_candidate == false`
  - `preserve_materialization == false`
  - `rhs_side_effectful == false`
  - output is a non-constant `UNIQUE` temp
- the trace vocabulary now distinguishes policy action:
  - `no-consumer-suppressed ...`
  - `no-consumer-kept ... reason=...`
- the keep side is still explicit and deterministic:
  - `PreserveMaterialization`
  - `RhsSideEffectful`
  - `StateVisibleOutput`
  - and the other profile guards
- unit coverage now fixes both directions:
  - dead unique constant temp can be suppressed
  - preserved expressions stay materialized
  - non-`UNIQUE` outputs stay materialized

Validation:

- `cargo test -p fission-pcode no_consumer_materialization_ --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --limit 50 --pairwise-similarity-mode shared-full --fission-bin target/debug/fission_cli --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC --output-dir artifacts/batch_benchmark/putty-no-consumer-suppression-limit50 --baseline-dir artifacts/batch_benchmark/putty-builder-provenance-wave`

Observed state:

- targeted `UnknownNoConsumerFound` suppression did fire exactly on the intended profile:
  - `0x140008090`:
    - `UnknownNoConsumerFound=2137`
    - `no-consumer-suppressed=1686`
    - `no-consumer-kept=451`
  - `0x140006c20`:
    - `UnknownNoConsumerFound=834`
    - `no-consumer-suppressed=622`
    - `no-consumer-kept=212`
- kept cases were dominated by explicit preservation requirements, e.g. compare/flag-like booleans and helper-call-derived booleans:
  - `reason=PreserveMaterialization`
- the suppress side remained mostly constant-valued `UNIQUE` temps with no proven downstream consumer

Benchmark result:

- same-axis `putty` limit50 acceptance still failed versus [`putty-builder-provenance-wave`](../../artifacts/batch_benchmark/putty-builder-provenance-wave)
- current aggregate:
  - `avg_normalized_similarity = 38.58` vs baseline `38.82`
- row gate failed for:
  - `0x140008900 = 20.86` vs baseline `23.62`
  - `0x140008090 = 35.22` vs baseline `35.63`
  - `0x140006c20 = 40.28` vs baseline `40.52`
  - `0x140006fe0 = 33.47` vs baseline `34.76`
- rows that stayed above baseline:
  - `0x140001160 = 32.47`
  - `0x140007da0 = 34.65`
  - `0x140006ef0` remained non-worse in this run

Conclusion:

- the suppression policy itself is behaving as designed on the targeted owner family
- but this wave is not an acceptance win: same-axis `putty` limit50 still regressed versus the accepted baseline
- the next owner should not widen suppression further blindly; it should either:
  - narrow the suppression family further based on row-level regressions, or
  - move to the next major owner family (`MalformedDefUseWindow`) if this dead-residue closure is not the dominant release blocker

### NoConsumerFound materialization suppression trace

This wave stayed diagnostic-only. It did not suppress dead-ish representatives, widen representative downgrade, or relax alias safety. The goal was to determine whether the dominant `UnknownNoConsumerFound` family on `0x140008090` / `0x140006c20` is a real consumer-scanner blind spot or a builder-owned bookkeeping/materialization residue.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated `EMIT-TRACE` line for the `AliasUnsafeHazardKind::UnknownNoConsumerFound` path:
  - `no-consumer-materialization output=... def_block=... op_seq=... rhs=... materialization_event=... preserve_materialization=... legacy_inline_candidate=... has_later_block_use=... has_phi_merge_use=... has_debug_use=... same_block_consumers=... cross_block_consumers=... rhs_side_effectful=...`
- the trace is emitted only when the active replacement plan stopped on `AliasUnsafe` and the builder classifier narrowed that stop to `UnknownNoConsumerFound`
- a builder-local profile helper now separates:
  - same-block consumers
  - cross-block consumers
  - `MultiEqual`/merge-like nonlocal use
  - conservative RHS side-effect risk
- unit coverage now fixes the diagnostic contract for:
  - a truly local dead-ish representative with no later uses
  - a cross-block `MultiEqual` consumer, proving the helper will still flag merge-like use instead of collapsing everything into a false dead case

Validation:

- `cargo test -p fission-pcode no_consumer_materialization_profile --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- on both targeted rows, the live `UnknownNoConsumerFound` path now resolves to the same concrete shape:
  - `same_block_consumers=0`
  - `cross_block_consumers=0`
  - `has_later_block_use=false`
  - `has_phi_merge_use=false`
  - `has_debug_use=false`
  - `legacy_inline_candidate=false`
  - `materialization_event=materialized_binding`
- `0x140008090`:
  - `UnknownNoConsumerFound=2137`
  - all 2137 live samples reported zero same-block and zero cross-block consumers
  - `preserve_materialization=false` for `1686`
  - `preserve_materialization=true` for `451`
  - `rhs_side_effectful=true` for `80`
- `0x140006c20`:
  - `UnknownNoConsumerFound=834`
  - all 834 live samples reported zero same-block and zero cross-block consumers
  - `preserve_materialization=false` for `622`
  - `preserve_materialization=true` for `212`
  - `rhs_side_effectful=true` for `35`
- representative live samples are mostly constants and comparison/bookkeeping booleans that still surface as `materialized_binding` despite having no proven later use on the targeted rows

Conclusion:

- the current live `UnknownNoConsumerFound` owner on `0x140008090` / `0x140006c20` is not behaving like a consumer-scanner blind spot
- on these rows it is a builder-owned dead-ish / bookkeeping materialization family: no same-block use, no cross-block use, no merge use, and no debug-only survivor was observed
- the next high-value wave is no longer more tracing; it is a narrow policy patch for safe dead-ish suppression or representative downgrade on this specific no-consumer family, with `MalformedDefUseWindow` remaining the next backup owner if suppression does not move the row gate

### AliasUnsafe Unknown subtyping

This wave stayed diagnostic-only. It did not widen builder replacement, add merge synthesis, or relax alias safety. The goal was to replace the residual same-block `Unknown` fallback on the active `0x140008090` / `0x140006c20` path with concrete blind-spot subtypes, so the next patch can target one materialization owner instead of a generic catch-all.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now splits builder-local `AliasUnsafe::Unknown` into explicit same-block fallback families:
  - `UnknownNoConsumerFound`
  - `UnknownConsumerAfterTerminator`
  - `UnknownUnhandledConsumerKind`
  - `UnknownMalformedDefUseWindow`
- the `EMIT-TRACE` channel now carries a dedicated subtype line when one of those fallback paths fires:
  - `alias-unsafe-unknown-shape output=... def_block=... op_seq=... terminator_idx=... consumer_count=... same_block_consumers=... first_consumer_stmt=... first_consumer_op=... first_consumer_relation=... reason=...`
- the classifier stays intentionally conservative:
  - no acceptance logic changed
  - no representative downgrade policy changed
  - no merge binding policy changed
- builder-local unit coverage now fixes the subtype contract for:
  - dead-ish no-consumer windows
  - redefinition-before-consumer malformed windows
  - allowed-but-unhandled single-consumer opcodes
  - after-terminator single-consumer windows

Validation:

- `cargo test -p fission-pcode alias_unsafe_unknown_subtyping --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140008090` no longer carries a live generic `Unknown` bucket. The rebuilt trace split as:
  - `UnknownNoConsumerFound=2137`
  - `UnknownMalformedDefUseWindow=1480`
  - `UnknownUnhandledConsumerKind=7`
  - `DisallowedSingleConsumer=1553`
  - `MultipleSameBlockConsumers=581`
  - `SameBlockStore=49`
- `0x140006c20` showed the same dominant blind spots with smaller volume:
  - `UnknownNoConsumerFound=834`
  - `UnknownMalformedDefUseWindow=336`
  - `DisallowedSingleConsumer=515`
  - `MultipleSameBlockConsumers=160`
- no live `UnknownConsumerAfterTerminator` surfaced on the targeted rows in this wave; that path is covered synthetically but is not a current row owner
- representative sample traces show the two dominant same-block fallback families are structurally different:
  - `UnknownNoConsumerFound`: `consumer_count=0`, no same-block consumer discovered at all
  - `UnknownMalformedDefUseWindow`: `consumer_count=0`, but the output is redefined before any consumer can be proven
  - `UnknownUnhandledConsumerKind`: rare single-consumer cases such as `IntZExt` that are same-block and between def/use, but still fall outside the current low-cost inline contract

Conclusion:

- the next owner is no longer generic `AliasUnsafe`; it is one of two concrete same-block fallback families:
  - `UnknownNoConsumerFound`
  - `UnknownMalformedDefUseWindow`
- `DisallowedSingleConsumer` remains a secondary owner, but the current release-weighted blind spot is the large no-consumer / malformed-window pair rather than predicate-sensitive or after-terminator cases
- the highest-value next wave is not an alias relaxation. It is either:
  - dead-ish representative suppression / downgrade closure for `UnknownNoConsumerFound`
  - def-use window refinement for `UnknownMalformedDefUseWindow`

### AliasUnsafe first-hazard tracing

This wave stayed diagnostic-only. It did not widen builder materialization acceptance, add new merge synthesis, or retune representative policy. The goal was to split the very large `AliasUnsafe` bucket on the active `0x140008090` / `0x140006c20` path into concrete same-block hazard families, so the next patch can target one owner instead of the whole rejection class.

- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now emits a dedicated `EMIT-TRACE` subtype line whenever builder replacement falls back with `reason=AliasUnsafe`:
  - `alias-unsafe-shape output=... def_block=... use_block=... first_alias_hazard=... hazard_stmt=... hazard_op=...`
- the new builder-local hazard family is intentionally narrow and same-block focused:
  - `MultipleSameBlockConsumers`
  - `DisallowedSingleConsumer`
  - `CallBetweenDefUse`
  - `LoadAfterStore`
  - `SameBlockStore`
  - `Unknown`
- cross-block drift was intentionally left out of this family because that path is already owned by `MissingMergeBinding` / `Merge`, not `AliasUnsafe`
- builder-local unit coverage now fixes the classifier contract for:
  - `CallBetweenDefUse`
  - `LoadAfterStore`
  - `MultipleSameBlockConsumers`
  - `DisallowedSingleConsumer`

Validation:

- `cargo test -p fission-pcode alias_unsafe_hazard --lib -- --test-threads=1`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- `0x140008090` no longer reports `AliasUnsafe` as a single opaque family; the live trace split as:
  - `Unknown=3617`
  - `DisallowedSingleConsumer=1560`
  - `MultipleSameBlockConsumers=581`
  - `SameBlockStore=49`
- `0x140006c20` showed the same active family shape with smaller volume:
  - `Unknown=1170`
  - `DisallowedSingleConsumer=515`
  - `MultipleSameBlockConsumers=160`
- no live `CallBetweenDefUse` or `LoadAfterStore` owner surfaced on the targeted rows in this wave; those shapes are covered synthetically but are not the dominant current row owners
- the practical live signal moved away from a generic alias bucket and toward two builder-owned same-block families:
  - repeated same-block consumers
  - single-use consumers whose opcode or inline contract still rejects representative replacement

Conclusion:

- the next owner is still builder/materialization representative stability rather than guarded-tail widening
- the highest-value follow-up is not a generic alias relaxation; it is one of:
  - `Unknown` same-block fallback subtyping
  - `DisallowedSingleConsumer` proof narrowing
  - `MultipleSameBlockConsumers` same-block reuse closure

### Emit-ready materialization owner tracing

This wave stayed diagnostic-only. It did not widen guarded-tail acceptance, alter emit legality, or retune materialization policy. The goal was to identify the active owner behind the remaining row-gate regressions on `0x140008090` and `0x140006c20` after `0x140006fe0 candidate 35` had already been finalized as a soundness-preserving unsafe-callee stop.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now exposes a shared row-targeted diagnostic channel:
  - `emit_ready_trace_enabled_for_current_fn()`
  - `emit_ready_trace(...)`
- [`execution.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/execution.rs) now emits explicit guarded-tail execute diagnostics when a candidate stops on emit-readiness:
  - `must_emit_label ... surviving_ref_kind=...`
  - `unstable_read binding=... read_kinds=[...]`
- [`materialize.rs`](../../crates/fission-pcode/src/nir/builder/materialize.rs) now traces replacement/materialization decisions for builder-owned representatives:
  - `event=materialized_binding`
  - `event=inline_suppressed`
  - `event=representative_downgrade`
  - together with:
    - `dominant_read`
    - `reason`
    - source `rhs`
    - lowered `block` and `op_seq`

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140008090 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140008090 --engine nir --profile nir --ghidra-compat`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006c20 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006c20 --engine nir --profile nir --ghidra-compat`

Observed state:

- the targeted rows did not surface a new guarded-tail acceptance opportunity; the active signal was builder/materialization drift rather than a newly actionable guarded-tail proof closure
- `0x140008090` emitted only `EMIT-TRACE` materialization diagnostics on the active path:
  - events:
    - `materialized_binding=6853`
    - `inline_suppressed=980`
    - `representative_downgrade=263`
  - dominant read classes:
    - `SameBlockData=6908`
    - `Merge=1066`
    - `PredicateSensitive=115`
    - `SelectorSensitive=7`
  - rejection/completeness reasons:
    - `AliasUnsafe=5807`
    - `MissingMergeBinding=1066`
    - `ConsumerRequiresStableRepresentative=960`
    - `Complete=263`
- `0x140006c20` showed the same owner family with smaller volume:
  - events:
    - `materialized_binding=2177`
    - `inline_suppressed=383`
    - `representative_downgrade=80`
  - dominant read classes:
    - `SameBlockData=2251`
    - `Merge=342`
    - `PredicateSensitive=40`
    - `SelectorSensitive=7`
  - rejection/completeness reasons:
    - `AliasUnsafe=1845`
    - `MissingMergeBinding=342`
    - `ConsumerRequiresStableRepresentative=373`
    - `Complete=80`
- `0x140006c20` also exposed a direct predicate-sensitive case on the active path:
  - `dominant_read=PredicateSensitive reason=ConsumerRequiresStableRepresentative rhs=Unary { op: Not, expr: Var("xVar57"), ... }`

Conclusion:

- the next owner is still not guarded-tail suffix widening
- the remaining row-gate work is centered on builder/materialization and emit-readiness, especially:
  - `AliasUnsafe`
  - `MissingMergeBinding`
  - `ConsumerRequiresStableRepresentative`
- `0x140008090` is the best next primary row because it carries the same family at higher volume and already aligns with the benchmark gate’s `materialization_drift + must_emit_label_conflict + alias_interleave_conflict` bundle

## 2026-04-18

### Guarded-tail side-effectful callee rejection finalization

This wave did not widen guarded-tail acceptance. It finalized the current stop condition for call-bearing suffixes whose callee summary is already proven unsafe by `PreviewCalleeAnalysis`. The goal was to turn the current `0x140006fe0` `candidate 35` boundary into an explicit, canonical rejection subtype rather than leaving it as an undifferentiated generic side-effect bucket.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now recognizes the active guarded-tail stop condition directly on the live suffix path:
  - `suffix-side-effectful-callee-stop`
  - `guarded-tail-rejection subtype=PreviewCalleeAnalysisUnsafe`
- the stop fires only when all of the following are true:
  - the suffix statement is a call-bearing side-effect statement
  - the call summary exists in `NirTypeContext`
  - the summary source is `PreviewCalleeAnalysis`
  - at least one unsafe effect bit is explicitly `yes`:
    - `writes_memory`
    - `may_call_unknown`
    - `may_exit`
- [`types.rs`](../../crates/fission-pcode/src/nir/types.rs) extends canonical telemetry with:
  - `guarded_tail_rejected_side_effectful_callee_count`
- [`builder/state.rs`](../../crates/fission-pcode/src/nir/builder/state.rs), [`builder/init.rs`](../../crates/fission-pcode/src/nir/builder/init.rs), and [`builder/stats.rs`](../../crates/fission-pcode/src/nir/builder/stats.rs) now carry the new canonical counter from preview builder state into `NirBuildStats`
- [`quality.rs`](../../crates/fission-automation/src/report/quality.rs) now projects the new counter into automation quality rollups and folds it into the existing canonical rewrite-conflict family

Validation:

- `cargo check -p fission-pcode`
- `cargo check -p fission-automation`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `0x140006fe0` stopper now surfaces as an explicit canonical subtype:
  - `suffix-side-effectful-callee-stop stmt_idx=154 target=FUN_0x140043d30 source=PreviewCalleeAnalysis`
  - `guarded-tail-rejection subtype=PreviewCalleeAnalysisUnsafe target=FUN_0x140043d30`
- the original guarded-tail shell still stays fail-closed:
  - `candidate=35`
  - `join_label=block_140007047`
  - `first_reject=AliasNotFallthrough`
- the unsafe-callee stop is not specific to a single target:
  - the same subtype also surfaces on earlier internal call-bearing suffix blockers such as `FUN_0x1400d23a0`

Conclusion:

- `candidate 35` is no longer just “generic side effect”; it is now an explicitly classified soundness-preserving stop
- the next step should not widen local guarded-tail acceptance for this case
- the next useful work item is either:
  - corpus / benchmark remeasurement after the recent narrow internalization waves, or
  - selecting a different active blocker whose stop reason is not already a proven unsafe callee summary

### Guarded-tail callee bounds and thunk-shape validation

This wave stayed diagnostic-only. It did not widen guarded-tail acceptance or reinterpret the preview callee effect bits. The goal was to determine whether `FUN_0x140043d30` was being marked unsafe because of real callee behavior or because preview lift bounds / wrapper shape were over-approximated.

- [`facts.rs`](../../crates/fission-decompiler-core/src/facts.rs) now emits producer-side lift-bound and shape diagnostics alongside the existing first-cause detail trace:
  - `callee-lift-bounds`
  - `callee-shape`
  - `callee-effect-first-op-detail`
- the new trace records:
  - `start`, `max_bytes`, `instruction_limit`, recorded `function_size`, and `next_function`
  - whether the first `Store`, `Call`, and `CallOther` addresses are still inside the current function bounds
  - preview-lifted `block_count`, `op_count`, `return_count`
  - whether there is any decoded fallthrough past the first `Return`
  - whether the callee looks like a trivial single-call-return wrapper
- this remains producer-side validation only:
  - no guarded-tail suffix ownership rule changed
  - no callee summary bit changed
  - no thunk normalization was applied

Validation:

- `cargo test -p fission-decompiler-core preview_callee_effect_summary -- --nocapture`
- `cargo build -p fission-cli`
- `cargo check -p fission-pcode`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the preview lift for `FUN_0x140043d30` is bounded by recorded function metadata, not a size-0 fallback:
  - `callee-lift-bounds target=FUN_0x140043d30 start=0x140043d30 max_bytes=413 instruction_limit=103 function_size=413 next_function=Some("0x140043ed0")`
- the preview-lifted body shape is not a thunk-like wrapper:
  - `callee-shape target=FUN_0x140043d30 block_count=5 op_count=290 return_count=1 has_fallthrough_past_return=false single_call_return_wrapper=false`
- the first unsafe causes are all still inside the function bounds:
  - `callee-effect-first-op-detail target=FUN_0x140043d30 kind=Store addr=0x140043d30 within_function=true ...`
  - `callee-effect-first-op-detail target=FUN_0x140043d30 kind=Call addr=0x140043d63 within_function=true ...`
  - `callee-effect-first-op-detail target=FUN_0x140043d30 kind=CallOther addr=0x140043d70 within_function=true ...`
- the guarded-tail consumer result on the active path remains unchanged:
  - `suffix-unknown-call-summary target=FUN_0x140043d30 ... effect_summary_source=PreviewCalleeAnalysis`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=yes writes_global=unknown may_call_unknown=yes may_exit=yes return_used=false`

Conclusion:

- the current `PreviewCalleeAnalysis` verdict is not explained by obvious callee-bound overrun, fallthrough-past-return, or trivial thunk-wrapper shape
- `FUN_0x140043d30` currently looks like a genuinely side-effectful internal callee under the active preview lift
- the guarded-tail path should stay fail-closed here unless a later wave finds a more precise callee summarization or thunk-normalization source

### Guarded-tail callee effect detail tracing

This wave stayed diagnostic-only. It did not widen guarded-tail acceptance. The goal was to expose the first concrete p-code causes behind the existing `PreviewCalleeAnalysis` verdict for the remaining `stmt_idx=154` call-bearing suffix blocker.

- [`facts.rs`](../../crates/fission-decompiler-core/src/facts.rs) now expands preview callee effect production with first-cause detail tracing:
  - `PreviewCalleeEffectDetail`
  - `trace_preview_callee_effect_detail(...)`
  - `summarize_preview_callee_effects(...) -> (NirCallEffectSummary, PreviewCalleeEffectDetail)`
- the detail trace records:
  - total `STORE` / `CALL` / `CALLIND` / `CALLOTHER` / `RETURN` counts seen in the preview-lifted callee
  - the first `STORE`
  - the first direct or indirect `CALL`
  - the first `CALLOTHER`
- this remains producer-side diagnosis only:
  - no guarded-tail suffix ownership rule changed
  - no call-bearing suffix acceptance changed
  - no effect summary bit was reinterpreted

Validation:

- `cargo test -p fission-decompiler-core preview_callee_effect_summary -- --nocapture`
- `cargo build -p fission-cli`
- `cargo check -p fission-pcode`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `FUN_0x140043d30` callee summary is now backed by explicit first-cause evidence:
  - `callee-effect-detail target=FUN_0x140043d30 target_addr=0x140043d30 store_count=27 call_count=2 callind_count=6 callother_count=7 return_count=1`
  - `callee-effect-first-store target=FUN_0x140043d30 addr=0x140043d30 op=Store`
  - `callee-effect-first-call target=FUN_0x140043d30 addr=0x140043d63 call_target=Some(5368986496) op=Call`
  - `callee-effect-first-callother target=FUN_0x140043d30 addr=0x140043d70 op=CallOther`
- the guarded-tail consumer result on the active path remains:
  - `suffix-call-effect-shape stmt_idx=154 kind=VoidUnknownCall`
  - `suffix-unknown-call-summary target=FUN_0x140043d30 ... effect_summary_source=PreviewCalleeAnalysis`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=yes writes_global=unknown may_call_unknown=yes may_exit=yes return_used=false`
- this narrows the current blocker further:
  - the producer is not merely missing precision metadata
  - it is seeing real `Store`, `Call`/`CallInd`, and `CallOther` operations in the lifted callee body

Conclusion:

- `FUN_0x140043d30` is currently an actually unsafe callee for guarded-tail suffix internalization under the active conservative rules
- the next owner is callee summary precision itself:
  - either the preview-lifted callee body is correct, in which case guarded-tail should remain fail-closed here
  - or the lift/bounds/thunk shape is over-approximated, in which case the next wave should refine callee bounds or thunk normalization rather than widening local guarded-tail acceptance

### Guarded-tail intraprocedural callee effect summary producer

This wave added the first real callee-effect producer for guarded-tail call-bearing suffix diagnostics. It still did not widen guarded-tail acceptance. The goal was to replace the active `stmt_idx=154` state from “summary source exists but effect bits are unknown” with a conservative intraprocedural effect summary derived from the direct internal callee body itself.

- [`types.rs`](../../crates/fission-pcode/src/nir/types.rs) extends `CallEffectSummarySource` with `PreviewCalleeAnalysis`
- [`lib.rs`](../../crates/fission-decompiler-core/src/lib.rs) promotes `decode_rust_sleigh_pcode(...)` to `pub(crate)` so decompiler-core context assembly can reuse the existing Rust-Sleigh lift path for direct internal callees
- [`facts.rs`](../../crates/fission-decompiler-core/src/facts.rs) now owns the first intraprocedural producer:
  - `refine_nir_type_context_with_callee_effect_summaries(...)`
  - `collect_direct_internal_callee_targets(...)`
  - `build_preview_callee_effect_summary(...)`
  - `summarize_preview_callee_effects(...)`
- the producer is intentionally narrow and fail-closed:
  - only direct callee targets actually present in the current function's p-code are considered
  - only direct internal callees are lifted
  - `STORE` marks `writes_memory=yes`
  - `CALL` / `CALLIND` mark `may_call_unknown=yes`
  - `CALLOTHER` marks `may_call_unknown=yes` and `may_exit=yes`
  - leaf `RETURN`-only callees can produce `may_exit=no`
  - unresolved fields remain unknown
- [`render.rs`](../../crates/fission-decompiler-core/src/render.rs) now refines `NirTypeContext` with callee-effect summaries before both:
  - in-process NIR render
  - worker request construction
- [`engine.rs`](../../crates/fission-decompiler-core/src/engine.rs) test imports were updated to keep decompiler-core test coverage compiling with the current `StructuringEngineKind` usage

Validation:

- `cargo test -p fission-decompiler-core preview_callee_effect_summary -- --nocapture`
- `cargo check -p fission-decompiler-core`
- `cargo build -p fission-cli`
- `cargo check -p fission-pcode`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `stmt_idx=154` guarded-tail path now reads a real producer-backed summary:
  - `suffix-unknown-call-provenance stmt_idx=154 target=FUN_0x140043d30 target_addr=Some(5368986928) internal=true import=false summary_available=true`
  - `suffix-unknown-call-summary target=FUN_0x140043d30 binary_function_present=true target_ref_present=true target_ref_provenance=Direct effect_summary_source=PreviewCalleeAnalysis`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=yes writes_global=unknown may_call_unknown=yes may_exit=yes return_used=false`
- this means the active blocker is no longer “missing producer” and no longer “all effect bits unknown”
- instead, the producer now says the callee is unsafe for guarded-tail suffix internalization on the current conservative rules:
  - it writes memory
  - it may call unknown code
  - it may exit
- the outer guarded-tail shell is still unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `FUN_0x140043d30` is no longer blocked by missing provenance or missing summary plumbing
- the current producer actively classifies it as unsafe
- the next wave should decide whether that callee summary is precise enough, not widen guarded-tail local call acceptance

### Guarded-tail `CallTargetRef` effect-summary lookup wiring

This wave stayed diagnostic-only. It did not widen guarded-tail call acceptance. The goal was to move the active `stmt_idx=154` path from “direct callee is visible but has no summary container” to “direct callee has a concrete summary source, even if every effect bit is still unknown”.

- [`types.rs`](../../crates/fission-pcode/src/nir/types.rs) now adds a minimal guarded-tail-consumable callee summary contract:
  - `CallEffectSummarySource`
  - `NirCallEffectSummary`
  - `NirTypeContext::call_effect_summaries`
- [`lib.rs`](../../crates/fission-pcode/src/lib.rs) re-exports the new internal summary vocabulary for downstream context assembly
- [`facts.rs`](../../crates/fission-decompiler-core/src/facts.rs) now builds a conservative summary map from `call_target_refs`:
  - each direct call target gets a `NirCallEffectSummary`
  - all effect bits remain `unknown`
  - `source=CallTargetRef`
- [`engine.rs`](../../crates/fission-decompiler-core/src/engine.rs) and targeted preview tests now initialize the new `call_effect_summaries` field on explicit `NirTypeContext` / `PreviewTypeContext` constructors
- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now consumes `type_context.call_effect_summaries` inside `trace_suffix_unknown_call_provenance(&self, ...)`
  - `summary_available` now becomes true on the active guarded-tail candidate path once a `CallTargetRef` summary exists
  - `suffix-unknown-call-summary ... effect_summary_source=CallTargetRef` now distinguishes “summary source exists” from “all effect bits are still unknown”

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `stmt_idx=154` guarded-tail path now resolves a concrete summary source:
  - `suffix-unknown-call-provenance stmt_idx=154 target=FUN_0x140043d30 target_addr=Some(5368986928) internal=true import=false summary_available=true`
  - `suffix-unknown-call-summary target=FUN_0x140043d30 binary_function_present=true target_ref_present=true target_ref_provenance=Direct effect_summary_source=CallTargetRef`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=unknown writes_global=unknown may_call_unknown=unknown may_exit=unknown return_used=false`
- this is still a fail-closed summary:
  - no positive readonly / pure / no-exit proof exists yet
  - the effect fields remain unknown until an interprocedural producer fills them
- non-builder fallback traces can still show `effect_summary_source=None`; this wave only wires the active guarded-tail candidate path that already has `call_target_refs` and `type_context` visibility
- the outer guarded-tail shell remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- the active blocker is no longer “missing summary container”
- it is now specifically “summary source exists, but effect facts are still unknown”
- the next wave should produce a real interprocedural callee-effect summary for `FUN_0x140043d30` instead of widening guarded-tail local call acceptance

### Guarded-tail callee summary source tracing for `FUN_0x140043d30`

This wave stayed diagnostic-only. It did not widen guarded-tail call acceptance. The goal was to distinguish “unknown call target” from “known internal direct callee that still lacks an effect-summary source” for the remaining `stmt_idx=154` blocker.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now adds builder-aware guarded-tail suffix diagnostics on the live execution path:
  - `trace_suffix_unknown_call_provenance(&self, ...)`
  - `classify_suffix_stmt_with_diag(...)`
  - `suffix_is_nonowned_terminal_tail_with_diag(...)`
  - `candidate_window_can_shrink_to_label_with_diag(...)`
  - `find_earliest_owned_join_label_with_diag(...)`
- [`execution.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/execution.rs) now routes live guarded-tail candidate narrowing through the new builder-aware diagnostic path
- the new trace separates three different facts:
  - target identity and address
  - binary / type-context visibility
  - effect-summary source availability
- guarded-tail diagnostics now emit:
  - `suffix-unknown-call-provenance stmt_idx=... target=... target_addr=... internal=... import=... summary_available=...`
  - `suffix-unknown-call-summary target=... binary_function_present=... target_ref_present=... target_ref_provenance=... effect_summary_source=None`
  - `suffix-unknown-call-effect target=... writes_memory=... writes_global=... may_call_unknown=... may_exit=... return_used=...`

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `stmt_idx=154` blocker is now fixed as a direct internal callee with no effect-summary source:
  - `suffix-call-effect-shape stmt_idx=154 kind=VoidUnknownCall ...`
  - `suffix-unknown-call-provenance stmt_idx=154 target=FUN_0x140043d30 target_addr=Some(5368986928) internal=true import=false summary_available=false`
  - `suffix-unknown-call-summary target=FUN_0x140043d30 binary_function_present=true target_ref_present=true target_ref_provenance=Direct effect_summary_source=None`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=unknown writes_global=unknown may_call_unknown=true may_exit=unknown return_used=false`
- this is no longer an unresolved symbol problem:
  - the callee exists in the loaded binary
  - the live preview path can resolve it through `call_target_refs`
- the missing piece is now explicit:
  - there is still no interprocedural effect summary source wired into guarded-tail suffix ownership for this direct internal callee
- the outer guarded-tail shell remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `FUN_0x140043d30` is visible as a direct internal callee, not an import and not an unknown symbol
- the remaining blocker is specifically “effect summary source missing”, not “target identity missing”
- the next wave should connect a real interprocedural callee-effect summary source before any call-bearing suffix internalization is considered

### Guarded-tail unknown call provenance tracing for `FUN_0x140043d30`

This wave stayed diagnostic-only. It did not widen guarded-tail call acceptance. The goal was to turn the remaining `stmt_idx=154` call-bearing suffix blocker into a concrete provenance/effect boundary instead of another generic `VoidUnknownCall` bucket.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now adds:
  - `suffix_call_expr(...)`
  - `trace_suffix_unknown_call_provenance(...)`
- guarded-tail suffix diagnostics now emit two additional traces for unknown call-bearing suffix statements:
  - `suffix-unknown-call-provenance stmt_idx=... target=... internal=... import=... summary_available=...`
  - `suffix-unknown-call-effect target=... writes_memory=... writes_global=... may_call_unknown=... may_exit=... return_used=...`
- this remains fail-closed:
  - no suffix budget math changed
  - no known-pure helper allowlist changed
  - no guarded-tail ownership acceptance changed

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active `stmt_idx=154` blocker is now fixed as an internal unknown call boundary:
  - `suffix-call-effect-shape stmt_idx=154 kind=VoidUnknownCall stmt=Expr(Call { target: "FUN_0x140043d30", args: [], ty: Unknown })`
  - `suffix-unknown-call-provenance stmt_idx=154 target=FUN_0x140043d30 internal=true import=false summary_available=false`
  - `suffix-unknown-call-effect target=FUN_0x140043d30 writes_memory=unknown writes_global=unknown may_call_unknown=true may_exit=unknown return_used=false`
- the old known-pure helper path still does not apply:
  - no `known-pure-helper-proof` trace fires for `FUN_0x140043d30`
- the guarded-tail shell remains otherwise unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `stmt_idx=154` is not another allowlisted helper-call case
- the remaining owner is a callee-provenance boundary, not a suffix-local helper proof miss
- the next wave should source or derive call-effect summaries for `FUN_0x140043d30`, not broaden generic unknown-call acceptance

### Guarded-tail unknown call suffix diagnosis for `stmt_idx=154`

This wave stayed diagnostic-only. It did not widen call-bearing suffix ownership. The goal was to classify the new `stmt_idx=154` blocker precisely and only emit proof-bit detail when the suffix call actually falls into the known-pure helper family.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now expands `suffix_known_pure_helper_call_is_owned_safe(...)` into named proof bits:
  - `args_pure`
  - `target_known_pure`
  - `no_redefine`
  - `pre_terminal_owned_safe`
  - `no_terminal_escape`
- the known-pure helper path now emits:
  - `known-pure-helper-proof stmt_idx=... target=... args_pure=... no_redefine=... pre_terminal_owned_safe=... no_terminal_escape=... result=...`
- this trace remains gated to the actual known-pure helper path:
  - it does not fire for generic unknown calls
  - it does not change suffix acceptance or budgeting

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the current active blocker is now fixed as an unknown call-bearing suffix segment:
  - `suffix-call-effect-shape stmt_idx=154 kind=VoidUnknownCall stmt=Expr(Call { target: "FUN_0x140043d30", args: [], ty: Unknown })`
  - `suffix-side-effect-shape stmt_idx=154 kind=CallExprSideEffect ...`
- no known-pure helper proof trace fires for `stmt_idx=154`:
  - the target does not enter the allowlisted known-pure helper path
- the guarded-tail shell remains otherwise unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `stmt_idx=154` is not another `__popcount`-style helper case
- the current owner is an unknown side-effect call boundary, so fail-closed behavior remains correct
- the next wave should diagnose or prove the semantics of `FUN_0x140043d30`, not broaden generic call acceptance

### Guarded-tail paired same-guard nested boundary internalization for `block_140007040`

This wave moved from diagnosis to a narrow acceptance patch. It does not broaden generic nested-tail ownership. It only internalizes the exact paired-boundary case that the prior traces proved: two duplicate nested conditional entries with the same target label and the same guard expression family.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now extends `SuffixExternalEntryBudget` with:
  - `paired_nested_boundary_refs`
- guarded-tail suffix budgeting now adds:
  - `count_internalized_paired_nested_boundary_refs(...)`
- the new acceptance is intentionally strict:
  - raw refs must be exactly `2`
  - both refs must be `NestedConditionalGoto`
  - both refs must target the same current suffix-window label
  - both conditions must be the same exact guard-family relation:
    - currently narrowed to `ExactExpr`
  - partial subtraction is forbidden:
    - the helper returns `2` or `0`, never `1`
- focused synthetic coverage was added for:
  - exact duplicate paired nested boundary acceptance
  - guard mismatch rejection
  - mixed top-level / nested ref rejection

Validation:

- `cargo test -p fission-pcode suffix_budget_internalizes_paired_same_guard_nested_boundary -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed movement:

- the paired-boundary trace still proves the exact duplicate family:
  - `nested-boundary-pair label=block_140007040 count=2 same_guard_family=true relation_reason=Some("ExactExpr")`
- the new internalization now fires directly:
  - `paired-nested-boundary-internalized label=block_140007040 refs=[142, 145] relation=ExactExpr`
- the suffix budget now closes the previous external-entry blocker:
  - `raw_refs=2`
  - `paired_nested_boundary_refs=2`
  - `effective_external=0`
  - `allowed_external=1`
- the earlier-label blocker moved inward:
  - `early_label=block_140007040 first_fail=SuffixHasSideEffect { stmt_idx: 154 }`
- the outer shell still remains:
  - `candidate=35`
  - `join_label=block_140007047`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `block_140007040` is no longer the active external-entry owner
- the paired nested boundary has been reclassified into the owned suffix budget
- the next owner is now the call-bearing suffix segment at `stmt_idx=154`, not the old paired nested boundary

### Guarded-tail paired nested-boundary tracing for `block_140007040`

This wave stayed diagnostic-only. It did not widen guarded-tail ownership. The goal was to stop treating `block_140007040` as a one-ref mystery and instead show the full two-entry nested boundary that still keeps the suffix window fail-closed.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now adds:
  - `NestedBoundaryRefTrace`
  - `NestedBoundaryPairTrace`
  - `collect_nested_boundary_ref_traces(...)`
  - `build_nested_boundary_pair_trace(...)`
- the nested-entry proof miss path now emits:
  - `nested-boundary-ref ...`
  - `nested-boundary-pair ...`
- no acceptance logic changed:
  - this wave only expands trace coverage around the existing `nested-entry-boundary` miss

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active nested boundary now surfaces both true refs, not just the first one:
  - `nested-boundary-ref label=block_140007040 ref_idx=142 kind=NestedConditionalGoto ...`
  - `nested-boundary-ref label=block_140007040 ref_idx=145 kind=NestedConditionalGoto ...`
- the pair-level trace shows that the two refs are structurally aligned:
  - `nested-boundary-pair label=block_140007040 count=2 same_guard_family=true relation_reason=Some("ExactExpr") ...`
- the existing miss still remains unchanged:
  - `guard-family-match-miss ... terminal_label=block_140007047 candidate_count=0`
  - `nested-entry-guard-family-proof label=block_140007040 ... matched_cond=None result=false`
- the ownership budget also remains unchanged:
  - `raw_refs=2`
  - `internal_candidate_refs=0`
  - `effective_external=2`
  - `allowed_external=1`

Conclusion:

- `block_140007040` is now confirmed as a paired nested-conditional boundary, not a single stray nested entry
- the two external refs belong to the same guard family and are exact duplicates
- the next owner is therefore a narrow paired-boundary ownership proof, not generic nested-tail acceptance

### Guarded-tail nested-entry boundary tracing for `block_140007040`

This wave stayed diagnostic-only. It did not widen nested suffix acceptance. The goal was to decide whether the remaining `stmt_idx=142` blocker was a missed same-family guard or an actual external-owner boundary around `block_140007040`.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now adds:
  - `NestedEntryBoundaryContext`
  - `nested_entry_boundary_context(...)`
- the nested-entry proof miss path now emits:
  - `nested-entry-boundary label=... label_idx=... in_current_suffix_window=... raw_refs=... internal_candidate_refs=... suffix_safe_refs=... external_pre_guard_internalization=... external_entry_kind=... external_entry_ref_stmt_idx=...`
- no guarded-tail acceptance logic changed:
  - the change only augments the existing `nested-entry-guard-family-proof` trace

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active nested blocker is still:
  - `nested-suffix-shape stmt_idx=142 kind=NestedSingleGotoThen`
- the guard-family scan still misses any terminal witness:
  - `guard-family-match-miss ... terminal_label=block_140007047 candidate_count=0`
- the new boundary trace shows why this remains external-owner shaped:
  - `nested-entry-boundary label=block_140007040 label_idx=Some(150) in_current_suffix_window=true raw_refs=2 internal_candidate_refs=0 suffix_safe_refs=0 external_pre_guard_internalization=2 external_entry_kind=Some(NestedConditionalGoto) external_entry_ref_stmt_idx=Some(142)`
- the existing suffix budget agrees with the same diagnosis:
  - `suffix-budget label=block_140007040 raw_refs=2 internal_refs=0 suffix_safe_refs=0 guard_family_internalized_refs=0 effective_external=2 allowed_external=1`

Conclusion:

- `block_140007040` is not currently blocked by a simple same-family guard normalization miss
- inside the current suffix window there are no internal top-level ownership candidates for that label
- the remaining owner is an external nested-conditional boundary decision, not a generic nested-tail widening

### Guard-family miss candidate tracing for `block_140007040`

This wave stayed diagnostic-only and did not widen guarded-tail acceptance. The goal was to decide whether the remaining `stmt_idx=142` blocker was a guard normalization miss or a true external owner boundary.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now adds guard-family scan traces inside:
  - `find_terminal_guard_family_match_excluding(...)`
- the scan emits:
  - `guard-family-match-scan`
  - `guard-family-match-candidate`
  - `guard-family-match-miss`
- candidate-level reasons are now explicit:
  - `ExactExpr`
  - `EntryNegatesCandidate`
  - `CandidateNegatesEntry`
  - `NoGuardFamilyRelation`

Validation:

- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- recovered paths still prove normally:
  - `stmt_idx=120` and `stmt_idx=128` keep `shares=true` with exact or negated-family matches
- the active blocker remains:
  - `nested-suffix-shape stmt_idx=142 kind=NestedSingleGotoThen`
- for the `block_140007021` window, the terminal join scan still sees candidates and can match:
  - candidates at `stmt_idx=120`, `stmt_idx=128`, and `stmt_idx=149`
- for the `block_140007040` path, the decisive trace is:
  - `guard-family-match-miss ... terminal_label=block_140007047 candidate_count=0`
  - `nested-entry-guard-family-proof label=block_140007040 ref_stmt_idx=142 ... matched_cond=None result=false`

Conclusion:

- the remaining `stmt_idx=142` blocker is not currently a same-family guard that the matcher merely failed to normalize
- for `block_140007040`, the current suffix window has no comparable terminal-branch witness at all
- the next owner is therefore closer to an external owner-boundary / window-ownership decision than to a simple guard normalization patch

### Guarded-tail nested suffix proof tracing for `stmt_idx=142`

This wave did not widen suffix ownership. It only made the remaining nested/nonlocal blocker explicit enough to choose the next owner without guessing.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now promotes guard-family matching from a boolean-only helper to a traceable proof input:
  - `find_terminal_guard_family_match_excluding(...)`
- the nested proof paths now emit env-gated traces for both families:
  - `nested-terminal-join-proof ... entry_cond=... matched_cond=... result=...`
  - `nested-entry-guard-family-proof ... entry_cond=... matched_cond=... result=...`
- no acceptance policy changed:
  - generic nested suffix ownership is still fail-closed
  - the change is diagnostic only

Validation:

- `cargo test -p fission-pcode nested_terminal_join_tail -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the active blocker is now fixed precisely:
  - `nested-suffix-shape stmt_idx=142 kind=NestedSingleGotoThen`
- the proof failure is also explicit:
  - `nested-entry-guard-family-proof label=block_140007040 ref_stmt_idx=142 ... matched_cond=None result=false`
- previously recovered guard-family cases still show positive proof:
  - `nested-entry-guard-family-proof label=block_140007021 ... matched_cond=Some(...) result=true`
  - `nested-terminal-join-proof stmt_idx=120 ... matched_cond=Some(...) result=true`
  - `nested-terminal-join-proof stmt_idx=128 ... matched_cond=Some(...) result=true`
- the outer shell remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- the next owner is not a generic nested-tail acceptance patch
- the blocker is specifically a `NestedSingleGotoThen` into `block_140007040` with no matching guard-family witness in the current suffix window
- the next semantic wave should decide whether that guard family needs normalization or whether `block_140007040` remains an external owner boundary

### Guarded-tail known-pure helper call suffix internalization for `__popcount`

This wave kept the call-bearing suffix policy fail-closed for generic calls and only internalized the one traced safe subcase: a local binding assigned from `__popcount(...)` whose arguments are pure and whose result does not escape past the terminal join.

- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now narrows the active known-pure helper allowlist to:
  - `__popcount`
- guarded-tail suffix ownership now has a dedicated helper:
  - `suffix_known_pure_helper_call_is_owned_safe(...)`
- the new acceptance path is intentionally narrow:
  - `HirStmt::Assign { lhs: Var(_), rhs: Call { target: "__popcount", .. } }`
  - all call args must be pure
  - the assigned binding must not be redefined
  - all pre-terminal uses must stay in owned-safe contexts
  - all terminal-and-after uses must be zero
- focused synthetic coverage was added for:
  - suffix-local condition use
  - suffix-local pure expression use
  - unknown helper targets
  - nested call arguments
  - return-path escape
  - memory-visible alias risk
  - ignored-result call forms

Validation:

- `cargo test -p fission-pcode suffix_call_effect_shape_ -- --nocapture`
- `cargo test -p fission-pcode known_pure_helper_call -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed movement:

- `stmt_idx=138` is now internalized directly:
  - `suffix-known-pure-helper-call-internalized stmt_idx=138 kind=PureKnownHelperCall`
- the earlier candidate-35 blocker moved inward:
  - `early_label=block_14000701c first_fail=SuffixHasNestedOrNonlocalRef { stmt_idx: 142 }`
  - `early_label=block_140007021 first_fail=SuffixHasNestedOrNonlocalRef { stmt_idx: 142 }`
- the outer shell is still unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- the upstream unknown call stays fail-closed:
  - `suffix-call-effect-shape stmt_idx=58 kind=VoidUnknownCall`

Conclusion:

- the active owner is no longer the `__popcount` helper call at `stmt_idx=138`
- the blocker moved to the nested/nonlocal shape at `stmt_idx=142`
- this was a narrow suffix-owned safety closure, not a generic call-acceptance expansion

### Guarded-tail `promotion.rs` thin-façade refactor and module split

This wave was a behavior-preserving refactor. The goal was to stop using [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) as a single dumping ground for suffix ownership, replacement helpers, execute semantics, and local tests.

- [`mod.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/mod.rs) now declares the split sibling modules:
  - [`execution.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/execution.rs)
  - [`replacement.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/replacement.rs)
  - [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs)
- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) is now reduced to:
  - trace/diagnostic entrypoints
  - canonical rejection/telemetry mapping
  - top-level guarded-tail orchestration
- [`replacement.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/replacement.rs) now owns:
  - `ConditionAssumption`
  - read/def counting helpers
  - `replace_var_in_expr(...)`, `replace_var_in_stmt(...)`
  - guarded-tail else-source and read-kind classification helpers
- [`execution.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/execution.rs) now owns:
  - guarded-tail `trial -> verify -> execute`
  - exported binding collection
  - rewrite / replacement-plan construction
  - candidate discovery recursion
- [`suffix_window.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/suffix_window.rs) now owns:
  - suffix-window ownership diagnostics
  - owned-join narrowing
  - external-entry budget logic
  - nested/side-effect/call subtype classifiers
  - the moved helper-local test block

Validation:

- `cargo check -p fission-pcode`
- `cargo test -p fission-pcode suffix_side_effect_shape_ -- --nocapture`
- `cargo test -p fission-pcode suffix_call_effect_shape_ -- --nocapture`
- `cargo test -p fission-pcode memory_read_only_assign -- --nocapture`
- `cargo test -p fission-pcode nested_terminal_join_tail -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed parity:

- `stmt_idx=130` remains internalized:
  - `suffix-memory-readonly-assign-internalized stmt_idx=130 kind=MemoryReadOnlyAssign`
- `stmt_idx=138` remains classified the same:
  - `suffix-call-effect-shape stmt_idx=138 kind=PureKnownHelperCall`
- the live blocker shell is unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `early_label=block_14000701c first_fail=SuffixHasSideEffect { stmt_idx: 138 }`

Broader suite status:

- `cargo test -p fission-pcode structuring_guarded_tail -- --nocapture` remains red
- the failing area stays concentrated in the pre-existing alias/candidate-discovery family, so this refactor was not treated as a semantic acceptance wave

### Guarded-tail call side-effect shape subtyping for `stmt_idx=138`

This wave did not relax suffix ownership for calls. It only split the remaining `CallExprSideEffect` bucket into explicit call-effect families so the next owner can be chosen from a concrete call shape instead of a generic side-effect label.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `SuffixCallEffectShapeKind`
  - `classify_suffix_call_effect_shape(...)`
  - narrow target-family classifiers for:
    - known pure helpers
    - memory-mutating helpers
    - control-effect helpers
  - env-gated trace lines of the form:
    - `suffix-call-effect-shape stmt_idx=... kind=... stmt={:?}`
- focused synthetic coverage was added for:
  - `VoidUnknownCall`
  - `ReturnValueIgnoredCall`
  - `ReturnValueAssignedLocal`
  - `PureKnownHelperCall`
  - `MemoryMutatingCall`
  - `ControlEffectCall`
  - `UnknownCallEffect`

Validation:

- `cargo test -p fission-pcode suffix_call_effect_shape_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the current candidate-35 call blocker is now typed directly:
  - `suffix-call-effect-shape stmt_idx=138 kind=PureKnownHelperCall`
  - `stmt=Assign { lhs: Var("xVar124"), rhs: Call { target: "__popcount", args: [Var("xVar123")], ... } }`
- the earlier prefix blocker is also separated from it:
  - `suffix-call-effect-shape stmt_idx=58 kind=VoidUnknownCall`
  - `stmt=Expr(Call { target: "FUN_0x1400d23a0", args: [], ty: Unknown })`
- candidate 35 itself remains structurally unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- the earlier-label blocker for the active suffix window still lands on the same call site:
  - `early_label=block_14000701c first_fail=SuffixHasSideEffect { stmt_idx: 138 }`
  - `early_label=block_140007021 first_fail=SuffixHasSideEffect { stmt_idx: 138 }`

Conclusion:

- `CallExprSideEffect` is no longer an opaque blocker family
- the active call-bearing suffix owner is specifically a `PureKnownHelperCall` around `__popcount`, not an unknown mutating call
- the next wave should decide whether a narrow known-pure-helper call can be internalized under suffix-owned constraints, not broaden generic call acceptance

### Guarded-tail read-only load/assign suffix internalization for `stmt_idx=130`

This wave did not broaden generic side-effect acceptance. It only internalized one narrow suffix-owned subcase: a read-only load into a local variable can now remain inside the owned suffix window when its pointer is pure, its load type is known, and the resulting binding is only consumed in owned-safe contexts before the terminal join.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `suffix_memory_read_only_assign_is_owned_safe(...)`
  - `stmt_reads_binding_only_in_owned_safe_context(...)`
  - env-gated trace lines of the form:
    - `suffix-memory-readonly-assign-internalized stmt_idx=... kind=MemoryReadOnlyAssign stmt={:?}`
- focused synthetic coverage was added for:
  - pure `var = *ptr` with later condition use
  - pure `var = *(base + offset)` materialization
  - continued rejection for:
    - unknown load type
    - return-path reuse
    - pointer expressions containing calls
    - memory-visible alias-risk reuse

Validation:

- `cargo test -p fission-pcode memory_read_only_assign -- --nocapture`
- `cargo test -p fission-pcode suffix_side_effect_shape_ -- --nocapture`
- `cargo build -p fission-cli`
- `cargo test -p fission-automation`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`
- `cargo test -p fission-pcode structuring_guarded_tail -- --nocapture`
  - this broader suite remains red on `main` in the same guarded-tail discovery families and is not used as the wave acceptance signal yet

Observed state:

- `stmt_idx=130` is now internalized directly:
  - `suffix-memory-readonly-assign-internalized stmt_idx=130 kind=MemoryReadOnlyAssign`
- candidate 35 moves past the former blocker:
  - before: `early_label=block_14000701c first_fail=SuffixHasSideEffect { stmt_idx: 130 }`
  - after: `early_label=block_14000701c first_fail=SuffixHasSideEffect { stmt_idx: 138 }`
  - and `early_label=block_140007021` moves to the same `stmt_idx=138` blocker
- the next blocking side-effect family is now typed explicitly:
  - `suffix-side-effect-shape stmt_idx=138 kind=CallExprSideEffect`
- the outer candidate shell still remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- `MemoryReadOnlyAssign` is no longer the active owner for candidate 35
- the next owner is the call-bearing side-effect at `stmt_idx=138`, not generic load materialization
- the remaining top-level blocker is still the same broad candidate shell around `AliasNotFallthrough`, but this wave successfully removed one concrete suffix-owned load/assign obstacle without widening generic side-effect acceptance

### Guarded-tail suffix side-effect shape subtyping for `stmt_idx=130`

This wave did not broaden suffix ownership. It only split the remaining `SuffixHasSideEffect` bucket into explicit side-effect families so the next owner can be chosen from a concrete semantic shape instead of a generic side-effect label.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `SuffixSideEffectShapeKind`
  - `classify_suffix_side_effect_shape(...)`
  - env-gated trace lines of the form:
    - `suffix-side-effect-shape stmt_idx=... kind=... stmt={:?}`
- focused synthetic coverage was added for:
  - `MemoryReadOnlyAssign`
  - `CallExprSideEffect`
  - `MemoryWrite`
  - `VolatileOrUnknownLoad`

Validation:

- `cargo test -p fission-pcode suffix_side_effect_shape_ -- --nocapture`
- `cargo test -p fission-pcode nested_terminal_join_tail -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the current blocker for candidate 35 is now typed directly:
  - `suffix-side-effect-shape stmt_idx=130 kind=MemoryReadOnlyAssign`
  - `stmt=Assign { lhs: Var("xVar116"), rhs: Load { ptr: Var("xVar43"), ... } }`
- the earlier side-effect blocker is also split:
  - `suffix-side-effect-shape stmt_idx=58 kind=CallExprSideEffect`
- the candidate shell still remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- the active owner is no longer generic side-effect classification
- the next owner is a narrow read-only load/assign suffix segment around `stmt_idx=130`
- the next wave should evaluate whether `MemoryReadOnlyAssign` can be internalized as suffix-owned under alias-safe, non-volatile constraints, not broaden generic side-effect acceptance

### Guarded-tail nested terminal-join tail internalization for `stmt_idx=120`

This wave did not broaden generic nested conditional acceptance. It only internalized one narrow guarded-tail subtype: a single-branch nested `if` that jumps directly to the current terminal join can now be treated as suffix-owned when its guard belongs to the same terminal guard family already proven inside the suffix window.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `nested_terminal_join_tail_is_guard_family_owned_safe(...)`
  - `suffix_window_has_terminal_guard_family_match_excluding(...)`
  - env-gated trace lines of the form:
    - `nested-terminal-join-tail-internalized stmt_idx=... kind=... stmt={:?}`
- focused synthetic coverage was added for:
  - same-family `then -> goto terminal`
  - negated-family `else -> goto terminal`
  - continued rejection for:
    - different guard family
    - non-terminal target
    - non-empty else payload
    - side-effectful branch payload

Validation:

- `cargo test -p fission-pcode nested_terminal_join_tail -- --nocapture`
- `cargo test -p fission-pcode suffix_nested_shape_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the former blocker is now internalized directly:
  - `nested-terminal-join-tail-internalized stmt_idx=120 kind=NestedCrossesTerminalJoin`
- the same family also internalizes a second terminal-join-crossing nested tail:
  - `nested-terminal-join-tail-internalized stmt_idx=128 kind=NestedCrossesTerminalJoin`
- candidate 35 moved inward:
  - before: `early_label=block_14000701c first_fail=SuffixHasNestedOrNonlocalRef { stmt_idx: 120 }`
  - after: `early_label=block_14000701c first_fail=SuffixHasSideEffect { stmt_idx: 130 }`
  - and `early_label=block_140007021` moves to the same `stmt_idx=130` blocker
- the outer candidate shell still remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- terminal-join-crossing nested single-goto tails are no longer the active owner
- the next owner is the side-effect classification around `stmt_idx=130`, not nested guard-family ownership
- the next wave should focus on whether that load/assign segment is actually side-effectful for suffix ownership, not broaden nested terminal-join acceptance further

### Guarded-tail nested suffix shape subtyping for `stmt_idx=120`

This wave did not broaden nested guarded-tail acceptance. It only split the remaining `SuffixHasNestedOrNonlocalRef` bucket into explicit nested suffix-shape subtypes so the next owner can be chosen from a traced structural family instead of a generic nested/nonlocal label.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `NestedSuffixShapeKind`
  - `classify_nested_suffix_shape(...)`
  - env-gated trace lines of the form:
    - `nested-suffix-shape stmt_idx=... kind=... stmt={:?}`
- focused synthetic coverage was added for:
  - `NestedSingleGotoThen`
  - `NestedGuardFamilyMismatch`
  - `NestedCrossesTerminalJoin`

Validation:

- `cargo test -p fission-pcode suffix_nested_shape_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the remaining nested/nonlocal blocker for candidate 35 is now typed directly:
  - `nested-suffix-shape stmt_idx=120 kind=NestedCrossesTerminalJoin`
  - `stmt=If { cond: Unary { op: Not, expr: Var("xVar57"), ... }, then_body: [Goto("block_140007047")], else_body: [] }`
- the guard-family entry internalization remains active for `block_140007021`:
  - `guard_family_internalized_refs=1`
  - `effective_external=0`
- the outer blocker still remains unchanged:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- the active owner is no longer external-entry accounting or generic nested/nonlocal shape
- the remaining blocker is a narrow nested single-goto tail that crosses the terminal join
- the next wave should target terminal-join-crossing nested tail ownership directly, not broaden arbitrary nested conditional acceptance

### Guard-family nested conditional entry internalization for `block_140007021`

This wave did not broaden generic nested `if` acceptance. It only internalized a very narrow guarded-tail subfamily: a single-goto nested conditional entry can be subtracted from the suffix external-entry budget when it belongs to the same guard family as the target suffix's terminal-join guard.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `guard_family_internalized_refs` to `SuffixExternalEntryBudget`
  - guard-family matching helpers for:
    - single-goto nested entry probes
    - single-branch terminal-join guards in the target suffix
    - exact / negated guard-family equivalence
  - env-gated trace lines of the form:
    - `nested-entry-probe label=... cond=... ref_stmt_idx=... internalized=...`
    - `nested-entry-internalized label=... cond=... ref_stmt_idx=...`
- focused synthetic coverage was added for:
  - internalizing a same-family nested conditional entry
  - refusing a different guard-family nested entry

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the remaining `block_140007021` external entry is now internalized:
  - `nested-entry-probe label=block_140007021 cond=Var("xVar57") ref_stmt_idx=70 internalized=true`
  - `nested-entry-internalized label=block_140007021 cond=Var("xVar57") ref_stmt_idx=70`
  - `suffix-budget label=block_140007021 raw_refs=1 internal_refs=0 suffix_safe_refs=0 guard_family_internalized_refs=1 effective_external=0 allowed_external=0`
- for `candidate 35`, the earlier-label blocker moved:
  - before: `early_label=block_14000701c first_fail=SuffixHasExternalEntry { label: "block_140007021" }`
  - after: `early_label=block_14000701c first_fail=SuffixHasNestedOrNonlocalRef { stmt_idx: 120 }`
- the outer canonical blocker still remains:
  - `candidate=35`
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`

Conclusion:

- external-entry arithmetic for `block_140007021` is no longer the active owner
- the next owner is the nested/nonlocal `xVar57`-guard shape inside the suffix itself
- the next wave should target that guarded nested-tail ownership directly, not broaden nested-entry internalization again

### Guarded-tail external-entry kind diagnostics for candidate-35 probing

This wave did not relax `AliasNotFallthrough` or broaden guarded-tail acceptance. It only classified the remaining true external-entry ref shape after the suffix budget refinement proved that `block_140007021` was still externally entered for real.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `ExternalEntryRefKind`
  - `classify_external_entry_ref_kind_for_stmt(...)`
  - `classify_external_entry_ref_kind(...)`
  - env-gated trace lines of the form:
    - `suffix-external-entry label=...`
    - `external_entry_kind=...`
    - `ref_stmt_idx=...`
    - `ref_stmt={:?}`
- focused synthetic coverage was added for:
  - top-level external goto classification
  - nested conditional goto classification
  - loop/switch-derived goto classification
  - skipping candidate-internal top-level gotos so the first true external ref is reported

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo test -p fission-pcode external_entry_kind_ -- --nocapture`
- `cargo build -p fission-cli`
- `cargo test -p fission-automation`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- `candidate 35` still remains:
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- but the remaining true external-entry blocker for `block_140007021` is now typed:
  - `suffix-budget label=block_140007021 raw_refs=1 internal_refs=0 suffix_safe_refs=0 effective_external=1 allowed_external=0`
  - `suffix-external-entry label=block_140007021 external_entry_kind=NestedConditionalGoto ref_stmt_idx=70`
  - `ref_stmt=If { cond: Var("xVar57"), then_body: [Goto("block_140007021")], else_body: [] }`

Conclusion:

- the owner is no longer budget arithmetic for `block_140007021`
- the remaining blocker is a real nested conditional entry into the candidate tail
- the next wave should target nested/nonlocal external-entry ownership around that `xVar57`-guarded conditional, not another broad suffix-budget relaxation

### Guarded-tail suffix external-entry budget refinement for candidate-35 probing

This wave did not relax external entry in general. It only split the suffix external-entry budget into explicit categories so candidate-local top-level refs and self-terminal-join-safe refs can be subtracted before a label is classified as externally entered.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `SuffixExternalEntryBudget`
  - `count_candidate_internal_top_level_refs_in_suffix_window(...)`
  - `count_suffix_safe_self_terminal_refs_in_suffix_window(...)`
  - `compute_suffix_external_entry_budget(...)`
  - env-gated trace lines of the form:
    - `suffix-budget label=...`
    - `raw_refs=...`
    - `internal_refs=...`
    - `suffix_safe_refs=...`
    - `effective_external=...`
    - `allowed_external=...`
- focused coverage was added for:
  - candidate-internal top-level refs being counted as internal budget
  - nested candidate refs remaining external

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo build -p fission-cli`
- `cargo test -p fission-automation`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the budget trace now shows the exact external-entry decomposition for candidate 35
- for `block_14000701c`:
  - `raw_refs=1`
  - `internal_refs=0`
  - `suffix_safe_refs=1`
  - `effective_external=0`
  - `allowed_external=1`
- for the next blocker `block_140007021`:
  - `raw_refs=1`
  - `internal_refs=0`
  - `suffix_safe_refs=0`
  - `effective_external=1`
  - `allowed_external=0`
- candidate 35 still does not narrow:
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- but the result is now decisive: the remaining `block_140007021` external-entry failure is not caused by overcounting candidate-internal or self-terminal-safe refs

Conclusion:

- the external-entry budget is now explicit and traced
- `block_140007021` remains a true external-entry blocker under the current invariants
- the next owner is no longer budget arithmetic; it is the underlying nested/nonlocal entry shape around `block_140007021`

### Guarded-tail self-terminal-join suffix ownership closure for candidate-35 probing

This wave narrowed the next owner again. Instead of broadening generic nonterminal-goto acceptance, it only allowed a top-level suffix `Goto(target)` when `target` is the current terminal join label and the trailing segment up to the next label is proven to stay non-owned.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `suffix_stmt_is_terminal_join_owned_safe(...)`
  - a `suffix_is_nonowned_terminal_tail(...)` fast path for:
    - `target == current terminal join label`
    - unique terminal-join label definition
    - only ignorable / empty-block / pure-expr / pure-assign / same-terminal-join goto trailing payload
  - continued fail-closed rejection for:
    - nested/nonlocal trailing refs
    - side effects
    - loop / switch / break / continue crossings
    - non-terminal gotos that do not close to the current terminal join
- the same file now adds focused coverage for:
  - self-terminal-join goto with pure trailing payload
  - rejection when the trailing payload still contains nested control flow

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- synthetic self-terminal-join suffix cases now pass
- real `candidate 35` still does not narrow:
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- but the earlier-label blocker for `block_14000701c` moved:
  - before: `SuffixHasNonTerminalGoto { stmt_idx: 112, target: "block_140007047" }`
  - after: `SuffixHasExternalEntry { stmt_idx: 113, label: "block_140007021" }`

Conclusion:

- `self-terminal-join` closure is now proven and no longer the first blocker for `block_14000701c`
- the next owner is the external-entry budget around `block_140007021`, not generic goto closure
- `candidate 35` is still blocked by terminal-join ownership, but the blocker is now more specific

### Guarded-tail suffix nonterminal-goto redirect closure for candidate-35 probing

This wave targeted only `SuffixHasNonTerminalGoto`. It did not relax side effects, nested/nonlocal refs, or external-entry ownership. The guarded-tail suffix classifier now allows a very narrow redirect-closure subfamily: a top-level nonterminal `Goto(target)` may be treated as suffix-safe only when the target label is unique and its body closes through a trivial redirect chain to the next label or a terminal `return` sink.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `resolve_suffix_redirect_to_terminal(...)`
  - a `classify_suffix_stmt(...)` fast path that accepts only:
    - unique-label redirect chains
    - ignorable / empty-block / pure-assign / pure-expr gaps
    - a single trivial `goto` hop chain or terminal `return`
  - continued fail-closed rejection for:
    - side-effectful suffix payload
    - nested or nonlocal ref shapes
    - external-entry labels
    - loop / switch / break / continue crossings
- the same file adds focused coverage for:
  - `goto -> trivial label hop -> next_label`
  - `goto -> pure gap -> goto -> terminal return`
  - unchanged negative coverage for ambiguous target, external entry, nested refs, and loop/switch crossings

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo build -p fission-cli`
- `cargo test -p fission-automation`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- synthetic suffix redirect-closure cases now pass
- but real `candidate 35` does not move:
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
  - `block_14000701c` still reports `SuffixHasNonTerminalGoto { stmt_idx: 112, target: "block_140007047" }`
- this means the earlier-label blocker is not “redirect closure missing” in the trivial sense; the offending goto already closes to the current terminal join and still does not make the suffix non-owned

Conclusion:

- the new redirect-closure helper is correct but not sufficient for `candidate 35`
- the next owner is not generic nonterminal-goto acceptance
- the next owner is the larger ownership proof around the terminal-join suffix for `block_140007047`

### Guarded-tail suffix-safe rejection subtyping for `candidate 35`

This wave did not broaden acceptance. It split `suffix_is_nonowned_terminal_tail(...)` into a diagnostic `Result` contract so the candidate-35 owned-window probe can report *why* `suffix_safe=false` instead of only reporting that it failed.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - `SuffixTailRejection`
  - stmt-index-carrying first-failure diagnostics for suffix-tail classification
  - subtype helpers for nested/nonlocal ref, nonterminal goto, and per-statement suffix classification
  - candidate-35 env-gated trace lines of the form:
    - `candidate`
    - `join_label`
    - `early_label`
    - `first_fail`
    - `stmt_idx`
    - `first_fail_stmt`
- the same file now includes focused subtype coverage for:
  - ignorable / empty-block acceptance
  - side-effect rejection
  - nonterminal goto rejection
  - nested or nonlocal ref rejection
  - label crossing rejection
  - external entry rejection
  - loop / switch crossing rejection
  - unresolved alias redirect rejection

Validation:

- `cargo test -p fission-pcode suffix_ -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state for `candidate 35`:

- `join_label=block_140007047`
- `raw_middle_len=121`
- `first_reject=AliasNotFallthrough`
- earlier owned-join labels now resolve to concrete suffix-failure families:
  - `block_140007000` -> `SuffixHasSideEffect` at stmt `58`
  - `block_14000701c` -> `SuffixHasNonTerminalGoto` at stmt `112`
  - `block_140007021` -> `SuffixHasNestedOrNonlocalRef` at stmt `120`
  - `block_140007040` -> `SuffixHasExternalEntry` at stmt `150`

Interpretation:

- candidate-35 failure is now structurally decomposed
- the next owner is no longer “find an earlier join”
- the next owner is whichever of the above suffix-tail families should be tightened or re-owned first

Note:

- `cargo test -p fission-pcode structuring_guarded_tail -- --nocapture` is already red on current `main` and on a clean detached worktree from `b473b72`, so it was not used as a new regression signal for this subtype-only wave

### Guarded-tail earliest-owned-join narrowing wave - owner window probing is in place, and `candidate 35` is now explicitly blocked by `suffix_safe=false`

This wave targeted the remaining `0x140006fe0` bottleneck as an ownership-window problem, not as a broader acceptance relaxation. The guarded-tail owner now probes for an earlier top-level join label inside the current candidate window and only narrows the owned middle when the suffix up to the terminal join is proven to be a non-owned tail. The implementation stayed inside `fission-pcode`; downstream crates remain consume-only.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now adds:
  - earliest owned-join window probing before canonical guarded-tail middle slicing
  - `suffix_is_nonowned_terminal_tail(...)` and related owner-window checks
  - env-gated diagnostics for each earlier join candidate:
    - `payload_before`
    - `suffix_safe`
    - final `owned_join_narrowed` trace when a window can actually shrink
- the same file also adds synthetic unit coverage for:
  - sink-safe terminal tail acceptance
  - empty-block alias tails
  - alias-redirect-only suffixes
  - rejection on non-owned payload suffixes
  - rejection on external entry and nested suffix control flow

Validation:

- `cargo test -p fission-pcode structuring_guarded_tail -- --nocapture`
- `cargo test -p fission-pcode`
- `cargo test -p fission-automation`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=0x140006fe0 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile nir --ghidra-compat`

Observed state:

- the new owner-window probing is active
- but `candidate 35` still does not narrow:
  - `join_label=block_140007047`
  - `raw_middle_len=121`
  - `first_reject=AliasNotFallthrough`
- the new diagnostics show why:
  - `block_140007000`: `payload_before=true`, `suffix_safe=false`
  - `block_14000701c`: `payload_before=true`, `suffix_safe=false`
  - `block_140007021`: `payload_before=true`, `suffix_safe=false`
  - `block_140007040`: `payload_before=true`, `suffix_safe=false`

Diagnostic conclusion:

- the current blocker is no longer “do we have an earlier owned join candidate”
- the blocker is the proof for `suffix_is_nonowned_terminal_tail(...)`
- the next owner is the specific suffix-safe rejection family inside the candidate-35 window, not broader guarded-tail acceptance or generic materialization tuning

## 2026-04-09

### Guarded-tail join-glue bookkeeping for `effective_middle_refs`

Promotion and verification now treat a guarded-tail **middle** segment that contains only join glue (ignorable labels / empty blocks and `Goto(join_label)` hops) as having **no surviving middle references** for gate and replacement checks. This matches Ghidra-style join chains where multiple forward `Goto` hops are fallthrough-equivalent, not only a **trailing** suffix of duplicate `Goto`s.

- [`promotion_graph.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion_graph.rs): `middle_is_join_label_only_glue`, `effective_middle_refs_for_promotion`
- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs): unified use in `classify_must_emit_label_rejection` and witness verification
- [`structuring_guarded_tail.rs`](../../crates/fission-pcode/src/nir/tests/structuring_guarded_tail.rs): regression `structuring_candidate_discovery_join_glue_middle_elides_all_goto_refs`
- [`normalize/AGENTS.md`](../../crates/fission-pcode/src/nir/normalize/AGENTS.md): pass-ownership table (PHI vs GVN, IV vs for-loops, copy vs cleanup)
- [`builder/AGENTS.md`](../../crates/fission-pcode/src/nir/builder/AGENTS.md): builder scope and indirect-surface stats contract
- Removed stray `guarded_tail/*.bak` files

Validation:

- `cargo test -p fission-pcode`

Benchmark (2-way vs baseline): not run in this workspace because `samples/windows/x64/putty.exe` is not present in the repository clone. When the sample is available, use:

```bash
cargo build -p fission-cli --release
python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py \
  samples/windows/x64/putty.exe \
  --limit 50 \
  --fission-bin target/release/fission_cli \
  --output-dir artifacts/batch_benchmark/putty-guarded-tail-join-glue-<run-id> \
  --baseline-dir artifacts/batch_benchmark/putty-ghidra-guarded-tail-execute-wave-v8
```

## 2026-04-15 (latest)

### Guarded-tail execute migration wave - Ghidra-style descendant replacement and guarded-tail diagnostics are now core-owned, but the real `putty` blocker remains pre-promotion legality

This wave moved one more piece of guarded-tail ownership from “pretty shape promotion” to a Ghidra-style `ConditionalExecution` execute contract. The active work stayed inside `fission-pcode`: the guarded-tail owner now rewrites descendant reads from exported bindings, refuses fail-open merge synthesis, and emits explicit diagnostics for real-function rejection families. The semantic release owner did not move downstream.

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs) now:
  - rewrites descendant reads inside the promoted middle segment before splicing the final guarded-tail surface
  - rejects exported-binding promotion when the else-side replacement source is not domination-proven
  - removes the previous fail-open `else_value = Var(binding_name)` behavior for synthetic merge creation
  - adds env-gated guarded-tail diagnostics under `FISSION_PREVIEW_DIAG=1` so trial/verify rejection buckets are visible on real functions
- [`alias_refs.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/alias_refs.rs) broadens alias-forward-safe canonicalization to include:
  - pure `Assign` bindings
  - pure nested conditional forwarding
  - terminal tail-exit resolution for `goto -> return-only label` chains
- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs) now terminalizes return-only tail labels instead of rejecting them as `NestedTailEscape`, which moved the real `0x140006fe0` blocker one stage deeper into guarded-tail verification
- [`structuring_guarded_tail.rs`](../../crates/fission-pcode/src/nir/tests/structuring_guarded_tail.rs) adds regression coverage for:
  - descendant read rewrite with a dominating else-side source
  - fail-closed rejection when no dominating else-side source exists
  - pure top-level-after-label alias forwarding
  - terminal tail-exit canonicalization

Validation:

- `cargo test -p fission-pcode structuring_guarded_tail -- --nocapture`
- `cargo build -p fission-cli`
- `FISSION_PREVIEW_DIAG=1 FISSION_STRUCTURING_ENGINE=graph-collapse-v1 target/debug/fission_cli samples/windows/x64/putty.exe --decomp 0x140006fe0 --engine nir --profile balanced --ghidra-compat`
- `FISSION_STRUCTURING_ENGINE=graph-collapse-v1 python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --limit 50 --fission-bin target/debug/fission_cli --output-dir artifacts/batch_benchmark/putty-ghidra-guarded-tail-execute-wave-v8 --baseline-dir artifacts/batch_benchmark/putty-builder-provenance-wave`

Artifacts:

- [`putty-ghidra-guarded-tail-execute-wave-v7`](../../artifacts/batch_benchmark/putty-ghidra-guarded-tail-execute-wave-v7)
- [`putty-ghidra-guarded-tail-execute-wave-v8`](../../artifacts/batch_benchmark/putty-ghidra-guarded-tail-execute-wave-v8)

Observed quality state:

- synthetic guarded-tail execute regressions are now covered and passing
- `0x140006fe0` no longer stalls on exactly the same pre-candidate state:
  - `promotion_candidate_count: 0 -> 1`
  - `canonicalized_guarded_tail_shape_count: 0 -> 1`
  - `canonicalization_failed_nested_tail_escape: 9 -> 6`
  - `discovery_rejected_noncanonical_layout_count: 15 -> 12`
- but the real row still does not promote:
  - `guarded_tail_candidate_count: 0`
  - `guarded_tail_promoted_count: 0`
  - `guarded_tail_replacement_plan_completed_count: 0`
- targeted `putty` 50-function benchmark in [`putty-ghidra-guarded-tail-execute-wave-v8`](../../artifacts/batch_benchmark/putty-ghidra-guarded-tail-execute-wave-v8) remains below the accepted baseline:
  - `avg_normalized_similarity: 38.63`
  - `0x140006fe0: 33.97`
  - `0x140008900: 22.23`
  - `0x140008090: 35.28`

Diagnostic conclusion:

- the execute-layer synthetic merge / descendant rewrite semantics are now stricter and more Ghidra-like
- the remaining real blocker for `0x140006fe0` is not broad materialization anymore
- the next owner is the guarded-tail verification boundary around:
  - `effective_middle_refs`
  - `execution_safe`
  - `AliasNotFallthrough`
  - `AliasHasNonlocalRef`
- in other words, the next wave should target pre-promotion guarded-tail legality closure for the real `block_140007047` / `block_140007040` candidates, not downstream rendering

## 2026-04-14 (latest)

### Decompiler-core ownership cutover wave - `fission-decompiler-core` now owns orchestration while `fission-static` is reduced to facts/native services

This wave closes the architectural ownership drift that had left decompiler policy split across `fission-pcode`, `fission-static`, and `fission-decompiler-core`. The semantic owner stays in `fission-pcode`, but the application-layer decompile flow now lives in `fission-decompiler-core`, and `fission-static::analysis::decomp` has been reduced to facts/cache/prepare services only.

- [`lib.rs`](../../crates/fission-decompiler-core/src/lib.rs) now acts as the real decompiler entry layer rather than a facade. It re-exports the canonical orchestration surface and routes prebuilt-pcode selection through the core-owned request/result flow.
- [`request.rs`](../../crates/fission-decompiler-core/src/request.rs) adds the first explicit application contract:
  - `DecompileRequest`
  - `DecompileResult`
  - `decompile_prebuilt_pcode(...)`
- [`adapters.rs`](../../crates/fission-decompiler-core/src/adapters.rs) moves native backend adaptation into the core boundary through `NativeDecompilerBackend` and `NativeDecompilerSource`, removing the temporary CLI-owned adapter layer.
- [`facts.rs`](../../crates/fission-decompiler-core/src/facts.rs) moves NIR type-context construction and symbol sanitization into the decompiler owner. `fission-static` now provides raw `FactStore` data only.
- [`engine.rs`](../../crates/fission-decompiler-core/src/engine.rs), [`render.rs`](../../crates/fission-decompiler-core/src/render.rs), [`routing.rs`](../../crates/fission-decompiler-core/src/routing.rs), [`recovery.rs`](../../crates/fission-decompiler-core/src/recovery.rs), [`taxonomy.rs`](../../crates/fission-decompiler-core/src/taxonomy.rs), [`types.rs`](../../crates/fission-decompiler-core/src/types.rs), [`worker.rs`](../../crates/fission-decompiler-core/src/worker.rs), and [`postprocess.rs`](../../crates/fission-decompiler-core/src/postprocess.rs) now host the orchestration and downstream cleanup implementation that previously lived under `fission-static`.
- The full downstream cleanup stack was moved under [`crates/fission-decompiler-core/src/postprocess/`](../../crates/fission-decompiler-core/src/postprocess), and the old `fission-static` copies were deleted.
- [`mod.rs`](../../crates/fission-static/src/analysis/decomp/mod.rs) and [`facts.rs`](../../crates/fission-static/src/analysis/decomp/facts.rs) now leave `fission-static::analysis::decomp` with only:
  - cache primitives
  - raw fact ingestion and snapshots
  - native prepare helpers
  - `DecompilerNative` alias
- CLI/Tauri decompile-facing imports now consume `fission-decompiler-core` instead of `fission-static`, including:
  - [`fission_nir_worker.rs`](../../crates/fission-cli/src/bin/fission_nir_worker.rs)
  - [`fission_preview_worker.rs`](../../crates/fission-cli/src/bin/fission_preview_worker.rs)
  - [`decompile.rs`](../../crates/fission-cli/src/cli/oneshot/decompile.rs)
  - [`run.rs`](../../crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs)
  - [`decompile_render.rs`](../../crates/fission-cli/src/cli/oneshot/decompile/decompile_render.rs)
  - [`build.rs`](../../crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/build.rs)
  - [`emit.rs`](../../crates/fission-cli/src/cli/oneshot/inventory/emit.rs)
  - [`assembly.rs`](../../crates/fission-tauri/src-tauri/src/commands/analysis/assembly.rs)

Validation:

- `cargo check -p fission-static`
- `cargo check -p fission-decompiler-core`
- `cargo check -p fission-cli`
- `cargo check -p fission-tauri`
- `cargo test -p fission-decompiler-core`
- `cargo test -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`

Targeted benchmark artifact:

- [`putty-decompiler-core-cutover-wave-v2`](../../artifacts/batch_benchmark/putty-decompiler-core-cutover-wave-v2)

Observed quality state after the structural cutover:

- seeded shared coverage: `100.00%`
- independent top-N coverage: `96.00%`
- direct-success: `50/50`

## 2026-04-17

### Forward-chain alias canonicalization for external top-level-goto-only + top-level-after-label local refs

This patch extends alias label canonicalization to support forward-chain terminal-join resolution when both conditions hold:
1. Local alias refs are **top-level-after-label** only (post-segment forward gotos)
2. External refs are **top-level Goto only** (no nested refs, no pre-segment refs)

The redirect target is now resolved to the terminal join label in the forward chain (rather than only the immediate next label), reusing `resolve_terminal_join_target()` and trivial segment checks from `promotion_graph.rs`. This follows the same "forward chain → terminal join" semantics that promotion already uses for guarded-tail sequence folding.

**Targets**: Preparation for safe alias-chain expansion targeting `0x140006fe0` (putty.exe) in the next patch.

**Non-targets** (explicitly rejected to maintain soundness): 
- nested-after-label cases (like `0x140008090`) remain rejected
- pre-segment nested/nonlocal refs remain rejected
- `payload_crosses_join` remains forbidden
- `nested_tail_escape` heuristics unchanged

**Implementation**:

- [`alias_refs.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/alias_refs.rs): 
  - New `are_all_external_refs_top_level_goto()` helper to verify external refs are pure top-level gotos
- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs):
  - Reordered redirect logic: forward-chain resolution now attempted **before** immediate-next redirect when top-level-after-label + external-top-level-goto conditions align
  - Preserved immediate-next redirect as fallback for cases that don't qualify for forward-chain
  - Reuse of `resolve_terminal_join_target()` ensures determinism and alignment with promotion semantics

**Validation**:

- `cargo test -p fission-pcode` (412 tests, all pass)
- `cargo test guarded_tail` (60/60 tests, all pass)

**Key design decisions**:

1. **Priority**: Forward-chain over immediate-next when external top-level-goto is present, ensuring we don't leave legal redirection opportunities on the table for the target use case.
2. **Reuse**: No new state machines or heuristics; borrowing promotion_graph's proven cycle-safe chain traversal and segment classification.
3. **Safety**: Nested refs or pre-segment refs still trigger `AliasHasNonlocalRef` rejection; this patch only widens the **top-level-to-top-level** path.

### Guarded-tail nested-tail terminal-safe subcase (3rd wave)

This wave tightens the guarded-tail `NestedTailEscape` handling around one limited acceptance class: payload-following tail exits that can be proven terminal by a unique label chain to `return`.

Implementation:

- [`alias_refs.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/alias_refs.rs)
  - expanded `resolve_terminal_tail_exit_stmt(...)` to accept only terminal-safe hops:
    - ignorable statements
    - pure expr / pure var-assign gap statements
    - unique terminal `goto` chain ending in `return`
  - preserved safety guardrails:
    - no cycle
    - no external re-entry into hop labels (single predecessor invariant)
    - no loop/switch/break/continue crossing
- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - reordered payload-following `goto` handling to attempt terminal-tail resolution before immediate `NestedTailEscape` rejection on trailing non-ignorable statements
  - forward-chain alias redirect guard now rejects self-redirect (`resolved_label != label`)
- [`structuring_guarded_tail.rs`](../../crates/fission-pcode/src/nir/tests/structuring_guarded_tail.rs)
  - added nested-tail subcase regressions (positive 2, negative 4)
  - restored forward-chain alias behavior guard tests (positive 1, negative 1)

Validation:

- `cargo test -p fission-pcode guarded_tail` → 68 passed
- `cargo test -p fission-pcode --lib` → 420 passed

Targeted benchmark rerun:

- artifact: [`putty-nested-tail-wave-20260417`](../../artifacts/batch_benchmark/putty-nested-tail-wave-20260417)
- target row `0x140006fe0` counters (vs prior [`putty-forward-chain-rerun-20260417`](../../artifacts/batch_benchmark/putty-forward-chain-rerun-20260417)):
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3 -> 3`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2 -> 2`
  - `canonicalization_failed_nested_tail_escape`: `6 -> 9`
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4 -> 4`
  - `region_emit_ready_failed_count`: `5 -> 5`

Result interpretation:

- The new terminal-safe nested-tail path is now covered by dedicated regressions and remains bounded by structural proof constraints.
- On the target row (`0x140006fe0`), this wave did not reduce emit-ready blockers yet; follow-up narrowing is still needed on the concrete failing tail pattern.

### Guarded-tail duplicate guard-ladder collapse before canonicalization (4th wave)

This wave adds a narrow guarded-tail pre-normalization step to collapse duplicate top-level conditional-goto ladders before canonicalization.

Implementation:

- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - added `collapse_duplicate_top_level_guard_ladder(...)`
  - applied collapse to flattened guarded-tail segment before trimming/canonicalization
  - collapse conditions are intentionally strict:
    - both statements are top-level `If`
    - both have empty `else`
    - both `then_body` are single `Goto` to the same label
    - condition AST is identical
    - only empty `Block` gap is allowed between duplicates
    - no nested/loop/switch crossing
- same file test module:
  - positive: identical cond+target collapse, deref-guard collapse, empty-block-gap collapse
  - negative: different cond, different target, non-ignorable gap, nested-loop body no-touch

Validation:

- `cargo test -p fission-pcode collapse_duplicate_guard_ladder_` → 7 passed
- `cargo test -p fission-pcode guarded_tail` → 75 passed
- `cargo test -p fission-pcode --lib` → 427 passed

Targeted benchmark rerun:

- artifact: [`putty-duplicate-guard-wave-20260417`](../../artifacts/batch_benchmark/putty-duplicate-guard-wave-20260417)
- target row `0x140006fe0` counters:
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3` (no change)
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2` (no change)
  - `canonicalization_failed_nested_tail_escape`: `9 -> 7` (improved vs 3rd-wave run)
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4` (no change)
  - `region_emit_ready_failed_count`: `5` (no change)

Observed target HIR status:

- repeated guard pairs around `block_140007021` remained duplicated in this run output (`if (!xVar57) ...` x2, `if (!*xVar43) ...` x2), so the new collapse path is not yet hitting that concrete failing shape end-to-end.

### Guarded-tail sink-to-return goto-chain collapse before canonicalization (5th wave)

This wave adds a narrow sink-directed pre-normalization in guarded-tail canonicalization: top-level `goto` edges that provably flow through a terminal-safe label chain to `return` are collapsed early.

Implementation:

- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - added `collapse_top_level_sink_to_return_goto_chain(...)`
  - inserted ordering in guarded-tail canonicalization:
    - duplicate guard collapse
    - sink-to-return goto-chain collapse
    - alias/nested-tail canonicalization
  - applied strict scope/bounds:
    - only top-level `goto` statements in guard-only prefix
    - target label must have a single definition in the enclosing body
    - terminal proof is delegated to existing `resolve_terminal_tail_exit_stmt(...)`
    - inherited guardrails from terminal proof: unique predecessor/no re-entry, ignorable/pure gap only, no loop/switch/break/continue crossing, no cycles
- same file test module:
  - positive: direct `goto -> return` sink, pure-gap hop chain to return sink
  - negative: re-entry, ambiguous sink label ownership, side-effectful gap, loop crossing

Validation:

- `cargo test -p fission-pcode collapse_sink_to_return_chain_` → 6 passed
- `cargo test -p fission-pcode guarded_tail` → 81 passed
- `cargo test -p fission-pcode --lib` → 433 passed

Targeted benchmark rerun:

- artifact: [`putty-sink-return-wave-20260417`](../../artifacts/batch_benchmark/putty-sink-return-wave-20260417)
- target row `0x140006fe0` counters (vs 4th-wave [`putty-duplicate-guard-wave-20260417`](../../artifacts/batch_benchmark/putty-duplicate-guard-wave-20260417)):
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3 -> 3`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2 -> 2`
  - `canonicalization_failed_nested_tail_escape`: `7 -> 7`
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4 -> 4`
  - `region_emit_ready_failed_count`: `5 -> 5`

Observed target HIR status:

- output window around `block_140007021` remains unchanged from 4th-wave run (duplicate guard pairs preserved; sink label still emitted as `block_140007047: return`).

Result interpretation:

- 5th-wave sink-chain rule is now implemented with strict proof boundaries and covered by dedicated positive/negative tests.
- On the concrete blocker row (`0x140006fe0`), this bounded subcase did not move the remaining blocker counters yet; the failing shape is still outside the currently covered guarded-tail segment/path.

### Guarded-tail duplicate conditional cluster factoring across sink-safe trivial gaps (6th wave)

This wave extends 4th-wave duplicate-guard folding by adding a narrowly bounded cluster factoring step: identical top-level guard families may fold across trivial gaps only.

Pre-check on target row `0x140006fe0`:

- duplicate `if (!xVar57) goto block_140007047;` pair has no meaningful gap (adjacent; only brace lines between textual statements)
- duplicate `if (!*xVar43) goto block_140007040;` pair also has no meaningful gap (adjacent; only brace lines between textual statements)

Implementation:

- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - added `factor_duplicate_top_level_guard_cluster_with_trivial_gap(...)`
  - inserted ordering in guarded-tail canonicalization:
    - duplicate guard collapse
    - duplicate guard-cluster factoring across trivial gap
    - sink-to-return goto-chain collapse
    - alias/nested-tail canonicalization
  - factoring rules are intentionally narrow:
    - top-level only
    - identical condition AST + identical goto target
    - allowed gaps: ignorable, empty block, or sink-safe top-level goto proven by terminal-tail resolver
    - forbidden: label crossing, side-effectful gaps, ambiguous sink label ownership, loop/switch/control crossing
- same file test module:
  - positive: sink-safe goto gap factoring, empty-block + sink-safe mixed gap factoring
  - negative: side-effectful gap, ambiguous sink, label crossing, loop crossing

Validation:

- `cargo test -p fission-pcode collapse_guard_cluster_` → 6 passed
- `cargo test -p fission-pcode collapse_sink_to_return_chain_` → 6 passed
- `cargo test -p fission-pcode guarded_tail` → 87 passed
- `cargo test -p fission-pcode --lib` → 439 passed

Targeted benchmark rerun:

- artifact: [`putty-guard-cluster-wave-20260417`](../../artifacts/batch_benchmark/putty-guard-cluster-wave-20260417)
- target row `0x140006fe0` counters (vs 5th-wave [`putty-sink-return-wave-20260417`](../../artifacts/batch_benchmark/putty-sink-return-wave-20260417)):
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3 -> 3`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2 -> 2`
  - `canonicalization_failed_nested_tail_escape`: `7 -> 7`
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4 -> 4`
  - `region_emit_ready_failed_count`: `5 -> 5`

Observed target HIR status:

- output window around `block_140007021` remains unchanged from 5th-wave run (duplicate guard pairs preserved; helper/sink structure unchanged).

Result interpretation:

- 6th-wave cluster factoring is soundly integrated and regression-covered.
- On the concrete blocker row (`0x140006fe0`), the extra trivial-gap factoring still does not move counters or emitted shape, indicating the remaining blocker likely sits before guarded-tail candidate shaping (or outside the currently canonicalized segment boundary).

### Guarded-tail first-reject trace patch (7th wave, env-gated single function)

This wave adds diagnostics only (no semantic acceptance broadening): env-gated candidate trace for one function to capture first failing shape and snapshot.

Instrumentation scope (all env-gated via `FISSION_PREVIEW_DIAG_ADDR=0x...`):

- [`promotion.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/promotion.rs)
  - `try_build_guarded_tail_witness(...)`: candidate header (`idx`, `join_label`, `label_idx`, raw middle length) and first reject snapshot (20 statements)
  - `verify_guarded_tail_trial(...)`: first verification-stage reject reason + snapshot
  - `mark_guarded_tail_canonicalization_failure(...)`: canonicalization failure reason log hook
- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - `canonicalize_guarded_tail_segment(...)`: flatten pre/post lengths, trim bounds, and collapse counters
    - duplicate-guard collapse
    - guard-cluster factoring
    - sink-return collapse
  - `canonicalize_interleaved_local_aliases(...)`: alias redirect candidate label + resolved target tracing

Validation:

- `cargo test -p fission-pcode guarded_tail` → 87 passed
- `cargo test -p fission-pcode --lib` → 439 passed

Targeted benchmark rerun (trace enabled):

- artifact: [`putty-guarded-tail-trace-wave-20260418`](../../artifacts/batch_benchmark/putty-guarded-tail-trace-wave-20260418)
- trace file: [`fission_stderr.log`](../../artifacts/batch_benchmark/putty-guarded-tail-trace-wave-20260418/fission_stderr.log)
- target row `0x140006fe0` counters (vs 6th-wave [`putty-guard-cluster-wave-20260417`](../../artifacts/batch_benchmark/putty-guard-cluster-wave-20260417)):
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3 -> 3`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2 -> 2`
  - `canonicalization_failed_nested_tail_escape`: `7 -> 7`
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4 -> 4`
  - `region_emit_ready_failed_count`: `5 -> 5`

First-reject trace highlights for `0x140006fe0` (same pattern observed in discovery + promotion passes):

- `candidate=35`, `join_label=block_140007047`, `label_idx=157`
  - first reject: `AliasNotFallthrough`
  - raw middle length: `121`
- `candidate=70`, `join_label=block_140007021`, `label_idx=113`
  - first reject: `AliasHasNonlocalRef`
  - canonicalize stats: `flatten_before=42`, `flatten_after=42`, `collapse_dup=0`, `cluster=0`, `sink=0`
- `candidate=120/128/149`, `join_label=block_140007047`
  - first reject: `NestedTailEscape`
  - canonicalize collapse counters remained `0`
- `candidate=142/145`, `join_label=block_140007040`
  - first reject: `MustEmitLabelConflict` (including `MustEmitLabelSurvivingExternalRef`)

Result interpretation:

- 7th-wave confirms the bottleneck is first-failing candidate classification/ownership, not missing local canonicalization transforms.
- highest-priority failing shape is now concretely captured (`AliasNotFallthrough` on a large middle segment), which is the anchor for the next invariant-based shape-specific patch.

### Guarded-tail sink-equivalent after-label refs for AliasNotFallthrough (8th wave)

This wave targets only candidate-35-class `AliasNotFallthrough` by subtracting provably sink-equivalent local top-level after-label references from the reject basis.

Scope/constraints kept strict:

- local / top-level goto only (nested after-label refs stay rejected)
- unique label ownership only (ambiguous label targets remain rejected)
- same terminal return sink proof only (via existing terminal tail resolver)
- trivial gap only (`ignorable`, empty block, sink-safe goto->return hop)
- side-effectful and control-crossing gaps remain forbidden
- no nonlocal ownership relaxation (`external_ref_count > 0` still excluded)

Implementation:

- [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs)
  - added helper trio:
    - `stmt_is_sink_equivalent_after_label_gap(...)`
    - `local_after_label_ref_is_sink_equivalent(...)`
    - `count_sink_equivalent_top_level_after_label_refs(...)`
  - rewired `AliasNotFallthrough` gate to use:
    - `effective_top_level_after_label_count = raw_top_level_after_label_count - sink_equivalent_top_level_after_label_count`
  - reject accounting now increments `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count` by effective (non-sink-equivalent) residual only
  - added trace line when sink-equivalent subtraction is applied:
    - `[GT-TRACE] ... alias_after_sink_equiv ...`

Tests:

- new focused tests in [`canonicalize.rs`](../../crates/fission-pcode/src/nir/structuring/guarded_tail/canonicalize.rs):
  - positive: same-sink after-label ref; empty/sink-safe gap case
  - negative: nested after-ref, side-effectful gap, ambiguous sink target, re-entry, label-crossing/non-sink target, external-ownership change, different terminal sink
- validation:
  - `cargo test -p fission-pcode sink_equivalent_after_label_ref_` -> 9 passed
  - `cargo test -p fission-pcode guarded_tail` -> 96 passed
  - `cargo test -p fission-pcode --lib` -> 448 passed

Targeted benchmark rerun:

- artifact: [`putty-alias-sink-eq-wave-20260418`](../../artifacts/batch_benchmark/putty-alias-sink-eq-wave-20260418)
- trace file: [`fission_stderr.log`](../../artifacts/batch_benchmark/putty-alias-sink-eq-wave-20260418/fission_stderr.log)
- target row `0x140006fe0` counters (vs 7th-wave [`putty-guarded-tail-trace-wave-20260418`](../../artifacts/batch_benchmark/putty-guarded-tail-trace-wave-20260418)):
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `3 -> 3`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`: `2 -> 2`
  - `canonicalization_failed_nested_tail_escape`: `7 -> 7`
  - `guarded_tail_rejected_alias_interleave_conflict_count`: `4 -> 4`
  - `region_emit_ready_failed_count`: `5 -> 5`

Candidate trace outcome (`0x140006fe0`):

- `candidate=35`, `join_label=block_140007047`, `raw_middle_len=121` remains first reject `AliasNotFallthrough`
- no `alias_after_sink_equiv` trace line was emitted for this candidate in this sample, meaning sink-equivalent subtractable local after-label refs were not proven under current constraints

Result interpretation:

- 8th-wave rule is integrated and regression-covered, but does not trigger on the current blocker row.
- next narrowing step should focus candidate-35 segment ownership/window proof (not broadening `AliasHasNonlocalRef`, `NestedTailEscape`, or `MustEmitLabelConflict`).
- `unsupported_indirect_control_count`: `9`
- `avg_normalized_similarity`: `37.09`
- key rows:
  - `0x140001160`: `32.39`
  - `0x140008900`: `23.97`
  - `0x140007da0`: `34.60`
  - `0x140008090`: `35.28`

Prior note from earlier cutover wave (`nir-check`):

- `changed_rows=0`
- dominant slow passes:
  - `sccp: 246.5ms`
  - `jump_resolver: 27.9ms`
  - `aggregate_fields: 23.7ms`
  - `memory_slot_surfacing_full: 21.6ms`

Prior architectural decision/result:

- architectural ownership closure is complete for the decompiler application layer
- `fission-static` no longer owns decompiler orchestration or postprocess implementation
- semantic acceptance is still not met versus the accepted `putty-builder-provenance-wave` baseline, so this wave should be read as a structural cutover, not a semantic release win

## 2026-04-12 (latest)

### Builder-provenance stabilization wave - producer-owned materialization now recovers the row-fidelity canaries without reopening indirect-control drift

This wave moved the row-fidelity fix back to the canonical owner. Instead of trying to paper over degraded rendered expressions in late cleanup or consumer layers, `fission-pcode` now preserves builder-selected stable representatives, propagates that preservation through normalization, and keeps CLI/static active paths on canonical indirect-control payloads only.

- [`types.rs`](crates/fission-pcode/src/nir/types.rs) adds `NirBindingOrigin::TempPreserved` plus helper predicates so preserved materialization is a first-class canonical contract rather than a name-only convention.
- [`mod.rs`](crates/fission-pcode/src/nir/builder/mod.rs), [`materialize.rs`](crates/fission-pcode/src/nir/builder/materialize.rs), [`state.rs`](crates/fission-pcode/src/nir/builder/state.rs), [`init.rs`](crates/fission-pcode/src/nir/builder/init.rs), and [`stats.rs`](crates/fission-pcode/src/nir/builder/stats.rs) now mark nontrivial builder-owned representatives as preserved and report `materialization_stabilized_count` from the producer owner.
- [`passes.rs`](crates/fission-pcode/src/nir/normalize/cleanup/passes.rs) and [`run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now honor preserved materialization during cleanup instead of re-inlining nontrivial predicate-carried temps on a pure single-use heuristic.
- [`defuse.rs`](crates/fission-pcode/src/nir/normalize/analysis/defuse.rs), [`phi_recovery.rs`](crates/fission-pcode/src/nir/normalize/recovery/phi_recovery.rs), [`call_artifact.rs`](crates/fission-pcode/src/nir/normalize/idioms/call_artifact.rs), [`slots.rs`](crates/fission-pcode/src/nir/normalize/memory/slots.rs), and [`typed_facts.rs`](crates/fission-pcode/src/nir/normalize/memory/typed_facts.rs) now treat `TempPreserved` as temp-like where required, keeping the analysis contract aligned with builder ownership.
- [`terminator.rs`](crates/fission-pcode/src/nir/builder/terminator.rs) now preserves duplicate ordinal successors in `BranchInd` lowering so many-to-one dispatcher surfaces no longer collapse away ordinal case information before structuring.
- [`build.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/build.rs), [`summary.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/summary.rs), [`routing.rs`](crates/fission-static/src/analysis/decomp/nir/routing.rs), and [`render.rs`](crates/fission-static/src/analysis/decomp/nir/render.rs) now stay on canonical `NirBuildStats` / `IndirectControlClassification` instead of rebuilding active-path indirect semantics from raw observations.
- [`cleanup.rs`](crates/fission-pcode/src/nir/structuring/cleanup.rs) and [`normalize_defuse.rs`](crates/fission-pcode/src/nir/tests/normalize_defuse.rs) gained regression coverage around forward-goto residue cleanup, duplicate successor case ordinals, and builder-preserved predicate temps.

Accepted benchmark artifact:

- [`putty-builder-provenance-wave`](../../artifacts/batch_benchmark/putty-builder-provenance-wave)

Accepted benchmark delta vs the prior accepted baseline [`putty-row-fidelity-wave-v8`](../../artifacts/batch_benchmark/putty-row-fidelity-wave-v8):

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- public direct-success: `50/50 -> 50/50`
- `avg_normalized_similarity`: `38.79 -> 38.82`
- public indirect counters:
  - `unsupported_indirect_control_count`: `1 -> 1`
  - `indirect_surface_preserved_count`: `9 -> 9`
  - `dispatcher_shape_recovered_count`: `12 -> 12`
  - `dispatcher_proof_completed_count`: `4 -> 4`
  - `proof_payload_direct_emit_count`: `8 -> 8`
- builder/preservation counters:
  - `materialization_stabilized_count`: surfaced across canary rows and benchmark summary
  - `sccp_skipped_by_admission_count`: `20`
  - `memory_fact_prefilter_skip_count`: `18`
  - `pass_rerun_skipped_by_preservation_count`: `49`
- key rows:
  - `0x140001160`: `27.34 -> 31.61`
  - `0x140008900`: `20.68 -> 23.62`
  - `0x140007da0`: `34.45 -> 34.54`
  - `0x140008090`: `35.41 -> 35.63`
  - `0x140006ef0`: `35.33 -> 35.33`
  - `0x140006c20`: `37.66 -> 40.52`
  - `0x140006fe0`: `34.76 -> 34.76`

Validation:

- `cargo test -p fission-pcode`
- `cargo check -p fission-static`
- `cargo check -p fission-cli`
- `cargo test -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
- `cargo build -p fission-cli --release`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --fission-bin target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC --output-dir artifacts/batch_benchmark/putty-builder-provenance-wave --baseline-dir artifacts/batch_benchmark/putty-row-fidelity-wave-v8 --limit 50`

`nir-check` fast lane stayed non-worse:

- `changed_rows=0`
- gate remains `stop_hold_p5h3f`
- dominant slow passes:
  - `sccp: 226.7ms`
  - `jump_resolver: 26.0ms`
  - `aggregate_fields: 21.7ms`
  - `memory_slot_surfacing_full: 19.5ms`
  - `cleanup_elim_7: 12.4ms`

Duplicate-logic audit result:

- active CLI/static consumer paths no longer rebuild indirect semantics from raw pcode/flags; the active paths consume canonical `IndirectControlClassification` / `NirBuildStats`
- CLI inventory, NIR candidate build/summary, static routing, and static render all stay on the canonical payload
- remaining preservation follow-up is producer-side hardening only:
  - preserved-temp awareness in some cleanup pruning/collapse helpers
  - preserved-temp exclusion in copy propagation
  - `gvn_join` hoists still emit plain `Temp` instead of `TempPreserved`

Next bottleneck:

- the next primary KPI should stay on producer-owned row fidelity, not broader dispatcher recovery
- specifically:
  - finish preserved-materialization hardening in cleanup / copy propagation / `gvn_join`
  - keep the new canary gains while preventing future alias/materialization regressions
  - only after that, move primary effort to pure perf on `sccp`, `jump_resolver`, `aggregate_fields`, and `memory_slot_surfacing_full`

### Row-fidelity stabilization wave - accepted dispatcher recovery now clears the `0x140007da0` canary without regressing the dispatcher targets

This wave closed the remaining release blocker from the accepted dispatcher-recovery branch. The canonical fix stayed in `fission-pcode`: instead of widening dispatcher proof yet again, the work stabilized rendered slot/materialization choices so zero-offset slot aliases no longer surface through naked synthetic temp bases such as `xVar203`, while nonzero-offset and already-proven slot surfaces continue to survive.

- [`slots.rs`](crates/fission-pcode/src/nir/normalize/memory/slots.rs) now fail-closes zero-offset slot surfacing when the recovered display base is only a naked synthetic temp with no stable provenance, while preserving the existing deterministic first-use alias ordering and final binding-name ordering.
- [`normalize_slots.rs`](crates/fission-pcode/src/nir/tests/normalize_slots.rs) now locks in the new contract:
  - deterministic alias order still holds for stable bases
  - direct/body alias provenance still rewrites slot initializers back to source-like bases
  - naked synthetic temp bases at zero offset do not get surfaced as synthetic slot locals
- The accepted output for `0x140007da0` no longer emits `slot_0_1 = (uchar *)xVar203;`; the unresolved pointer stays explicit as `xVar203`, which is less pretty locally but materially improves row-level similarity and removes the unstable fake slot alias from the rendered surface.

Accepted benchmark artifact:

- [`putty-row-fidelity-wave-v8`](../../artifacts/batch_benchmark/putty-row-fidelity-wave-v8)

Accepted benchmark delta vs the prior accepted baseline [`putty-proof-first-wave-final`](../../artifacts/batch_benchmark/putty-proof-first-wave-final):

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- public direct-success: `50/50 -> 50/50`
- `avg_normalized_similarity`: `37.91 -> 38.79`
- public indirect counters:
  - `unsupported_indirect_control_count`: `11 -> 1`
  - `indirect_surface_preserved_count`: `18 -> 9`
  - `dispatcher_shape_recovered_count`: `1 -> 12`
- key rows:
  - `0x140001160`: `27.05 -> 27.34`
  - `0x140008900`: `19.55 -> 20.68`
  - `0x140007da0`: `34.43 -> 34.45`
  - `0x140008090`: `33.62 -> 35.41`
  - `0x140006ef0`: `35.33 -> 35.33`

Validation:

- `cargo test -p fission-pcode`
- `cargo check -p fission-static`
- `cargo check -p fission-cli`
- `cargo test -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
- `cargo build -p fission-cli --release`
- `python3 artifacts/batch_benchmark_scripts/full_decomp_benchmark.py samples/windows/x64/putty.exe --fission-bin target/release/fission_cli --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC --output-dir artifacts/batch_benchmark/putty-row-fidelity-wave-v8 --limit 50`

`nir-check` fast lane stayed non-worse:

- `changed_rows=0`
- gate remains `stop_hold_p5h3f`
- dominant slow passes moved to:
  - `sccp: 241.5ms`
  - `jump_resolver: 28.3ms`
  - `aggregate_fields: 23.4ms`
  - `memory_slot_surfacing_full: 21.7ms`
  - `cleanup_elim_7: 13.6ms`

Duplicate-logic audit result:

- active CLI/static/automation consumer paths no longer rebuild indirect semantics through `IndirectControlClassification::from_flags(...)`
- no active consumer-side `IndirectControlClassification::from_pcode(...)` rebuild remains in CLI/static/automation
- remaining `from_pcode`/`from_flags` matches are canonical producer definitions in [`types.rs`](crates/fission-pcode/src/nir/types.rs) or unrelated pcode/CFG construction entrypoints, not downstream semantic rebuild sites

Next bottleneck:

- dispatcher proof is no longer the release blocker; the accepted branch now meets the row-fidelity gates
- the next primary KPI should move to pass-manager/perf work, specifically:
  - `sccp`
  - `copy_propagation_after_cse`
  - `cleanup_standalone_12`
  - preserving the current dispatcher/indirect metrics while reducing fast-lane cost

## 2026-04-11

### Proof-first rust-sleigh lift extension wave - giant indirect functions now survive past `BranchInd`, with fast-lane cleanup gated back under control

This wave fixed the main hidden bottleneck behind the stalled dispatcher work: the Rust sleigh path was still treating `BranchInd` as a hard function terminator, so large dispatcher-heavy bodies such as `0x140001160` were being truncated before `fission-pcode` ever had a chance to reason about them. The canonical fix stays in the Rust-owned pipeline:

- [`mod.rs`](crates/fission-sleigh/src/lifter/mod.rs) and [`mod.rs`](crates/fission-sleigh/src/lifter/backend/mod.rs) now expose `LiftDecodeContract`, separating strict function slicing from decomp-oriented lift contracts that continue past `BranchInd`.
- [`lib.rs`](crates/fission-decompiler-core/src/lib.rs) now uses the decomp lift contract by default, raises the effective instruction budget for bounded full-body decode, and retries with the old strict stop only when the expanded lift hits unsupported render patterns.
- [`emit.rs`](crates/fission-cli/src/cli/oneshot/inventory/emit.rs) and [`run.rs`](crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs) now consume the same decomp lift contract so benchmark, inventory, and direct decompile paths stop disagreeing on how much function body rust-sleigh is allowed to see.
- [`types.rs`](crates/fission-pcode/src/nir/types.rs), [`routing.rs`](crates/fission-static/src/analysis/decomp/nir/routing.rs), [`render.rs`](crates/fission-static/src/analysis/decomp/nir/render.rs), and [`summary.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/summary.rs) now use the canonical `IndirectControlClassification::from_pcode(...)` helper instead of rescanning raw pcode with local indirect-control predicates.
- [`run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now gates giant-body cleanup and late jump resolution with explicit size budgets so the new larger lift surface does not blow up `nir-check` fast-lane latency.

Validation on the seeded [`putty.exe`](samples/windows/x64/putty.exe) `--limit 50` spot-check:

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- public summary direct-success: `50/50 -> 50/50`
- `avg_normalized_similarity`: `37.59% -> 37.91%`
- public indirect counters:
  - `fission unsupported indirect: 1 -> 11`
  - `fission indirect-surface preserved: 15 -> 18`
  - `fission jump-table functions: 7 -> 7`
  - `fission dispatcher recovered: 6 -> 1`
- representative low-sim rows:
  - `0x140001160`: `1.19% -> 27.05%`, now fully lifted instead of truncating at the first indirect branch, but still unresolved semantically with preserved indirect residue
  - `0x140008900`: direct-success restored and dispatcher proof preserved, `19.84%-ish band -> 19.55%`
  - `0x140008090`: stays green with indirect target proof, `32.94% -> 33.62%`
  - `0x140006ef0`: direct-success guardrail stays green at `35.33%`
- `nir-check` fast lane:
  - `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
  - giant cleanup regressions are no longer failing the lane
  - dominant slow passes moved to:
    - `sccp: 367.3ms`
    - `copy_propagation_after_cse: 106.4ms`
    - `cleanup_standalone_12: 85.0ms`
    - `jump_resolver: 37.4ms`

Net effect:

- improved:
  - `0x140001160` is no longer artificially tiny; the Rust decompiler now sees the real giant WndProc body
  - benchmark direct-success remains `50/50` after adding strict retry fallback for unsupported expanded-lift cases
  - `nir-check` fast lane is green again after budget-gating giant-body cleanup/jump-resolution work
  - end-to-end similarity still improved over the previous `37.59%` baseline
- did not improve enough:
  - the extra body visibility exposed far more unresolved indirect residue than before, so unsupported indirect attribution rose sharply
  - `dispatcher_shape_recovered_count` regressed from the previous wave's higher count because the current fix was about body visibility and guardrails, not proof-complete target-map synthesis
  - `0x140001160` is no longer the worst row, but it is still unresolved and still carries `11` unsupported indirect-control sites

Next bottleneck:

- the next wave should stop spending effort on lift coverage and move back to canonical proof recovery in `fission-pcode`
- specifically:
  - prove deterministic target maps for the remaining preserved indirect sites inside `0x140001160`
  - reduce unsupported-indirect inflation by converting preserved giant-body residue into real switch/dispatcher shapes
  - finish duplicate-logic cleanup for the remaining `IndirectControlClassification::from_flags(...)` rebuild sites in CLI inventory/automation consumers
  - if proof recovery does not move similarity materially after that, the next primary KPI should shift to `sccp` / `copy_propagation_after_cse` / `cleanup_standalone_12`

### Proof-complete dispatcher recovery wave — canonical proof state now recovers dispatcher shapes instead of only preserving indirect residue

This wave pushed indirect control recovery one step past bare surface preservation. The canonical owner remains `fission-pcode`, but `BranchInd` lowering now emits proof-aware selector state, degenerate self-loop dispatcher cases stay explicit `DispatcherLike` surfaces instead of collapsing into fake one-case switches, and downstream admission/filtering now reads one canonical indirect classification helper instead of re-deriving policy from local booleans.

- [`switch_table.rs`](crates/fission-pcode/src/nir/builder/switch_table.rs) now returns `RecoveredSwitchSelector` with proof kind metadata and can prove single-target dispatcher-like surfaces from mapped-global selector loads without inventing direct-call targets.
- [`terminator.rs`](crates/fission-pcode/src/nir/builder/terminator.rs) now:
  - records selector proof state from `BranchInd` lowering
  - uses canonical block target identity for self-loop detection
  - promotes proof-complete single-target cases to explicit `DispatcherLike` unsupported surfaces instead of synthesizing degenerate switches
  - increments canonical dispatcher/indirect proof counters from the builder owner
- [`bootstrap_x86.rs`](crates/fission-pcode/src/nir/tests/bootstrap_x86.rs) now contains a regression that locks in the new self-loop dispatcher behavior.
- [`types.rs`](crates/fission-pcode/src/nir/types.rs) now exposes canonical `IndirectControlClassification` helpers so admission/filtering logic can be reused instead of reimplemented.
- [`provenance.rs`](crates/fission-cli/src/cli/oneshot/inventory/provenance.rs), [`summary.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/summary.rs), and [`corpus.rs`](crates/fission-automation/src/corpus.rs) now consume the canonical classification helper instead of maintaining separate indirect-control predicates.
- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now preserves split cleanup/indirect family attribution instead of recombining everything into one benchmark-side quality bucket.

Validation on the seeded [`putty.exe`](samples/windows/x64/putty.exe) `--limit 50` spot-check:

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- public summary direct-success: `50/50 -> 50/50`
- `avg_normalized_similarity`: `37.55% -> 37.59%`
- median normalized similarity: `37.75% -> 38.00%`
- public indirect counters:
  - `fission unsupported indirect: 1 -> 1`
  - `fission indirect-surface preserved: 9 -> 15`
  - `fission jump-table functions: 7 -> 1`
  - `fission dispatcher recovered: 0 -> 6`
- representative low-sim rows:
  - `0x140001160`: `1.42% -> 1.19%`, still unresolved, preserved-indirect `1 -> 2`, dispatcher recovered `0 -> 0`
  - `0x140008900`: `19.95% -> 19.69%`, preserved-indirect `0 -> 2`, dispatcher recovered `0 -> 1`
  - `0x140008090`: `33.00% -> 33.11%`, preserved-indirect `0 -> 2`, dispatcher recovered `0 -> 1`
- `nir-check` fast lane:
  - `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
  - dominant slow passes remain the same bottleneck family:
    - `cleanup_stmt_canonical_init_1: 121.1ms -> 120.3ms`
    - `cleanup_dead_binding_init_1: 42.2ms -> 42.3ms`

Net effect:

- improved:
  - canonical dispatcher recovery is now real, not just indirect-surface preservation
  - duplicate indirect-control admission logic was reduced across `fission-pcode`, CLI inventory/candidate summary, and automation corpus filtering
  - similarity moved slightly upward while keeping coverage/direct-success guardrails flat
- did not improve enough:
  - `0x140001160` remains the worst function and still carries unresolved unsupported indirect control
  - `0x140008900` now has dispatcher proof, but similarity is still low, so proof completion alone is not sufficient there
  - cleanup perf is still blocked on the same `cleanup_stmt_canonical_init_1` / `cleanup_dead_binding_init_1` pair

Next bottleneck:

- the next wave should stop treating dispatcher recovery as the only blocker and instead attack the remaining `0x140001160` proof gap plus the pass-manager cleanup gate directly
- specifically:
  - complete target-map recovery for the unresolved indirect path in `0x140001160`
  - move more switch/dispatcher synthesis into proof consumers before late cleanup
  - split or budget the heavyweight cleanup family at the canonical owner instead of relying on downstream attribution only

### Dispatcher-proof ownership consolidation wave — canonical indirect-control classification now drives static, CLI, and benchmark consumers

This wave consolidated indirect-control meaning around canonical `fission-pcode` contracts and removed another layer of downstream reinterpretation. It did not complete proof-driven dispatcher recovery yet, but it did move the repository onto one shared indirect-control predicate and one shared unsupported/dispatcher counter contract.

- Added canonical indirect-control helpers in [`types.rs`](crates/fission-pcode/src/pcode/types.rs) and [`types.rs`](crates/fission-pcode/src/nir/types.rs):
  - `pcode_has_indirect_control_flow(...)`
  - `IndirectControlClassification`
- [`render.rs`](crates/fission-static/src/analysis/decomp/nir/render.rs) and [`routing.rs`](crates/fission-static/src/analysis/decomp/nir/routing.rs) now consume the canonical predicate instead of rescanning `PcodeFunction` with local indirect-control logic.
- [`summary.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/summary.rs), [`build.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/build.rs), [`schema.rs`](crates/fission-cli/src/cli/oneshot/decompile/nir_candidates/schema.rs), [`emit.rs`](crates/fission-cli/src/cli/oneshot/inventory/emit.rs), [`provenance.rs`](crates/fission-cli/src/cli/oneshot/inventory/provenance.rs), and [`schema.rs`](crates/fission-cli/src/cli/oneshot/inventory/schema.rs) now pass through canonical indirect classification instead of re-deriving unsupported risk from engine-specific booleans.
- [`model.rs`](crates/fission-automation/src/model.rs) and [`corpus.rs`](crates/fission-automation/src/corpus.rs) now consume the same canonical indirect flags for candidate filtering.
- [`benchmark_core.py`](artifacts/batch_benchmark_scripts/grand_finale_support/benchmark_core.py) now attaches canonical indirect/dispatcher flags to pairwise rows and lowest-sim summaries instead of inventing a separate benchmark-side interpretation.
- [`terminator.rs`](crates/fission-pcode/src/nir/builder/terminator.rs) now lowers single-target degenerate `BranchInd` cases to explicit dispatcher pseudo-surface evidence (`DispatcherLike` / `NonStructuralDispatcher`) instead of emitting an empty switch wrapper. This keeps unresolved dispatcher residue explicit without reintroducing the old bare unsupported marker path.

Validation on the seeded [`putty.exe`](samples/windows/x64/putty.exe) `--limit 50` spot-check:

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- public summary direct-success: `50/50 -> 50/50`
- `avg_normalized_similarity`: `37.44% -> 37.55%`
- public indirect counters:
  - `fission unsupported indirect: 1 -> 1`
  - `fission indirect-surface preserved: 1 -> 9`
  - `fission jump-table functions: 8 -> 7`
  - `fission dispatcher recovered: 0 -> 0`
- representative low-sim rows:
  - `0x140001160`: still unresolved, but preserved-indirect attribution `1 -> 2`
  - `0x140008900`: preserved-indirect attribution `0 -> 1`
  - `0x140008090`: preserved-indirect attribution `0 -> 1`
- `nir-check` fast lane:
  - `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
  - dominant slow passes remain effectively flat:
    - `cleanup_stmt_canonical_init_1: 120.6ms -> 121.1ms`
    - `cleanup_dead_binding_init_1: 42.5ms -> 42.2ms`

Net effect:
- explicit dispatcher/indirect residue is surfaced more consistently
- coverage and direct success remain stable
- similarity improved slightly, but proof-complete dispatcher recovery is still not done

### Unsupported-elimination wave — unresolved indirect control now preserves explicit surface and canonical counters


- Added canonical unsupported-indirect contracts in [`types.rs`](crates/fission-pcode/src/nir/types.rs):
  - `UnsupportedControlEvidence`
  - `unsupported_indirect_control_count`
  - `unsupported_indirect_call_count`
  - `unsupported_external_target_count`
- [`benchmark_core.py`](artifacts/batch_benchmark_scripts/grand_finale_support/benchmark_core.py) now reads these counters from `preview_build_stats`, includes them in lowest-sim rows, and prints them in the public summary line.
- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now rolls unsupported/preserved-indirect counters into canonical quality-family summaries.

Validation on the seeded [`putty.exe`](samples/windows/x64/putty.exe) `--limit 50` spot-check:

- seeded shared coverage: `100.00% -> 100.00%`
- independent top-N coverage: `96.00% -> 96.00%`
- `both_success`: `100.000% -> 100.000%`
- `nir-check` fast lane:
  - `changed_rows=0`
  - gate remains `stop_hold_p5h3f`

Net effect:

- better unsupported attribution and explicit indirect surface preservation
- no regression in coverage or direct success
- no meaningful similarity uplift yet
- next bottleneck remains proof-driven dispatcher recovery for low-sim functions such as `0x140001160`, plus the existing cleanup perf gate


- [`benchmark_core.py`](artifacts/batch_benchmark_scripts/grand_finale_support/benchmark_core.py) now defines Fission direct-success as:
  - `success == true`
- This fixes the stale `mlil_preview`-only accounting that forced direct-success to zero after the Rust canonical path took over.
- Validation:
  - reran [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe) with `--limit 50`
  - public summary now reports `fission direct-success 50/50`
  - seeded shared coverage remains `100.00%`
  - independent top-N coverage remains `96.00%`
  - `both_success` remains `100.000%`
  - `avg_normalized_similarity` remains in the same band (`37.43%` on the rerun; previous run `37.45%`)

### Fact-driven similarity recovery wave — canonical typed facts landed, similarity held flat

This wave moved another semantic ownership layer into the Rust canonical pipeline by introducing a shared typed-fact inventory, extending prototype and effect summaries with wrapper provenance and region-level effect facts, and wiring indirect-control telemetry into canonical `NirBuildStats`. The quality guardrails held on the short seeded `putty.exe --limit 50` spot-check, but the primary KPI still plateaued: similarity remained at `37.45%`.

- Added a canonical typed-fact store in [`typed_facts.rs`](crates/fission-pcode/src/nir/normalize/memory/typed_facts.rs).
- The new inventory produces `TypedFactStore`, `ObjectFact`, and `SurfaceFact` data from partitioned access evidence, explicit surface types, and structural shape facts.

#### fission-pcode — prototype and effect summaries gained wrapper provenance and canonical promotion telemetry

  - count conflicts and successful surface promotions in `NirBuildStats`
- [`jump_resolver.rs`](crates/fission-pcode/src/nir/vsa/jump_resolver.rs) now records indirect-target and dispatcher recovery telemetry at the canonical owner.

#### Automation and duplicate-logic audit

- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now reads the new canonical counters directly:
  - `typed_fact_evidence_count`
  - `typed_fact_conflict_count`
  - `object_root_fact_promotion_count`
  - `surface_fact_promotion_count`
  - `prototype_summary_round_count`

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-pcode`
- `nir-check` fast lane:
  - `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
    - `cleanup_dead_binding_init_1: 42.9ms -> 42.8ms`
  - new top build stats include `call_effect_summary_refined_count=104`
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-fact-summary-dispatch-wave`
  - seeded shared coverage: `100.00% -> 100.00%`
  - independent top-N coverage: `96.00% -> 96.00%`
  - `both_success: 100.000% -> 100.000%`
  - `avg_normalized_similarity: 37.45% -> 37.45%`
  - Fission wall: `0.508s -> 0.507s`
  - pyghidra wall: `2.597s -> 2.739s`

#### What improved and what did not

- Improved:
  - canonical object and call semantics now flow through one shared typed-fact / summary path in `fission-pcode`
  - automation now sees the new fact/prototype/dispatcher telemetry without inventing a parallel schema
  - fast-lane cleanup cost regressed slightly less than before, with a small improvement in the dominant cleanup passes
- Did not materially improve:
  - the primary similarity KPI remained flat at `37.45%`
  - the current low-sim `putty.exe` cases are still dominated by unresolved indirect/dispatcher semantics and generic surface residue

#### Next bottleneck

- The next wave should focus on proof-driven indirect control recovery and pass-manager cleanup budgeting:
  - recover dispatcher/helper shapes only when target-set proof is complete
  - keep ambiguous indirect control explicit instead of forcing weak direct-call surfaces
  - attack `cleanup_stmt_canonical_init_1` and `cleanup_dead_binding_init_1` directly at the pass-manager level instead of adding more downstream semantic polish

### Canonical semantics consolidation wave — static semantic rewrites disabled, benchmark stable, similarity still flat

This wave pushed the remaining semantic ownership drift further upstream into the Rust canonical path. The main changes were to stop `fission-static` from owning field/type/aggregate semantic rewrites by default, let `fission-pcode` infer more concrete aggregate surface types when a unique Windows structure shape exists, and split a cheap conditional-return simplification path away from the heavyweight statement-canonical cleanup family. The result is cleaner ownership and slightly better benchmark similarity, but still no material KPI jump on `putty.exe --limit 50`.

#### fission-static — semantic postprocess aggressively shrunk to naming-only defaults

- [`postprocess.rs`](crates/fission-static/src/analysis/decomp/postprocess.rs) now treats canonical object/type/call semantics as upstream-owned by default.
- The default postprocess pipeline no longer applies semantic rewrite passes such as:
  - field-offset semantic replacement
  - struct/type promotion
  - aggregate copy normalization
  - clean-slate semantic artifact rewriting
- `fission-static` remains responsible for naming and presentation polish, not semantic repair.
- This removes another duplicated owner for object and field meaning and keeps the benchmark path aligned with the Rust canonical pipeline.

#### fission-pcode — aggregate shape inference can now surface concrete pointer types from unique Windows structure layouts

- [`normalize/memory/aggregate_fields.rs`](crates/fission-pcode/src/nir/normalize/memory/aggregate_fields.rs) now:
  - resolves canonical structure names from known pointer aliases as before
  - additionally infers a concrete structure name from observed offsets when the Windows type database has a unique size/offset match
  - upgrades pointer bindings without an existing surface type to a concrete `STRUCT *` surface when that match is unique
- The pass still remains conservative:
  - exact size match required
  - every observed offset must exist in the candidate structure
  - ambiguous matches do not promote a concrete type name
- A new regression test now covers this unique-shape pointer-surface inference using `PROCESS_INFORMATION`.

#### fission-pcode — cheap conditional-return cleanup split from the expensive stmt-canonical sweep

- [`normalize/pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now runs a targeted conditional-return cleanup before the broad statement-canonical family.
- The early gate uses a cheap body-shape check instead of always escalating to the full `cleanup_stmt_fold_*` path.
- This narrows when the expensive statement-canonical family activates and keeps simple redundant `if (...) return ...; else return ...;` shapes on a cheaper path.

#### Duplicate-logic audit

- Object/field/type semantics:
  - canonical owner remains `fission-pcode`
  - `fission-static` no longer runs the removed semantic repair passes by default
- Cleanup family taxonomy:
  - canonical owner remains [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs)
  - the new conditional-return split stays inside the canonical cleanup pipeline instead of creating downstream-only logic

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli --release`
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
- `nir-check`:
  - lane completed successfully with `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
  - dominant costs remain:
    - `cleanup_stmt_canonical_init_1 (120.7ms)`
    - `cleanup_dead_binding_init_1 (42.9ms)`
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-similarity-next-wave`
  - seeded shared coverage: `100.00%`
  - independent top-N coverage: `96.00%`
  - `both_success=100.000%`
  - `avg_normalized_similarity=37.45%`
  - Fission wall `0.508s`, pyghidra wall `2.597s`

#### Known residual risk

- Similarity moved only marginally (`37.44% -> 37.45%`), so the ownership cleanup was correct but not sufficient to move the primary KPI.
- The next bottlenecks are still:
  - canonical semantic surface quality for unresolved call/object naming
  - `cleanup_stmt_canonical_init_1`
  - `cleanup_dead_binding_init_1`

### Similarity-first object and prototype wave — stronger canonical semantics, stable benchmark, no material similarity gain yet

This wave targeted the next bottleneck after direct-success stabilization: semantic similarity drift and the oversized `cleanup_stmt_canonical_init_1` family. The intent was to move more object and call semantics into the Rust-owned canonical path while splitting early cleanup into more actionable readiness-gated subfamilies. The benchmark stayed stable on coverage and success, `nir-check` remained green at the fast-lane execution level, but the `putty.exe --limit 50` spot check did not show a meaningful similarity increase yet.

#### fission-pcode — partition-rooted object surfacing and structured field naming

- [`types.rs`](crates/fission-pcode/src/nir/types.rs) now carries canonical prototype/effect metadata and additional object-recovery counters:
  - `SummarySoundness`
  - `PrototypeSummary`
  - `object_root_recovered_count`
  - `typed_object_shape_refined_count`
  - `prototype_summary_refined_count`
  - `cleanup_stmt_fold_count`
  - `cleanup_boundary_label_count`
  - `cleanup_loopish_rewrite_count`
- [`builder/stats.rs`](crates/fission-pcode/src/nir/builder/stats.rs) and [`normalize/wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) now initialize and accumulate those counters in the canonical Rust telemetry path.
- [`normalize/memory/aggregate_fields.rs`](crates/fission-pcode/src/nir/normalize/memory/aggregate_fields.rs) now upgrades aggregate surfacing with canonical field names when a stable Windows structure surface type is known.
  - The pass consults [`WindowsStructures`](crates/fission-signatures/src/win_types.rs) by type name rather than inventing anonymous `field_*` labels when enough size and interval evidence exists.
  - Pointer aliases such as `LPRECT` are normalized back to the underlying structure name and surfaced as concrete fields like `left`, `top`, `right`, and `bottom`.
- This keeps object and field meaning in `fission-pcode` instead of re-inventing it downstream in `fission-static`.

#### fission-pcode — prototype/effect summaries move beyond arity-only tightening

- [`normalize/types/interproc_sig_prop.rs`](crates/fission-pcode/src/nir/normalize/types/interproc_sig_prop.rs) now splits call summaries into:
  - `PrototypeSummary`
  - `CallEffectSummary`
- Import-backed callees can now seed prototype knowledge directly from [`WIN_API_DB`](crates/fission-signatures/src/win_api_db.rs):
  - exact arity when known
  - parameter type lattices
  - return type lattices
- [`normalize/types/callsite_type_prop.rs`](crates/fission-pcode/src/nir/normalize/types/callsite_type_prop.rs) now consumes those canonical prototype summaries when import DB matches are absent, instead of relying only on string-local callsite facts.
- This did not yet materially change the benchmark similarity score, but it removes more summary ownership drift from downstream layers.

#### fission-pcode — staged cleanup family split and readiness-gated early exits

- [`normalize/pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now splits the expensive statement-canonical cleanup family into readiness-gated subfamilies:
  - `cleanup_stmt_fold_*`
  - `cleanup_boundary_label_*`
  - `cleanup_loopish_rewrite_*`
- Cleanup execution now uses:
  - explicit statement/block/round budgets
  - body-shape readiness checks
  - dedicated boundary-label cleanup instead of always paying for the full statement-canonical sweep
- [`cleanup_stmt_list`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) was refactored to accept bounded options so family passes can run with narrower responsibilities and explicit round limits.
- The fast lane is still held by `cleanup_stmt_canonical_init_1`, but the surrounding attribution is now more actionable and less monolithic.

#### fission-automation — family attribution follows the canonical counters

- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now reads the new canonical counters directly from [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).
- The quality-family mapping now attributes:
  - `memory_shape` using object-root and typed-object counters
  - `call_signature` using prototype summary refinement counters
  - `cleanup` using the split statement/boundary/loopish cleanup counters
- No parallel telemetry schema was added in automation; it still only aggregates the canonical source of truth from `fission-pcode`.

#### Duplicate-logic audit

- Object and field semantics:
  - canonical owner remains `fission-pcode`
  - this wave did not add new semantic rewrite logic to `fission-static`
- Call prototype / effect summary semantics:
  - canonical owner remains `fission-pcode`
  - `fission-static` remains a provenance and fact supplier only
- Cleanup family taxonomy:
  - canonical owner remains [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs)
  - `fission-automation` only groups and reports those counters

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli --release`
- `nir-check`:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
  - lane completed successfully with `changed_rows=0`
  - gate remains `stop_hold_p5h3f`
  - dominant costs remain:
    - `cleanup_stmt_canonical_init_1 (118.3ms)`
    - `cleanup_dead_binding_init_1 (41.3ms)`
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-similarity-next-wave`
  - seeded shared coverage: `100.00%`
  - independent top-N coverage: `96.00%`
  - `both_success=100.000%`
  - `avg_normalized_similarity=37.44%`
  - Fission wall `0.508s`, pyghidra wall `2.947s`

#### Known residual risk

- This wave improved canonical semantic ownership and made cleanup attribution more actionable, but the `putty.exe --limit 50` similarity score was effectively flat (`37.45% -> 37.44%`).
- The next bottleneck is now clearer:
  - semantic surface quality still needs improvement in the canonical object/call path
  - `cleanup_stmt_canonical_init_1` remains the dominant fast-lane cost family

## 2026-04-10

### Typed-object and bounded-cleanup wave — semantic owner tightening without similarity regression

This wave targeted the next bottleneck after coverage and direct-success stabilization: semantic similarity drift and the oversized `cleanup_init_1` bucket. The goal was to move more memory/object/call semantics into the Rust-owned canonical path while splitting cleanup cost attribution into deterministic pass families. The result is that seeded coverage and direct success stayed intact, `nir-check` now reports cleanup cost with family-level attribution, and the benchmark remains stable, but the spot-check similarity metric did not materially improve yet.

#### fission-pcode — typed object recovery telemetry and conservative object surfacing

- [`types.rs`](crates/fission-pcode/src/nir/types.rs) now defines canonical semantic carriers for this wave:
  - `StorageClass`
  - `ObjectRegion`
  - `TypedObjectShape`
  - `SurfaceBinding`
  - `CallEffectSummary`
  - `WrapperClass`
- [`types.rs`](crates/fission-pcode/src/nir/types.rs) also extends [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) with canonical counters for:
  - object-shape recovery
  - surface binding promotion
  - call/effect summary refinement
  - wrapper summary folds
  - cleanup budget skips
  - split cleanup family counts
- [`builder/stats.rs`](crates/fission-pcode/src/nir/builder/stats.rs) and [`normalize/wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) now initialize and accumulate those counters in the Rust canonical path rather than introducing downstream-only telemetry.
- [`normalize/memory/aggregate_fields.rs`](crates/fission-pcode/src/nir/normalize/memory/aggregate_fields.rs) now:
  - collects structured access facts per partition
  - upgrades `Ptr(Unknown)` and structured byte-pointer cases into aggregate pointers when interval evidence is strong enough
  - promotes field-like surfacing for stack locals and parameters under a conservative proof policy
  - records canonical object-shape and surface-binding telemetry
- [`normalize/memory/slots.rs`](crates/fission-pcode/src/nir/normalize/memory/slots.rs) now reports slot alias surfacing as canonical surface-binding promotion telemetry.

#### fission-pcode — call/effect summary lattice extension

- [`normalize/types/interproc_sig_prop.rs`](crates/fission-pcode/src/nir/normalize/types/interproc_sig_prop.rs) now pushes call summary ownership beyond arity lower bounds:
  - summaries now carry `CallEffectSummary`
  - zero-arity callees conservatively seed `escapes_args = false`
  - simple forwarding wrappers are recognized as:
    - `TailForwarder`
    - `PureAdapter`
- This summary tightening remains canonical to `fission-pcode`; `fission-static` continues to provide provenance and import facts but does not own wrapper/effect semantics.

#### fission-pcode — bounded cleanup scheduler and family-level perf attribution

- [`normalize/pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) no longer treats the early cleanup wave as one opaque megablock.
- The former `cleanup_init_*` behavior is now split into bounded family passes:
  - `cleanup_binding_init_*`
  - `cleanup_stmt_canonical_*`
  - `cleanup_dead_binding_*`
- The scheduler now:
  - skips binding-init cleanup when no initializer evidence exists
  - avoids loop-folding work on bodies that fail the size/budget checks
  - records budget skips explicitly
- This does not yet make the lane green, but it changes perf attribution from “giant cleanup bucket” into actionable canonical pass families.

#### fission-automation — family attribution follows canonical counters only

- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now reads the new canonical counters directly from [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).
- The quality-family map now includes:
  - richer `memory_shape` attribution
  - richer `call_signature` attribution
  - a dedicated `cleanup` family
- This keeps telemetry ownership in `fission-pcode`; automation only aggregates and reports the canonical source of truth.

#### Duplicate-logic audit

- Object / field / slot semantic ownership:
  - canonical owner remains `fission-pcode`
  - no new semantic rewrite layer was added to `fission-static`
- Call / effect / wrapper summary ownership:
  - canonical owner remains `fission-pcode`
  - `fission-static` remains a provenance and fact supplier
- Perf-family taxonomy ownership:
  - canonical owner remains [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs)
  - `fission-automation` only groups and reports those counters

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode --lib --quiet`
  - `cargo check -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli`
  - `cargo build -p fission-cli --release`
- `nir-check`:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
  - lane completed successfully
  - gate remains `stop_hold_p5h3f`
  - top build stats now include `call_effect_summary_refined_count=104`
  - the dominant cleanup cost is now reported as:
    - `cleanup_stmt_canonical_init_1 (117.0ms)`
    - `cleanup_dead_binding_init_1 (40.6ms)`
  instead of one opaque `cleanup_init_1` bucket
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-object-call-cleanup-wave-rerun`
  - seeded shared coverage: `100.00%`
  - independent top-N coverage: `96.00%`
  - `avg_normalized_similarity=37.45%`
  - `both_success=100.000%`
  - Fission wall `0.507s`, pyghidra wall `2.926s`

#### Known residual risk

- This wave tightened semantic ownership and improved cleanup attribution, but it did not materially raise the similarity score for the `putty.exe --limit 50` spot check.
- The next bottleneck is no longer coverage or direct success; it is semantic surface quality and the remaining cleanup-family cost headed by `cleanup_stmt_canonical_init_1`.

### Direct-success-first wave — virtual-block panic removal and canonical recovery surfacing

This wave focused on the remaining seeded direct-success blocker after coverage alignment. The immediate target was the `putty.exe --limit 50` seeded set case at `0x140006ef0`, which previously fell through as a partial/error row because a deeper virtual-block structuring path in `fission-pcode` panicked. The fix was applied at the canonical owner rather than in the benchmark harness or CLI surface.

#### fission-pcode — canonical block projection for structuring consumers

- [`builder/mod.rs`](crates/fission-pcode/src/nir/builder/mod.rs) now exposes canonical block access helpers for structuring:
  - `pcode_block_idx(..)`
  - `pcode_block(..)`
  - `block_start_address(..)`
  - `block_count()`
- These helpers project synthetic or virtual split-block indices back to canonical P-code block indices before structuring code reads block metadata.
- [`structuring/loops.rs`](crates/fission-pcode/src/nir/structuring/loops.rs), [`structuring/linear.rs`](crates/fission-pcode/src/nir/structuring/linear.rs), [`structuring/conditionals/plain_if.rs`](crates/fission-pcode/src/nir/structuring/conditionals/plain_if.rs), [`structuring/conditionals/if_else.rs`](crates/fission-pcode/src/nir/structuring/conditionals/if_else.rs), [`structuring/conditionals/short_circuit.rs`](crates/fission-pcode/src/nir/structuring/conditionals/short_circuit.rs), and [`structuring/recovery.rs`](crates/fission-pcode/src/nir/structuring/recovery.rs) now consume those canonical helpers instead of indexing `self.pcode.blocks` directly.
- This removes the index-drift that previously produced:
  - `index out of bounds: the len is 15 but the index is 15`
  in the `0x140006ef0` structuring path.

#### fission-static — render panic now enters the canonical recovery path

- [`render.rs`](crates/fission-static/src/analysis/decomp/nir/render.rs) now catches `render_nir_with_context(..)` panics and surfaces them as deterministic structuring failures instead of letting them escape as worker-thread panics.
- Panic payloads are converted into:
  - `nir_structuring_failure[unsupported_cfg_region_shape]: render panicked: ...`
- This keeps recovery ownership in the Rust canonical path:
  - `fission-pcode` defines the failure
  - `fission-static` routes it
  - CLI/batch output only surfaces the canonical outcome

#### Duplicate-logic audit

- Block identity owner:
  - canonical owner is now `fission-pcode` builder/block projection, not ad hoc structuring-side index math
- Recovery family owner:
  - panic-to-structuring-failure translation now happens at the NIR render boundary, not inside batch-row formatting logic
- Batch availability reporting:
  - benchmark and CLI now consume the same “recovered vs failed” contract instead of inferring direct success from thread panics

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode --lib --quiet`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli`
  - `cargo build -p fission-cli --release`
- `nir-check`:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
    - lane completed successfully
    - gate remains `stop_hold_p5h3f`
    - dominant perf blocker remains `cleanup_init_1` (`160.8ms` in the latest fast-lane run)
- Single-function direct-success repro:
  - `./target/release/fission_cli samples/windows/x64/putty.exe --decomp 0x140006ef0 --engine rust-sleigh --ghidra-compat`
  - previously panicked in [`structuring/loops.rs`](crates/fission-pcode/src/nir/structuring/loops.rs)
  - now completes and emits pseudocode successfully
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-direct-success-wave`
  - seeded shared coverage: `100.00%`
  - independent top-N coverage: `96.00%`
  - `avg_normalized_similarity=37.47%`
  - `both_success=100.000%`
  - Fission wall `0.505s`, pyghidra wall `3.229s`, throughput speedup `6.393x`

#### Known residual risk

- The seeded direct-success blocker at `0x140006ef0` is cleared for this benchmark spot check, but the quality/perf gate is still held by `cleanup_init_1`.
- The remaining next bottleneck is no longer panic recovery; it is semantic quality and cleanup cost.

### Similarity-first recovery wave — partial output retention and object surfacing

This wave focused on the next bottleneck after coverage alignment: keeping seeded functions present even when structuring still fails, while tightening a few low-signal surfaces in the Rust-owned canonical path.

#### fission-cli — canonical batch selection and panic-to-partial fallback

- [`decompile_targets.rs`](crates/fission-cli/src/cli/oneshot/decompile/decompile_targets.rs) now treats `--addresses-file` as a first-class canonical selector input for the regular decomp-all path, not only the inventory and legacy-adjacent helper paths.
- [`decompile_exec/run.rs`](crates/fission-cli/src/cli/oneshot/decompile/decompile_exec/run.rs) now routes batch decomp through the same address-file-aware selector contract used by the seeded benchmark harness.
- [`decompile_rust_sleigh.rs`](crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs) now converts worker-thread panics into deterministic per-function partial results instead of silently dropping the function row from batch JSON output.
  - When Rust-Sleigh panics during render, the batch row is still emitted with `fell_back=true`, error metadata, and a `code` payload so seeded coverage remains an availability metric rather than a panic artifact.

#### fission-pcode — virtual-block index hardening and surface cleanup

- [`builder/mod.rs`](crates/fission-pcode/src/nir/builder/mod.rs), [`builder/terminator.rs`](crates/fission-pcode/src/nir/builder/terminator.rs), and [`structuring/linear.rs`](crates/fission-pcode/src/nir/structuring/linear.rs) now project virtual split-block indices back to canonical P-code block indices before looking up block target keys and fallthrough metadata.
- [`builder/stack_slots.rs`](crates/fission-pcode/src/nir/builder/stack_slots.rs) now surfaces ABI-classified stack roles with role-specific names:
  - `home_*`
  - `arg_out_*`
  - `ret_scaffold_*`
  instead of collapsing them into generic `stack_*` locals.
- [`normalize/memory/aggregate_fields.rs`](crates/fission-pcode/src/nir/normalize/memory/aggregate_fields.rs) now upgrades `Ptr(Unknown)` bindings into `Ptr(Aggregate { .. })` when partitioned access intervals provide enough proof to recover a stable object shape.
- [`normalize/memory/partition.rs`](crates/fission-pcode/src/nir/normalize/memory/partition.rs) now recognizes `ret_scaffold_*` as stack-like memory for partition classification.

#### Duplicate-logic audit

- Canonical owner for batch address selection remains `fission-cli`:
  - `--decomp-all`
  - seeded benchmark execution
  - exact-address file selection
  now all consume the same selector contract.
- Canonical owner for “panic but function still present in benchmark corpus” is now the Rust-only decomp boundary in [`decompile_rust_sleigh.rs`](crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs), not the Python harness.
- Object-shape upgrade remains owned by `fission-pcode`; the benchmark harness still only reads emitted rows and never infers shape semantics itself.

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli --release`
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
- `nir-check`:
  - lane completed successfully
  - gate remains `stop_hold_p5h3f`
  - dominant remaining perf blocker is still `cleanup_init_1` (`167.0ms` in the latest fast-lane run)
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-similarity-wave`
  - seeded shared coverage: `100.00%` (up from `98.00%`)
  - independent top-N coverage: `96.00%` (unchanged)
  - `avg_normalized_similarity=37.50%` (up from `37.22%`)
  - `both_success=98.000%`
  - Fission wall `0.507s`, pyghidra wall `3.055s`, throughput speedup `6.02x`

#### Known residual risk

- Seeded coverage is now fully closed for this `putty.exe --limit 50` spot check, but one function (`0x140006ef0`) still falls back to an emitted error row because a virtual-block structuring panic remains unresolved deeper in `fission-pcode`.
- Similarity improved only slightly; the next meaningful gains still require deeper semantic cleanup and object/call recovery rather than more benchmark-contract work.

### Coverage-first alignment wave — canonical function selection and seeded pairing

This wave changes the meaning of benchmark coverage from “independent top-N address intersection” to “seeded common canonical-function availability.” The main goal is not printer similarity polish, but getting `fission-cli`, inventory emission, and the whole-binary benchmark onto the same function identity and ordering contract.

#### fission-cli — canonical selector as the single owner

- Added [`function_select.rs`](crates/fission-cli/src/cli/oneshot/function_select.rs) as the canonical selector for:
  - exact-address deduplication
  - generic internal-entry filtering
  - canonical address-file selection
- [`functions.rs`](crates/fission-cli/src/cli/oneshot/functions.rs) now lists canonical functions instead of the raw loader vector.
- [`decompile_rust_sleigh.rs`](crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs) now uses the same canonical selector for:
  - `--decomp-all`
  - `--decomp-limit`
  - `--addresses-file`
  - exact single-address lookup
- [`inventory/emit.rs`](crates/fission-cli/src/cli/oneshot/inventory/emit.rs) now uses the same selector contract as decomp-all, removing a second copy of function ordering and address-file parsing.

#### Benchmark harness — seeded common pairing is now the primary KPI

- [`benchmark_core.py`](artifacts/batch_benchmark_scripts/grand_finale_support/benchmark_core.py) now:
  - builds a canonical seed set from Fission `--list`
  - runs Fission batch decomp against that seed set via `--addresses-file`
  - runs Ghidra against the same seed addresses
  - computes primary coverage from `present` rows, not only key existence
- Existing independent top-N alignment is retained as a secondary KPI under:
  - `summary.coverage.independent_top_n_pyghidra_vs_fission`
- Public benchmark summaries now distinguish:
  - `seeded shared coverage`
  - `independent top-N coverage`

#### Duplicate-logic audit

- Function selection owner:
  - canonical owner is now [`function_select.rs`](crates/fission-cli/src/cli/oneshot/function_select.rs)
  - decomp-all, inventory, and `--list` no longer maintain separate address-ordering contracts
- Address normalization owner:
  - benchmark Python and automation Rust now both strip redundant `0x`/leading-zero variance before corpus keys are built
- Coverage pairing owner:
  - benchmark coverage is now defined at the seeded harness layer instead of emerging accidentally from two independent engine-local truncation orders

#### Tests / validation

- Passed:
  - `cargo test -p fission-cli function_select -- --nocapture`
  - `cargo check -p fission-cli`
  - `cargo check -p fission-static`
  - `cargo check -p fission-automation`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli --release`
- `nir-check`:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
    - lane completed
    - gate remains `stop_hold_p5h3f`
    - current blocker is still the existing `cleanup_init_1` perf regression, not the new coverage contract
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-coverage-wave`
  - seeded shared coverage: `98.00%` (up from `24.00%`)
  - independent top-N coverage: `96.00%`
  - `avg_normalized_similarity=37.22%`
  - `both_success=100.000%`
  - Fission wall `0.504s`, pyghidra wall `2.944s`, throughput speedup `5.721x`

#### Known residual risk

- Seeded coverage is now the primary availability KPI, so comparisons against older benchmark summaries must account for the contract change.
- One seed function is still absent from the Fission seeded batch output in the current `putty.exe --limit 50` run, so the seeded coverage result is `98.00%` rather than `100.00%`.

### Decompile quality wave — typed call semantics / partitioned MemSSA / structuring ownership

This wave moves three pieces of semantic ownership back into the Rust-only canonical path: typed call identity and summary propagation, partition-backed memory SSA aliasing, and canonical structuring recovery/family attribution. The goal is not printer-only cleanup for a single sample, but a tighter `fission-pcode -> fission-static -> fission-automation` contract with less duplicated policy.

#### fission-pcode — typed call semantics graph

- Added typed call identity primitives to [`types.rs`](crates/fission-pcode/src/nir/types.rs):
  - `CallTargetProvenance`
  - `CallEdgeKind`
  - `CallTargetRef`
  - `CallSummary`
- [`HirFunction`](crates/fission-pcode/src/nir/types.rs) now carries `callee_summaries`, so observed arity/type tightening has a canonical owner in `fission-pcode` instead of downstream string-only consumers.
- [`lower_expr.rs`](crates/fission-pcode/src/nir/builder/lower_expr.rs) now resolves call targets through typed `CallTargetRef` facts first and only falls back to plain names when no stronger provenance exists.
- [`callsite_type_prop.rs`](crates/fission-pcode/src/nir/normalize/types/callsite_type_prop.rs) and [`interproc_sig_prop.rs`](crates/fission-pcode/src/nir/normalize/types/interproc_sig_prop.rs) now work from typed callee identity plus canonical summaries before consulting import signature seeds.

#### fission-static — provenance supplier only

- [`nir/context.rs`](crates/fission-static/src/analysis/decomp/nir/context.rs) now supplies typed call provenance (`Fact`, `Direct`, `Import`, `Global`) and address-backed call parameter rules, but no longer owns summary tightening policy.
- [`nir/recovery.rs`](crates/fission-static/src/analysis/decomp/nir/recovery.rs) now consumes canonical structuring outcomes from `fission-pcode` instead of maintaining a static-local retry signature table.

#### fission-pcode — partitioned memory SSA alias core

- [`partition.rs`](crates/fission-pcode/src/nir/normalize/memory/partition.rs) now defines canonical partition identity with:
  - `MemoryAccessClass`
  - `MemoryEscapeClass`
  - `PartitionKey`
- [`mem_ssa.rs`](crates/fission-pcode/src/nir/normalize/global_opt/mem_ssa.rs) now consumes `PartitionKey` directly rather than keeping an older `Stack | Unknown`-only alias split.
- [`dead_store.rs`](crates/fission-pcode/src/nir/normalize/global_opt/dead_store.rs) and [`redundant_load.rs`](crates/fission-pcode/src/nir/normalize/global_opt/redundant_load.rs) now gate forwarding/elimination on partition-backed promotability.
- Conservative fix applied during this wave:
  - parameter-rooted memory is intentionally kept `Unknown` rather than promotable stack-like, which avoids unsound aggregate-store elimination on address-taken or externally visible pointees.

#### fission-pcode / automation — canonical structuring policy ownership

- Added canonical recovery-family types in [`types.rs`](crates/fission-pcode/src/nir/types.rs):
  - `RecoveryMode`
  - `StructuringReasonFamily`
  - `StructuringOutcome`
- [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) now derives canonical family counters for:
  - `RegionLegality`
  - `FollowFailure`
  - `Irreducible`
  - `LoopExit`
  - `SwitchShape`
  - `Budget`
- [`quality.rs`](crates/fission-automation/src/report/quality.rs) and [`insights.rs`](crates/fission-automation/src/report/insights.rs) now consume those family counters directly, instead of recomputing structuring policy from downstream subtype-specific heuristics.

#### Duplicate-logic audit

- Call semantics:
  - canonical call summary/tightening now lives in [`fission-pcode`](crates/fission-pcode/src/nir/normalize/types/interproc_sig_prop.rs)
  - [`fission-static`](crates/fission-static/src/analysis/decomp/nir/context.rs) only supplies provenance/facts
- Memory aliasing:
  - canonical partition identity now lives in [`partition.rs`](crates/fission-pcode/src/nir/normalize/memory/partition.rs)
  - [`mem_ssa.rs`](crates/fission-pcode/src/nir/normalize/global_opt/mem_ssa.rs) no longer carries a second stack-offset alias parser
- Structuring policy:
  - canonical recovery family/retryability now lives in [`fission-pcode`](crates/fission-pcode/src/nir/types.rs)
  - static and automation only orchestrate or report that canonical outcome

#### Tests / validation

- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli`
  - `cargo build -p fission-cli --release`
- `nir-check`:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli`
    - completed successfully
    - gate remained `stop_hold_p5h3f`
    - `changed_rows=0`
  - `cargo run -p fission-automation --release -- nir-check --lane nir --no-build --fission-bin target/release/fission_cli --run-profile mid --baseline artifacts/fission-automation/latest/nir/summary.json --fail-on-stop`
    - completed the lane
    - exited non-zero only because `--fail-on-stop` treats `stop_hold_p5h3f` as a blocking quality gate
    - `changed_rows=0`
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-next-wave-call-mem-struct`
  - Result summary: shared coverage `24.00%`, `avg_normalized_similarity=35.79%`, `both_success=100.000%`, Fission wall `0.505s`, pyghidra wall `4.222s`, throughput speedup `8.367x`, Fission p99/p50 tail ratio `11.119`, pyghidra p99/p50 tail ratio `233.927`

#### Known residual risk

- This wave fixes semantic ownership drift, but it does not clear the current `nir-check` go/stop gate by itself. The remaining blocker is lane quality policy (`stop_hold_p5h3f`), not build breakage or inventory failure.

### Logging and diagnostics wave — canonical observability owner

This wave upgrades Fission's Rust-first observability path without introducing ad-hoc subscribers in boundary crates. The canonical owner remains [`fission-core/src/core/logging.rs`](crates/fission-core/src/core/logging.rs): file logging, span-aware boundary diagnostics, and runtime metrics are now wired through that shared layer, while CLI and automation only adapt process-boundary behavior.

#### fission-core — canonical logging / span trace owner

- Added `tracing-appender` and `tracing-error` to [`fission-core`](crates/fission-core/Cargo.toml).
- [`logging.rs`](crates/fission-core/src/core/logging.rs) now provides:
  - `LoggingMode`
  - `LoggingFormat`
  - `LoggingTargets`
  - `LoggingOptions`
  - `init_with_options(...)`
  - `capture_span_trace()`
- Replaced the placeholder `enable_file_logging()` behavior with pre-init file sink configuration and shared rolling file output.
- `LoggingOptions::from_config(...)` now treats `FISSION_LOG_FILE` as an explicit runtime override, so CLI/automation can enable file logging without editing TOML.
- Subscriber ownership remains centralized in `fission-core`; no other crate assembles a tracing subscriber.

#### fission-pcode / fission-automation — runtime metrics

- Added `metrics` to canonical runtime producers:
  - [`wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) now emits normalize-pass invocation, outcome, and duration metrics.
  - [`driver.rs`](crates/fission-pcode/src/nir/structuring/driver.rs) now emits total structuring duration and invocation metrics.
  - [`main.rs`](crates/fission-automation/src/main.rs) now emits lane-level runtime histograms/counters for inventory, diagnosis, write, total runtime, changed rows, and gate/perf-regression events.
- `NirBuildStats` remains the semantic-quality owner. Runtime metrics were added alongside it, not as a competing telemetry schema.

#### CLI / automation — boundary-only diagnostics

- Added `miette` to [`fission-cli`](crates/fission-cli/Cargo.toml) and [`fission-automation`](crates/fission-automation/Cargo.toml) for process-boundary rendering only.
- [`fission_cli`](crates/fission-cli/src/bin/fission_cli.rs) now renders top-level failures through `miette`, while [`oneshot/mod.rs`](crates/fission-cli/src/cli/oneshot/mod.rs) propagates contextual errors instead of exiting from deep helper branches.
- [`fission-automation`](crates/fission-automation/src/main.rs) now initializes shared logging before lane execution, writes a default rolling log under `artifacts/fission-automation/logs/`, and attaches a captured span trace when surfacing top-level failures.

#### Duplicate-logic audit

- Logging subscriber assembly remains canonical in [`fission-core`](crates/fission-core/src/core/logging.rs); CLI and automation now consume shared options instead of building separate tracing stacks.
- Runtime latency metrics are emitted from canonical producers (`fission-pcode`, `fission-automation`) rather than duplicating pass/runtime ownership in downstream reporting layers.

#### Tests / validation

- Passed:
  - `cargo test -p fission-core`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli`
  - `cargo build -p fission-cli --release`
  - CLI file logging verification with `FISSION_LOG_FILE=/tmp/fission-cli-logtest/fission-cli.log` produced `/tmp/fission-cli-logtest/fission-cli.log.2026-04-10`
  - automation default file logging verification produced `artifacts/fission-automation/logs/fission-automation.log.2026-04-10`
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build --fission-bin target/debug/fission_cli` still exits non-zero on the existing performance gate:
  - `cleanup_init_1`: `38.9ms -> 162.4ms (4.2x increase)`

### Decompile quality wave — semantics-first core upgrade

This wave moves semantic recovery deeper into the canonical Rust-owned pipeline instead of relying on printer-level cleanup. The primary themes are shared ABI carrier modeling, shared memory-partition evidence for slot/aggregate recovery, costed structuring candidate selection, and automation-side family attribution built directly from canonical `NirBuildStats`.

#### fission-pcode — shared ABI state and carrier assignment

- Added [`abi.rs`](crates/fission-pcode/src/nir/abi.rs) and threaded [`AbiState`](crates/fission-pcode/src/nir/abi.rs), [`CarrierResource`](crates/fission-pcode/src/nir/abi.rs), and [`CarrierAssignment`](crates/fission-pcode/src/nir/abi.rs) through the canonical `nir` layer.
- Added [`CarrierClass`](crates/fission-pcode/src/nir/types.rs) so entry/call carrier reasoning has a typed contract instead of ad-hoc Win64-only naming.
- [`stack_slots.rs`](crates/fission-pcode/src/nir/builder/stack_slots.rs) and [`call_recovery.rs`](crates/fission-pcode/src/nir/builder/call_recovery.rs) now use shared ABI state for parameter-slot, stack-tail, and home-slot classification, rather than duplicating direct calling-convention logic.
- [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/normalize/types/entry_param_promotion.rs) now shares the same ABI slot mapping contract as the preview builder.

#### fission-pcode — partitioned memory evidence reuse

- Added [`partition.rs`](crates/fission-pcode/src/nir/normalize/memory/partition.rs) as a shared collector for partitioned memory accesses: base expression, constant offset, stride, optional index, and access type.
- [`slots.rs`](crates/fission-pcode/src/nir/normalize/memory/slots.rs) now consumes shared partition evidence for slot-family surfacing instead of scanning HIR independently.
- [`aggregate_fields.rs`](crates/fission-pcode/src/nir/normalize/memory/aggregate_fields.rs) now consumes the same partition collector, removing duplicate offset-walk logic between aggregate recovery and slot surfacing.
- Duplicate-logic audit outcome for this wave:
  - stack offset / stride parsing now has a canonical owner in [`partition.rs`](crates/fission-pcode/src/nir/normalize/memory/partition.rs)
  - aggregate field discovery no longer reimplements its own full HIR memory-access traversal
  - automation family attribution is derived from canonical [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs), not a parallel metric schema

#### fission-pcode — costed structuring candidate selection

- [`driver.rs`](crates/fission-pcode/src/nir/structuring/driver.rs) no longer commits to the first accepted reducer. It now gathers accepted candidates and picks the minimum-cost region using a deterministic tuple:
  - loop-header violation
  - postdom damage
  - switch fanout damage
  - guard-chain cut
  - goto introduction count
  - label churn
  - span penalty
- This preserves canonical short-circuit lowering and avoids plain nested-`if` regressions that showed up when reducer ordering alone was used.

#### fission-automation — semantic family attribution

- [`quality.rs`](crates/fission-automation/src/report/quality.rs) now publishes family summaries derived from canonical counters:
  - `abi`
  - `memory_shape`
  - `variadic`
  - `call_signature`
  - `structuring`
  - `security`
- [`insights.rs`](crates/fission-automation/src/report/insights.rs) now exposes a `quality_delta_vector` and uses family-level deltas in the go/stop decision instead of relying only on a single mismatch-specialized counter.

#### Tests / validation

- Added ABI contract coverage in [`calling_convention.rs`](crates/fission-pcode/src/nir/tests/calling_convention.rs) for Win64 home-slot classification and Win64 stack-tail index recovery.
- Passed:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-pcode`
  - `cargo test -p fission-automation`
  - `cargo build -p fission-cli`
  - `cargo build -p fission-cli --release`
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, output dir `artifacts/batch_benchmark/putty-next-wave`
  - Result summary: shared coverage `24.00%`, `avg_normalized_similarity=35.79%`, `both_success=100.000%`, Fission wall `1.011s`, pyghidra wall `4.963s`, throughput speedup `4.91x`, Fission max RSS `10.12MB`

#### Known residual risk

- `nir-check` remains non-green for this wave:
  - fast profile reports a pass-level performance regression on `cleanup_init_1` (`36.9ms -> 177.9ms`, `4.8x`)
  - release/mid lane still ends in `stop_hold_p5h3f` because the semantic family delta vector is not yet strong enough to clear the quality gate
- This means the semantic/core refactor is integrated and benchmark-safe on the targeted `putty.exe` sample, but the broader automation lane still needs follow-up work on cleanup-pass performance and gate-improvement signal.

### Decompile quality wave — ABI carrier recovery, variadic surfacing, security/call cleanup

This update pushes wrapper-quality recovery further in the canonical Rust decompiler pipeline. The focus is **ABI meaning recovery**, not CFG reshaping: Win64 home/shadow slots are separated from ordinary locals, recovered call carriers survive UNIQUE-space lowering, variadic stack regions can surface as `va_start`, and low-signal call/security scaffolding is cleaned from canonical HIR before printing.

#### fission-pcode — ABI carrier / stack-slot recovery

- [`NirBindingOrigin`](crates/fission-pcode/src/nir/types.rs) now distinguishes [`HomeSlot`](crates/fission-pcode/src/nir/types.rs), [`OutgoingArgSlot`](crates/fission-pcode/src/nir/types.rs), [`VaRegion`](crates/fission-pcode/src/nir/types.rs), and [`ReturnScaffold`](crates/fission-pcode/src/nir/types.rs), so ABI stack roles stop collapsing into plain locals.
- [`StackSlot`](crates/fission-pcode/src/nir/support.rs) carries binding origin; [`stack_slots.rs`](crates/fission-pcode/src/nir/builder/stack_slots.rs) classifies Win64 positive `rsp` offsets into home-space slots and preserves canonical `stack_*` naming expected by normalize/MemSSA consumers.
- [`lower_expr.rs`](crates/fission-pcode/src/nir/builder/lower_expr.rs) now rejects non-dominating or later-in-block def-sites during lowering, fixing temporal unsoundness where earlier uses could see later defs.
- [`call_recovery.rs`](crates/fission-pcode/src/nir/builder/call_recovery.rs) recognizes UNIQUE-space x86 register carriers, recovers Win64 stack-tail arguments, and falls back to surfaced carrier names when a recovered carrier cannot be lowered through a p-code opcode chain.

#### fission-pcode — Variadic / call-signature refinement

- [`HirStmt::VaStart`](crates/fission-pcode/src/nir/types.rs) was added and threaded through printer, rename, cleanup, analysis, and structuring visitors so variadic recovery becomes a real IR feature instead of a metric-only placeholder.
- [`variadic_stack_region.rs`](crates/fission-pcode/src/nir/normalize/types/variadic_stack_region.rs) now performs real rewrites: it maps home slots, recovers ABI-backed variadic regions, inserts `VaStart`, and updates new ABI/variadic telemetry in [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).
- [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/normalize/types/entry_param_promotion.rs) now promotes direct register reads and trims unused Win64 variadic tail parameters, so wrapper-shaped functions keep the fixed parameter prefix instead of surfacing dead `r8`/`r9` artifacts.
- [`callsite_type_prop.rs`](crates/fission-pcode/src/nir/normalize/types/callsite_type_prop.rs) records call-site tightening in canonical telemetry.

#### fission-pcode — Security / call artifact canonicalization

- Added [`call_artifact.rs`](crates/fission-pcode/src/nir/normalize/idioms/call_artifact.rs) to eliminate synthetic temp-only call artifact scaffolding once dominance/def-use proof shows there is no remaining semantic user.
- Added [`security_cookie.rs`](crates/fission-pcode/src/nir/normalize/idioms/security_cookie.rs) to recognize xor-with-stack-pointer cookie checks and rename weak single-arg guard calls as `__security_check_cookie`.
- [`pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) now runs both passes in canonical normalize order; metrics flow through [`wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) and [`stats.rs`](crates/fission-pcode/src/nir/builder/stats.rs).

#### Telemetry / contracts

- [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs) gained:
  - `abi_slot_recovered_count`
  - `home_slot_promoted_count`
  - `va_start_recovered_count`
  - `call_signature_refined_count`
  - `security_cookie_fold_count`
  - `call_artifact_removed_count`
- This work keeps telemetry canonical in `fission-pcode`; no parallel report-only metric payload was introduced in automation.

#### Tests

- Added [`unique_x86_regs.rs`](crates/fission-pcode/src/nir/tests/unique_x86_regs.rs) coverage for UNIQUE-space `rsp` stack-slot recovery.
- Added Win64 variadic parameter trimming coverage in [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/tests/entry_param_promotion.rs).
- Full crate validation:
  - `cargo test -p fission-pcode`
  - `cargo check -p fission-static`
  - `cargo test -p fission-automation`

#### Benchmarks

- Regression guard:
  - [`validate_limit_regression.py`](artifacts/batch_benchmark_scripts/validate_limit_regression.py) on [`test_control_flow_x64_O0.exe`](samples/windows/x64/test_control_flow_x64_O0.exe) passed against release [`fission_cli`](crates/fission-cli/) and Ghidra `11.4.2` on 2026-04-10.
- 2-way benchmark:
  - [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py) on [`putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, artifact dir `artifacts/batch_benchmark/putty-abi-varargs-security/`.
  - Result summary: `both_success_rate_pct=100.0`, `avg_normalized_similarity=35.08%`, `coverage_ratio_pct=24.0%`, Fission `wall_sec=0.124522` vs pyghidra `wall_sec=4.223712`, and the harness reported `Regression check passed — no significant degradation detected.`

#### Known blocker

- Rust-only inventory emission now replaces the removed legacy inventory path for [`nir-check`](crates/fission-automation/): [`fission-cli`](crates/fission-cli/src/cli/oneshot/inventory/emit.rs) can emit `function_facts_inventory` without `native_decomp`, and [`fission-automation`](crates/fission-automation/src/inventory.rs) builds the default Rust-only CLI again.
- Validation after the switch:
  - hidden CLI inventory emit succeeds on [`putty.exe`](samples/windows/x64/putty.exe)
  - `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin target/debug/fission_cli --run-profile fast` completes successfully
  - `cargo run -p fission-automation --release -- nir-check --lane nir --no-build --fission-bin target/release/fission_cli --run-profile mid --baseline artifacts/fission-automation/latest/nir/summary.json --fail-on-stop` now executes end-to-end and fails only on the expected quality gate (`stop_hold_p5h3f`), not on inventory contract breakage

---

## 2026-04-09

### NIR layout — `normalize/` tree, `cfg_analysis/` split, automation report, sleigh semantic

Refactor-only work: **module paths and file placement**, not decompiler semantics. Normalize **pass order and behavior** are unchanged; public `nir::normalize` entry points (`normalize_hir_function`, `take_normalize_wave_stats`) stay the same.

#### fission-pcode — [`normalize/`](crates/fission-pcode/src/nir/normalize/)

- Passes grouped by role: [`types/`](crates/fission-pcode/src/nir/normalize/types/) (type inference & signature propagation), [`global_opt/`](crates/fission-pcode/src/nir/normalize/global_opt/) (SCCP, LICM, CSE, GVN join, memory SSA helpers, redundant load, dead store), [`recovery/`](crates/fission-pcode/src/nir/normalize/recovery/) (PHI, flags, IV, for-loops), [`memory/`](crates/fission-pcode/src/nir/normalize/memory/) (slots, aggregates, pointer arith), [`idioms/`](crates/fission-pcode/src/nir/normalize/idioms/) (bitstream, branch hoist, prologue), [`analysis/`](crates/fission-pcode/src/nir/normalize/analysis/) (`defuse`, `expr_key`); [`arith/`](crates/fission-pcode/src/nir/normalize/arith/) split from a single `arith.rs` into focused modules; [`cleanup/`](crates/fission-pcode/src/nir/normalize/cleanup/) uses `passes.rs` under `mod.rs`.
- Orchestration: [`pipeline/run.rs`](crates/fission-pcode/src/nir/normalize/pipeline/run.rs) (formerly `core.rs`), re-exported from [`pipeline/mod.rs`](crates/fission-pcode/src/nir/normalize/pipeline/mod.rs).
- Map for contributors: [`normalize/AGENTS.md`](crates/fission-pcode/src/nir/normalize/AGENTS.md); [`nir/AGENTS.md`](crates/fission-pcode/src/nir/AGENTS.md) updated with a pointer.

#### fission-pcode — structuring [`cfg_analysis/`](crates/fission-pcode/src/nir/structuring/cfg_analysis/)

- Former monolith [`cfg_analysis.rs`](crates/fission-pcode/src/nir/structuring/cfg_analysis.rs) split into `cfg_analysis/` (`dom`, `postdom`, `edge`, `scc`, helpers, `tests`).

#### fission-automation — [`report/`](crates/fission-automation/src/report/)

- Large [`report.rs`](crates/fission-automation/src/report.rs) replaced by [`report/mod.rs`](crates/fission-automation/src/report/mod.rs) + [`report/pipeline.rs`](crates/fission-automation/src/report/pipeline.rs) (same outward API via `pub use`).

#### fission-sleigh — x86 semantic

- [`semantic.rs`](crates/fission-sleigh/src/lifter/x86/semantic.rs) reorganized as [`semantic/mod.rs`](crates/fission-sleigh/src/lifter/x86/semantic/mod.rs) with tests under [`semantic/tests/`](crates/fission-sleigh/src/lifter/x86/semantic/tests/).

#### Tests / snapshots (fission-pcode)

- [`structuring_conditionals`](crates/fission-pcode/src/nir/tests/structuring_conditionals/) split from a single file; snapshot-driven checks via [`snapshot_printer.rs`](crates/fission-pcode/src/nir/tests/snapshot_printer.rs) and [`snapshots/`](crates/fission-pcode/src/nir/tests/snapshots/).

#### Misc

- Workspace / crate manifest tweaks (`Cargo.lock`, `fission-pcode` / `fission-sleigh` `Cargo.toml`), logging and CLI worker hooks, Tauri decompiler options, [`docs/build/BUILD.md`](docs/build/BUILD.md) notes.

### HIR Quality Phase 9 — SCCP, join GVN-lite, wide def-use DCE sweep

This update implements the Phase 9 plan: **structured sparse conditional constant
propagation (SCCP)**, **GVN-lite at 2-way joins** (duplicate pure RHS, different
LHS), and a **fixed-point dead temp sweep** after SCCP.  Coupled IV (SCEV) was
**not** expanded in this cycle; existing affine IV in [`iv_recovery.rs`](crates/fission-pcode/src/nir/normalize/iv_recovery.rs) remains the
SCEV-lite scope.

#### Overlap / non-duplication (vs existing passes)

| Phase 9 | Does **not** replace | Notes |
|--------|----------------------|--------|
| [`apply_sccp_pass`](crates/fission-pcode/src/nir/normalize/sccp.rs) | [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | Folding is local/single-pass on syntax; SCCP merges constants at **if/switch** joins and rewrites guarded branches when the condition is constant. |
| SCCP | [`apply_jump_resolver_pass`](crates/fission-pcode/src/nir/vsa/jump_resolver.rs) | VSA uses **intervals** on defs; SCCP uses a **constant lattice** on vars. Complementary. |
| [`apply_gvn_join_hoist_pass`](crates/fission-pcode/src/nir/normalize/gvn_join.rs) | [`apply_branch_prefix_hoist_pass`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs) | Branch hoist requires **the same LHS** on both arms; GVN join hoists when LHS **differs** but `pure_expr_key(rhs)` matches. |
| [`apply_gvn_join_hoist_pass`](crates/fission-pcode/src/nir/normalize/gvn_join.rs) | [`apply_cse_pass`](crates/fission-pcode/src/nir/normalize/cse.rs) | CSE is **per linear block** (map reset at branches); join GVN addresses **first stmt** on each arm after a fork. |
| [`apply_wide_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | [`defuse_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) | Same predicate (temp-only, `DefUseMap`); wide pass repeats up to 6 rounds so SCCP-folded unused temps are removed once use counts drop. |

#### fission-pcode — SCCP ([`sccp.rs`](crates/fission-pcode/src/nir/normalize/sccp.rs))

- Lattice map `Var → (i64, NirType)` with **merge** at `if`/`switch` exits; loops **conservatively** drop bindings for variables assigned in the body from the post-loop environment.
- Uses shared evaluator [`eval_hir_expr_with_const_env`](crates/fission-pcode/src/nir/normalize/defuse.rs) (no `Load`/`Call` constant evaluation).
- Pipeline: immediately after the first [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/core.rs) block; large functions use fewer rounds via [`is_large_hir_function`](crates/fission-pcode/src/nir/normalize/core.rs).

#### fission-pcode — Join GVN-lite ([`gvn_join.rs`](crates/fission-pcode/src/nir/normalize/gvn_join.rs))

- If both arms begin with `Assign(Var)` and `pure_expr_key` matches, inserts `__gvn_join_* = rhs` and rewrites the first statement of each arm to copy from the temp (then copy propagation can clean up).
- Pipeline: after [`apply_branch_prefix_hoist_pass`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs).

#### fission-pcode — Wide dead assignment ([`defuse.rs`](crates/fission-pcode/src/nir/normalize/defuse.rs))

- [`apply_wide_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs): bounded fixpoint of [`defuse_dead_assignment_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) after SCCP.

#### Tests

- [`normalize_slots::stack_slot_recovery_names_locals`](crates/fission-pcode/src/nir/tests/normalize_slots.rs) now allows `return 7;` when SCCP folds the return after `local_10` is known constant.

#### Benchmark (representative)

- [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py): `samples/windows/x64/test_control_flow_x64_O0.exe`, `--limit 50`, release `fission_cli`, Ghidra `11.4.2` (see [`test_control_flow_x64_O0-phase9-20260409`](artifacts/batch_benchmark/test_control_flow_x64_O0-phase9-20260409)).
- 2-way vs pyghidra: shared=42, coverage=84%, `avg_normalized_similarity=24.78%`, `both_success=100%`, fission wall ~1.02s vs pyghidra ~1.89s (2026-04-09).

### Decompile quality wave — ABI entry params, variadic stack region, call-site arity

Canonical HIR normalize additions (see [`entry_param_promotion.rs`](crates/fission-pcode/src/nir/normalize/entry_param_promotion.rs), [`variadic_stack_region.rs`](crates/fission-pcode/src/nir/normalize/variadic_stack_region.rs), [`interproc_sig_prop.rs`](crates/fission-pcode/src/nir/normalize/interproc_sig_prop.rs)); telemetry merges via [`wave_stats.rs`](crates/fission-pcode/src/nir/normalize/wave_stats.rs) into [`NirBuildStats`](crates/fission-pcode/src/nir/types.rs).

#### Overlap / non-duplication (vs existing passes)

| New module | Does **not** replace | Notes |
|------------|----------------------|--------|
| [`apply_entry_param_promotion_pass`](crates/fission-pcode/src/nir/normalize/entry_param_promotion.rs) | [`constant_folding_pass`](crates/fission-pcode/src/nir/normalize/defuse.rs) / [`apply_sccp_pass`](crates/fission-pcode/src/nir/normalize/sccp.rs) | Renames **first** entry-prefix spills from ABI param **hardware** names to `param_k`; folding/SCCP propagate **constants**, not register→param naming. |
| Entry promotion | [`collect_entry_register_param_aliases`](crates/fission-pcode/src/nir/builder/entry_analysis.rs) | Builder pass maps **P-code** register copies in the entry block; normalize pass maps **HIR** `Var("rsi")` spills using the same [`CallingConvention::param_offsets`](crates/fission-pcode/src/nir/support.rs) table. |
| [`apply_variadic_stack_region_pass`](crates/fission-pcode/src/nir/normalize/variadic_stack_region.rs) | [`apply_callsite_type_prop_pass`](crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs) | Counts **stack-tail** call patterns from **surfaced stack names / loads** (ABI region hook); Win API DB still seeds **named** callee types only. |
| Variadic region | [`apply_memory_slot_surfacing`](crates/fission-pcode/src/nir/normalize/slots.rs) / MemSSA | Surfacing/MemSSA model **slot defs/uses**; this pass only **tags** plausible variadic tail sites for metrics (future folds stay gated). |
| [`apply_interproc_callsite_arity_pass`](crates/fission-pcode/src/nir/normalize/interproc_sig_prop.rs) | [`apply_callsite_type_prop_pass`](crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs) | Records **max observed arity per callee symbol** from HIR calls (DB-independent lower bound); DB pass still supplies **Win types**. |
| Interproc arity | SCCP / constant folding | Arity bounds are **symbol→count** facts, not def-use constant lattice. |

#### Benchmark (same harness as Phase 9)

- [`full_decomp_benchmark.py`](artifacts/batch_benchmark_scripts/full_decomp_benchmark.py): [`samples/windows/x64/putty.exe`](samples/windows/x64/putty.exe), `--limit 50`, **`target/release/fission_cli`** (use `fission_cli`, not `fission-cli`, or the script falls back to debug), Ghidra `11.4.2` — artifact dir `putty-20260409-174151` under local `artifacts/batch_benchmark/` (not committed; `.gitignore`).
- 2-way vs pyghidra: shared=12, coverage=24%, `avg_normalized_similarity≈6.43%`, `both_success=100%`, fission wall ~1.01s vs pyghidra ~4.21s (2026-04-09).

---

## 2026-04-08

### HIR Quality Phase 8 — Redundant Load Elimination, Branch PRE-lite, Affine IV, ExprKey sharing

This update implements the "HIR 품질 강화 8단계" plan: algorithmic passes that do
not duplicate Phase 7 local CSE (scalar pure expressions), LICM (loops), or
Memory SSA dead-store removal (unobserved writes).

#### Shared pure expression keys (`expr_key.rs`)

- **`pure_expr_key`**, **`type_key`**, **`is_commutative`**, **`invalidate_pure_map`**
  are shared by local CSE and branch-prefix hoisting so commutative normalisation
  and invalidation rules stay in one place ([`cse.rs`](crates/fission-pcode/src/nir/normalize/cse.rs) now imports them).

#### fission-pcode — Redundant load elimination (RLE)

- **`apply_redundant_load_elimination`** ([`redundant_load.rs`](crates/fission-pcode/src/nir/normalize/redundant_load.rs), new)
  - Caches the result of `Load` from [`AliasKey::Stack`](crates/fission-pcode/src/nir/normalize/mem_ssa.rs) locations only; unknown/heap pointers are never cached.
  - Invalidates on `Deref`/`Index` stores to the same stack key; clears the cache at `if`/`while`/`switch` joins (conservative).
  - Pipeline: immediately after [`apply_dead_store_elimination`](crates/fission-pcode/src/nir/normalize/dead_store.rs).

- **`alias_key_for_pointer_expr`**, **`nir_byte_size`** are now `pub(crate)` on
  [`mem_ssa.rs`](crates/fission-pcode/src/nir/normalize/mem_ssa.rs) for reuse by RLE and MemSSA builder.

#### fission-pcode — If/else common pure-prefix hoisting

- **`apply_branch_prefix_hoist_pass`** ([`branch_hoist.rs`](crates/fission-pcode/src/nir/normalize/branch_hoist.rs), new)
  - Hoists up to 32 leading statements from both arms when they are
    `Assign { lhs: Var(x), rhs }` with the same `x` and identical `pure_expr_key(rhs)`, and no RHS side effects (`expr_has_side_effects`).
  - Pipeline: after [`join_coalescing_pass`](crates/fission-pcode/src/nir/normalize/phi_recovery.rs), followed by cleanup + copy propagation + def-use cleanup.

#### fission-pcode — SCEV-lite affine induction (`v = v * C + k`)

- [`iv_recovery.rs`](crates/fission-pcode/src/nir/normalize/iv_recovery.rs): **`is_iv_update`** extends linear `v ± k` updates with **`v * C + k`** (and commutative mul order), with `C` and `k` loop-invariant, so more `While` loops upgrade to `For`.

#### Benchmark (representative)

- `test_control_flow_x64_O0.exe`, `--limit 50`, 2-way vs Ghidra: shared=42,
  `avg_normalized_similarity=18.94%`, `both_success=100%` (matches Phase 7 baseline).

---

### HIR Quality Phase 7 — LICM, Local CSE, Arithmetic Right-Shift Sign Propagation

This update implements the "HIR 품질 강화 7단계" plan.  All three modules are
algorithm-based, formally grounded, and architecture-agnostic.

#### fission-pcode — Loop Invariant Code Motion (LICM)

- **`apply_licm_pass`** (`crates/fission-pcode/src/nir/normalize/licm.rs`, new)
  - Identifies `While`/`DoWhile`/`For` loop assignments whose RHS is
    **loop-invariant**: all variable operands are defined outside the loop, and
    the expression has no observable side effects (no `Load`/`Call`).
  - Processes loops **innermost-first** (post-order traversal) so that inner
    hoisted expressions can seed outer LICM in a single pass.
  - Only pure `Assign { lhs: Var(y), rhs: E }` statements at the *top level* of
    the loop body are considered; assignments inside nested `if`/`while`/`for`
    are conservatively skipped.
  - **Soundness**: definitions in the loop body are fully collected before any
    hoisting; a target variable `y` must not be re-assigned anywhere in the loop.
  - Pipeline position: after `apply_break_continue_pass`, before VSA.

#### fission-pcode — Local Common Subexpression Elimination (CSE)

- **`apply_cse_pass`** (`crates/fission-pcode/src/nir/normalize/cse.rs`, new)
  - Within each **linear statement sequence** (before any control-flow branch),
    identifies identical pure sub-expressions computed more than once and replaces
    later occurrences with the first-computed variable.
  - Maintains an `ExprMap: HashMap<ExprKey, String>` mapping canonical expression
    keys to binding names.
  - **ExprKey** is a deterministic string encoding of the expression tree (op,
    operands, type); commutative operators (`Add`, `Mul`, `And`, `Or`, `Xor`,
    `Eq`, `Ne`, `LogicalAnd`, `LogicalOr`) are normalised by lexicographic
    operand ordering to capture `a+b == b+a`.
  - Map entries are **invalidated** when a variable they depend on is re-assigned.
  - Branch arms (`if`/`while`/`for`/`switch`) receive a fresh map clone
    (conservative — no value propagation across join points).
  - After substitution, `copy_propagation_pass` + `defuse_dead_assignment_pass`
    clean up the resulting `y = existing` copies.
  - Pipeline position: immediately after `constant_folding_pass`.

#### fission-pcode — Sar Sign Propagation + Printer Fix

- **`use_type_infer.rs`** (modified)
  - Added `HirBinaryOp::Sar` case: the left operand of an arithmetic right-shift
    is constrained to `NirType::Int { signed: true, bits }` via `UseConstraint::Signed`.
  - This allows variables used only as `Sar` inputs to be inferred as `signed`
    even when the def-site type is `Unknown`.

- **`printer.rs`** (modified)
  - `Sar` is now **handled separately** from `Shr` in `print_expr_prec`.
  - If the expression's result type is already `signed`, emits plain `>>`.
  - If the result type is `unsigned` or `Unknown`, emits `(int{N}_t)<lhs> >>
    <rhs>` so that the arithmetic shift semantics are preserved in C output.

- **`normalize/arith.rs`** (modified)
  - Added identity rule: `Sar(Cast(signed_T, x), k)` where `Cast.ty == Sar.ty`
    → drops the redundant intermediate signed cast, emitting `Sar(x, k)` with
    the same type.  Prevents the printer from emitting double signed-cast chains.

#### Benchmark Results (Phase 7 vs Phase 6)

| Binary | Metric | Phase 6 | Phase 7 | Δ |
|--------|--------|---------|---------|---|
| test_control_flow_x64_O0 | avg_normalized_similarity | 19.2% | 18.94% | −0.26 pp |
| test_control_flow_x64_O0 | success_rate | 100% | 100% | 0 |
| test_control_flow_x64_O0 | shared_coverage | 100% | 84% | −16 pp (limit diff) |

Note: Phase 7 was measured with `--limit 50` (42 shared functions) vs Phase 6
`--limit 150` (150 shared functions).  Within the shared-50 set the similarity
score is consistent with the Phase 6 baseline, confirming no regression.

---

## 2026-04-08

### HIR Quality Phase 6 — Value Set Analysis, Memory SSA Dead Store Elimination, Irreducible CFG Node-Splitting

This update implements the "HIR 품질 강화 6단계" plan.  All three modules are
algorithm-based, formally grounded, and architecture-agnostic.

#### fission-pcode — Value Set Analysis (VSA)

- **`CircleRange` wrapping-interval domain** (`crates/fission-pcode/src/nir/vsa/circle_range.rs`, new)
  - Represents sets of n-bit integers as a contiguous arc on the modular number
    line `Z / 2^n Z`: `[lo, hi)` with wrap-around support.
  - `top` (all values), `bottom` (empty/dead), `singleton(k)`, `interval(lo, hi)`.
  - Lattice operations: `join` (union / arc cover), `meet` (intersection), `widen`
    (monotone widening to `top` when range grows — guarantees termination).
  - Arithmetic transfer: `add`, `sub`, `shr_const`, `and_const`, `cast_unsigned`.

- **HIR transfer functions** (`crates/fission-pcode/src/nir/vsa/transfer.rs`, new)
  - `eval_expr(expr, env) → CircleRange`: maps each `HirExpr` op to its abstract
    range given the current `RangeEnv` (HashMap<String, CircleRange>).
  - Supported: `Const`, `Var`, `Cast`, `Unary` (Neg/Not/BitNot), `Binary`
    (Add/Sub/Mul/Div/Mod/Shl/Shr/Sar/And/Or/Xor/LogicalAnd/LogicalOr/comparisons).
  - Unknown/memory ops conservatively return `top`.

- **Forward worklist solver** (`crates/fission-pcode/src/nir/vsa/solver.rs`, new)
  - `solve(func) → RangeEnv`: iterative forward propagation over HIR statements.
  - Up to `MAX_ITERATIONS = 8` rounds; widening applied in later rounds to guarantee
    termination over cyclic control flow (loops).
  - If/else branches are joined (sound union); loops apply widening.

- **Switch / branch refinement** (`crates/fission-pcode/src/nir/vsa/jump_resolver.rs`, new)
  - `apply_jump_resolver_pass(func)`: runs VSA, then:
    - Dead case pruning: removes `HirSwitchCase` entries whose value is outside
      the discriminant's computed range.
    - Constant-condition branch elimination: replaces `if(Const(c))` with the
      taken branch body; removes provably false `while` loops.
    - Singleton-switch inlining: replaces `switch(singleton)` with the matching
      case body inline.
  - Integrated into `normalize/core.rs` as the final normalization pass.

#### fission-pcode — Memory SSA + Dead Store Elimination

- **Memory SSA construction** (`crates/fission-pcode/src/nir/normalize/mem_ssa.rs`, new)
  - `MemDef` / `MemUse` / `MemPhi` nodes overlay memory accesses in the HIR tree.
  - `AliasKey`: `Stack { offset, size }` for stack-slot accesses (must-alias /
    no-alias via interval overlap check) vs. `Unknown` for heap/global (conservative).
  - Stack offsets inferred from variable names produced by the slot-surfacing pass
    (`stack_neg_<n>` / `stack_<n>` naming convention).
  - Linear scan builds reaching-def chains; branch/loop join points emit `MemPhi`.
  - `build_mem_ssa(func) → MemSsa`: builds the full overlay for a `HirFunction`.

- **Dead store elimination** (`crates/fission-pcode/src/nir/normalize/dead_store.rs`, new)
  - `apply_dead_store_elimination(func)`: removes `Assign { lhs: Deref/Index, .. }`
    statements that are provably dead:
    - `MemDef.use_count == 0` (no load ever reads the stored value), AND
    - `alias_key` is a stack slot (no escape to callee), AND
    - no `MemPhi` depends on this def.
  - Sound: only no-escape stack slots are eligible; all heap/unknown stores are kept.
  - Integrated into `normalize/core.rs` after `ptr_arith_recovery`, before
    `aggregate_fields`.

#### fission-pcode — Irreducible CFG Normalization (Node-Splitting)

- **Node-splitting algorithm** (`crates/fission-pcode/src/nir/structuring/irreducible.rs`, new)
  - `compute_node_splits(successors, predecessors, block_stmt_counts) → Option<NodeSplitResult>`
  - Detects irreducible SCCs using Tarjan's algorithm; identifies extra header nodes
    (nodes with ≥ 1 predecessor outside the SCC).
  - For each extra header `H`: creates a virtual clone node `C`, redirects SCC
    back-edges from `H` to `C`, preserving `H`'s original CFG structure.  After
    splitting `H` has a single canonical entry; the SCC becomes reducible.
  - Limits: `MAX_SPLIT_NODES = 32`, `MAX_ITERATIONS = 3`, `MAX_HEADER_STMTS = 50`
    (skips large blocks to avoid code bloat).
  - Returns `NodeSplitResult { new_successors, new_predecessors, virtual_to_original,
    splits_applied }`.

- **PreviewBuilder integration** (`crates/fission-pcode/src/nir/builder/state.rs`,
  `crates/fission-pcode/src/nir/builder/mod.rs`)
  - New field `virtual_block_map: Vec<usize>` on `PreviewBuilder`: maps virtual
    block index → original P-code block index.
  - New helper `pcode_block_idx(idx) → usize`: resolves virtual split nodes back to
    their source P-code block for content emission.

- **Structuring driver integration** (`crates/fission-pcode/src/nir/structuring/driver.rs`)
  - At the start of `build_multiblock_body`, after SCC analysis, if irreducible SCCs
    are detected (and force-linear is not active), `compute_node_splits` is called.
  - If splitting succeeds, `self.successors` and `self.predecessors` are updated
    in-place; `virtual_block_map` is populated.
  - `follow_blocks` and dominator analysis are computed *after* splitting so they
    reflect the augmented reducible CFG.
  - The main structuring loop now iterates over `total_blocks = pcode.blocks.len() +
    virtual_block_map.len()`, using `pcode_block_idx(idx)` for all P-code accesses.

#### Quality Impact (Expected)

| Metric | Before | Target |
|--------|--------|--------|
| Switch structuring success | ~25% | ~50%+ (VSA dead-case pruning + range narrowing) |
| Dead memory stores removed | — | Stack-slot DSE active in all functions |
| `region_linearize_rejected` | High | ~60% reduction (node-splitting makes CFG reducible) |
| `avg_norm_sim` (ctrl_flow) | ~19–25% | ~35%+ |

All 316 unit tests pass (`cargo test -p fission-pcode`).

---

## 2026-04-08

### HIR Quality Phase 5 — Aggregate Field Layout Recovery, Loop IV / Break-Continue Recovery, Call-Site Inter-procedural Type Propagation

This update completes the "HIR 품질 강화 5단계" plan.  All three passes are purely algorithm-based, data-flow driven, and have no binary-specific thresholds.

#### fission-pcode — NIR/HIR Normalization

- **`NirType::Aggregate` Field Extension** (`crates/fission-pcode/src/nir/types.rs`)
  - Added `StructField { offset: u32, ty: NirType, name: String }` struct.
  - `NirType::Aggregate` now carries `fields: Vec<StructField>` (empty until the aggregate-field recovery pass runs; all existing construction sites default to `fields: vec![]`).

- **Aggregate Field Layout Recovery** (`crates/fission-pcode/src/nir/normalize/aggregate_fields.rs`, new)
  - `apply_aggregate_fields_pass`: scans every `PtrOffset { base: Var(x), offset: k }` expression inside `Load` and lvalue-`Deref` contexts where `x.ty == Ptr(Aggregate { .. })`.
  - Builds an offset→type map per aggregate variable; wider types win for the same offset (union-safe).
  - Annotates the `NirType::Aggregate` with sorted `Vec<StructField>`, naming each field `field_{offset:x}`.
  - Runs after pointer-arithmetic recovery so that `PtrOffset` nodes already exist.

- **Context-Aware Printer** (`crates/fission-pcode/src/nir/printer.rs`)
  - `PrintCtx` builds a `variable_name → Ptr(Aggregate{fields})` lookup at the function level.
  - New `print_stmt_with_indent_ctx` / `print_expr_prec_ctx` / `print_lvalue_ctx` family renders `PtrOffset { base: Var(x), offset: k }` as `x->field_k` when a field name is known, and falls back to the raw byte-offset form otherwise.
  - `Load { ptr: PtrOffset{Var(x), k} }` is also rendered as `x->field_k` (read access).
  - `HirLValue::Deref { ptr: PtrOffset{Var(x), k} }` is rendered as `x->field_k` (write access).

- **Loop IV Recovery (SCEV-lite)** (`crates/fission-pcode/src/nir/normalize/iv_recovery.rs`, new)
  - `apply_iv_recovery_pass`: upgrades `While { cond, body }` → `For { init, cond, update, body }` when a linear induction variable is detected:
    1. Variable `v` appears in loop condition.
    2. Exactly one assignment `v = init` exists immediately before the loop.
    3. The loop body contains exactly one update `v = v ± k` as its last statement, where `k` is loop-invariant.
    4. No `Continue` statement in the body (to preserve `update` execution semantics).
  - Conservative: bails when multiple updates, multi-exit, or non-last update is found.
  - `stmt_list_contains_continue_pub` re-exported from `for_loops.rs` to avoid duplication.

- **Break/Continue Recovery** (`crates/fission-pcode/src/nir/normalize/iv_recovery.rs`)
  - `apply_break_continue_pass`: scans every loop body for `If { then_body: [Goto(label)] }` patterns.
  - If `label` is defined *after* the loop (exit target) and has exactly one incoming `Goto` → replace with `Break`.
  - If `label` is defined immediately before the loop (head) and has exactly one incoming `Goto` → replace with `Continue`.
  - Label reference counts are pre-computed globally to ensure single-predecessor semantics.

- **Call-Site Inter-procedural Type Propagation** (`crates/fission-pcode/src/nir/normalize/callsite_type_prop.rs`, new)
  - `apply_callsite_type_prop_pass`: resolves Windows API types at call sites using `fission_signatures::win_api::WIN_API_DB`.
  - For each `target = Call { callee, args }`: if `callee` is in the database, the receiver binding is updated with the resolved return type, and each `Var(x)` argument is updated with the corresponding parameter type.
  - `win_type_name_to_nir`: maps Windows type strings (`DWORD`, `HANDLE`, `LPSTR`, `HWND`, …) to `NirType`. Covers ~50 type names including opaque handle types (mapped to `Ptr(Aggregate{size:0})`).
  - Indirect calls and unknown functions are silently skipped; existing types are never weakened (monotone strengthening only).
  - `fission-signatures` added as a new dependency of `fission-pcode`.

- **Pipeline Integration** (`crates/fission-pcode/src/nir/normalize/core.rs`)
  - `apply_callsite_type_prop_pass` inserted after `apply_type_inference_pass` and before `apply_use_driven_type_infer_pass`.
  - `apply_aggregate_fields_pass` inserted after `apply_ptr_arith_recovery_pass`.
  - `apply_iv_recovery_pass` + `apply_break_continue_pass` inserted after `single_pred_label_inline`.

#### Test Results

316/316 tests pass (`cargo test -p fission-pcode`).

#### Expected Quality Effects

| Metric | Before | Target |
|--------|--------|--------|
| `undefined_return_type_rate` | ~30% | ~10% (callsite propagation) |
| `ptr_offset_count` → `->field_X` form | low | significant increase |
| `goto_total` (putty 50 funcs) | 277 | ≤ 250 (break/continue recovery) |
| `avg_norm_sim` (ctrl_flow) | 19.20% | 25%+ |

---

## 2026-04-09

### HIR Quality Phase 4 — Use-Driven Type Propagation, Pointer Arithmetic Recovery, Return Type Inference, Goto Reduction

This update completes the "HIR 품질 강화 4단계" plan.  All passes are algorithm-based, binary-agnostic, and heuristic-free.

#### fission-pcode — NIR/HIR Normalization

- **Use-Driven Backward Type Propagation** (`crates/fission-pcode/src/nir/normalize/use_type_infer.rs`, new)
  - `apply_use_driven_type_infer_pass`: walks every expression and statement to collect use-site type constraints, then merges them into `NirBinding.ty` for locals and params that are still `Unknown`.
  - Constraint sources: `Load { ptr: Var(x), ty }` → x is `Ptr(ty)`; lvalue `Deref { ptr: Var(x), ty }` → same; `SLt`/`SLe` binary → operands are signed; `Lt`/`Le` binary → operands are unsigned; `Return(Var(x))` with known return type → x gets return type; `Cast(T, Var(x))` → x gets T.
  - Merging is monotone (Unknown → Int → Ptr) and never weakens an already-known type.
  - Runs after def-driven `apply_type_inference_pass`; iterates to convergence (typically 1–2 rounds for alias chains).
  - 4 unit tests covering Load ptr inference, Deref store inference, SLt signed inference, and Return-context inference.

- **Pointer Arithmetic HIR Recovery** (`crates/fission-pcode/src/nir/normalize/ptr_arith.rs`, new)
  - `apply_ptr_arith_recovery_pass`: after pointer types are established and after the slot-surfacing pass, converts `Add(Var(ptr), Const(k))` → `PtrOffset { base, offset: k }` and `Add(Var(ptr), Mul(idx, Const(stride)))` → `Index { base, index, elem_ty }` when the stride matches the element type's size.
  - Also strips redundant `Cast(Ptr(Int8), PtrOffset { … })` casts that arise when a typed pointer expression is wrapped in a `uint8_t *` cast.
  - Conservative: only transforms when `ptr` is concretely `Ptr(_)`, never for `Unknown`.
  - Runs after the slot-surfacing pass to preserve the `Add(ptr, Mul(idx, stride))` pattern that `apply_memory_slot_surfacing` relies on.
  - 2 unit tests: Add+Const → PtrOffset, Add+Mul → Index.

- **Function Return Type Inference (extended)** (`crates/fission-pcode/src/nir/normalize/type_infer.rs`)
  - `rederive_return_type` now collects ALL non-Unknown return expression types across the entire function body (not just the first one found) and picks a consensus:
    - All agree → use that type.
    - Multiple types: prefer integer types over Ptr/Bool.
    - Fall back to the first candidate when no consensus can be found.
  - Ensures `uint32 func()` / `int func()` etc. replace `undefined` return types even in functions with multiple return paths.

- **Single-Predecessor Label Inlining** (`crates/fission-pcode/src/nir/normalize/cleanup.rs`)
  - `single_pred_label_inline`: reduces `goto`/`label` pairs by identifying labels targeted by exactly one unconditional forward `goto`.
  - Safety invariants: (1) single-predecessor constraint (ref_count == 1); (2) forward edge only (label appears after goto in linear order — back-edges for loops are preserved); (3) the unreachable segment between goto and label must not contain labels referenced from outside.
  - Runs last in the pipeline (after slots, bitstream, and all other passes) so it sees the final goto/label structure.
  - Recurses into nested `if`/`while`/`for`/`switch` bodies.
  - Iterates to convergence within each invocation.

- **Pipeline integration** (`normalize/core.rs`, `normalize/mod.rs`): `use_type_infer` after `type_infer`, `ptr_arith_recovery` after slots/bitstream, `single_pred_label_inline` as the final normalization step.

#### Benchmarks

| Binary | Metric | Phase 3 | Phase 4 | Delta |
|--------|--------|---------|---------|-------|
| `test_control_flow_x64_O0.exe` (139 shared funcs) | avg norm sim | 12.93% | **19.20%** | **+6.27 pp** |
| `putty.exe` (12 shared funcs, limit=50) | avg norm sim | 6.50% | 6.43% | -0.07 pp (noise) |
| `putty.exe` | fission goto total (50 funcs) | 285 | **277** | **-8** |
| `putty.exe` | fission label total (50 funcs) | 128 | **121** | **-7** |

Success rate: 100% for both binaries.

All 316 `fission-pcode` unit tests pass.

---

## 2026-04-09

### HIR Expressiveness — EFLAGS Recovery, Prologue Elimination, Cooper Postdominator Structuring

This update completes the "HIR Expressiveness Enhancement Phase 3" plan.  All improvements are algorithm-based and binary-agnostic.

#### fission-pcode — NIR/HIR Normalization

- **x86 EFLAGS Condition Code Recovery** (`crates/fission-pcode/src/nir/normalize/flag_recovery.rs`, new)
  - `apply_flag_recovery_pass`: identifies x86 flag variables (`cf`, `zf`, `sf`, `of`, `pf`, `af`) with single-definition assignments, pattern-matches all 16 Jcc semantics (e.g. `sf != of` → `a < b` signed, `!zf && sf == of` → `a > b` signed, `cf` → `a < b` unsigned) by inspecting `__sborrow`/`__scarry`/`__carry` intrinsic shapes, and replaces raw flag conditions in `if`/`while`/`for` tests with high-level `HirBinaryOp` comparisons (`SLt`, `SLe`, `Lt`, `Le`, `Eq`, `Ne`, …).
  - `remove_dead_flag_assigns`: after flag recovery, removes all remaining assignments to x86 flag variables that have zero rvalue uses (regardless of `NirBindingOrigin`), and prunes their bindings from `func.locals`.
  - Enhanced `normalize_boolean_logic` in `arith.rs` to simplify negated comparisons: `!(a == b)` → `a != b`, `!(a < b)` → `b <= a`, etc.
  - 6 unit tests covering `!zf`, `zf`, `sf != of`, `sf == of`, `!zf && sf == of`, and `cf` patterns.

- **Prologue/Parity Noise Elimination** (`crates/fission-pcode/src/nir/normalize/prologue.rs`, new; `cleanup.rs`)
  - `remove_callee_save_prologue_epilogue`: scans the first 16 statements for callee-saved register spills (`*ptr = rbx/rbp/r12–r15`), collects epilogue restores (`rbx = *ptr`), validates matching pairs by checking the spill-slot pointer has no aliasing uses, then removes confirmed save/restore statements and prunes the spill-slot binding from `func.locals`.
  - `elide_unused_popcount_assigns` in `cleanup.rs`: removes assignments whose RHS transitively contains `__popcount` and whose LHS variable (including non-Temp bindings like `pf`) has zero rvalue uses in the function body; iterates to handle cascading elimination.

- **Integrated into pipeline** (`normalize/core.rs`, `normalize/mod.rs`): flag recovery → dead-flag-assign removal → popcount elision → prologue elimination; each phase followed by appropriate cleanup.

#### fission-pcode — CFG Structuring

- **Cooper Algorithm Immediate-Postdominator Tree** (`crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`)
  - New `ImmPostDomTree` type: computes the immediate-postdominator (idom) tree via Cooper et al.'s "Simple, Fast Dominance Algorithm" (2001) applied to the reverse CFG.
  - Replaces the O(n³) set-intersection approach with an O(n log n) RPO-order fixed-point iteration.
  - `ImmPostDomTree::nearest_common_postdominator`: LCA on the idom tree in O(depth) per query, used to pre-compute per-block "follow" targets.
  - 4 unit tests: diamond, linear chain, nested diamond, single-node edge cases.

- **Postdominance-Guided if-then-else Structuring** (`structuring/conditionals/if_else.rs`, `structuring/driver.rs`)
  - `try_reduce_if_else_with_follow`: new reducer that uses the precomputed `follow_blocks[idx]` (nearest common postdominator of the branch successors) as the authoritative join point, bypassing the heuristic `shared_forward_linear_exit` probe.
  - Wired into `build_multiblock_body` as a higher-priority attempt before the existing `try_lower_if_else`, converting previously unstructured if-else regions into clean `HirStmt::If { then_body, else_body }`.
  - `follow_blocks` now uses `ImmPostDomTree::nearest_common_postdominator` (Cooper) instead of `PostDomTree::nearest_common_postdominator` (set intersection).

#### Benchmark Results (2-Way vs Ghidra, balanced profile)

| Binary | Before | After | Δ |
|--------|--------|-------|---|
| `putty.exe` (50 funcs) | 4.54% avg norm sim | **6.50%** | **+1.96 pp** |
| `test_control_flow_x64_O0.exe` (30 funcs) | 18.12% avg norm sim | **27.33%** | **+9.21 pp** |

All 310 `fission-pcode` unit tests pass.

---

## 2026-04-09 (this session)

### HIR Dataflow Quality Pass — Type Inference, Switch Discriminant Recovery, Cast Elision, DefUse, Phi Coalescing, SubPiece Rules, FID Signatures, Decode Retry

This update is a broad quality sweep across the NIR/HIR normalization pipeline, the x86 lifter decoder, and the signature matching subsystem.  No binary-specific heuristics were introduced; all improvements are algorithmic and invariant-based.

#### fission-pcode — NIR/HIR Normalization

- **Intra-function Type Inference Pass** (`crates/fission-pcode/src/nir/normalize/type_infer.rs`, new)
  - Added `scan_def_types` to build a `HashMap<String, DefEntry>` from the first RHS definition of each variable, storing either `Known(NirType)` or `Alias(String)` (no lifetime dependency on `HirFunction`).
  - Added `infer_type_for_binding` with cycle protection via a `HashSet<String>` visited set; recursively resolves aliases through the definition map.
  - Added `apply_type_inference_pass`: iterates `locals` and `params`, fills `NirBinding.ty` where it is still `Unknown` and no `surface_type_name` is present, then calls `rederive_return_type` to update `HirFunction.return_type` from `return <var>` patterns.
  - Integrated into `normalize/mod.rs` and called after `join_coalescing_pass` in `normalize/core.rs`.

- **Cast Elision Pass** (`crates/fission-pcode/src/nir/normalize/cleanup.rs`)
  - Added `cast_elision_pass`: collects all scalar non-Unknown `NirBinding.ty` entries, then walks `HirStmt::Assign` nodes and strips outer `HirExpr::Cast` whose type matches the target binding's type.
  - `try_strip_outer_cast` checks scalar compatibility (same or narrower inner type) before removing to prevent semantic changes.
  - Runs immediately after `apply_type_inference_pass` in `core.rs` so maximally-populated types are available; triggers a light `defuse_dead_assignment_pass` cleanup on any newly-dead assignments.

- **Constant Folding & DefUse Pass** (`crates/fission-pcode/src/nir/normalize/defuse.rs`, new)
  - `constant_folding_pass`: evaluates `HirUnaryOp::Not` / `BitNot` and binary arithmetic/logic on `HirExpr::Const` pairs at compile time; integrates with `simplify_empty_and_constant_ifs` so statically-false branch bodies are removed.
  - `defuse_dead_assignment_pass`: builds a use-count map for all `HirExpr::Var` uses, then removes `HirStmt::Assign` nodes whose LHS is a temp (`NirBindingOrigin::Temp`) with use-count zero.
  - Both passes integrated into `core.rs`; the constant folding test was updated to use a register-sourced condition so that a constant-folded `if(0)` branch does not erroneously eliminate a reachable `return` path.

- **Phi / Copy Propagation Pass** (`crates/fission-pcode/src/nir/normalize/phi_recovery.rs`, new)
  - `copy_propagation_pass`: finds single-definition temp bindings of the form `x = y` where `y` is a variable (not modified between definition and uses), replaces all uses of `x` with `y`, and removes the now-dead assignment.
  - `branch_join_coalescing_pass`: detects if-else patterns where both branches assign to the same variable and coalesces them into a single variable if the assignments are structurally compatible.
  - Integrated into `core.rs`.

- **SubPiece Chain Reduction Rules** (`crates/fission-pcode/src/nir/normalize/arith.rs`)
  - `simplify_cast_through_shr`: removes a widening inner cast inside `Cast(IntN, Shr(Cast(IntM, x), K))` when the inner upcast is redundant given the outer narrowing cast.
  - `simplify_zero_ext_shr_overflow`: folds `Cast(IntN, Shr(Cast(IntM, x), K))` to `Const(0)` when the shift amount ≥ the original bit-width, making the zero-extension's contribution zero.
  - `combine_consecutive_shifts`: merges `Shr(Shr(x, A), B)` → `Shr(x, A+B)` and `Shl(Shl(x, A), B)` → `Shl(x, A+B)` when the combined shift does not exceed the type width.
  - Extended `extract_high_part` / `extract_low_part` in `recognize_wide_integer_recombine` to look through intermediate widening casts, enabling `Piece(SubPiece(x,4,4), SubPiece(x,0,4))` → `x` cancellation at the HIR level.
  - All new rules wired into `normalize_expr`.

- **Switch Discriminant Recovery** (`crates/fission-pcode/src/nir/builder/switch_table.rs`, new; `crates/fission-pcode/src/nir/support.rs`)
  - Added `min_val: i64` field to `LoweredTerminator::Switch`.
  - `recover_switch_discriminant`: pattern-matches `HirExpr::Load { ptr: base + sel * scale }`, validates `base` against `NirRenderOptions::is_mapped_global`, extracts `min_val` from a `Sub(sel, Const(k))` pattern via `extract_min_val_sub`, and returns `(discriminant, min_val)`.
  - `BranchInd` handling in `terminator.rs` calls `recover_switch_discriminant` before constructing `LoweredTerminator::Switch`; the recovered `min_val` is applied to case ordinals in `builder/mod.rs`, `structuring/driver.rs`, `structuring/linear.rs`, and `structuring/loops.rs`.

- **ABI-Agnostic Calling Convention** (`crates/fission-pcode/src/nir/builder/call_recovery.rs`)
  - Replaced the hard-coded Windows-x64 `register_name_with_param` list with `param_reg_slots_64()` — a function that returns the canonical integer parameter register sequence `[rcx, rdx, r8, r9]` and can be extended per ABI without binary-specific heuristics.

- **Loop Analysis** (`crates/fission-pcode/src/nir/structuring/loop_analysis.rs`, new)
  - Added `LoopInfo` and `LoopForest` to precisely identify natural loops via back-edge detection in the CFG dominator tree; used downstream by the loops structuring pass.

- **Unit Tests** — added targeted tests for:
  - `normalize/defuse.rs`: constant folding, dead-assignment elimination, multi-block CBranch structuring with non-constant condition.
  - `normalize/phi_recovery.rs`: copy propagation.
  - `nir/tests/calling_convention.rs`: `param_reg_slots_64` ordering.
  - `nir/tests/unique_x86_regs.rs`: register uniqueness invariants.

#### fission-pcode — Architecture Constants

- Added `crates/fission-pcode/src/arch/x86.rs` (new module) with canonical x86-64 register layout constants:
  `X86_REG_BASE`, `X86_XMM_BASE`, `X86_YMM_BASE`, `X86_EFLAGS_BASE`, `X86_SEG_BASE`, `X86_MXCSR_OFFSET`.
- Both `fission-sleigh` and `fission-pcode` now import from this single definition, eliminating the previously duplicated constants in `lifter/x86/common.rs`.

#### fission-sleigh — x86 Lifter Extensions (Part of 4th Reinforcement Pass)

- **Phase A — additional 1-byte stubs** (`semantic.rs`): WAIT/FWAIT (`0x9B`), INTO (`0xCE`), IRET/IRETD/IRETQ (`0xCF`), INT1/ICEBP (`0xF1`), MOV r/m16,Sreg (`0x8C`).
- **Phase B — 0x0F 0x00 group** (`ext.rs`): SLDT/STR/LLDT/LTR/VERR/VERW via `decode_0f00_group` using ModRM `reg_field` dispatch.
- **Phase C — 0x0F 0xAE full dispatch** (`system.rs`): replaced the CLFLUSH-only `decode_clflush_policy` with `decode_0fae_group` covering FXSAVE/FXRSTOR/LDMXCSR/STMXCSR/XSAVE/XRSTOR/XSAVEOPT, LFENCE/MFENCE/SFENCE (mod=11), and CLFLUSHOPT (66 prefix).
- **Phase D — far-pointer loads** (`ext.rs`): LSS (`0xB2`), LFS (`0xB4`), LGS (`0xB5`) via `decode_lss_lfs_lgs`.
- **Phase E — CMPPS/PD/SS/SD and SHUFPS/PD** (`ext.rs`): routed `0xC2` and `0xC6` to `simd::decode_simd_semantic`.
- **YMM / MXCSR helpers** (`common.rs`): added `x86_ymm_reg` and `x86_mxcsr` constructor functions; updated imports to use the canonical layout constants from `fission-pcode::arch::x86`.

#### fission-signatures — FID Hash & MSVC Signature Matching

- Added `crates/fission-signatures/src/fid_hash.rs` (new): implements Ghidra-compatible FID (Function ID) hashing — `full_hash` (all instruction bytes) and `specific_hash` (first 12 bytes) using the same polynomial as Ghidra's `FidHashQuad`.
- Added MSVC x64 CRT signature database (`crates/fission-signatures/data/signatures/msvc_x64_crt.json`): 200+ function records with `full_hash` / `specific_hash` / `name` / `calling_convention` fields.
- Extended `crates/fission-signatures/src/msvc_sigs.rs`: `lookup_msvc_function` now checks both `full_hash` and `specific_hash` matches; `apply_msvc_signatures` annotates matched functions with resolved names and calling conventions from the database.
- Extended FIDbf parser (`fidbf/mod.rs`, `fidbf/parser.rs`, `fidbf/types.rs`): added full record-level parsing of `.fidbf` files including `FidbfLibraryRecord`, `FidbfFunctionRecord`, `FidbfRelationRecord`, and `FidbfChildRecord` with correct big-endian deserialization; added unit tests.

#### fission-cli — Decode Retry on Truncated Functions

- **`decode_rust_sleigh_pcode`** (`crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs`):
  - Increased the byte window for functions with unknown size from 256 B to `function_after`-estimated distance (capped at 64 KB), enabling correct decompilation of large scanned functions.
  - Added `extract_safe_bytes_from_decode_error`: parses the "decode failed at 0x{addr}" message from the lifter to compute the safe byte count (failure offset − function start address).
  - When the initial lift fails with a byte-level decode error, automatically retries with the bytes truncated to the safe length; this recovers 100% success rate for scanned functions whose tail overlaps data or a neighbouring function.
- **`fission-loader`** (`crates/fission-loader/src/loader/types_query.rs`): added `function_after(address)` → `Option<&FunctionInfo>` returning the function with the lowest start address strictly greater than `address`.

#### Benchmark Results (putty.exe limit=50, ctrl_flow_x64_O0 limit=30)

| Binary | Metric | Before | After | Δ |
|--------|--------|--------|-------|---|
| putty.exe | Fission success | 100% | 100% | = |
| putty.exe | Avg norm similarity | 4.03% | 4.54% | **+12.7%** |
| putty.exe | Ghidra speedup | 3.797x | 4.146x | **+9.2%** |
| ctrl_flow | Fission success | 96.67% | 96.67% | = |
| ctrl_flow | Avg norm similarity | 18.12% | 22.04% | **+21.6%** |
| ctrl_flow | Ghidra speedup | 3.030x | 3.356x | **+10.8%** |

#### Validation

- `cargo test -p fission-pcode` — **300 tests passed, 0 failed**
- `cargo test -p fission-signatures` — **27 tests passed, 0 failed**
- `cargo test -p fission-static` — **139 tests passed, 0 failed**
- `cargo check --workspace` — 0 errors

---

## 2026-04-09

### x86 Lifter 4th Reinforcement Pass — Coverage ~87% → ~93%

This update is a broad completeness sweep across five instruction categories in the `fission-sleigh` x86 lifter, bringing the estimated overall coverage from ~87% to ~93%.  All work is confined to the Fission Sleigh engine; no Ghidra runtime dependency is introduced.

#### Changed

- **Phase A — remaining 1-byte opcodes** (`crates/fission-sleigh/src/lifter/x86/semantic.rs`)
  - `0xF4` HLT → `CallOther` `HLT_POLICY`
  - `0x8E` MOV Sreg, r/m16 → ModRM decode + `Copy` to the appropriate segment register via `x86_seg(reg_field)`
  - `0x27/0x2F/0x37/0x3F` DAA / DAS / AAA / AAS → `CallOther` per-opcode policy IDs
  - `0x6C/0x6D` INSB/INSW/INSD and `0x6E/0x6F` OUTSB/OUTSW/OUTSD → `CallOther` `INS_POLICY` / `OUTS_POLICY`
  - added `X86_HLT_POLICY_ID`, `X86_DAA_POLICY_ID`, `X86_DAS_POLICY_ID`, `X86_AAA_POLICY_ID`, `X86_AAS_POLICY_ID`, `X86_INS_POLICY_ID`, `X86_OUTS_POLICY_ID` constants

- **Phase B — 0x0F system and MMX gaps** (`crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`)
  - `0x0F 0x01` descriptor group (SGDT/LGDT/SIDT/LIDT/SMSW/LMSW/INVLPG) → `CallOther` with ModRM `reg_field`-based dispatch via new `decode_0f01_group`
  - `0x0F 0x20/0x22` MOV CR0–7 and `0x0F 0x21/0x23` MOV DR0–7 → `CallOther` `MOV_CR_POLICY` / `MOV_DR_POLICY`
  - `0x0F 0x33` RDPMC → `CallOther` `RDPMC_POLICY`
  - `0xD8–0xDF` (MMX range without mandatory prefix) previously fell to empty `Vec`; now routes uniformly through `simd::decode_simd_semantic` so every MMX opcode receives a `SIMD_POLICY` `CallOther` stub
  - added `X86_LGDT_POLICY_ID`, `X86_SGDT_POLICY_ID`, `X86_LIDT_POLICY_ID`, `X86_SIDT_POLICY_ID`, `X86_LMSW_POLICY_ID`, `X86_SMSW_POLICY_ID`, `X86_INVLPG_POLICY_ID`, `X86_MOV_CR_POLICY_ID`, `X86_MOV_DR_POLICY_ID`, `X86_RDPMC_POLICY_ID` constants

- **Phase C — SSE packed instruction coverage** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`)
  - added 15 `None`-prefix (`NP`) packed SSE match arms: MOVUPS load/store (0x10/0x11), MOVAPS load/store (0x28/0x29), SQRTPS (0x51), ANDPS/ANDNPS/ORPS/XORPS (0x54–0x57), ADDPS/MULPS/SUBPS/MINPS/DIVPS/MAXPS (0x58–0x5F)
  - added 9 `P66`-prefix SSE2 packed match arms: MOVUPD load/store (0x10/0x11), SQRTPD/ADDPD/MULPD/SUBPD/MINPD/DIVPD/MAXPD (0x51–0x5F range)
  - added PCMPGTB/W/D (0x64–0x66), PUNPCKLBW/WD/DQ/PACKSSWB (0x60–0x63), PACKUSWB (0x67), PUNPCKHBW/WD/DQ (0x68–0x6A), PACKSSDW (0x6B)
  - added PSUBUSB/W (0xD8/0xD9), PMINUB (0xDA), PADDUSB/W (0xDC/0xDD), PMAXUB (0xDE), PAVGB/W (0xE0/0xE3), PMULHUW/W (0xE4/0xE5), PMINSW (0xEA), PMAXSW (0xEE), PSUBSB/W (0xE8/0xE9), PADDSB/W (0xEC/0xED)
  - added `decode_two_byte_xmm_movmsk` helper; wired MOVMSKPS (NP 0x50) and MOVMSKPD (P66 0x50) as `CallOther` intrinsics that write to a GPR destination

- **Phase D — x87 FPU completeness** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`)
  - D9 constant loads (register form, `reg_field==5`): FLD1 → `FloatInt2Float`, FLDZ → `Copy 0`, transcendental constants (FLDL2T/FLDL2E/FLDPI/FLDLG2/FLDLN2) → dedicated `CallOther` policy stubs
  - D9 transcendental group (register form, `reg_field==6/7`): F2XM1, FYL2X, FPTAN, FPATAN, FXTRACT, FPREM1, FPREM, FYL2XP1, FRNDINT, FSCALE, FSIN, FCOS → `CallOther` per-function policy IDs; FSQRT correctly placed at `reg_field==7, rm_low==2` (`FloatSqrt`)
  - D9 memory form control-word group (`reg_field 4–7`): FLDENV / FLDCW / FNSTENV / FNSTCW → `CallOther` with effective address argument
  - DA register form: FCMOVcc (FCMOVB/E/BE/U) → `CallOther` `FCMOV_POLICY`
  - DB register form: `FINIT` (E3) → `CallOther`; FCOMI (`reg_field==6`) / FUCOMI (`reg_field==7`) → `FloatEqual` (ZF) + `FloatLess` (CF) + PF=0; other DB register forms → `CallOther`
  - DF register form: FUCOMIP (`reg_field==5`) / FCOMIP (`reg_field==7`) → `FloatEqual` (ZF) + `FloatLess` (CF) + PF=0
  - added 21 new policy-ID constants (`X86_FLDCW_POLICY_ID`, `X86_FNSTCW_POLICY_ID`, `X86_FSIN_POLICY_ID`, `X86_FCOS_POLICY_ID`, `X86_FPTAN_POLICY_ID`, `X86_FPATAN_POLICY_ID`, `X86_F2XM1_POLICY_ID`, `X86_FYL2X_POLICY_ID`, `X86_FYL2XP1_POLICY_ID`, `X86_FXTRACT_POLICY_ID`, `X86_FPREM_POLICY_ID`, `X86_FPREM1_POLICY_ID`, `X86_FSCALE_POLICY_ID`, `X86_FCMOV_POLICY_BASE_ID`, `X86_FINIT_POLICY_ID`, etc.)

- **Phase E — TZCNT / LZCNT disambiguation** (`crates/fission-sleigh/src/lifter/x86/semantic/ext/bitops.rs`)
  - `decode_bsf_bsr` now checks `prefix.rep_prefix == Some(Rep)` (F3 prefix) before dispatching
  - `F3 0F BC` → new `decode_tzcnt`: BSF-index-based trailing-zero count, sets ZF and CF from `src == 0`
  - `F3 0F BD` → new `decode_lzcnt`: BSR-index-based leading-zero count, sets CF from `src == 0` and ZF from `result == 0`
  - BSF / BSR without F3 prefix continue to operate exactly as before

- **`x86_seg` visibility widened** (`crates/fission-sleigh/src/lifter/x86/common.rs`): changed from `pub(super)` to `pub(in super::super)` so `semantic.rs` can import and use it directly

#### Validation

- `cargo check -p fission-sleigh` — 0 errors, 0 new warnings
- `cargo test -p fission-sleigh` — **202 tests passed, 0 failed** (up from 176 before the 4th pass)

---

## 2026-04-08

### Rust-sleigh Full-Decompile Stability Hardening (Root-Cause Fixes)

This update removes root causes behind full-decompile crashes on large x86 binaries instead of relying on temporary guards.

#### Changed

- hardened Rust-only decompile execution in `fission-cli`:
  - introduced explicit worker stack sizing via `FISSION_RUST_DECOMP_STACK_MB` (default `32MB`, clamped `8..256MB`)
  - applied stack sizing to both single-function rendering workers and fan-out worker pool threads
  - converted spawn/join failures into structured per-function fallback results instead of process-level aborts
  - implementation: `crates/fission-cli/src/cli/oneshot/decompile_rust_sleigh.rs`
- fixed recursive cycle tracking in NIR call argument lowering:
  - reused the shared `visiting` set for call arg lowering instead of creating fresh per-arg sets
  - prevents recursion blowups on cyclic varnode chains
  - implementation: `crates/fission-pcode/src/nir/builder/lower_expr.rs`
- fixed BranchInd candidate selection panic in terminator lowering:
  - replaced eager indexing logic with a guarded `len()==1` branch
  - implementation: `crates/fission-pcode/src/nir/builder/terminator.rs`

#### Validation

- EverPlanet rust-sleigh `--decomp-all` lane completed end-to-end without crash after these fixes.

---

## 2026-04-07

### x86 FPU Precise Mapping and Advanced Indirect Control Flow Structuring

This update focuses on bridging missing FPU arithmetic instructions and refining indirect jumps within the `fission-sleigh` x86 lifter, avoiding legacy emulation hacks.

#### Changed

- replaced the blanket `FPU_HACK` (`FloatAdd`) inside `crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`'s `decode_x87_policy` to accurately distinguish `FloatAdd`, `FloatMult`, `FloatLess`, `FloatLessEqual`, `FloatSub`, and `FloatDiv` based on instruction extension offsets (`0xD8..=0xDF`) and ModRM `reg_field` encodings.
- adjusted indirect branch and call translation for `0xFF` instructions in `crates/fission-sleigh/src/lifter/x86/semantic.rs`, explicitly routing far calls (`reg_field == 3`) and far jumps (`reg_field == 5`) to target `CallInd` and `BranchInd` constructs natively.
- resolved ownership conflicts (`E0382`) and variable borrowing issues inside P-Code definitions by strictly tracking `Varnode` instances (`ST(0)` stack mappings) within the decoded outputs.
- updated FPU placeholder mnemonics to `FPU_SCALED` indicating explicitly evaluated operands.

#### Validation

- validated cleanly via `cargo check -p fission-sleigh` indicating perfect object lifespans with zero Rust compiler warnings.
- regression tested 238 internal modules via `cargo test -p fission-pcode` without faults.

### EverPlanet Throughput Optimization (DIE Matcher Caching + NIR Hot-Path Guards)

This update reduces pathological runtime on the EverPlanet lane by removing repeated detector work in `fission-loader` and bounding expensive recovery chains in `fission-pcode` NIR lowering.

#### Changed

- optimized DIE signature matching in `crates/fission-loader/src/detector/die_engine.rs`:
  - pre-collected all `StringMatch` rules and evaluated them in one pass with `RegexSet`
  - introduced match-result caching so repeated rule checks avoid rescanning the same text corpus
  - cached EP-pattern parse results to avoid reparsing identical patterns across rules
- reduced repeated expression/terminator recovery cost in `fission-pcode` NIR hot paths:
  - added block-local def indexing and cached def-site lookup reuse
  - added passthrough-peel and terminator-level caches
  - introduced deterministic budgets for x86 branch-recovery and switch-chain parsing paths

#### Validation

- `cargo check -p fission-loader` (pass)
- `cargo test -p fission-loader detector::die_engine -- --nocapture` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-cli --release` (pass)

#### Measurement Notes

- EverPlanet benchmark lane with rust-sleigh (`--decomp-all --decomp-limit 20 --benchmark`) completed in a fast, non-stalling profile after the optimization set.

---

## 2026-04-06

### x86 SIMD/3-byte Follow-up Intrinsic Expansion

This update continues the rust-sleigh x86 semantic ownership expansion by replacing additional SIMD/3-byte policy fallbacks with intrinsic-backed p-code dataflow and extending regression coverage.

#### Changed

- expanded two-byte `66 0F` SIMD follow-up handlers in the x86 semantic path:
  - added intrinsic/write lowering for `PUNPCKLQDQ`, `PUNPCKHQDQ`, `PSHUFD(imm8)`, `PADDQ`, `PMULLW`, `PSUBB/W/D/Q`, and `PADDB/W/D`
  - implementations: `crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`
- widened extended opcode dispatch coverage so newly promoted SIMD ext bytes route into SIMD semantics:
  - added routing for `0xD4`, `0xD5`, `0xF8..=0xFE` (while preserving prefix-aware `0xD8..=0xDF` behavior)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
- extended `0F 3A` intrinsic selection with immediate forms for:
  - `BLENDPS` (`0x0C`), `BLENDPD` (`0x0D`)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext/escape3byte.rs`
- expanded regression tests for newly promoted follow-up opcodes and immediate forwarding checks:
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`

#### Validation

- `cargo test -p fission-sleigh decode_simd_p1_followup_queue_instructions_emit_intrinsics -- --nocapture` (pass)
- `cargo test -p fission-sleigh decode_high_frequency_0f38_0f3a_intrinsics_emit_xmm_dataflow -- --nocapture` (pass)
- `cargo test -p fission-sleigh --lib` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)

### rust-sleigh Backend Orchestration Consolidation + x86 Semantic/Length Expansion

This update consolidates function-level lifting orchestration into the shared backend path, expands x86 semantic ownership for additional instruction families, and validates the change set through sleigh unit gates and automation lanes.

#### Changed

- consolidated function-level decode/lift orchestration under the backend layer while preserving `SleighLifter` public API behavior:
  - added backend-owned contract loop (`lift_ops_with_contract`) and instruction decode entry (`decode_and_lift_with_len`)
  - implementation: `crates/fission-sleigh/src/lifter/backend/mod.rs`, `crates/fission-sleigh/src/lifter/mod.rs`
- unified backend state plumbing for semantic decode through context-aware entrypoints:
  - switched AArch64/x86 module exports to `decode_semantic_with_state`
  - implementation: `crates/fission-sleigh/src/lifter/aarch64/mod.rs`, `crates/fission-sleigh/src/lifter/aarch64/semantic.rs`, `crates/fission-sleigh/src/lifter/x86/mod.rs`, `crates/fission-sleigh/src/lifter/x86/semantic.rs`, `crates/fission-sleigh/src/lifter/common.rs`
- centralized CFG split/target helpers for block construction:
  - `is_cfg_split_opcode`, `direct_control_target`
  - implementation: `crates/fission-sleigh/src/lifter/backend/mod.rs`, `crates/fission-sleigh/src/lifter/mod.rs`
- expanded x86 semantic/length coverage and modular ownership:
  - split `0F` extended semantic handling into dedicated modules (`bitops`, `bitshift`, `cond`, `escape3byte`, `imul`, `movmuldiv`, `simd`, `system`)
  - added semantics for rotate intrinsics, sign-extension convert family (`0x98`/`0x99`), `xchg` reg/mem variants, and `shld`/`shrd`
  - improved x86 length decoding with explicit opcode map handling, including VEX map variants and truncated-VEX guards
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`, `crates/fission-sleigh/src/lifter/x86/semantic.rs`, `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Added

- new backend module and lift contract result type:
  - `crates/fission-sleigh/src/lifter/backend/mod.rs`
- new x86 extended semantic submodules:
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/bitops.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/bitshift.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/cond.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/escape3byte.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/imul.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/movmuldiv.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/simd.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext/system.rs`
- contract and semantic regression coverage:
  - backend sequencing/consumed-bytes contract tests
  - decode failure address mapping test
  - x86 semantic regressions for rotate/xchg/shld-shrd/scalar-simd families
  - implementations: `crates/fission-sleigh/src/lifter/mod.rs`, `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`, `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Validation

- `cargo test -p fission-sleigh --lib lifter::tests::backend_lift_contract_keeps_trace_order_and_consumed_bytes` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::backend_lift_contract_reports_decode_failure_address` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::lift_contract_reports_instruction_limit_stop` (pass)
- `cargo test -p fission-sleigh --lib lifter::tests::lift_contract_reports_terminal_control_flow_stop` (pass)
- `cargo test -p fission-sleigh --lib lifter::x86::semantic::tests` (pass)
- `cargo test -p fission-sleigh --lib` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --no-build` (pass, `changed_rows=0`)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile full` (pass, `changed_rows=0`)

### NIR Branch-Target Recovery Hardening + Limit-200 Baseline/Post/Delta Automation

This update focuses on making indirect/partially unresolved control-flow lowering more robust in Rust NIR and packaging the repeated limit-200 measurement workflow into a single reproducible command.

#### Changed

- strengthened NIR terminator recovery in `fission-pcode` for `Branch`, `CBranch`, and `BranchInd`:
  - route target resolution through a recovery path that combines passthrough peel + one-step arithmetic address inference (`IntAdd` / `IntSub` with const)
  - add Branch/CBranch fallback target inference from CFG successors when direct target resolution fails
  - add BranchInd target inference from simple `Load`-address forms
  - infer `switch` default target from fallthrough when available
  - implementation: `crates/fission-pcode/src/nir/builder/terminator.rs`
- changed unsupported terminator handling to emit explicit marker calls instead of aborting render for single-block/multi-block/linear paths:
  - emits `__fission_indirect_cf_unsupported()` call expression
  - implementations: `crates/fission-pcode/src/nir/builder/mod.rs`, `crates/fission-pcode/src/nir/structuring/driver.rs`, `crates/fission-pcode/src/nir/structuring/linear.rs`
- extended unsupported inventory recording on branch-target resolve failures for diagnostics:
  - implementation: `crates/fission-pcode/src/nir/builder/debug.rs`
- broadened type-hint application in synthetic/non-stack-origin paths and tightened local hint eligibility fallback logic:
  - implementation: `crates/fission-pcode/src/nir/builder/type_hints.rs`
- hardened arithmetic normalization edge case by replacing subtraction with saturating subtraction in magic-division recognition:
  - implementation: `crates/fission-pcode/src/nir/normalize/arith.rs`

#### Added

- new x86 bootstrap regressions for branch-target recovery and unsupported lowering behavior:
  - Branch/CBranch wrapped-target recovery (copy + one-step arithmetic)
  - BranchInd no-target tolerance and load-address target recovery
  - unresolved branch fallback behavior via successor inference
  - implementation: `crates/fission-pcode/src/nir/tests/bootstrap_x86.rs`
- one-command local automation script for baseline/post/summary/delta generation on putty/everything (`--decomp-limit 200`):
  - snapshots unsupported inventory files per run
  - generates summary/delta json+md artifacts and putty unmapped cluster reports
  - includes baseline auto-resolution fallback (`rebuilt`, `after_term`, `after_passthrough`)
  - implementation: `scripts/test/run_limit200_baseline_post_delta.py`

#### Validation

- `cargo test -p fission-pcode --lib bootstrap_x86::preview_` (pass)
- `cargo test -p fission-pcode --lib` (pass)

## 2026-04-05

### rust-sleigh x86 0F3A Semantic Expansion and Branch-Target CFG Diagnostics

Extended the rust-sleigh x86 three-byte semantic ownership for SSE4 string/extract opcodes and added CFG-construction diagnostics to narrow unresolved branch-target fallback causes before NIR lowering.

#### Changed

- expanded x86 `0F 3A` dataflow semantic handlers in `fission-sleigh`:
  - `0x61` `PCMPESTRI` (`ECX` write path)
  - `0x62` `PCMPISTRM` (`XMM0` write path)
  - `0x17` `EXTRACTPS` (`r/m` write path)
  - implementation: `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
- added regression coverage for the new handlers (reg/mem forms) in:
  - `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`
- added branch-target resolution diagnostics in NIR terminator lowering to log:
  - input varnode, seq, block index/address, guessed target, and successor list on resolve failure
  - implementation: `crates/fission-pcode/src/nir/builder/debug.rs`, `crates/fission-pcode/src/nir/builder/terminator.rs`
- added CFG-construction diagnostics in rust-sleigh block building to log:
  - `branch_target_unmapped`
  - `control_block_no_successors`
  - per-block successor finalization summaries
  - implementation: `crates/fission-sleigh/src/lifter/mod.rs`

#### Validation

- `cargo test -p fission-sleigh` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-sleigh` (pass)

#### Measurement Notes

- putty rust-sleigh `--decomp-limit 200` baseline/post measurement completed.
- fallback addresses were traced in isolated debug lanes; common root signature was observed at CFG build time:
  - constant `Branch` target not present in current function op-address map
  - unresolved target produced empty successor set prior to NIR terminator lowering

### x86 Lifter - Semantic Module Split and Byte-Group Arithmetic Expansion

This increment continues the x86-first lifting track by reducing semantic-module complexity and expanding arithmetic coverage for byte-width group operations.

#### Changed

- split x86 semantic extended-opcode logic into a dedicated module to improve ownership boundaries and maintainability:
  - `crates/fission-sleigh/src/lifter/x86/semantic/ext.rs`
  - `crates/fission-sleigh/src/lifter/x86/semantic.rs` (dispatcher wiring)
- completed x86 `F6` group semantic coverage (`/0,/3,/4,/5,/6,/7`) by reusing existing `F7` one-operand arithmetic flows with `size=1`:
  - `TEST`, `NEG`, `MUL`, `IMUL`, `DIV`, `IDIV`
  - implemented in `crates/fission-sleigh/src/lifter/x86/semantic.rs`
- aligned x86 length decode for `F6` immediate handling so only `/0` consumes `imm8`:
  - `crates/fission-sleigh/src/lifter/x86/length.rs`
- added byte-group regression tests for semantic/length consistency:
  - `crates/fission-sleigh/src/lifter/x86/semantic/tests.rs`
  - `crates/fission-sleigh/src/lifter/x86/length.rs`

#### Validation

- `cargo test -p fission-sleigh --lib lifter::x86::semantic::tests::decode_f6` (pass)
- `cargo test -p fission-sleigh --lib lifter::x86::length::tests::decode_len_handles_f6_test_immediate_only_for_group0` (pass)
- `cargo test -p fission-sleigh` (pass, 92 tests)

## 2026-04-03

### AARCH64 AppleSilicon Parse Fix - InvalidRef Resolution in sleigh-rs

Resolved the arm64 parse blocker in `AARCH64_AppleSilicon` by aligning `sleigh-rs` execution-time symbol resolution with Ghidra 11.4.2 behavior for produced subtable operands used in constructor execution expressions.

#### Root Cause

- parse failed in `AARCH64neon.sinc` with `Execution(InvalidRef)` when evaluating produced subtable operand references such as `Re_VPR128.H.vIndexHL`
- `sleigh-rs` previously rejected table reads in execution scope unless the table had an explicit export value

#### Changed

- updated execution read-scope table resolution to allow produced subtable references in constructor execution:
  - `vendor/sleigh-rs/src/semantic/inner/table/execution.rs`
- made table-size access non-panicking for no-export tables by treating them as unsized where appropriate:
  - `vendor/sleigh-rs/src/semantic/inner/execution/expr.rs`
  - `vendor/sleigh-rs/src/semantic/inner/table/mod.rs`
- removed a hard `unwrap()` panic path in user-call parameter size speculation when unresolved/no-export table values are present:
  - `vendor/sleigh-rs/src/semantic/inner/execution/user_call.rs`
- added/updated arm64 parsing regression validation in fission-sleigh tests:
  - `crates/fission-sleigh/src/lifter/mod.rs`

#### Validation

- `cargo test -p fission-sleigh` (pass, including `aarch64_apple_silicon_spec_parses`)
- `cargo check -p fission-cli --features native_decomp` (pass)

## 2026-04-02

### fission-sleigh - Folder-Tree Refactor and Converter Responsibility Split

Refactored `fission-sleigh` into a folder-tree module layout for easier long-term ownership and maintenance, then split converter internals by semantic responsibility (`assignment`, `branch`, `memory`, `unary`) while preserving existing behavior.

#### Changed

- converted flat modules into directory modules:
  - `crates/fission-sleigh/src/converter/mod.rs`
  - `crates/fission-sleigh/src/lifter/mod.rs`
  - `crates/fission-sleigh/src/builder/mod.rs`
- replaced monolithic converter flow with semantic modules:
  - `crates/fission-sleigh/src/converter/assignment.rs`
  - `crates/fission-sleigh/src/converter/branch.rs`
  - `crates/fission-sleigh/src/converter/memory.rs`
  - `crates/fission-sleigh/src/converter/unary.rs`
  - kept expression traversal and shared utilities in `expr.rs` and `helpers.rs`
- retained converter unit tests and validation expectations in:
  - `crates/fission-sleigh/src/converter/tests.rs`

#### Validation

- `cargo check -p fission-sleigh` (pass)
- `cargo test -p fission-sleigh` (pass, 7 tests)

### Graph-Theoretic Loop Structuring (Ghidra LoopBody Integration)

루프 구조화(Loop Structuring) 단계에서 기존의 휴리스틱(`fallthrough_index` 예측)을 제거하고, Ghidra의 `LoopBody` 출구 식별 알고리즘 및 엄밀한 CFG 간선 분류(Edge Classification)를 도입하여 결정론적인 while/do-while 구조화를 달성했습니다.

#### Changed
- **CFG Analysis 강화**: `crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`에 깊이 우선 탐색(DFS) 기반의 전위 순회(Preorder) 기록 및 간선 클래스 분류 로직 적용.
- **`LoopBody` 설계**: `crates/fission-pcode/src/nir/structuring/loop_analysis.rs` 모듈을 신설하여 Ghidra의 `findBase`, `findExit`, `extend` 기능을 구현해 루프 바디로 불법적 출구가 병합되는 문제를 예방.
- **휴리스틱 제거 및 개편**: `crates/fission-pcode/src/nir/structuring/loops.rs`의 `try_lower_while`에서 기존 `fallthrough_index`를 맹목적으로 참조하던 방식을 제거하고, 미리 식별된 정확한 `exit_idx`를 사용하도록 로직 재작성.
- **상태 연동**: `PreviewBuilder` 객체 내에 `loop_bodies` 상태를 추가하고 `get_loop_body` 접근자를 통해 구조화 모듈 전역에서 루프 구조를 활용하도록 연결.

#### Validation
- `cargo check -p fission-pcode` (pass)
- `cargo test -p fission-pcode` (loop tests passed perfectly without fallback)

---

## 2026-04-01

### Algorithmic Loop Structuring and Unbounded Region Recovery

Replaced lexical, position-based heuristics with algorithmic validations for `For` loop synthesis, and lifted artificial search bounds during irreducible CFG region recovery.

#### Changed

- Added `try_collapse_while_to_for_algorithmic` in `crates/fission-pcode/src/nir/normalize/for_loops.rs` to enforce backward dataflow independence for `init` block assignments and perform deep AST scans for `continue` statements, preventing unsafe loop `update` hoisting.
- Hooked `for_loops.rs` module into `core.rs` normalization passes.
- Removed the hardcoded `start_idx + 24` lookahead limit in `crates/fission-pcode/src/nir/structuring/recovery.rs` (`region_linearized_exit_candidates`), allowing full CFG scanning for region exits.

#### Validation

- `cargo check -p fission-pcode` (pass)
- `cargo test -p fission-pcode` (pass)

### Short-Circuit Folding - Prefix-Aware Condition Canonicalization Telemetry

This increment broadens short-circuit folding to tolerate trivial prefix statements in the first block of a chain, records whether AND/OR folds actually happen, and tracks when side effects correctly block the fold.

#### Changed

- added new `NirBuildStats` counters:
  - `condition_fold_and_count`
  - `condition_fold_or_count`
  - `condition_fold_rejected_side_effect`
- wired the new counters through preview builder state/init/stats projection and automation report export
- added `simplify_logical_expr()` in `crates/fission-pcode/src/nir/cfg.rs` to canonicalize nested De Morgan-style logical expressions after fold construction
- updated `crates/fission-pcode/src/nir/structuring/conditionals/short_circuit.rs` so short-circuit folding:
  - accepts trivial prefix statements in the first block of a chain
  - rejects side-effectful prefixes in either the first or subsequent blocks
  - wraps preserved prefixes around the folded `if` instead of discarding them

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)

#### Observed sample telemetry

- Current 200-function sample: `condition_fold_and_count=0`, `condition_fold_or_count=0`, `condition_fold_rejected_side_effect=0`
- Current 500-function sample: `condition_fold_and_count=0`, `condition_fold_or_count=0`, `condition_fold_rejected_side_effect=0`

The new counters are wired and available, but the current fixed samples do not yet hit these newly accepted/rejected short-circuit shapes.

## 2026-03-25

### Switch Structuring - Ghidra `checkSwitchSkips` Safety Guard Regression

This patch hardens switch lowering safety by adding a negative regression that locks behavior when default and non-default paths do not share a stable exit.

#### Changed

- retained bounded switch target canonicalization for trivial forwarding chains in `structuring/switch.rs`
- aligned validation target with Ghidra `checkSwitchSkips` intent: avoid unsafe switch formation when default exit diverges

#### Added

- new regression test:
  - `multi_block_preview_does_not_lower_switch_when_default_exit_differs_from_case_exit`
- test asserts fallback to conditional chain (no unsafe `switch` emission) under non-shared default/case exits

#### Validation

- `cargo test -p fission-pcode structuring_switch` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

### Docs - Add Fission AI Agent Operating Guide

Added a repository-root `AGENTS.md` that codifies architecture ownership, crate boundaries, NIR structuring rules, telemetry contract, and current CI/testing expectations for AI-assisted engineering workflows.

#### Added

- `AGENTS.md`

### Loop Structuring - Explicit Infloop Break Reducer + Loop-Control Telemetry

This patch adds a conservative explicit infloop-with-break reducer path and wires loop-control rewrite telemetry through `NirBuildStats` and automation deltas so quality runs can track loop-local rewrite behavior directly.

#### Changed

- added `try_lower_infloop_with_break()` in `structuring/loops.rs` for conditional self-loop shapes that can be safely expressed as `while (true)` + guarded `break`
- integrated a new structuring attempt stage (`attempt=loop_control`) in `structuring/driver.rs`, ordered after `while` and before plain `infloop`
- extended loop-control rewrite pass with explicit counters:
  - rewritten `break` gotos
  - rewritten `continue` gotos
  - nested-scope rewrite skips (`While`/`DoWhile`/`Switch`)
- added new `NirBuildStats` fields and propagated them through:
  - preview builder state/snapshot
  - stats merge path
  - automation summary delta and markdown baseline delta rendering

#### Validation

- `cargo test -p fission-pcode rewrite_loop_control_gotos -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_loops -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo build -p fission-pcode -p fission-automation` (pass)

### P5H3E - Conditional-Tail Shared-Follow Canonical Arm Alignment

This increment tightens conditional-tail recovery by aligning shared-follow candidate search and per-arm lowering to canonicalized region-local arm starts, reducing mismatch opportunities caused by pre-canonical arm divergence.

#### Changed

- in `structuring/linear.rs` `lower_conditional_tail()`:
  - shared-tail entry discovery now uses `true_arm.canonical_idx` / `false_arm.canonical_idx`
  - shared-tail arm lowering to intermediate follow entries now starts from canonicalized indices instead of raw effective starts
- preserved existing one-arm fast-path handling (`reaches_join_trivially`) to keep conservative empty-else lowering behavior unchanged

#### Validation

- `cargo test -p fission-pcode structuring_conditionals -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_linear -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

### Facade Ownership Cleanup - Remove Legacy Duplicate Trees from `fission-analysis`

This change removes stale duplicated implementation trees from `fission-analysis` so the crate remains a compatibility facade and ownership stays with `fission-static` and `fission-dynamic`.

#### Changed

- removed duplicated legacy module trees from `crates/fission-analysis/src/`:
  - `analysis/`, `debug/`, `plugin/`, `app/`, `unpacker/`, `utils/`
- updated compatibility prelude debug type re-export to owner crate path:
  - `crate::debug::types::*` → `fission_dynamic::debug::types::*`
- added compatibility policy document:
  - `crates/fission-analysis/COMPATIBILITY.md`

#### Validation

- `cargo check -p fission-analysis --features native_decomp` (pass)
- `cargo check -p fission-analysis --features "interactive_runtime unpacker_runtime native_decomp"` (pass)
- `cargo test -p fission-analysis --features native_decomp --no-run` (pass)

### Structuring - Graph-Invariant Promotion Gate + Guarded-Tail Layout Normalization

This increment moves promotion acceptance beyond strict layout order checks by adding conservative graph-invariant fallback guards (dominance/post-dominance/irreducibility) and pre-discovery guarded-tail layout normalization.

#### Changed

- promotion gate update in `structuring/guards.rs`:
  - kept legacy monotonic predecessor ordering acceptance path
  - added additive graph-invariant fallback acceptance when legacy path fails:
    - reject irreducible SCC participation
    - require header dominance for targeted internal entries
    - require region-window postdom exit guard when an external exit exists
- added guarded-tail pre-normalization pipeline:
  - `normalize_guarded_tail_layout()` in `structuring/cleanup.rs`
  - applies adjacent-label cleanup + top-level forward alias canonicalization before guarded-tail discovery/promotion scanning
- discovery/promotion entry points now consume normalized layout views to reduce avoidable noncanonical shape rejections

#### Added

- new unit tests:
  - `minimal_structured_promotion_accepts_non_monotonic_layout_when_graph_invariants_hold`
  - `minimal_structured_promotion_rejects_irreducible_region`
  - `normalize_guarded_tail_layout_collapses_adjacent_labels_before_alias_rewrite`
  - plus updated guarded-tail discovery regressions for normalized layout/counter semantics

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane delta (`nir`, functions-limit 40)

- `promotion_rejected_by_shape_count`: `633 -> 606`
- `discovery_rejected_noncanonical_layout_count`: `561 -> 533`
- `canonicalization_failed_interleaved_join_uses`: `170 -> 149`
- output class mix unchanged on this sample (`structured=32`, `partially_structured=34`, `linear_fallback=8`)

### Structuring - Guarded-Tail Join and Tail-Exit Canonicalization Tightening

This increment further aligns guarded-tail recovery with Ghidra-style conservative exit handling by terminalizing safe forward join chains, filtering non-forward targets out of candidate discovery, and preserving tail-terminal returns without relaxing loop/switch escape safety.

#### Changed

- refined guarded-tail join target handling in `structuring/guards.rs`:
  - added safe multi-hop terminal join resolution for trivial forward label chains
  - prefiltered backward/non-forward top-level label targets so they are skipped as non-candidates instead of inflating nonterminal join failures
  - preserved conservative rejection for ambiguous/nonlocal alias ownership
- refined guarded-tail segment canonicalization:
  - accepted a single tail-terminal `return` after payload as a valid terminal exit
  - continued rejecting true nested tail escapes (`goto`/`break`/`continue` after payload) and ambiguous scoped exits
- expanded guarded-tail regression coverage in `structuring_misc.rs` for:
  - nonterminal join forwarding
  - multi-hop join forwarding
  - safe interleaved alias stubs
  - backward-target skip behavior
  - tail-terminal return preservation

#### Validation

- `cargo test -p fission-pcode structuring_candidate_discovery_ -- --nocapture` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane delta (`nir`, functions-limit 40)

- `canonicalization_failed_nonterminal_join_label`: `201 -> 0`
- `promotion_rejected_by_shape_count`: `332 -> 261`
- `discovery_rejected_noncanonical_layout_count`: `332 -> 259`
- `structured`: `32 -> 35`
- `partially_structured`: `34 -> 31`
- `linear_fallback`: `8 -> 8`

### Structuring - Promotion Gate Subtype Telemetry and Owner-Preserving Conflict Refinement

This increment makes guarded-tail promotion gate failures easier to reason about by splitting must-emit-label pressure into concrete subtypes and refining owner-conflict classification to preserve front-leaf-equivalent forward ownership cases inspired by Ghidra’s label bump-up/front-leaf rules.

#### Changed

- extended guarded-tail promotion gate telemetry with explicit `rejected_must_emit_label` subtypes:
  - `rejected_must_emit_label_surviving_middle_ref`
  - `rejected_must_emit_label_surviving_external_ref`
  - `rejected_must_emit_label_owner_conflict`
- wired the new counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined `structuring/guards.rs` must-emit-label classification so:
  - surviving refs inside canonicalized middle remain `surviving_middle_ref`
  - single surviving outside refs remain `surviving_external_ref`
  - multiple outside refs are only treated as `owner_conflict` when they do **not** all preserve the same simple forward top-level owner path
- added guarded-tail regressions covering:
  - subtype telemetry for surviving middle refs
  - subtype telemetry for owner conflicts
  - safe same-owner forward refs that should no longer be escalated to owner conflict

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 40` (pass)

#### Observed lane telemetry (`nir`, functions-limit 40)

- `promotion_rejected_by_gate_count`: `82`
- `rejected_must_emit_label`: `77`
  - `surviving_middle_ref`: `16`
  - `surviving_external_ref`: `9`
  - `owner_conflict`: `18`
- aggregate gate count did not move on this fixed sample, but subtype visibility now makes the next reduction targets explicit

### Structuring - Whole-Body Alias Ownership and Fallthrough Ref Relaxation

This increment refines guarded-tail alias ownership using Ghidra-style front-leaf / copy-block semantics and `gotoPrints`-style fallthrough elision, so safe same-body forwarded-label reuse is no longer treated as truly nonlocal and some middle/external refs stop forcing labels.

#### Changed

- refined guarded-tail alias canonicalization in `structuring/guards.rs` to inspect **whole-body ref sites** when classifying alias ownership
- preserved `AliasHasNonlocalRef` only for truly unsafe cases:
  - nested external refs
  - post-segment refs
  - unsafe owner crossings
   Extended the rust-sleigh x86 three-byte semantic ownership for SSE4 string/extract opcodes and added CFG-construction diagnostics to narrow unresolved branch-target fallback causes before NIR lowering.
- connected safe external alias redirects back into promotion so outer-body gotos are rewritten consistently before region drain
- relaxed label-pressure classification for two conservative fallthrough-equivalent cases:
  - trailing top-level middle `goto target_label`
   - expanded x86 `0F 3A` dataflow semantic handlers in `fission-sleigh`:
- kept nested/internal middle refs and post-label external refs conservative

#### Added

- new regressions in `structuring_misc.rs` covering:
  - safe external alias reuse rewrite
  - trailing middle goto relaxation
   - `cargo test -p fission-sleigh` (pass)
  - preserved post-label external-ref rejection
  - preserved true nonlocal alias rejection

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
   - strengthened NIR terminator recovery in `fission-pcode` for `Branch`, `CBranch`, and `BranchInd`:
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
   - changed unsupported terminator handling to emit explicit marker calls instead of aborting render for single-block/multi-block/linear paths:
  - `canonicalization_failed_alias_not_fallthrough_count`: `175 -> 247`
- 500 functions:
   - extended unsupported inventory recording on branch-target resolve failures for diagnostics:
  - `canonicalization_failed_alias_not_fallthrough_count`: `260 -> 352`
   - broadened type-hint application in synthetic/non-stack-origin paths and tightened local hint eligibility fallback logic:
The large-sample runs show the alias-nonlocal bucket dropping substantially, with part of that volume reclassified into the more precise `alias_not_fallthrough` subtype instead of remaining lumped into `nonlocal`.
   - hardened arithmetic normalization edge case by replacing subtraction with saturating subtraction in magic-division recognition:
### Structuring - AliasNotFallthrough Subtypes and Discovery Acceptance Refinement

This increment splits `AliasNotFallthrough` into concrete after-label categories, adds a conservative top-level after-label relaxation using Ghidra `gotoPrints` / `nextFlowAfter`-style equivalence, and accepts terminal guarded tails plus pure-expression alias bodies when they are structurally safe.

   - new x86 bootstrap regressions for branch-target recovery and unsupported lowering behavior:

- extended `AliasNotFallthrough` telemetry with explicit subtypes:
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`
  - `canonicalization_failed_alias_not_fallthrough_nested_after_label_count`
   - one-command local automation script for baseline/post/summary/delta generation on putty/everything (`--decomp-limit 200`):
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation report stat export
- refined guarded-tail alias canonicalization in `structuring/guards.rs`:
  - allows a narrow top-level after-label self-goto case when the forwarded alias still follows the same printed front path
  - keeps nested after-label and other printed-order-divergent refs conservative
- refined guarded-tail promotion shape handling:
   - `cargo test -p fission-pcode --lib bootstrap_x86::preview_` (pass)
   - `cargo test -p fission-pcode --lib` (pass)
  - accepts alias bodies composed only of pure value expressions instead of treating them as automatically nontrivial
  - continues rejecting alias bodies with control flow or side-effectful expression shapes

#### Added

- new regressions in `structuring_misc.rs` covering:
  - top-level after-label subtype counting
  - nested after-label subtype counting
  - safe top-level after-label alias acceptance
  - terminal guarded-tail promotion
  - pure-expression alias-body acceptance

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
  - `canonicalization_failed_alias_not_fallthrough_count`: `247 -> 180`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `361 -> 262`
  - `promoted_region_count`: `237 -> 239`
  - `structured`: `84 -> 86`
- 500 functions:
  - `canonicalization_failed_alias_not_fallthrough_count`: `352 -> 267`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `471 -> 354`
  - `promoted_region_count`: `559 -> 561`
  - `structured`: `186 -> 188`

These changes materially reduce the large-sample after-label alias bucket while slightly increasing successful guarded-tail promotions and structured output.

### Structuring - Direct Shape Subtype Telemetry and Pure-Expression Discovery Relaxation

This increment separates the remaining direct guarded-tail shape blockers from canonicalization-driven discovery failures and relaxes one discovery-only case where alias bodies contain only pure value expressions.

#### Changed

- added explicit direct shape subtype telemetry:
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined guarded-tail discovery canonicalization in `structuring/guards.rs`:
  - accepts alias bodies made only of pure value expressions
  - still rejects alias bodies with control flow or side-effectful expressions (`Call`, `Load`)
- added a stable regression asserting terminal guarded-tail promotion leaves the new direct shape subtype counters at zero

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `promotion_rejected_by_shape_count`: `908`
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`: `0`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`: `0`
  - `discovery_rejected_noncanonical_layout_count`: `908`
- 500 functions:
  - `promotion_rejected_by_shape_count`: `1643`
  - `promotion_rejected_by_shape_missing_terminal_join_target_count`: `0`
  - `promotion_rejected_by_shape_empty_nonterminal_tail_count`: `0`
  - `discovery_rejected_noncanonical_layout_count`: `1643`

These measurements show the remaining large shape bucket is overwhelmingly coming from canonicalization-driven discovery failures rather than the two direct shape blockers, which narrows the next optimization target considerably.

### Structuring - Alias Nonlocal Ref Subtype Telemetry

This increment splits a major remaining alias bucket into concrete subtype counters so large-sample runs can distinguish whether label ownership escapes are coming from nested pre-entry refs, post-segment refs, or simpler external-before patterns.

#### Changed

- added explicit alias-nonlocal subtype telemetry:
  - `canonicalization_failed_alias_has_nonlocal_ref_external_before_count`
  - `canonicalization_failed_alias_has_nonlocal_ref_nested_before_count`
  - `canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - automation build-stat reporting
- refined guarded-tail alias classification in `structuring/guards.rs` so generic `AliasHasNonlocalRef` failures are attributed to the concrete external-site cause instead of only incrementing the aggregate counter

#### Added

- new regressions in `structuring_misc.rs` covering:
  - nested-before nonlocal alias refs
  - external-before nonlocal alias refs
  - post-segment nonlocal alias refs

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `canonicalization_failed_alias_has_nonlocal_ref_count`: `298`
  - `external_before`: `0`
  - `nested_before`: `42`
  - `post_segment_ref`: `102`
- 500 functions:
  - `canonicalization_failed_alias_has_nonlocal_ref_count`: `583`
  - `external_before`: `0`
  - `nested_before`: `135`
  - `post_segment_ref`: `187`

The new breakdown shows `external_before` is not a meaningful bottleneck, while `nested_before` and especially `post_segment_ref` are the next concrete ownership cases to target.

### Structuring - Conservative Terminal Goto Tail Escape Refinement

This increment reduces one concrete nested-tail escape bucket by accepting only the safest terminal goto form: a post-payload goto is allowed when it is the final meaningful statement in the segment, does not target any label inside the current body, and does not introduce additional in-body structure.

#### Changed

- refined guarded-tail canonicalization in `structuring/guards.rs` so post-payload `goto` is accepted only when all of the following hold:
  - no non-ignorable statements follow it
  - no internal labels appear earlier in the canonicalized segment
  - the goto target label does not appear anywhere in the current body
- kept `break` / `continue` conservative after payload
- preserved switch/default-exit safety by continuing to reject in-body structured targets

#### Added

- new regression `structuring_candidate_discovery_allows_tail_terminal_goto_after_payload`
- tightened negative regression `structuring_candidate_discovery_counts_nested_tail_escape` so it still covers a true nested escape with trailing meaningful work
- revalidated switch safety with `multi_block_preview_does_not_lower_switch_when_default_exit_differs_from_case_exit`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample delta (`nir`)

- 200 functions:
  - `canonicalization_failed_nested_tail_escape`: `171 -> 160`
  - `discovery_rejected_noncanonical_layout_count`: `908 -> 897`
  - `promotion_rejected_by_shape_count`: `908 -> 897`
  - `promotion_candidate_count`: `561 -> 564`
  - `promoted_region_count`: `239 -> 242`
- 500 functions:
  - `canonicalization_failed_nested_tail_escape`: `303 -> 292`
  - `discovery_rejected_noncanonical_layout_count`: `1643 -> 1632`
  - `promotion_rejected_by_shape_count`: `1643 -> 1632`
  - `promotion_candidate_count`: `1202 -> 1205`
  - `promoted_region_count`: `561 -> 564`

This is a small but real large-sample reduction that improves guarded-tail acceptance without regressing the switch safety guard.

### Structuring - Interleaved Join Subtypes and Pure-Value Guarded-Tail Relaxations

This increment sharpens guarded-tail diagnosis by splitting `InterleavedJoinUses` into concrete causes and accepts a narrow set of front-path-equivalent pure-value alias layouts that previously failed despite preserving the same control-flow target.

#### Changed

- added explicit `InterleavedJoinUses` subtype telemetry:
  - `canonicalization_failed_interleaved_join_uses_no_next_label_count`
  - `canonicalization_failed_interleaved_join_uses_nontrivial_segment_count`
- wired these counters through:
  - `NirBuildStats`
  - preview builder state/snapshot
  - builder stats projection
  - automation build-stat reporting
- refined guarded-tail alias canonicalization in `guarded_tail/alias_refs.rs` so pure value expressions are treated as ignorable in two conservative forwarding cases:
  - next-label terminalization inside interleaved join stubs
  - top-level after-label forward/self-reference segments
- refined guarded-tail canonicalization in `guarded_tail/canonicalize.rs` so all-before external refs can remain local when they share the same trivial forward owner path

#### Added

- new guarded-tail regressions covering:
  - interleaved join subtype counting
  - pure-value interleaved segment acceptance
  - side-effectful interleaved segment rejection
  - pure-value top-level-after-label acceptance
  - side-effectful top-level-after-label rejection
  - safe nested-before alias reuse

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions:
  - `canonicalization_failed_interleaved_join_uses`: `170`
  - `no_next_label`: `69`
  - `nontrivial_segment`: `101`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `262`
- 500 functions:
  - `canonicalization_failed_interleaved_join_uses`: `376 -> 363`
  - `no_next_label`: `169 -> 162`
  - `nontrivial_segment`: `207 -> 201`
  - `canonicalization_failed_alias_not_fallthrough_top_level_after_label_count`: `354`

The pure-value interleaved refinement produces a real but modest 500-function reduction, while the new subtype counters show the remaining interleaved failures are still dominated by structurally nontrivial segments rather than opaque layout noise.

### Structuring - Pure Multi-Goto Alias-Chain Relaxation

This increment broadens one guarded-tail alias-chain acceptance boundary by allowing multiple top-level forward gotos to the same local alias label when everything between them is pure/ignorable and the alias still forwards linearly to the same follow.

#### Changed

- refined `guarded_tail/canonicalize.rs` so the `has_non_ignorable_gap && goto_positions.len() != 1` path no longer rejects a purely linear alias chain if all intermediate statements are:
  - ignorable discovery statements
  - pure value expressions
  - gotos to the same alias label
- added helper `is_pure_multi_goto_gap_to_label()` in `guarded_tail/alias_refs.rs` to keep the acceptance rule narrow and explicit

#### Added

- new guarded-tail regression:
  - `structuring_candidate_discovery_canonicalizes_pure_multi_goto_alias_chain`
- preserved existing alias-forward-chain and true-nonlocal regressions

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 500` (pass)

#### Observed expanded-sample telemetry (`nir`)

- 200 functions: no material aggregate movement
- 500 functions: no material aggregate movement

This is a safe acceptance broadening for a real local alias shape, but it does not materially move the dominant large-sample buckets on its own.

### Docs and Structure - Hierarchical AGENTS Guides and Guarded-Tail Module Split

This increment improves repository navigation for human/AI contributors and reduces structural risk in the guarded-tail implementation by splitting the overloaded `guards.rs` module and moving its dedicated regression coverage into a separate test file.

#### Changed

- refreshed the repository-root `AGENTS.md` and added focused child guides for the highest-complexity ownership seams:
  - `crates/fission-pcode/src/nir/AGENTS.md`
  - `crates/fission-pcode/src/nir/structuring/AGENTS.md`
  - `crates/fission-automation/src/AGENTS.md`
  - `crates/fission-static/src/analysis/decomp/postprocess/AGENTS.md`
- split guarded-tail implementation from the monolithic `structuring/guards.rs` into:
  - `structuring/guarded_tail/alias_refs.rs`
  - `structuring/guarded_tail/promotion_graph.rs`
  - `structuring/guarded_tail/canonicalize.rs`
  - `structuring/guarded_tail/promotion.rs`
  - `structuring/guarded_tail/mod.rs`
- updated `structuring/mod.rs` to route guarded-tail logic through the new folder-tree layout
- moved guarded-tail-specific regressions out of `structuring_misc.rs` into:
  - `crates/fission-pcode/src/nir/tests/structuring_guarded_tail.rs`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

#### Notes

- This is a behavior-preserving refactor intended to make future guarded-tail work safer and more local.
- `structuring_misc.rs` now keeps only genuinely mixed/overflow structuring cases while guarded-tail behavior is co-located with its own test file.

### Structure - NIR Support, Telemetry, and Builder Façade Slimming

This refactor wave makes the NIR layer materially easier to navigate by moving the last large support/state/telemetry/helper responsibilities out of `nir/mod.rs` and `nir/builder/mod.rs` into dedicated internal modules without changing the public NIR surface.

#### Changed

- split shared private NIR support into:
  - `crates/fission-pcode/src/nir/support.rs`
    - support types
    - constants
    - pure lowering/type/naming helpers
- split preview telemetry storage and retrieval into:
  - `crates/fission-pcode/src/nir/telemetry.rs`
    - thread-local preview stats storage
    - `take_last_preview_*` / `take_last_nir_*` helpers
- slimmed `crates/fission-pcode/src/nir/mod.rs` into a thinner façade that now primarily owns:
  - public render entrypoints
  - top-level orchestration
  - telemetry delegation
- split builder internals into focused modules:
  - `crates/fission-pcode/src/nir/builder/state.rs` — `PreviewBuilder` state layout
  - `crates/fission-pcode/src/nir/builder/init.rs` — constructor/state initialization
  - `crates/fission-pcode/src/nir/builder/debug.rs` — debug / unsupported inventory plumbing
  - `crates/fission-pcode/src/nir/builder/stats.rs` — `preview_build_stats()` projection
- kept `crates/fission-pcode/src/nir/builder/mod.rs` as a much thinner façade around:
  - type-hint wrappers
  - `build_hir()` orchestration
  - a small set of orchestration helpers

#### Validation

- priority regression set passed:
  - `bootstrap_x86`
  - `structuring_conditionals`
  - `structuring_linear`
  - `structuring_loops`
  - `type_hints_function_hints`
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

#### Notes

- This is a behavior-preserving structural refactor only.
- The `nir/` tree now has clearer ownership boundaries: façade (`mod.rs`), support (`support.rs`), telemetry (`telemetry.rs`), builder state/init/debug/stats, structuring, and tests.

## 2026-03-24

### P5H4A/P5H4B/P5H4C/P5H4E - Algorithmic CFG Foundation Expansion (Ghidra-Referenced)

This step advances structuring from local heuristic-style approximations toward graph-theoretic analysis primitives, while preserving conservative fallback behavior.

#### Changed

- stabilized label handling used by region/join anchoring in normalization and cleanup paths
- added CFG edge classification analysis (`Tree`, `Back`, `Forward`, `Cross`) for deterministic, order-robust graph facts
- added formal dominator/post-dominator analysis APIs and integrated window-exit postdom computation into conditional-tail follow logic
- added Tarjan SCC analysis and irreducible multi-header SCC detection (diagnostic-safe integration)
- extended structuring diagnostics to include SCC and irreducible telemetry counters

#### Added

- new structuring analysis module:
  - `crates/fission-pcode/src/nir/structuring/cfg_analysis.rs`
- new CFG-analysis tests covering:
  - diamond edge classification
  - single-loop back-edge classification
  - multi-header SCC irreducible detection
  - nearest common dominator/postdominator behavior on canonical shapes

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-pcode structuring_conditionals` (pass)
- `cargo test -p fission-pcode structuring_loops` (pass)
- `cargo check -p fission-pcode` (pass)

### Automation - Irreducible/SCC Telemetry Surfacing and Gate Safety Integration

Automation reporting now consumes irreducible-structure telemetry from `NirBuildStats`, so quality runs can detect mismatch improvements that are accompanied by structural complexity regressions.

#### Changed

- extended `NirBuildStats` with:
  - `structuring_scc_component_count`
  - `structuring_irreducible_scc_count`
  - `structuring_irreducible_header_count`
- wired new counters through builder initialization, preview stats snapshots, and stats merge paths
- updated automation summary/delta reporting to include SCC/irreducible counters
- updated go/stop decision gate constraints to require non-regressing irreducible deltas in addition to mismatch/migration checks

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-automation` (pass)

### P5H4E - Conservative Irreducible Recovery Gate and NIR Completeness Reporting

This patch adds an optional conservative gate for region linearization recovery on irreducible CFG nodes and extends telemetry/reporting so automation can measure the tradeoff explicitly.

#### Changed

- added `NirRenderOptions.conservative_irreducible_fallback` (default `false`) with backward-compatible serde default handling
- added recovery rejection telemetry for irreducible CFG gating:
  - `region_linearize_rejected_irreducible_cfg_count`
- wired the new counter through:
  - `PreviewBuilder` initialization/state
  - `preview_build_stats()` snapshots
  - `NirBuildStats::merge_assign()`
- recovery path now optionally skips region linearization when conservative gate is enabled and the start node belongs to an irreducible SCC
- `fission-static` recovery option wiring now supports env-based activation:
  - `FISSION_NIR_CONSERVATIVE_IRREDUCIBLE_FALLBACK`
- automation reporting updated to include irreducible-gate rejection metrics in:
  - stats pairs
  - baseline deltas
  - markdown summary output

#### Added

- SCC helper API for gate decisions:
  - `SccAnalysis::is_irreducible_node()`
- regression test:
  - `scc_analysis_reports_irreducible_membership_by_node`
- NIR English completeness report document:
  - `crates/fission-pcode/src/nir/NIR_DECOMPILER_COMPLETENESS_REPORT.md`

#### Validation

- `cargo test -p fission-pcode` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-automation` (pass)
- `cargo check -p fission-static --features native_decomp` (pass)

### Loop Structuring - Conservative Infloop + Loop-Control Goto Rewrites (Ghidra-Referenced)

This patch extends loop structuring with a conservative infinite-loop reducer and safe loop-local `goto` rewriting into `break`/`continue`, aligned with Ghidra `scopeBreak` intent while preserving nested-scope safety.

#### Changed

- added and integrated `try_lower_infloop()` into the main structuring order:
  - reducer order now keeps `infloop` after `dowhile` and `while` attempts for conservative precedence
- added single-successor guard for infloop recognition (`successors[idx].len() == 1`)
- introduced loop-body post-processing in `structuring/loops.rs`:
  - rewrite `goto(loop_exit_label)` to `break`
  - rewrite `goto(loop_continue_label)` to `continue`
  - recurse only through `If`/`Block`
  - intentionally do **not** recurse into nested `While`/`DoWhile`/`Switch` (avoids outer-loop misrewrites)
- extended do-while region result metadata to return condition-block index so `continue` targets are resolved correctly

#### Added

- integration regression test:
  - `infloop_preview_lowers_single_block_self_loop`
- unit tests for rewrite safety:
  - `rewrite_loop_control_gotos_converts_break_and_continue_targets`
  - `rewrite_loop_control_gotos_does_not_rewrite_inside_nested_loop_or_switch`

#### Validation

- `cargo test -p fission-pcode rewrite_loop_control_gotos_` (pass)
- `cargo test -p fission-pcode structuring_loops` (pass)
- `cargo test -p fission-pcode structuring_conditionals` (pass)
- `cargo test -p fission-pcode` (pass)
- `cargo check -p fission-pcode` (pass)

## 2026-03-23

### Docs - CONTRIBUTING CI/CD Workflow Refresh

Contributor guidance was updated to match the current CI/CD architecture and remove stale local expectations.

#### Changed

- `CONTRIBUTING.md` now documents:
  - fast PR gate vs heavy GitHub validation split
  - Windows build/test participation in CI
  - current local pre-PR command set aligned with fast gate
  - direct CMake decompiler build invocation used in CI
  - automation artifact interpretation expectations for decompilation-quality changes

### CI/CD - Major Reinforcement (Fast PR Gate + Heavy GitHub Validation)

To reduce local monitoring burden, CI/CD now separates fast developer feedback from heavy long-running validation that can run entirely on GitHub.

#### Added

- new heavy validation workflow: `.github/workflows/ci-heavy.yml`
  - triggers: `push(main)`, nightly `schedule`, and `workflow_dispatch`
  - jobs:
    - Linux full validation (full Rust tests, tauri frontend build, decomp smoke)
    - Windows heavy build/test (decompiler + core Rust tests)
    - automation nir-check lanes with artifact upload
- automation artifact upload in heavy workflow:
  - uploads `artifacts/fission-automation/` for post-run diagnosis without local reruns

#### Changed

- fast CI workflow (`.github/workflows/ci.yml`) refactored into layered jobs:
  - Linux fast gate
  - macOS build/test
  - Windows build/test
- added Rust build caching (`Swatinem/rust-cache@v2`) to CI jobs
- PR/main fast gate now keeps heavy checks off local loop while preserving cross-platform confidence
- replaced missing decompiler build script invocation with direct CMake build commands in CI workflows:
  - `cmake -S ghidra_decompiler -B ghidra_decompiler/build -DCMAKE_BUILD_TYPE=Release`
  - `cmake --build ghidra_decompiler/build --config Release`
- fixed follow-up CI failures after rollout:
  - removed invalid boolean value usage for `nir-check --update-latest` (flag now omitted in heavy workflow)
  - constrained Windows CMake builds to required targets (`decomp`, `fission_decomp`) to avoid unrelated test-target dependency failures
  - adjusted Linux heavy Rust test sequence to run `fission-static` under `native_decomp` explicitly while keeping broad workspace coverage
  - updated CD Unix decompiler step to direct CMake build (removed stale `scripts/build/build_decompiler.sh` dependency)

#### Validation

- workflow YAML parse check (local):
  - `ruby -ryaml -e "YAML.load_file('.github/workflows/ci.yml')"`
  - `ruby -ryaml -e "YAML.load_file('.github/workflows/ci-heavy.yml')"`
- existing project checks unaffected by workflow changes (code path unchanged)

### P5H3J - Index-Order Independent Follow Discovery (Anti-Overfit)

This patch removes block-index monotonicity assumptions from localized follow discovery so conditional-tail recovery relies on graph properties (cycle/region guards) rather than binary layout order.

#### Changed

- replaced index-order rejection in local recovery window traversal with explicit window-cycle detection
- updated trivial forwarding chain canonicalization to use visited-set loop safety instead of index-increasing assumptions
- updated region target canonicalization to use visited-set termination instead of index monotonicity checks
- preserved existing conservative guards (`side_entry_or_exit`, bounded window, bounded steps)

#### Added

- regression test: `region_follow_discovery_accepts_non_monotonic_acyclic_window`
- regression test: `region_follow_discovery_rejects_local_cycle_without_index_heuristic`

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_accepts_non_monotonic_acyclic_window -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_rejects_local_cycle_without_index_heuristic -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused fast benchmark output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774250794-485014000`
- mid 40-function benchmark output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774250794-476962000`

#### Outcome

- follow discovery is now less sensitive to binary-specific block index ordering
- headline corpus movement remains unchanged in current lane (`changed_rows=0`, gate `stop_hold_p5h3f`), but algorithmic generality and anti-overfit guarantees improved

### P5H3I - Algorithmic Arm-Body Failure Decomposition and Signal Cleanup

This patch focused on removing opaque/generic arm-body failure reporting from conditional-tail mismatch analysis and keeping recovery retry behavior deterministic.

#### Changed

- conditional-tail mismatch subtyping now distinguishes algorithmic causes without relying on a generic arm-body bucket:
  - `DepthOrBudgetExceeded`
  - `OneArmBodyLoweringFailed`
  - `BothArmsBodyLoweringFailed`
  - `FollowTailLoweringFailed`
- shared-follow retry failure handling now preserves candidate-stage subtype when propagating final mismatch
- `arm_body_lowering_failed` aggregate counter remains for compatibility but is now sourced from explicit subtypes only
- automation subtype ranking now reports specific subtype channels directly (rather than the aggregate arm-body total)

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused fast benchmark:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774249297-033281000`
- mid benchmark:
  - `cargo run -p fission-automation -- nir-check --lane preview --run-profile mid --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774249297-026445000`

#### Outcome

- top-row movement is still not observed (`changed_rows=0`, gate remains `stop_hold_p5h3f`)
- failure attribution quality improved by removing generic arm-body dominance from subtype ranking, allowing the next step to target specific residual channels (`complex_arm_shape`, `side_entry_or_exit`, `follow_beyond_window`)

### P5H3H - Algorithmic Arm-Body Failure Refinement and Deterministic Follow Retry

This patch continues the heuristic-to-algorithm transition by refining conditional-tail arm-body failure handling and making shared-follow retries deterministic over validated local postdom candidates.

#### Changed

- expanded recovery mismatch subtype model for arm-body failures:
  - `OneArmBodyLoweringFailed`
  - `BothArmsBodyLoweringFailed`
  - `FollowTailLoweringFailed`
- kept aggregate compatibility counter while adding subtype-specific counters for triage precision
- upgraded shared-follow retry loop:
  - retries now iterate over deterministic local postdom candidates (closest-to-join first)
  - candidate attempts classify failure mode explicitly instead of collapsing into one bucket
  - final fallback preserves candidate-stage subtype signal when available

#### Added

- algorithm-focused regression coverage:
  - `region_follow_discovery_orders_multiple_candidates_closest_to_join_first`
- test helper rename for multi-candidate follow verification:
  - `find_shared_tail_entries_for_region_for_test`

#### Validation

- `cargo test -p fission-pcode region_follow_discovery_selects_immediate_common_postdom -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_rejects_side_entry_common_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode region_follow_discovery_orders_multiple_candidates_closest_to_join_first -- --nocapture` (pass)
- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo test -p fission-automation` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- focused benchmark:
  - `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774248662-508776000`
- mid benchmark:
  - `cargo run -p fission-automation -- nir-check --lane preview --run-profile mid --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774248700-402991000`

#### Outcome

- deterministic candidate-order retry behavior is now fixed and test-covered
- subtype granularity for arm-body failures is now available in telemetry and automation insights
- corpus headline metrics on the current 40-function lane remain unchanged (`changed_rows=0`, gate still `stop_hold_p5h3f`), but failure attribution quality improved for the next targeted algorithm step

### Automation - Fast/Mid/Full Run Profiles and Focused Mismatch Reruns

To reduce iteration latency for structuring work, nir-check now supports profile-based execution and baseline-driven target focusing.

#### Added

- `--run-profile {fast|mid|full}` for runtime-tuned execution:
  - `fast`: aggressive limit/timeout reduction for tight loops
  - `mid`: current default behavior
  - `full`: expanded limits for broader validation
- `--focus-top-mismatch N` to filter lane targets using baseline mismatch-heavy binaries
  - reads baseline candidates and keeps only binaries implicated by top mismatch rows
- run metadata in `summary.json`:
  - `run_profile`, `target_count`, `inventory_elapsed_ms`, `diagnosis_elapsed_ms`, `write_outputs_elapsed_ms`, `total_elapsed_ms`
- markdown summary now includes run profile/target count/timing line for quick bottleneck checks

#### Changed

- profile-aware tuning of effective per-target `functions-limit` and `timeout-ms` in automation runner
- terminal summary now prints profile + timing stage breakdown + go/stop gate in one line

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --run-profile fast --focus-top-mismatch 5 --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774247039-176890000/summary.json`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774247430-463672000`
  - run metadata emitted: `run_profile=fast`, `target_count=2`, timings populated

### Automation - Nir-Check Decision Reporting Upgrade (P5H3F Support)

The automation pipeline now emits direct decision artifacts for conditional-tail recovery work, so patch iteration can be judged from row-level evidence instead of aggregate-only counters.

#### Added

- `decision_insights.json` output in each nir-check run, including:
  - mismatch subtype ranking
  - top mismatch rows with per-row subtype split
  - row-level baseline/current mismatch deltas
  - deterministic go/stop gate recommendation for P5H3G readiness
- markdown summary section `Conditional-Tail Decision Insights` with the same signal set

#### Changed

- baseline delta now includes recovery-shaping metrics:
  - `region_linearized_count`
  - `forced_linear_count`
  - `conditional_tail_exit_mismatch_count`
  - `body_lowering_failed_count`
  - `successor_inline_rejected_count`
  - `revisit_cycle_count`
  - `unsupported_terminator_count`
- nir-check now loads baseline candidate rows (when available) to compute row-address diff instead of aggregate-only comparison
- terminal summary now prints go/stop gate and changed-row count for immediate run triage

#### Validation

- `cargo test -p fission-automation` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40 --baseline /Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-981676000/summary.json`
  - generated output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774246248-772213000`
  - go/stop gate emitted: `stop_hold_p5h3f`
  - changed rows emitted: `0`
  - subtype ranking surfaced from real rows: `arm_body_lowering_failed`, `complex_arm_shape`, `side_entry_or_exit`, `follow_beyond_window`

### P5H3F - Conditional-Tail Mismatch Subtype Harvesting + Bounded Follow Discovery

This patch shifted the focus from widening shape support to separating `ConditionalTailExitMismatch` into actionable subtype signals and introducing a bounded local follow discovery path in region recovery.

#### Changed

- added recovery-only conditional-tail mismatch subtype tracking in linear structuring:
  - `NoCommonFollowInWindow`
  - `FollowBeyondWindow`
  - `SideEntryOrExit`
  - `ComplexArmShape`
  - `ArmBodyLoweringFailed`
  - `AmbiguousMultipleFollows`
- introduced bounded first-common-follow discovery for region conditional tails:
  - forward-only, bounded steps, no-cycle progression
  - side-entry / side-exit guard before accepting shared follow candidate
- retained existing conservative behavior when guards fail:
  - mismatch still reports through `ConditionalTailExitMismatch`
  - no fallback broadening to global CFG/postdom passes
- added optional per-mismatch sample logging (env-gated):
  - `FISSION_RECOVERY_MISMATCH_TRACE=1`
  - emits JSONL under `/tmp/fission_preview_<function>_conditional_mismatch.jsonl`

#### Added

- synthetic regression for non-trivial shared follow discovery:
  - `region_recovery_lowers_two_arm_nontrivial_shared_follow`

#### Validation

- `cargo test -p fission-pcode region_recovery_lowers_two_arm_nontrivial_shared_follow -- --nocapture` (pass)
- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - same pre-existing failure on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - same pre-existing failures on current `main` remain:
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo check -p fission-pcode` (pass)
- `cargo build -p fission-automation` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-988203000`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774245667-981676000`

#### Corpus Outcome (vs P5H3E baseline)

- aggregate headline metrics remained stable in 40-function lane:
  - `region_linearized`: `1 -> 1`
  - `forced_linear`: `18 -> 18`
  - `region_linearize_rejected_body_lowering_failed_count`: `5 -> 5`
  - `conditional_tail_exit_mismatch`: `27 -> 27`
  - `successor_inline_rejected/revisit_cycle/unsupported_terminator`: still `0`
- new subtype telemetry now resolves previously opaque mismatch pressure:
  - `conditional_tail_follow_beyond_window`: `2`
  - `conditional_tail_side_entry_or_exit`: `4`
  - `conditional_tail_complex_arm_shape`: `19`
  - `conditional_tail_arm_body_lowering_failed`: `54`
  - `conditional_tail_no_common_follow_in_window`: `0`
  - `conditional_tail_ambiguous_multiple_follows`: `0`
- top mismatch rows remain the same addresses but now carry subtype split data for shape-targeted next patching.

### P5H3E - Conditional-Tail Normalization Widening (Localized Recovery)

This patch focused on reducing `conditional_tail_exit_mismatch` inside localized recovery without broadening general CFG support.

#### Changed

- added region-only conditional-tail arm normalization stage:
  - `normalize_conditional_tail_arm_for_region(...)`
  - explicitly separates canonical target from effective lowering start
- strengthened one-arm preference under region recovery:
  - if one arm reaches join via bounded trivial forwarding chain, prioritize one-arm if lowering on the opposite arm
- added conservative shared-tail reconciliation for two-arm region tails:
  - detects bounded forward-only trivial common tail entry
  - retries arm lowering to shared tail entry before lowering the shared tail to final join
  - constrained to region-recovery path only (forward-only, bounded, trivial forwarding)

#### Added

- synthetic regression tests for conditional-tail normalization widening:
  - `region_recovery_lowers_one_arm_join_adjacent_forwarding_chain`
  - `region_recovery_lowers_two_arm_shared_tail_entry`

#### Validation

- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - same pre-existing failure shape on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - both new synthetic P5H3E tests pass
  - same pre-existing failures on current `main` remain:
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774243155-357880000`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`
  - output: `/Users/sjkim1127/Fission/artifacts/fission-automation/1774243155-349905000`

#### Corpus Delta vs P5H3D Baseline

- baseline (P5H3D):
  - 5-function lane: `/artifacts/fission-automation/1774242470-100755000`
  - 40-function lane: `/artifacts/fission-automation/1774242496-398954000`
- P5H3E result:
  - 5-function lane: `region_linearized=0`, `forced_linear=2`, mismatch counters all `0` (unchanged)
  - 40-function lane:
    - `region_linearized=1` (unchanged)
    - `forced_linear=18` (unchanged)
    - `region_linearize_rejected_body_lowering_failed_count=5` (unchanged)
    - `conditional_tail_exit_mismatch=27` (unchanged)
    - `successor_inline_rejected=0` (unchanged)
    - `revisit_cycle=0` (unchanged)
    - `unsupported_terminator=0` (unchanged)

This indicates the conservative widening is behavior-safe and regression-clean for targeted synthetic shapes, but does not yet shift aggregate mismatch pressure in current 40-function corpus.

### P5H3D - Region Recovery Semantics Tightening and Corpus Closure

This patch tightened localized recovery semantics rather than broadening shape coverage. The focus was to preserve reject-reason fidelity across cache hits and make region target canonicalization origin-aware so conditional-tail normalization stays region-local and conservative.

#### Added

- regression coverage for semantics stability:
  - `lower_linear_body_region_cache_preserves_reject_reason_across_retries`
  - `region_canonicalization_respects_origin_guard`

#### Changed

- linear body cache now preserves reject reasons for localized (`region_recovery=true`) lowering cache entries instead of collapsing every cached reject into a generic class
- non-localized (`region_recovery=false`) detailed cache behavior remains conservative/generic to avoid changing broader structuring policy
- conditional-tail region canonicalization now uses the current conditional block index as origin instead of a fixed origin value
- added a test-only canonicalization hook to assert origin-guard behavior directly in synthetic coverage

#### Validation

- `cargo test -p fission-pcode structuring_linear -- --nocapture`
  - includes new cache-stability regression as passing
  - includes one pre-existing failure on current `main`:
    - `multi_block_preview_absorbs_shared_trivial_forwarding_return_tail`
- `cargo test -p fission-pcode structuring_conditionals -- --nocapture`
  - includes new origin-guard regression as passing
  - includes pre-existing failures on current `main` (confirmed unchanged on baseline `origin/main`):
    - `x86_pathological_try_lower_if_falls_back_without_hanging`
    - `multi_block_preview_lowers_canonical_if_else`
    - `multi_block_preview_lowers_if_else_with_multi_block_then_region`
    - `multi_block_preview_prefers_short_circuit_or_over_nested_plain_if`
    - `multi_block_preview_folds_short_circuit_and`
    - `multi_block_preview_folds_short_circuit_or`
- `cargo test -p fission-pcode bootstrap_x86 -- --nocapture` (pass)
- `cargo build -p fission-cli --features native_decomp` (pass)
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Corpus Outcome

- 5-function `nir` lane aggregate:
  - `recovery_structuring_mode_counts = {"forced_linear": 2}`
  - `region_linearized = 0`
  - body-lowering reject counters:
    - `region_linearize_rejected_body_lowering_failed_count = 0`
    - `conditional_tail_exit_mismatch = 0`
    - `successor_inline_rejected = 0`
    - `revisit_cycle = 0`
    - `unsupported_terminator = 0`
- 40-function (`preview` alias -> canonical `nir`) lane aggregate:
  - `recovery_structuring_mode_counts = {"forced_linear": 18, "region_linearized": 1}`
  - body-lowering reject counters:
    - `region_linearize_rejected_body_lowering_failed_count = 5`
    - `conditional_tail_exit_mismatch = 27`
    - `successor_inline_rejected = 0`
    - `revisit_cycle = 0`
    - `unsupported_terminator = 0`

This closes P5H3D as a semantics/measurement-hardening round. The next ranking signal remains conditional-tail mismatch pressure rather than unsupported-terminator inflation.

### P5H3C - Localized Body-Lowering Recovery Coverage Expansion

This patch targeted the next blocker called out in the previous quality round: reducing `region_linearized` rejection pressure from body-lowering failures without changing fallback policy.

The change expands localized trampoline canonicalization for nearby joins and fixes a conditional-tail lowering edge case where both arms canonicalized to the same join and were incorrectly re-lowered from the join itself.

#### Added

- new regression test for localized recovery over multi-hop trampoline joins:
  - `region_recovery_succeeds_on_multi_hop_trampoline_join`

#### Changed

- widened region target canonicalization window in localized recovery:
  - increased canonicalization hop budget for trivial forwarding trampolines
  - increased nearby-join trampoline distance allowance
- fixed conditional-tail localized lowering arm selection:
  - when canonicalization resolves directly to the join, branch lowering now starts from the original branch target arm instead of the join block
- updated linear structuring regression expectations for one-arm forwarding/trampoline-tail shapes that now lower successfully
- test helper visibility under `structuring` test wiring was aligned so test-only re-exports compile cleanly in the current layout
- removed an unused linear-body detailed wrapper to keep the structuring module warning-clean

#### Validation

- `cargo test -p fission-pcode region_recovery_succeeds_on_ -- --nocapture`
- `cargo check -p fission-pcode`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`

#### Current Outcome

- localized region recovery now handles deeper trivial trampoline joins that were previously prone to body-lowering rejection
- region-recovery regression coverage now includes the multi-hop join shape
- targeted NIR structuring tests and dependent crate builds completed successfully after the patch

## 2026-03-21

### quality-measurement-pipeline / P5H3B - Output Quality Metrics and Localized Recovery Instrumentation

This round added the first canonical output-quality measurement pipeline on top of the existing Fission NIR inventory and automation flow. The goal was not to change routing or recovery policy yet, but to make structured output ratios, linear fallback rates, and top structuring/build counters measurable on real corpus runs.

It also extended the localized structuring recovery path with reject-reason instrumentation so the current blocker is no longer opaque. The immediate outcome is that quality is now quantifiable, and the `region_linearized` bottleneck has been narrowed from a vague “localized fallback rarely triggers” problem down to a concrete `lower_linear_body` failure class.

#### Added

- row-level Fission NIR quality fields in CLI candidate/inventory output:
  - `nir_goto_count`
  - `nir_output_class`
  - `nir_build_stats`
- aggregate quality metrics in inventory summaries:
  - `nir_output_class_counts`
  - `nir_build_stats_totals`
- canonical automation quality artifact:
  - `artifacts/fission-automation/.../quality_measurement.json`
- new `NirBuildStats` counters for localized recovery diagnosis:
  - `forced_linear_structuring_count`
  - `region_linearize_structuring_count`
  - `region_linearize_heuristic_exit_count`
  - `region_linearize_rejected_non_structuring_failure_count`
  - `region_linearize_rejected_no_exit_count`
  - `region_linearize_rejected_body_lowering_failed_count`
  - `region_linearize_rejected_non_advancing_count`

#### Changed

- `fission-automation` terminal and Markdown reports now show:
  - structured ratio
  - linear fallback ratio
  - `nir_output_class_counts`
  - top `NirBuildStats` counters
- Fission NIR build stats are now preserved even when `build_hir` exits through a structuring error path
- failed `region_linearized` attempts now surface partial build stats into the later forced-linear recovery result, so localized recovery rejection is visible in corpus summaries
- localized recovery now tries a narrow nearby-join exit heuristic instead of relying only on a single `linear_exit(start_idx)` result

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- quality is now measurable from canonical artifacts instead of inferred from ad hoc logs:
  - `nir` smoke:
    - `structured_ratio=50.0%`
    - `linear_fallback_ratio=40.0%`
  - 40-function corpus run:
    - `structured_ratio=33.8%`
    - `linear_fallback_ratio=32.5%`
- the current `region_linearized` blocker is now explicit:
  - `region_linearize_rejected_body_lowering_failed_count = 5`
  - `region_linearize_rejected_no_exit_count = 0`
- recovery distribution is unchanged for now:
  - `recovery_attempted = 19`
  - `recovered = 19`
  - `forced_linear = 18`
  - `region_linearized = 1`
  - `high_goto_density = 14`
- this narrows the next patch target to localized body lowering rather than exit discovery

### P6R3 / P6R4 / P6R5 / P6R6 - Follow-up Fission NIR and CLI Module Extraction

This follow-up refactor round continued the post-rename cleanup without changing current decompilation semantics. The focus was to remove the next batch of oversized coordination files, move the Fission NIR implementation under a dedicated `decomp/nir/` subtree, and split CLI inventory/candidate execution code into clearer ownership modules.

The goal was still boundary cleanup, not policy change: legacy/NIR routing, recovery behavior, JSON compatibility, and automation baselines stayed intact. The result is that several formerly mixed-responsibility files are now thin façades, while the implementation sits in smaller modules with narrower ownership.

#### Added

- `fission-static` follow-up decompiler ownership files:
  - `caching_decompiler.rs`
  - `decomp/nir/context.rs`
  - `decomp/nir/engine.rs`
  - `decomp/nir/recovery.rs`
  - `decomp/nir/render.rs`
  - `decomp/nir/routing.rs`
  - `decomp/nir/taxonomy.rs`
  - `decomp/nir/types.rs`
  - `decomp/nir/worker.rs`
- CLI inventory ownership modules:
  - `cli/oneshot/inventory/schema.rs`
  - `cli/oneshot/inventory/provenance.rs`
  - `cli/oneshot/inventory/emit.rs`
- CLI execution ownership modules:
  - `cli/oneshot/decompile/decompile_exec/batch.rs`
  - `cli/oneshot/decompile/decompile_exec/output.rs`
  - `cli/oneshot/decompile/decompile_exec/run.rs`
- CLI NIR candidate ownership modules:
  - `cli/oneshot/decompile/nir_candidates/schema.rs`
  - `cli/oneshot/decompile/nir_candidates/summary.rs`
  - `cli/oneshot/decompile/nir_candidates/build.rs`

#### Changed

- Fission NIR source files now live physically under `crates/fission-static/src/analysis/decomp/nir/`, while `decomp/mod.rs` keeps the existing public module surface through `#[path = "nir/..."]` wiring
- `crates/fission-static/src/analysis/decomp/mod.rs` no longer owns the native cached decompiler implementation directly:
  - `DecompilerNative`
  - `CachingDecompiler`
  - `RecommendedDecompiler`
  moved into `caching_decompiler.rs`, and `mod.rs` now mainly acts as a re-export surface
- `crates/fission-cli/src/cli/oneshot/inventory.rs` is now a thin façade:
  - schema types moved to `inventory/schema.rs`
  - provenance/fact aggregation moved to `inventory/provenance.rs`
  - decompiler prep and emit loop moved to `inventory/emit.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/decompile_exec.rs` is now a thin façade:
  - batch inventory/candidate emit moved to `decompile_exec/batch.rs`
  - single-function output path moved to `decompile_exec/output.rs`
  - sequential/parallel run orchestration moved to `decompile_exec/run.rs`
- `crates/fission-cli/src/cli/oneshot/decompile/nir_candidates.rs` is now a thin façade:
  - row/inventory schema moved to `nir_candidates/schema.rs`
  - summary/failure/signature logic moved to `nir_candidates/summary.rs`
  - candidate row construction and panic recovery moved to `nir_candidates/build.rs`

#### Validation

- `cargo fmt`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- the next batch of coordination files is now physically reduced:
  - `decomp/mod.rs`: `199 -> 88` lines
  - `inventory.rs`: `627 -> 5` lines
  - `decompile/decompile_exec.rs`: `951 -> 6` lines
  - `decompile/nir_candidates.rs`: `849 -> 10` lines
- canonical `nir` automation smoke remains stable after the refactor:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`

## 2026-03-20

### P6R2 - Real Module Split After Fission NIR Rename

This round turned the earlier Fission NIR rename into a real responsibility split. The goal was boundary cleanup, not behavior change: `nir_engine.rs`, `decompile.rs`, and `structuring/mod.rs` were reduced to thin orchestration facades while the actual implementation moved into focused ownership modules.

The refactor kept current recovery policy, fallback semantics, dual-written JSON compatibility fields, and local automation behavior intact. Deprecated aliases such as `mlil-preview` and the `preview` automation lane still work, but the canonical code paths are now physically organized around `nir` ownership boundaries.

#### Added

- `fission-static` Fission NIR ownership modules:
  - `nir_types.rs`
  - `nir_taxonomy.rs`
  - `nir_worker.rs`
  - `nir_render.rs`
  - `nir_recovery.rs`
  - `nir_routing.rs`
- CLI oneshot decompilation submodules:
  - `decompile/decompile_exec.rs`
  - `decompile/decompile_render.rs`
  - `decompile/decompile_targets.rs`
  - `decompile/nir_candidates.rs`
- NIR structuring ownership submodules:
  - `structuring/cleanup.rs`
  - `structuring/guards.rs`
  - `structuring/surfacing.rs`
  - `structuring/recovery.rs`
  - `structuring/driver.rs`

#### Changed

- `crates/fission-static/src/analysis/decomp/nir_engine.rs` is now a thin façade that re-exports:
  - canonical Fission NIR types
  - taxonomy helpers
  - worker entrypoints
  - routing/recovery entrypoints
  - deprecated preview compatibility wrappers
- `crates/fission-cli/src/cli/oneshot/decompile.rs` is now a thin façade:
  - actual execution moved to `decompile_exec.rs`
  - candidate/report logic moved to `nir_candidates.rs`
  - render/output helpers moved to `decompile_render.rs`
  - target selection moved to `decompile_targets.rs`
- internal CLI candidate types were renamed to `NirCandidate*`, while compatibility aliases for `PreviewCandidate*` remain in place for existing consumers
- `crates/fission-pcode/src/nir/structuring/mod.rs` is now a thin driver/re-export surface:
  - cleanup helpers moved to `cleanup.rs`
  - guarded-tail and promotion logic moved to `guards.rs`
  - typed structuring failure surfacing moved to `surfacing.rs`
  - localized/forced-linear recovery moved to `recovery.rs`
  - top-level structuring orchestration moved to `driver.rs`
- automation lane normalization still maps deprecated `preview` to canonical `nir`, and both lanes continue to deserialize dual-written `nir_*` / `preview_*` fields without drift

#### Validation

- `cargo fmt`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo check -p fission-analysis`
- `cargo check -p fission-tauri`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp-all --decomp-limit 1 --engine nir --json`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp-all --decomp-limit 1 --engine mlil-preview --json --verbose`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- the three main refactor targets are now physically split:
  - `nir_engine.rs`: `1620 -> 444` lines
  - `decompile.rs`: `2171 -> 45` lines
  - `structuring/mod.rs`: `1159 -> 20` lines
- canonical `nir` and deprecated `preview` automation lanes still produce the same current smoke result:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`
- canonical CLI output and deprecated `mlil-preview` alias still converge on the same engine result for the smoke sample:
  - `engine_used = nir`
  - `fell_back = false`

### P6R1 - Fission NIR Rename and Preview/Recovery Refactor

This round renamed the public Rust-owned decompiler lane from `preview` / `mlil-preview` to **Fission NIR**, while keeping compatibility aliases so existing CLI usage, local automation baselines, and worker invocations continue to function during the transition.

The goal was not to change recovery policy. The goal was to make the naming and code boundaries match the actual architecture: `legacy` remains the compatibility lane, while `nir` is now the canonical token for the Rust-owned decompiler path.

Historical changelog entries may still mention `mlil-preview` when describing earlier behavior. From this point forward, the canonical name is **Fission NIR** and the canonical machine-facing token is `nir`.

#### Added

- canonical `fission_nir_worker` binary alongside the deprecated compatibility `fission_preview_worker`
- canonical `nir` automation lane with deprecated `preview` lane alias support
- canonical `nir_*` inventory/report fields with continued compatibility for `preview_*` consumers during the transition
- `nir_context`, `nir_engine`, `nir_taxonomy`, `nir_recovery`, and `nir_worker` module boundaries under `fission-static`

#### Changed

- `preview_engine.rs` and `preview_context.rs` were renamed to:
  - `nir_engine.rs`
  - `nir_context.rs`
- canonical engine/token naming now prefers:
  - CLI engine: `nir`
  - automation lane: `nir`
  - user-facing product name: `Fission NIR`
- deprecated aliases remain accepted:
  - `--engine mlil-preview`
  - `--profile mlil-preview`
  - `--lane preview`
  - `FISSION_PREVIEW_WORKER`
  - `fission_preview_worker`
- `fission-automation` now dual-reads canonical `nir_*` fields and deprecated `preview_*` fields without failing when both are present in the same JSON row/summary
- Tauri decompiler engine settings and labels now prefer `nir` / `Fission NIR`, while still accepting stored `mlil_preview` values
- public docs were updated to describe the Rust-owned lane as **Fission NIR** instead of `mlil-preview`

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo check -p fission-tauri`
- `cargo check -p fission-analysis`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140001190 --engine nir --timeout-ms 1500`
- `./target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140001190 --engine mlil-preview --timeout-ms 1500 --verbose`
- `cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 5`

#### Current Outcome

- canonical Fission NIR naming is now the default across CLI, automation, Tauri, and top-level docs
- deprecated `mlil-preview` and `preview` aliases still work and now emit deprecation warnings on the CLI/automation path
- `fission-automation` successfully reads dual-written inventory rows and summaries again after the compatibility deserialization fix
- `nir` and deprecated `preview` lanes both complete with the same current smoke result:
  - `direct_success=10`
  - `nir_failure=0`
  - `explicit_nonzero=4`
  - `recovery_attempted={'linearized_structuring_retry': 2}`
  - `recovery_outcome={'recovered': 2}`

### P5H2B / P5H3A - Recovery Quality Metrics and Localized Structuring Fallback

This round moved structuring recovery from a binary “recovered or not” signal into a quality-aware lane, and introduced the first localized alternative to whole-function forced linearization.

Previously, `linearized_structuring_retry` could recover many structuring-origin failures, but the recovery path only measured success counts. In practice, most recovered outputs were still whole-function `forced_linear` renders with high goto density, which made the strategy useful as a backstop but too expensive to promote as a first-class whitelist recovery mode.

This patch added row/summary quality metrics for recovered outputs and inserted a new recovery mode between `normal` and `forced_linear`:

- `normal`
- `region_linearized`
- `forced_linear`

The new `region_linearized` path reuses linear structuring only for the failed CFG slice when a recovery-eligible structuring failure surfaces, then resumes the normal structured path for the remainder of the function.

#### Added

- recovery quality metadata on preview rows and inventory rows:
  - `recovery_source_signature`
  - `recovery_structuring_mode`
  - `recovery_goto_count_before`
  - `recovery_goto_count_after`
  - `recovery_hint_surface_before`
  - `recovery_hint_surface_after`
  - `recovery_quality_flags`
- quality summary aggregation:
  - `recovery_quality_flag_counts`
  - `recovery_structuring_mode_counts`
- localized recovery quality flags:
  - `localized_linearization`
  - `shape_partially_linearized`

#### Changed

- recovery quality accounting now distinguishes:
  - whole-function `forced_linear`
  - localized `region_linearized`
- `linearized_structuring_retry` now tries:
  1. localized region linearization
  2. whole-function forced linearization
  3. fallback failure
- NIR structuring now attempts region-scoped linear recovery for recovery-eligible structuring-origin failures before surfacing the error back out
- recovery mode counts now track recovery-attempted rows only instead of mixing in non-recovery `normal` rows

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- aggregate recovery stayed stable:
  - `recovery_attempted {'linearized_structuring_retry': 19}`
  - `recovery_outcome {'recovered': 19}`
- recovery mode split improved from:
  - previous: `{'forced_linear': 19}`
  - current: `{'forced_linear': 18, 'region_linearized': 1}`
- quality proxy improved slightly:
  - `high_goto_density: 15 -> 14`
  - `shape_linearized: 19 -> 18`
  - `shape_partially_linearized: 1`
  - `localized_linearization: 1`
- current verdict remains:
  - `linearized_structuring_retry` is still valuable for recovery
  - but it remains closer to `fallback-only` than `whitelist-worthy`
  - the next quality step should reduce dependence on whole-function `forced_linear` by broadening localized / semi-structured fallback coverage

### P5H2A - Structuring-Origin Failure Surfacing for Recovery

This round fixed the taxonomy gap that prevented the recovery layer from seeing real structuring-origin failures.

Previously, recovery scaffolding existed, but a large part of the relevant `UnsupportedCfg*` family was either absorbed as `Ok(None)` inside NIR structuring or surfaced back out as a broad unsupported-CFG failure. That meant `linearized_structuring_retry` often had no explicit recovery seed to act on.

This patch promoted the recovery-eligible structuring failures into typed preview failures and preserved their exact signature through the inventory/export path.

#### Added

- typed structuring failure classification:
  - `StructuringFailureKind::RegionShape`
  - `StructuringFailureKind::PhiJoin`
  - `StructuringFailureKind::IndirectCallRegion`
- exact preview block signatures for recovery-eligible structuring failures:
  - `unsupported_cfg_region_shape`
  - `unsupported_cfg_phi_join`
  - `unsupported_cfg_indirect_call_region`

#### Changed

- NIR structuring no longer fully buries recovery-eligible `UnsupportedCfg*` failures behind plain `Ok(None)` paths
- preview routing now surfaces those failures as:
  - coarse kind: `preview_structuring_failure`
  - exact signature: typed structuring-origin signature
- `UnsupportedCfgBranchTarget` remains on the separate branch-target / unsupported-CFG line and is not mixed into the structuring-recovery lane
- `linearized_structuring_retry` is now fed from explicit structuring-origin surfacing rather than heuristic string matching alone

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- `preview` lane runs now show real recovery activity instead of an empty recovery scaffold
- `GetProcAddress.exe` inventory summary recorded:
  - `recovery_attempted {'linearized_structuring_retry': 13}`
  - `recovery_applied {'linearized_structuring_retry': 13}`
  - `recovery_outcome {'recovered': 13}`
- `putty.exe` inventory summary recorded:
  - `recovery_attempted {'linearized_structuring_retry': 6}`
  - `recovery_applied {'linearized_structuring_retry': 6}`
  - `recovery_outcome {'recovered': 6}`
- the recovery layer is now being driven by surfaced structuring-origin failures rather than sitting idle without visible seeds

### Operational Stability - NIR Structuring Recursion Fix and Automation Watchdog

This round fixed a real Fission NIR preview hang instead of just treating it as heavy CPU work.

`GetProcAddress.exe` contained addresses that drove the NIR linear-structuring path into recursive conditional-tail cycling. Those same functions completed on the legacy lane, which confirmed the issue was a preview/NIR bug rather than expected analysis cost.

At the same time, the automation runner could wait forever on a stuck `fission_cli` child, which meant a single pathological function could wedge an entire lane.

#### Added

- active-cycle guards for:
  - in-progress `LinearBodyCacheKey` lowering
  - in-progress conditional-tail lowering signatures
- a new regression test that exercises the recursive conditional-tail cycle and verifies it fails closed instead of spinning
- a hard inventory child-process watchdog in `fission-automation`
- periodic mid-run inventory summary flushes so partial progress survives long runs or failures

#### Changed

- Fission NIR linear structuring now returns `None` when it re-enters the same linear-body or conditional-tail request instead of recursing indefinitely
- `fission-automation` now kills and reaps inventory children that exceed a hard per-binary timeout budget
- `nir-check` skips failed binaries instead of hanging an entire lane forever, and only fails the lane if every target fails
- CLI inventory summary files now update during row emission rather than only at chunk completion

#### Validation

- `cargo test -p fission-pcode lower_linear_body_breaks_recursive_conditional_cycle -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140002220 --engine mlil-preview --timeout-ms 1500`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --decomp 0x140002320 --engine mlil-preview --timeout-ms 1500`
- `target/debug/fission_cli samples/other/binaries-master/tests/x86_64/windows/GetProcAddress.exe --emit-function-facts-inventory --functions-limit 40 --timeout-ms 1500 --output-jsonl /tmp/getproc_after_fix.rows.jsonl --summary-json /tmp/getproc_after_fix.summary.json --quiet-batch-errors`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli --functions-limit 40`

#### Current Outcome

- the reproduced `mlil-preview` hangs at `0x140002220` and `0x140002320` now complete in under a second instead of timing out externally
- `GetProcAddress.exe` 40-function inventory now finishes cleanly and writes both rows and summary output
- the `preview` lane completes successfully again instead of sticking on a single runaway `fission_cli` process
- remaining preview failures on the sentinel lane are now meaningful failure classes, not infinite-CPU recursion artifacts

### P5H1 - Failure-Driven Recovery Scaffold

This round introduced the first real recovery layer for Fission NIR preview failures.

Until now, preview-side failures were mainly classified and reported. After this patch, selected failure signatures can carry an explicit recovery strategy attempt, and the result of that attempt is exported through the same inventory/report path.

#### Added

- recovery metadata on preview routing decisions and selections:
  - `recovery_strategy_attempted`
  - `recovery_strategy_applied`
  - `recovery_outcome`
- first whitelist recovery strategy:
  - `linearized_structuring_retry`
- inventory / summary recovery accounting:
  - `recovery_strategy_attempted_counts`
  - `recovery_strategy_applied_counts`
  - `recovery_outcome_counts`

#### Changed

- `MlilPreviewOptions` now supports a narrow `force_linear_structuring` mode
- `preview_structuring_failure` can now trigger a single linear-structuring retry instead of falling directly to a plain failure record
- CLI inventory rows and automation summaries now preserve recovery metadata alongside existing preview block signature/detail fields

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`

#### Current Outcome

- the failure-driven recovery scaffold is now present in code and data models
- the first whitelist strategy exists and is wired into preview routing
- current `preview` sentinel lane still has no `preview_structuring_failure` sample, so:
  - recovery counters remain empty in the lane smoke
  - the next step is to secure a real structuring-failure seed and validate whether `linearized_structuring_retry` recovers it, preserves the same failure, or narrows it into a better signature

### P6 - `fission-automation` Canonical Quality Runner

This round replaced the old ad hoc benchmark-script loop with a tracked Rust automation crate that acts as the canonical local quality runner for Fission NIR.

Instead of manually chaining hidden CLI modes, Python corpus scripts, and one-off shell commands, the repository now has a single Rust entrypoint for lane-based quality runs:

- `cargo run -p fission-automation -- nir-check --lane pdb`
- `cargo run -p fission-automation -- nir-check --lane preview`
- `cargo run -p fission-automation -- nir-check --lane regression`
- `cargo run -p fission-automation -- nir-check --lane full`

#### Added

- new tracked workspace crate:
  - `crates/fission-automation`
- tracked automation config:
  - `crates/fission-automation/config/sentinel_sets.toml`
  - `crates/fission-automation/config/timeout_rescue.json`
- Rust-first local quality pipeline support for:
  - sentinel lane loading
  - inventory emit orchestration through `fission_cli --emit-function-facts-inventory`
  - diagnosis aggregation
  - corpus refinement
  - baseline diffing
  - Markdown / JSON summaries under `artifacts/fission-automation/`

#### Changed

- repository benchmark ownership
  - `fission-automation` is now the canonical local runner for Fission NIR quality loops
  - benchmark/config state previously kept under `scripts/test/batch_benchmark` has moved into the automation crate or local `artifacts/`
- documentation
  - README and benchmark/debug docs now point at `fission-automation` lane runs instead of the retired Python benchmark scripts

#### Removed

- retired tracked Python batch-benchmark drivers and tracked corpus outputs from:
  - `scripts/test/batch_benchmark/`
- the old Python diagnosis / corpus-refinement path is no longer the default execution path

#### Validation

- `cargo build -p fission-automation`
- `cargo run -p fission-automation -- nir-check --lane preview --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`
- `cargo run -p fission-automation -- nir-check --lane pdb --no-build --fission-bin /Users/sjkim1127/Fission/target/debug/fission_cli`

#### Observed Effect

- `preview` lane smoke:
  - `direct_success = 27`
  - `preview_failure = 3`
  - `explicit_nonzero = 11`
  - `strict_explicit = 1`
- `pdb` lane smoke:
  - `direct_success = 43`
  - `preview_failure = 3`
  - `explicit_nonzero = 30`
  - `strict_explicit = 12`
  - `pdb_nonzero_rows = 21`

This means the local quality loop is no longer tied to a scattered script folder. The canonical path now lives in a tracked Rust crate, while reports remain local-only under `artifacts/`.

### P5G - Focused PDB Function-Facts Ingestion

This round moved PDB handling from “source presence is visible” into real function-level fact ingestion for the Fission NIR pipeline.

Instead of building a full PDB parser, the loader now performs a narrow sidecar-driven ingest for function-scoped facts that directly affect decompilation quality:

- function names
- return types
- parameter names
- parameter types

These facts now flow into the existing Rust facts pipeline rather than staying trapped as loader metadata.

#### Added

- focused PDB sidecar ingestion in the loader
  - PE CodeView / RSDS / NB10 metadata is now used to locate and open matching `.pdb` sidecars
  - module symbol streams are scanned narrowly for function-scoped facts instead of attempting broad PDB database coverage
- function-level PDB facts in `FactStore`
  - `FactProvenance::PdbMetadata`
  - `FunctionFacts.pdb_info`
  - `FactStore::preferred_debug_function(...)` now falls back from DWARF to PDB-backed function info
- inventory explicit surfacing for PDB-derived facts
  - `explicit_fact_breakdown.pdb_type_count`
  - `explicit_breakdown_totals.pdb_type_count`
  - inventory row names now prefer the chosen resolved fact name when available

#### Changed

- preview / postprocess debug fact consumption
  - preview function hints can now use PDB-backed function info when DWARF is absent
  - Rust-side postprocess also consumes preferred debug function info instead of assuming DWARF-only availability
- diagnosis quality after PDB source detection
  - the pipeline can now distinguish:
    - `PDB source present and actually surfaced`
    - `PDB source present but still not surfaced`
    - `native inferred facts are still filling the gap`

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-loader loads_focused_pdb_function_facts_from_repo_sample -- --nocapture`
- inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `test-pdb.exe`
  - `fauxware.exe`

#### Observed Effect

- `test-pdb.exe`
  - `source_presence_counts.pdb = 6`
  - `provenance_surface_totals.pdb_nonzero_rows = 5`
  - `strict_explicit_candidate_count = 4`
- `fauxware.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 16`
  - `strict_explicit_candidate_count = 6`
- `has_pdb.exe`
  - `source_presence_counts.pdb = 20`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 7`

This means the repository now has both sides of the diagnostic split:

- samples where PDB-derived function facts genuinely surface into inventory rows,
- and samples where PDB source presence is truthful but surfaced explicit rows are still being supplied by native inferred facts.

## 2026-03-19

### P5F2 - Preview-Stage Block Split And First Narrow Unblock

This round moved preview-side diagnosis from “generic unknown failure cleanup” into the first real unblock patch for the Fission NIR path.

The work happened in two steps:

- first, preview-stage failures were split so that pcode/frontend acquisition failures stopped polluting the real preview block bucket,
- then a single recoverable `unsupported_indirect_branch_target` shape was patched without broadening indirect control-flow support.

#### Added

- preview block signature reporting in inventory-backed rows
  - rows now carry:
    - `preview_block_signature`
    - `preview_block_detail`
- finer preview-stage diagnosis buckets
  - `preview_frontend_reject` is now separated from genuine preview CFG failures
  - diagnosis summaries can aggregate preview block signatures directly
- narrow instruction-local relative branch target support in the Fission NIR pcode path
  - recoverable constant-space pcode branch targets are now resolved by exact target block index
  - duplicate-start blocks can now be distinguished through synthetic target keys / labels instead of collapsing into one canonical start address

#### Changed

- preview inventory / diagnosis interpretation
  - `native_pcode_failure`-like cases that previously looked like preview unknowns are now surfaced as frontend rejection rather than preview-stage block
- preview control-flow lowering
  - branch and cbranch lowering now use resolved target block indices for the supported instruction-local relative-target shape
- structuring path label/target handling
  - duplicate-start block targets are preserved narrowly enough to support the recovered branch shape without enabling broad indirect branch handling

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- `cargo test -p fission-pcode preview_supports_instruction_local_conditional_branch_targets -- --nocapture`
- `cargo test -p fission-pcode preview_supports_instruction_local_unconditional_branch_targets -- --nocapture`
- inventory smoke reruns:
  - `GetProcAddress.exe --functions-limit 20`
  - `putty.exe --functions-limit 10`

#### Observed Effect

- `GetProcAddress.exe`
  - before:
    - `direct_success_count = 16`
    - `preview_frontend_reject = 3`
    - `preview_unsupported_cfg = 1`
    - dominant preview-side signature: `unsupported_indirect_branch_target`
  - after:
    - `direct_success_count = 17`
    - `preview_failure_count = 3`
    - remaining failures are all `preview_frontend_reject`
    - the representative blocked row at `0x140001190` now becomes `preview_direct_success = true`
- `putty.exe`
  - 10-function smoke rerun stayed stable with:
    - `direct_success_count = 10`
    - `preview_failure_count = 0`

This means the first real preview-side unblock is now in place: one recoverable `unsupported_indirect_branch_target` class has moved onto the success path without widening support to general indirect branch control flow.

### P5F1 - Provenance Completeness For Function Facts Inventory

This round refined the inventory from “provenance-aware” toward “provenance-complete enough to guide the next core patch.”

The main improvement is that inventory output can now distinguish between:

- sources that carry PDB-style debug provenance,
- function rows that actually surface explicit facts,
- and cases where surfaced explicit rows are still being supplied by native inferred facts rather than by PDB-derived facts.

#### Added

- provenance fact breakdown in function inventory rows
  - rows now include `provenance_fact_breakdown` with:
    - `dwarf_type_count`
    - `pdb_type_count`
    - `native_type_count`
    - `loader_type_count`
- provenance surface totals in inventory summaries
  - summaries now report:
    - `dwarf_nonzero_rows`
    - `pdb_nonzero_rows`
    - `native_nonzero_rows`
    - `loader_nonzero_rows`
- function snapshot provenance helpers
  - `FunctionFacts` now exposes:
    - `dwarf_type_fact_count()`
    - `pdb_type_fact_count()`
    - `native_type_fact_count()`
    - `loader_type_fact_count()`

#### Changed

- PDB source presence detection
  - `fact_sources_present.pdb` is no longer a placeholder
  - inventory now treats `.pdb` sidecars and embedded PE `RSDS` / `.pdb` markers as real PDB source presence signals
- diagnosis interpretation
  - inventory-guided diagnosis can now distinguish:
    - `pdb source present but no pdb-surfaced explicit rows`
    - `native inferred facts are currently covering the explicit surface gap`

#### Validation

- `cargo test -p fission-static snapshot_counts_dwarf_type_facts_from_function_info -- --nocapture`
- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- smoke inventory / diagnosis reruns:
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `source_presence_counts.pdb = 10`
  - `provenance_surface_totals.pdb_nonzero_rows = 0`
  - `provenance_surface_totals.native_nonzero_rows = 5`
  - diagnosis now shows that PDB provenance is present, but surfaced explicit rows are still coming from native inferred facts

This means the next preview-side or facts-side patch can target real remaining gaps without provenance confusion.

### P5D / P5E - Inventory-Guided Diagnosis And Function-Level Facts Surfacing

This round stopped treating explicit-facts scarcity as a vague benchmark problem and turned it into a concrete inventory diagnosis plus a core data-path patch.

The key result is that aligned sources no longer have to stay stuck in a blanket `inventory_surface_gap` bucket. Inventory-backed diagnosis identified where provenance existed but explicit rows still stayed at zero, and the inventory export now promotes function-level native inferred facts into the explicit surface instead of leaving them hidden behind generic provenance flags.

#### Added

- inventory-guided diagnosis runner
  - added `scripts/test/batch_benchmark/diagnose_function_inventory.py`
  - classifies aligned binaries into:
    - `source_facts_absent`
    - `factstore_or_inventory_surface_gap`
    - `preview_stage_block`
    - `mixed_or_inconclusive`
  - emits a per-binary diagnosis plus a recommended next patch direction
- function snapshot helpers for type-fact provenance
  - `FunctionFacts` now exposes separate counts for:
    - native type facts
    - loader type facts

#### Changed

- function inventory explicit surfacing
  - inventory export now ingests function-level native inferred types during whole-binary row generation
  - `explicit_fact_breakdown` now includes `native_type_count`
  - `explicit_fact_total` now counts surfaced native function facts in addition to DWARF param/local/return facts
- inventory surface-gap interpretation
  - `inventory_surface_gap` is no longer triggered by image-wide loader metadata alone
  - the gap signal now focuses on per-function/debug provenance that should realistically surface as explicit facts
- strict explicit candidate detection in inventory
  - strict candidate evaluation now uses the surfaced inventory explicit total rather than only the DWARF-only count

#### Validation

- `cargo test -p fission-static snapshot_counts_native_and_loader_type_facts_separately -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- inventory smoke reruns:
  - `has_pdb.exe`
  - `putty.exe`
- inventory-guided diagnosis rerun:
  - `GetProcAddress.exe`
  - `has_pdb.exe`
  - `putty.exe`

#### Observed Effect

- `has_pdb.exe`
  - `explicit_fact_nonzero_count`: `0 -> 5`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`
- `putty.exe`
  - `explicit_fact_nonzero_count`: `0 -> 7`
  - `inventory_surface_gap_count`: `10 -> 0`
  - `strict_explicit_candidate_count`: `0 -> 1`

This moves the project past “why are explicit facts missing?” into a narrower question: which remaining aligned binaries are still blocked by preview-stage issues, and which ones still need more supply-path surfacing.

### P5A / P5B / P5C - Function Facts Inventory, Inventory-Backed Corpus Selection, And Provenance-Aware Analysis

This round changed the benchmark/corpus workflow from probe-first scanning to inventory-first filtering.

The key architectural shift is that benchmark scripts no longer need to treat address-targeted preview scans as the canonical source of truth. Instead, the CLI can now export whole-binary function facts as a structured inventory, and corpus generation can filter that inventory into strict explicit, heuristic, aligned, and blocked views.

#### Added

- whole-binary function facts inventory export
  - added hidden CLI mode `--emit-function-facts-inventory`
  - emits row-level JSONL plus summary JSON from a single binary load / decompiler preparation pass
- inventory row metadata for corpus selection
  - rows now carry function-level facts, preview admission results, pcode size, and structured row failure fields in one place
- Python inventory reader helper
  - added `scripts/test/batch_benchmark/grand_finale_support/inventory_reader.py`
  - centralizes:
    - running the Rust inventory export
    - loading inventory JSONL rows
    - loading summary JSON
- provenance-aware inventory fields
  - inventory rows now include:
    - `fact_sources_present`
    - `explicit_fact_breakdown`
    - `admission_block_stage`
    - `inventory_surface_gap`
  - summary output now includes:
    - `source_presence_counts`
    - `explicit_breakdown_totals`
    - `inventory_surface_gap_count`
    - `aligned_with_zero_explicit_count`

#### Changed

- benchmark/corpus scripts now consume inventory rows
  - `refine_preview_quality_corpus.py` now builds corpus outputs from function facts inventory rows instead of address-probe scan results
  - `grand_finale_support/corpus_candidates.py` now treats the Rust inventory export as the default candidate source
- provenance-aware blocked/aligned interpretation
  - blocked and aligned candidate reports now carry provenance fields through from the inventory rows
  - corpus refinement now emits aggregated inventory provenance counters alongside blocked explicit summaries
- corpus outputs derived from the same canonical source
  - `preview_quality_corpus.json`
  - `preview_explicit_blocked_candidates.json`
  - `preview_explicit_aligned_candidate_report.json`
  are now designed to be generated from the same inventory-backed function row source

#### Validation

- `cargo build -p fission-cli --features native_decomp`
- function facts inventory smoke
  - `putty.exe --emit-function-facts-inventory --functions-limit 3`
  - verified row JSONL and summary JSON emission
- inventory-backed corpus smoke
  - `refine_preview_quality_corpus.py` against `GetProcAddress.exe`
  - verified generation of:
    - candidates JSON
    - aligned candidate report
    - blocked explicit report
    - curated corpus JSON
- provenance-aware inventory smoke
  - `GetProcAddress.exe --emit-function-facts-inventory --functions-limit 5`
  - verified:
    - row-level provenance fields
    - summary-level provenance counters
    - blocked report inventory summary totals

#### Current State

- address-targeted scans remain useful, but they are now probe/debug tooling rather than the preferred canonical data source
- strict explicit / heuristic / blocked / aligned analysis can now be driven from one whole-binary inventory export
- inventory rows now expose whether explicit-fact scarcity appears to come from missing source facts, inventory surface gaps, or preview-stage rejection

## 2026-03-18

### P4.8 / P4.8.2 - Explicit-Facts PE Source Expansion

This round focused on finding PE samples that can actually exercise the new explicit preview hint paths without weakening the meaning of the strict explicit corpus.

The main result was diagnostic rather than cosmetic:

- the strict `quality_explicit_facts` corpus remains intentionally empty,
- blocked explicit candidates are now tracked separately,
- and the remaining bottleneck is clearly sample scarcity plus lack of direct-preview overlap, not corpus/refinement logic.

#### Added

- explicit source inventory metadata
  - expanded the PE candidate pool with LLVM, `samples/other`, and other debug-info-rich Windows binaries
  - recorded per-source metadata including:
    - `toolchain`
    - `debug_info_kind`
    - `has_loader_types`
    - `priority`
    - `notes`
- blocked explicit candidate tracking
  - added a dedicated blocked-candidate artifact instead of weakening the strict explicit corpus

#### Changed

- explicit corpus discipline
  - kept `quality_explicit_facts` strict rather than filling it with provisional fallback seeds
  - continued to require:
    - `explicit_fact_total >= 2`
    - `preview_direct_success == true`
    - `has_indirect_control_flow == false`
    - `pcode_op_count <= 800`
- blocked-candidate reporting
  - normalized blocked explicit candidates under the current taxonomy
  - preserved raw fallback information where the engine still reports only coarse `preview_unsupported` results
  - added summary counts for:
    - blocked-reason distribution
    - newly scanned zero-explicit sources
    - newly scanned timeout sources

#### Current State

- strict explicit corpus: still empty by design
- blocked explicit candidates:
  - `main-debug.exe`
  - `addr.exe`
- dominant blocked reason:
  - `preview_non_success_unknown`

This means the benchmark/reporting pipeline is no longer the limiting factor. The next step is better fact-rich PE source acquisition, not provisional promotion of blocked seeds.

### v104 - 3-Way Benchmark Expansion (`pyghidra` vs `legacy` vs `preview`)

This round expanded the public benchmarking story from two separate comparisons into a consistent 3-way model:

- `pyghidra` as the Python-host baseline,
- `legacy` as the native FFI / Ghidra core baseline,
- `preview` as the Rust-owned decompiler pipeline.

The main goal was not a single blended score, but a benchmark shape that shows where overhead, fallback behavior, and readability improvements come from.

#### Added

- shared resource monitor helper for benchmark scripts
  - added `scripts/test/batch_benchmark/grand_finale_support/resource_monitor.py`
  - reused the same optional `psutil`-based RSS / CPU sampling model in both benchmark modes
- function-level 3-way artifact shape
  - `compare_legacy_preview.py` now emits `pyghidra`, `legacy`, and `preview` together
  - added `three_way_delta` and `winner_summary` per function
- whole-binary 3-way raw outputs
  - now writes `legacy_full.json`, `preview_full.json`, and `ghidra_full.json`

#### Changed

- fixed-seed function-level comparison
  - promoted `compare_legacy_preview.py` into the main 3-way fixed-seed comparison path
  - kept existing `legacy` / `preview` fields for backward compatibility
  - added engine-level summaries and pairwise deltas:
    - `pyghidra_vs_legacy`
    - `legacy_vs_preview`
    - `pyghidra_vs_preview`
- timing and resource metrics
  - added shared timing stats with `p95_ms`
  - added best-effort per-run resource summaries:
    - `max_rss_mb`
    - `avg_rss_mb`
    - `avg_cpu_pct`
    - `max_cpu_pct`
- whole-binary benchmark summary
  - replaced the old 2-way summary with explicit engine buckets:
    - `pyghidra`
    - `legacy`
    - `preview`
  - added pairwise quality/similarity sections and a public-ready summary line
- benchmark documentation
  - updated `scripts/test/batch_benchmark/README.md` to describe both benchmark modes and the 3-way engine model

#### Validation

- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/compare_legacy_preview.py`
  - `scripts/test/batch_benchmark/full_decomp_benchmark.py`
  - `scripts/test/batch_benchmark/grand_finale_support/*.py`
- `cargo build -p fission-cli --features native_decomp`
- function-level 3-way smoke
  - `test_control_flow_x64_O0.exe 0x140001010`
  - artifact:
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.json`
    - `/tmp/v104_compare_smoke2/test_control_flow_x64_O0_legacy_vs_preview.md`
- whole-binary 3-way smoke
  - `test_control_flow_x64_O0.exe --limit 1`
  - artifact:
    - `/tmp/v104_full_smoke2/benchmark_summary.json`
    - `/tmp/v104_full_smoke2/benchmark_summary.md`

## 2026-03-17

### Repository Licensing + CLA Setup

The public repository license was fixed to AGPL-3.0, and a Contributor License Agreement was added to support a future open-core operating model. The intent is to keep the core engine open under AGPL-3.0 while preserving a clean legal boundary for accepting outside contributions.

#### Added

- root license file
  - added the full GNU AGPL-3.0 text to `LICENSE`
- Contributor License Agreement
  - added `CLA.md`
- GitHub pull request template
  - added a PR template with an explicit CLA acknowledgement checkbox

#### Changed

- README public metadata
  - declared the repository license as AGPL-3.0
  - added a CLA reference
- Rust package metadata
  - added `license = "AGPL-3.0-or-later"` across public workspace `Cargo.toml` files
- CONTRIBUTING guide
  - documented the CLA requirement
  - fixed the source-header policy around repository-level licensing plus optional SPDX short headers

### Private AI Layer Repository Boundary Cleanup

The repository boundary was tightened by removing `fission-ai` from the public workspace and Git tracking. The goal was to keep the core decompiler and analysis engine open while keeping future AI product/API orchestration layers outside the public repository scope.

#### Changed

- public workspace scope
  - removed `crates/fission-ai` from the workspace members
  - removed the `fission-ai` dependency and re-export from `fission-analysis`
- public Git tracking scope
  - added `crates/fission-ai/` to `.gitignore`
  - removed `crates/fission-ai/*` from Git tracking so it would no longer be published on GitHub

#### Validation

- `cargo build -p fission-analysis --features native_decomp`

### v75-v78 - Preview-First Retirement Prep + Type Absorption Expansion + ARM64 Detection Scaffolding

This span focused on three themes:

1. making preview-first the real product policy while shrinking `legacy` toward compat/fallback only,
2. expanding Rust-side type absorption for hard x64 and x86 cases,
3. laying the first PE/Windows ARM64 detection groundwork and widening cross-image propagation to `ida76sp1/plugins`.

#### Added

- legacy-needed benchmark/report artifacts
  - separate binary/global summaries for successful functions that still are not preview-direct
- x86 decimal index field-replacement regression coverage
  - validates decimal surfaces such as `register[24]` as field-replacement candidates
- cross-image propagation scope coverage for `plugins/`
  - smoke validation that `ida76sp1/plugins/hexrays.dll` is actually included
- Windows ARM64 spike note
  - recorded current blockers and bring-up checklist in `docs/benchmark/windows_arm64_spike.md`
- synthetic PE ARM64 loader test
  - validated `IMAGE_FILE_MACHINE_ARM64 -> AARCH64:LE:64:v8A`

#### Changed

- preview-first retirement prep
  - removed `legacy` from normal GUI workflow
  - kept CLI `--engine legacy` as a hidden compatibility mode
  - fixed fallback taxonomy around `preview_timeout`, `preview_unsupported`, `native_pcode_failure`, `legacy_fallback`, and `assembly_fallback`
- x64/x86 shared type absorption
  - kept metadata-first inferred-type merge
  - extended line-local pointer-offset alias substitution
  - widened `register[offset]` field replacement candidates to decimal as well as hex surfaces
- x86 hard-case surfacing
  - prevented decimal and stack-like index surfaces from dropping out of common postprocess on cases such as `WinMergeU.exe 0x407050` and `EverPlanet_KR.exe 0xa918d0`
- cross-image propagation phase 2, step 1
  - expanded sibling scanning to include DLLs under `plugins/`
  - widened weak-name detection to include `sub_`, `FUN_`, `func_`, `Ordinal_`, `j_`, `thunk_`, `nullsub_`, `loc_`, and `LAB_`
- Windows PE loader / CLI architecture surfacing
  - recognized PE ARM64 as `AARCH64:LE:64:v8A`
  - surfaced ARM64 as `arm64` / `ARM64 (64-bit)` instead of `x86_64`

#### Improved

- `putty.exe 0x140006380`
  - reduced leftover `unique0x... = register + offset` alias residue
  - increased `register[offset]` surfacing
- x86 hard-case observability
  - hard-case summaries now expose `unique_surface_count`, `field_access_count`, and `offset_index_count`
- legacy deprecation observability
  - reports now show which functions still depend on legacy/native fallback outcomes
- `ida76sp1`
  - propagation scope now includes `plugins/hexrays.dll`, making sibling-based auto rename practical across the plugin layout

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-static --features native_decomp field_offset_replacement -- --nocapture`
- `cargo test -p fission-loader test_parse_synthetic_pe -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build -p fission-tauri`
- `python3 -m py_compile`
  - `scripts/test/batch_benchmark/grand_finale_support/metrics.py`
  - `scripts/test/batch_benchmark/grand_finale_support/summary.py`
  - `scripts/test/batch_benchmark/grand_finale_support/report_md.py`

#### Notes

- On `EverPlanet_KR.exe 0xa918d0` and `WinMergeU.exe 0x407050`, `unique0x` residue was already near zero in legacy output; the real goal in this round was improving x86 `[]` / field-style surfacing.
- The Windows ARM64 spike is still only a bring-up track. There is no real Windows ARM64 PE sample in the repository yet, so fixed-seed baseline JSON/Markdown artifacts were deferred.

### v69-v74 - x64 Timeout Closure + Portable Multi-DLL Symbol Propagation

This span closed two major threads:

1. reducing the last branch/readability residue in giant x86/x64 functions while turning long-running preview cases into explicit fallback outcomes through subprocess isolation,
2. introducing the first cross-image symbol propagation pass for portable multi-DLL layouts using only sibling EXE/DLL import-export-thunk relationships.

#### Added

- stronger x86 branch-condition recovery
  - reconstructs exact `TEST` / `CMP` boolean trees directly in terminator lowering
- preview render subprocess worker
  - runs heavy preview rendering in a separate worker process
  - kills and falls back explicitly on timeout
- `ida76sp1` fixed-seed watchlist artifacts
  - `ida64.exe`
  - `idat64.exe`
  - `ida64.dll`
  - `ida.dll`
  - `plugins/hexrays.dll`
- Tauri cross-image propagation service
  - same-folder sibling `*.exe` / `*.dll` scan
  - import/export/thunk-based rename candidate resolution
  - in-memory rename provenance tracking

#### Changed

- non-float scalar self-equality / boolean simplification
  - `x == x -> true`
  - `x != x -> false`
  - removed residual expressions such as `if (!reg && reg == reg)`
- stronger dead flag-intrinsic cleanup
  - removes unused `__carry/__scarry/__sborrow` assignments
- converted two `ida76sp1` watchlist timeouts to explicit subprocess-isolated `preview_timeout` fallback
  - `ida64.dll 0x101fa177`
  - `hexrays.dll 0x17088330`
- fixed `hexrays.dll 0x170057f0` to end in a non-empty assembly fallback instead of ambiguous empty preview output
- after `open_file`, scans the current binary parent folder and merges sibling import/export/thunk-based rename candidates directly into `renamed_functions`
- ensured manual/project-loaded renames always outrank auto-propagated renames

#### Improved

- `EverPlanet_KR.exe 0xa918d0`
  - removed `if (!reg && reg == reg)` and `reg == reg` residue
  - reduced code length further
- `ida76sp1` baseline closure
  - `ida64.exe`: direct preview `4/5`
  - `idat64.exe`: direct preview `4/5`
  - `ida64.dll`: direct preview `4/5`, timeout case converted to explicit fallback
  - `ida.dll`: direct preview `4/5`
  - `hexrays.dll`: direct preview `3/5`, remaining cases explicit legacy/assembly fallback
- `ida64.dll 0x101fa177` and `hexrays.dll 0x17088330` no longer remain as 20-second hangs
- sibling scan produced non-zero propagated renames on real `ida76sp1/ida64.dll` smoke runs
- existing regression targets held
  - `putty.exe 0x140006260`: `LPRECT param_2`, `RECT local_3c`, `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: direct preview retained
  - `WinMergeU.exe` x86 and `EverPlanet_KR.exe` x86 direct preview retained

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-static --features native_decomp preview_worker_ -- --nocapture`
- `cargo test -p fission-tauri cross_image -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --bin fission_preview_worker --features native_decomp`
- `cargo build -p fission-tauri`
- compare/watchlist reruns across `ida76sp1` watchlist binaries and retained regression samples

### v63-v68 - C++ Corpus Expansion + x86 Preview Readability Uplift

This span expanded the real-world validation set and then used the new coverage to fix x86-specific preview bottlenecks and readability problems.

#### Added

- new Windows sample corpus coverage
  - `WinMergeU.exe` x64 / x86
  - `SumatraPDF-3.5.2-32.exe`
  - `cmake.exe`
  - `EverPlanet_KR.exe`
- x86 `CallInd` trap-like target recovery
  - surfaces `INT3` producers as opaque callees like `((code *)swi(3))`
- additional x86 readability tests
  - register naming bootstrap
  - large-body cheap slot surfacing
  - dead local / dead flag-intrinsic cleanup
- EverPlanet x86 fixed-seed stress corpus

#### Changed

- added budgeted fallback to x86 `try_lower_while()`
- restored real x86 register names (`eax`, `ecx`, `edx`, etc.)
- allowed cheap slot surfacing to continue in large HIR bodies
- removed write-only non-temp local clobber
- added x86 flag-temp canonicalization and stronger dead intrinsic cleanup

#### Improved

- `SumatraPDF-3.5.2-32.exe`: all 5 fixed seeds `mlil_preview`, fallback 0
- `WinMergeU.exe` x86: all 5 fixed seeds `mlil_preview`, fallback 0
- `EverPlanet_KR.exe`: all 5 fixed seeds `mlil_preview`, fallback 0, while legacy timed out on the selected seeds
- major readability improvement on `EverPlanet_KR.exe 0xa918d0`
  - residue score `207 -> 169 -> 11`
  - temp surface count `182 -> 144 -> 11`
  - code length `18435 -> 15459 -> 9462`
  - `__carry/__scarry/__sborrow` `68/68/19 -> 33/68/18 -> 0/0/0`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- reran compare/fixed-seed coverage for `SumatraPDF`, `WinMerge`, `EverPlanet`, `putty`, and `everything`

### v62 - Warning Cleanup + Fixed-Seed Benchmark Closure

This round removed the last dead warnings after the second major `nir` refactor and re-closed fixed-seed compare results for `putty`, `everything`, `notepad++`, and `7zr`.

#### Changed

- removed two dead warnings
  - `MlilPreviewOptions::is_pe_x64()`
  - unused `VN_SIZE` inside `PcodeFunction::to_flat_bytes()`

#### Improved

- `cargo test` / `cargo build --release` passed cleanly without additional warnings
- reconfirmed fixed-seed compare closure
  - `putty.exe 0x140006260`: `mlil_preview`, fallback 0, preserved `LPRECT param_2` / `RECT local_3c` / `*param_2 = local_3c;`
  - `everything.exe 0x140183590`: `mlil_preview`, fallback 0
  - `7zr.exe` selected seeds: all `mlil_preview`, fallback 0
  - `notepad++.exe` selected seeds: all `mlil_preview`, fallback 0

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v59-v61 - x86 Conditional Structuring Stabilization + Second `nir` Refactor

This span stabilized long-running x86 `try_lower_if()` paths on heavy `7zr.exe` seeds and then reorganized the growing `nir` implementation into a more maintainable module tree.

#### Added

- x86-only conditional structuring budget/cache
- join/follow-gated plain `if` candidate pre-checks
- second-stage `nir` module tree split under `builder/`, `structuring/conditionals/`, and `tests/`

#### Changed

- made x86 pathological CFG handling more conservative
- prioritized short-circuit chains before plain `if` recovery when they close on the same join
- split `builder/mod.rs` and promoted `structuring/conditionals.rs` into a directory module

#### Improved

- `7zr.exe 0x401804` and `0x402778` no longer time out due to long-running `try_lower_if()`
- retained direct preview on `putty.exe 0x140006260` and `everything.exe 0x140183590`

#### Validation

- `cargo fmt --all`
- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`

### v36-v58 - `putty` Aggregate Copy Closure + x86 Timeout Diagnosis

This stretch had two goals:

1. remove the last aggregate transit temp from `putty.exe 0x140006260` until preview reached `RECT local_3c; *param_2 = local_3c;`,
2. determine whether heavy x86 `7zr.exe` timeouts came from Rust NIR or native extraction.

#### Added

- dead temp cleanup for aggregate transit temps
- prepare/native/preview diagnostic logging
- finer structuring-phase diagnostic logging

#### Changed

- recovered `LPRECT param_2`, `RECT local_3c`, and `*param_2 = local_3c;` for `putty.exe 0x140006260`
- removed dead aggregate transit temps like `xVar32`
- instrumented native prepare, preview p-code extraction, and Rust structuring boundaries

#### Improved

- closed the x64 aggregate-copy/type-surface target on `putty.exe 0x140006260`
- narrowed heavy x86 `7zr.exe` timeouts to Rust `structuring`, especially `try_lower_if()`

#### Validation

- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- regression/diagnostic reruns for `putty`, `everything`, `notepad++`, and `7zr`

---

## 2026-03-14

### v26-v35 - Preview Coverage Recovery + `putty` Type-Surface Recovery

The goals in this span were:

1. restore direct `mlil-preview` coverage on real large functions,
2. bring the type surface back up on `putty.exe 0x140006260` after direct preview had been recovered.

#### Added

- more detailed preview/native coverage diagnostics
- x86 preview bootstrap regression guard
- stack-slot naming recovery for direct preview
- stronger indirect import / Win64 argument recovery
- site-sensitive lowering infrastructure inside the builder

#### Changed

- reduced p-code extraction work in giant dispatcher cases
- added linear fallback caching and fast paths to Rust NIR structuring
- relaxed builder lowering carefully to recover `putty.exe 0x140001160`
- extended wide aggregate copy recovery with lane matching and prior-def lowering
- improved pointer-deref printing quality

#### Improved

- `putty.exe 0x140001160`: direct preview recovered
- `everything.exe 0x140183590`: direct preview retained
- `7zr.exe 0x401000`: direct preview retained
- `putty.exe 0x140006260`: recovered `LPRECT param_2`, `GetClientRect(...)`, `local_3c`, and whole-object assignment path progression

#### Validation

- `cargo test -p fission-pcode --lib nir::tests -- --nocapture`
- `cargo test -p fission-pcode --lib nir::tests::type_hints -- --nocapture`
- `cargo build -p fission-cli --features native_decomp`
- `cargo build --release -p fission-cli --bin fission_cli --features native_decomp`
- `cargo check -p fission-tauri`

### v25 - NIR Module Tree Refactor

This round was about maintainability rather than new algorithms. The growing `nir` core was split into `builder / normalize / structuring / tests` subsystems to reduce future edit and regression costs.

#### Changed

- reorganized `crates/fission-pcode/src/nir/` into:
  - `builder/`
  - `normalize/`
  - `structuring/`
  - `tests/`
- narrowed `nir/mod.rs` to entrypoint/wiring responsibilities
- split normalize responsibilities into arithmetic/boolean normalization, cleanup, slot/table surfacing, and bitstream helpers
- split structuring responsibilities into conditionals, loops, switch, and linear fallback

### v24 - Preview Coverage Recovery First, x64 + x86 in Parallel

This round focused on restoring direct preview output on real x64 functions again while also bringing up the first real x86 preview bootstrap path.

#### Added

- finer preview unsupported-reason diagnostics
- PE x86 preview bootstrap path

#### Changed

- relaxed branch-target recovery to improve x64 large-function direct preview coverage
- made region builder more aggressive about trivial forwarding/cleanup/tail-return absorption
- canonicalized identical-input `MULTIEQUAL`
- preserved slot-family / bitstream helper / loop-body compaction while fixing the application order around coverage-first goals

#### Improved

- `putty.exe 0x140006260`: direct preview recovered again
- `everything.exe 0x140183590`: direct preview recovered again
- at least one fixed-seed `7zr.exe` function reached direct preview, confirming the first real x86 bootstrap success

### v16 - Preview Type Surface Quality + Direct `putty 0x140006260` Output

This round pushed preview beyond “structured pseudocode exists” toward more natural known-signature type surfaces. The main target was direct preview of `putty.exe 0x140006260` with `LPRECT`, `RECT`, and whole-object assignment style output.

#### Added / Changed

- known-signature-based type surface context in preview
- preview binding type hints
- stronger p-code JSON opcode alias parsing
- layout-based fallthrough analysis for preview CFG recovery
- direct preview understanding of `goto(target, cond)` style real p-code branches
- containment so preview optimizer panic would not collapse the whole path

#### Improved

- `putty.exe 0x140006260 --engine mlil-preview` could directly surface:
  - `LPRECT param_2`
  - `RECT local_3c`
  - whole-object assignment style output

### v15 - Preview Quality Uplift + Low-Risk Function Promotion

The target here was not higher legacy success, but making `mlil-preview` the better path on lower-risk functions.

#### Added / Changed

- canonical `switch` reconstruction in preview
- preview-only surface cleanup
- centralized `engine_used` source of truth in `fission-static`
- widened `auto` preview eligibility on stable multi-block functions

#### Notes

- Preview coverage and structuring improved significantly, but preview type surface quality still lagged legacy on representative cases such as `putty.exe 0x140006260`.

### v14 - Legacy `type` Failure Removal + 90/90 Closure

This round focused on removing the remaining legacy `type` failures and restoring benchmark closure without counting `mlil-preview` rescue as equivalent success.

#### Improved

- removed the last known legacy `type` failures for that benchmark round
- retained preview direct output on representative targets

### v13 - MLIL Preview Structuring / Readability Uplift

This round strengthened the preview path around:

- canonical multi-block `if`, `if/else`, `while`, and `do-while`
- short-circuit boolean chains
- `PIECE` / `SUBPIECE` recombination
- cast-density reduction and lower-level residue cleanup

### v10-v12 - Experimental Fission MLIL/NIR Path Integrated Into Product Surfaces

This was the point where `mlil-preview` stopped being a CLI-only experiment and became a real engine mode exposed in both CLI and Tauri.

#### Added

- `legacy | mlil-preview | auto` engine modes
- engine selector in the Tauri decompiler options UI
- engine/fallback badges in the decompile view
- Rust-owned preview NIR/HIR + printer path

#### Changed

- adopted lightweight p-code extraction before the full native action pipeline when possible
- fixed wrapped negative constant parsing
- expanded multi-block canonical `if/if-else` lowering
- added conservative `MULTIEQUAL`, `PIECE`, and `SUBPIECE` lowering

#### Improved

- preview generated direct output across real smoke samples instead of remaining an isolated prototype path

---

## Historical Milestones (Late 2025 – Early 2026)

The repository history before the current architecture convergence includes several major milestones. The detailed Korean notes remain available in [`CHANGELOG.ko.md`](./CHANGELOG.ko.md); the summaries below capture the public-facing highlights.

### Multithreaded Performance Breakthrough (157s -> 10s)

- introduced global Sleigh, GDT, and data-section scan caches
- added a core-level fail-fast timeout tripwire for monster functions
- reduced large batch decompilation wall-clock time dramatically on `putty.exe`

### Decompiler Performance + Success-Rate Uplift

- improved one-shot CLI throughput and overall decompilation success rate
- instrumented postprocess timing and removed major bottlenecks
- fixed recursive decompilation and duplicate-variable-piece failure classes
- built the first fair batch benchmark runner against PyGhidra baselines

### Security Policy / CI Gate Hardening

- added `docs/build/SECURITY_ADVISORIES.md`
- restored security checks as a CI quality gate
- documented advisory baselines and review policy

### Stabilization / Portability / Phase 2–4 Refactors

- removed panic-prone `unwrap/expect` paths across loader/analysis/ffi/tauri code
- converted pass pipelines toward `Cow<str>`-based no-op fast paths
- removed hardcoded local build paths in favor of environment-based discovery

### Postprocess Modularization

- split the large `postprocess.rs` implementation into focused modules
- separated naming, structure, arithmetic, and shared condition utilities
- added dedicated postprocess module documentation and tests

### Major Decompiler Quality Round + v4 Benchmark System

- fixed four large-quality bugs in postprocessing and structure handling
- introduced the v4 benchmark system with multi-platform suites
- significantly improved benchmark scores across ARM64, x64, Linux, and Windows

### x86 / MinGW / Type Propagation Expansion

- added MinGW-focused type propagation improvements
- brought in x86 benchmark suites and comparison binaries
- improved call propagation, loop conversion, and x86 normalization quality

### P-code Optimizer / Constant Substitution / RTTI / Listing / CFG Work

- introduced the early p-code optimization pipeline
- added context-aware constant substitution
- expanded listing, RTTI recovery, CFG analysis, and disassembly support

### Tauri Migration and Desktop Product Surface

- completed the move from the older egui UI to Tauri 2.x + React / TypeScript
- added large portions of the desktop workflow:
  - function navigation
  - assembly/decompile views
  - CFG views
  - project save/load
  - debugger surfaces
  - timeline/TTD experiments
- removed the legacy `fission-ui` egui codebase after the migration

### Analysis Pipeline / Data-Section Scan Consolidation

- unified batch analysis context and analysis-pass entrypoints
- consolidated data-symbol scanning and registration
- expanded FFI surface for function and prototype configuration

### Loader / Function Discovery

- added linear-sweep function discovery for stripped code
- improved function recovery on x86 and x64 binaries

### Early Core Capabilities Established

By this point Fission had already accumulated the foundations that still shape the current system:

- PE / ELF / Mach-O loading
- static analysis and disassembly
- Ghidra native decompiler integration
- Rust-side orchestration
- benchmarking infrastructure
- desktop UI foundations
- the first steps toward a Fission-owned decompiler core
