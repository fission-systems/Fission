# Automatic String Constant Inlining

This note documents a completed improvement in the legacy native pipeline:
recovering data-section string references as inline string literals instead of
raw `DAT_...` pointers.

## Problem

Before this work, calls that should have displayed readable strings often came
out like this:

```c
puts(&DAT_140004038);
```

That was technically valid but much less readable than the expected output:

```c
puts("=== Fission Decompiler Comparison Test ===\n");
```

## Goal

Detect string-like data in scanned data sections, register it with the correct
datatype, and let the existing native C printer inline the actual string when a
pointer references that data.

## High-Level Approach

The implementation added string-aware data-section scanning and registered those
findings as `char[]` data symbols in the global scope.

That matters because the native printer already knows how to inline character
pointer constants when the referenced data is recognized as printable string
content.

## Main Changes

### Data-Section String Detection

Added string detection to the data-section scanner with heuristics such as:

- minimum length threshold
- required null terminator
- mostly printable ASCII / UTF-8-compatible content
- support for common whitespace characters like newline and tab

The scan order was intentionally staged:

1. strings first
2. doubles second
3. floats last

This reduced overlap problems where numeric scanning could otherwise consume
regions that were better represented as strings.

### String Symbol Registration

String findings were registered as `char[]`-typed data symbols rather than
opaque raw addresses.

That datatype choice is the key step that lets the native printer treat the
memory as printable character data instead of a generic pointer target.

## Why It Worked

The improvement did not require inventing a new string-printing system.
Instead, it aligned Fission's data-symbol registration with what the native
printer already expected for pointer-to-character constant emission.

In other words:

- better scanning
- better datatype registration
- existing printer logic did the rest

## Observed Effect

Representative before/after behavior:

```c
// Before
printf((char *)((longlong)&DAT_140004060 + 4), ...);

// After
printf("Add: %d, Multiply: %d\n", ...);
```

This substantially improved readability and reduced noisy pointer-surface
artifacts in output comparisons.

## Detection Rules

The original implementation used simple practical heuristics:

- minimum printable length
- maximum scan length
- required null terminator
- printable-character ratio threshold

These rules were intentionally conservative so random data would not be
misclassified too aggressively as strings.

## Follow-Up Opportunities

Potential future extensions noted in the original draft:

- UTF-16 / wide-string handling
- better simplification of pointer arithmetic around string bases
- further surface normalization where type aliases affect comparison quality

## Takeaway

This was a good example of a high-leverage compatibility improvement:

- the core decompiler did not need a major redesign
- the main work was better data-section classification
- once strings were registered correctly, the native printer produced much more
  human-readable output automatically
