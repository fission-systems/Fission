# Owned FSL/FIR pipeline — 2026-10-16

Goal: binary format facts -> owned FSL decode -> canonical FIR -> checked output,
without SLEIGH/SLA/P-code or legacy specification fallback. First integration is
an explicit little-endian ELF i386/x86-64 executable span, not full Fission
replacement or automatic general function recovery.

## Hidden metadata dependency removed from the new route

Previous FSL windows already owned instruction decoding/execution, but called
`LoadedBinary::from_file`. Its ELF architecture selection queries Ghidra opinion
and ldefs resources. No linked Sleigh runtime was not sufficient evidence of
asset independence.

`LoadedBinary::from_file_format_facts` traverses canonical LoaderPipeline routing
and the existing ELF mapper/relocation/symbol/builder path. An explicit selection
policy validates ELF machine/class/endian from canonical constants. It returns
architecture facts with ABI unselected and no BinaryLoadSpec. `arch_spec` remains
unknown; no FSL identity is disguised as a Ghidra language ID. No external debug,
identity enrichment or signature-resource discovery is run by this narrow API.
Non-ELF, non-x86, mismatched class or endian refuses. Existing default loader
selection remains opinion based and is not used by this owned route.

`X86Program::from_owned_binary` uses this opt-in path, preserving executable and
file-backed span, relocation overlap, exact bytes and architecture checks.
`from_binary` retains the previous metadata selection for comparison consumers.

## Entry point and artifacts

```sh
fsl-x86 decompile-owned INPUT.elf ADDRESS SIZE c|rust|go NEW_DIRECTORY
```

`pipeline::project_x86_binary` compiles embedded `scalar-immediate.fslx` and
`scalar-immediate-bodies.fsl` in memory, maps the supplied span, binds canonical
FIR and invokes the checked backend. There is no separate package preparation
step or legacy fallback. The current profile has 48 rules / 150 width-specific
bodies; counts are derived from the actual compiled profile.

The new directory contains `output.c`/`.rs`/`.go`, `output.fir`, `profile.fslxc`,
`projection.txt`, `pipeline.txt`, `build.txt`. The portable package/FIR and source
retain original bytes/origins/hashes; generated reports mark compilation and
execution equivalence as unperformed. Admission/emission finishes before creating
the directory. Existing directories refuse; I/O failure may leave a partial new
directory. This is not atomic publication.

## Acceptance and limits

The research gate checks dependency closure, package/FIR/source hashes for three
actual ELF immediate-store spans across C/Rust/Go, nine source compilations,
nine existing-directory refusals and unsupported opcode/backend refusals.
Portable local execution copies only the executable/inputs into an empty cwd
with empty resource roots and PATH; the rest of the host filesystem is still
accessible, so this alone does not prove filesystem isolation.

A dedicated Linux CI job fetches crates without utils/vendor or legacy assets,
builds the executable there, then runs it in a chroot containing only that
executable, its dynamic system libraries and fixture inputs. strace file traces
reject even failed opens/stat of legacy asset extensions. This is a resource
closure experiment, not a security sandbox. No original/recompiled execution is
measured by this gate; source bytes must equal previously execution-validated
artifacts, and existing execution CI remains separate.

The complete product path still uses SLEIGH/NIR/HIR. This experimental path is
limited to selected spans and explicit state outputs. Source function boundaries,
CALL/callee ABI, general CFG/type recovery, guest faults/concurrency, full x86,
other container/ISA migration and whole Python/JVM/WASM methods remain incomplete.
Next substantive semantic work is FSL-owned ABI/state contracts and function CFG
recovery; the new entry point is infrastructure, not a decompiler quality claim.
