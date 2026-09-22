# Decompiler Change Proposal: Loop-Carried Matrix Store Bindings

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/memory_layouts_gcc_O2.exe`
- Function: `matrix_multiply`
- Address: `0x140001570`
- Corpus row or benchmark command:
  `cd /Users/sjkim1127/fission-benchmark && .venv/bin/python runner/runner.py --corpus dev --function matrix_multiply --decompilers fission,ghidra --run-mode local --no-resume --output results/issue77_baseline_matrix_multiply.json`
- Current output summary: raw p-code contains the correct indexed store and the correct XMM1 low-lane value, but NIR/HIR emits `*rbx = xmm1_da` and loses the `r9 * 4` output index.
- Semantic cases passed / total: GCC O2 `0/5`; across the nine focused Fission rows, `6/45` cases passed. Clang O0 is the only complete row (`5/5`).
- Failure category: GCC O2 `runtime_error` with an emitted `0/5` harness result; related variants include compile and runtime failures.
- Relevant benchmark/static/readability observations: the baseline focused rows were `gcc -O0 1/5`, `gcc -O1 0/5`, `gcc -O2 0/5`, `gcc -O3 0/5`, `gcc -Os 0/5`, `gcc-m32 -O0 0/5`, `gcc-m32 -O2 0/5`, `clang -O0 5/5`, and `clang -O2 0/5`. The direct HIR/ NIR store is semantically wrong before rendering.

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
At 0x1400015d8 raw p-code is:
  IntMult unique:8 <- R9:8, const:8(4)
  IntAdd  unique:8 <- RBX:8, unique:8
  Copy    unique:4 <- XMM1:4
  Store   space=3, unique_address, unique_value

The address and value operands are therefore present in the lifted semantics.
PreviewBuilder materialization instead resolves the block-entry R9 read to the
preheader zero seed and resolves the XMM1:4 read to `xmm1_da`, while the
loop-carried definitions are given different or no shared carriers. NIR already
contains `*rbx = xmm1_da`; the printer is not the first owner of the error.
```

The earliest semantic disagreement is the loop-carried binding decision for the
register values consumed by the store's address/value operands, before the
`Store` lowering itself. The loop proof accepts the R9 backedge update, but the
carrier lookup is order-dependent: the latch is inspected before the internal
zero seed has a materialized name, while the later fallback treats the ABI-capable
R9 slot as a possible formal. XMM1 has the dual width problem: its entry seed is
16 bytes and its loop update is the low 4-byte lane, so the narrow read falls
back to the lane hardware name instead of the definition-scoped accumulator.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a proven loop-carried register update, all same-storage reads and the
backedge definition must share one definition-scoped carrier. Select that
carrier from CFG/SSA/alias evidence, independent of materialization order. If
the entry seed is an internal definition, it wins over the ABI formal even when
the storage is ABI-capable. If the update is a narrower view of a wider seed,
reuse the wider seed's carrier through the register-alias projection contract;
do not invent an unrelated lane variable.
```

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is based on loop proof, def-use, ABI-slot ownership, and register alias facts rather than a binary/function/address guard.
- [x] ISA-specific storage facts remain in the register namer/calling-convention model.
- [x] Synthetic tests describe the CFG and alias shape without the corpus row.

Comparable coverage:

- Similar shape 1: `loop_pointer_scan_load_and_add_share_cursor_binding` already requires a load and stride update to share one cursor.
- Similar shape 2: `loop_carried_byte_accumulator_with_movzx_preserves_add` covers a narrow accumulator update, but not a wider SIMD seed.
- Synthetic invariant test: internal zero-seeded ABI-capable register used as a loop index, plus a wider register seed followed by a narrower floating-point lane update and store.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `materialize/loop_carried` and its `binding.rs`/`phi_latch.rs` helpers.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [x] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the loop-carried proof, scalar SSA phi operands, materialized-name table, and register alias predicates are already co-located in this owner. No new pass or cross-layer dependency is required.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass is planned; the change extends the existing carrier selection helper.
- Possible interaction with existing normalize/structuring/materialize passes: preserving a carrier can increase explicit assignments, so existing primary-return, transformed-seed, and global-name rejection rules must remain ahead of the fallback. Wide SIMD aliases must be projected as a lane when needed rather than changing the carrier's aggregate type.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: none.
- Known cases that must not change: entry-owned formal parameters must remain formal bindings; transformed dominating seeds must not be replaced by their original parameters; same-loop wide temporaries must not hijack unrelated narrow updates; global-address names must not become loop cursors.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode --filter-expr 'test(loop_carried_update_reuses_post_loop_join_binding) | test(win64_ecx_zext_then_shr_self_loop_uses_param_1) | test(loop_carried_update_reserves_internal_abi_seed_before_formal_fallback) | test(loop_carried_narrow_simd_update_reuses_wide_seed_carrier)'`
  - Result: `4 tests run: 4 passed, 1108 skipped`.
  - The new internal-seed and wide-seed/lane tests fail before the fix and pass after it; the existing post-loop-join and formal-parameter carrier tests remain green.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: `1111 tests run: 1108 passed, 3 failed, 1 skipped`; the three failures are the pre-existing `diamond_join_lowers_copy_through_join_read_as_select`, `movzx_after_byte_add_zero_extends_unsigned`, and `x64_byte_add_movzx_does_not_double_add_load` tests. No loop-carried regression remains.
- [x] Focused benchmark row:
  - Command: `FISSION_ENDPOINT=http://localhost:8007 .venv/bin/python runner/runner.py --corpus dev --function matrix_multiply --decompilers fission,ghidra --run-mode local --no-resume --output results/issue77_after_4f1316eea_matrix_multiply.json`.
  - After provenance: local Fission `4f1316eea`, source fingerprint `ab1af7bca0d7e22a114b57d1891b854ded53050cdf466cbf287fd858eaa225f6`.
  - Result: the nine focused rows increased from `6/45` to `16/45`. GCC O1 improved `0/5 → 5/5`, and GCC O2 improved `0/5 → 5/5`; GCC O0, GCC O3, GCC Os, GCC-m32 O0/O2, Clang O0, and Clang O2 were unchanged. The after run took 160.5 seconds versus 168.6 seconds for baseline.
  - The GCC O2 output now contains `rbx[iVar8] = xmm1_qa;` instead of `*rbx = xmm1_da;`, preserving both the reaching accumulator and the indexed destination. This is a measured semantic improvement on the anchored corpus rows, not a synthetic-only claim.
- [ ] Smoke or automation sample:
  - Command: existing benchmark smoke manifest after the focused fix.
  - Expected no-regression signal: existing passing rows remain passing.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode`, `cargo build -p fission-cli --release`, `cargo fmt --all --check`, `git diff --check`.
  - Expected signal: all pass.
- [ ] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: not run; this fix was validated against the anchored nine-row real-binary slice and the focused Rust regressions.
  - Synthetic invariant test command/result: the two new carrier tests and two existing carrier regressions passed after the fix (`4 passed, 1108 skipped`).

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the existing loop-carried materialization owner is extended.
