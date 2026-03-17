# Refactoring Progress Log

This document tracks large-scale cleanup and maintainability work across the
Fission codebase. It is an internal engineering note, not a formal roadmap.

## Summary Of Findings

The original audit identified more than 1,200 improvement opportunities.
They were grouped roughly as follows:

| Category | Approx. count | Notes |
|---|---:|---|
| Hardcoded values and paths | 270 | Constants, fixed paths, magic numbers |
| Allocation and performance issues | 100 | Mostly avoidable `String` churn |
| Crash-risk `unwrap` / `expect` usage | 200 | High priority stability work |
| Logging inconsistencies | 150 | `println!` / `eprintln!` mixed with tracing |
| `unsafe` documentation / review | 155 | Mostly justification and boundary cleanup |
| Structural refactors | 110 | Duplication, ownership cleanup, module boundaries |
| Documentation gaps | 50 | Missing or stale docs |
| TODO / FIXME debt | 200 | Mostly medium-priority cleanup items |

These numbers were never intended to be precise permanent metrics. They were
used as a working inventory to prioritize refactoring phases.

## Phase 1: Foundation Work

Status: complete

### Constants Library

Completed:

- Introduced a shared constants module under `crates/fission-core/src/constants/`
- Split constants into focused files such as:
  - binary format constants
  - Windows API constants
  - memory-size and alignment constants
- Exported the constants module from `fission-core`
- Replaced a first wave of loader-side magic numbers with named constants

Impact:

- Reduced repeated binary-format constants
- Improved readability in PE / ELF / Mach-O paths
- Lowered the chance of accidental inconsistencies when extending loaders

### Configuration Review

Completed:

- Confirmed that `fission.toml`-based configuration already existed
- Verified that core path settings and environment override handling were in place

Remaining follow-up:

- Continue replacing hardcoded paths with explicit config lookups
- Add more targeted environment-variable overrides where useful

### Logging Direction

Current direction:

- Keep explicit CLI output where the user expects it
- Keep internal diagnostics on `tracing`
- Gradually remove ad-hoc `println!` and `eprintln!` usage from library code

## Phase 2: Stability Work

Status: substantially complete for the first pass

Goal:

- remove high-risk `unwrap` / `expect` usage
- turn malformed inputs into recoverable errors instead of panics

### CLI Paths

Completed:

- Replaced JSON serialization `unwrap` calls in one-shot CLI commands
- Normalized serialization failures into explicit `io::Error` or command-level errors

Representative change:

```rust
// Before
serde_json::to_string_pretty(&data).unwrap()

// After
serde_json::to_string_pretty(&data)
    .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?
```

### Loader Paths

Completed:

- Removed `unwrap` usage from several Mach-O parsing paths
- Replaced byte-slice conversion panics with safe bounds-checked logic
- Improved error messages for malformed binary structures

Representative change:

```rust
// Before
LoadCommand::read_options(&mut reader, endian, ()).unwrap()

// After
LoadCommand::read_options(&mut reader, endian, ())
    .map_err(|e| err!(loader, "Failed to read Mach-O load command: {}", e))?
```

### Visualization / Postprocess / Formatting Paths

Completed:

- Removed `unwrap` from DOT graph rendering paths
- Replaced fragile regex initialization with clearer panic messages when a
  hardcoded pattern is invalid
- Removed formatting-path `unwrap` usage from disassembly output

## Phase 3: Allocation And String Churn

Status: planned / partially addressed

Goal:

- reduce postprocess memory churn
- replace eager `String` rebuilding with more selective borrowed-or-owned flows

Primary targets:

- arithmetic cleanup
- loop cleanup
- naming cleanup

The intended direction was a gradual `String` to `Cow<'_, str>` style migration
or equivalent localized allocation reduction.

## Later Cleanup Phases

These later phases were tracked as buckets rather than strict project
milestones:

- hardcoded values and paths
- logging consolidation
- `unsafe` review and documentation
- duplication and module-boundary cleanup
- documentation cleanup

## Progress Snapshot

The original internal estimate placed the effort around one-third complete after
the first two major passes. The more important outcome, however, was not the
percentage but the direction:

- core constants and configuration boundaries became clearer
- panic-heavy loader and CLI paths were reduced
- future cleanup work became easier to stage by category

## Notes

- This document is intentionally approximate. It captures refactoring direction
  and engineering intent, not a strict audited metric set.
- Some references in the original Korean draft reflected the code state at the
  time of the audit and may no longer match the exact current file layout.
