# Build Guide

Complete guide for building Fission from source on Windows, Linux, and macOS.

---

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Build (TL;DR)](#quick-build-tldr)
- [Platform-Specific Instructions](#platform-specific-instructions)
  - [Windows](#windows)
  - [Linux (Ubuntu/Debian)](#linux-ubuntudebian)
  - [Linux (Fedora/RHEL)](#linux-fedorarhel)
  - [macOS](#macos)
- [Build Options](#build-options)
- [Troubleshooting](#troubleshooting)
- [Development Build](#development-build)
- [Cross-Compilation](#cross-compilation)

---

## Prerequisites

### Common Requirements

| Component | Version | Purpose |
|-----------|---------|---------|
| Rust | 1.85+ | Core language |
| CMake | 3.16+ | Building Ghidra decompiler |
| C++ Compiler | C++17 | Decompiler components |
| zlib | Latest | Compression library |

### Optional Requirements

| Component | Required For |
|-----------|--------------|
| vcpkg | Windows zlib management |
| pkg-config | Linux library detection |

---

## Quick Build (TL;DR)

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Clone repository
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# 3. Build Fission CLI
cargo build --release --bin fission_cli

# 4. Run
./target/release/fission_cli                    # CLI
# GUI: cd crates/fission-tauri && npm install && npm run tauri dev
```

**Build time**: ~5-10 minutes (first build)  
**Disk space**: ~2 GB (with debug symbols)

---

## Platform-Specific Instructions

### Windows

#### Step 1: Install Visual Studio 2022

Download and install [Visual Studio 2022](https://visualstudio.microsoft.com/downloads/) with:
- **Desktop development with C++** workload
- **CMake tools for Windows**
- **C++ CMake tools for Linux** (optional, for cross-compile)

Or install via command line:
```powershell
winget install Microsoft.VisualStudio.2022.Community --override "--add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended"
```

#### Step 2: Install Rust

```powershell
# Using winget
winget install Rustlang.Rustup

# Or download installer
# https://rustup.rs/
```

After installation, restart your terminal.

#### Step 3: Install CMake

```powershell
winget install Kitware.CMake
```

Or download from [cmake.org](https://cmake.org/download/).

#### Step 4: Install vcpkg and zlib

```powershell
# Clone vcpkg
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
cd C:\vcpkg

# Bootstrap vcpkg
.\bootstrap-vcpkg.bat

# Install zlib
.\vcpkg install zlib:x64-windows

# Set environment variable (PowerShell)
$env:VCPKG_ROOT = "C:\vcpkg"
[System.Environment]::SetEnvironmentVariable("VCPKG_ROOT", "C:\vcpkg", "User")
```

#### Step 5: Clone and Build Fission

```powershell
# Clone repository
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# Build Fission
cargo build --release --bin fission_cli
```

#### Step 6: Run Fission

```powershell
# CLI mode
.\target\release\fission_cli.exe test.exe

# GUI (Tauri): from repo root
cd crates\fission-tauri
npm install
npm run tauri dev
```

#### Windows-Specific Notes

No extra native runtime copy step is required for the default Rust-only CLI build.

**Antivirus**: Some antivirus software may flag Fission. Add exclusion for the `target/` directory.

---

### Linux (Ubuntu/Debian)

#### Step 1: Install Build Tools

```bash
sudo apt update
sudo apt install -y \
    build-essential \
    cmake \
    pkg-config \
    zlib1g-dev \
    libssl-dev \
    git
```

#### Step 2: Install GUI Dependencies (Optional)

```bash
# For GUI support
sudo apt install -y \
    libgtk-3-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libfontconfig1-dev
```

#### Step 3: Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Step 4: Clone and Build

```bash
# Clone repository
git clone https://github.com/sjkim1127/Fission.git
cd Fission

# Build Fission (with all features)
cargo build --release

# Or CLI only (smaller binary)
cargo build --release --no-default-features --features cli
```

#### Step 5: Run Fission

```bash
# CLI mode
./target/release/fission_cli test.exe

# GUI (Tauri): from repo root
cd crates/fission-tauri && npm install && npm run tauri dev

# Install CLI system-wide (optional)
sudo cp target/release/fission_cli /usr/local/bin/
```

#### Ubuntu/Debian-Specific Notes

For the default Rust-only CLI build, extra `LD_LIBRARY_PATH` setup is not required.

---

### Linux (Fedora/RHEL)

#### Step 1: Install Build Tools

```bash
# Fedora
sudo dnf install -y \
    gcc gcc-c++ \
    cmake \
    pkg-config \
    zlib-devel \
    openssl-devel \
    git

# RHEL 8/9
sudo dnf install -y \
    gcc gcc-c++ \
    cmake \
    pkg-config \
    zlib-devel \
    openssl-devel \
    git
```

#### Step 2: Install GUI Dependencies (Optional)

```bash
sudo dnf install -y \
    gtk3-devel \
    libxcb-devel \
    libxkbcommon-devel \
    fontconfig-devel
```

#### Step 3: Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Step 4: Clone and Build

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission

cargo build --release
```

---

### macOS

#### Step 1: Install Xcode Command Line Tools

```bash
xcode-select --install
```

#### Step 2: Install Homebrew (if not installed)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

#### Step 3: Install Dependencies

```bash
brew install cmake pkg-config zlib
```

#### Step 4: Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### Step 5: Clone and Build

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission

cargo build --release
```

#### Step 6: Run Fission

```bash
# CLI
./target/release/fission_cli

# GUI (Tauri)
cd crates/fission-tauri && npm install && npm run tauri dev
```

#### macOS-Specific Notes

**Code Signing**: Unsigned binaries may be blocked by Gatekeeper:

```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine target/release/fission_cli

# Or disable Gatekeeper temporarily
sudo spctl --master-disable
```

**Apple Silicon (M1/M2)**: Fission builds natively for ARM64. No Rosetta required.

---

## Build Options

### Feature Flags

Customize the build with Cargo features:

```bash
# CLI (recommended for first build)
cargo build --release --bin fission_cli

# GUI (Tauri): build from Tauri crate
cd crates/fission-tauri && npm run tauri build

# Without default feature set
cargo build --release --bin fission_cli --no-default-features
```

### Available Features

| Feature | Description | Default |
|---------|-------------|---------|
| `gui` | Tauri 2.x + React 19 desktop GUI — build with `cd crates/fission-tauri && npm run tauri build` | Separate app |
| `cli` | CLI binary: `fission_cli` | `cargo build --bin fission_cli` |
| `tui` | Terminal UI (ratatui) | ❌ No |

### Optimization Levels

```bash
# Debug build (fast compile, slow runtime)
cargo build

# Release build (slow compile, fast runtime)
cargo build --release

# Maximum optimization (even slower compile)
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

### Build Profiles

Defined in `Cargo.toml`:

```toml
[profile.release]
opt-level = 3        # Maximum optimization
lto = true          # Link-time optimization
strip = true        # Strip symbols (smaller binary)
codegen-units = 1   # Better optimization, slower compile
```

---

## Troubleshooting

### CMake Not Found

```bash
# Linux
sudo apt install cmake  # Ubuntu/Debian
sudo dnf install cmake  # Fedora/RHEL

# macOS
brew install cmake

# Windows
winget install Kitware.CMake
```

### zlib Not Found

**Linux**:
```bash
sudo apt install zlib1g-dev     # Ubuntu/Debian
sudo dnf install zlib-devel     # Fedora/RHEL
```

**macOS**:
```bash
brew install zlib
# If CMake still can't find it:
export ZLIB_ROOT=$(brew --prefix zlib)
```

**Windows**:
```powershell
# Install via vcpkg
C:\vcpkg\vcpkg install zlib:x64-windows

# Set environment variable
$env:VCPKG_ROOT = "C:\vcpkg"
```

### Linker Errors

If you see linker errors, make sure toolchains and system dependencies are installed correctly, then run a clean rebuild:

```bash
cargo clean
cargo build --release --bin fission_cli
```

### Rust Version Too Old

```bash
# Update Rust
rustup update stable

# Check version
rustc --version  # Should be 1.85+
```

### Out of Memory During Build

```bash
# Limit parallel jobs
cargo build --release -j 2

# Or increase swap space (Linux)
sudo fallocate -l 8G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

### GUI Build Fails on Headless Server

```bash
# Build CLI only (no GUI dependencies)
cargo build --release --no-default-features --features cli
```

## Development Build

### Debug Build (Fast Iteration)

```bash
# Build without optimizations (faster compile)
cargo build

# Run CLI
cargo run --bin fission_cli -- test.exe

# Run with debug logging
RUST_LOG=debug cargo run --bin fission_cli -- test.exe
```

### Tracing and preview diagnostics

The CLI installs a `tracing` subscriber at startup (via `fission_core::logging::try_init_tracing`). Default filter is `warn`; `-v` / `--verbose` raises the default to `info`. If `RUST_LOG` is set and non-empty, it defines the filter and overrides the CLI default.

| Variable | Role |
|----------|------|
| `RUST_LOG` | `tracing-subscriber` `EnvFilter` (for example `fission_pcode::nir::normalize=debug`, `fission_static::analysis::decomp::nir=trace`). |
| `FISSION_PREVIEW_DIAG` | Extra stderr diagnostics from NIR normalize and preview paths (presence enables). |
| `FISSION_PREVIEW_PERF` | Per-pass timing lines during NIR normalize (presence enables). |

Snapshot-based tests use [insta](https://github.com/mitsuhiko/insta): after intentional output changes, run `cargo insta test --accept` (or `INSTA_UPDATE=1 cargo test …`) and review with `cargo insta review`.

### Watch Mode (Auto-Rebuild)

```bash
# Install cargo-watch
cargo install cargo-watch

# Auto-rebuild on changes
cargo watch -x "build"

# Auto-run tests
cargo watch -x "test"
```

### Incremental Compilation

Already enabled by default. To disable:
```bash
CARGO_INCREMENTAL=0 cargo build
```

### Build Cache

Use [sccache](https://github.com/mozilla/sccache) to speed up rebuilds:

```bash
# Install sccache
cargo install sccache

# Configure Rust to use it
export RUSTC_WRAPPER=sccache

# Check stats
sccache --show-stats
```

### Development Tools

```bash
# Install useful tools
cargo install cargo-edit      # cargo add/rm/upgrade
cargo install cargo-outdated  # Check outdated dependencies
cargo install cargo-audit     # Security audit
cargo install cargo-bloat     # Analyze binary size
```

---

## Cross-Compilation

### Linux → Windows

```bash
# Install MinGW toolchain
sudo apt install mingw-w64

# Add Rust target
rustup target add x86_64-pc-windows-gnu

# Build
cargo build --release --target x86_64-pc-windows-gnu
```

**Note**: Native decompiler cross-compilation requires additional setup.

### macOS → Linux

```bash
# Install cross-compilation tools
brew install FiloSottile/musl-cross/musl-cross

# Add target
rustup target add x86_64-unknown-linux-musl

# Build static binary
cargo build --release --target x86_64-unknown-linux-musl
```

---

## Build Performance

### Typical Build Times

| Configuration | First Build | Incremental | Clean Release |
|---------------|-------------|-------------|---------------|
| Debug (dev) | 2-3 min | 10-30s | 5-8 min |
| Release | 5-8 min | 20-60s | 8-12 min |
| CLI Only | 1-2 min | 10-20s | 3-5 min |

*Times measured on: Intel i7-10700K, 32GB RAM, NVMe SSD*

### Optimization Tips

1. **Use faster linker** (Linux):
```bash
# Install mold
sudo apt install mold

# Use it
RUSTFLAGS="-C link-arg=-fuse-ld=mold" cargo build --release
```

2. **Parallel compilation**:
```bash
# Use all CPU cores (default)
cargo build -j $(nproc)

# Limit cores to avoid OOM
cargo build -j 4
```

3. **Incremental builds**:
   - Already enabled for dev builds
   - Disabled for release (better optimization)

---

## Binary Size

### Size Comparison

| Configuration | Size (Linux) | Size (Windows) |
|---------------|--------------|----------------|
| Debug | ~150 MB | ~180 MB |
| Release | ~40 MB | ~50 MB |
| Release (stripped) | ~15 MB | ~20 MB |
| CLI only | ~8 MB | ~10 MB |

### Reduce Binary Size

```bash
# 1. Strip symbols (already done in release profile)
strip target/release/fission_cli

# 2. Use UPX compression
upx --best --lzma target/release/fission_cli

# 3. Build with size optimization
RUSTFLAGS="-C opt-level=z" cargo build --release

# 4. CLI only (smaller binary)
cargo build --release --bin fission_cli
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Build

on: [push, pull_request]

jobs:
  build:
    strategy:
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    
    runs-on: ${{ matrix.os }}
    
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true
      
      - name: Install dependencies (Ubuntu)
        if: matrix.os == 'ubuntu-latest'
        run: |
          sudo apt update
          sudo apt install -y zlib1g-dev
      
      - name: Build Fission
        run: cargo build --release
      
      - name: Run tests
        run: cargo test --release
```

---

## Verification

### Test Build

```bash
# Run basic tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_loader
```

### Verify Decompiler

```bash
# Test CLI
./target/release/fission_cli --info examples/struct_test.exe
# Or with path to a PE/ELF/Mach-O sample
./target/release/fission_cli --info <path-to-binary>
```

### Check Dependencies

```bash
# Linux
ldd target/release/fission_cli

# macOS
otool -L target/release/fission_cli

# Windows (PowerShell)
dumpbin /dependents target\release\fission_cli.exe
```

---

## Rust workspace / rust-analyzer

### `failed to read .../fission-tauri/src-tauri/Cargo.toml` (No such file or directory)

- **Cause**: This happens when the workspace member path `crates/fission-tauri/src-tauri` in the root `Cargo.toml` does not contain a `Cargo.toml`.
- **Action**:
  1. **Verify the full clone/sync state**: run `git status`, `git pull`, then `ls crates/fission-tauri/src-tauri/Cargo.toml` to confirm the file exists.
  2. **If you are working without the GUI**:
     - comment out the `"crates/fission-tauri/src-tauri",` line in the root `Cargo.toml`, or
     - replace the workspace with the CLI-only variant by running `cp Cargo.toml.workspace-cli-only Cargo.toml`.
     When you need the GUI build again, restore the default file with `git checkout Cargo.toml` and confirm the path exists.
  3. **Restart rust-analyzer**: `Ctrl+Shift+P` → `Rust-analyzer: Restart server`.

### `file not found: .../fission-cli/src/bin/ffi_test.rs`

- **Cause**: This can happen when the old `ffi_test` binary was removed but the IDE or rust-analyzer still has the file open or cached in its index.
- **Action**:
  1. Close the `ffi_test.rs` tab if it is still open.
  2. **Restart rust-analyzer**: `Ctrl+Shift+P` → `Rust-analyzer: Restart server`.
  3. `fission-cli` now defines only the `fission_cli` binary, so it is normal for `ffi_test` references to be gone.

---

## Decompiler logging

Decompiler preparation (binary load, sections, symbols, FID, and related setup) now flows through a single path in `fission-analysis`. Both the CLI and GUI call the same entry point. See [ARCHITECTURE.md](../architecture/ARCHITECTURE.md) for the per-binary preparation contract. Initialization cost can be inspected step-by-step through JSON `_meta.prepare_timings` when `--benchmark` is enabled. Performance priorities are also documented in [ARCHITECTURE.md](../architecture/ARCHITECTURE.md).

Decompiler (C++) diagnostic logging is controlled only through the following paths:

- **Config**: use `[decompiler]` in `fission.toml`, with `log_verbose` (default `false`) and `log_file` (default `""`). If `log_file` is non-empty, logs are appended there.
- **CLI**: overridden by `--verbose`. The effective value is `config.decompiler.log_verbose || cli.verbose`.
- **Errors**: failures always propagate through `last_error` into a Rust `Result`, independently of the log stream.
- **OutputSilencer**: when the CLI is not verbose, stderr is redirected to `/dev/null` so third-party stderr output is suppressed even if C++ logging is disabled.

See [ARCHITECTURE.md](../architecture/ARCHITECTURE.md) for the detailed logging and error-handling contract.

---

## Related Documentation

- [README.md](../README.md) - Project overview
- [PLUGIN_DEVELOPMENT.md](../plugins/PLUGIN_DEVELOPMENT.md) - Plugin development
- [ARCHITECTURE.md](../architecture/ARCHITECTURE.md) - System architecture

---

## Getting Help

- **GitHub Issues**: https://github.com/sjkim1127/Fission/issues
- **Discussions**: https://github.com/sjkim1127/Fission/discussions
- **Matrix Chat**: Coming soon

---

## Summary

**Minimum steps**:
1. Install Rust + platform dependencies
2. Build Fission CLI: `cargo build --release --bin fission_cli`
3. Run: `./target/release/fission_cli` (for the GUI, run `npm run tauri dev` in `crates/fission-tauri`)

**Most common issues**:
- ❌ Rust toolchain mismatch → `rustup update stable`
- ❌ zlib not found → Install zlib-dev package
- ❌ GUI dependency mismatch → install GTK/WebKit/Tauri deps
- ❌ Linker errors → `cargo clean && cargo build --release --bin fission_cli`

**Build time**: ~5-10 minutes (first build), ~30s (incremental)  
**Disk space**: ~2 GB (with debug info)
