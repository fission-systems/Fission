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

# 3. Build decompiler
cd ghidra_decompiler
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build
cd ..

# 4. Build Fission
cargo build --release --bin fission_cli

# 5. Run
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
# CLI mode
.\target\release\fission_cli.exe test.exe

# GUI (Tauri): from repo root
cd crates\fission-tauri
npm install
npm run tauri dev
```

#### Windows-Specific Notes

**Dynamic Library Path**: The decompiler library (`libdecomp.dll`) must be in the same directory as `fission_cli.exe` or in PATH.

```powershell
# Copy DLL to output directory
copy ghidra_decompiler\build\Release\libdecomp.dll target\release\
# CLI binary name: fission_cli.exe
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
# CLI mode
./target/release/fission_cli test.exe

# GUI (Tauri): from repo root
cd crates/fission-tauri && npm install && npm run tauri dev

# Install CLI system-wide (optional)
sudo cp target/release/fission_cli /usr/local/bin/
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

# CLI with native decompiler
cargo build --release --bin fission_cli --features native_decomp

# GUI (Tauri): build from Tauri crate
cd crates/fission-tauri && npm run tauri build

# Without native decompiler
cargo build --release --bin fission_cli --no-default-features
```

### Available Features

| Feature | Description | Default |
|---------|-------------|---------|
| `native_decomp` | Built-in Ghidra decompiler (fission-cli, fission-ffi) | ✅ in fission-cli |
| `gui` | Tauri 2.x + React 19 desktop GUI — 빌드: `cd crates/fission-tauri && npm run tauri build` | 별도 앱 |
| `cli` | CLI 바이너리: `fission_cli` | `cargo build --bin fission_cli` |
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

## Rust 워크스페이스 / rust-analyzer

### `failed to read .../fission-tauri/src-tauri/Cargo.toml` (No such file or directory)

- **원인**: 루트 `Cargo.toml`의 워크스페이스 멤버 `crates/fission-tauri/src-tauri` 경로에 `Cargo.toml`이 없을 때 발생합니다.
- **조치**:
  1. **전체 클론/동기화 확인**: `git status`, `git pull` 후 `ls crates/fission-tauri/src-tauri/Cargo.toml`로 파일 존재 여부 확인.
  2. **GUI 없이 작업할 때**:  
     - 루트 `Cargo.toml`에서 `"crates/fission-tauri/src-tauri",` 한 줄을 **주석 처리**하거나,  
     - `cp Cargo.toml.workspace-cli-only Cargo.toml` 로 CLI 전용 워크스페이스로 교체할 수 있습니다.  
     GUI 빌드가 필요해지면 `git checkout Cargo.toml` 로 원복하고 해당 경로가 있는지 확인하세요.
  3. **rust-analyzer 재시작**: 수정 후 `Ctrl+Shift+P` → "Rust-analyzer: Restart server".

### `file not found: .../fission-cli/src/bin/ffi_test.rs`

- **원인**: 예전에 있던 `ffi_test` 바이너리가 제거된 뒤에도 IDE/rust-analyzer가 해당 파일을 열거나 인덱스에 남아 있을 때 발생할 수 있습니다.
- **조치**:
  1. `ffi_test.rs` 탭이 열려 있으면 **닫기**.
  2. **Rust-analyzer 재시작**: `Ctrl+Shift+P` → "Rust-analyzer: Restart server".
  3. 현재 `fission-cli`에는 `fission_cli` 바이너리만 정의되어 있으므로, `ffi_test` 참조는 제거된 상태가 정상입니다.

---

## Decompiler logging

디컴파일러 준비(바이너리 로드·섹션·심볼·FID 등)는 fission-analysis의 `prepare_native_decompiler_for_binary` 한 경로만 사용하며, CLI와 GUI가 동일한 진입점을 호출한다. 구조는 [ARCHITECTURE.md](../architecture/ARCHITECTURE.md)의 "Per-binary decompiler preparation" 참고. 초기화 비용은 `--benchmark` 시 JSON `_meta.prepare_timings`로 단계별 확인 가능하다. 성능 최적화 우선순위는 [ARCHITECTURE.md](../architecture/ARCHITECTURE.md)의 "Decompiler performance optimization priorities" 참고.

디컴파일러(C++) 진단 로그는 다음으로만 제어합니다.

- **설정**: `fission.toml`의 `[decompiler]`에서 `log_verbose`(기본 `false`), `log_file`(기본 `""`). 비어 있지 않으면 해당 경로에 append.
- **CLI**: `--verbose` 플래그로 오버라이드. 실제 적용값은 `config.decompiler.log_verbose || cli.verbose`.
- **에러**: 실패 시 항상 `last_error` → Rust `Result`로 전달되며, 로그 스트림과 별개입니다.
- **OutputSilencer**: CLI에서 verbose가 아닐 때 stderr를 `/dev/null`로 리다이렉트해, C++에서 로그를 꺼도 서드파티가 쓴 stderr를 막습니다.

자세한 계약은 [ARCHITECTURE.md](../architecture/ARCHITECTURE.md)의 "Decompiler Logging and Errors"를 참고하세요.

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
1. Install Rust + CMake + C++ compiler + zlib
2. Build decompiler: `cd ghidra_decompiler && cmake -B build && cmake --build build && cd ..`
3. Build Fission CLI: `cargo build --release --bin fission_cli`
4. Run: `./target/release/fission_cli` (GUI는 `crates/fission-tauri`에서 `npm run tauri dev`)

**Most common issues**:
- ❌ CMake not found → Install CMake
- ❌ zlib not found → Install zlib-dev package
- ❌ libdecomp not found → Build decompiler first
- ❌ Linker errors → Set LD_LIBRARY_PATH

**Build time**: ~5-10 minutes (first build), ~30s (incremental)  
**Disk space**: ~2 GB (with debug info)
