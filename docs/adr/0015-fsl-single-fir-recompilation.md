# ADR 0015: One canonical FIR with correctness and recompilation outputs

Status: accepted for the experimental `fission-fsl` direction

Date: 2026-10-01

## Context

The FSL/FIR research direction is authorized to replace Fission's decompiler
architecture and ultimately remove SLEIGH. Its acceptance criteria prioritize
semantic correctness and recompilation. Existing PreHIR/NIR/HIR product
contracts describe the legacy pipeline; they do not define the new FIR model.

## Decision

`fission-fsl` owns one canonical typed FIR. The new pipeline will not introduce
separate NIR and HIR semantic representations. Instruction effects, function
control flow, value definitions, memory/address-space state, exceptions, and
architecture extensions belong in FIR as they are implemented. Inferred types,
names, recovered objects, provenance, and structuring facts enrich that same
representation with evidence. Analysis indexes and output documents are views
or derived facts, not independent semantic truth.

All output layers consume the same validated FIR: diagnostic text, compilable
C, compilable Rust, future VM/ISA outputs, and native compilation. A writer
cannot rewrite effects or silently replace unsupported operations. Reordering,
elimination, and recovered abstractions require a semantic-preservation
argument at the FIR owner. Readability does not justify weaker semantics.

Recompilation success requires more than compilable text. The rebuilt artifact
must preserve the admitted observable contract: returned values, memory and
state updates, evaluation order, control-flow outcomes, exception behavior,
and external/ABI effects as supported. Unmodeled effects are unsupported,
not implicitly correct. Identical binary bytes, source reconstruction, and
behavioral equivalence are distinct claims. The default research target is
behavioral equivalence; byte identity can be a separate encoding round-trip gate.

## Implemented boundary

The current canonical instruction body is `CompiledInstruction::{values, ops}`
with `FirOp` operations. Its portable serialization remains `.fslc`. Native
decoder/lifter records are an ABI projection of FIR, not a second IR. C and
Rust execution source now consume that same body through `emit_instruction`;
`execute_instruction` is the reference evaluator for the admitted domain.

That domain is ordered VM stack effects and fixed-width wrapping integer
addition, with widths 1..=64 for executable outputs. Stack slots hold untagged
`u64` bit patterns, pop truncates to the declared width, and additions wrap
modulo that width. Signedness stays in FIR; output arithmetic uses unsigned
storage to avoid C signed-overflow undefined behavior. There is no assumption
that a signed machine overflow should trap.

Status 0 is success, 1 is stack underflow, 2 is insufficient capacity, and 3 is
invalid input depth. Stack requirements come from all ordered effects,
including transient growth, and are checked before mutation. Failure preserves
the active stack. Inactive slots after success are outside this contract.
These statuses are a tooling ABI, not modeled JVM verifier or exception behavior.

FIR validation checks dense SSA identities, single definitions, uses after
definition, integer widths, and matching wrapping-add types. It applies to
source compilation, binary package reading/writing, native lifting, and output
emission. FIR widths above 64 remain representable; the execution outputs
reject them explicitly. C pointers must identify valid non-null storage for
the advertised capacity, with depth storage disjoint from stack storage;
memory safety of arbitrary foreign caller pointers
is not something the generated C function can establish.

## Validation and consequences

The recompilation gate builds emitted C11 and Rust with optimization levels
0 and 2 and executes the resulting programs. It compares status, active depth,
and active stack against the FIR evaluator for signed/unsigned widths
1, 8, 16, 32, and 64, wrapping boundaries, unrelated stack prefixes, malformed
depths, underflow, and transient capacity requirements. A separate wide-integer
modulo oracle checks arithmetic. Malformed SSA/type artifacts are rejected.

These are instruction-contract tests. They neither prove universal equivalence
nor establish whole-program decompiler quality. Native memory operations,
register models, CFG/phi edges, calls/ABI, exceptions, GPU execution masks and
barriers, VM frames, dynamic operands, and target-specific re-encoding remain
future work. Each domain needs explicit observables and corpus evidence.

Migration can replace existing owners and APIs once a corresponding FIR-native
path has demonstrated coverage. A legacy adapter must declare information loss
and reject unsupported effects. A mechanical rename of NIR/HIR or conversion
of presentation trees does not complete this migration. The accepted end state
is FIR as the semantic owner, with FSL-owned specifications and multiple outputs.
