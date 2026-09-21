# Proposal: Normalize DWARF CFA stack hints before HIR naming

Status: investigation complete; implementation pending measured validation

Issue: GitHub #118 (`memory_layouts_gcc_O2.exe`, `main`, `0x140002830`)

## Observed baseline

The real `memory_layouts.c` row declares `a`, `b`, and `c` at DWARF
`DW_OP_fbreg` offsets `-64`, `-48`, and `-32`.  The compiler emits a normal
x86-64 frame-pointer prologue, whose CFA is `rbp + 16`.  The builder's
canonical stack-slot origins are therefore `-48`, `-32`, and `-16`.

The raw builder/PreHIR output preserves the three distinct slots and passes
the call carriers correctly (`local_30`, `local_20`, `local_80`).  The final
type-hint stage currently matches the DWARF offsets directly, renaming the
builder slots as `b`, `c`, and `local_80`; the source-level slot identity is
therefore shifted and `c` loses its debug name/type.  This is an observed
real-binary defect, not a synthetic-only hypothesis.

## Owner and invariant

The owner is the debug-fact to NIR/HIR hint boundary.  `DwarfLocation::StackOffset`
is relative to the DWARF frame base, while `NirBindingOrigin::StackOffset` is
relative to the builder's canonical stack coordinate.  The hint contract must
carry the source frame-base kind, and the builder must translate a
`DW_AT_frame_base = DW_OP_call_frame_cfa` offset using the frame layout it
already proved from entry p-code.  User/structural hints remain in the
canonical builder coordinate.

The rule must be deterministic and architecture-neutral at the semantic
boundary: no function name, binary address, or corpus guard.  When the frame
layout cannot establish a safe conversion, the debug hint remains unmatched
rather than being guessed.

## Validation plan

1. Add loader/fact coverage for the DWARF frame-base marker and a focused
   hint-coordinate regression.
2. Re-run the affected real `memory_layouts` rows across available compiler
   and optimization variants with caches disabled.
3. Compare final slot names/types, call arguments, compilation status, and
   source-semantic metrics before/after.
4. Run the relevant Rust nextest/check/build gates and the DecBench/source
   semantic smoke path.

Success requires the real row to recover `a`, `b`, and `c` on their actual
   slots without regressing existing x86/x64 debug-hint tests.  Synthetic
tests alone will be reported only as mechanical coverage.
