# Natural While-Head Recovery

## 1. Baseline Row Anchor

- Binary: `corpus/dev/binaries/control_flow_gcc-m32_O0.exe`
- Function: `count_bits`
- Address: `0x4015b0`
- Corpus row or benchmark command: focused local Fission row through the SHA-pinned benchmark bundle
- Current output summary: the preheader is emitted as an `if` followed by a label, the loop back-edge disappears, and the exit return reads an uninitialized binding instead of the accumulator stack slot loaded into the primary return register.
- Semantic cases passed / total: 1 / 6
- Failure category: `runtime_error`
- Relevant observations: raw p-code has four blocks with `preheader -> head`, `body -> head`, and `head -> body | exit`; structuring telemetry reports an SCC of size two but zero while-subgraph lowerings.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
The raw CFG is a reducible natural loop:

  preheader(0) -> head(2)
  body(1)      -> head(2)
  head(2)      -> body(1), exit(3)

The header dominates the body, the body-to-header edge is a natural back-edge,
and the only edge leaving the loop body targets exit(3). try_lower_while rejects
the condition prefix because p-code-derived `__sborrow` and `__popcount`
expressions are classified like side-effecting user calls. Orphan repair then
localizes the remaining label and loses the loop continuation.
```

## 3. Generality / Invariant Proof

Generalized rule:

```text
A reducible single-header SCC with exactly one natural back-edge, one external
preheader, and one conditional header exit is a while-loop candidate regardless
of physical block index or address order. Predicate setup may contain pure
p-code semantic intrinsics whose arguments are recursively side-effect-free;
ordinary calls and memory writes remain rejection conditions.
```

Comparable coverage:

- Similar shape 1: header-first layout with a multi-block body, already covered by structuring tests.
- Similar shape 2: preheader/body/header/exit layout where the body falls through to the header.
- Synthetic invariant test: a four-block out-of-layout-order natural loop with pure predicate intrinsics must emit `while`, preserve the body store and exit return, and contain no header goto; a regular call remains side-effecting. A stack-local load copied into the primary return register must return the loaded slot, not a new register binding.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `LoopBody::identify_loops`, `try_lower_while`, and `lower_loop_body_subgraph`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [x] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the CFG fact cache already proves the loop shape, while shared expression support owns the pure p-code intrinsic fact consumed by structuring and normalize; no new pass is required.
- If adding a new pass/helper/metric: none planned.
- Possible interaction: condition-prefix handling, linear loop-body lowering, return-register def-use recovery, and orphan-goto cleanup.
- New or changed owner-to-owner dependency:
  - [x] None
  - [ ] Existing migration debt only
  - [ ] New dependency justified below
- Telemetry impact: reuse existing while/subgraph counters; rejection diagnostics may be refined without a schema change.
- Known cases that must not change: irreducible loops, side-entry loops, multi-exit loops without a common continuation, and condition blocks with calls or memory side effects.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-pcode natural_while_with_preheader_after_body_layout --no-fail-fast`
  - Result: structured `while`, preserved body assignment and `return local_4`, no header goto. Pure-intrinsic and regular-call classification plus x86-32 return-register tests also pass.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1,136 passed, 9 skipped.
- [x] Focused benchmark row:
  - Command: rebuild the local Linux CLI and rerun the focused x86-32 row with SHA provenance.
  - Result: x86-32 O0 moved from 1/6 to 5/6 and x86-32 O2 from 0/6 to 2/6; x64 O0/O2 remain 6/6. The remaining O0 timeout is signedness recovery and the remaining O2 failures are loop-carried update/accumulator recovery, outside this slice.
- [x] Smoke or automation sample:
  - Command: `python runner/runner.py --corpus dev --decompilers fission --limit 10 --variant-limit 1 --output results/local_structuring_smoke.json`
  - Result: all 10 GCC O0 rows passed every semantic case, including checksum, crc32, rc4_init, rc4_crypt, and the control-flow rows.
- [x] Optional related checks:
  - Command: `cargo check -p fission-pcode && cargo check -p fission-decompiler`
  - Result: both crates compile.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: no new owner-boundary violation; the scan still reports 5 existing violations and 47 migration-debt edges.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external model; local implementation follows the repository skill and ADR gate.
- Information exposed in the AI prompt:
  - [x] Structural failure pattern only for implementation reasoning
  - [x] Owner evidence only
  - [x] Invariant candidates only
  - [x] Validation matrix only
- Redaction confirmed for external prompts:
  - [x] Function names removed
  - [x] Addresses removed
  - [x] Binary paths removed
  - [x] Corpus row ids removed
  - [x] Compiler tuple / row-identifying labels removed
- Ghidra guidance confirmed:
  - [x] Correctness/reference use only; no output-style mimicry request
- Unseen or synthetic validation evidence:
  - Patch validation pool command/result: pending focused validation.
  - Synthetic invariant test command/result: three targeted tests passed for natural-loop intrinsic acceptance, regular-call rejection, and x86-32 return-register recovery.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed
