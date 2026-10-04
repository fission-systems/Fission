# Structured control in the canonical FIR

Implemented experimental slice: 2026-10-04, in `fission-fsl`. This preserves
instruction semantic-body control; it is not whole-function binary lifting.

## Ownership and syntax

`CompiledInstruction` owns one `values` table, one `ops` table and a `blocks`
table. Each `FirBlock` owns a disjoint contiguous op range, has typed parameter
IDs and a `FirTerminator`. No second semantic IR or P-code lowering is involved.
Legacy linear bodies have an implicit entry block and return view, without
changing their serialized versions 1–5. New bodies/operations require `.fslc` v6.

```text
semantics {
    block entry() {
        %x: u32 = stack.pop;
        %limit: u32 = int.const 10;
        %small: u1 = int.ult %x, %limit;
        branch.if %small, low(%x), high(%x);
    }
    block low(%low_value: u32) { branch join(%low_value); }
    block high(%high_value: u32) { branch join(%high_value); }
    block join(%joined: u32) { stack.push %joined; return; }
}
```

`int.const` stores a raw bit pattern fitting the declared integer width 1–64;
negative signed values use their two's-complement bit pattern. Comparisons
`int.eq`, `int.ult`, `int.slt` require matching input types and a `u1` result;
less-than signedness must match the predicate. Branch conditions require `u1`.
Equality does not reinterpret signedness. Signed less-than in reference/C/Rust
uses sign-bit ordering to avoid signed overflow or implementation-defined casts.

Each SSA value has one global ID/name/definition. Within a block, only its
parameters and earlier local definitions are readable. Cross-block values
must be passed as edge arguments, including values from a dominating block.
Targets may appear later or earlier in the source; parameter arity and exact
types must match. All arguments are evaluated before any target parameter is
assigned. Every block must be reachable from entry along syntactic edges.
Entry has no external parameters in this slice. Returns finish the semantic
body and expose its stack effects; they are not guest call returns.

## Validation and execution

Source compilation, package serialization/loading, reference execution and
output generation share validation. Block names and value IDs are unique,
ranges own every op once, conditions/arguments stay in scope, and type rules
are checked before any effects. Binary counts, IDs, tags, bounds and EOF are
validated. Limits: 256 blocks, 65,535 values/ops and 64 MiB per package.

The reference and C/Rust backends admit acyclic integer/stack CFGs at widths
1–64, with equal stack deltas at each join and all returns. Topological analysis
derives the worst stack depth over all syntactic paths. The wrapper checks
required inputs and capacity before effects; failed preflight preserves state.
An untaken path can still increase the required capacity: this is a declared
conservative backend precondition, not a guest ISA fault or JVM verifier rule.
Within admitted executions, only the selected successor executes.

C/Rust directly traverse the same block graph and preserve edge argument order.
They use the existing bit-vector stack/status ABI. Source outputs do not recover
or claim original high-level if/loop syntax. Cyclic CFGs and unequal-delta joins
can be represented/serialized/diagnosed, but execution and C/Rust output refuse.
Native lift records, JIT/AOT decoder generation, register/wave executors and
CUDA/PTX outputs refuse control bodies instead of executing their op tables
linearly. Register/lane control, memory, traps, calls and region effects require
further contracts and execution support.

## Portable package v6

Versions 1–5 retain their byte layouts. v6 adds op tags 14 (constant: output ID,
u64 bits) and 15 (compare: output/left/right IDs, predicate byte 0/1/2). After
each instruction's op table is a u16 block count, then each block's name,
parameter IDs, u16 start/end range and terminator. Terminator tags are 0 return,
1 branch and 2 conditional branch. Edges store a u16 target followed by their
counted value-ID arguments. Empty block tables mean legacy entry/return views;
an explicitly structured return-only body may have zero ops/values.

## Validation evidence

`tests/control.rs` verifies package compatibility/truncation/downgrades, scopes,
types, range/edge refusal, native CLI execution and refusal by other backends.
One self-authored unsigned branch/join body is evaluated for 65,536 inputs with
an independent arithmetic oracle. Another 16 profiles (1/8/32/64-bit predicates,
argument reordering, constants, empty body and transient stack peak) compare
2,048 reference/oracle states and 8,192 C/Rust O0/O2 runs. Failure comparisons
check all four backing slots; successful comparisons check active stack state.
C and Rust builds deny warnings. These are synthetic semantic contract checks,
not real binary decompilation or whole-function equivalence measurements.
