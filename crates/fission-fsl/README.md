# fission-fsl

Fission-owned research compiler for FSL instruction definitions and typed FIR.
This crate is an independent vertical slice toward replacing SLEIGH in the
Fission product path; it does not yet replace any existing architecture
frontend.

## Current slice

- Dedicated `.fsl` text input with one-byte opcode patterns.
- Required per-instruction evidence fields.
- Typed integer values and a small VM-stack/wrapping-add FIR dialect.
- Versioned portable binary `.fslc` output. JSON is not used by the compiler,
  package loader, or native path.
- Cranelift JIT for the host-native decode-and-lift path. It emits compact FIR
  records; it does not execute guest instructions.

The JVM `iadd` fixture is the first end-to-end example. CPU register and memory
semantics, GPU masks and synchronization, JVM method/class behavior, variable
length encodings, multi-byte patterns, reference evaluation, AOT, and Fission
consumer adapters are future work.

## Try it

From the Fission workspace root:

```sh
cargo run -p fission-fsl -- check crates/fission-fsl/specs/jvm-se26-iadd.fsl
cargo run -p fission-fsl -- compile \
  crates/fission-fsl/specs/jvm-se26-iadd.fsl /tmp/jvm-se26-iadd.fslc
cargo run -p fission-fsl -- decode /tmp/jvm-se26-iadd.fslc 0x60
cargo run -p fission-fsl -- jit-decode /tmp/jvm-se26-iadd.fslc 0x60
```

The JIT command compiles a host-native decoder/lifter and prints the FIR
records produced for opcode `0x60`. This is a correctness foothold, not a
performance claim.

## Migration goal

FSL source definitions, the compiler, FIR, package format, and Fission
adapters are intended to become Fission-owned. SLEIGH and `.sla` artifacts may
serve as temporary differential oracles while coverage is built; the final
Fission build and runtime should not require them. See
[`docs/research/fsl-fir-compiler-architecture.md`](../../docs/research/fsl-fir-compiler-architecture.md)
for the proposed migration stages and parity gates.
