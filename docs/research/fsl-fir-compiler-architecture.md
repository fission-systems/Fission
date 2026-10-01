# FSL/FIR Compiler Architecture

**Status:** research RFC with an initial compiler prototype
**Scope:** language, compiler, and eventual SLEIGH replacement direction; the
current JVM `iadd` slice implements only a small part of this design

## Summary

The accepted experimental direction uses **one canonical FIR**, with no
separate NIR/HIR semantic models. Correctness and behavioral recompilation are
the primary objectives. Multiple output layers consume the same FIR and its
evidence-backed analysis facts. Existing decompiler architecture may be replaced
as the corresponding FIR-native behavior passes its coverage gates. See
[ADR 0015](../adr/0015-fsl-single-fir-recompilation.md).

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

The earlier parity probe began with a TOML-like sample and `fslc_probe.py`
emitted a JSON pattern package. That remains a schema/provenance experiment for
checking one JVM opcode against the existing Fission `.sla` path; it is not the
FSL compiler or its runtime format. The `fission-fsl` crate now has a separate
text grammar and binary `.fslc` package. JSON can remain an optional
debug/export/interchange view, but it is not the authored language or hot-path
runtime representation.

Fission already has more than one execution mode: the SLEIGH runtime selects
compiled templates and evaluates them to emit P-code, while the emulator has
an existing [Cranelift P-code-to-host JIT](../../crates/fission-emulator/src/jit/compiler.rs).
FSL/FIR should be able to bypass template evaluation when producing semantic
output, without requiring the first research slice to replace the existing
path. The new [`fission-fsl` crate](../../crates/fission-fsl/) is an
independent compiler slice: its small `.fsl` frontend produces a typed FIR
package and a versioned binary `.fslc`; Cranelift JIT and AOT paths compile the
same decoder/lifter to emit native FIR records or a host relocatable object. It
has no dependency on `fission-sleigh`, `.sla`, JSON, or P-code. This proves an
implementation boundary, not broad parity or a speedup.

The same instruction FIR now also has a reference evaluator and compilable C
and Rust execution outputs. The recompilation gate compiles those outputs at
two optimization levels and compares active stack/state behavior and failures.
This differs from the native decoder/lifter path: generated execution code
updates guest stack state, while the decoder/lifter emits FIR records. Current
execution support is limited to bit-vector stack operations and wrapping
integer addition of widths 1..=64. Wider FIR is preserved but rejected by these
execution outputs. No instruction-contract test is a whole-program quality claim.
The current [template evaluator](../../crates/fission-sleigh/src/runtime/spine/compiled_table/template_eval.rs)
remains a comparison baseline while migration proceeds.

## End state: Fission-owned specifications and runtime

The research goal is to remove SLEIGH as a Fission runtime and build-time
requirement. FSL source, its compiler, FIR semantics, compiled packages, and
the adapters used by Fission consumers should all be owned and maintained in
the Fission ecosystem. `.sla` files and the SLEIGH implementation may be used
as temporary comparison oracles during migration; they are not the target
source language, an `.fslc` dependency, or a required runtime component.

This is a staged replacement, not a one-time format conversion:

1. **Inventory and baseline.** Record every supported language profile,
   context field, register/address-space model, decode/display behavior,
   semantic effect, calling-convention source, and Fission consumer that
   currently depends on SLEIGH. Pin reproducible binaries and expected decode,
   lift, control-flow, and analysis outputs.
2. **Own the compiler core.** Grow the independent `fission-fsl` frontend,
   typed FIR, portable artifact validation, reference evaluator, and code
   generators. Add real architecture examples before freezing abstractions.
   Keep generated Rust and Cranelift JIT/AOT as competing backends until
   equivalent-output measurements establish where each fits.
3. **Build owned target definitions.** Author FSL definitions from
   architecture manuals and other recorded primary sources. Store source
   identity, revision, claims, and applicable license/provenance information
   with each definition. Any importer used to bootstrap existing definitions
   is a migration aid; generated output must become reviewable FSL source and
   must not make `.sla` a build input for the final pipeline.
4. **Prove parity by profile.** Compare old and new paths over the same
   instruction and binary corpora: decode boundaries and modes, display,
   typed effects, control flow, register and memory spaces, exceptions, and
   downstream Fission results. Promote a profile only after its explicit
   coverage gates pass; a single opcode or synthetic corpus is not sufficient.
5. **Migrate consumers.** Move disassembly, static and dynamic analysis,
   decompilation, and emulation consumers onto FSL/FIR-native APIs. Where a
   legacy consumer still needs P-code, use a compatibility adapter and report
   effects FIR cannot represent instead of silently discarding them.
6. **Remove SLEIGH from the product path.** Once every shipped profile and
   consumer has an owned definition and passes its gates, remove the
   `fission-sleigh` runtime dependency, `.sla` packaging/loading, and SLEIGH
   build tools from normal Fission builds. Keep only stable differential
   fixtures and provenance needed to reproduce migration evidence.

CPU, GPU, and VM targets need not share one flat semantic vocabulary. The
owned model must preserve such effects as GPU execution masks/barriers, CPU
flags and address spaces, and VM stack/frame/verifier behavior. Scope is open,
but each target still needs a source-backed semantic contract and observable
parity criteria before it is called supported.

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
