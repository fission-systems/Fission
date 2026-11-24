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

Every `SparseMemoryCapture` reports `SparseCoverageSummary` (`requested_bytes`, `observed_bytes`, `missing_bytes`, `observed_runs`, `gap_count`, `complete`) where `observed_bytes + missing_bytes == requested_bytes`, and supports exact slice extraction (`read_exact_slice`) that fails if any requested byte lies inside a gap.
