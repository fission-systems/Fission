# Integer conversions and register-state CFG in the canonical FIR

Experimental implementation: 2026-10-04. These are self-authored semantic-body
fixtures and backend regression gates, not lifting a real binary function.

## Integer conversion contract

The sole FIR op table gains `IntConvert { output, input, kind }`:

| FSL operation | Type rule | Raw bit-vector result |
|---|---|---|
| `int.zext %x` | Unsigned input/output, strictly larger output width | Preserve low input bits, fill new bits with zero |
| `int.sext %x` | Signed input/output, strictly larger output width | Repeat the input sign bit in new high bits |
| `int.trunc %x` | Matching signedness, strictly smaller output width | Keep only low output bits |

Both widths must be 1..64 in this slice. Equal widths, reversed width changes,
signedness changes and wider execution types refuse at source/package validation.
These operations do not implicitly reinterpret signedness. Storage remains
unsigned host bit vectors even for signed FIR values. Reference sign extension
uses masks; source outputs use unsigned XOR/subtraction with wrapping behavior.
They avoid signed shifts, signed overflow and a shift by 64. The same helpers
serve stack and register-state backends. Narrow register writes replace the
whole logical slot with the result bit pattern; partial byte-view writes require
a separate storage contract.

`.fslc` v7 adds tag 16: output ID (u16), input ID (u16), kind byte
(0 zero extend, 1 sign extend, 2 truncate). The v6 block section is retained.
Compilation selects v7 only when conversions are present. Existing v1–v6 byte
layouts and the v6 fixture's package/output hashes remain unchanged.

## Register-state control contract

Reference execution and C/Rust projection traverse the same `FirBlock` graph.
They support scalar register/flag reads and ordered writes, constants,
comparisons, conversions and existing unsigned wrap/carry operations. The
existing register/flag/raw-field ABI remains in use. Each selected block executes
its own range in the instruction's single op table, then transfers edge
arguments simultaneously to the target's parameters.

A single mutable register/flag runtime context persists through edges and joins.
SSA snapshots retain captured values even if a later write aliases a source.
Reads in a join observe preceding writes on the selected path. Only that path
mutates state. This does not implement first-class domain state values, effect
SSA, effect reordering, memory tokens, lane state joins or region execution.

Preflight checks the decoded observation, all raw-field constraints, all used
register/flag indices, and every flag value before any effects. It includes all
syntactic paths, including untaken branches. This conservative backend ABI
precondition is not a guest architecture fault. Cyclic graphs remain valid FIR
for serialization/diagnostics, but register execution and source output refuse
them before mutation. Stack effects and lane effects cannot mix into this
register executor. The byte-layout adapter still requires exact value/view
widths and disjoint slot bindings; the mixed-width fixture is a logical-slot
test, not validation of that adapter.

## Validation and boundaries

`tests/convert_state.rs` checks v7 round-trip/truncation/downgrades, invalid
conversion types, native CLI, failure preservation and independent arithmetic
oracles. There are 24 conversion profiles (including 1/7/9/63/64-bit boundaries),
3,072 reference/oracle inputs and 12,288 C/Rust O0/O2 comparisons. The register
fixture exhausts 256 input bit patterns and 27 source/destination/auxiliary slot
placements. Its recompilation gate has 6,912 success cases and eight failure
cases, totaling 27,680 C/Rust O0/O2 comparisons. Full backing storage is compared,
including storage outside active slices on failure.

`tests/register_state.rs` also constructs two branch/join carry graphs at 32
and 64 bits. Each has 1,024 reference/widened-sum cases and six source ABI refusal
cases: 8,240 C/Rust comparisons together. They exercise wrap-add, add-carry,
carry-input arithmetic and reordered block arguments. These graphs are authored
by the test; they are not recovered GPU CFGs. C and Rust compile with warnings
as errors at both optimization levels.

The research repository locks an 803-byte package and FIR/C/Rust outputs,
reproduced by `tools/register_fir_reproduce.py`, with 44 native CLI oracle states
and three failure cases. No vendor ISA decoder, SMT proof, hardware execution or
whole-function equivalence is asserted. Loops, memory/calls/exceptions, GPU
CFG/divergence, binary function lifting, and structured native JIT/AOT remain
unsupported. CUDA/PTX backends refuse these bodies.

## Native CLI example

```sh
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/register-branch-convert.fsl /tmp/register-cfg.fslc
cargo run -p fission-fsl -- execute-state /tmp/register-cfg.fslc \
  register.branch.convert 10a20000 128,77,88 0,0,0
cargo run -p fission-fsl -- emit-bytes /tmp/register-cfg.fslc \
  register.branch.convert 10a20000 c /tmp/register-cfg.c
```

The result is `registers=[128, 4294967168, 128] flags=[1, 1, 1]`.

The subsequent [sequence state/origin slice](fir-sequence-state-origin.md)
composes bounded sequential instances of these same bodies. It does not add
inter-instruction branch recovery or a binary function loader.
