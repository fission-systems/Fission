# Fission

> **"Split the Binary, Fuse the Power."**

A next-generation hybrid dynamic analysis platform that unifies the best features of x64dbg, Frida, Radare2, and Ghidra into a single high-performance Rust-powered binary.

![Fission Screenshot](docs/screenshot.png)

## Target Users

- Malware Analysts
- Vulnerability Researchers
- Reverse Engineers

## Features

### Static Analysis

- **Ghidra-Powered Decompiler** - High-performance C code decompilation via subprocess pool
- **iced-x86 Disassembler** - Pure Rust x86/x64 disassembly with syntax highlighting
- **.NET Binary Support** - CLR metadata parsing, IL disassembly, native stub analysis
- **Cross-Platform** - Windows (PE) and Linux (ELF) binary support

### Dynamic Analysis

- **Process Debugging** - Attach/detach, breakpoints, register/memory access
- **Time Travel Debugging** - Execution timeline with snapshot navigation
- **Live Memory Patching** - Modify running process memory

### Performance

- **Decompiler Pool** - Multi-process parallelization (auto-detects CPU cores)
- **Architecture Caching** - Reuses Ghidra objects across requests
- **LRU Result Cache** - Configurable cache with automatic eviction
- **Smart Load Balancing** - Idle worker prioritization
- **Background Prefetching** - Pre-decompiles adjacent functions

### Smart Decompilation

- **Context-Aware Constant Substitution** - Replaces magic numbers with symbolic names based on API parameter context
- **16 Enum Groups** - PAGE_PROTECT, MEM_ALLOC, GENERIC_ACCESS, HKEY_ROOT, AF_FAMILY, etc.
- **100+ API Mappings** - 9 DLLs supported (kernel32, user32, ntdll, advapi32, ws2_32, winhttp, wininet, shell32, bcrypt)
- **Dynamic Flag Resolution** - Automatically detects OR combinations (e.g., `0x3000` → `MEM_COMMIT | MEM_RESERVE`)
- **Dynamic Flag Resolution** - Automatically detects OR combinations (e.g., `0x3000` → `MEM_COMMIT | MEM_RESERVE`)
- **GDT Type Loading** - 5,700+ structures and 6,500+ typedefs from Ghidra data

### Advanced Decompilation Features

- **Auto-Inferred Structures** - Automatically detects structure layouts and generates C `typedef` definitions.
- **Reverse Type Propagation** - Propagates inferred types from callees back to callers for better variable typing.
- **Smart String Recovery** - Converts hex constants (`0x6d65...`) into readable string literals (`"TestItem"`).
- **VTable Analysis** - Recovers C++ virtual tables and resolves indirect calls (`call [rax+0x10]` → `Class::method`).
- **Precise Field Typing** - Distinguishes `float`/`double` fields via FPU instruction analysis.

### Extensibility

- **Plugin System** - Native Rust and Python plugin support
- **Event Bus** - Subscribe to binary load, decompile, debug events
- **Hook Priority** - Control plugin execution order
- **Python Scripting API** - Access binary info, functions, sections via PyO3

## GUI Layout

| Panel | Description |
|-------|-------------|
| **Explorer** | Function list (imports/exports) with virtual scrolling |
| **Search** | Full-text search across functions and strings |
| **Editor** | Tabbed interface for Assembly and Decompiled C code |
| **Console** | Colored log output with integrated CLI |
| **Debug** | Execution control, breakpoints, registers |
| **Hex View** | Binary hex dump with patching support |
| **Timeline** | Time travel debugging visualization |
| **Plugins** | Plugin management and status |

## Tech Stack

| Component | Technology |
|-----------|------------|
| Language | Rust 2021 Edition |
| GUI | egui 0.29 + eframe 0.29 |
| Theme | Catppuccin |
| Disassembler | iced-x86 1.21 |
| Decompiler | Ghidra C++ (subprocess) |
| Binary Parsing | goblin 0.8 + object 0.32 |
| .NET Parsing | Custom Rust |
| Debugging | Windows 0.54 / nix 0.28 (ptrace) |
| Scripting | PyO3 0.24 (optional) |
| Async | Tokio 1.36 |
| Caching | lru 0.12 |
| CLI | reedline 0.30 + clap 4.5 |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│                         Fission (Rust)                           │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐  │
│  │  GUI (egui)  │  │  CLI (repl)  │  │   Plugin Manager       │  │
│  └──────┬───────┘  └──────┬───────┘  └───────────┬────────────┘  │
│         │                 │                      │               │
│         └─────────────────┴──────────────────────┘               │
│                           │                                       │
│  ┌────────────────────────┴────────────────────────────────────┐ │
│  │                    Analysis Core                             │ │
│  │  ┌─────────┐  ┌──────────┐  ┌─────────┐  ┌───────────────┐  │ │
│  │  │ Loader  │  │ Disasm   │  │ Decomp  │  │ Debug Engine  │  │ │
│  │  │ PE/ELF  │  │ iced-x86 │  │ Pool    │  │ Win32/ptrace  │  │ │
│  │  └─────────┘  └──────────┘  └────┬────┘  └───────────────┘  │ │
│  └──────────────────────────────────┼──────────────────────────┘ │
└─────────────────────────────────────┼────────────────────────────┘
                                      │ stdin/stdout (JSON)
                    ┌─────────────────┼─────────────────┐
                    │                 │                 │
              ┌─────┴─────┐     ┌─────┴─────┐     ┌─────┴─────┐
              │ Worker 1  │     │ Worker 2  │     │ Worker N  │
              │ fission_  │     │ fission_  │     │ fission_  │
              │ decomp    │     │ decomp    │     │ decomp    │
              └───────────┘     └───────────┘     └───────────┘
                    │                 │                 │
              ┌─────┴─────────────────┴─────────────────┴─────┐
              │              Ghidra Engine (C++)              │
              │  SleighArch → Funcdata → PrintC → C Code     │
              └───────────────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- Rust 1.75+
- CMake 3.16+
- vcpkg (for ZLIB)
- Visual Studio 2022 (Windows) or GCC/Clang (Linux/macOS)

### Build

```bash
# 1. Build Ghidra Native CLI
cd ghidra_decompiler
cmake -B build -DCMAKE_TOOLCHAIN_FILE=[VCPKG_PATH]/scripts/buildsystems/vcpkg.cmake
cmake --build build --config Release

# 2. Build and Run Fission
cd ..
cargo run --release
```

### Usage

1. Launch: `cargo run --release`
2. **File → Open Binary** to load an executable
3. Use **Explorer** (left) to browse functions
4. Double-click a function for Assembly/Decompiled tabs
5. Use **Debug** tab (bottom) to attach and control execution

## CLI Commands

Run with `--headless` for CLI mode. Available commands:

| Command | Aliases | Syntax | Description |
|---------|---------|--------|-------------|
| `load` | `open`, `o` | `load <path>` | Load a binary file |
| `info` | `i` | `info` | Display binary information |
| `funcs` | `functions`, `f` | `funcs` | List discovered functions |
| `sections` | `sec` | `sections` | Show section table |
| `strings` | `str` | `strings` | Extract ASCII strings (min 4 bytes) |
| `analyze` | `anal`, `a` | `analyze` | Discover internal functions |
| `disasm` | `dis`, `d` | `disasm <addr> [count]` | Disassemble at address |
| `decompile` | `dec`, `decomp` | `decompile <addr>` | Decompile function at address |
| `clear` | `cls` | `clear` | Clear screen |
| `help` | `?`, `h` | `help` | Show help message |
| `quit` | `exit`, `q` | `quit` | Exit program |

**Address formats:** `0x1000`, `140001000`, `1000` (decimal)

## Configuration

All settings defined in `src/core/config.rs`:

```rust
// Decompiler settings
DecompilerConfig {
    mode: DecompilerMode::Single, // Single (memory efficient) or Pool (parallel)
    num_workers: 0,               // 0 = auto (CPU cores, max 8)
    max_workers: 8,               // Cap on auto-detected workers
    default_function_size: 4096,  // 4KB default function size
    max_function_size: 65536,     // 64KB max function size
    min_function_size: 16,        // 16 bytes minimum
    timeout_ms: 30000,            // 30 second timeout
    enable_prefetch: true,        // Pre-decompile adjacent functions
    prefetch_count: 3,            // Number of functions to prefetch
    requests_before_restart: 500, // Restart subprocess to reclaim memory
}

// Analysis settings
AnalysisConfig {
    max_string_search_size: 262144, // 256KB for string extraction
    min_string_length: 4,           // Minimum 4 bytes for strings
    auto_xref_analysis: true,       // Auto cross-reference on load
    decompile_cache_size: 100,      // LRU cache entries
    function_address_range: 4096,   // 4KB range for function matching
}

// Debug/TTD settings
DebugConfig {
    max_snapshots: 10000,   // Time travel debugging snapshots
    max_process_ids: 4096,  // Max processes to enumerate
}

// UI settings
UiConfig {
    show_performance: false,   // Performance metrics display
    auto_scroll_entry: true,   // Auto-scroll to entry point on load
    max_log_entries: 1000,     // Console history limit
    hex_rows_per_page: 64,     // Hex viewer pagination
}
```

## Project Structure

```
Fission/
├── Cargo.toml
├── build.rs                    # Native library linking
├── ghidra_decompiler/
│   ├── CMakeLists.txt
│   ├── fission_decomp.cpp      # Subprocess decompiler
│   └── languages/              # Sleigh (.sla) files
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── core/
│   │   ├── config.rs           # All configuration options
│   │   ├── context.rs          # FissionContext
│   │   ├── events.rs           # Event bus system
│   │   ├── errors.rs           # Unified error types
│   │   ├── logging.rs          # Log levels
│   │   ├── constants.rs        # Magic bytes
│   │   └── modules.rs          # Module lifecycle
│   ├── analysis/
│   │   ├── loader/             # PE/ELF/Mach-O parsing
│   │   ├── disasm/             # iced-x86 wrapper
│   │   ├── decomp/             # Decompiler pool
│   │   ├── dotnet/             # .NET/CLR analysis
│   │   ├── gdt_parser.rs       # GDT type extraction
│   │   ├── signatures/         # Windows API mappings (100+ APIs)
│   │   ├── detector/           # Binary signature detection
│   │   ├── xrefs/              # Cross-reference database
│   │   └── patch/              # Memory patching
│   ├── debug/
│   │   ├── windows/            # Win32 debugger
│   │   ├── linux.rs            # ptrace debugger
│   │   ├── ttd/                # Time travel debugging
│   │   ├── traits.rs           # Platform trait
│   │   └── memory.rs           # Cross-platform memory ops
│   ├── plugin/
│   │   ├── traits.rs           # FissionPlugin trait
│   │   ├── manager.rs          # Plugin registry
│   │   ├── python.rs           # PyO3 integration
│   │   ├── hooks.rs            # Event bus
│   │   └── api.rs              # Python API
│   ├── script/
│   │   ├── bridge.rs           # Python interop
│   │   └── types.rs            # Exported types
│   └── ui/
│       ├── gui/
│       │   ├── app/            # FissionApp orchestrator
│       │   ├── panels/         # UI components
│       │   ├── theme.rs        # Catppuccin styling
│       │   └── state.rs        # AppState management
│       └── cli/
│           ├── mod.rs          # REPL loop
│           └── commands.rs     # Command parsing
└── tests/
    ├── cli_tests.rs
    ├── decompiler_tests.rs
    └── loader_tests.rs
```

## Development Status

- [x] CLI Base - Binary loader, disassembler, REPL
- [x] Ghidra Integration - Native subprocess decompilation
- [x] VS Code Style GUI - Tabs, Activity Bar, themes
- [x] .NET Support - CLR detection, metadata, IL disassembly
- [x] Debugging - Attach, breakpoints, registers, memory
- [x] Plugin System - Native Rust and Python plugins
- [x] Plugin System - Native Rust and Python plugins
- [x] Performance Optimization - Pool, caching, prefetch
- [x] Advanced Type Analysis - Struct inference, VTable, Type Propagation
- [ ] Advanced TTD - Full time travel debugging
- [ ] Remote Debugging - Network-based debug sessions

## Testing

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test loader_tests
cargo test --test decompiler_tests
cargo test --test cli_tests

# Run with verbose output
cargo test -- --nocapture
```

## 🚀 Ultimate Roadmap: Project Restoration

**최종 목표:** 바이너리에서 원본 프로젝트 완전 복원

### Vision

> 변수명, 순서가 다르더라도 **기능적 + 외형적으로 동일**하면 동일한 프로그램.  
> 3개의 AI 에이전트가 협업하여 바이너리를 원본 프로젝트로 복원.

### 3 AI Agents

| Agent | 역할 | 기술 |
|-------|------|------|
| **🔍 Observer** | 정적 분석 | 디컴파일, 타입 추론, 데이터 흐름, 패턴 인식 |
| **▶️ Executor** | 동적 분석 | 런타임 추적, 메모리 스냅샷, I/O 모니터링, 커버리지 |
| **✏️ Author** | 코드 생성 | 추론-검증-수정 루프, 테스트 생성, 빌드 검증 |

### Workflow

```
┌────────────┐     ┌────────────┐     ┌────────────┐
│  Observer  │────▶│  Executor  │────▶│   Author   │
│ (Static)   │     │ (Dynamic)  │     │ (Generate) │
└─────┬──────┘     └─────┬──────┘     └─────┬──────┘
      │                  │                  │
      └──────────────────┴──────────────────┘
                         │
                    ┌────▼────┐
                    │ Original │
                    │ Source   │
                    │ Project  │
                    └──────────┘
```

### Phase 1: AI Integration

- [ ] LLM API 연동 (OpenAI, Claude, Local)
- [ ] Observer 에이전트 - 디컴파일 결과 분석
- [ ] 함수 목적 추론 및 이름 제안

### Phase 2: Dynamic Analysis AI

- [ ] Executor 에이전트 - 실행 추적 분석
- [ ] I/O 패턴 및 시스템 콜 분석
- [ ] 동적 데이터 흐름 매핑

### Phase 3: Code Generation

- [ ] Author 에이전트 - 코드 생성 및 검증
- [ ] 생성된 코드 빌드 및 테스트
- [ ] 원본과의 기능 동등성 검증

### Phase 4: Full Restoration

- [ ] 3 에이전트 협업 오케스트레이션
- [ ] 증분 복원 (함수 → 모듈 → 프로젝트)
- [ ] 빌드 시스템 및 의존성 복원

## License

MIT License - See [LICENSE](LICENSE) for details.

## Acknowledgments

- [Ghidra](https://ghidra-sre.org/) - NSA's software reverse engineering framework
- [iced-x86](https://github.com/icedland/iced) - High-performance x86/x64 disassembler
- [egui](https://github.com/emilk/egui) - Immediate mode GUI library
- [Catppuccin](https://github.com/catppuccin/catppuccin) - Soothing pastel theme
