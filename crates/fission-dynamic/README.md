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
