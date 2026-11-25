# `fission-dynamic`

Dynamic analysis / interactive runtime / unpacker engine for Fission.

## Smoke API (always built)

- **`runtime_status()`** — compile-time feature flags + host OS (`std::env::consts::OS`). See crate-level docs in [`src/lib.rs`](src/lib.rs).

```bash
cargo run -p fission-dynamic --example runtime_status --no-default-features
cargo test -p fission-dynamic --no-default-features
```

## Feature builds

| Feature | Notes |
|---------|--------|
| `interactive_runtime` | Heavy stack (Tokio, plugins, OS helpers). **`nix`** on Linux; **`windows`** on Windows. **`cargo check --features interactive_runtime`** succeeds on macOS (verified in development); expect longer builds. |
| `windows_native_debugger` | Enables the opt-in live Win32 debugger on Windows. The feature implies `interactive_runtime`; Windows CI checks the dynamic crate, CLI, and native debugger tests. |
| `unpacker_runtime` | Intended for **Windows** targets (`windows` sys crates). On **macOS**, `cargo check -p fission-dynamic --features unpacker_runtime` has been observed to succeed (cross-target stubs); Linux/CI should still treat Windows as the primary validation OS for unpack behavior. |

The emulator backend remains available independently of the native Win32 debugger feature.

## Memory Read Semantics and Sparse Coverage

Unread or short-transferred memory bytes are never represented as observed `0x00` bytes.

| Contract | API Surface | Typical Consumers | Behavior on Short / Unreadable Bytes |
|---|---|---|---|
| **Exact read** | `MemoryManager::{read, read_exact, read_u8, read_u16, read_u32, read_u64}`, `ExecutionBackend::read_memory` | PE/ELF headers, import/export tables, stack slots, breakpoints, register-width scalar reads | Fails with `MemoryError::ShortRead` (or backend error); never returns zero-padded tails |
| **Explicit partial read** | `MemoryManager::{read_partial, read_into}`, `ExecutionBackend::read_memory_partial` | Page-edge string scans and probing near segment boundaries | Returns only transferred prefix bytes (`0..=requested`) plus `trailing_gap: Option<MemoryGap>` |
| **Sparse acquisition** | `MemoryManager::read_sparse`, `ExecutionBackend::read_memory_sparse` | Module/region dumps, sparse forensic snapshots, pattern scans | Emits ordered `MemorySegment::Observed` (preserving genuine zeros) and `MemorySegment::Gap` (`unmapped`, `denied`, `partial`, `process_exited`, `budget`, `read_error`) |

A `SparseMemoryCapture` can only be built through the fallible `SparseMemoryCapture::from_segments`, which checks in every build profile that the segments are an exact partition of `[start, start + requested_len)`: non-empty, in address order, each starting where the previous one ended, inside the requested range, with no `u64` overflow, and covering it to the end. Duplicates, overlaps, holes, out-of-order or out-of-range segments and short covers are rejected with `SparseCaptureError` (surfaced by `read_sparse` as `MemoryError::InvalidSparseCapture`). Fields are private, so `coverage()` (`requested_bytes`, `observed_bytes`, `missing_bytes`, `observed_runs`, `gap_count`, `complete`) always satisfies `observed_bytes + missing_bytes == requested_bytes`, and `complete` means every requested address was observed exactly once. `read_exact_slice` fails if any requested byte lies inside a gap, and allocates at most the observed byte count, never the span a gap describes. `PartialRead` likewise exposes its prefix and `trailing_gap()` through accessors only.
