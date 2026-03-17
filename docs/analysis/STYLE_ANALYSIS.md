# Output Style Analysis

This note discusses historical style differences between stock Ghidra-like
output and the surface conventions Fission used in older paths.

The point of this document is not that one style is universally correct. It is
to explain why text-level similarity could differ sharply even when the
underlying semantics were close.

## Main Style Differences

### Type Names

Examples:

| Ghidra-style | Fission-style | Notes |
|---|---|---|
| `undefined4` | `DWORD` | Windows-oriented surface |
| `uint` | `UINT` | Windows-oriented surface |
| `uint *` | `void *` | More aggressive generic pointer surface in some cases |

### Variable Names

Examples:

| Ghidra-style | Fission-style | Notes |
|---|---|---|
| `local_38` | `xStack_38` | stack-local naming difference |
| `local_c` | `uStack_c` | unsignedness reflected in name |
| `local_18` | `pvStack_18` | pointer-ish prefixing |

### Cast Surface

Examples:

| Ghidra-style | Fission-style |
|---|---|
| `sum_array((longlong)&local_38, 5)` | `sum_array(&xStack_38, 5)` |

These differences often changed benchmark similarity scores much more than they
changed meaning.

## Why Style Still Matters

Even if two outputs are semantically close, style differences affect:

- benchmark diff quality
- interoperability with Ghidra-oriented expectations
- ease of comparing output against stock Ghidra
- user familiarity for reverse engineers already trained on Ghidra conventions

## Benefits Of Moving Closer To Ghidra-Like Surface

Potential advantages:

- easier output comparison
- improved similarity metrics
- more predictable ecosystem expectations
- easier mapping to public tutorials and existing reverse-engineering habits

## Downsides Of Pure Ghidra-Like Surface

Potential tradeoffs:

- loss of explicit Windows-oriented type vocabulary
- less platform-specific readability for Windows-first workflows
- friction for users who prefer semantic type aliases such as `DWORD`

## Historical Options Considered

### Option A: Default To Ghidra-Like Style

Pros:

- stronger benchmark alignment
- easier comparison with legacy Ghidra outputs
- more conventional reverse-engineering surface

Cons:

- weaker Windows-specific naming flavor
- reduced explicitness for some users

### Option B: Configurable Output Style

Pros:

- supports both Ghidra-like and Windows-oriented consumers
- lets benchmarking and user experience diverge cleanly when needed

Cons:

- more maintenance
- more printer and postprocess complexity

### Option C: Keep Existing Fission Surface

Pros:

- clearer Windows-centric type naming
- stronger product identity

Cons:

- lower text similarity to stock Ghidra
- harder benchmark comparison

## Practical Takeaway

The original conclusion was that style normalization was a relatively
high-leverage way to improve comparison quality without needing deeper
decompiler-core changes.

That remains true historically, even though the current long-term direction is
increasingly centered on the Rust-owned preview pipeline rather than strict
surface similarity to legacy Ghidra output.
