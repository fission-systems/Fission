# Rust ↔ C++ Bridge: Duplications and Gaps

> ℹ️ **Status:** 이 문서는 Rust↔C++ FFI 경계에서 과거에 존재하던 **중복/미구현 항목의 작업 메모**입니다.  
> 현재 설계 관점에서의 전체 구조는 `docs/architecture/ARCHITECTURE.md`와 `docs/ROADMAP.md`를 우선 참고하고,  
> 여기 내용은 “어디에서 무엇이 중복되었는지”를 추적하는 **역사적/세부 참고 자료**로 사용하는 것을 권장합니다.

This document lists **duplicated logic** (same concept implemented in both Rust and C++) and **unimplemented or incomplete** parts on either side of the FFI boundary.

---

## 1. ~~Unimplemented~~ Implemented on Rust Side (C++ API ↔ Rust binding)

The following C FFI functions are implemented in `ghidra_decompiler` and are **now bound** in `crates/fission-ffi/src/decomp.rs` with safe `DecompilerNative` methods:

| C++ FFI function | Purpose | Rust method |
|------------------|---------|-------------|
| `decomp_set_function_inline` | Mark a function as inline (Ghidra OptionInline) | `set_function_inline(addr, enabled)` |
| `decomp_set_function_noreturn` | Mark a function as noreturn (Ghidra OptionNoReturn) | `set_function_noreturn(addr, enabled)` |
| `decomp_set_function_extrapop` | Set per-function stack cleanup bytes (OptionExtraPop) | `set_function_extrapop(addr, extrapop)` |
| `decomp_set_default_prototype` | Set default prototype model (OptionDefaultPrototype) | `set_default_prototype(model_name)` |
| `decomp_set_protoeval_current` | Set prototype eval model for current function | `set_protoeval_current(model_name)` |
| `decomp_set_protoeval_called` | Set prototype eval model for called functions | `set_protoeval_called(model_name)` |

**Location:** `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h`, `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp`, `crates/fission-ffi/src/decomp.rs`.

---

## 2. Duplicated Implementations

### 2.1 Path / resource configuration

- **C++:** `ghidra_decompiler/src/config/PathConfig.cc`, `include/fission/config/PathConfig.h`  
  - FID search dirs, MSVC/GCC/LIBC/CRYPTO/EL FID file lists, `find_fid_file`, `get_all_fid_paths`, GDT prefixes, etc.
- **Rust:** `crates/fission-core/src/core/path_config.rs`  
  - Same intent: “Mirrors C++ fission::config::PathConfig” (see module comment).  
  - Same search dirs and similar FID filename logic.

**Issue:** Two sources of truth. Adding a new FID file or search path requires updating both. C++ has more FID lists (LIBC, CRYPTO, EL); Rust only has MSVC + GCC (see §3).

---

### 2.2 PE section parsing

- **C++:** `ghidra_decompiler/src/decompiler/DecompilationPipeline.cc` (Phase 9.5)  
  - Inline “Simple PE section parsing”: reads DOS/PE header, section table, collects `.rdata`/`.data` for data-symbol scanning.
- **Rust:** `crates/fission-loader/src/loader/pe/mod.rs`  
  - Full PE parse (e.g. via `binrw`), produces `SectionInfo`; used when loading binary and when calling `decomp_add_memory_block` (FFI path).

**Issue:** PE layout is parsed in two places. Batch C++ pipeline does not use Rust loader; FFI path uses Rust loader and passes sections to C++. Unifying on one parser (e.g. Rust → export section list for C++ batch, or C++ only) would remove duplication.

---

### 2.3 Function identification (signatures / FID)

- **C++:**  
  - `FidDatabase` + hash-based matching (e.g. `FidHasher::calculate_full_hash`).  
  - `InternalMatcher` (prologue + string refs).  
  - Used inside decompiler (batch and FFI) for IAT/symbol names and FID names.
- **Rust:**  
  - `fission-signatures`: `SignatureDatabase` (byte-pattern + first-byte index), MSVC CRT patterns, `identify_functions_in_binary`.  
  - Used in UI/analysis (e.g. `fission-ui`, message handlers) to identify functions.

**Issue:** Two separate “identify library function” mechanisms. Rust does not feed `SignatureDatabase` results into C++ FID; symbols can be passed via `decomp_add_symbol` but the matching logic and DBs are independent. Risk of divergence (e.g. different names or coverage for the same function).

---

## 3. Incomplete on Rust Side (subset of C++)

### 3.1 Path config: FID file lists

- **C++** `PathConfig.cc` includes:
  - MSVC, GCC, **LIBC**, **CRYPTO**, **EL** (Enterprise Linux) FID file lists for x64/x86.
- **Rust** `path_config.rs`:
  - Only **MSVC** and **GCC** in `MSVC_FID_FILES_*` / `GCC_FID_FILES_*`; `get_all_fid_paths()` only uses these two.

**Effect:** Any Rust code that uses `PATHS.get_all_fid_paths()` (or equivalent) will see fewer FID databases than C++ `get_all_fid_paths()`, so Rust-driven flows (e.g. CLI loading FID paths from Rust) can miss LIBC/CRYPTO/EL DBs.

---

## 4. Summary Table

| Category | Item | Rust | C++ | Action |
|----------|------|------|-----|--------|
| **FFI** | Per-function inline, noreturn, extrapop, default/protoeval | ✅ | ✅ | Bound in `decomp.rs` |
| **중복** | Path / resource config | ✅ PathConfig | ✅ PathConfig | Unify or keep in sync (e.g. codegen from one source) |
| | PE section parsing | ✅ loader/pe | ✅ pipeline inline | Prefer single source (e.g. Rust loader for both paths) |
| | Function ID (signatures vs FID) | ✅ SignatureDatabase | ✅ FidDatabase + InternalMatcher | Document roles; optionally feed Rust IDs into C++ as symbols |
| **Rust 불완전** | FID file lists | MSVC + GCC only | + LIBC, CRYPTO, EL | Add LIBC/CRYPTO/EL lists to `path_config.rs` if Rust should match C++ |

---

## 5. References

- FFI declarations: `ghidra_decompiler/include/fission/ffi/libdecomp_ffi.h`
- FFI implementation: `ghidra_decompiler/src/ffi/libdecomp_ffi.cpp`
- Rust FFI: `crates/fission-ffi/src/decomp.rs`
- C++ path config: `ghidra_decompiler/src/config/PathConfig.cc`
- Rust path config: `crates/fission-core/src/core/path_config.rs`
- Rust signatures: `crates/fission-signatures/src/database.rs`
- C++ FID: `ghidra_decompiler/src/analysis/FidDatabase.*`, `InternalMatcher.*`
