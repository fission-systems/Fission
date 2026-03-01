# Fission - Feature Documentation

> ℹ️ **Note:** 상위 수준 기능 개요와 최신 상태는 루트 `README.md`를 기준으로 합니다.  
> 이 문서는 세부 기능/서브시스템(분석 모듈, GUI 세부 기능 등)을 좀 더 자세히 설명하는 **보조 문서**입니다.

## 🎯 Overview

Fission is a next-generation decompiler and reverse engineering platform.

---

## 📦 Crate Structure

| Crate | Description | LOC |
|-------|-------------|-----|
| `fission-core` | Configuration, errors, utilities | ~1.5K |
| `fission-loader` | Binary parsing (PE/ELF/Mach-O) | ~6K |
| `fission-analysis` | CFG, optimization, post-processing | ~8K |
| `fission-ffi` | Ghidra native decompiler FFI | ~1K |
| `fission-pcode` | P-code IR and optimizer | ~5K |
| `fission-signatures` | Function signatures database | ~2K |
| `fission-disasm` | Fast x86/x64 disassembler (iced-x86) | ~200 |
| `fission-tauri` | Tauri 2.x + React 19 desktop GUI | ~8K |
| `fission-cli` | Command-line interface | ~3K |

---

## 🔬 Decompiler Features

### Binary Format Support

- ✅ PE (Windows EXE/DLL)
- ✅ ELF (Linux/Unix executables)
- ✅ Mach-O (macOS/iOS)

### Architecture Support

- ✅ x86-32/x86-64 (Sleigh spec)
- ✅ ARM64/AARCH64 (Sleigh spec + Apple Silicon variant)
- ✅ ARM64 Big Endian

### Language-Specific Analysis

| Language | Function Names | Type Recovery | Field Names |
|----------|---------------|---------------|-------------|
| Swift | ✅ Demangling | ✅ Metadata | ✅ __swift5_fieldmd |
| Objective-C | ✅ Method names | ✅ ivar parsing | ✅ ObjC2 runtime |
| Go | ✅ pclntab | ✅ .rodata types | ✅ Struct fields |
| C/C++ | ✅ DWARF symbols | ✅ Debug info | ✅ DWARF parsing |
| Rust | ✅ Demangling | ✅ VTable/Trait | ✅ Automatic |

### Post-Processing

- ✅ IAT symbol replacement
- ✅ Smart constant replacement (Windows API: VirtualAlloc, CreateFile, etc.)
- ✅ String inlining from .rdata
- ✅ GUID substitution
- ✅ Unicode string recovery
- ✅ SEH boilerplate cleanup
- ✅ C++ name demangling
- ✅ FID (Function ID) matching
- ✅ Structure offset annotation
- ✅ Control flow structurization (goto elimination)
- ✅ Compound operator conversion (i++ / +=)
- ✅ Condition simplification

### Type System

- ✅ GDT (Ghidra Data Type) loading (65K+ Windows functions)
- ✅ Custom struct registration via FFI
- ✅ Type propagation
- ✅ Parameter type hints

---

## 🖥️ GUI Features (Tauri + React)

### Main Panels

| Panel | Description | File |
|-------|-------------|------|
| Assembly View | Disassembly with syntax highlighting | `AssemblyView.tsx` |
| Decompile View | C-like decompiled code | `DecompileView.tsx` |
| Hex View | Raw hex editor | `HexView.tsx` |
| Listing View | Linear listing | `ListingView.tsx` |
| Functions List | Function browser with search | `FunctionsList.tsx` |
| Search Panel | Global search | `SearchPanel.tsx` |
| Settings | Application settings | `SettingsPanel.tsx` |
| Plugins | Plugin manager | `PluginsPanel.tsx` |

### Bottom Tabs

- XRefs (`XrefsPanel.tsx`)
- String XRefs (`StringXrefsPanel.tsx`)
- CFG (`CfgPanel.tsx`)
- Exports (`ExportsPanel.tsx`)
- Patches (`PatchesPanel.tsx`)
- Notes (`NotesPanel.tsx`)
- Debug (`DebugTab.tsx`)
- Timeline (`TimelinePanel.tsx`)

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `G` | Go to Address |
| `N` | Rename function/variable |
| `;` | Add comment |
| `F2` | Toggle bookmark |
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save project |
| `Ctrl+F` | Search |

### Theme

- Catppuccin Mocha dark theme
- Code syntax highlighting
- JetBrains Mono font support

---

## 🔧 CLI Features

```bash
# Basic decompilation
fission --cli <binary> <address>

# Verbose output
fission --cli <binary> <address> -v

# Show binary info
fission --cli <binary> info

# List functions
fission --cli <binary> functions
```

---

## 📁 Key Directories

```
/ghidra_decompiler/        # Native Ghidra C++ integration
  /src/
    /analysis/             # Type propagation, VTable, etc.
    /decompiler/           # Core decompilation pipeline
    /ffi/                  # FFI interface
    /processing/           # String scanner, constants
    /types/                # GDT parser, type resolver
  /languages/              # Sleigh specs (x86, ARM64)
  /decompile/              # Ghidra decompiler source

/crates/                   # Rust crates
  /fission-loader/         # Binary loading
    /src/loader/
      /macho/apple.rs      # Swift/ObjC analysis
      /golang.rs           # Go analysis
      /dwarf.rs            # DWARF debug info
  /fission-analysis/       # Analysis passes
  /fission-tauri/          # Tauri GUI (React frontend + Rust backend)
```

---

## 🔮 Known Limitations

1. **Swift Accessors**: VTable-based property access not fully resolved
2. **PDB**: No native PDB parsing (uses GDT instead)
3. **WASM**: Not supported yet

---

## 📊 Build & Test

```bash
# Build release
cargo build --release

# Run tests
cargo test

# Run GUI
./target/release/fission --gui

# Run CLI
./target/release/fission --cli <binary> <address>
```

---

*Last updated: 2026-01-20*
