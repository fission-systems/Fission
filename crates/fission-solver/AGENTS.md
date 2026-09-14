# fission-solver Agent Guide

Generated: 2026-07-07
Scope: `crates/fission-solver`

## Overview

`fission-solver` is Fission's pure-Rust symbolic constraint engine. It provides the `SymExpr` AST and `Solver` infrastructure for expressing, storing, and eventually checking symbolic constraints during concolic and symbolic execution.

**Key principle:** This crate must remain free of C/C++ dependencies and must not bind to any external SMT library (Z3, STP, Boolector, CVC5, etc.). All solver logic must be implemented in pure Rust.

## Module Map

| Module | File | Purpose |
|---|---|---|
| `SymExpr` | `src/ast.rs` | Symbolic expression AST: constants, variables, arithmetic, bitwise, comparisons, ITE, bitvector ops |
| `Solver` | `src/solver.rs` | Node registry, assertions, `check_sat` with a memory-oracle CEGAR loop, `is_true`/`is_false`/`min`/`max` |
| `AigManager` | `src/aig.rs` | Bit-blasting to an and-inverter graph: adders, comparisons, shifts, multiply, unsigned divide, if-then-else |
| `CnfBuilder` | `src/cnf.rs` | Tseitin encoding of the AIG |
| `SatSolver` | `src/sat.rs` | CDCL: watched literals, VSIDS heap, phase saving, LBD-scored learned-clause GC |
| theories | `src/theory/` | Bitvector (eager bit-blasting) and array (select/store) |

## Key Invariants

1. **No FFI to SMT solvers** — Do not add `z3`, `z3-sys`, `boolector`, `cvc5-sys`, or equivalent crates.
2. **Node IDs are globally unique** — `VAR_COUNTER` is a process-global `AtomicU32`. Node IDs must never be reused across solver instances.
3. **`SymExpr` is clone-friendly** — All AST nodes must implement `Clone`. Expressions are shared by cloning into `solver.nodes`.
4. **`Solver::register_node` is the canonical way to store a computed expression** — Do not store nodes in ad-hoc side maps.
5. **`Solver::register_var` is the canonical way to create a new symbolic variable** — Used by taint sources.
6. **Assertions must be 1-bit** — `solver.assert(expr)` should only accept `SymExpr` values where `get_size() == 1`.
7. **An operation without a circuit makes the answer `Unknown`** — `AigManager` records every operation it cannot encode (and every assertion or assumption that is not one bit wide) and lowers it to *unconstrained* bits. `check_sat` then returns `Unknown`, never `Sat` or `Unsat`. It used to lower them to all-false bits, which made `x*3 != x*5` UNSAT: a false equivalence proof. fission-dir maps `Unsat` straight to `Equivalent`, so this invariant is what keeps a missing circuit from becoming a wrong proof.
8. **`Unknown` is not a verdict** — `is_true`/`is_false` require an explicit `Unsat` of the negation; `min`/`max` return `None` when a probe is `Unknown`. Treating `!satisfiable(..)` as proof reads "could not tell" as "always".
9. **Sizes are not consistently bits** — `new_var(_, 8)` and the constant-folding masks treat size as bits, while the emulator registers a stdin byte with size `1` and `max()` multiplies `get_size()` by eight. Know which convention a caller uses before relying on a width.
10. **Check circuits against definitions, not against Z3** — the division circuit is ported from Z3 (MIT); a test that agrees with Z3 shows the port is faithful, not that it is right. `tests/soundness_probe.rs` checks every 4-bit input against the SMT-LIB definition.

## `SymExpr` AST Reference

| Variant | Inputs | Output size | Notes |
|---|---|---|---|
| `Const { val, size }` | — | `size` | Concrete bitvector constant |
| `Var { id, name, size }` | — | `size` | Named symbolic variable |
| `Add(a, b)` | bitvec, bitvec | `a.size` | Unsigned addition |
| `Sub(a, b)` | bitvec, bitvec | `a.size` | Unsigned subtraction |
| `Mul(a, b)` | bitvec, bitvec | `a.size` | Unsigned multiplication |
| `Udiv(a, b)` | bitvec, bitvec | `a.size` | Unsigned division |
| `And(a, b)` | bitvec, bitvec | `a.size` | Bitwise AND |
| `Or(a, b)` | bitvec, bitvec | `a.size` | Bitwise OR |
| `Xor(a, b)` | bitvec, bitvec | `a.size` | Bitwise XOR |
| `Shl(a, b)` | bitvec, bitvec | `a.size` | Left shift |
| `Lshr(a, b)` | bitvec, bitvec | `a.size` | Logical right shift |
| `Eq(a, b)` | bitvec, bitvec | `1` | Equality comparison |
| `Neq(a, b)` | bitvec, bitvec | `1` | Inequality |
| `Ult(a, b)` | bitvec, bitvec | `1` | Unsigned less-than |
| `Ule(a, b)` | bitvec, bitvec | `1` | Unsigned less-or-equal |
| `Ite { cond, t, f }` | bool, bitvec, bitvec | `t.size` | If-then-else |
| `Extract { expr, lsb, size }` | bitvec | `size` | Bit extraction |
| `Concat(a, b)` | bitvec, bitvec | `a.size + b.size` | Bitvector concatenation |

## Planned Development

Bit-blasting and CDCL exist; constant folding exists for the common constructors. What is missing, roughly in order of what a benchmark would need first:

1. **Remainder, signed division, arithmetic shift, sign extension** — no AST nodes and no circuits yet (Z3 reference: `mk_sdiv_srem_smod`, `mk_ashr`, `mk_sign_extend` in `src/ast/rewriter/bit_blaster/bit_blaster_tpl_def.h`). Until then they surface as `Unknown`.
2. **An SMT-LIB QF_BV reader** — to run standard benchmarks, whose `:status` is the answer key.
3. **A consistent size unit** — see invariant 9.
4. **Word-level simplification before blasting** — Z3's `bv_rewriter.cpp` is the reference; add rules only where Fission's own queries show a cost.
6. **Model extraction** — After `Sat`, extract concrete variable assignments from the learned model.

## Anti-Patterns

- Do not add `z3`, `z3-sys`, `boolector-sys`, `cvc5`, or any C/C++ SMT library dependency.
- Do not store node expressions in maps outside `solver.nodes`.
- Do not reuse `SymNodeId` values.
- Do not emit `SatResult::Sat` after implementing CDCL if the actual result is `Unsat` — correctness is required.
- Do not treat `check_sat()` returning `Sat` as a proven result until the stub is replaced.

## Build / Test Commands

```bash
# Type-check solver crate
cargo check -p fission-solver

# Run solver tests
cargo nextest run -p fission-solver

# Build everything that depends on fission-solver
cargo check -p fission-emulator
cargo check -p fission-cli
```

## References

- `crates/fission-emulator/src/pcode/eval.rs` — Primary consumer of `SymExpr` and `Solver`
- `crates/fission-emulator/src/pcode/state.rs` — Stores `SymNodeId` references in `shadow_memory`
- `vendor/angr-master/` — Reference for symbolic execution concepts (read-only, no dependency)
- Root `AGENTS.md` — Repository-level rules take precedence
