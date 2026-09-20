# Decompiler Change Proposal: Control-Flow-Safe PHI Copy and Constant Propagation

Date: 2026-09-20

## 1. Baseline Row Anchor

- Binary: `decbench-data/binaries/O0/openssh-portable/ssh`
- Function: `process_config_line_depth`
- Address: `0x18f1d`
- Corpus row / command: HF `noelo-lab/decbench-dataset`, `unoptimized`; local
  full run through `decbench-harness/fullrun.py` and `fullreport.py`.
- Current measured output: `decompiled=true`, source CFG nodes `590`, Fission
  CFG nodes `52`, GED `1397`, type score `0.18`.
- Failure category: large structured-CFG mismatch in a real binary. This row is
  the regression anchor for the normalization/structuring pipeline; the
  conditional-definition defect below is a separate correctness risk exposed
  by the same pre-structuring owner and must not be claimed as the cause of
  this row without a remeasurement.
- Dataset baseline: `34,106/34,406` functions decompiled; GED measurable on
  `32,196`, with `12,489` perfect (`38.8%`).

The current row is intentionally recorded as a before/after quality anchor,
not as proof that the latent wrong-code shape occurs in this function. The
synthetic regression is the direct proof of the semantic defect; the real row
checks that the conservative guard does not make the existing normalization
and structuring quality worse.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

`crates/fission-midend-normalize/src/recovery/phi_recovery.rs` collects
single-definition copies and constants by recursively entering structured
control-flow nodes. `remove_copy_assigns` / `remove_constant_assigns` and
`substitute_*_in_stmts` then walk the complete body with one global map. A
definition such as `res = 42` inside an `If` is therefore eligible when its
whole-function definition count is one, even though it does not dominate a
return after the `If`.

The smallest reproducer is:

```text
if (condition) {
    res = 42;
}
return res;
```

The old phase records `res -> 42`, deletes the conditional assignment, and
rewrites the unconditional return to `return 42`. The same issue exists for a
temporary copy `tmp = source` inside a branch.

## 3. Generality / Invariant Proof

Generalized rule:

```text
A copy or constant may be substituted only within a straight-line statement
run. The active map is cleared at labels, gotos, calls, and every nested
control-flow construct. A definition inside a branch/loop/switch is never
treated as a function-wide definition unless a separate dominance/reaching-
definition proof exists.
```

This is deliberately conservative: it declines some old substitutions rather
than inventing a second dominance engine for PreHIR. It preserves the existing
whole-function definition-count checks as an admission guard, but does not use
that count as a dominance proof.

ISA-agnostic check:

- [x] Production condition depends only on PreHIR statement/control-flow shape,
      def-use facts, and expression purity.
- [x] No ISA, calling-convention, compiler, binary, function, or address guard.
- [x] Synthetic tests express the control-flow/dataflow shape directly.

Comparable coverage:

- Similar shape 1: a constant assigned only in one `If` arm and read after it.
- Similar shape 2: a temporary copy assigned only in one `Switch` arm and read
  after the switch.
- Synthetic invariant test: conditional constant and copy definitions must
  remain in their branch and the post-dominating read must remain a variable.

## 4. Risk And Ownership Check

- Existing owner: `recovery/phi_recovery.rs::copy_propagation_pass`.
- Shared analysis candidate: def-use / reaching-definition facts, but a full
  CFG dominance proof is not currently available at this PreHIR boundary.
- Extending the existing owner is sufficient because the bug is the scope of
  its substitution map, not a printer or structuring representation issue.
- Do not replace the pass wholesale with `cleanup/run_copy_prop.rs`: that pass
  is designed for a later structured-AST phase and has a different scratch-temp
  and liveness contract. Reuse its straight-line boundary invariant only.
- Known cases that must not change: straight-line single-definition constant
  propagation, copy-chain resolution, preserved materialization names,
  loop-preservation vetoes, and address-taken locals.
- Main risk: reduced temporary elimination in bodies containing residual
  control flow, which may affect line count/GED without changing semantics.
  The focused corpus row and crate-level regression gate must measure this.

## 5. Validation Matrix

- [x] Targeted invariant tests:
  - Command: `cargo nextest run -p fission-midend-normalize phi_recovery`
  - Result: the old implementation failed both initial conditional-definition
    tests; the fixed implementation passes 9 focused `phi_recovery` tests,
    including `If`, `While`, and `Switch` boundaries.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize -p fission-pcode`
  - Result: `fission-midend-normalize` 385 passed and `fission-pcode` 1,052
    passed / 1 skipped.
- [x] Focused benchmark row:
  - Command: the same HF `unoptimized` full-run row set with caches disabled,
    including `process_config_line_depth` above.
  - Result: on the exact row, before and after HIR are byte-for-byte equal;
    GED remains `1397.0`, source CFG `590` nodes, and Fission CFG `52` nodes.
    This is a measured non-regression, not a quality-score improvement claim.
- [x] Smoke or automation sample:
  - Command: `cargo check` and the repository's release CLI build.
  - Result: workspace check and `cargo build -p fission-cli --release` pass.
- [x] Optional related checks:
  - Commands: `cargo fmt --all --check`, `git diff --check`.
  - Result: both pass.
- [x] Boundary audit:
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: 0 findings, 0 violations, 0 migration debt.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice? No.
- No external implementation prompt contains benchmark identity. The row is
  used only for local before/after validation.
- No production code will contain function, address, binary, or corpus guards.
- Synthetic validation is required in addition to the real-binary measurement.

## 7. Review Notes

- [x] Production code contains no hardcoded binary/function/address/corpus
      guards in the proposed rule.
- [x] No quality improvement claim will be made from synthetic tests alone.
- [x] The change extends the existing normalize owner and does not create a
      duplicate metric or downstream presentation workaround.

## 8. Measured Implementation Result

The production fix is in commit `3a127e69a` and the proposal/baseline is in
`34b0a5e62`. It changes only the normalize owner: replacement maps are now
scoped to straight-line runs and are cleared at control-flow, label, call, and
non-trivial memory boundaries. No ISA or corpus-specific condition was added.

The same stripped HF `unoptimized` binary and address were run against the
release CLI built at the parent commit and at the fixed commit. Both emitted
the same HIR (`10,822` bytes, identical SHA-256), and both produced the same
published-CFG score (`1397.0`). The fixed CLI's three measured runs were
`6.52–6.61s`; the parent comparison run was `7.51s`. This is only a sanity
check, not a benchmark claim, but it shows no obvious broad slowdown from the
new normalize walk.

The real-binary anchor therefore records **no score movement**. The quality
claim supported by this change is semantic risk reduction: the old code was
shown by failing regression tests to turn a conditional definition into an
unconditional value. The HF row confirms that the conservative rule does not
alter this existing difficult decompilation; a larger corpus sweep is needed
before claiming a leaderboard or aggregate-quality change.
