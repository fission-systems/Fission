# Owned x86 decoding and canonical FIR output

Status: experimental, 2026-10-11. Work stays on `codex/fsl-jvm-iadd-parity`.

`fsl-x86` is a separate experimental entry point behind the `x86` feature. It
reuses the canonical Fission loader for selected binary spans and the existing
FIR semantic validator, evaluator and C/Rust emitter. No external instruction
decoder dependency is introduced: no iced-x86, Capstone, Zydis, SLEIGH runtime or
P-code conversion. Existing workspace dependencies remain.

## Pipeline and ownership

`specs/x86/scalar.fslx` declares opcode/mask/group-extension/operand-form/body
rules. `scalar-bodies.fsl` defines the sole canonical FIR semantics. The offline
compiler validates both and creates `.fslxc`: an 80-byte header (`FSLXPKG1`,
two little-endian u32 lengths, two SHA-256 digests), compiled rules and a `.fslc`
body package. The runtime reads compiled rules, not JSON or source grammar.
Format v1 limits total size to 16 MiB and requires exact EOF and valid hashes.

The owned decoder implements bounded prefix/opcode/ModRM/SIB/immediate
mechanisms. Encoding data selects FIR bodies; arithmetic, flags, LEA address
calculation and RET effects live in those bodies. Adding unsupported operand
forms still requires extending the generic decoder. This is a limited DSL
expressiveness prototype, not complete SLEIGH migration.

`X86Program` contains immutable package data and original-byte instances, with
address/offset/length and canonical body references. Its 128-bit normalized
operand tokens are internal bindings, not guest encodings. Variable-length
origins are separate from semantic bodies; no second semantic IR is introduced.
The original fixed-width `FirSequence` APIs and earlier package formats remain.

`.fslc` v9 adds generic subtraction/bitwise operations; v10 adds
`MemoryLoadLittle`. It takes an unsigned u64 guest address and produces an
unsigned 8/16/32/64-bit value from an explicitly supplied bounded byte window.
It never dereferences a guest address as a host pointer. Standalone state,
stack, wave and JIT paths without this memory context refuse the operation.

## Entry points

```sh
cargo build --locked -p fission-fsl --features x86 --bins
fsl-x86 compile scalar.fslx scalar-bodies.fsl scalar.fslxc
fsl-x86 decompile scalar.fslxc 32 0x1000 b807000000bb0500000001d8c3 c output.c
fsl-x86 decompile-binary scalar.fslxc add64.elf 0x401000 5 rust output.rs
```

`execute` and `execute-binary` append registers CSV, flags CSV, memory base,
memory hex and step budget to the same input arguments (without output layer
and path). CLI output is diagnostic text. Output C/Rust takes explicit state,
memory and budget; it is not a recovered source-level calling signature.

## Contract and limits

17 GPR slots: RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8..R15, constant zero.
12 flags: CF, PF, ZF, SF, OF, AF followed by six known bits. Input known bits
must be one. Logical operations mark AF unknown; its placeholder is not a
defined output. 16-bit writes preserve upper logical-slot bits, 32-bit writes
zero them. Legacy upper transport bits are not a full architectural snapshot.

The whole supplied window (1..65,536 bytes, at most 4,096 instances) is admitted
before output/execution. Unsupported/truncated/ambiguous encodings fail closed.
Direct branch targets must be instruction starts inside that window. A near RET
must exist. Dispatch may revisit instructions with a 1..1,000,000 step budget;
this is not inferred CFG structuring or arbitrary function discovery.

The loader owns architecture and executable section mapping. The selected span
must be file-backed, equal original file bytes, and free of overlapping
relocations. File and input-window hashes have separate meanings. No separate
function-facts database or architecture inference is created here.

Near RET reads an explicit readonly stack window, increments SP and writes exit
PC in FIR. 16-bit mode assumes flat SS.base=0. Segment state, MMU, guest faults,
privilege and concurrent memory are unsupported. Wrapper failure preserves
caller state by executing on temporary storage; this transaction is not CPU
exception ordering or atomicity. C caller storage must be valid and disjoint.

The current 46 rules and 63 bodies cover selected scalar register/immediate,
LEA and direct-control forms in 16/32/64 modes. Ordinary memory operands,
CALL/callee ABI, stack frames, indirect control, byte registers, most arithmetic
families, x87/SIMD/AVX, cspec/pspec/context and complete asset translation remain
unsupported. GPU memory and kernel ABI are outside this slice.

The research repository records authored templates, 78 source asset hashes,
1,380/1,440 migrated register views, three raw windows and two fixed Clang ELF
spans. Five FIR executions and ten warning-denied C/Rust source compilations
were observed locally. Existing unit suites were not rerun locally during this
slice. Artifact regeneration does not establish independent ISA execution
parity, emitted-source runtime parity or full function equivalence.
