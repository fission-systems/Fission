# Rust ↔ C++ Bridge: Duplications and Gaps

> ⚠️ **Status: Historical Reference**
> This document is an archived working note about duplication and incompleteness that once existed around the Rust ↔ C++ FFI boundary.
> For the current architecture and ownership model, prefer [`docs/architecture/ARCHITECTURE.md`](./architecture/ARCHITECTURE.md) and [`docs/ROADMAP.md`](./ROADMAP.md).

This document records:

- logic that was duplicated on both sides of the FFI boundary
- bindings that used to be incomplete on the Rust side
- places where Rust and C++ resource/config logic could drift apart

It is kept as a historical reference, not as current source of truth.

---

## 1. FFI Surface That Is Already Bound on the Rust Side

The following native FFI functions are implemented in `ghidra_decompiler` and are already exposed in `crates/fission-ffi/src/decomp.rs` through safe `DecompilerNative` methods:

| C++ FFI function | Purpose | Rust method |
|------------------|---------|-------------|
| `decomp_set_function_inline` | Mark a function as inline (`OptionInline`) | `set_function_inline(addr, enabled)` |
| `decomp_set_function_noreturn` | Mark a function as noreturn (`OptionNoReturn`) | `set_function_noreturn(addr, enabled)` |
| `decomp_set_function_extrapop` | Set per-function stack cleanup bytes (`OptionExtraPop`) | `set_function_extrapop(addr, extrapop)` |
| `decomp_set_default_prototype` | Set the default prototype model | `set_default_prototype(model_name)` |
| `decomp_set_protoeval_current` | Set the prototype-eval model for the current function | `set_protoeval_current(model_name)` |
| `decomp_set_protoeval_called` | Set the prototype-eval model for called functions | `set_protoeval_called(model_name)` |

Relevant locations:

- `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h`
- `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp`
- `crates/fission-ffi/src/decomp.rs`

---

## 2. Historically Duplicated Implementations

### 2.1 Path / Resource Configuration

- **C++**: `ghidra_decompiler/src/config/PathConfig.cc`, `include/fission/config/PathConfig.h`
  - FID search directories, MSVC/GCC/LIBC/CRYPTO/EL FID file lists, GDT prefixes, `find_fid_file`, `get_all_fid_paths`
- **Rust**: `crates/fission-core/src/core/path_config.rs`
  - same overall intent and similar filename/path logic

**Issue:** two sources of truth. Adding a new FID file or search path required keeping both implementations in sync.

### 2.2 PE Section Parsing

- **C++**: `ghidra_decompiler/src/decompiler/DecompilationPipeline.cc`
  - inline PE section parsing used to locate data sections for native-side scanning
- **Rust**: `crates/fission-loader/src/loader/pe/mod.rs`
  - full PE parsing that produces `SectionInfo`

**Issue:** the PE layout was parsed in two places with overlapping intent.

### 2.3 Function Identification

- **C++**
  - `FidDatabase`
  - `InternalMatcher`
- **Rust**
  - `fission-signatures::SignatureDatabase`

**Issue:** two separate library-function identification mechanisms could diverge in naming, coverage, or precedence.

---

## 3. Historical Rust-Side Gaps

### 3.1 Path Config FID Coverage

At one point:

- **C++** path config included MSVC, GCC, **LIBC**, **CRYPTO**, and **EL**
- **Rust** path config included only **MSVC** and **GCC**

**Effect:** Rust-driven flows that relied on `PATHS.get_all_fid_paths()` could see a narrower FID set than the C++ path.

---

## 4. Summary Table

| Category | Item | Rust | C++ | Recommended Direction |
|----------|------|------|-----|-----------------------|
| FFI | Per-function inline / noreturn / extrapop / prototype controls | ✅ | ✅ | Already bound in `decomp.rs` |
| Duplicated | Path / resource config | ✅ | ✅ | Unify or keep synchronized from one source |
| Duplicated | PE section parsing | ✅ | ✅ | Prefer one parser authority |
| Duplicated | Function identification | ✅ | ✅ | Keep role boundaries explicit |
| Former Rust gap | FID file list coverage | Partial | Broader | Expand only if Rust should match native behavior exactly |

---

## 5. References

- FFI declarations: `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h`
- FFI implementation: `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp`
- Rust FFI: `crates/fission-ffi/src/decomp.rs`
- C++ path config: `ghidra_decompiler/src/config/PathConfig.cc`
- Rust path config: `crates/fission-core/src/core/path_config.rs`
- Rust signatures: `crates/fission-signatures/src/database.rs`
- C++ FID: `ghidra_decompiler/src/analysis/FidDatabase.*`, `InternalMatcher.*`
