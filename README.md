# Fission 🔬

> **"Split the Binary, Fuse the Power."**

**Fission** is a next-generation hybrid dynamic analysis platform that unifies the best features of x64dbg, Frida, Radare2, and Ghidra into a single Rust-powered binary.

## 🎯 Target Users

- Malware Analysts
- Vulnerability Researchers  
- Reverse Engineers

## ✨ Core Features

- **Hybrid Interface**: GPU-accelerated GUI (egui) + Radare2-style CLI running in perfect sync
- **Ghidra-Powered Decompiler**: Full C code decompilation via gRPC server ✅
- **Python Scripting**: Inline hooking with full access to internal state via PyO3
- **Cross-Platform Debugging**: Windows (Debug API) and Linux (ptrace) support

## 🛠️ Tech Stack

| Component | Technology | Purpose |
|-----------|------------|---------|
| Language | Rust 2021 | Memory safety, C++ performance |
| GUI | egui + wgpu | GPU-accelerated, immediate mode |
| CLI | reedline | Syntax highlighting, autocomplete |
| Decompiler | Ghidra C++ (gRPC) | Full C code generation |
| Binary Parsing | goblin | PE/ELF/Mach-O support |
| Scripting | Python 3 (PyO3) | User-friendly automation |

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

# Run tests
cargo test --bin fission decomp::tests -- --nocapture
```

### Example Output

```
✅ Connected to Ghidra Server!
   Ping: true
✅ Load Binary success
[Server] Decompiling function at 0x1000
[Server] Decompilation complete

=== Generated C Code ===
int4 func_1000(int4 param_1,int4 param_2)
{
  return param_1 + param_2;
}
========================
```

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
│   ├── app.rs              # Application state
│   ├── core/               # Debugger backend
│   │   ├── debugger.rs     # OS Debug API wrapper
│   │   └── memory.rs       # Memory operations
│   ├── decomp/             # Decompiler integration
│   │   ├── client.rs       # gRPC client
│   │   ├── mod.rs
│   │   └── tests.rs        # Integration tests
│   ├── disasm/             # Disassembly layer
│   │   └── engine.rs       # Data structures
│   ├── script/             # Python integration
│   │   └── bridge.rs       # Rust <-> Python bridge
│   └── ui/                 # Interface layer
│       ├── cli.rs          # reedline REPL
│       └── gui.rs          # egui rendering
```

## 📅 Development Roadmap

- [x] **Phase 1**: CLI Base - Binary loader, disassembler, REPL
- [x] **Phase 2**: Ghidra Integration - gRPC-based C decompilation ✅
- [ ] **Phase 3**: GUI & Debug Loop - Attach, detach, breakpoints
- [ ] **Phase 4**: Python Scripting - Full Python API
- [ ] **Phase 5**: Advanced Features - Time travel debugging, plugins

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
- [gRPC](https://grpc.io/) - High-performance RPC framework
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
