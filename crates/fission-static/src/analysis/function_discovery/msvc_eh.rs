//! MSVC C++ exception-handling `FuncInfo` table scanner (x86 only).
//!
//! Real-world coverage gap this closes: catch handlers and unwind/cleanup
//! actions are reachable *only* from these tables, never from any `call`/
//! `jmp` in the code and never matched by the ghidra-pattern XML database
//! -- every other scanner in this module starts from "an address the code
//! itself references", so these were structurally invisible to Fission
//! before this scanner existed. Confirmed against a real MSVC-built
//! `sqlite3.dll`: cross-referencing Fission's discovered functions against
//! a Ghidra analysis of the same binary, every one of Ghidra's non-thunk
//! misses was a named MSVC EH support routine (`Catch_All@...`,
//! `Unwind@...`) or an unnamed function with a real prologue reachable
//! only from a `FuncInfo` table.
//!
//! # Layout, and where it came from
//!
//! Ghidra ships a dedicated analyzer for this exact structure
//! (`PEExceptionAnalyzer`/`EHFunctionInfoModel` et al., under
//! `Ghidra/Features/MicrosoftCodeAnalyzer`), which this scanner was
//! verified field-offset-by-field-offset against (not reverse-engineered
//! from scratch): the struct shapes below, the magic number, and the
//! "same memory block" sanity check are all sourced from that reference
//! implementation, not guessed. No signature database or external file is
//! needed -- these are Microsoft's own public `ehdata.h` struct layouts,
//! a small, fixed set of magic constants, hardcoded here the same way
//! `fission-loader` already hardcodes the PE/COFF header layout.
//!
//! `FuncInfo`'s first field is a magic number ORed with 3 "bbt flags" bits
//! in the top of the word (`field0 & 0x1FFF_FFFF` recovers the magic;
//! Ghidra: `AbstractCreateDataTypeModel`/`EHFunctionInfoModel.init()`).
//! Valid values are exactly `0x19930520`/`0x19930521`/`0x19930522`
//! (VC2-VC6 / VC7 / VC7+-with-expected-exception-spec-list); the version
//! only changes trailing fields (`pESTypeList`, `EHFlags`) this scanner
//! doesn't need, since it only walks the unwind/try-block maps every
//! version has.
//!
//! Two encodings exist depending on target: x64 (and, per Ghidra's own
//! `isRelative() == is64Bit()`, only x64) uses image-base-relative 32-bit
//! displacements instead of absolute pointers -- a different, unimplemented
//! layout, not attempted here. x86 -- our case, and this scanner's only
//! supported target -- uses plain absolute VAs for every pointer field,
//! which is what's parsed below:
//!
//! ```text
//! FuncInfo (28 bytes; x86, all versions read the same first 28 bytes):
//!   +0  magicNumber_and_bbtFlags  u32
//!   +4  maxState                 i32   -- UnwindMapEntry count
//!   +8  pUnwindMap                VA   -- UnwindMapEntry[maxState], or 0
//!   +12 nTryBlocks                u32
//!   +16 pTryBlockMap              VA   -- TryBlockMapEntry[nTryBlocks], or 0
//!   +20 nIPMapEntries             u32  -- not walked (no function pointers)
//!   +24 pIPToStateMap             VA   -- not walked (no function pointers)
//!
//! UnwindMapEntry (8 bytes):
//!   +0 toState   i32
//!   +4 action     VA  -- cleanup/unwind function, or 0            [candidate]
//!
//! TryBlockMapEntry (20 bytes):
//!   +0  tryLow       i32
//!   +4  tryHigh      i32
//!   +8  catchHigh    i32
//!   +12 nCatches     u32
//!   +16 pHandlerArray VA  -- HandlerType[nCatches]
//!
//! HandlerType (16 bytes; x86 -- x64 adds a trailing dispFrame field this
//! scanner never reads since it's x86-only):
//!   +0  adjectives       u32
//!   +4  pType             VA  -- RTTI type descriptor, not a function
//!   +8  dispCatchObj     i32
//!   +12 addressOfHandler  VA  -- catch handler entry point         [candidate]
//! ```

use fission_loader::loader::LoadedBinary;

const EH_MAGIC_V1: u32 = 0x1993_0520;
const EH_MAGIC_V2: u32 = 0x1993_0521;
const EH_MAGIC_V3: u32 = 0x1993_0522;

/// Mirrors Ghidra's own `PEExceptionAnalyzer.MAX_MAP_ENTRY_COUNT`: a count
/// field this large is a magic-number coincidence in unrelated data, not a
/// real `FuncInfo`.
const MAX_MAP_ENTRIES: u32 = 16_000;

pub(crate) fn scan_msvc_eh_funcinfo(binary: &LoadedBinary) -> Vec<u64> {
    // x64 FuncInfo uses image-base-relative displacements instead of
    // absolute pointers -- see module doc. Not attempted here.
    if binary.is_64bit {
        return Vec::new();
    }

    // Matches Ghidra's own search scope: `.rdata`, falling back to `.text`
    // if the binary has no `.rdata` (e.g. some non-MSVC-linker PEs).
    let mut sections: Vec<_> = binary
        .sections
        .iter()
        .filter(|s| s.name == ".rdata")
        .collect();
    if sections.is_empty() {
        sections = binary
            .sections
            .iter()
            .filter(|s| s.name == ".text")
            .collect();
    }

    let mut candidates = Vec::new();
    for section in sections {
        let Some(bytes) = binary.view_bytes(section.virtual_address, section.virtual_size as usize)
        else {
            continue;
        };
        let section_start = section.virtual_address;
        let section_end = section.virtual_address + section.virtual_size;

        // 4-byte aligned scan, matching Ghidra's own `alignment = 4`.
        let mut offset = 0usize;
        while offset + 4 <= bytes.len() {
            let field0 = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let magic = field0 & 0x1FFF_FFFF;
            if matches!(magic, EH_MAGIC_V1 | EH_MAGIC_V2 | EH_MAGIC_V3) {
                let addr = section_start + offset as u64;
                collect_func_info_candidates(
                    binary,
                    addr,
                    section_start,
                    section_end,
                    &mut candidates,
                );
            }
            offset += 4;
        }
    }

    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

fn collect_func_info_candidates(
    binary: &LoadedBinary,
    func_info_addr: u64,
    section_start: u64,
    section_end: u64,
    out: &mut Vec<u64>,
) {
    let Some(header) = binary.get_bytes(func_info_addr, 28) else {
        return;
    };
    let read_u32 = |off: usize| u32::from_le_bytes(header[off..off + 4].try_into().unwrap());
    let max_state = read_u32(4);
    let p_unwind_map = read_u32(8) as u64;
    let n_try_blocks = read_u32(12);
    let p_try_block_map = read_u32(16) as u64;
    let n_ip_map_entries = read_u32(20);

    // Mirrors Ghidra's `EHFunctionInfoModel.validateModelSpecificInfo`:
    // reject unreasonable counts, and reject a FuncInfo with no map data at
    // all (a magic-number coincidence, not a real match).
    if max_state > MAX_MAP_ENTRIES
        || n_try_blocks > MAX_MAP_ENTRIES
        || n_ip_map_entries > MAX_MAP_ENTRIES
    {
        return;
    }
    if max_state == 0 && n_try_blocks == 0 && n_ip_map_entries == 0 {
        return;
    }

    // Mirrors Ghidra's `validateLocationsInSameBlock`: every referenced
    // sub-table must live in the same section as the FuncInfo itself --
    // real compiler-emitted tables always do, so a pointer landing
    // elsewhere means this wasn't really a FuncInfo match.
    let in_same_section = |addr: u64| addr != 0 && addr >= section_start && addr < section_end;

    if max_state > 0 && in_same_section(p_unwind_map) {
        let len = max_state as usize * 8;
        if let Some(bytes) = binary.get_bytes(p_unwind_map, len) {
            for entry in bytes.chunks_exact(8) {
                let action = u32::from_le_bytes(entry[4..8].try_into().unwrap());
                if action != 0 {
                    out.push(action as u64);
                }
            }
        }
    }

    if n_try_blocks > 0 && in_same_section(p_try_block_map) {
        let len = n_try_blocks as usize * 20;
        if let Some(try_blocks) = binary.get_bytes(p_try_block_map, len) {
            for try_block in try_blocks.chunks_exact(20) {
                let n_catches = u32::from_le_bytes(try_block[12..16].try_into().unwrap());
                let p_handler_array =
                    u32::from_le_bytes(try_block[16..20].try_into().unwrap()) as u64;
                if n_catches == 0
                    || n_catches > MAX_MAP_ENTRIES
                    || !in_same_section(p_handler_array)
                {
                    continue;
                }
                let handlers_len = n_catches as usize * 16;
                let Some(handlers) = binary.get_bytes(p_handler_array, handlers_len) else {
                    continue;
                };
                for handler in handlers.chunks_exact(16) {
                    let address_of_handler =
                        u32::from_le_bytes(handler[12..16].try_into().unwrap());
                    if address_of_handler != 0 {
                        out.push(address_of_handler as u64);
                    }
                }
            }
        }
    }
}
