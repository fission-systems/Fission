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
It covers what the 4-bit matrix cannot: widths up to 64 bits, operands of
different widths, arrays (`QF_ABV`) with and without a memory oracle, the
folding constructors, the model behind every `Sat`, and a solver asked several
questions in a row. `crates/fission-solver/AGENTS.md` lists its modes.

A query may be bounded with `Solver::set_conflict_budget`. One that runs out
returns `Unknown` with `UnknownReason::ConflictBudget`; it is never a verdict.

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

A read is decided only when `ArrayTheory` wrote axioms that describe it.
Two shapes it does not describe return `Unknown`, and
`tests/array_sort_contract.rs` holds both with a control beside each:

- **A store whose value is wider than the array's element.** The axiom
  `select(store(a, i, v), i) == v` compares an element with `v` by
  zero-extending the element, so it asserted that `v`'s high bits are zero:
  `v == 0x1234` stored into a byte array made the problem UNSAT. SMT-LIB has
  no such store to take a meaning from. A narrower value is zero-extended,
  like every other unsigned operand.
- **An array whose index or element is not a 1-to-64-bit scalar.** The theory
  skips it, so a read through a narrow index was free bits with no record,
  and `a[i] != a[i + 0]` was SAT.

An index whose width differs from the array's index sort is compared by
zero-extension in every axiom, so the array behaves as a map from the
zero-extended index; that is consistent, and it is what an address means.

## Incremental query contracts

- Unsigned mixed-width subtraction zero-extends the subtrahend **before**
  complementing it for two's-complement addition.
- Eager assertion loading and query catch-up share one monotonic cursor. Pending
  append-only compatibility assertions are loaded before the cursor advances.
- Concrete evaluation and extrema require a satisfiable, supported context.
  Unsupported widths and UNSAT contexts do not produce guessed `u64` values.
- Input clauses and global theory lemmas are installed at decision level zero.
  All query assumptions are replayed after theory lemmas are installed. Clause
  insertion validates fresh variables, watches and immediate root conflicts.
- Learned-clause quality is captured on the conflict trail before backtracking.
  Collection retains every glue clause (`LBD <= 2`), every input/theory clause,
  and every locked reason clause.

`issue_regressions.rs` checks all 900 pairs of unsigned operands at widths one
through four (both equality and disequality queries), assertion cursors,
constant-query context, invalid extracts, array scalar abstention, and widths
through `u32::MAX`. `theory_lemma_regressions.rs` compares 2,000 delayed theory
clause sets with assumptions against direct enumeration and checks each SAT
model. The separate SAT-core suite retains its 20,000-CNF checks and the 2,000
incremental clause-sequence checks.

Whole-array scalar projections, including nested array sorts, abstain before
bit-blasting. Unsupported placeholders are capped at 64 bits regardless of
array range width; array-valued model entries are never cast into `u64`.

## Constant folding

`SymExpr::new_*` answers for constant operands without building a node, and a
caller that builds the node gets the circuit. The two must give the same
value; `tests/folding_agrees_with_lowering.rs` asserts `raw != folded` for
each constructor and expects UNSAT.

- A constant's value is its low `size` bits. `SymExpr::Const` has public
  fields, so a payload can carry more; lowering ignores the excess and the
  folds now do too. `new_eq` of the 8-bit payloads `0x1ff` and `0xff` said
  "different" where the circuit says "equal".
- The same holds for a constant shift amount, which lowering applies without
  building a shifter: the 8-bit payload `0x101` shifts by one, not by 257.
- And for `is_true` / `is_false`, which answer for a constant without a
  query: the 1-bit payload `2` is false.
- `new_fneg` and `new_fabs` act on the sign bit of the bit pattern. Folding
  a 32-bit constant through `f64` quieted a signalling NaN, so the folded
  payload differed from the circuit's.
