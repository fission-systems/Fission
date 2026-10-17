# Quantum effects in the single FIR — 2026-10-17

This is a logical-circuit experiment with explicit noise channels, executed on a
classical host. It is not a QPU binary decoder, hardware implementation, calibrated device model or quantum
speedup claim. The CPU/GPU/VM research pipeline remains separate in maturity.

## Ownership and representation

`FirOp::Quantum(QuantumOp)` lives in the existing canonical op table. It adds
H, X, Z, CX, Z-basis measurement, reset and classical-bit-controlled X. There is
no second quantum semantic IR. The ordinary FSL compiler, SSA/type validator,
package reader/writer and diagnostic FIR consume these operations. Packages
containing ideal quantum operations select v12 (tag 23). Declared noise channels
select v13. Earlier sources retain their package version and bytes. New readers
accept v1..v14; v12 packages cannot contain noise, and relaxation requires v14.

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

Strict reference admission: acyclic canonical blocks, <=256 ops, <=64 classical
integer values (1..64 bits), <=12 total measurement/reset/noise operations (<=4096 enumerated
branches), <=8 actual state qubits. Block parameters use parallel edge transfer.
All syntactic paths are checked, including untaken cycles, before execution. Canonical FIR can preserve larger logical
indices while the reference and OpenQASM projection refuse unsupported extents.
Mixed CPU/GPU effects are preserved by canonical validation when well typed,
but not executed by this quantum reference. Cycles require the bounded entry
point described below. Classical backends
C/Rust/Go, CUDA/PTX and native JIT/AOT refuse quantum input. Registered source
backends have to explicitly declare the quantum projection context.

This is f64 numerical evidence. Rounding, underflow and tolerance matter; no
symbolic identity proof, exact amplitude arithmetic or calibrated hardware model is
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

## Explicit channels and acyclic CFG

FSL accepts `quantum.bit_flip q,n,d;`, `quantum.phase_flip q,n,d;` and
`quantum.amplitude_damping q,n,d;`. Each integer is unsigned 8-bit; probability
p=n/d requires d>0 and n<=d. No floating source parameter or implicit calibration
is stored. Bit/phase flip implement (1-p)rho+p U rho U† for U=X/Z. Amplitude
damping uses K0=diag(1,sqrt(1-p)) and K1=sqrt(p)|0><1|. Local channels act on the
joint state and preserve the partner's reduced density under a trace-preserving
local operation. These are declared mathematical models, not inferred hardware
noise. No environment outcome becomes a measurement record or classical SSA bit.

Existing `block`, `branch`, `branch.if`, parameters and `return` are reused.
Classical conditions select execution paths; quantum state is carried without
measuring at a join. Paths remain separate ensemble contributions until densities
are summed for observation. The initial v13 experiment admitted only constants
and unsigned u1 classical values; the integer extension below broadens execution.
Quantum control-flow superposition, dynamic allocation and physical scheduling
remain unsupported.
OpenQASM import/export retains its earlier linear subset and **rejects** CFG and
noise; it never silently drops these effects. No classical source/native backend
is enabled for them.

The channels/cfg research experiment separately compares 33 authored programs
with a dense Kraus oracle over basis, complex product and entangled inputs. The
original seven ideal-circuit artifacts remain a separate pinned experiment.
Channel formula reference: [Qiskit Aer standard error definitions](https://github.com/Qiskit/qiskit-aer/blob/main/qiskit_aer/noise/errors/standard_errors.py).
No SDK code is copied or linked into the executor.

## Bounded cyclic execution and historical idle-model parameters

`simulate_bounded(body, initial, QuantumLimits)` admits cycles in existing FIR
blocks, including the integer subset described below. It counts global operation
and terminator steps, per-path split cost and total retained branch capacity.
Bounds are steps <=1,000,000, splits <=12 and branches 1..4096; default 65,536 /
12 /4096. Strict `simulate` continues to reject cyclic graphs before execution.
Both paths execute the same operation helper, with no second semantic IR.

A stopped `QuantumRun` retains completed branches and a frontier with canonical
block/op positions, latest dynamic SSA slots, measurement history and path split
counts. Repeated execution of a static measurement ID appends a new measurement
event; its SSA slot holds the latest dynamic result. Edge arguments transfer in
parallel. No terminal density is returned for an incomplete run, no probability
is discarded or renormalized, and `into_complete` refuses. Frontier retention is
diagnostic/API evidence; a resumable package format is not implemented. The
worklist is deterministic depth-first, so partial completed probability depends
on the traversal/budget and is not an unbiased sample or termination proof.

```sh
fslc simulate-quantum-bounded PACKAGE NAME QUBITS INITIAL STEPS SPLITS BRANCHES
```

The CLI emits a typed-status JSON envelope. Incomplete output has `result:null`
and a nonzero exit status. Gates and terminators consume steps; reset/measurement/
rational noise consume one split; relaxation below consumes two. Admission of
branch capacity and split cost is atomic before that effect. The canonical FSL
and FSLC CFG encoding is unchanged by the executor.

`quantum.relaxation qubit,duration_ps,t1_ps,t2_ps;` adds FSLC v14. All times are
unsigned 64-bit picoseconds. Require T1,T2>0 and T2<=2*T1, checked without integer
overflow. The model assumes zero-temperature Markovian idle evolution, with
excited population decay exp(-t/T1) and off-diagonal decay exp(-t/T2). Native
execution composes amplitude damping and phase flip on the joint state, using
`expm1` for small rates. No environment event becomes a measurement bit. Wire
tag 23 / subtag 10 / payload 0 is followed by duration/T1/T2 as little-endian
u64; older versions refuse it. Ideal/rational-noise sources keep previous bytes.

The research experiment imports an archived IBM Manila property snapshot dated
2024-05-27, not live calibration. Unit conversion explicitly rounds within 0.5ps,
records residuals and checks resulting density differences against the original
parameters. Physical snapshot qubit i is modeled in logical reference slot 0;
an auxiliary joint-state slot is only a numerical test spectator. No physical
routing, scheduling, QPU execution, readout/gate-error/crosstalk fit, finite-
temperature model, calibration measurement or error correction is provided.
OpenQASM and classical/native codegen still reject these effects.

A separate owned FSL data profile `qisa-qelab-9b024494.fsl` decodes strict canonical
QWAIT/QWAITR encodings and opaque quantum bundle fields from a pinned reference
assembler configuration. Every body is explicitly unsupported. This is binary
layout evidence only: timing queues, physical opcode meaning and pulse execution
are not implemented. No reference assembler is linked or invoked by Fission.


## Integer-controlled CFG (2026-10-17)

Both reference entry points now admit the existing `IntConstant`, `IntAddWrap`,
`IntBinary` (sub/and/or/xor), `IntCompare` (eq/ult/slt) and `IntConvert` operations.
Integer values use `u64` bit patterns masked to the declared 1..64 bit width;
signed values are two's complement. Existing scalar comparison/conversion/binary
helpers own the semantics. Quantum measurement, `XIf` and conditional terminators
still require unsigned u1. `QuantumBranch.values` changes from `Option<u8>` to
`Option<u64>`; measurement event bits remain u8. JSON clients must preserve u64
integers exactly rather than rounding them through a floating number type.
No op tag or wire version changes: the existing package format already holds
these canonical operations. Older quantum packages retain their bytes.

`simulate` continues to reject every syntactic cycle. `simulate_bounded` follows
arbitrary validated block edges within its admitted operation subset, without
requiring a structured loop shape; step/split/branch limits still preserve an
incomplete frontier. Each static SSA slot retains its latest dynamic definition.
Unexecuted slots are None; cross-block reads still require explicit parameters.
Counters transfer through the same parallel edge argument mechanism as bits.
This adds neither memory, calls, dynamic qubit addressing nor pulse scheduling.

The authored `quantum-integer-cfg.fsl` fixture covers 42 bodies: 16 decrementing
counters (including zero iterations), two signed loops, 18 arithmetic boundary
cases, four extension/truncation cases, one measurement-selected counter and one
cycle with two entry blocks. A separate research oracle uses Python integers,
closed-form loop summaries and unnormalized dense density operators. It compares
complete classical slots, ordered measurement history, conditional density and
probability, including complex product and entangled inputs. Finite evidence
cannot establish arbitrary program equivalence or hardware correctness.

OpenQASM projection remains restricted to linear u1 constants/measurement and
supported quantum effects. Integer arithmetic refuses even for u1 values; an
explicit preflight prevents unsupported operations reaching emission. There is
no currently connected QPU for this research session. Real QPU execution and
current-device calibration/correction remain unverified.

## IBM physical-circuit projection (2026-10-17)

`quantum::ibm::emit` projects a checked linear subset of the same canonical FIR
into physical OpenQASM 3, with an explicit injective logical-to-physical layout,
computational-basis input index and one X/Y/Z terminal observation per logical
qubit. Input preparation is explicit reset followed by X for set input bits.
Terminal observations and preparation are wrappers, not inferred FIR effects.
Measurement record slots map back to canonical SSA IDs. FIR operation index
comments retain source operation ownership in the emitted sequence.

The native basis is RZ/SX/X plus directed CX or CZ on supplied physical tuples.
H becomes RZ(pi/2);SX;RZ(pi/2), Z becomes RZ(pi), and CX on a CZ target becomes
H(target);CZ;H(target). These preserve the instrument up to unobservable global
phase; no coherent control of these gate decompositions is admitted. CZ can use
a reported reverse tuple because CZ is symmetric; reverse CX never substitutes
for forward CX. Missing couplings refuse without routing. A selected entangler
is not automatically replaced by another gate family.

Unsigned u1 constants fold into classical X conditions; measurement conditions
become single `if` statements. Blocks (including acyclic ones), arithmetic,
noise, dynamic addressing and unknown effects refuse before an output file is
written. There is no ECR lowering, scheduler, pulse or device noise model.
The old logical OpenQASM interface and package bytes remain unchanged.

```sh
fslc emit-ibm PACKAGE bell 7,3 cz 3:7 0 XX /tmp/bell-xx.qasm
```

`Target` is a caller-declared contract. Native emission alone does not validate a
live backend, authenticate or submit. The separate fission-research
`tools/ibm_quantum.py` adapter checks snapshot gate availability and coupling,
prepares a reviewable request/reference bundle, and rechecks the Open Plan and
live target before an explicitly invoked submission. It uses Python stdlib REST,
not a core SDK dependency. Hardware API acceptance and real-device comparison
require account access and remain unverified in this session.
