# Structuring Rule Budget And Fail-Closed Recovery

## 1. Baseline Row Anchor

- Binary: external source-semantic benchmark crypto variants
- Function: `crc32`, with `rc4_init` and `rc4_crypt` as comparable shapes
- Address: recorded by the external benchmark; not used by production logic
- Corpus row or benchmark command: local Fission Docker benchmark plus focused
  `fission_cli decomp --benchmark --timeout-ms 30000`
- Current output summary: initial HIR structuring does not return before the
  preview timeout for one optimized variant
- Semantic cases passed / total: not reached for the timed-out row
- Failure category: preview timeout
- Relevant observations: the focused p-code stage has six blocks and seven
  edges. Approximately 98 of 99 seconds are spent in initial structuring;
  normalization and rendering remain below 200 ms combined.

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
[DIAG] build_hir multiblock_start: blocks=6 ops=495
[DIAG] structuring start: blocks=6 edges=7 force_linear=false
stage build_ms=98327 structuring_ms=98143 normalize_ms=175 render_ms=2
```

The existing region proof ceiling is checked between collapse rules. A single
rule invocation can therefore run for substantially longer than the ceiling and
prevent the normal unstructured fallback from taking ownership.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Every speculative structuring reducer and recursive subgraph lowering must
cooperate with the shared function-level structuring deadline. Once the deadline
expires, the reducer returns no candidate and leaves the CFG to the existing
fail-closed fallback. The decision depends only on elapsed work and CFG shape.
```

ISA-agnostic check:

- [x] No calling-convention or ISA enum gate is introduced.
- [x] No ISA-specific data is introduced.
- [x] Synthetic tests use CFG shape only.

Comparable coverage:

- Similar shape 1: a natural loop with a nested conditional body
- Similar shape 2: a loop with multiple body blocks and an external exit
- Synthetic invariant test: an expired shared deadline rejects speculative
  subgraph lowering without changing normal-budget output

## 4. Risk And Ownership Check

- Existing owner: `nir::structuring` collapse driver and reducer-local budgets
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: the shared `structuring_start` already
  defines the deadline; missing checkpoints are an owner-local enforcement gap.
- New pass/helper: none planned
- Possible interaction: an expensive speculative structured candidate may be
  skipped after budget expiry, preserving semantics through labeled/goto fallback.
- New owner dependency: none
- Telemetry impact: diagnostic-only per-rule elapsed logging; no schema change
- Known cases that must not change: fast reducible loops and conditionals under
  the budget must retain their current structured output.

## 5. Validation Matrix

- [x] Targeted invariant test
  - Command: `cargo nextest run -p fission-pcode -E 'test(structuring)'`
  - Result: 310 focused structuring/materialize tests passed.
- [x] Crate-level gate
  - Command: `cargo nextest run -p fission-pcode`
  - Result: 1168 passed, 9 skipped
- [x] Focused benchmark row
  - Command: local Docker focused crypto benchmark and 30-second CLI repro
  - Result: no preview timeout. Focused wall times changed from approximately
    9.9s to 2.74s (`rc4_init`), 10.7s to 4.21s (`rc4_crypt`), and 99s to
    10.14s (`crc32`). The external row moved from adapter timeout to 1/6
    executed cases for the motivating optimized variant.
- [x] Smoke or automation sample
  - Command: external dev corpus with local Fission bundle
  - Result: focused 24-row crypto sample preserved the existing fully passing
    `crc32 clang-m32 -O2` row. `rc4_crypt clang -O2` improved from a 3/4
    timeout result to 4/4. No adapter errors remained in the focused rows.
- [x] Optional related checks
  - Command: `cargo check -p fission-pcode`
  - Result: clean build
- [x] Boundary audit
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`
  - Result: baseline unchanged at 52 findings (5 violations, 47 migration debt).
    Fast benchmark-smell scan remained at the existing seven warnings.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No external model or benchmark-identified review prompt was used
- Information used locally: owner timing and structural CFG size
- Redaction: no benchmark identity is introduced into production code
- Ghidra guidance: no output-style comparison is used
- Unseen or synthetic validation evidence: synthetic structuring coverage plus
  the external smoke corpus will be recorded after implementation.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- No semantic improvement is claimed from benchmark-only edits:
  - [x] Confirmed
- No duplicate pass/helper is introduced:
  - [x] Confirmed
