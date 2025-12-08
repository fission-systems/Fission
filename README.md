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
- **Capstone Disassembler**: Fast x86/x64 disassembly with syntax highlighting
- **Decompile Caching**: Results are cached for instant re-access
- **Auto Server Recovery**: Automatic reconnection with binary reload on server crash
- **Cross-Platform**: Windows (PE) and Linux (ELF) binary support

## 🖥️ GUI Panels

| Panel | Description |
|-------|-------------|
| **[Functions]** | Clickable list of detected functions (imports/exports) |
| **[Assembly]** | x64dbg-style disassembly with address, bytes, mnemonic, operands |
| **[Decompiled Code]** | Ghidra-generated C code with syntax highlighting |
| **[Console]** | Colored log output with CLI input, Copy All / Clear buttons |

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust 2021 | Memory safety, C++ performance |
| GUI | egui + eframe | GPU-accelerated, immediate mode |
| Disassembler | Capstone | x86/x64 instruction decoding |
| Decompiler | Ghidra C++ (gRPC) | Full C code generation |
| Binary Parsing | goblin + object | PE/ELF with fallback support |
| Async | tokio + tonic | gRPC client communication |

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
│   │   ├── disasm/         # Capstone disassembler
│   │   └── decomp/         # Ghidra gRPC client
│   └── ui/
│       └── gui/            # Modular GUI
│           ├── app.rs      # Main orchestrator
│           ├── state.rs    # Shared AppState
│           ├── messages.rs # Async message types
│           ├── menu.rs     # Menu bar
│           ├── status_bar.rs
│           └── panels/     # UI panels
│               ├── functions.rs
│               ├── console.rs
│               ├── assembly.rs
│               └── decompile.rs
```

## 📅 Development Roadmap

- [x] **Phase 1**: CLI Base - Binary loader, disassembler, REPL
- [x] **Phase 2**: Ghidra Integration - gRPC-based C decompilation ✅
- [x] **Phase 3**: x64dbg-Style GUI - Multi-panel layout, caching, recovery ✅
- [ ] **Phase 4**: Debug Loop - Attach, detach, breakpoints
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

### Example Usage (Rust)

```rust
let mut client = GhidraClient::connect().await?;
client.load_binary(bytes, 0x1000, "x86:LE:64:default").await?;
let result = client.decompile_function(0x1000).await?;
println!("{}", result.c_code);
```

## 📜 License

MIT License - See [LICENSE](LICENSE) for details.

## 🙏 Acknowledgments

- [Ghidra](https://ghidra-sre.org/) - NSA's software reverse engineering framework
- [Capstone](https://www.capstone-engine.org/) - Multi-architecture disassembly framework
- [gRPC](https://grpc.io/) - High-performance RPC framework
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
