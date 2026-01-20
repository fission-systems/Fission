# Fission - Feature Documentation

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
| `fission-ui` | egui-based GUI | ~15K |
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
| Rust | ✅ Demangling | ⚠️ Partial | ⚠️ VTable |

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

## 🖥️ GUI Features (egui)

### Main Panels

| Panel | Description | File |
|-------|-------------|------|
| Assembly View | Disassembly with syntax highlighting | `assembly.rs` |
| Decompile View | C-like decompiled code | `decompile.rs` |
| Functions List | Function browser with search | `functions.rs` |
| Side Bar | Project navigation | `side_bar.rs` |
| XRefs | Cross-reference analysis | `xrefs.rs` |
| String XRefs | String reference browser | `string_xrefs.rs` |
| Search | Global search | `search.rs` |
| Settings | Application settings | `settings.rs` |

### Bottom Tabs

- Console (log output)
- Strings
- Imports
- Exports
- Sections
- Bookmarks
- Patches
- Scripts
- Notes
- Symbols

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
  /fission-ui/             # GUI
```

---

## 🔮 Known Limitations

1. **Swift Accessors**: VTable-based property access not fully resolved
2. **Rust Traits**: dyn Trait vtable parsing incomplete
3. **PDB**: No native PDB parsing (uses GDT instead)
4. **WASM**: Not supported yet

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
