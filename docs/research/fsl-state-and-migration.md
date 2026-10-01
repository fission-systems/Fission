# FIR state expansion and migration contracts

Date: 2026-10-01. Experimental branch `codex/fsl-jvm-iadd-parity`.

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

## Next EXEC/lane contract

This remains design work. There is one semantic FIR; uniform and per-lane state
are effect domains in that FIR, not additional NIR/HIR layers.

The next state schema must identify a wave size, scalar bank, lane register
bank, EXEC bit vector and special flags. A lane-scoped read produces explicitly
lane-indexed values; a masked write updates exactly the lanes selected by an
EXEC snapshot at the declared effect point. Inactive lanes preserve their
registers. Scalar effects run once per instruction, including when EXEC is
zero. Lane widths/mask bounds and all affected bank sizes must be validated
before mutation. Source/destination aliasing must preserve operand capture.

The first vector acceptance gate should be GFX900 `v_add_u32` with all-zero,
all-active, sparse and highest-lane masks, scalar broadcast and lane sources,
aliased operands, and invalid masks/banks. The model must state wave64 explicitly;
wave32 support belongs to another architectural profile. Do not infer barriers,
memory ordering, VCC behavior or divergence from a masked integer-add test.

## Migration owners and first artifacts

| Input | First admitted output | Current limit |
|---|---|---|
| `eBPF_le.slaspec` + `eBPF.sinc` | One executable FSL ADD64 register leaf | Explicit source shape; no general preprocessor |
| BPF/eBPF `cspec` | Typed FSL ABI metadata | Symbolic names; no allocator/register-layout linker |
| `eBPF_le.sla` | Structural evidence + live legacy oracle | Direct decision/template-to-FIR conversion unsupported |

The offline importer belongs to the research repository; the FSL parser,
validated FIR, source output and typed ABI model belong to `fission-fsl`.
Imported data retains source snapshot/path/hash and Ghidra attribution. Runtime
execution of the migrated instruction does not depend on SLEIGH or a vendor
implementation. ABI metadata currently has a source parser but no binary package
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

1. Model FSL register layout and overlapping slices from SLEIGH/SLA; link ABI
   names to identities and widths. Preserve byte order and address-space units.
2. Add grouped ABI slots, stack storage, join storage and datatype allocation
   rules from the recorded cspec refusal inventory. Preserve rule ordering.
3. Lower SLA decision trees/context updates and bound ConstructTpl effects into
   FSL/FIR with an explicit supported-operation registry. Unknown effects must
   retain their source IDs and stop executable conversion.
4. Compare source-derived and SLA-derived candidates on the same byte corpus,
   with default/context state and instruction length as observables; only then
   expand CPU/VM profiles and connect whole-function recompilation.

Replacing imported grammar does not remove the evidence/source obligations.
Full replacement requires measured coverage for decoding, instruction effects,
register layout, ABI, context, control flow and recompiled behavior.
