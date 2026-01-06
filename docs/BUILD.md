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
| Python 3.8+ | Python plugin support |
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

# 3. Build decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
cd ..

# 4. Build Fission
cargo build --release

# 5. Run
./target/release/fission
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

# Build Ghidra decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_TOOLCHAIN_FILE=C:\vcpkg\scripts\buildsystems\vcpkg.cmake
cmake --build build --config Release
cd ..

# Build Fission
cargo build --release
```

#### Step 6: Run Fission

```powershell
# GUI mode
.\target\release\fission.exe

# CLI mode
.\target\release\fission.exe --cli test.exe
```

#### Windows-Specific Notes

**Dynamic Library Path**: The decompiler library (`libdecomp.dll`) must be in the same directory as `fission.exe` or in PATH.

```powershell
# Copy DLL to output directory
copy ghidra_decompiler\build\Release\libdecomp.dll target\release\
```

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

# Build decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j$(nproc)
cd ..

# Build Fission (with all features)
cargo build --release

# Or CLI only (smaller binary)
cargo build --release --no-default-features --features cli
```

#### Step 5: Run Fission

```bash
# GUI mode
./target/release/fission

# CLI mode
./target/release/fission --cli test.exe

# Install system-wide (optional)
sudo cp target/release/fission /usr/local/bin/
```

#### Ubuntu/Debian-Specific Notes

**Library Path**: The decompiler library is automatically found via rpath. If you move the binary:

```bash
# Set LD_LIBRARY_PATH
export LD_LIBRARY_PATH=$LD_LIBRARY_PATH:$(pwd)/ghidra_decompiler/build

# Or copy library
sudo cp ghidra_decompiler/build/libdecomp.so /usr/local/lib/
sudo ldconfig
```

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

cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j$(nproc)
cd ..

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

cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j$(sysctl -n hw.ncpu)
cd ..

cargo build --release
```

#### Step 6: Run Fission

```bash
./target/release/fission
```

#### macOS-Specific Notes

**Code Signing**: Unsigned binaries may be blocked by Gatekeeper:

```bash
# Remove quarantine attribute
xattr -d com.apple.quarantine target/release/fission

# Or disable Gatekeeper temporarily
sudo spctl --master-disable
```

**Apple Silicon (M1/M2)**: Fission builds natively for ARM64. No Rosetta required.

---

## Build Options

### Feature Flags

Customize the build with Cargo features:

```bash
# Default build (GUI + CLI + decompiler)
cargo build --release

# CLI only (minimal dependencies)
cargo build --release --no-default-features --features cli

# GUI only
cargo build --release --no-default-features --features gui

# With Python scripting support
cargo build --release --features python

# All features
cargo build --release --features "gui cli python"

# Without native decompiler (use external Ghidra)
cargo build --release --no-default-features --features "gui cli"
```

### Available Features

| Feature | Description | Default |
|---------|-------------|---------|
| `gui` | egui-based GUI | ✅ Yes |
| `cli` | CLI with REPL | ✅ Yes |
| `native_decomp` | Built-in Ghidra decompiler | ✅ Yes |
| `python` | Python plugin support (PyO3) | ❌ No |
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

**Error**: `cannot find -ldecomp`

**Solution**: Ensure decompiler was built:
```bash
cd ghidra_decompiler
cmake --build build
ls build/libdecomp.*  # Should exist
```

**Error**: `library not loaded: libdecomp.so`

**Solution**: Set library path:
```bash
# Linux
export LD_LIBRARY_PATH=$(pwd)/ghidra_decompiler/build:$LD_LIBRARY_PATH

# macOS
export DYLD_LIBRARY_PATH=$(pwd)/ghidra_decompiler/build:$DYLD_LIBRARY_PATH
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

### Python Feature Build Fails

```bash
# Install Python development headers
sudo apt install python3-dev  # Ubuntu/Debian
sudo dnf install python3-devel  # Fedora/RHEL
brew install python@3.11  # macOS
```

---

## Development Build

### Debug Build (Fast Iteration)

```bash
# Build without optimizations (faster compile)
cargo build

# Run immediately
cargo run -- --cli test.exe

# Run with debug logging
RUST_LOG=debug cargo run
```

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
strip target/release/fission

# 2. Use UPX compression
upx --best --lzma target/release/fission

# 3. Build with size optimization
RUSTFLAGS="-C opt-level=z" cargo build --release

# 4. Remove unnecessary features
cargo build --release --no-default-features --features cli
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
          sudo apt install -y cmake zlib1g-dev
      
      - name: Build decompiler
        run: |
          cd ghidra_decompiler
          cmake -B build -DCMAKE_BUILD_TYPE=Release
          cmake --build build
      
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
# Test FFI integration
cargo run --bin ffi_test

# Or manually
./target/release/fission --cli test/struct_test.exe --info
```

### Check Dependencies

```bash
# Linux
ldd target/release/fission

# macOS
otool -L target/release/fission

# Windows (PowerShell)
dumpbin /dependents target\release\fission.exe
```

---

## Related Documentation

- [README.md](../README.md) - Project overview
- [PLUGIN_DEVELOPMENT.md](PLUGIN_DEVELOPMENT.md) - Plugin development
- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture

---

## Getting Help

- **GitHub Issues**: https://github.com/sjkim1127/Fission/issues
- **Discussions**: https://github.com/sjkim1127/Fission/discussions
- **Matrix Chat**: Coming soon

---

## Summary

**Minimum steps**:
1. Install Rust + CMake + C++ compiler + zlib
2. Build decompiler: `cmake -B build && cmake --build build`
3. Build Fission: `cargo build --release`
4. Run: `./target/release/fission`

**Most common issues**:
- ❌ CMake not found → Install CMake
- ❌ zlib not found → Install zlib-dev package
- ❌ libdecomp not found → Build decompiler first
- ❌ Linker errors → Set LD_LIBRARY_PATH

**Build time**: ~5-10 minutes (first build), ~30s (incremental)  
**Disk space**: ~2 GB (with debug info)
