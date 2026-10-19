# Owned x86 ELF to checked scalar C functions

The experimental `fission-fsl` pipeline now admits a restricted ordinary C
function projection from the existing owned x86 FIR. Product decompilation
continues to use its existing SLEIGH path. This is not a general ABI recovery
or decompiler quality claim.

## Route and semantic ownership

```
.fslprofiles/.fslpc -> canonical ELF loader -> exact FSL package binding
    -> selected executable bytes -> canonical FIR instructions
    -> derived CFG/dependency/ABI admission -> FIR C backend -> uint64_t function
```

There is no SLEIGH/P-code/NIR/HIR conversion and no external decoder dependency.
`function.rs` consumes borrowed canonical FIR bodies and decoded bindings. Its
worklist tracks entry dependencies and affine register facts, not executable
expressions or a second IR. x86 physical register names are bound to logical
slots by the existing x86 adapter. A caller explicitly selects the restricted
`sysv-amd64-u64-leaf-v1` ABI. ELF class/machine do not prove that ABI.

The authored `.fslabi` records ordered integer input registers, the RAX output,
preserved GPRs and the eight-byte return-stack change. Its provenance points to
the pinned compiler-model reference. It deliberately does not import the full
cspec's float/aggregate/stack-argument, data layout or injection policies.

## Admission

Every explored canonical FIR control alternative must satisfy:

- An acyclic guest CFG remains within the caller-selected instruction span.
- Return and control dependencies use only ABI integer argument registers.
  Unknown entry flags, stack addresses and non-argument GPRs are rejected when
  observable. Dead pure reads do not create a parameter.
- No memory writes or data loads occur, including dead potentially faulting loads.
  The only memory load is the exact original return token at the entry stack.
- Return PC is that unmodified token, stack increases by eight, and preserved
  GPRs retain their entry identities.

Unknown conditions explore both outcomes without assuming path predicates;
this can conservatively refuse an infeasible path. Limits are 16,384 instruction
path visits, 1,024 pending guest states and eight unknown FIR branches per
instruction. Cycles or exhausted budgets refuse; they do not truncate silently.
The CFG is an instruction-level derived view, not recovered source structure.

The public function has the smallest prefix of ABI argument registers covering
observed inputs. A hole remains as an unused preceding argument. This prefix is
**not** the original source parameter count, type or signedness. The caller's
ABI assertion and candidate boundary remain external inputs.

## C projection

The generated public signature is `uint64_t recovered_NAME(uint64_t arg0, ...)`.
The existing canonical FIR C backend emits internal state helpers and the guest
CFG dispatcher. The wrapper initializes only a private state/memory window;
admission establishes that unexposed entry state cannot influence the admitted
return/control result. RET itself executes from FIR with a synthetic private
return token. An unexpected runtime refusal aborts instead of inventing a value.
Internal state helpers are static and namespaced by the requested public symbol.
A `recovered_` prefix is required to avoid C runtime/macro names. Readability and
source-level restructuring are not implemented by this step.

## Reproduce

```sh
cargo build -p fission-fsl --features x86 --bin fsl-x86
./target/debug/fsl-x86 compile-profiles \
  crates/fission-fsl/specs/x86/elf.fslprofiles /tmp/elf.fslpc
./target/debug/fsl-x86 decompile-function /tmp/elf.fslpc input.elf \
  ADDRESS SIZE sysv-amd64-u64-leaf-v1 recovered_candidate NEW_DIRECTORY
cargo nextest run -p fission-fsl --features x86
cargo clippy -p fission-fsl --features x86 --all-targets -- -D warnings
```

The research repository's `tools/scalar_function_validate.py` compiles three
freestanding C functions at O1/O2, links actual ELF files, invokes this catalog
route, recompiles generated C at O0/O2 with fatal UBSan, and compares 2,048
boundary/random triples per function against unmodified extracted machine bytes
and a separately compiled source reference. macOS uses Rosetta for x86 bytes;
Linux x86-64 executes them natively. `-x86-cmov-converter-force-all` constrains
compiler output to the existing branch subset. This is an authored six-function
experiment, not DecBench, full ABI coverage or formal equivalence.

General function discovery, source types, calls, loops, data memory, faults,
floating point, aggregates, stack arguments and x86-32 function ABI projection
remain unsupported. The existing explicit-state 16/32/64-bit projections remain
available. No legacy product consumer has been migrated by this experiment.
