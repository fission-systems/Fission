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
| Language | Rust 2021 |
| GUI | egui + eframe |
| Theme | Catppuccin |
| Disassembler | iced-x86 |
| Decompiler | Ghidra C++ (subprocess) |
| Binary Parsing | goblin + object |
| .NET Parsing | Custom Rust |
| Debugging | Windows API / ptrace |
| Scripting | PyO3 |
| Caching | lru crate |

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

## Configuration

Key settings in `src/core/config.rs`:

```rust
DecompilerConfig {
    num_workers: 0,              // 0 = auto (CPU cores, max 8)
    timeout_ms: 30000,           // 30 second timeout
    enable_prefetch: true,       // Pre-decompile adjacent functions
    requests_before_restart: 500 // Restart subprocess to reclaim memory
}

AnalysisConfig {
    decompile_cache_size: 100    // LRU cache entries
}
```

## Project Structure

```
Fission/
├── Cargo.toml
├── ghidra_decompiler/
│   ├── CMakeLists.txt
│   ├── fission_decomp.cpp   # Subprocess decompiler
│   ├── wrapper.cpp          # FFI wrapper (legacy)
│   └── languages/           # Sleigh (.sla) files
├── src/
│   ├── main.rs
│   ├── core/
│   │   ├── config.rs        # Global configuration
│   │   ├── events.rs        # Event bus system
│   │   └── modules.rs       # Module lifecycle
│   ├── analysis/
│   │   ├── loader/          # PE/ELF parsing
│   │   ├── disasm/          # iced-x86 wrapper
│   │   ├── decomp/          # Decompiler pool
│   │   └── dotnet/          # .NET/CLR analysis
│   ├── debug/               # Debugger core
│   ├── plugin/              # Plugin system
│   │   ├── traits.rs        # FissionPlugin trait
│   │   ├── manager.rs       # Plugin registry
│   │   └── python.rs        # PyO3 integration
│   └── ui/gui/
│       ├── app/             # App logic
│       │   ├── decomp_worker.rs  # Worker threads
│       │   └── decompiler.rs     # Decompile API
│       ├── panels/          # UI components
│       └── theme.rs         # Catppuccin styling
```

## Development Status

- [x] CLI Base - Binary loader, disassembler, REPL
- [x] Ghidra Integration - Native subprocess decompilation
- [x] VS Code Style GUI - Tabs, Activity Bar, themes
- [x] .NET Support - CLR detection, metadata, IL disassembly
- [x] Debugging - Attach, breakpoints, registers, memory
- [x] Plugin System - Native Rust and Python plugins
- [x] Performance Optimization - Pool, caching, prefetch
- [ ] Advanced TTD - Full time travel debugging
- [ ] Remote Debugging - Network-based debug sessions

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
