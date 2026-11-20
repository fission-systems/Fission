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
//! (`Size >= 72` covers the SEH block `+60..+72`, `Size >= 76` covers GuardCF at `+72..+76`):
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

    if size >= 72
        && let Some(seh_va) = lc_va.checked_add(60)
        && let Some(block) = binary.get_bytes(seh_va, 12)
    {
        let se_handler_table = u32::from_le_bytes(block[4..8].try_into().unwrap()) as u64;
        let se_handler_count = u32::from_le_bytes(block[8..12].try_into().unwrap());
        collect_safeseh_candidates(binary, se_handler_table, se_handler_count, &mut candidates);
    }

    if size >= 76
        && let Some(guard_va) = lc_va.checked_add(72)
        && let Some(block) = binary.get_bytes(guard_va, 4)
    {
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
    let Some(byte_len) = usize::try_from(count).ok().and_then(|c| c.checked_mul(4)) else {
        return;
    };
    let Some(bytes) = binary.get_bytes(table_va, byte_len) else {
        return;
    };
    for entry in bytes.chunks_exact(4) {
        let rva = u32::from_le_bytes(entry.try_into().unwrap());
        if rva != 0
            && let Some(va) = binary.image_base.checked_add(u64::from(rva))
        {
            out.push(va);
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
    let sig_end = e_lfanew.checked_add(4)?;
    if file.get(e_lfanew..sig_end)? != b"PE\0\0" {
        return None;
    }
    let oh = sig_end.checked_add(20)?;

    // PE32 Optional Header magic must be 0x10b -- this scanner is x86-only
    // (see `is_64bit` check above), so this is a sanity check, not a
    // format branch.
    let magic_end = oh.checked_add(2)?;
    let magic = u16::from_le_bytes(file.get(oh..magic_end)?.try_into().ok()?);
    if magic != 0x10b {
        return None;
    }

    // NumberOfRvaAndSizes at Optional-Header-relative offset 92; the
    // DataDirectory[NumberOfRvaAndSizes] array starts right after it, at
    // offset 96, each entry 8 bytes (RVA u32 + Size u32). Entry 10
    // (0-indexed) is IMAGE_DIRECTORY_ENTRY_LOAD_CONFIG.
    let rva_count_start = oh.checked_add(92)?;
    let rva_count_end = oh.checked_add(96)?;
    let number_of_rva_and_sizes =
        u32::from_le_bytes(file.get(rva_count_start..rva_count_end)?.try_into().ok()?);
    if number_of_rva_and_sizes <= 10 {
        return None;
    }

    let entry_offset = oh.checked_add(96 + 10 * 8)?;
    let entry_end = entry_offset.checked_add(4)?;
    let rva = u32::from_le_bytes(file.get(entry_offset..entry_end)?.try_into().ok()?);
    if rva == 0 {
        return None;
    }
    binary.image_base.checked_add(u64::from(rva))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

    fn build_x86_pe_with_load_config(lc_size: u32, image_base: u64) -> LoadedBinary {
        let mut raw = vec![0u8; 0x1000];
        let e_lfanew = 0x80usize;
        raw[0x3C..0x40].copy_from_slice(&(e_lfanew as u32).to_le_bytes());
        raw[e_lfanew..e_lfanew + 4].copy_from_slice(b"PE\0\0");
        let oh = e_lfanew + 4 + 20;
        raw[oh..oh + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        raw[oh + 92..oh + 96].copy_from_slice(&16u32.to_le_bytes());
        // Entry 10: Load Config directory at RVA 0x400 (file offset 0x400)
        raw[oh + 96 + 10 * 8..oh + 96 + 10 * 8 + 4].copy_from_slice(&0x400u32.to_le_bytes());

        // Populate Load Config at 0x400:
        // +0: lc_size
        // +64: SEHandlerTable = image_base + 0x500
        // +68: SEHandlerCount = 1
        // +72: GuardCFCheckFunctionPointer = image_base + 0x520
        raw[0x400..0x404].copy_from_slice(&lc_size.to_le_bytes());
        let seh_table_va = (image_base.wrapping_add(0x500)) as u32;
        raw[0x400 + 64..0x400 + 68].copy_from_slice(&seh_table_va.to_le_bytes());
        raw[0x400 + 68..0x400 + 72].copy_from_slice(&1u32.to_le_bytes());
        let guard_slot_va = (image_base.wrapping_add(0x520)) as u32;
        raw[0x400 + 72..0x400 + 76].copy_from_slice(&guard_slot_va.to_le_bytes());

        // SafeSEH handler RVA 0x600 at file offset 0x500
        raw[0x500..0x504].copy_from_slice(&0x600u32.to_le_bytes());
        // GuardCF check target VA (image_base + 0x700) at file offset 0x520
        let guard_target = (image_base.wrapping_add(0x700)) as u32;
        raw[0x520..0x524].copy_from_slice(&guard_target.to_le_bytes());

        LoadedBinaryBuilder::new("lc_test.exe".to_string(), DataBuffer::Heap(raw))
            .format("PE")
            .is_64bit(false)
            .image_base(image_base)
            .add_section(SectionInfo {
                name: ".rdata".to_string(),
                virtual_address: image_base.wrapping_add(0x400),
                virtual_size: 0x400,
                file_offset: 0x400,
                file_size: 0x400,
                is_executable: false,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build test PE")
    }

    #[test]
    fn size_guards_require_full_seh_and_guardcf_fields() {
        let base = 0x40_0000u64;
        // Size = 68 stops inside SEH fields (+60..+72): must ignore both SEH and GuardCF.
        let lc68 = build_x86_pe_with_load_config(68, base);
        assert!(scan_load_config_candidates(&lc68).is_empty());

        // Size = 72 covers SEH (+60..+72), not GuardCF (+72..+76).
        let lc72 = build_x86_pe_with_load_config(72, base);
        assert_eq!(scan_load_config_candidates(&lc72), vec![base + 0x600]);

        // Size = 74 stops inside GuardCF (+72..+76): still must not read GuardCF.
        let lc74 = build_x86_pe_with_load_config(74, base);
        assert_eq!(scan_load_config_candidates(&lc74), vec![base + 0x600]);

        // Size = 76 covers both SEH and GuardCF.
        let lc76 = build_x86_pe_with_load_config(76, base);
        assert_eq!(
            scan_load_config_candidates(&lc76),
            vec![base + 0x600, base + 0x700]
        );
    }

    #[test]
    fn overflowing_image_base_does_not_panic() {
        let lc = build_x86_pe_with_load_config(76, u64::MAX - 0x10);
        assert!(scan_load_config_candidates(&lc).is_empty());
    }
}
