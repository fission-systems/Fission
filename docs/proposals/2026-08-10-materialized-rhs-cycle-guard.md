# Materialized RHS Cycle Guard

## 1. Baseline Row Anchor

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/scale/binaries/O2-noinline/libbsd/libbsd.so.0.11.7`
- Function: `gotdata`
- Address: `0xed10`
- Corpus row or benchmark command: `target/release/fission_cli decomp <binary> --addr 0xed10 --profile nir --json`
- Current output summary: process aborts after about four seconds with `thread 'fission-rust-decomp-0xed10' has overflowed its stack`; no decompilation result is emitted.
- Semantic cases passed / total: N/A; the target function produces no candidate because the process aborts.
- Failure category: builder/materialize non-termination.
- Relevant benchmark/static/readability observations: raw p-code succeeds with 5 blocks, 5 edges, and 94 operations. `speed`, `nir`, `balanced`, and `quality` all abort. Increasing `FISSION_RUST_DECOMP_STACK_MB` to 256 does not change the outcome.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [ ] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
LLDB repeats this cycle until stack exhaustion:

try_lower_materialized_output_rhs
  -> rewrite_block_entry_accumulator_rhs_with_live_gpr
  -> block_entry_partial_gpr_incoming_expr
  -> partial_gpr_incoming_expr_from_pred_path
  -> partial_gpr_incoming_expr_for_pred_def
  -> try_lower_materialized_output_rhs

The self/back-edge predecessor is admitted at depth zero. Lowering its
unmaterialized definition re-enters the same materialized-output def-site.
This path does not pass through lower_varnode_inner's existing
varnode_redirect_depth guard.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
Materialized-output RHS recovery is a def-site graph traversal. A def-site
already active on the current RHS-recovery stack cannot be expanded again:
that is a cyclic definition, not a finite expression. Fail closed for only
that nested recovery attempt, allowing the caller to retain the existing
materialized/live-register expression. Remove the active marker when the
outer attempt finishes so independent later attempts remain eligible.
```

ISA-agnostic check:

- [x] Production condition is not gated only on one calling convention / ISA enum.
- [x] ISA-specific data remains in existing register/calling-convention models.
- [x] Synthetic test states the def-site/CFG cycle without a compiler tuple or function name.

Comparable coverage:

- Similar shape 1: a loop header/join whose back-edge predecessor defines a partial register from its previous value.
- Similar shape 2: mutually dependent join definitions reached while recovering predecessor expressions.
- Synthetic invariant test: same materialized-output def-site re-entered through a self predecessor fails closed and leaves the active set balanced.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `PreviewBuilder::try_lower_materialized_output_rhs` and block-entry partial-GPR predecessor recovery.
- Shared analysis/substrate candidate:
  - [ ] CFG / dominance / postdominance fact
  - [x] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient, or why a new pass/helper is needed: the cycle is created and can be rejected at the existing materialized-RHS recovery entry point. No new pass is needed.
- If adding a new pass/helper/metric, why existing shared analysis cannot express the invariant: no new pass or metric; a builder-local active def-site set represents traversal state.
- Possible interaction with existing normalize/structuring/materialize passes: only recursive re-entry of the same materialized def-site is rejected; acyclic predecessor recovery remains unchanged.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact, if any: none.
- Known cases that must not change: existing block-entry accumulator and partial-GPR recovery tests, loop-carried binding recovery, and acyclic predecessor materialization.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode -E 'test(materialized_rhs_reentry_through_self_predecessor_fails_closed)'`
  - Expected signal: completes without recursion, returns a finite RHS, and clears the active def-site set.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: all fission-pcode tests pass.
- [x] Focused benchmark row:
  - Command: `target/release/fission_cli decomp <binary> --addr 0xed10 --profile nir --json`
  - Expected row-level improvement: exit 0 with one non-empty candidate instead of process abort.
- [x] Smoke or automation sample:
  - Command: run the external DecBench regression check plus `--all --limit 173` on the same binary.
  - Expected no-regression signal: the focused function and the first 173 selected functions complete without a process-level abort.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode && cargo build -p fission-cli --release`
  - Expected signal: clean compilation.
- [ ] Boundary audit, if a new pass/helper/dependency was added: not required; no pass or dependency is added.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external or cross-model implementation review was requested.
- Information exposed in an AI prompt: N/A.
- Redaction confirmed: N/A; no external/cross-model prompt was sent.
- Ghidra guidance confirmed: N/A.
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: libbsd `--all --limit 173 --profile nir` completed with 173 JSON results and no process abort after the final combined change set.
  - Synthetic invariant test command/result: focused guard test and the full 988-test fission-pcode suite pass.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
