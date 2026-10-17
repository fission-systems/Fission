# Quantum effects in the single FIR — 2026-10-17

This is an ideal logical-circuit experiment, executed on a classical host. It is
not a QPU binary decoder, hardware implementation, noisy-device model or quantum
speedup claim. The CPU/GPU/VM research pipeline remains separate in maturity.

## Ownership and representation

`FirOp::Quantum(QuantumOp)` lives in the existing canonical op table. It adds
H, X, Z, CX, Z-basis measurement, reset and classical-bit-controlled X. There is
no second quantum semantic IR. The ordinary FSL compiler, SSA/type validator,
package reader/writer and diagnostic FIR consume these operations. Packages
containing quantum operations select v12 (tag 23); earlier sources retain their
previous package version and bytes. New readers accept v1..v12; old readers
must refuse v12.

```text
FSL quantum statements ───────┐
restricted OpenQASM → FSL ────┴→ canonical FIR + .fslc
                                ├→ diagnostic FIR
                                ├→ restricted logical OpenQASM
                                └→ classical-host reference simulation
```

Qubit indices identify subsystems of one joint state; they are not ordinary SSA
integers containing independent amplitudes. The wire representation admits
indices 0..255. CX requires distinct subsystems. Classical measurement outputs
and feed-forward conditions are unsigned u1 and follow existing SSA definition/
use rules. Operations are ordered effects on the supplied joint state, matching
the existing implicit register/wave state style. No guest state-copy operation,
allocator, lifetime/linear-type system or effect SSA is introduced by this slice.
Host copies of simulation data do not represent a quantum cloning operation.

```fsl
semantics {
    quantum.h 0;
    quantum.cx 0, 1;
    %a: u1 = quantum.measure_z 0;
    quantum.x_if 1, %a;
}
```

`quantum-circuits.fsl` has seven authored circuit bodies. Their one-byte opcodes
are synthetic selectors in our logical profile, not vendor QPU machine code.

## State and observations

Qubit 0 is basis-index bit 0. The reference accepts an explicitly supplied pure
joint state with 1..8 qubits, exactly 2^n complex f64 amplitudes, finite entries and
norm within 1e-12 of 1. `basis:N` is an explicit caller precondition. Qubit declarations
in OpenQASM do not justify assuming an initial zero state; reset or an external
state assumption is required.

H/X/Z/CX transform the joint state. Measurement enumerates weighted outcomes and
normalizes each conditional post-state. Reset branches internally, applies X to
outcome 1, and excludes its hidden outcome from observable measurement records.
The weighted ensemble therefore represents mixed states after resetting an
entangled subsystem. The diagnostic density matrix is the sum of those weighted
outer products. Measurement record/SSA bits, probabilities and conditional quantum
states must all agree; a final computational-basis histogram is insufficient.

A mathematical specification for this domain is an instrument
`rho -> { (classical_record, E_record(rho)) }`, where each E output is an
unnormalized density operator. Its trace is the probability; normalization gives
the conditional post-state. Reset sums its hidden channels. This extends the
classical possible-outcomes relation with quantum state/probability semantics.

Reference admission: linear bodies, <=256 ops, <=64 classical values in dense
SSA definition order, <=12 measurement/reset operations (<=4096 enumerated
branches), <=8 actual state qubits. Canonical FIR can preserve larger logical
indices while the reference and OpenQASM projection refuse unsupported extents.
Mixed CPU/GPU effects and general block CFG are preserved by canonical validation
when well typed, but not executed by this quantum reference. Classical backends
C/Rust/Go, CUDA/PTX and native JIT/AOT refuse quantum input. Registered source
backends have to explicitly declare the quantum projection context.

This is f64 numerical evidence. Rounding, underflow and tolerance matter; no
symbolic identity proof, exact amplitude arithmetic or hardware/noise model is
claimed. Enumeration/state-vector storage grows exponentially. No RNG is needed
because all admitted measurement branches are enumerated.

## OpenQASM boundary and CLI

Supported input: ASCII OpenQASM 3.0 with `stdgates.inc`, fixed names q/m, static
indexed qubits, dense single-assignment classical slots, h/x/z/cx/reset/measure,
bit constants and `if (m[i] == 1) { x q[j]; }`. Unsupported syntax, reassignment,
dynamic indexing, gates/externs, pulses, loops, physical qubits and custom includes
refuse. The include is recognized as the owned supported logical gate set; no
external file is loaded. This is not a general OpenQASM parser/conformance claim.

The imported source SHA-256 is retained in package evidence. A source statement
map is not implemented. The import API returns declared qubit count separately;
`.fslc` stores instruction effects, not a complete OpenQASM program envelope.
Callers must retain that count (including unused qubits) and the explicit initial
state. CLI projection/simulation therefore always take the qubit count. Import
prints the declared count. Classical unused slots/source names are not recovered
as a source ABI.

```sh
fslc compile crates/fission-fsl/specs/quantum-circuits.fsl /tmp/circuits.fslc
fslc simulate-quantum /tmp/circuits.fslc bell 2 basis:0
fslc simulate-quantum /tmp/circuits.fslc hh 1 vector:0.7071067811865475:0,0:0.7071067811865475
fslc emit-qasm /tmp/circuits.fslc bell 2 /tmp/bell.qasm
fslc import-qasm /tmp/bell.qasm /tmp/bell.fslc
fslc simulate-quantum /tmp/bell.fslc circuit 2 basis:0
```

JSON is diagnostic simulation evidence only; FSL text and binary FSLC remain
source/package formats. OpenQASM output is a logical program projection, not a
physical-device executable. QIR import/output is not implemented.

## Validation

Rust tests cover FSLC/QASM roundtrips, complex H;H and H;Z;H identities, Bell
coherence versus measurement/reset mixtures, repeated measurement, feed-forward,
bit constants, small-probability support, upper resource bounds, invalid types/
indices/SSA, and rejection by foreign backends. A separate Python stdlib oracle
uses dense density matrices and Kraus channels, unlike the native amplitude
interpreter. Seven authored circuits, 154 inputs and 308 original/roundtrip
comparisons inspect conditional states and probabilities within 2e-10. It includes
basis, complex product and entangled input states. Finite authored-corpus checks
are not whole-program equivalence, an independent language-parser oracle, or a
comparison with a physical QPU.

Semantic references: [OpenQASM reset/measurement](https://openqasm.com/versions/3.0/language/insts.html),
[classical control](https://openqasm.com/versions/3.0/language/classical.html),
[standard logical gates](https://github.com/openqasm/openqasm/blob/main/examples/stdgates.inc).
The design owns these FIR semantics; OpenQASM/QIR do not define FIR's full scope.
