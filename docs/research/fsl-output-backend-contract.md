# Output backend contract, first slice

Historical C/Rust scope at `cc02f3be7`. The registry now also supports
[Go instruction/window projections](fsl-go-backend.md); the earlier limits and
counts below describe this first slice.

C and Rust source emission now share `backend::OutputBackend`: `descriptor`,
`plan`, and `emit`. The input is a borrow of canonical `CompiledInstruction` or
an owned-frontend `X86Program` window. This does not introduce another semantic
IR. Existing C/Rust implementations remain the source renderers.

## Admission

`ProjectionPlan::checked` checks descriptor validity, requested dialect, runtime
policy, memory model, input context, canonical body validity and the existing
stack/register/wave/window execution contract before emission. It collects
bitvector, control, register/flag, lane, guest-PC and memory requirements.
Unsupported input returns `UnsupportedReport` with a code and body/operation
when that location is available. The plan borrows its input and clones the target
profile, so the checked input cannot be edited before emission.

Current profiles are C11 and Rust 2021. Both require embedded state/guard code
and bounded flat byte memory. Architectural faults, concurrency, complete ABI
recovery, object runtime and general exception semantics are not admitted.
This is a contract check, not a semantic proof or compiler invocation.

## Extension and artifacts

`BackendRegistry::register` accepts an in-process `OutputBackend` provider.
Duplicate IDs and invalid descriptors refuse. Providers own source syntax;
canonical FIR remains the semantic owner. New semantic domains require explicit
contracts before shared admission can be extended. Dynamic plugins and Go,
Python, Java, CUDA/PTX integration into this registry are not implemented.
Existing CUDA/PTX entrypoints remain separate experimental projections.

`ArtifactBundle` contains source files, a compiler recipe, symbol-level origins
and an admission report. x86 origins retain body, source symbol, guest address,
raw instruction bytes, input offset and length. Statement-level source maps are
not provided. Reports explicitly say compilation and equivalence are unperformed.

```sh
fsl-x86 plan-binary PACKAGE BINARY ADDRESS SIZE c
fsl-x86 bundle-binary PACKAGE BINARY ADDRESS SIZE rust NEW_DIRECTORY
```

The CLI uses the builtin C/Rust registry. It admits and renders before creating
the output directory, refuses existing directories, and writes `output.c` or
`output.rs`, `projection.txt`, and `build.txt`. Filesystem write failures can
leave a partial new directory; publication is not transactional.

## Acceptance

Six backend tests cover input borrowing/source compatibility, dialect/runtime/
memory/foreign-plan refusal, invalid FIR and unsupported locations, registry
extension and descriptor limits, and x86 origin/transaction requirements. The
research repository owns locked bundle metadata and a six-source compilation
gate. Existing source goldens remain unchanged. Execution regression is a
separate gate over the existing three bounded immediate-store fixtures.

Next: a Go provider over the same explicit bitvector/state contract, with
compilation and differential execution gates. General Python/JVM/WASM frontend
and output support remain later work; a registered language name alone is not
support.
