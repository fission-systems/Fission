//! GCC/Itanium C++ LSDA (`.gcc_except_table`) discovery for ELF.
//!
//! `gimli` parses `.eh_frame`'s CIE/FDE structure (and already resolves the
//! LSDA *pointer* via `Fde::lsda()`) but has no knowledge of
//! `.gcc_except_table`'s own byte format -- that's
//! [`crate::loader::gcc_lsda::parse_lsda`], shared with the PE/SEH path
//! (see that module's doc comment). This module is just the ELF-specific
//! half: walking `.eh_frame`'s FDEs to find each function's LSDA pointer,
//! then handing the bytes it points to off to the shared parser.

use crate::loader::gcc_lsda::{LsdaInfo, parse_lsda};
use crate::loader::types::LoadedBinary;
use gimli::{BaseAddresses, EhFrame, RunTimeEndian, UnwindSection};
use std::collections::HashMap;

fn read_native(bytes: &[u8], is_64bit: bool) -> u64 {
    if is_64bit {
        let mut buf = [0u8; 8];
        let n = bytes.len().min(8);
        buf[..n].copy_from_slice(&bytes[..n]);
        u64::from_le_bytes(buf)
    } else {
        let mut buf = [0u8; 4];
        let n = bytes.len().min(4);
        buf[..n].copy_from_slice(&bytes[..n]);
        u32::from_le_bytes(buf) as u64
    }
}

/// Walks `.eh_frame`'s FDEs to find each function's LSDA pointer (via
/// `gimli::Fde::lsda()`, resolving one more indirection if the pointer
/// encoding is `DW_EH_PE_indirect`), then parses the LSDA it points to.
/// Runs post-load -- unlike `eh_frame::parse_eh_frame`'s early-pipeline
/// function-boundary extraction, this needs `LoadedBinary::get_bytes` (for
/// indirect type-table pointers that can live anywhere in the binary) and
/// `LoadedBinary::relocation_symbols` (to identify a dynamically-linked
/// caught type when its GOT slot isn't statically resolvable), both of
/// which require the full section/relocation tables to already exist.
pub fn analyze_eh_lsda(binary: &LoadedBinary) -> HashMap<u64, LsdaInfo> {
    let mut out = HashMap::new();

    let Some(eh_frame_sec) = binary.sections.iter().find(|s| s.name == ".eh_frame") else {
        return out;
    };
    let Some(except_table_sec) = binary
        .sections
        .iter()
        .find(|s| s.name == ".gcc_except_table")
    else {
        return out;
    };

    let start = eh_frame_sec.file_offset as usize;
    let end = start.saturating_add(eh_frame_sec.file_size as usize);
    let Some(eh_frame_data) = binary.data.as_slice().get(start..end) else {
        return out;
    };

    let endian = if binary.arch_spec.contains("BE") {
        RunTimeEndian::Big
    } else {
        RunTimeEndian::Little
    };
    let eh_frame = EhFrame::new(eh_frame_data, endian);
    let mut bases = BaseAddresses::default().set_eh_frame(eh_frame_sec.virtual_address);
    if let Some(text_sec) = binary.sections.iter().find(|s| s.name == ".text") {
        bases = bases.set_text(text_sec.virtual_address);
    }

    let read_at = |addr: u64, len: usize| binary.get_bytes(addr, len);
    let symbol_at = |addr: u64| binary.relocation_symbols.get(&addr).cloned();
    let table_start_vma = except_table_sec.virtual_address;
    let table_end_vma = table_start_vma + except_table_sec.virtual_size;

    let mut entries = eh_frame.entries(&bases);
    loop {
        let entry = match entries.next() {
            Ok(Some(entry)) => entry,
            Ok(None) => break,
            Err(_) => break,
        };
        let gimli::CieOrFde::Fde(partial_fde) = entry else {
            continue;
        };
        let Ok(fde) = partial_fde.parse(|_, bases, offset| eh_frame.cie_from_offset(bases, offset))
        else {
            continue;
        };
        let Some(lsda_ptr) = fde.lsda() else {
            continue;
        };

        let mut lsda_vma = lsda_ptr.pointer();
        if matches!(lsda_ptr, gimli::Pointer::Indirect(_)) {
            let ptr_size = if binary.is_64bit { 8 } else { 4 };
            let Some(bytes) = read_at(lsda_vma, ptr_size) else {
                continue;
            };
            lsda_vma = read_native(&bytes, binary.is_64bit);
        }

        if lsda_vma < table_start_vma || lsda_vma >= table_end_vma {
            continue;
        }
        let max_len = usize::try_from((table_end_vma - lsda_vma).min(4096)).unwrap_or(0);
        let Some(lsda_bytes) = read_at(lsda_vma, max_len) else {
            continue;
        };

        let region_start = fde.initial_address();
        if let Some(info) = parse_lsda(
            &lsda_bytes,
            lsda_vma,
            region_start,
            binary.is_64bit,
            &read_at,
            &symbol_at,
        ) {
            out.insert(region_start, info);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::LoadedBinary;

    /// End-to-end (real file, real section table, real relocation table)
    /// against the non-PIE fixture: `analyze_eh_lsda` must find both
    /// functions' LSDAs and resolve the caught type's *name* via
    /// `LoadedBinary::relocation_symbols`, matching the demangled name
    /// Fission's own ELF symbol handling already produces.
    #[test]
    fn analyze_eh_lsda_end_to_end_non_pie() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_lsda_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load lsda test ELF");

        // `LoadedBinary::from_file` must already have run the analyzer
        // (loader/mod.rs's `auto_detect_and_parse` wiring), not just
        // `analyze_eh_lsda` called standalone here.
        assert_eq!(binary.eh_lsda.len(), 2, "{:#x?}", binary.eh_lsda);

        let table = analyze_eh_lsda(&binary);
        assert_eq!(table, binary.eh_lsda);
        assert_eq!(table.len(), 2, "{table:#x?}");
        let risky = &table[&0x4011a6];
        assert!(risky.type_table.is_empty());
        assert_eq!(risky.call_sites.len(), 2);

        let guarded = &table[&0x40120c];
        assert_eq!(guarded.type_table.len(), 1);
        assert_eq!(guarded.type_table[0].address, 0x403dc0);
        assert_eq!(
            guarded.type_table[0].symbol.as_deref(),
            Some("typeinfo for std::runtime_error")
        );
        assert_eq!(guarded.call_sites[0].action_chain, vec![1]);
    }

    /// The same source, compiled `-fPIE`, to exercise the pcrel+indirect
    /// pointer-encoding path a static/non-PIE build never touches. The raw
    /// `address` genuinely can't be resolved statically here (the GOT slot
    /// is populated by the dynamic linker at process start, not present in
    /// the file) -- confirmed `0` -- but `symbol` still identifies the
    /// caught type correctly via the relocation table, which is the whole
    /// reason `LsdaTypeEntry` carries both fields instead of just an
    /// address.
    #[test]
    fn analyze_eh_lsda_end_to_end_pie_resolves_via_relocation_symbol() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_dyn_lsda_pie_test.elf");
        let binary = LoadedBinary::from_file(&path).expect("load lsda pie test ELF");
        let table = analyze_eh_lsda(&binary);

        assert_eq!(table.len(), 2, "{table:#x?}");
        let guarded = &table[&0x120e];
        assert_eq!(guarded.type_table.len(), 1);
        assert_eq!(
            guarded.type_table[0].symbol.as_deref(),
            Some("typeinfo for std::runtime_error")
        );
        assert_eq!(guarded.call_sites[0].action_chain, vec![1]);
    }
}
