# FSL/FIR Compiler Architecture

**Status:** research RFC, initial proposal
**Scope:** language and compiler direction; not a claim that the current
`iadd` proof implements this design

## Summary

FSL should be a human-authored, typed language for instruction encodings,
decode constraints, display, and semantic effects. FIR should be the typed
semantic model produced from those specifications. Neither the source language
nor the semantic model should be defined by JSON or by P-code.

The compiler should have a portable, inspectable output and optimized native
backends. A reference interpreter is useful as a correctness oracle, but it
should not be the only runtime path. Cranelift is a reasonable optional backend
for compiling decoder/lifter functions to host code, and later for executing
FIR programs. These are separate uses and should not be conflated.

## Current prototype boundary

The current `.fsl` sample is TOML syntax, and `fslc_probe.py` turns it into a
JSON pattern package. This is useful for validating schema fields and proving
one JVM opcode against the existing Fission `.sla` path. It is not the proposed
FSL source grammar or final compiled artifact format. Keep JSON as an optional
debug/export/interchange view, not as the authored language or hot-path runtime
representation.

Fission already has more than one execution mode: the SLEIGH runtime selects
compiled templates and evaluates them to emit P-code, while the emulator has
an existing [Cranelift P-code-to-host JIT](../../crates/fission-emulator/src/jit/compiler.rs).
FSL/FIR should be able to bypass template evaluation when producing semantic
output, without requiring the first research slice to replace the existing
path. The current [template evaluator](../../crates/fission-sleigh/src/runtime/spine/compiled_table/template_eval.rs)
is the relevant baseline for that comparison.

## Design goals

- Describe CPU instruction sets, GPU instruction sets, JVM-like bytecode, and
  other virtual or physical architectures in one extensible toolchain.
- Preserve architecture-specific effects when a scalar, flat operation list
  would lose meaning: examples include GPU execution masks and barriers, vector
  lanes, address-space-specific memory, VM operand stacks, and exceptions.
- Compile one specification into several consumers: Fission P-code, FIR
  graphs, execution backends, and future language or analysis adapters.
- Make decode, type, effect, overlap, and provenance errors compile-time
  diagnostics with source locations.
- Make repeated decoding and lifting fast through generated decision logic and
  precompiled semantic emitters.
- Keep a small, obviously-correct reference path for differential checking.

## Source and artifact formats

### Authored source

Use a dedicated textual `.fsl` grammar. Borrow SLEIGH's compact separation of
encoding, display, and semantics, but do not copy its P-code commitment or make
all architecture concepts look like register arithmetic. SLEIGH's constructor
model is a useful precedent: a constructor associates an encoding pattern,
display meaning, and semantic meaning. See the [SLEIGH constructor reference](https://ghidra.re/ghidra_docs/languages/html/sleigh_constructors.html).

Illustrative syntax only; names and grammar are not frozen:

```text
language jvm.se26 {
    byte_order big
    address_unit byte

    instruction iadd {
        encoding { opcode: u8 = 0x60 }
        display  { mnemonic "iadd" }

        semantics {
            let rhs: i32 = vm.stack.pop()
            let lhs: i32 = vm.stack.pop()
            vm.stack.push(i32.add_wrap(lhs, rhs))
        }
    }
}
```

The final grammar needs named fields, slices and concatenation, variable-length
and prefix encodings, decode modes/context, operand decoding, aliases,
overlap/priority rules, imports, reusable semantic macros, source provenance,
and explicit unsupported behavior. Those features should be admitted with
target-backed examples rather than guessed up front.

### Compiled output

Use a versioned binary package (provisionally `.fslc`) with a section table,
schema/compiler identity, source hashes, target/profile identity, diagnostics
metadata, decode representation, typed semantic templates, and optional
host-specific native code. A portable form must remain available when native
code is absent or unusable. Never serialize raw pointers or depend on JSON
object ordering for runtime behavior.

JSON remains valuable for `fslc dump --format json`, tests, reports, and tools
that need to inspect the package. It should be generated from the typed
compiler model rather than serve as that model.

## Compiler pipeline

```text
FSL source
  -> lexer/parser with source spans
  -> resolved specification (names, imports, profiles)
  -> typed decode/effect IR
  -> validation and overlap analysis
  -> FIR semantic graphs and decode decision network
  -> optimization and artifact generation
       |-> portable package + reference evaluator
       |-> Fission P-code adapter
       |-> AOT native decoder/lifter object
       |-> JIT decoder/lifter module
       `-> later FIR execution backends
```

The typed semantic IR should be canonical before choosing a backend. Type and
effect checking must distinguish fixed-width wrapping arithmetic, signedness,
memory spaces, control flow, exceptions, and architecture-specific state
transitions. A typed effect should say what changes, not just which arithmetic
operator appears.

FIR should have a small shared core plus explicit extension/dialect points.
Possible domains include bit-vector and floating-point operations, registers,
memory spaces, vectors/predication, GPU execution and synchronization, and VM
stack/frame effects. This is a design boundary, not a requirement to implement
all those domains in the first compiler version. Do not flatten GPU or VM
semantics into P-code-shaped operations before checking whether that loses
observable behavior.

## Performance architecture

Separate these workloads because they have different outputs and performance
costs:

1. **Decode and lift:** bytes plus mode/context become a decoded instruction
   and FIR graph or selected output IR. Specialize pattern matching into a
   decision DAG/table and compile semantic templates into direct emitters. The
   emitter builds semantic output; it does not execute the guest instruction.
2. **Guest execution:** an FIR program or block updates a guest state. This is
   where native code execution/JIT is appropriate for emulation or simulation.
3. **Analysis output:** FIR is translated to a consumer representation such as
   Fission P-code or another analysis IR. This must preserve effects and
   provenance, not merely produce equivalent host machine instructions.

Cranelift can serve as an optional host-code backend for (1) and (2). Its JIT
module places compiled functions in memory; its object module emits `.o`
artifacts. Its `TargetIsa` set is the set of host code-generation targets,
which is separate from the guest architectures described by FSL. Therefore,
Cranelift can compile a fast x86/AArch64/RISC-V decoder or lifter that *reads a
GPU binary*, but it does not by itself define GPU instruction semantics or
produce GPU-native code. See the [Cranelift JIT module](https://docs.rs/cranelift-jit/latest/cranelift_jit/struct.JITModule.html),
[object module](https://docs.rs/cranelift-object/latest/cranelift_object/struct.ObjectModule.html),
and [target ISA API](https://docs.rs/cranelift-codegen/latest/cranelift_codegen/isa/index.html).

Recommended backend progression:

- Keep a reference evaluator for correctness and bring-up only.
- First compile decode patterns and semantic templates into compact,
  prevalidated data structures and direct Rust emitters; measure this against
  the current Fission template-walking path.
- Add an AOT Cranelift backend for static, frequently used profiles and emit
  relocatable objects for normal linking.
- Add a JIT Cranelift backend for dynamic profile loading or rapid iteration,
  with a cache keyed by source/package hash, compiler version, host target, and
  enabled host features.
- Separately evaluate FIR-to-Cranelift execution for emulation. Do not make
  `FIR -> Cranelift` the only way to obtain a semantic graph for decompilation.

Generated Rust and Cranelift should be measured against each other before
selecting a default. The right choice may differ between decoder/lifter code,
guest execution, and deployment constraints.

## Correctness and performance gates

For each target slice, compare the reference evaluator, generated emitter, and
available existing implementation on the same input bytes and context. Compare
normalized semantic graphs/effects, consumed byte lengths, control-flow
classification, and provenance. Add overlap fuzzing, boundary encodings,
variable-length inputs, and malformed input cases as support grows.

Performance reports should separate compile time, cold startup, warm decode
throughput, lift/output throughput, allocations per instruction, and package
size. Report both decoder-only and decode-plus-semantic-output measurements.
Do not call FSL “faster” until the same corpus, output contract, and warm/cold
conditions show it. The JVM `iadd` probe is a correctness foothold, not a
performance result.

## Open research questions

- What minimum shared FIR core supports CPU, GPU, and VM semantics without
  erasing their differences?
- Should decoder output retain a common instruction record while semantic
  effects remain dialect-specific?
- How should user-defined operations, opaque hardware behavior, and incomplete
  semantics be typed and propagated to consumers?
- Which features require context-sensitive state, packet/bundle decoding,
  delayed effects, or multiple instructions per decode unit?
- Which artifact sections can be portable, and which need host-specific code
  or signatures?
- What is the best first multi-target corpus for deciding between generated
  Rust, Cranelift JIT, and Cranelift AOT?

## Proposed next experiment

Keep the JVM `iadd` parity fixture as the smallest golden case, then add one
register-register CPU arithmetic instruction and one GPU instruction whose
semantics require a non-scalar effect (for example, a predicate or execution
mask). Use the same source frontend and typed FIR validation pipeline for all
three. Before adding more syntax, make the compiler print its typed FIR and
report precise source locations for type, effect, and encoding-overlap errors.
