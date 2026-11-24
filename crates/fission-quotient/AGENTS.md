# fission-quotient Agent Guide

Scope: `crates/fission-quotient`

## Overview

Experimental WBCQ prototype (RFC #1093, stages P0/P1): computes the minimal
observable state machine of a P-code slice instead of structuring its CFG.
Nothing in the production pipeline (`fission-midend-*` -> HIR) depends on
this crate; the CLI reaches it only through the experimental `quotient`
command (`crates/fission-cli/src/cli/oneshot/quotient.rs`), which builds a
boundary from flags and prints the report. It is not a decompiler output surface, and nothing it
reports is a DecBench or readability claim.

## Module map

| Module | Owns |
|---|---|
| `boundary.rs` | `QuotientBoundary` (inputs, hidden context, pointer regions, observed result) and the one byte space they are laid out in (`layout`), `QuotientBudget`, `AbiModel` read from the SLEIGH register/space tables and the emulator's calling-convention table (argument, return, preserved registers) |
| `calls.rs` | Call facts per site (`fission_dir::call_sites` for no-return and tail jumps, symbol/PLT names, `fission-signatures` prototypes) and the `CallModel` both sides observe a call through |
| `summary.rs` | One-step symbolic execution of the slice over a separated memory model (register file, stack frame, context, pointer regions); per-path `(guard, result, context', calls, exit)`; solver-pruned dead branch sides; how each call site is observed; pointer-field discovery and indexed (`buf[i]`) reads and writes within a region |
| `expr.rs` | Folding `SymExpr` constructors, the concrete evaluator they fold with, substitution, rendering |
| `learn.rs` | L# apartness tree, basis/frontier rules, Rivest-Schapire counterexample processing |
| `srqa.rs` | Control/data slot factoring; the symbolic register quotient automaton built from a hypothesis and the summaries |
| `certify.rs` | Bounded divergence query (slice vs automaton, `H` steps); deterministic probes |
| `oracle.rs` | `MembershipOracle`; `EmulatorOracle` over `fission_dir::EmulatorHarness` (calls stubbed with `call_stubbed`, returns taken from the symbol; the context and every pointer region mapped as one buffer, each region's address in its pointer field) |
| `report.rs` | The `quotient.v1` JSON contract; its schema is `docs/schemas/cli-quotient-data.v1.schema.json` (`fission_cli schema quotient`) -- change both together |
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
4. **Fail closed at the boundary.** User-defined operations, indirect calls,
   absolute addresses outside the image's sections, data-computed addresses
   outside a region, observed floating-point values, values wider than 8
   bytes, and results that read undeclared live-ins stop the summary with a
   reason.
5. **Calls are effects, never run.** A direct call is observed as its target
   and argument registers; its return is the next call-return input port
   (`retN`), chosen by the environment, so the quotient holds for every
   value. Both sides stub the callee. After a call only the convention's
   preserved registers keep their values. A proven no-return callee ends the
   step and halts the machine; a jump out of the slice is a call whose return
   is the step's. A library callee (a prototype in the signature library)
   observes exactly its parameters; an unknown callee observes every argument
   register the boundary defines. An address of the context or the frame
   passed where the callee may write through it is unsupported. Every such
   choice is listed in the report's `assumptions`.
6. **Pointer fields are regions, never aliases.** A pointer the slice reads
   out of the context (or out of a region) and dereferences is a region of
   its own: declared with the boundary, or discovered and zeroed. Its base is
   never known, only offsets in it; it aliases nothing, is not null and never
   moves -- all reported in `assumptions`. An index computed from data reads
   or writes every place it can take; the side of the path where it would
   leave the region ends in `LeftRegion`, and a quotient in which a reachable
   state takes it is unsupported. An overwritten pointer field, a stored
   address, an indexed read of a pointer, a comparison between two regions'
   addresses, and a region pointer given to a callee are unsupported.
   Two more kinds of region are anchored rather than pointed to: global
   memory at an absolute address in an image section (starting as the
   oracle's loaded baseline holds it, copied in and read back around every
   step), and the block of a register the caller sets up (the thread
   pointer: thread-local storage, the stack protector's canary), which a
   call preserves when the convention does not list it as changed. A
   floating-point result, or a register wider than 8 bytes, is poison:
   refused only where something observes it.
7. **Deterministic output.** Alphabet symbols and counterexamples are least
   solutions (port by port), never whichever model the solver found first;
   every collection is ordered. Same binary and budget, same JSON bytes.
8. **ISA-neutral (ADR 0009).** No rule names an instruction or a register.
   Register locations, endianness, the return-address convention and the
   address spaces come from the SLEIGH tables and the calling-convention
   table; the AArch64 fixture must keep producing the x86-64 quotient.
9. **No quality claims.** Fixture results show the mechanism works; they are
   not evidence about decompiler output (root `AGENTS.md`, Core Rule 10).

## Fixtures

`testdata/quotient_fixture.S` (x86-64) and `quotient_fixture_aarch64.S` are
assembled, not compiled, so the control-flow shape is exactly what the tests
name. The build command is in each file's header; rebuild and commit the
`.elf` with any change to the `.S`.

## Evaluation

`scripts/quality/wbcq/` holds the RFC's P3 evaluation:
- an OLLVM-style obfuscator;
- a state-machine corpus with negative cases;
- a native ground truth;
- a DecBench step-function sample.

Results and procedure are in `docs/proposals/2026-11-23-wbcq-p3-evaluation.md`
(P4 steps: section 10 onwards).
Re-run it after any change to what the engine accepts. A changed state count
on the corpus is a regression until the ground truth says otherwise.

## Validation

```bash
cargo nextest run -p fission-quotient
cargo clippy -p fission-quotient --all-targets -- -D warnings -A clippy::style -A clippy::complexity
cargo run --release -p fission-quotient --example quotient -- \
  crates/fission-quotient/testdata/quotient_fixture.elf 0x201122
```
