# Separate displacement and literal operands

Experimental continuation, 2026-10-11, on `codex/fsl-jvm-iadd-parity`.

The x86 wrapper now recognizes `owned-x86-v2` / `FSLXPKG2`. The underlying
canonical FIR package stays v11: no new semantic operation, dependency or
architecture-specific arithmetic executor is introduced. `FSLXPKG1` retains
its binary encoding and normalized operand interpretation. V1 refuses memory
immediates; a v2 package carrying split bodies cannot be relabeled v1.

V2 retains old six-field bodies and adds a seven-field schema for memory
immediates. It binds destination/source/base/index, a 64-bit normalized address
addend (`displacement`, including resolved IP-relative addresses), scale, and
an independent 32-bit literal bit pattern. The split token uses bits 86..117
for the literal and bits 118..127 for the body selector. Fields cannot overlap
fixed selector bits. These are normalized semantic tokens, not machine bytes.
Raw bytes, original lengths, file mapping and addresses remain separate origins.

The optional `scalar-immediate` pool has 48 encoding rules and 150 FIR bodies,
adding nine MOV C7 memory bodies (three operand widths × three address widths)
to the 141-body stateful pool. Decoding reuses the generic ModRM/SIB address
reader and consumes the complete immediate before computing the next-IP address.
The 64-bit store sign extends imm32 in authored FIR; 16/32-bit writes truncate
and preserve neighboring bytes. MOV preserves flags. No new FIR primitive is
needed: constants, truncation, xor/subtract and the existing ordered store suffice.

Some width/address combinations cannot occur in a selected mode; body count is
not opcode count. Other memory ALU immediate destinations have no authored body
and refuse. LOCK/REP/segment semantics, byte/high-byte registers, CALL/callee ABI,
full CFG recovery and complete x86 migration remain outside this slice. The
existing flat memory/transaction/scratch contract is unchanged.

Five native tests cover version round trip/downgrade refusal, independent values,
16/32/64 modes, negative displacement, sign extension, operand/address prefixes,
SIB and IP-relative addressing, malformed optional-body schemas, truncation,
unsupported effects and late rollback. The research repository additionally
locks three actual Clang ELF spans containing 16/32/64-bit C7 writes at RDI+8.
It compares 768 modeled FIR states, 6,144 C/Rust O0/O2 success/rollback rows and
768 original-byte cases through Rosetta64 locally. Original-byte observables are
result, six defined arithmetic flags, two external words, preserved RBP and SP
balance; not a full hardware register/stack snapshot or whole-function proof.

The actual SLA/canonical evaluator baseline agrees on modeled GPRs, return PC,
160 memory bytes and five arithmetic flags. AF differs 96/256 times per fixture
because the existing reference spec omits AF. This is not evidence of an intrinsic
P-code limit or broader FIR accuracy improvement. Controlled matched-coverage
ablation remains future research.
