# fission-fsl

Fission-owned research compiler for FSL instruction definitions and typed FIR.
This crate is an independent vertical slice toward replacing SLEIGH in the
Fission product path; it does not yet replace any existing architecture
frontend.

One typed FIR is the semantic owner. Output layers consume that same body;
the experimental direction has no NIR/HIR split. The priorities are accuracy
and behavioral recompilation. See [ADR 0015](../../docs/adr/0015-fsl-single-fir-recompilation.md).

## Current slice

- `.fslc` v6 preserves typed block parameters, explicit branch/conditional branch
  and return terminators. `int.const` and `int.eq`/`int.ult`/`int.slt` are shared
  FIR operations. Reference/C/Rust execute acyclic integer/stack control bodies
  with equal stack deltas at joins/returns. Cycles are representable but this
  backend refuses them. Other backends refuse control bodies. See the
  [structured control contract](../../docs/research/fir-structured-control.md).

- Owned `.fsldb` v1 prototype-candidate catalog reader and exact symbol query,
  independent of FPK. Type spellings remain unresolved; missing variadic
  evidence is unknown. This does not replace the product signature loader or
  supply callable ABI semantics. The offline `.fslib` TOML compiler/importer
  lives in `fission-research/tools/fsl_library_migrate.py`. The three-row fixture
  is self-authored (all-zero provenance commit denotes a synthetic fixture).

- Dedicated `.fsl` text input with exact byte opcodes and fixed-width
  8/32/64/128-bit mask/value encodings, raw bitfields, and excluded selectors.
- Required per-instruction evidence fields.
- Typed integer values and a small VM-stack/wrapping-add FIR dialect.
- Versioned portable binary `.fslc` output. JSON is not used by the compiler,
  package loader, or native path.
- Cranelift JIT and host-native relocatable object output for the same
  decode-and-lift function. Both emit compact FIR records; neither executes
  guest instructions.
- A reference instruction evaluator and compilable C11/Rust execution outputs
  directly from that FIR. These update guest stack state, and the recompilation
  gate builds and runs both languages at optimization levels 0 and 2.
- Generic register-state FIR effects: register reads/writes, one-bit flag
  writes, and unsigned add-carry. The first executable GPU slice maps GFX900
  SGPR selector fields to register slots and flag slot 0 to SCC. Its reference
  evaluator and C/Rust projections are covered by an independent widened-sum
  oracle; native Cranelift lifting still rejects state effects.
- Shared SSA/type validation at source, package, native-lifting, and output
  boundaries. Unsupported execution widths/effects produce errors.
- Four GFX900 encoding rules, explicit profile selection, raw-field extraction,
  and lossless re-encoding with validated field edits. GPU semantic bodies
  remain explicitly unsupported rather than becoming executable no-ops.
- Generic lane-mask snapshots, lane reads/writes and uniform-to-lane arithmetic
  broadcast. A separate executable GFX900 `v_add_u32` wave64 profile covers VGPR
  and SGPR sources; the four-rule encoding-only profile stays unsupported.
- Binary package v5 adds lane effects; v4 adds flag reads and carry-input arithmetic; v3 stores register-state FIR operations. Package v2 stores
  encoding plans and existing v1 byte-opcode packages remain readable; packages
  retain their version when serialized.

The JVM `iadd` fixture is the first end-to-end example. CPU register and memory
semantics, GPU synchronization and general divergence, JVM method/class behavior, variable
length encodings, split fields, selector-to-register resolution, cross-target AOT
selection, and Fission consumer adapters are future work.

## Try it

From the Fission workspace root:

```sh
cargo run -p fission-fsl -- check crates/fission-fsl/specs/jvm-se26-iadd.fsl
cargo run -p fission-fsl -- library-inspect crates/fission-fsl/specs/library-candidates.fsldb
cargo run -p fission-fsl -- library-query crates/fission-fsl/specs/library-candidates.fsldb beta
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/jvm-se26-iadd.fsl /tmp/jvm-se26-iadd.fslc
cargo run -p fission-fsl -- decode /tmp/jvm-se26-iadd.fslc 0x60
cargo run -p fission-fsl -- jit-decode /tmp/jvm-se26-iadd.fslc 0x60
cargo run -p fission-fsl -- aot-object /tmp/jvm-se26-iadd.fslc /tmp/jvm-iadd.o
cargo run -p fission-fsl -- emit /tmp/jvm-se26-iadd.fslc 0x60 c /tmp/fsl_iadd.c
cargo run -p fission-fsl -- emit /tmp/jvm-se26-iadd.fslc 0x60 rust /tmp/fsl_iadd.rs
cargo run -p fission-fsl -- execute /tmp/jvm-se26-iadd.fslc 0x60 4 0x7fffffff 1
cc -std=c11 -O2 -c /tmp/fsl_iadd.c -o /tmp/fsl_iadd_c.o
rustc --edition=2021 --crate-type lib -C opt-level=2 --emit=obj /tmp/fsl_iadd.rs -o /tmp/fsl_iadd_rust.o
```

The first register-state path can be exercised directly:

```sh
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/amdgcn-gfx900-sadd-u32.fsl /tmp/gfx900-sadd-u32.fslc
cargo run -p fission-fsl -- decode-bytes /tmp/gfx900-sadd-u32.fslc \
  amdgcn.gfx900.sadd_u32 00010280
cargo run -p fission-fsl -- execute-state /tmp/gfx900-sadd-u32.fslc \
  amdgcn.gfx900.sadd_u32 00010280 0xffffffff,1,123 0
cargo run -p fission-fsl -- emit-bytes /tmp/gfx900-sadd-u32.fslc \
  amdgcn.gfx900.sadd_u32 00010280 c /tmp/fsl_sadd.c
```

The execution writes `0` to SGPR2 and `1` to SCC. The state executor validates
the decoded observation and all array/flag preconditions before any mutation.
This is one fixed-width GFX900 instruction with SGPR 0..95 only; it does not
model inline constants, special registers, literals, wave execution, or a
kernel.

The JIT command compiles a host-native decoder/lifter and prints the FIR
records produced for opcode `0x60`. The AOT command emits a host-native object
exporting the same decoder/lifter ABI. These are correctness footholds, not
performance claims.

Execution output uses `u64` bit-vector stack slots, truncates each pop to its
declared width, and wraps addition at that width. C receives a stack pointer,
depth pointer, and capacity (depth storage must be disjoint from stack storage);
Rust receives a stack slice and depth reference.
Both return status 0 for success, 1 for underflow, 2 for capacity, and 3 for
invalid depth. The active stack is preserved on failure. Signedness is retained
in FIR while wrapping arithmetic uses unsigned storage. These are instruction
contracts, not JVM class-file verification or whole-method semantics.

Run the focused gate with `cargo nextest run -p fission-fsl`. The recompilation
test requires `cc` and `rustc` (override executables with `CC` and `RUSTC`),
and compares C/Rust builds at two optimization levels against the evaluator
over 1,920 cases, with an independent modulo-arithmetic check. This is
synthetic instruction coverage, not measured real-binary decompiler quality.

## CUDA C++ and PTX output slice

`emit <package> <opcode> cuda <output.cu>` and `ptx <output.ptx>` consume the
same canonical stack FIR as C/Rust. The admitted operations are stack pop/push
and wrapping addition with 1..64-bit signed or unsigned bit-vector types.
Register/flag/lane effects, unsupported bodies, wider values and malformed SSA
refuse before output. No new FIR dialect or package version is introduced.

```sh
target/debug/fslc emit /tmp/jvm-se26-iadd.fslc 60 cuda /tmp/fsl_iadd.cu
target/debug/fslc emit /tmp/jvm-se26-iadd.fslc 60 ptx /tmp/fsl_iadd.ptx
clang --target=x86_64-linux-gnu -x cuda --cuda-device-only --cuda-gpu-arch=sm_70 \
  -nocudainc -nocudalib -Xclang -target-feature -Xclang +ptx70 \
  -O2 -S /tmp/fsl_iadd.cu -o /tmp/fsl_iadd_clang.ptx
ptxas -arch=sm_70 /tmp/fsl_iadd.ptx -o /tmp/fsl_iadd.cubin
```

Both targets export `fsl_execute(stack_ptr, depth_ptr, capacity, status_ptr)`.
The stack uses 8-byte bit-vector slots; depth/capacity are unsigned 64-bit;
status is unsigned 32-bit (0/1/2/3 as above). Only block and thread coordinates
all equal to zero own the state. Other invocations return without global
memory accesses. A null status returns without touching state; a null stack
or depth reports 3 when status is valid. Depth/capacity/underflow/peak-capacity
checks happen before stack/depth writes. Popped backing slots are preserved
unless a later ordered push overwrites them.

Caller contract: all non-null buffers are valid, global, naturally aligned and
mutually disjoint; the stack allocation covers `capacity` slots with no address
overflow. One invocation owns that state, including across concurrent launches.
The caller waits for kernel completion before inspecting status/depth/stack.
This is a reference-kernel ABI defined here, not an ABI recovered from a GPU
binary. CUDA coordinates use inline PTX, so device compilation needs no runtime
headers. Direct PTX is pinned to PTX 7.0, sm_70, 64-bit addressing.

Research validation compiles 41 profiles to CUDA device PTX at O0/O2 and checks
3,444 emitted-PTX scalar reference states. The scalar interpreter assumes flat
global addresses and does not model NVIDIA scheduling or memory consistency.
Linux research CI additionally assembles direct and Clang-produced PTX with
hash-locked NVIDIA ptxas. Compilation/assembly is not hardware execution or
whole-kernel behavioral equivalence. C/Rust recompilation regressions are
separate gates. GPU guest decode, SIMT/divergence, memory/barriers/atomics and
kernel ABI recovery remain unsupported by this projection slice.

## GFX900 encoding slice

```sh
cargo run -p fission-fsl -- compile crates/fission-fsl/specs/amdgcn-gfx900.fsl /tmp/gfx900.fslc
cargo run -p fission-fsl -- decode-bytes /tmp/gfx900.fslc amdgcn.gfx900 01050068
cargo run -p fission-fsl -- reencode /tmp/gfx900.fslc amdgcn.gfx900 01050068 /tmp/gfx900.bin destination=3
```

The encoding declaration is part of the same grammar:

```text
encoding 32 mask 0xfe000000 value 0x68000000 {
    field source0 offset 0 bits 9 exclude 249, 250, 255;
    field source1 offset 9 bits 8;
    field destination offset 17 bits 8;
}
semantics unsupported;
```

The example extracts raw source selectors 257 and 2 and destination 0, then
changes bytes `01 05 00 68` to `01 05 06 68`. The caller supplies the exact
profile identity; this does not automatically identify the hardware generation
of arbitrary bytes. Unsupported selectors and unknown instructions return no
match, so callers must stop rather than infer the next instruction boundary.

The compiler rejects overlapping masks, overlapping/out-of-range fields,
unreachable exclusions, mixed widths within one profile, and invalid mask/value
pairs. Each field is contiguous and at most 64 bits wide. The current native
JIT/AOT decoder remains restricted to exact byte opcodes with supported FIR;
GPU decoding currently uses the portable encoding plan. Re-encoding preserves
the original unedited bits; it is not executable GPU recompilation. See the
[slice report](../../docs/research/fsl-gfx900-encoding-slice.md).

## GFX900 `s_add_u32` state slice

[`amdgcn-gfx900-sadd-u32.fsl`](specs/amdgcn-gfx900-sadd-u32.fsl) is the first
GPU instruction with executable FIR state effects. It uses one canonical body:

```text
register.read source0/source1 → u32.add.wrap + u1 int.add.carry
register.write destination      flag.write 0 (SCC)
```

`cargo nextest run -p fission-fsl` covers 1,030 state inputs, including SGPR
aliasing and carry boundaries, then compares the reference evaluator with C and
Rust output at O0 and O2 (4,120 process comparisons). The expected state comes
from widened unsigned addition and quotient/remainder arithmetic, independent
of the FIR carry operation. Invalid selectors, short register/flag banks,
invalid flag values, and invalid encoded observations preserve the entire state.
The same test checks v3 package serialization and the `fslc execute-state` CLI.

This is an execution-semantic vertical slice, not GPU hardware evidence. The
carry-in (`s_addc_u32`) and a masked vector-add state slice are now implemented.

## Carry input and specification migration

`specs/amdgcn-gfx900-saddc-u32.fsl` reads SCC into an SSA value before
register/flag writes, adds the sources and carry modulo 32 bits, and writes the
new SCC. Generic `flag.read`, `uN.add.carry`, and `int.add.carry.in` operations
also support the admitted 1..64-bit domain. V1/V2/V3 packages remain readable;
carry-input operations require V4. Existing scalar-add profiles still emit V3.

Local validation covers 1,030 states per width at 1, 8, 16, 32, and 64 bits,
including six invalid cases per width. C/Rust O0/O2 agree with an independent
widened-integer oracle in 20,600 comparisons. Only the 32-bit profile is GFX900
evidence; other widths exercise generic FIR. A separate reference test chains
`s_add_u32` and `s_addc_u32` for 1,060 64-bit additions.

`specs/ebpf-add64-register.fsl` is the first migrated SLEIGH leaf. It owns the
little-endian register ADD64 encoding and semantics without loading `.sla`.
484 synthetic register states and 1,936 C/Rust O0/O2 comparisons passed. The
SLA reference example separately checks 121 decode/binding/effect shapes:

```sh
cargo run -p fission-sleigh --example ebpf_add_leaf_oracle
cargo run -p fission-fsl -- check-abi crates/fission-fsl/specs/ebpf.fslabi
```

`abi::compile_abi_source` reads FSL-owned ABI metadata: byte sizes, size
alignments, global spaces, stack pointer, ordered input/output register entries,
cleanup (including explicit `unknown`), and preserved/clobbered effects.
The BPF/eBPF `.fslabi` sources preserve all admitted source metadata. Unknown
properties are rejected. The layout linker resolves admitted register names;
parameter allocation, ABI call execution and binary metadata packaging remain unsupported.

The offline converter and source inventory live in `fission-research/tools/fsl_migrate.py`.
It currently admits two of 110 cspec files. This is an explicit narrow migration
subset, with per-file refusals. Full SLEIGH preprocessing, decisions/context,
macros and dynamic templates remain future work. A bounded direct SLA leaf
converter now supplies an additional owned FSL fixture (see below).
See [state and migration contract](../../docs/research/fsl-state-and-migration.md).

## Wave64 EXEC and `v_add_u32`

`specs/amdgcn-gfx900-vadd-u32-wave64.fsl` owns two VOP2 e32 patterns:
VGPR/VGPR sources, and SGPR0..95 broadcast with a VGPR second source. VGPR
selectors span 0..255. This profile requires exactly 64 lanes. It does not
infer wave32 support, inline constants, special registers or extension words.

```text
%exec: u64 = lane.mask.read 64;
%lhs: u32 = lane.register.read source0, 256, %exec;
%rhs: u32 = lane.register.read source1, 0, %exec;
%sum: u32 = u32.add.wrap %lhs, %rhs;
lane.register.write destination, %sum, %exec;
```

Element types remain in FIR. `WaveContract` derives uniform, mask and lane
domains from the typed producer operations. Masks cannot enter integer
arithmetic; lane values cannot enter uniform register/flag/stack effects.
Uniform arithmetic inputs broadcast. No second semantic IR is introduced.
Lane reads capture all slots at their declared effect point; masked writes
update active slots only, preserving inactive bits, including high bits of
untouched u64 storage. Ordered scalar effects execute once, even at EXEC=0.

`WaveState` uses register-major lane slots (`register * lanes + lane`) plus the
existing scalar bank, flags, lane extent and EXEC. All state and fields are
validated before effects, including inactive bank entries. The C/Rust output
ABI mirrors these inputs; C arrays must be valid disjoint storage. The mask
is supplied by value and remains unchanged. No EXEC-write operation is admitted.

```sh
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/amdgcn-gfx900-vadd-u32-wave64.fsl /tmp/wave.fslc
cargo run -p fission-fsl -- decode-bytes /tmp/wave.fslc \
  amdgcn.gfx900.vadd_u32.wave64 01050068
cargo run -p fission-fsl -- emit-bytes /tmp/wave.fslc \
  amdgcn.gfx900.vadd_u32.wave64 01050068 c /tmp/fsl_wave.c
```

The `execute-wave` CLI accepts lane count, EXEC, scalar registers, flags and
flat lane slots. Tests check 1,984 valid states and 19 invalid emitter inputs
(2,003 rows / 8,012 C/Rust O0/O2 comparisons), plus a synthetic four-lane
scalar/mask contract (4 rows / 16 comparisons). Register aliasing, zero/full/
sparse/highest-lane masks, scalar broadcast, untouched state and rejection
before any mutation are included. Maximum VGPR/SGPR boundaries are additionally
checked in the reference evaluator. All 22 crate tests passed locally.

These are synthetic state/recompilation results, not GPU hardware or kernel
equivalence. Wave execution currently admits wrapping addition and lane/scalar/
flag reads and writes. Carry operations in mixed wave bodies, EXEC updates,
VCC operations, divergence, barriers, memory, traps and Cranelift wave JIT/AOT
remain unsupported. Historical scalar/stack package versions stay unchanged.

## Register layout and ABI linking

`.fslregs` is an FSL text companion to instruction and ABI source. Its grammar
records a default memory space, spaces with explicit byte-address width/unit
and byte order, and named register views with byte offsets and widths:

```text
layout example.registers {
    evidence "example" "local" "1" "Byte-storage example";
    default_space "ram";
    space "ram" memory 8 byte little;
    space "register" register 4 byte little;
    register "R0" "register" 0 8;
    register "R0.low" "register" 0 4;
}
```

`RegisterFile` shares bytes across overlapping views; partial writes preserve
other bytes. `link_abi` resolves ordered register entries, verifies widths and
memory ranges, and refuses conflicting preserved/clobbered aliases. Imported
eBPF LE/BE layouts link successfully. BPF LE imports 15 views, but its unchanged
4-byte RS conflicts with `pointer_size=8` under the first strict stack gate.
That diagnostic is retained; no replacement size is inferred.

```sh
cargo run -p fission-fsl -- check-layout crates/fission-fsl/specs/bpf.le.registers.fslregs
cargo run -p fission-fsl -- link-abi crates/fission-fsl/specs/ebpf.fslabi \
  crates/fission-fsl/specs/ebpf.le.registers.fslregs
cargo run -p fission-fsl -- execute-layout \
  crates/fission-fsl/specs/ebpf.le.registers.fslregs crates/fission-fsl/specs/ebpf.fslabi \
  /tmp/ebpf-add.fslc ebpf.le.add64.register 0f12000000000000 \
  R0,R1,R2 0,1,0xffffffffffffffff
```

The execution command validates the ABI link then uses explicit FIR register
slots; it does not execute a call convention. `execute_bound` admits disjoint
bound views with exact widths and commits storage only after success. It uses
the existing canonical FIR evaluator. Inter-slot aliases and lane banks are
refused; storage-view aliases remain available through byte access.

Seven new native tests cover metadata refusal, shared views, endian/width
behavior, linking, slot/flag adapters and the CLI. The byte-storage oracle checks
968 states (484 per storage byte order), both with the existing LE instruction
profile. This does not add BE eBPF decoding or storage C/Rust projection.
All 29 crate tests, including earlier recompilation gates, were rerun. No
package version or historical package hashes changed. Layout/ABI binary
packaging, GPU layout/kernel ABI and general SLA semantic conversion are pending.

## Direct SLA symbol/template slice

The research importer reads SLA v4 symbols, instruction decision constraints,
selector tables and ordered ConstructTpl atoms. One BUILD/pure-register-export/
INT_ADD path emits `ebpf-sla-add64-register.fsl` plus an SLA-derived register
layout. This uses existing canonical FIR primitives and package v3. There is
no Sleigh source or runtime dependency when compiling/executing these fixtures.
Unknown effects and dynamic/context/constant-export paths remain refused.

```sh
cargo run -p fission-fsl -- compile crates/fission-fsl/specs/ebpf-sla-add64-register.fsl /tmp/ebpf-sla.fslc
cargo run -p fission-fsl -- execute-layout \
  crates/fission-fsl/specs/ebpf.le.sla.registers.fslregs crates/fission-fsl/specs/ebpf.fslabi \
  /tmp/ebpf-sla.fslc ebpf.le.sla.add64.register 0f12000000000000 \
  R0,R1,R2 0,1,0xffffffffffffffff
```

The source- and SLA-derived layout/encoding/FIR agree on 65,536 prefixes and
admit the same 121 combinations. The SLA candidate separately passes 484
synthetic states / 1,936 C/Rust O0/O2 comparisons and 484 bound-storage states.
All 32 native tests, including earlier gates, are rerun. These are instruction
contract checks, not full SLA replacement, eBPF verifier legality or VM execution.

## Migration goal

FSL source definitions, the compiler, FIR, package format, and Fission
adapters are intended to become Fission-owned. SLEIGH and `.sla` artifacts may
serve as temporary differential oracles while coverage is built; the final
Fission build and runtime should not require them. See
[`docs/research/fsl-fir-compiler-architecture.md`](../../docs/research/fsl-fir-compiler-architecture.md)
for the proposed migration stages and parity gates.
