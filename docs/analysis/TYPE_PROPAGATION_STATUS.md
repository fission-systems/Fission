# Type Propagation Improvement Status

This note records an earlier round of type-propagation and data-symbol work in
the legacy native decompilation path.

The focus was not purely on abstract type inference quality. It was also on the
surface problem that floating-point constants and data-section references often
printed as raw hexadecimal values instead of meaningful symbolic references.

## Main Issues Tracked

### Local Variable Presentation

Problem:

- stack locals were sometimes collapsed into a synthetic stack-struct-style
  surface, reducing readability

Resolution:

- disable the older `StackFrameAnalyzer` path in favor of the native core's
  built-in local-variable handling where it produced better output

Result:

- clearer per-local variable presentation

### Constant Presentation

Problem:

- floating-point values sometimes appeared as raw integer-like hex literals
  instead of meaningful symbolic references or readable values

Example of the bad surface:

```c
create_item(..., 0x4048feb851eb851f);
```

Desired surface:

```c
create_item(..., DAT_1400040c8);
```

or, where appropriate, a more readable literal/value-oriented form.

## Implemented Direction

The work combined a few pieces:

- data-section scanning
- data-symbol registration
- preservation of symbol references during decompilation
- pointer-propagation support for relevant load patterns

## Key Components

### Data-Section Scanner

Added logic to scan relevant data sections and identify likely floating-point
or other typed data values worth promoting into explicit symbols.

### Data Symbol Registry

Added registration of those discovered data items into the global scope so the
decompiler had stable symbolic references to point at.

### Pipeline Integration

Integrated data-section scanning into the binary-load / decompile lifecycle so
the symbols would be available when decompilation ran.

### Pointer / Read-Only Handling

The critical practical issue turned out not to be just "detect the symbol."
It was also:

- preserve the symbol reference
- avoid prematurely replacing memory reads with raw constant inlining

That required changes so read-only data handling would not destroy useful
symbolic references too early in the process.

## Why This Mattered

Without this work, the output could be technically correct but much harder to
read:

- raw hex bit-patterns instead of meaningful data references
- loss of similarity against symbol-oriented native output
- harder reasoning about what constant data a call was actually using

With the change, the output could keep references like:

```c
DAT_1400040c8
```

instead of flattening them into opaque machine-level literals.

## Observed Outcome

The original internal comparison recorded:

- clearer local-variable output
- automatic creation of data-section symbols
- preservation of those symbol references in decompiled output
- much closer alignment with Ghidra-style symbolic data references

## Architectural Lesson

This work was a good example of a recurring pattern in Fission:

the biggest readability gain often did not come from inventing a new abstract
analysis pass. It came from making sure the right semantic object survived long
enough in the pipeline to reach the printer:

- symbol instead of hex literal
- local variable instead of synthetic stack aggregate
- typed data reference instead of flattened constant blob

## Related Notes

For related historical analysis, see:

- [`TYPE_PROPAGATION_ANALYSIS.md`](./TYPE_PROPAGATION_ANALYSIS.md)
- [`IMPROVEMENT_LOG.md`](./IMPROVEMENT_LOG.md)
- [`KNOWN_ISSUES.md`](./KNOWN_ISSUES.md)
