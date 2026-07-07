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
| `Solver` | `src/solver.rs` | Node registry, path condition list, SAT result enum, skeleton `check_sat()` |

## Key Invariants

1. **No FFI to SMT solvers** — Do not add `z3`, `z3-sys`, `boolector`, `cvc5-sys`, or equivalent crates.
2. **Node IDs are globally unique** — `VAR_COUNTER` is a process-global `AtomicU32`. Node IDs must never be reused across solver instances.
3. **`SymExpr` is clone-friendly** — All AST nodes must implement `Clone`. Expressions are shared by cloning into `solver.nodes`.
4. **`Solver::register_node` is the canonical way to store a computed expression** — Do not store nodes in ad-hoc side maps.
5. **`Solver::register_var` is the canonical way to create a new symbolic variable** — Used by taint sources.
6. **Assertions must be 1-bit** — `solver.assert(expr)` should only accept `SymExpr` values where `get_size() == 1`.
7. **`check_sat` is a stub** — Until DPLL/CDCL bit-blasting is implemented, `check_sat()` returns `SatResult::Sat`. Do not treat this as a verified result.

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

The solver is currently a scaffolding. The planned implementation path is:

1. **Constant folding** — Evaluate `Add(Const(3), Const(5))` to `Const(8)` at construction time.
2. **Simplification** — Implement algebraic simplifications (identity, absorption, De Morgan).
3. **Bit-blasting** — Lower bitvector expressions to CNF SAT clauses.
4. **DPLL** — Implement the Davis-Putnam-Logemann-Loveland SAT procedure.
5. **CDCL** — Extend with Conflict-Driven Clause Learning for practical performance.
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
