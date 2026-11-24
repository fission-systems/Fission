# fission-causal Agent Guide

Scope: `crates/fission-causal`. Experimental CSIC prototype (RFC #1101,
Track B). P1/P2 report: `docs/proposals/2026-11-23-csic-p1-counterfactual-runner.md`.

## Rules

- **No production dependents.** Nothing outside this crate (CLI, decompiler,
  DIR) may depend on it until an ADR adopts the track. It must not change
  NIR/HIR output.
- **Verdict vocabulary is the contract** (`src/contract.rs`). A
  `CounterfactualWitness` is a one-location flip confirmed by a fresh replay,
  not an actual-cause or minimality claim. Do not add a "cause"/"minimal"
  verdict without contingency search and a written soundness argument.
  `Minimality::MinimalWithinBudget` is bounded (input bytes only, others
  held at recorded values, what the symbolic shadow models) and must say so.
- **Solver models are proposals.** Every model is replayed in the emulator
  before it is used; one that does not reproduce is rejected and logged, and
  the search then ends `SolverUnknown`. Unknown is never "no cause".
- **Input first.** Input bytes and named variables are the primary
  candidates; internal registers/stack slots only via
  `InternalPolicy::Always` or when no input-level witness exists.
- **Dynamic analysis only on this crate's fixtures** (`testdata/`) or on
  `~/fission-benchmark/dir-verify/binaries`. Never on DecBench/evalkit binaries.
- **Correctness evidence is native execution** (`testdata/native/verify_native.py`)
  plus fresh emulator replay. Agreement between two emulator paths shares the
  SLEIGH lift (ADR 0017).
- **Deterministic output.** Candidate order, values and verdicts must not depend
  on hash order or timing; only `CaseReport::timing` may.
- **ISA-agnostic** (ADR 0009): read control flow, registers and memory from
  p-code and the language's register map; classify syscalls by name.
- Do not build a fresh `Emulator` per experiment: `Emulator::new` re-reads the
  compiled Sleigh tables. Restore checkpoints into the pooled `Replayer`.

## Layout

| File | Role |
|---|---|
| `src/contract.rs` | Sink Φ, locations, interventions, fingerprint, verdicts |
| `src/machine.rs` | `Program` (pinned binary + input), `Run`, exact `run_to_step` |
| `src/trace.rs` | Single-step recorder, `SinkMonitor`, observers |
| `src/cfg.rs` | Per-function CFG, postdominators, control dependence |
| `src/slice.rs` | Backward dynamic slice, candidates, value alternatives |
| `src/runner.rs` | Checkpoints (`Replayer`), tiered oracle, verification, solver and pair joint search, minimality, contingencies |
| `src/symbolic.rs` | Symbolic replays (`ShadowMode::Symbolic`, interpreter), canonical input variables, solver queries |
| `src/baseline.rs` / `src/static_slice.rs` | Fission-taint and pinned static-slice baselines |
| `src/eval.rs` | The controlled cases and scoring |
| `examples/csic_eval.rs` | Evaluation harness (per-case child processes) |

## Commands

```bash
CARGO_TARGET_DIR=... cargo nextest run -p fission-causal
cargo run --profile quick-release -p fission-causal --example csic_eval -- --out report.json
python3 crates/fission-causal/testdata/native/verify_native.py report.json
```
