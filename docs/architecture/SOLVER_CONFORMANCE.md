# Solver Bitvector Conformance

The initial solver conformance surface is the pure bitvector fragment printed
by `fission-solver::smtlib`. Supported scalar widths are 1 through 64 bits, and
all widths in this fragment are measured in bits.
The AST may represent more theories, but they are not part of this matrix until
their semantics have an independent definition-based check.

Unsigned arithmetic, bitwise operations, equality, and unsigned comparisons
zero-extend the narrower operand to the wider width. `ite` zero-extends its
value arms and requires a 1-bit condition. Signed comparisons, signed
division/remainder, and shifts require equal operand widths; a mismatch is
outside the matrix and returns `Unknown`.

## Supported QF_BV Matrix

The expected result in every row is the corresponding SMT-LIB bitvector
definition. `soundness_probe.rs` compares both satisfiability and
unsatisfiability for exhaustive 4-bit inputs or operand pairs.

| Operators | Definition-based coverage |
|---|---|
| `bvadd`, `bvsub`, `bvmul` | Every 4-bit symbolic pair; constant operands and same-variable cases are also checked |
| `bvudiv`, `bvurem` | Every 4-bit constant pair and symbolic pair, including division by zero |
| `bvsdiv`, `bvsrem`, `bvsmod` | Every 4-bit constant pair and symbolic pair, including zero divisors and signed overflow cases |
| `bvand`, `bvor`, `bvxor` | Every 4-bit symbolic pair, constant pair, and same-variable cases |
| `bvshl`, `bvlshr`, `bvashr` | Every 4-bit constant pair and symbolic shift amount, including amounts at or above the width |
| `=`, `distinct`, `bvult`, `bvule`, `bvslt`, `bvsle`, `bvsgt` | Every 4-bit symbolic pair; signed constant-fold boundaries at 1, 8, 32, and 64 bits |
| `ite` | Every 4-bit condition/value pair |
| `extract` | Every 4-bit source value for a valid slice; invalid slices must return `Unknown` |
| `concat` | Every pair of 4-bit source values |

The exhaustive checks are in `crates/fission-solver/tests/soundness_probe.rs`.
Wider boundary coverage lives in `tests/bit_width_contract.rs` and the AST
unit tests. The optional `tests/differential_z3.rs` process oracle is a second
implementation check; it is ignored by default and has no Z3 or FFI dependency.

## Unsupported Operations

An operation without an exact circuit or a formula outside the supported
width/shape rules (including widths above 64 bits) must make
`Solver::check_sat()` return `Unknown`. Fresh,
unconstrained bits may stand in for the result internally, but they are never a
verdict. In particular, symbolic IEEE arithmetic and comparisons that do not
yet model all IEEE cases must not be consumed as `Sat` or `Unsat`. The DIR
verifiers share one result classifier: only `Unsat` maps to `Equivalent`.

Arrays are outside this QF_BV matrix and are checked through the solver's
separate array/memory-oracle path.
