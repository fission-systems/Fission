# FIR state expansion and migration contracts

Date: 2026-10-02. Experimental branch `codex/fsl-jvm-iadd-parity`.

## Executable carry-input contract

GFX900 `s_addc_u32` reads SGPR 0..95 and the old SCC, computes an unsigned
33-bit total, writes low 32 bits to SDST, and writes carry-out to SCC. SCC is
flag slot 0. Source/destination aliasing is admitted: all operand values are
captured before writes. Invalid state is rejected before any mutation.

FIR adds a typed u1 flag read and two arithmetic projections of the same total:
`uN.add.carry` returns the low N bits; `int.add.carry.in` returns the unsigned
carry. Both require same-width unsigned operands and a u1 carry input. Source,
package and output boundaries enforce SSA/type validation. Package V4 stores
these operations; old V1/V2/V3 artifacts stay readable. A C emitter uses only
unsigned arithmetic and comparison, avoiding undefined overflow and 128-bit C
extensions. The reference evaluator uses u128; outputs are compared against
an independently calculated quotient/remainder oracle.

The new carry regression runs 1,030 states per width (1, 8, 16, 32, 64), with
20,600 C/Rust O0/O2 comparisons, and 1,060 chained 64-bit additions. The widths
other than 32 are synthetic generic-FIR contracts. This does not establish GPU
hardware, whole-function or kernel equivalence. JIT/AOT state lifting remains
unsupported.

## Executable EXEC/lane contract

The wave64 `v_add_u32` slice now implements this contract. There is one semantic
FIR; uniform and per-lane state are effect domains in that FIR, not additional
NIR/HIR layers. V5 adds lane mask/read/write operations and retains V1..V4 reads.

The state schema identifies a wave size, scalar bank, register-major lane
bank, EXEC bit vector and u1 flags. A lane-scoped read captures all lane-indexed
values at its ordered effect point; a masked write updates exactly the lanes selected by an
EXEC snapshot at the declared effect point. Inactive lanes preserve their
registers. Scalar effects run once per instruction, including when EXEC is
zero. Lane widths/mask bounds and all affected bank sizes must be validated
before mutation. Source/destination aliasing must preserve operand capture.

The first vector gate covers GFX900 `v_add_u32` with all-zero, all-active,
sparse and highest-lane masks, SGPR broadcast and VGPR sources, aliased operands,
and invalid banks/extents. There are 1,984 valid and 19 invalid output rows,
8,012 C/Rust O0/O2 comparisons. A separate synthetic four-lane gate checks
out-of-range masks and scalar effects with EXEC=0 in 16 output comparisons.
Wave32 support belongs to another architectural profile. Masked add does not
establish barriers, memory ordering, VCC behavior, EXEC updates or divergence.

`WaveContract` derives each SSA value's `Uniform`, `Mask(n)` or `Lanes(n)` domain.
Element widths/sign remain canonical FIR types. Masks are not arithmetic
integers; uniform effects cannot consume lane values. Generic addition combines
equal lane extents and broadcasts uniform operands. All mask reads in one body
must agree on extent. Current wave execution refuses mixed carry/stack bodies;
these refusal boundaries apply equally to the evaluator and C/Rust outputs.

## Migration owners and first artifacts

| Input | First admitted output | Current limit |
|---|---|---|
| `eBPF_le.slaspec` + `eBPF.sinc` | One executable FSL ADD64 register leaf | Explicit source shape; no general preprocessor |
| BPF/eBPF `cspec` | Typed FSL ABI metadata and strict layout linker | eBPF links; BPF stack-width gate refuses; no allocator |
| BPF/eBPF space/register declarations | Own `.fslregs` views and shared byte storage | Three explicit endian entries; declaration prefix only |
| `eBPF_le.sla` | Named registers + one direct bound ADD64 template → FSL/FIR | Bounded instruction decisions/BUILD/export/add; context/other effects refuse |

The offline importer belongs to the research repository; the FSL parser,
validated FIR, source output and typed ABI model belong to `fission-fsl`.
Imported data retains source snapshot/path/hash and Ghidra attribution. Runtime
execution of the migrated instruction does not depend on SLEIGH or a vendor
implementation. ABI metadata has a source parser and register linker but no binary package
or call-effect integration; this must not be called whole-ABI migration.

The eBPF state test checks 484 synthetic states and 1,936 C/Rust O0/O2
comparisons. `ebpf_add_leaf_oracle` checks the actual SLA's 121 register bindings,
instruction lengths and single IntAdd templates. It is a separate decoder/effect
oracle with shared upstream lineage, not independent eBPF verifier evidence.
Unused off/imm bits survive reencoding; R10 follows the source binding, and
the profile does not claim verifier legality for every decoded instruction.

The strict ABI importer scanned 110 cspecs and admitted two. Unknown attributes,
grouped entries, datatype rules, join storage, extensions, hidden returns and
injection snippets are recorded as refusals. The source's explicit unknown
cleanup must survive, rather than becoming numeric zero. Accepted byte sizes,
alignment entries, argument order and effects are checked by the native parser.

## Next migration gates

1. Extend the initial byte-addressed register layout/linker to GPU lane banks
   and direct SLA register metadata; preserve unsupported units as refusals.
2. Add grouped ABI slots, stack storage, join storage and datatype allocation
   rules from the recorded cspec refusal inventory. Preserve rule ordering.
3. Expand bounded SLA decision/template lowering to context and further effects in
   FSL/FIR with an explicit supported-operation registry. Unknown effects must
   retain their source IDs and stop executable conversion.
4. Compare source-derived and SLA-derived candidates on the same byte corpus,
   with default/context state and instruction length as observables; only then
   expand CPU/VM profiles and connect whole-function recompilation.

Replacing imported grammar does not remove the evidence/source obligations.
Full replacement requires measured coverage for decoding, instruction effects,
register layout, ABI, context, control flow and recompiled behavior.

## Register layout and ABI linking slice

`registers` owns a text `.fslregs` parser, address-space metadata and register
views. All admitted offsets/widths are bytes; non-byte address units are rejected.
Each space has explicit address width and byte order. The memory default is
preserved, but memory spaces are not allocated/emulated. Register names resolve
to identities in a particular layout, never process-global IDs.

`RegisterFile` stores bytes once per register space. Overlapping views share
storage, and a partial write preserves every byte outside that view. It does
not infer architectural zero extension or read-only register rules. The owned
layout and storage are private; invalid writes leave the file unchanged.
Metadata accepts 1..512-byte views; integer access is limited to 64 bits, and
the reference storage allocator refuses totals above 16 MiB.

`link_abi` retains original ordered metadata and resolves inputs, outputs,
preserved/clobbered registers and the stack register. It verifies entry widths,
memory references/ranges, duplicate identities and preserved/clobbered overlap.
The strict first stack gate requires register width = ABI pointer size. eBPF
links R1..R5, R0 and preserved R6..R10; R10 occupies register bytes 80..87.
The unchanged BPF source has a 4-byte RS and `pointer_size=8`, so linking refuses
with a diagnostic. This is a conservative supported-contract refusal, not a
claim that every ABI requires stack register width = default pointer size.

`execute_bound` projects byte storage into the existing canonical FIR evaluator,
using an explicit logical register/flag slot binding. It requires disjoint
bound views and exact integer widths. Same instruction operand selectors may
alias the same slot. Register-file view aliases are supported, but *inter-slot*
aliases are refused until ordered direct-storage FIR effects exist. Candidate
storage commits only after successful evaluation; PC/unbound bytes survive.
No second semantic IR, architecture dispatch, or package version was added.

The importer admits BPF LE (15 views, 15 overlapping pairs) and eBPF LE/BE
(12 views each). No BPF BE entry exists in the pinned corpus; its BE alias test
is synthetic. Native tests compare 968 bound leaf states against a byte-storage
oracle (484 per storage byte order), using the existing LE instruction profile
in both cases. They do not establish BE eBPF decoding. Existing C/Rust leaf
recompilation tests are rerun separately; the new storage adapter is reference
execution only. GPU register/lane layout, kernel ABI, allocation rules, calls
and further SLA semantic lowering remain subsequent gates.

## Direct SLA symbol/template migration slice

The offline `sla_migrate.py` reads packed SLA v4 directly. It does not read
Sleigh source grammar or invoke the legacy decoder. Format identifier facts
and ordered template fields are referenced to hashed Ghidra 12.0.4 files.
The generated instruction and layout are ordinary owned FSL text; their native
compiler/evaluator/recompiler has no SLA runtime dependency.

The first registry admits ConstructTpl BUILD of a pure register export followed
by handle-based INT_ADD. BUILD opcode 60 here is a compiler directive, not an
executable phi. Constructor/operand IDs resolve through the symbol table;
selectors, widths, holes and byte constraints are derived from the SLA. No
mnemonic, constructor number, source line or known opcode byte selects the
semantic body. The final first-slice gate requires one admitted root variant.

Decision constraints include ancestor instruction bits. Pair order is retained:
an earlier overlapping root pair refuses conversion, and later overlaps are
recorded. In this input, the ADD root pair precedes a broader conditional-jump
pair. Subtable register export resolves into the existing FIR reads/wrapping
addition/write. Constant export, unknown opcodes/atoms, dynamic offsets, context
effects, sections, delay slots, memory and control flow do not become executable.

eBPF LE has 12 named register symbols and 129 constructors (98 root). One root
register variant and its pure export dependency are consumed; other constructors
and the immediate branch have IDs, source metadata/opcodes and refusal records.
This is not general decoding or support for 129 instruction semantics.

The SLA-derived layout, encoding and canonical FIR match the earlier source
candidate. All 65,536 opcode/selector prefixes agree; both admit 121. The same
SLA-derived FIR passes 484 synthetic states / 1,936 C/Rust O0/O2 comparisons
and 484 bound-byte-storage states. A separate live legacy SLA oracle checks
121 bindings/lengths/IntAdd effects; the two paths share source lineage.
Eight mutated semantic/selector/priority cases refuse. Native crate tests total
32, with the previous execution and recompilation gates rerun. Whole-VM,
whole-function and GPU/kernel execution remain unsupported.
