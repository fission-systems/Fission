# Fission 🔬

> **"Split the Binary, Fuse the Power."**

**Fission** is a next-generation hybrid dynamic analysis platform that unifies the best features of x64dbg, Frida, Radare2, and Ghidra into a single high-performance Rust-powered binary.

![Fission Screenshot](docs/screenshot.png)

## 🎯 Target Users

- Malware Analysts
- Vulnerability Researchers  
- Reverse Engineers

## ✨ Core Features

- **VS Code Style GUI**: Modern multi-panel layout with Activity Bar, Side Bar, Tabbed Editor, and Panel Area.
- **Ghidra-Powered Decompiler**: High-performance C code decompilation via native FFI (Foreign Function Interface) ✅
- **iced-x86 Disassembler**: High-performance pure Rust x86/x64 disassembly with syntax highlighting
- **.NET Binary Support**: CLR metadata parsing, IL disassembly, and native stub analysis ✅
- **Decompile Caching**: Results are cached for instant re-access
- **Cross-Platform**: Windows (PE) and Linux (ELF) binary support
- **Debug Support**: Process attach/detach, breakpoints, and register/memory access via Windows debugging API ✅

## 🖥️ GUI Panels

| Panel | Description |
|-------|-------------|
| **[Explorer]** | Function list (imports/exports) with virtual scrolling |
| **[Editor]** | Tabbed interface for Assembly and Decompiled C code |
| **[Console]** | Colored log output with integrated CLI input |
| **[Debug]** | Execution control, event timeline, breakpoints, and registers |
| **[Hex View]** | High-performance binary hex dump viewer |

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust 2021 | Memory safety, native performance |
| GUI | egui + eframe | GPU-accelerated, immediate mode |
| Theme | Catppuccin | Modern, eye-friendly color palette |
| Disassembler | iced-x86 | Pure Rust x86/x64 instruction decoding |
| Decompiler | Ghidra C++ (FFI) | Direct high-performance C code generation |
| Binary Parsing | goblin + object | PE/ELF with fallback support |
| .NET Parsing | Custom Rust | CLR metadata & IL disassembly |
| Debugging | Windows API | Process attach, breakpoints, registers |

## 🔧 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Fission (Rust)                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   GUI       │  │   CLI       │  │   Native FFI        │  │
│  │  (egui)     │  │ (reedline)  │  │   (libloading)      │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│         └────────────────┴─────────────────────┘             │
│                          │ direct call                       │
└──────────────────────────┼───────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│               Ghidra Engine (C++)                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ SleighArch  │  │  Funcdata   │  │      PrintC         │  │
│  │ (Disasm)    │  │ (Analysis)  │  │   (C Code Gen)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+
- CMake 3.16+
- vcpkg (for ZLIB)
- Visual Studio 2022 (Windows)

### Build

```bash
# 1. Build Ghidra Native Library
cd ghidra_decompiler
cmake -B build -DCMAKE_TOOLCHAIN_FILE=[PATH_TO_VCPKG]/scripts/buildsystems/vcpkg.cmake
cmake --build build --config Release

# 2. Build and Run Fission
cd ..
cargo run --release
```

### Usage

1. Launch Fission: `cargo run`
2. **File → Open Binary** to load an executable
3. Use the **Explorer** (left side) to browse functions
4. Double-click a function to open **Assembly** and **Decompiled** tabs
5. Use the **Debug** tab (bottom) to attach to processes and control execution

## 📁 Project Structure

```
Fission/
├── Cargo.toml              # Rust dependencies
├── build.rs                # Build configuration
├── ghidra_decompiler/      # C++ Ghidra Core & FFI Wrapper
│   ├── CMakeLists.txt
│   ├── wrapper.cpp         # C ABI implementation
│   └── languages/          # Sleigh (.sla) files
├── src/
│   ├── main.rs             # Entry point
│   ├── analysis/           # Analysis modules
│   │   ├── loader/         # Binary parsing (PE/ELF)
│   │   ├── disasm/         # iced-x86 disassembler
│   │   ├── decomp/         # Native FFI interface
│   │   └── dotnet/         # .NET/CLR analysis
│   ├── debug/              # Debugging core (Win32/Linux)
│   └── ui/
│       └── gui/            # VS Code style GUI
│           ├── app/        # App logic modules
│           ├── theme.rs    # Catppuccin styling
│           └── panels/     # UI components
```

## 📅 Development Roadmap

- [x] **Phase 1**: CLI Base - Binary loader, disassembler, REPL
- [x] **Phase 2**: Ghidra Integration - Native FFI C decompilation ✅
- [x] **Phase 3**: VS Code Style GUI - Tabs, Activity Bar, Catppuccin theme ✅
- [x] **Phase 4**: .NET Support - CLR detection, metadata, IL disassembly ✅
- [x] **Phase 5**: Debugging - Attach, breakpoints, registers, memory ✅
- [ ] **Phase 6**: Python Scripting - Full Python API
- [ ] **Phase 7**: Advanced Features - Time travel debugging, plugins

## 📜 License

MIT License - See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- [Ghidra](https://ghidra-sre.org/) - NSA's software reverse engineering framework
- [iced-x86](https://github.com/icedland/iced) - High-performance x86/x64 disassembler
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
- [Catppuccin](https://github.com/catppuccin/catppuccin) - Soothing pastel theme
