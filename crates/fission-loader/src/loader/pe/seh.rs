//! `.pdata`/`.xdata` (x64 SEH exception directory) discovery for PE, for
//! the mingw-w64 `g++` case where the same GCC/Itanium LSDA byte format
//! used on ELF (see [`crate::loader::gcc_lsda`]) is embedded directly in
//! `.xdata` as each function's `UNWIND_INFO` "language-specific handler
//! data", instead of being referenced from `.eh_frame`.
//!
//! Verified byte-for-byte against a real `x86_64-w64-mingw32-g++` build
//! (`guarded()`/`x64_seh_guarded_test.exe`): `UNWIND_INFO`'s
//! `ExceptionHandler` RVA resolves to `__gxx_personality_seh0`, and the
//! bytes immediately after it decode with the exact same `LPStart`/
//! `TType`/call-site-table header [`crate::loader::gcc_lsda::parse_lsda`]
//! already implements for ELF, landing on the same `catch` block the
//! disassembly shows -- confirmed with `region_start` = the function's own
//! begin address (from `.pdata`'s `RUNTIME_FUNCTION` entry), since this
//! fixture's `LPStart` encoding is `DW_EH_PE_omit` (`parse_lsda` then
//! defaults `lp_start` to `region_start`), the overwhelmingly common case.
//!
//! MSVC-compiled PE C++ EH (`__CxxFrameHandler3`/`4`, its own `FuncInfo`/
//! `UnwindMapEntry`/`TryBlockMapEntry` tables) and raw MSVC `__try`/
//! `__except` (`_C_specific_handler`'s own scope-table format) are
//! genuinely different, unrelated encodings this module doesn't attempt --
//! narrower scope than "any PE personality routine", matching only the one
//! this session has a real fixture for.

use crate::loader::gcc_lsda::{parse_lsda, LsdaInfo};
use crate::loader::types::LoadedBinary;
use std::collections::HashMap;

const UNW_FLAG_EHANDLER: u8 = 0x1;
const UNW_FLAG_UHANDLER: u8 = 0x2;
const UNW_FLAG_CHAININFO: u8 = 0x4;

/// Walks `.pdata`'s `RUNTIME_FUNCTION` table (12-byte `BeginAddress`/
/// `EndAddress`/`UnwindInfoAddress` RVA triples -- the same table
/// [`super::pdata::parse_pdata`] reads for function-boundary discovery,
/// re-read here independently since that early-pipeline pass runs before
/// `LoadedBinary` exists and only keeps the begin/end pair, not
/// `UnwindInfoAddress`; same "two independent traversals of the same
/// directory, different pipeline stage" split as `elf::eh_frame`'s
/// early-pipeline FDE walk vs. `elf::lsda::analyze_eh_lsda`'s post-load
/// one). For each entry whose `.xdata` `UNWIND_INFO` carries an exception
/// handler and isn't chained (see below), parses the trailing
/// language-specific data with the shared `gcc_lsda::parse_lsda`.
///
/// Skips entries with `UNW_FLAG_CHAININFO` set (this unwind info is shared
/// with an earlier `RUNTIME_FUNCTION`, e.g. cold/split function fragments):
/// following the chain to find the real handler is unimplemented scope, not
/// a correctness bug -- those fragments just contribute no landing pads
/// today, same as a function with no `.pdata` entry at all.
pub fn analyze_seh_lsda(binary: &LoadedBinary) -> HashMap<u64, LsdaInfo> {
    let mut out = HashMap::new();

    let Some(pdata_sec) = binary.sections.iter().find(|s| s.name == ".pdata") else {
        return out;
    };
    let Some(xdata_sec) = binary.sections.iter().find(|s| s.name == ".xdata") else {
        return out;
    };

    let start = pdata_sec.file_offset as usize;
    let end = start.saturating_add(pdata_sec.file_size as usize);
    let Some(pdata_bytes) = binary.data.as_slice().get(start..end) else {
        return out;
    };

    let read_at = |addr: u64, len: usize| binary.get_bytes(addr, len);
    let symbol_at = |addr: u64| binary.relocation_symbols.get(&addr).cloned();
    let xdata_end_vma = xdata_sec.virtual_address + xdata_sec.virtual_size;

    for entry in pdata_bytes.chunks_exact(12) {
        let begin_rva = u32::from_le_bytes(entry[0..4].try_into().unwrap());
        let end_rva = u32::from_le_bytes(entry[4..8].try_into().unwrap());
        let unwind_info_rva = u32::from_le_bytes(entry[8..12].try_into().unwrap());
        if begin_rva == 0 || begin_rva >= end_rva || unwind_info_rva == 0 {
            continue;
        }

        let region_start = binary.image_base + begin_rva as u64;
        let unwind_info_vma = binary.image_base + unwind_info_rva as u64;

        // UNWIND_INFO header: VersionAndFlags, SizeOfProlog, CountOfCodes,
        // FrameRegisterAndOffset (4 bytes) -- only Flags (top 5 bits of
        // byte 0) and CountOfCodes (byte 2) matter here, the latter just to
        // skip over the variable-length UNWIND_CODE array that follows.
        let Some(header) = binary.get_bytes(unwind_info_vma, 4) else {
            continue;
        };
        let flags = header[0] >> 3;
        let count_of_codes = header[2] as usize;
        if flags & UNW_FLAG_CHAININFO != 0 {
            continue; // chained unwind info -- see doc comment
        }
        if flags & (UNW_FLAG_EHANDLER | UNW_FLAG_UHANDLER) == 0 {
            continue; // no exception handler (ordinary function)
        }

        // UnwindCode[CountOfCodes] (2 bytes each), plus 2 bytes padding if
        // CountOfCodes is odd (keeps the following ULONG 4-byte aligned).
        let codes_bytes = count_of_codes * 2 + (count_of_codes % 2) * 2;
        let handler_rva_vma = unwind_info_vma + 4 + codes_bytes as u64;
        let Some(handler_rva_bytes) = binary.get_bytes(handler_rva_vma, 4) else {
            continue;
        };
        let handler_rva = u32::from_le_bytes(handler_rva_bytes.try_into().unwrap());
        if handler_rva == 0 {
            continue;
        }

        // The language-specific data (this GCC build's LSDA bytes) starts
        // immediately after the 4-byte ExceptionHandler RVA field.
        let lsda_vma = handler_rva_vma + 4;
        if lsda_vma >= xdata_end_vma {
            continue;
        }
        let max_len = usize::try_from((xdata_end_vma - lsda_vma).min(4096)).unwrap_or(0);
        let Some(lsda_bytes) = binary.get_bytes(lsda_vma, max_len) else {
            continue;
        };

        let region_end = region_start + (end_rva - begin_rva) as u64;
        if let Some(info) = parse_lsda(
            &lsda_bytes,
            lsda_vma,
            region_start,
            binary.is_64bit,
            &read_at,
            &symbol_at,
        )
        .filter(|info| call_sites_within_region(info, region_start, region_end))
        {
            out.insert(region_start, info);
        }
    }

    out
}

/// `.xdata`'s "language-specific handler data" isn't reserved exclusively
/// for GCC LSDAs the way ELF's `.gcc_except_table` section is -- *any*
/// `UNW_FLAG_EHANDLER`/`UHANDLER` handler can stash arbitrary bytes there
/// (e.g. mingw's CRT stack-probe handler), and there's no reliable way to
/// name-check `ExceptionHandler`'s target in a stripped binary to rule
/// those out upfront. Structural validation catches it after the fact
/// instead: every real LSDA call-site range and landing pad is, by
/// construction, an offset *inside the very function the LSDA belongs to*
/// -- so a `parse_lsda` result with any address outside `[region_start,
/// region_end)` is a non-LSDA handler's data having been decoded as if it
/// were one, not a real (if unusual) LSDA. Confirmed necessary against a
/// real binary: without this, one of `x64_seh_guarded_test.exe`'s non-C++
/// SEH handlers decoded as call-site addresses in the billions, wildly
/// outside any function.
fn call_sites_within_region(
    info: &crate::loader::gcc_lsda::LsdaInfo,
    region_start: u64,
    region_end: u64,
) -> bool {
    info.call_sites.iter().all(|cs| {
        let in_region = |addr: u64| (region_start..=region_end).contains(&addr);
        in_region(cs.start) && in_region(cs.end) && cs.landing_pad.is_none_or(in_region)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// End-to-end against a real `x86_64-w64-mingw32-g++`-compiled fixture
    /// (`guarded()`/`risky()`, `try`/`catch (const std::runtime_error&)`,
    /// same source shape as the ELF LSDA fixtures): `analyze_seh_lsda` must
    /// find both functions' language-specific data and resolve `guarded()`'s
    /// landing pad to the exact address confirmed by disassembly (the
    /// `cmp rdx, 0x1` type-selector check at the start of its `catch` body).
    #[test]
    fn analyze_seh_lsda_finds_guarded_landing_pad() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_seh_guarded_test.exe");
        let binary = LoadedBinary::from_file(&path).expect("load SEH test PE");

        // `LoadedBinary::from_file` must already have run the analyzer
        // (loader/mod.rs's `auto_detect_and_parse` wiring), not just
        // `analyze_seh_lsda` called standalone here.
        assert_eq!(binary.eh_lsda.len(), 2, "{:#x?}", binary.eh_lsda);

        let table = analyze_seh_lsda(&binary);
        assert_eq!(table, binary.eh_lsda);

        let guarded = &table[&0x1400014c9];
        assert_eq!(guarded.lp_start, 0x1400014c9); // LPStart omitted
        assert_eq!(guarded.call_sites.len(), 2);
        assert_eq!(
            guarded.call_sites[0].landing_pad,
            Some(0x1400014e6),
            "must match the real `cmp rdx, 0x1` catch-dispatch address from objdump"
        );
        assert_eq!(guarded.call_sites[0].action_chain, vec![1]);
        assert_eq!(guarded.call_sites[1].landing_pad, None);

        let risky = &table[&0x140001450];
        assert!(risky.type_table.is_empty()); // cleanup-only, no catch of its own
        assert_eq!(risky.call_sites.len(), 2);
    }
}
