//! `IMAGE_LOAD_CONFIG_DIRECTORY` scanner (x86 PE only): the SafeSEH handler
//! table and the Control Flow Guard check-function pointer.
//!
//! Same rationale as [`super::msvc_eh`]: both are function addresses
//! reachable *only* from PE metadata, never from any `call`/`jmp` in the
//! code, so every other scanner in this module structurally can't find
//! them. No external data file needed -- this is fixed-offset struct
//! parsing of Microsoft's own public `winnt.h`
//! `IMAGE_LOAD_CONFIG_DIRECTORY32` layout, verified field-offset-by-
//! field-offset against Ghidra's reference implementation
//! (`Ghidra/Features/Base/.../format/pe/LoadConfigDirectory.java`,
//! `LoadConfigDataDirectory.java`, `ControlFlowGuard.java`), not
//! reverse-engineered from scratch.
//!
//! # Locating the directory
//!
//! Unlike every other scanner here, this one starts by reading *raw file
//! bytes* rather than [`LoadedBinary::get_bytes`]/`view_bytes` (which
//! resolve a virtual address through the section table): the DOS/PE/COFF/
//! Optional headers needed to find the Data Directory array live *before*
//! any section's mapped range, so there's no section to resolve them
//! through. `LoadedBinary::view_bytes`'s own implementation confirms
//! `binary.data` is the raw file image (it computes `section.file_offset +
//! offset_in_section` and indexes into `binary.data.as_slice()` with
//! that), so indexing it directly at the well-known DOS/PE header offsets
//! below is exactly what a from-scratch PE header parser would do. Once
//! the directory's own virtual address is known, everything else goes
//! through the normal `get_bytes`/`view_bytes` API like any other scanner.
//!
//! # Layout (x86 -- see [`super::msvc_eh`] for why x64 isn't attempted:
//! same "different pointer width, different, unimplemented layout" reason)
//!
//! `IMAGE_LOAD_CONFIG_DIRECTORY32`'s fields are read sequentially by the
//! loader; only fields that fit within the struct's own self-declared
//! `Size` (first field) are valid to read -- older/minimal PEs have a
//! shorter struct that simply doesn't include the later fields. Offsets
//! below assume the two blocks this scanner needs are both present
//! (`Size > 60` covers the first, `Size > 72` the second):
//!
//! ```text
//! +0  Size                          u32
//!   ... (fields this scanner doesn't need) ...
//! +60 SecurityCookie                 VA   (not used)
//! +64 SEHandlerTable                 VA   -- sorted array of SEHandlerCount RVAs
//! +68 SEHandlerCount                u32
//!   ... (CodeIntegrity fields this scanner doesn't need) ...
//! +72 GuardCFCheckFunctionPointer     VA  -- address of the
//!                                           `__guard_check_icall_fptr`
//!                                           slot, not the check function
//!                                           itself -- see below
//! ```
//!
//! `SEHandlerTable` itself is an absolute VA (subject to base relocation
//! like any other pointer field in this struct), but -- per Ghidra's own
//! `LoadConfigDataDirectory.markupSeHandler`, which subtracts image base
//! before walking it with `IBO32DataType` entries -- each of its
//! `SEHandlerCount` entries is a 4-byte **image-base-relative** RVA, not
//! an absolute VA: every valid, `/SAFESEH`-registered exception handler in
//! the module. [candidate, once added to image base]
//!
//! `GuardCFCheckFunctionPointer` needs one more dereference than its name
//! suggests: the compiler emits `__guard_check_icall_fptr` as a *pointer
//! variable* the OS loader patches at load time (the actual check routine
//! lives in `ntdll.dll`), so the directory field is the address of that
//! variable, not the check function's own address. Confirmed against a
//! real binary (the field's raw value pointed at the very start of a
//! dedicated `.00cfg` section -- clearly a data slot, not code) and
//! against Ghidra's own `ControlFlowGuard.markupCfgFunction`, which does
//! exactly this second `mem.getInt(functionPointerAddr)` read before
//! labeling the result `_guard_check_icall`. [candidate, after dereferencing]

use fission_loader::loader::LoadedBinary;

/// Generous sanity bound on `SEHandlerCount` -- a real SafeSEH table lists
/// every handler in the module, so this only rejects a directory that
/// clearly isn't real load-config data.
const MAX_SAFESEH_ENTRIES: u32 = 100_000;

pub(crate) fn scan_load_config_candidates(binary: &LoadedBinary) -> Vec<u64> {
    if binary.is_64bit {
        return Vec::new();
    }

    let Some(lc_va) = locate_load_config_directory_va(binary) else {
        return Vec::new();
    };
    let Some(size_bytes) = binary.get_bytes(lc_va, 4) else {
        return Vec::new();
    };
    let size = u32::from_le_bytes(size_bytes.try_into().unwrap()) as usize;

    let mut candidates = Vec::new();

    if size > 60 {
        if let Some(block) = binary.get_bytes(lc_va + 60, 12) {
            let se_handler_table = u32::from_le_bytes(block[4..8].try_into().unwrap()) as u64;
            let se_handler_count = u32::from_le_bytes(block[8..12].try_into().unwrap());
            collect_safeseh_candidates(binary, se_handler_table, se_handler_count, &mut candidates);
        }
    }

    if size > 72 {
        if let Some(block) = binary.get_bytes(lc_va + 72, 4) {
            let guard_check_slot = u32::from_le_bytes(block.try_into().unwrap()) as u64;
            // `GuardCFCheckFunctionPointer` is the address of a *pointer
            // variable* (the compiler-emitted `__guard_check_icall_fptr`
            // slot the OS loader patches at load time), not the check
            // function's own address -- one more dereference gets there.
            // Confirmed against a real binary: Ghidra's own
            // `ControlFlowGuard.markupCfgFunction` does the exact same
            // `mem.getInt(functionPointerAddr)` before labeling the
            // result `_guard_check_icall`.
            if guard_check_slot != 0 {
                if let Some(target_bytes) = binary.get_bytes(guard_check_slot, 4) {
                    let target = u32::from_le_bytes(target_bytes.try_into().unwrap());
                    if target != 0 {
                        candidates.push(target as u64);
                    }
                }
            }
        }
    }

    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn collect_safeseh_candidates(
    binary: &LoadedBinary,
    table_va: u64,
    count: u32,
    out: &mut Vec<u64>,
) {
    if table_va == 0 || count == 0 || count > MAX_SAFESEH_ENTRIES {
        return;
    }
    let Some(bytes) = binary.get_bytes(table_va, count as usize * 4) else {
        return;
    };
    for entry in bytes.chunks_exact(4) {
        let rva = u32::from_le_bytes(entry.try_into().unwrap());
        if rva != 0 {
            out.push(binary.image_base + rva as u64);
        }
    }
}

fn locate_load_config_directory_va(binary: &LoadedBinary) -> Option<u64> {
    let file = binary.data.as_slice();

    // DOS header: e_lfanew (PE header's file offset) is a 4-byte LE value
    // at file offset 0x3C.
    let e_lfanew = u32::from_le_bytes(file.get(0x3C..0x40)?.try_into().ok()?) as usize;

    // "PE\0\0" signature, then a 20-byte COFF file header, then the
    // Optional Header.
    if file.get(e_lfanew..e_lfanew + 4)? != b"PE\0\0" {
        return None;
    }
    let oh = e_lfanew + 4 + 20;

    // PE32 Optional Header magic must be 0x10b -- this scanner is x86-only
    // (see `is_64bit` check above), so this is a sanity check, not a
    // format branch.
    let magic = u16::from_le_bytes(file.get(oh..oh + 2)?.try_into().ok()?);
    if magic != 0x10b {
        return None;
    }

    // NumberOfRvaAndSizes at Optional-Header-relative offset 92; the
    // DataDirectory[NumberOfRvaAndSizes] array starts right after it, at
    // offset 96, each entry 8 bytes (RVA u32 + Size u32). Entry 10
    // (0-indexed) is IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG.
    let number_of_rva_and_sizes =
        u32::from_le_bytes(file.get(oh + 92..oh + 96)?.try_into().ok()?);
    if number_of_rva_and_sizes <= 10 {
        return None;
    }

    let entry_offset = oh + 96 + 10 * 8;
    let rva = u32::from_le_bytes(file.get(entry_offset..entry_offset + 4)?.try_into().ok()?);
    if rva == 0 {
        return None;
    }
    Some(binary.image_base + rva as u64)
}
