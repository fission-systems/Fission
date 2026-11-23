# fission-quotient Agent Guide

Scope: `crates/fission-quotient`

## Overview

Experimental WBCQ prototype (RFC #1093, stages P0/P1): computes the minimal
observable state machine of a P-code slice instead of structuring its CFG.
Nothing in the production pipeline (`fission-midend-*` -> HIR) or the CLI
depends on this crate. It is not a decompiler output surface, and nothing it
reports is a DecBench or readability claim.

## Module map

| Module | Owns |
|---|---|
| `boundary.rs` | `QuotientBoundary` (inputs, hidden context, observed result), `QuotientBudget`, `AbiModel` read from the SLEIGH register/space tables and the emulator's calling-convention table |
| `summary.rs` | One-step symbolic execution of the slice over a separated memory model (register file, stack frame, context region); per-path `(guard, result, context')`; solver-pruned dead branch sides |
| `expr.rs` | Folding `SymExpr` constructors, the concrete evaluator they fold with, substitution, rendering |
| `learn.rs` | L# apartness tree, basis/frontier rules, Rivest-Schapire counterexample processing |
| `srqa.rs` | Control/data slot factoring; the symbolic register quotient automaton built from a hypothesis and the summaries |
| `certify.rs` | Bounded divergence query (slice vs automaton, `H` steps); deterministic probes |
| `oracle.rs` | `MembershipOracle`; `EmulatorOracle` over `fission_dir::EmulatorHarness` |
| `report.rs` | The `quotient.v1` JSON contract |
| `engine.rs` | `WBCQ_Synthesize`: the learn / build / refute loop and the final report |

## Invariants

1. **Apartness is concrete.** Two states are distinct only with a word the
   emulator ran; every reported witness is re-run from scratch
   (`verified_by_emulator`). A witness that does not reproduce downgrades the
   result to `UnsupportedSemantics`.
2. **`Unknown` is not `Unsat`.** Only an `Unsat` divergence query yields
   `ProvedUnderModel`. An undecided query falls back to probes and yields at
   most `EmpiricallyConsistent`. A feasibility query that is `Unknown` keeps
   both branch sides.
3. **Solver counterexamples are confirmed on the emulator** before they
   refine anything. One the emulator does not reproduce means the P-code model
   and the machine disagree: `UnsupportedSemantics`, never a silent retry.
4. **Fail closed at the boundary.** Calls, user-defined operations, absolute
   or data-computed addresses, floating point, values wider than 8 bytes, and
   results that read undeclared live-ins stop the summary with a reason.
5. **Deterministic output.** Alphabet symbols and counterexamples are least
   solutions (port by port), never whichever model the solver found first;
   every collection is ordered. Same binary and budget, same JSON bytes.
6. **ISA-neutral (ADR 0009).** No rule names an instruction or a register.
   Register locations, endianness, the return-address convention and the
   address spaces come from the SLEIGH tables and the calling-convention
   table; the AArch64 fixture must keep producing the x86-64 quotient.
7. **No quality claims.** Fixture results show the mechanism works; they are
   not evidence about decompiler output (root `AGENTS.md`, Core Rule 10).

## Fixtures

`testdata/quotient_fixture.S` (x86-64) and `quotient_fixture_aarch64.S` are
assembled, not compiled, so the control-flow shape is exactly what the tests
name. The build command is in each file's header; rebuild and commit the
`.elf` with any change to the `.S`.

## Validation

```bash
cargo nextest run -p fission-quotient
cargo clippy -p fission-quotient --all-targets -- -D warnings -A clippy::style -A clippy::complexity
cargo run --release -p fission-quotient --example quotient -- \
  crates/fission-quotient/testdata/quotient_fixture.elf 0x201122
```
