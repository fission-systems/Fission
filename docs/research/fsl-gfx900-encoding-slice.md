# FSL GFX900 encoding slice

Date: 2026-10-01. Status: implemented fixed-width encoding frontend; GPU state
semantics, variable-length extensions, and whole-kernel recompilation remain open.

## Implemented

The same text FSL parser now accepts a fixed-width `encoding` declaration as an
alternative to `opcode`. Each compiled instruction owns an `Encoding` plan and
its canonical FIR body. This plan describes bytes and fields, not another
semantic IR tier. There is no NIR/HIR split or dependency on vendor code.

The plan supports 8, 32, 64, and 128-bit words, guest byte order, mask/value
matching, contiguous fields up to 64 bits, and excluded field values. Package
validation rejects mixed widths, ambiguous overlapping patterns, overlapping
fields, width errors, impossible mask/value pairs, and field exclusions that
make a pattern unreachable. Overlap checks are deliberately conservative:
exclusions do not establish disjointness between otherwise overlapping masks.
One profile still contains at most 256 instructions.

`.fslc` v2 stores these plans in the binary format. V1 packages retain byte-opcode
compatibility and remain readable. All new mask/value integers are 128-bit
little-endian package fields, independent of the guest instruction byte order.
This is an experimental format change and Rust API change: `encoding` replaces
the instruction's byte-only `opcode` member. Native JIT/AOT generation explicitly
rejects profiles outside its current exact-byte-opcode scope.

`decode_bytes` checks the supplied profile identity and returns the instruction
index, consumed raw bytes, and raw field values. It checks input length before
reading. Architecture detection and ELF/cubin metadata routing are not provided
by this API; a caller must choose the correct profile from external evidence.

`reencode` re-decodes an observation and checks its identity, raw bytes, index,
and fields before applying edits. It rejects duplicate/unknown edit names,
overflow, fixed-bit changes, and excluded selectors, then preserves all other
bits. Raw reconstruction and behavioral recompilation are separate claims.

## GFX900 profile

[`amdgcn-gfx900.fsl`](../../crates/fission-fsl/specs/amdgcn-gfx900.fsl) ports the
existing research slice for `s_mov_b32`, `v_add_u32_e32`, `s_barrier`, and
`s_endpgm`. Evidence distinguishes Vega documentation, recorded LLVM probe bytes,
and pinned LLVM encoding definitions. SOPP immediate fields are constrained to
zero through the fixed mask. Literal, SDWA and DPP extension selectors are
excluded where applicable. Raw selectors are not yet resolved into register
banks, constants, or special-register operands, and their other architecture
legality rules are not claimed to be complete.

All four semantic bodies are `semantics unsupported;`. This becomes an explicit
`FirOp::Unsupported`, not an empty body. Validation forbids mixing it with other
operations; reference execution and executable C/Rust output refuse the body.
Diagnostic FIR still displays the missing semantic support.

## Observed local behavior

The compiler built and the following CLI operations completed:

- Compiled the four-instruction GFX900 text profile to a binary v2 package.
- Decoded `01 05 00 68`: raw source0=257, source1=2, destination=0; consumed four bytes.
- Re-encoded destination=3 as `01 05 06 68`, retaining the remaining bits.
- Compiled the existing JVM `iadd` profile to v2 and obtained its four JIT FIR records.
- Executed the JVM integer contract for `0x7fffffff + 1`, yielding `0x80000000`.
- Read the previously created v1 JVM package successfully.

These are focused CLI observations, not a complete new regression run, an ISA
coverage measurement, or GPU hardware execution evidence. Existing instruction
recompilation tests predate this encoding extension. No GPU equivalence result
is claimed. The 64/128-bit infrastructure has not yet been exercised against a
NVIDIA or Intel corpus.

## Next implementation boundary

Resolve architecture-scoped operand selectors, then add canonical FIR
register/state effects and an explicit GPU reference-state contract. Start with
scalar move and integer vector addition under EXEC, preserving inactive lanes.
Wave termination and workgroup synchronization need distinct contracts; a
barrier cannot be implemented as ordinary sequential host arithmetic. Conditional
extension lengths and split fields are still required before broad ISA decoding.

## Reference inputs

- [Vega ISA document 70656](https://docs.amd.com/v/u/en-US/vega-shader-instruction-set-architecture), dated 2020-01-27.
- [LLVM 22.1.0 VOP2 definitions](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.0/llvm/lib/Target/AMDGPU/VOP2Instructions.td).
- [LLVM 22.1.0 operand selector definitions](https://github.com/llvm/llvm-project/blob/llvmorg-22.1.0/llvm/lib/Target/AMDGPU/SIDefines.h).
- Existing local research `experiments/gpu/amdgpu-gfx900/probe.text.bin`, SHA-256
  `bf545049f82875a073eb48c2e2bd5f0e58ae4af81b156ed10c3bdac618b17a18`.
