# Go backend over canonical FIR

The in-process backend registry now includes `GoBackend`, ID `go`, dialect
`go1.25`. It uses the same checked plan and stack/register/wave/window contracts
as C/Rust. The original FIR op table remains the sole semantic input. There is
no new IR, package version, opcode executor or decoder dependency.

## Output and ABI

Generated files are `package fsl`, with no imports or external Go packages.
Bitvectors use `uint64` and explicit width masks; carry uses unsigned arithmetic
and comparisons, signed ordering flips the sign bit, and extension/truncation
preserves bit patterns. Block edges assign parameters simultaneously. Go syntax
is emitted directly from canonical operations and terminators.

- Stack body: `fsl_execute(stack []uint64, depth *int) uint32`.
- Scalar body: `fsl_execute(registers, flags, fields []uint64) uint32`.
- Wave body: `fsl_execute(registers, flags, lane_registers []uint64, lanes uint16,
  exec uint64, fields []uint64) uint32`.
- x86 window: `fsl_x86(registers, flags []uint64, memory []byte, memory_base uint64,
  budget int, exit_pc *uint64) uint32`.

Stack and standalone scalar bodies use private snapshots. Wave preconditions are
checked before effects; inactive lane bit patterns and unrelated banks remain.
x86 operates on private state and mutable-memory snapshots and commits only on
successful RET. Mutable memory is bounded to 1 MiB; readonly windows retain the
existing contract. Go allocates snapshots internally instead of receiving C/Rust
scratch storage. Heap exhaustion/host resource failure is not a guest status.

Safe Go permits aliasing slices. Output banks must be disjoint; overlap is checked
using element pointer equality, without `unsafe`. x86 also refuses an exit pointer
inside either output bank. Fields are snapshotted before effects, allowing field
input aliasing safely. Concurrent callers accessing the same storage and aliases
constructed through unsafe host code are outside the contract. The
`fsl_runtime_` symbol namespace is reserved. Existing C/Rust source bytes and
metadata remain unchanged.

## CLI

```sh
fslc emit-backend PACKAGE INSTRUCTION go OUTPUT.go
fsl-x86 plan-binary PACKAGE BINARY ADDRESS SIZE go
fsl-x86 bundle-binary PACKAGE BINARY ADDRESS SIZE go NEW_DIRECTORY
```

Build recipe: `go build -o output.a output.go`. A bundle includes the same admission
report and symbol origins as other registry providers; output generation still
marks compilation and equivalence as unperformed. The legacy `OutputLayer` enum
and sequence emitter remain unchanged; Go instruction/window output uses the
registry. Go `FirSequence` emission, general source prototypes/ABI/CFG recovery,
loops in semantic bodies, exceptions, object runtime, concurrent guest memory
and architectural faults remain unsupported. Existing admitted x86 direct guest
control is bounded by the window execution budget.

## Validation scope

Native tests cover all four input contexts, registry/foreign-plan/domain and
reserved-symbol refusal, and 16/32/64 x86 emission/origins. The research gate
compiles and executes six locked actual 32/64 ELF spans in Go at default and
optimization-disabled settings, and compares C/Rust O0/O2 on the same payload.
Independent expected states include GPRs, defined flag slots, return PC, all 160
window bytes, and rollback after writes. Go Linux amd64/386 cross compilation is
compile-only evidence, not guest execution or 32-bit host execution.

Synthetic domain gates cover 1/8/16/32/64 carry-in/out, destination/input alias,
stack/structured branches, signed extension/comparison, and VGPR/SGPR broadcast
wave64 masks. Real GPU hardware, full ISA coverage, source-level Go recovery,
whole-function equivalence and a performance advantage are not established.
