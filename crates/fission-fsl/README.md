# fission-fsl

Fission-owned research compiler for FSL instruction definitions and typed FIR.
This crate is an independent vertical slice toward replacing SLEIGH in the
Fission product path; it does not yet replace any existing architecture
frontend.

One typed FIR is the semantic owner. Output layers consume that same body;
the experimental direction has no NIR/HIR split. The priorities are accuracy
and behavioral recompilation. See [ADR 0015](../../docs/adr/0015-fsl-single-fir-recompilation.md).

## Current slice

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
properties are rejected. Symbolic registers are not linked yet; parameter
allocation, ABI execution and binary metadata packaging remain unsupported.

The offline converter and source inventory live in `fission-research/tools/fsl_migrate.py`.
It currently admits two of 110 cspec files. This is an explicit narrow migration
subset, with per-file refusals. Full SLEIGH preprocessing, decisions/context,
macros, dynamic templates and direct SLA-to-FIR lowering remain future work.
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

## Migration goal

FSL source definitions, the compiler, FIR, package format, and Fission
adapters are intended to become Fission-owned. SLEIGH and `.sla` artifacts may
serve as temporary differential oracles while coverage is built; the final
Fission build and runtime should not require them. See
[`docs/research/fsl-fir-compiler-architecture.md`](../../docs/research/fsl-fir-compiler-architecture.md)
for the proposed migration stages and parity gates.
