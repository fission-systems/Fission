# Fission 🔬

> **"Split the Binary, Fuse the Power."**

**Fission** is a next-generation hybrid dynamic analysis platform that unifies the best features of x64dbg, Frida, Radare2, and Ghidra into a single Rust-powered binary.

![Fission Screenshot](docs/screenshot.png)

## 🎯 Target Users

- Malware Analysts
- Vulnerability Researchers  
- Reverse Engineers

## ✨ Core Features

- **x64dbg-Style GUI**: Multi-panel layout with Assembly, Decompiled Code, Functions, and Console views
- **Ghidra-Powered Decompiler**: Full C code decompilation via gRPC server ✅
- **iced-x86 Disassembler**: High-performance pure Rust x86/x64 disassembly with syntax highlighting
- **.NET Binary Support**: CLR metadata parsing, IL disassembly, and native stub analysis ✅
- **Decompile Caching**: Results are cached for instant re-access
- **Auto Server Recovery**: Automatic reconnection with binary reload on server crash
- **Cross-Platform**: Windows (PE) and Linux (ELF) binary support
- **Debug Support**: Process attach/detach with Windows debugging API ✅

## 🖥️ GUI Panels

| Panel | Description |
|-------|-------------|
| **[Functions]** | Clickable list of detected functions (imports/exports) |
| **[Assembly]** | x64dbg-style disassembly with address, bytes, mnemonic, operands |
| **[Decompiled Code]** | Ghidra-generated C code with syntax highlighting |
| **[Console]** | Colored log output with CLI input, Copy All / Clear buttons |
| **[Registers]** | CPU register state during debugging |
| **[Memory]** | Memory view and hex dump |

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust 2021 | Memory safety, C++ performance |
| GUI | egui + eframe | GPU-accelerated, immediate mode |
| Disassembler | iced-x86 | Pure Rust x86/x64 instruction decoding |
| Decompiler | Ghidra C++ (gRPC) | Full C code generation |
| Binary Parsing | goblin + object | PE/ELF with fallback support |
| .NET Parsing | Custom Rust | CLR metadata & IL disassembly |
| Async | tokio + tonic | gRPC client communication |
| Debugging | Windows API | Process attach, breakpoints |

## 🔧 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Fission (Rust)                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   GUI       │  │   CLI       │  │   Client (tonic)    │  │
│  │  (egui)     │  │ (reedline)  │  │                     │  │
│  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
│         │                │                     │             │
│         └────────────────┴─────────────────────┘             │
│                          │ gRPC                              │
│  ┌─────────────────────────────────────────────────────────┐ │
│  │                 .NET Analysis Module                     │ │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │ │
│  │  │ CLR Detect  │  │  Metadata   │  │  IL Disasm      │  │ │
│  │  │             │  │  Parser     │  │                 │  │ │
│  │  └─────────────┘  └─────────────┘  └─────────────────┘  │ │
│  └─────────────────────────────────────────────────────────┘ │
└──────────────────────────┼───────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│               Ghidra Server (C++)                            │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ SleighArch  │  │  Funcdata   │  │      PrintC         │  │
│  │ (Disasm)    │  │ (Analysis)  │  │   (C Code Gen)      │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.70+
- CMake 3.16+
- vcpkg with gRPC and protobuf installed
- Visual Studio 2022 (Windows)

### Build

```bash
# Build Ghidra gRPC Server
cmake -S ghidra_decompiler -B build -DCMAKE_TOOLCHAIN_FILE=C:/vcpkg/scripts/buildsystems/vcpkg.cmake
cmake --build build --config Release

# Build Rust client
cargo build --release

# Run GUI
cargo run

# Run tests
cargo test --bin fission decomp::tests -- --nocapture
```

### Usage

1. Launch Fission: `cargo run` or `fission.exe`
2. **File → Open Binary** to load an executable
3. Click a function in the left panel to decompile
4. View assembly in center, decompiled C code on the right
5. Use console commands: `help`, `funcs`, `clear`, `exit`

### .NET Binary Support

Fission automatically detects .NET binaries and provides:
- CLR runtime version detection
- Native entry point stub disassembly (x64dbg-style)
- IL metadata parsing (TypeDef, MethodDef, Field tables)

## 📁 Project Structure

```
Fission/
├── Cargo.toml              # Rust dependencies
├── build.rs                # Proto generation
├── protos/
│   └── ghidra_service.proto  # gRPC service definition
├── ghidra_decompiler/      # C++ Ghidra server
│   ├── CMakeLists.txt
│   ├── server_main.cc      # gRPC service implementation
│   └── languages/          # .sla, .ldefs, .pspec, .cspec files
├── src/
│   ├── main.rs             # Entry point
│   ├── analysis/           # Analysis modules
│   │   ├── loader/         # Binary parsing (PE/ELF)
│   │   ├── disasm/         # iced-x86 disassembler
│   │   ├── decomp/         # Ghidra gRPC client
│   │   └── dotnet/         # .NET/CLR analysis ✅
│   │       ├── mod.rs      # Entry point, RVA→offset
│   │       ├── metadata.rs # CLR metadata parser
│   │       └── il_disasm.rs # IL instruction decoder
│   ├── debug/              # Debugging support ✅
│   │   ├── mod.rs          # Debugger trait
│   │   ├── types.rs        # DebugEvent, DebugState
│   │   └── windows/        # Windows-specific debugger
│   └── ui/
│       └── gui/            # Modular GUI
│           ├── app/        # App modules
│           │   ├── mod.rs  # Main orchestrator
│           │   ├── decompiler.rs
│           │   └── debug_ops.rs
│           ├── state.rs    # Shared AppState
│           ├── messages.rs # Async message types
│           ├── menu.rs     # Menu bar
│           ├── status_bar.rs
│           └── panels/     # UI panels
│               ├── functions.rs
│               ├── console.rs
│               ├── assembly.rs
│               ├── decompile.rs
│               └── bottom_tabs/
```

## 📅 Development Roadmap

- [x] **Phase 1**: CLI Base - Binary loader, disassembler, REPL
- [x] **Phase 2**: Ghidra Integration - gRPC-based C decompilation ✅
- [x] **Phase 3**: x64dbg-Style GUI - Multi-panel layout, caching, recovery ✅
- [x] **Phase 3.5**: .NET Support - CLR detection, metadata parsing, IL disassembly ✅
- [x] **Phase 3.6**: Disassembler Migration - Capstone → iced-x86 ✅
- [ ] **Phase 4**: Debug Loop - Attach ✅, detach ✅, breakpoints (WIP)
- [ ] **Phase 5**: Python Scripting - Full Python API
- [ ] **Phase 6**: Advanced Features - Time travel debugging, plugins

## 🔗 gRPC API

### Services

| RPC | Description |
|-----|-------------|
| `Ping` | Health check |
| `LoadBinary` | Load binary data with architecture spec |
| `DecompileFunction` | Decompile function at address, returns C code |
| `DisassembleRange` | Disassemble address range |

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `max_instructions` | 200,000 | Maximum instructions per function |
| `max_message_size` | 50 MB | gRPC message size limit |

### Example Usage (Rust)

```rust
let mut client = GhidraClient::connect().await?;
client.load_binary(bytes, 0x1000, "x86:LE:64:default").await?;
let result = client.decompile_function(0x1000).await?;
println!("{}", result.c_code);
```

## 🆕 Recent Changes (v0.1.0)

- **iced-x86**: Migrated from Capstone to iced-x86 for faster, pure-Rust disassembly
- **.NET Support**: Added CLR binary detection and IL disassembly
- **gRPC Limits**: Increased message size to 50MB for large binaries
- **Flow Limits**: Increased max instructions to 200K, truncation instead of error
- **VA→FileOffset**: Fixed address translation for correct disassembly display
- **Debug Infrastructure**: Added process attach/detach dialogs

## 📜 License

MIT License - See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- [Ghidra](https://ghidra-sre.org/) - NSA's software reverse engineering framework
- [iced-x86](https://github.com/icedland/iced) - High-performance x86/x64 disassembler
- [gRPC](https://grpc.io/) - High-performance RPC framework
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
