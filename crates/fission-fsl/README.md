# fission-fsl

Fission-owned research compiler for FSL instruction definitions and typed FIR.
This crate is an independent vertical slice toward replacing SLEIGH in the
Fission product path; it does not yet replace any existing architecture
frontend.

One typed FIR is the semantic owner. Output layers consume that same body;
the experimental direction has no NIR/HIR split. The priorities are accuracy
and behavioral recompilation. See [ADR 0015](../../docs/adr/0015-fsl-single-fir-recompilation.md).

## Current slice

- Dedicated `.fsl` text input with one-byte opcode patterns.
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
- Shared SSA/type validation at source, package, native-lifting, and output
  boundaries. Unsupported execution widths/effects produce errors.

The JVM `iadd` fixture is the first end-to-end example. CPU register and memory
semantics, GPU masks and synchronization, JVM method/class behavior, variable
length encodings, multi-byte patterns, cross-target AOT
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

## Migration goal

FSL source definitions, the compiler, FIR, package format, and Fission
adapters are intended to become Fission-owned. SLEIGH and `.sla` artifacts may
serve as temporary differential oracles while coverage is built; the final
Fission build and runtime should not require them. See
[`docs/research/fsl-fir-compiler-architecture.md`](../../docs/research/fsl-fir-compiler-architecture.md)
for the proposed migration stages and parity gates.
