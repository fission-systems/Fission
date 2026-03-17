# Ghidra Vs Fission: Missing Features Analysis

This note compares the legacy Ghidra-core-backed decompilation path and the
analysis layers Fission adds on top of it. The main point is simple:

Fission historically reused Ghidra's decompiler core directly, so the question
was not "which core decompiler features are missing?" but rather "where do the
outputs differ, and why?"

## Executive Summary

When the legacy engine path is active, Fission already executes Ghidra's core
decompilation pipeline, including:

- simplification rules
- type inference
- control-flow restructuring
- variable merging and cleanup

So the largest differences were typically not missing decompiler passes. They
were usually one of these:

1. output style differences
2. extra Fission-side analysis layers
3. postprocess and naming conventions

## What Ghidra Already Provides

In the legacy path, Ghidra's core action pipeline already performs work such as:

- unreachable-code cleanup
- SSA construction
- parameter and return recovery
- dead-code removal
- type inference
- constant-pointer propagation
- large sets of simplification rules
- block structuring and control-flow cleanup
- high-level variable assignment and merge passes

That means Fission did not need to re-implement those capabilities in the
legacy engine path just to reach baseline functionality.

## What Fission Added On Top

Historically, Fission added extra analysis around the Ghidra core, including:

- pointer-return inference
- structure analysis
- global data analysis
- additional type propagation
- data-section scanning for strings and constants

These layers were intended to improve type quality, global-data recovery, and
Windows-oriented output quality rather than replace the decompiler core itself.

## Areas Where Output Commonly Differed

### Variable Naming

One of the largest practical differences was variable naming style.

Typical examples:

- Ghidra-style names such as `local_XX`
- Fission-style stack names such as `xStack_XX` or `uStack_XX`

This had a strong effect on text-based similarity metrics even when the actual
meaning of the code was close.

### Type Naming

Another large difference was type-surface choice.

Examples:

- Ghidra-style names such as `undefined4`, `uint`
- Windows-oriented names such as `DWORD`, `UINT`

Again, this often affected similarity scores more than semantic quality.

### Explicit Casts

Ghidra sometimes emitted casts more aggressively than Fission's output path.
That difference usually did not change semantics, but it changed the surface
form of the pseudocode.

## Features Fission Historically Strengthened

Compared to plain legacy-core output, Fission often added:

- better Windows type vocabulary
- stronger structure-oriented analysis
- global-data inspection
- signature- and symbol-driven enrichment

In other words, the project was often stronger in enrichment layers than in raw
text similarity to stock Ghidra output.

## What Was Actually Missing

The conclusion of the original analysis was that the core issue was rarely a
missing decompiler capability inside the legacy engine path.

More often, the real gaps were:

- output-style incompatibility with Ghidra expectations
- inconsistent naming and type normalization
- places where extra casts or standard surface conventions improved readability

## Practical Implication

If the goal is "closer to stock Ghidra output," the highest-leverage work is
often:

- naming normalization
- type-surface normalization
- cast-surface cleanup

If the goal is "better overall project recovery," those style gaps are less
important than:

- richer type facts
- stronger symbol recovery
- better struct/class recovery
- more stable preview-first control-flow recovery

## Conclusion

The original takeaway was:

- Fission did not primarily suffer from missing legacy-core decompiler passes
- the larger differences came from surface conventions and added analysis layers
- style normalization could close a large part of the visible gap

That conclusion still matters as historical context, even though the newer
Rust-owned preview pipeline is increasingly becoming the more important
long-term direction.
