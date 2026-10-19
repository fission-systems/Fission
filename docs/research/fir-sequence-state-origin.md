# Sequential FIR instances: state and byte origins

Implemented experimental slice: 2026-10-04. `fission-fsl::sequence::FirSequence`
composes an explicitly supplied, bounded instruction window. It does not discover
functions or follow guest branch targets. This is separate from product
`fission-analysis-db` loader facts: no binary loader/symbol/program metadata is
added or inferred by this research execution plan.

## Semantic ownership

One immutable `FslcPackage` retains its instruction value/op/block tables. Each
`FirInstructionInstance` references a decoded instruction index and records its
byte origin. Repeated instances reuse the same body. Value IDs remain local to
a body invocation, with fresh SSA storage each time. Only the ordered logical
register/flag state crosses instance boundaries. This is a container around
canonical FIR bodies, not a new semantic IR, NIR/HIR split or flattened op copy.

Input profile selection is explicit. Every fixed-width word must decode and its
selected scalar body must pass the existing state/acyclic validator. All raw
selector/flag indices, including untaken block paths, must fit the declared
state contract. Unsupported, cyclic, stack and lane bodies are refused during
composition. The whole window must be covered; no truncated suffix or unknown
word is silently dropped. Unselected unsupported bodies may remain in a package
without becoming executable.

Only sequential inter-instruction flow is represented. A `FirTerminator` still
branches within one instruction's semantic body; its return completes that
body and advances to the next supplied word. It is not a guest return, PC write
or edge to another machine address. Real branch semantics require an explicit
inter-instruction control contract before admission.

## State contract

`SequenceStateContract` specifies exact counts of logical u64 register slots and
u1 flag slots. Each bank has at most 4,096 slots; empty banks are allowed when the
selected bodies do not use them. Value widths and narrow whole-slot writes keep
the existing instruction semantics. Register layout aliases, ABI allocation,
partial byte views, memory, EXEC/lane state and effect SSA are not inferred.

The reference and C/Rust wrappers validate bank lengths and flag values before
execution. They copy state into temporary banks, run each existing body in
order, and commit only after success. A failed status/error does not commit a
prefix. This wrapper transaction is not guest fault ordering, a CPU atomic
operation or a concurrent memory model. The current admitted primitives have
no dynamic guest faults; late body failures are a defensive fallback, not a
new simulated guest exception. C callers provide disjoint valid arrays and
nonnull pointers, including dummy storage for an empty bank; Rust borrowing
supplies disjoint slices. C/Rust temporary banks use at most 64 KiB of local
storage, excluding each body's own value frame. This is a correctness prototype;
no performance gain is claimed.

## Origin contract and `.fslseq` v1

Every instance records byte address, offset relative to the supplied input
window, byte length, exact raw bytes and the canonical instruction index/raw
fields. Input-window and compiled-package SHA-256 hashes are stored once in the
owning sequence. The referenced instruction retains its specification evidence.
Diagnostic FIR exports all of these; C/Rust preserve derived origin comments
and reference the same body pool. Arbitrary evidence strings stay in diagnostic
metadata rather than executable source comments.

The caller supplies the base address and state ABI. The window hash is not a
whole-file hash; the package hash is not a hash of original `.fsl` source text.
No file-offset/virtual-address mapping, relocation, source path or symbol facts
are inferred. Hashes detect content mismatch and are not authenticity evidence.
A valid new base/count header describes a different caller-supplied contract;
it is not authenticated by the package/window hashes. The research lock hashes
the entire container too.

The 96-byte header is little endian:

| Offset | Field |
|---:|---|
| 0 | 8-byte `FSLSEQ\0\x01` magic/version |
| 8 | Base byte address, u64 |
| 16, 20 | Register and flag counts, u32 each |
| 24, 28 | Embedded package and input lengths, u32 each |
| 32 | SHA-256 of canonical `.fslc`, 32 bytes |
| 64 | SHA-256 of input window, 32 bytes |
| 96 | Exact `.fslc`, then exact window bytes |

Bounds: 1–4,096 complete fixed-width instructions, 64 MiB total container, and a
representable end-exclusive address. Loading checks lengths/EOF/hashes, parses
the package and derives all instances/origins again through the same constructor.
Existing `.fslc` versions and byte layouts do not change. There is no JSON runtime
format or external SLEIGH/P-code dependency. `sha2` 0.10.9 supplies SHA-256.

## Validation

`tests/sequence.rs` checks framing/truncation/trailing bytes/hash corruption,
address overflow, bank/selector bounds, late unsupported word refusal,
cycle/stack refusal, immutable package evidence identity, 4,096-step bounds,
native CLI composition/inspection/execution and exact empty-bank contracts.

- Two supported scalar bodies `s_add_u32` / `s_addc_u32` update low/high words
  and carry: 1,060 separate reference/u128 oracle states.
- C/Rust O0/O2: 10 scalar configurations (five widths × two alias placements),
  1,320 reference/oracle and ABI failure rows, 5,280 comparisons. Widths other
  than 32 are synthetic primitive checks, not GPU ISA support.
- Three register CFG instances: 256 inputs plus three failed ABIs, 1,036 C/Rust
  comparisons, with later instances reading earlier writes and full storage
  checks.
- Explicit zero-register and zero-flag banks: two rows / eight comparisons.

The new sequence source total is 6,324 comparisons. The research reproducer
independently reads two envelopes, compares six output hashes/bytes, executes
112 CLI oracle states and checks four failed states. Scalar fixture bytes are
self-authored instruction windows. These are not GPU hardware/emulator,
whole-function equivalence or real binary function lifting evidence.

```sh
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/amdgcn-gfx900-scalar-sequence.fsl /tmp/scalar.fslc
cargo run -p fission-fsl -- compose-sequence /tmp/scalar.fslc \
  amdgcn.gfx900.scalar.sequence 0x1000 7 2 0002048001030582 /tmp/chain.fslseq
cargo run -p fission-fsl -- inspect-sequence /tmp/chain.fslseq
cargo run -p fission-fsl -- execute-sequence /tmp/chain.fslseq \
  4294967295,0,1,0,99,99,77 1,1
cargo run -p fission-fsl -- emit-sequence /tmp/chain.fslseq c /tmp/chain.c
```

Output: `registers=[4294967295, 0, 1, 0, 0, 1, 77] flags=[0, 1]`.

Loops, GPU CFG/divergence, memory/calls/exceptions, binary function lifting,
mixed profiles/variable-width streams, guest PC changes and sequence native
Cranelift JIT/AOT remain unsupported. CUDA/PTX sequence output refuses.
