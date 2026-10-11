# Readonly x86 operands and FIR-derived input evidence

Experimental follow-up, 2026-10-11, branch `codex/fsl-jvm-iadd-parity`.

## Canonical semantics and ownership

The owned decoder reuses the existing generic ModRM/SIB/displacement address
reader for RegRm memory sources. It binds address fields to canonical FIR
bodies; it does not calculate instruction arithmetic. The optional
`scalar-memory-bodies.fsl` has 126 bodies: the original 63 plus readonly
MOV/ADD/SUB/CMP/AND/OR/XOR source bodies for three value/address widths.
The encoding rules stay at 46. Package versions and the original 63-body
artifacts remain unchanged. No external decoder dependency or lockfile change.

FIR computes modular addresses, performs bounded `MemoryLoadLittle`, and
updates the existing typed register/flag state. Flat segment bases are zero.
Memory stores, LOCK, nondefault segment overrides and general fault semantics
remain unsupported. Wrapper rollback on failure remains an API contract,
not CPU exception behavior. The 126 templates are an authored scope count,
not 126 independently verified ISA cases.

## Candidate recovery from the same FIR

`recovery::RecoveryState` is a derived abstract analysis view, not a second
instruction semantics owner. It tracks entry-register dependencies and modular
affine addresses through typed FIR operations, conversions and block arguments.
Memory reads record instruction address, FIR operation index, width and optional
affine address. Unknown branches/unsupported transfers refuse. A narrower modular
wrap cannot be flattened into an equivalent wider addition without evidence.

`X86Program::recover_input_evidence()` admits a sequential guest path to near
RET and inspects return-register slot 0. This external return-register assumption
is printed. Return-data reads and control/other reads are distinguished; raw
instruction bytes and addresses remain attached. It does not choose an ABI,
prototype, argument count, signedness or pointer type. No inferred convention
is silently made executable.

```sh
cargo build --locked -p fission-fsl --features x86 --bins
target/debug/fsl-x86 compile crates/fission-fsl/specs/x86/scalar.fslx \
  crates/fission-fsl/specs/x86/scalar-memory-bodies.fsl /tmp/memory.fslxc
target/debug/fsl-x86 recover-binary /tmp/memory.fslxc \
  cdecl32.elf 0x401000 9 /tmp/input-evidence.txt
```

The research repository's locked cdecl32 ELF yields entry ESP+4/+8 32-bit
return-data candidates and ESP+0 return control. Two pointer64 ADD/XOR ELFs
yield entry RDI+0/+8 64-bit return-data candidates and RSP+0 return control.
Generated C/Rust still take explicit machine state and bounded memory.

## Observed validation

- Full x86-feature nextest: 61 passed, no skipped tests. Three new tests cover
  candidate origins, memory guard/late-return rollback and unsupported effects.
- Three fixed Clang/LLD 22.1.8 ELF spans: 768 FIR states and 3,072 executions of
  generated C/Rust at O0/O2 agree with independent arithmetic/state calculations.
- ARM64 macOS: exact pointer64 ADD/XOR instruction bytes run through Rosetta,
  512 result/defined-flag observations agree. i386 machine execution was not run
  locally. The Linux CI adapter is intended to run all three spans on x86.
- Research-only `fission-emulator/examples/x86_fir_baseline.rs` uses the actual
  compiled SLA frontend and canonical P-code evaluator. It is not a runtime
  dependency of FSL. ADD AF differs in 96/256 cases in each ADD fixture; all
  compared registers, return PC and other arithmetic flags agree. XOR's undefined
  AF is excluded in 256 cases; its five defined flags agree.
- FSL strict Clippy and formatting pass. Baseline build and CI-equivalent Clippy
  (`-D warnings -A clippy::style -A clippy::complexity`) pass. Unqualified strict
  baseline Clippy fails on existing solver/emulator style and complexity debt;
  those libraries were not changed.

The checked-in x86 `resultflags` source explicitly omits AF. P-code can express
memory, branches and this flag calculation; the observed difference is current
specification coverage, not proof of a P-code expressive limit. See the
[P-code operations](https://ghidra.re/ghidra_docs/languages/html/pcodedescription.html)
and [Intel SDM](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html).

Independent machine comparison measures results and defined arithmetic flags,
not all architectural registers, faults, memory ordering or full function
equivalence. Source types and general ABI allocation remain unresolved. Next
work needs stores/stack frames/calls, recovered CFG joins and additional
independent executable corpora before widening accuracy claims.
