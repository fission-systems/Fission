//! GCC/Itanium C++ LSDA byte-format parser -- the per-function
//! Language-Specific Data Area a C++ personality routine walks at unwind
//! time to find which `catch` clause (if any) matches a thrown exception,
//! and where its landing pad is. Documented informally in libgcc's
//! `unwind-c.c`/`unwind-pe.h`, not in the DWARF spec itself, so this parses
//! it by hand, cross-checked byte-for-byte against real `g++`-compiled
//! output before trusting it.
//!
//! This byte format is shared by two otherwise-unrelated container
//! conventions for *finding* the bytes in the first place:
//! - ELF: a pointer to it lives in `.eh_frame`'s per-FDE augmentation data,
//!   and the bytes themselves live in `.gcc_except_table` -- see
//!   [`super::elf::lsda::analyze_eh_lsda`].
//! - PE (mingw-w64 `g++` targeting Windows SEH): there's no `.eh_frame`;
//!   instead the same byte stream is appended directly after each
//!   function's `UNWIND_INFO` in `.xdata` (as the "language-specific
//!   handler data" following the `ExceptionHandler` RVA, when that handler
//!   is `__gxx_personality_seh0`) -- see [`super::pe::seh::analyze_seh_lsda`].
//!   Confirmed identical byte-for-byte against a real `x86_64-w64-mingw32-g++`
//!   build: the same `LPStart`/`TType`/call-site-table header layout, the
//!   same call-site record shape, decoding to the same landing pad address
//!   as the disassembly's actual `catch` block.
//!
//! Both callers just need "here are the LSDA's bytes, here's the function's
//! own base address (`region_start`, what call-site offsets are relative to
//! when `LPStart` is omitted, the overwhelmingly common case), here's how to
//! read arbitrary bytes/resolve a relocation symbol at an address" -- this
//! module has no opinion on how those bytes were found.

use gimli::constants::DwEhPe;

/// One entry of the LSDA's call-site table: an instruction range, and where
/// (if anywhere) to land if an exception unwinds through it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsdaCallSite {
    /// Absolute start address of the range this entry covers.
    pub start: u64,
    /// Absolute end address (exclusive).
    pub end: u64,
    /// Absolute address of the landing pad to unwind to, when this range
    /// has one (`None` means no unwind action needed for this range).
    pub landing_pad: Option<u64>,
    /// Type filters to try against the thrown exception's typeinfo, in
    /// match order: a positive value is a 1-based index into `LsdaInfo::
    /// type_table` (a `catch` of that type), `0` is a cleanup-only landing
    /// pad (runs destructors, no `catch` matches), negative values index an
    /// exception-specification table this parser doesn't resolve. Empty
    /// when there's no landing pad, or the landing pad is cleanup-only.
    pub action_chain: Vec<i64>,
}

/// One entry of the LSDA's type table -- the `std::type_info` a `catch`
/// clause matches against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsdaTypeEntry {
    /// Best-effort resolved address of the `type_info` object. For a
    /// dynamically-linked type in a PIE binary this is frequently *not*
    /// meaningful on its own -- the LSDA stores an indirect (GOT-style)
    /// pointer whose slot is only populated by the dynamic linker at
    /// process load time, so the raw file bytes read here are an
    /// unrelocated placeholder (commonly `0`). Prefer `symbol` when
    /// present; it's resolved independently via the binary's relocation
    /// table and doesn't have this limitation.
    pub address: u64,
    /// Symbol name for this type (e.g. `"typeinfo for std::runtime_error"`),
    /// resolved by checking the binary's relocation table for an entry at
    /// the GOT/indirect slot address -- confirmed against real `-fPIE`
    /// output where it's the *only* statically-available identification of
    /// a caught type. `None` for `catch (...)` (`address` is `0` with no
    /// relocation) or when no relocation covers the slot.
    pub symbol: Option<String>,
}

/// A parsed LSDA: one function's exception-handling table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LsdaInfo {
    /// Base address landing-pad offsets are relative to (defaults to the
    /// function's own start when the LSDA doesn't specify one explicitly,
    /// which is the overwhelmingly common case in practice).
    pub lp_start: u64,
    pub call_sites: Vec<LsdaCallSite>,
    /// Each caught type, in filter order (filter `N` -> `type_table[N - 1]`).
    pub type_table: Vec<LsdaTypeEntry>,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.pos)?;
        self.pos += 1;
        Some(b)
    }

    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos + n)?;
        self.pos += n;
        Some(s)
    }

    fn uleb128(&mut self) -> Option<u64> {
        let mut result: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            if shift < 64 {
                result |= u64::from(byte & 0x7f) << shift;
            }
            if byte & 0x80 == 0 {
                return Some(result);
            }
            shift += 7;
        }
    }

    fn sleb128(&mut self) -> Option<i64> {
        let mut result: i64 = 0;
        let mut shift = 0u32;
        let mut byte;
        loop {
            byte = self.u8()?;
            if shift < 64 {
                result |= i64::from(byte & 0x7f) << shift;
            }
            shift += 7;
            if byte & 0x80 == 0 {
                break;
            }
        }
        if shift < 64 && (byte & 0x40) != 0 {
            result |= -1i64 << shift;
        }
        Some(result)
    }
}

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

/// Reads one `DW_EH_PE`-encoded value's *magnitude*, applying its base
/// (`absptr` -> 0, `pcrel` -> `field_vma`) but stopping short of following
/// `DW_EH_PE_indirect` -- callers that need the indirect dereference decide
/// for themselves how to handle an unrelocated/placeholder read (see
/// `resolve_type_table`, the only indirect-sensitive caller).
///
/// Only `absptr` and `pcrel` application modes are supported (both
/// empirically validated against real `g++` output); `textrel`/`datarel`/
/// `funcrel`/`aligned` return `None` rather than risk silently computing a
/// wrong address from a guessed base -- none of these appeared in any
/// validation fixture, and `datarel` in particular is an embedded/EABI
/// convention no fixture here exercises.
fn read_encoded_value(
    cur: &mut Cursor<'_>,
    encoding: DwEhPe,
    field_vma: u64,
    is_64bit: bool,
) -> Option<u64> {
    if encoding == gimli::constants::DW_EH_PE_omit {
        return None;
    }
    let base = match encoding.application() {
        gimli::constants::DW_EH_PE_absptr => 0,
        gimli::constants::DW_EH_PE_pcrel => field_vma,
        _ => return None,
    };

    let magnitude = match encoding.format() {
        gimli::constants::DW_EH_PE_absptr => {
            let ptr_size = if is_64bit { 8 } else { 4 };
            read_native(cur.bytes(ptr_size)?, is_64bit)
        }
        gimli::constants::DW_EH_PE_uleb128 => cur.uleb128()?,
        gimli::constants::DW_EH_PE_udata2 => {
            u16::from_le_bytes(cur.bytes(2)?.try_into().ok()?) as u64
        }
        gimli::constants::DW_EH_PE_udata4 => {
            u32::from_le_bytes(cur.bytes(4)?.try_into().ok()?) as u64
        }
        gimli::constants::DW_EH_PE_udata8 => u64::from_le_bytes(cur.bytes(8)?.try_into().ok()?),
        gimli::constants::DW_EH_PE_sleb128 => cur.sleb128()? as u64,
        gimli::constants::DW_EH_PE_sdata2 => {
            i16::from_le_bytes(cur.bytes(2)?.try_into().ok()?) as i64 as u64
        }
        gimli::constants::DW_EH_PE_sdata4 => {
            i32::from_le_bytes(cur.bytes(4)?.try_into().ok()?) as i64 as u64
        }
        gimli::constants::DW_EH_PE_sdata8 => {
            i64::from_le_bytes(cur.bytes(8)?.try_into().ok()?) as u64
        }
        _ => return None,
    };

    Some(base.wrapping_add(magnitude))
}

/// [`read_encoded_value`] plus the `DW_EH_PE_indirect` dereference, for
/// fields that never need symbol resolution (call-site table entries,
/// `LPStart`) -- see `resolve_type_table` for the type-table path, which
/// needs the pre-dereference address too.
fn read_encoded(
    cur: &mut Cursor<'_>,
    encoding: DwEhPe,
    field_vma: u64,
    is_64bit: bool,
    read_at: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
) -> Option<u64> {
    let mut value = read_encoded_value(cur, encoding, field_vma, is_64bit)?;
    if encoding.is_indirect() {
        let ptr_size = if is_64bit { 8 } else { 4 };
        let bytes = read_at(value, ptr_size)?;
        value = read_native(&bytes, is_64bit);
    }
    Some(value)
}

/// Follows an LSDA action-table chain starting at 1-based byte offset
/// `first_action` (as stored in a call-site table entry's `cs_action`
/// field), returning the type filters tried in order. Each action record is
/// `(filter: sleb128, next_disp: sleb128)`; `next_disp == 0` ends the chain,
/// otherwise the next record is at the address of the `next_disp` field plus
/// `next_disp` -- relative to where that field itself is stored, after the
/// filter, not to where the record started (the Itanium ABI, and what
/// libgcc's `unwind-c.c` does: it reads the filter, then advances from the
/// position of the displacement). Measuring from the record start walks into
/// the middle of a record whenever a chain has more than one link, i.e.
/// whenever a `try` has more than one `catch`.
fn resolve_action_chain(
    data: &[u8],
    action_table_start: usize,
    first_action: usize,
) -> Option<Vec<i64>> {
    let mut chain = Vec::new();
    let mut pos = action_table_start
        .checked_add(first_action)?
        .checked_sub(1)?;
    for _ in 0..64 {
        let mut cur = Cursor::at(data, pos);
        let filter = cur.sleb128()?;
        let disp_field = cur.pos;
        let disp = cur.sleb128()?;
        chain.push(filter);
        if disp == 0 {
            return Some(chain);
        }
        pos = usize::try_from(i64::try_from(disp_field).ok()?.checked_add(disp)?).ok()?;
    }
    None // malformed or cyclic chain -- bail rather than loop forever
}

/// Resolves type-table entries `1..=max_filter`, each stored *backward* from
/// `ttype_base` (entry `N` at `ttype_base - N * entry_size`, per the
/// Itanium LSDA convention -- confirmed against the real fixtures'
/// `typeinfo for std::runtime_error`).
///
/// For each entry, always tries `symbol_at` against the *pre-indirect*
/// address first: a dynamically-linked type's GOT-style slot is only
/// populated by the loader at process start, so the raw bytes read from the
/// file there are frequently a meaningless placeholder (validated: the
/// `-fPIE` fixture's slot reads back as `0`) even though the relocation
/// table already names exactly which type it's for.
fn resolve_type_table(
    ttype_base: u64,
    ttype_encoding: DwEhPe,
    max_filter: i64,
    is_64bit: bool,
    read_at: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
    symbol_at: &dyn Fn(u64) -> Option<String>,
) -> Option<Vec<LsdaTypeEntry>> {
    let entry_size: u64 = match ttype_encoding.format() {
        gimli::constants::DW_EH_PE_absptr => {
            if is_64bit {
                8
            } else {
                4
            }
        }
        gimli::constants::DW_EH_PE_udata2 | gimli::constants::DW_EH_PE_sdata2 => 2,
        gimli::constants::DW_EH_PE_udata4 | gimli::constants::DW_EH_PE_sdata4 => 4,
        gimli::constants::DW_EH_PE_udata8 | gimli::constants::DW_EH_PE_sdata8 => 8,
        // A uleb128/sleb128 type-table entry would break the fixed-size
        // backward indexing this whole scheme depends on -- no real
        // producer does this. Unsupported rather than guessed.
        _ => return None,
    };

    // `max_filter` comes from the action table. The reads below stop at the
    // first one that falls outside the image, so reserve a modest start and let
    // the vector grow rather than trust the value for an allocation.
    let mut table = Vec::with_capacity(usize::try_from(max_filter).ok()?.min(4096));
    for filter in 1..=max_filter {
        let entry_vma =
            ttype_base.wrapping_sub(u64::try_from(filter).ok()?.wrapping_mul(entry_size));
        let bytes = read_at(entry_vma, entry_size as usize)?;
        let mut cur = Cursor::at(&bytes, 0);
        let pre_indirect =
            read_encoded_value(&mut cur, ttype_encoding, entry_vma, is_64bit).unwrap_or(0);

        let symbol = symbol_at(pre_indirect);
        let address = if ttype_encoding.is_indirect() {
            let ptr_size = if is_64bit { 8 } else { 4 };
            read_at(pre_indirect, ptr_size)
                .map(|b| read_native(&b, is_64bit))
                .unwrap_or(0)
        } else {
            pre_indirect
        };

        table.push(LsdaTypeEntry { address, symbol });
    }
    Some(table)
}

/// Parses one function's LSDA, per the header layout `libgcc`'s
/// `parse_lsda_header` (`unwind-c.c`) uses: `lpstart_encoding [+ LPStart]`,
/// `ttype_encoding [+ ttype_offset]`, `call_site_encoding`,
/// `call_site_table_length`, then the call-site table itself. `data` should
/// start at the LSDA's first byte and extend at least as far as its call-site
/// table + action table + type table reach -- callers don't need to know the
/// LSDA's exact length upfront, since every sub-table here is
/// self-delimiting (explicit length, or a chain/index that terminates on its
/// own).
pub fn parse_lsda(
    data: &[u8],
    lsda_vma: u64,
    region_start: u64,
    is_64bit: bool,
    read_at: &dyn Fn(u64, usize) -> Option<Vec<u8>>,
    symbol_at: &dyn Fn(u64) -> Option<String>,
) -> Option<LsdaInfo> {
    let mut cur = Cursor::at(data, 0);

    let lpstart_encoding = DwEhPe(cur.u8()?);
    let lp_start = if lpstart_encoding == gimli::constants::DW_EH_PE_omit {
        region_start
    } else {
        let field_vma = lsda_vma + cur.pos as u64;
        read_encoded(&mut cur, lpstart_encoding, field_vma, is_64bit, read_at)?
    };

    let ttype_encoding = DwEhPe(cur.u8()?);
    let ttype_base = if ttype_encoding != gimli::constants::DW_EH_PE_omit {
        let ttype_offset = cur.uleb128()?;
        Some(lsda_vma + cur.pos as u64 + ttype_offset)
    } else {
        None
    };

    let call_site_encoding = DwEhPe(cur.u8()?);
    let call_site_table_len = usize::try_from(cur.uleb128()?).ok()?;
    let call_site_table_end = cur.pos.checked_add(call_site_table_len)?;
    let action_table_start = call_site_table_end;

    let mut call_sites = Vec::new();
    let mut max_filter: i64 = 0;
    while cur.pos < call_site_table_end {
        let field_vma = lsda_vma + cur.pos as u64;
        let cs_start = read_encoded(&mut cur, call_site_encoding, field_vma, is_64bit, read_at)?;
        let field_vma = lsda_vma + cur.pos as u64;
        let cs_len = read_encoded(&mut cur, call_site_encoding, field_vma, is_64bit, read_at)?;
        let field_vma = lsda_vma + cur.pos as u64;
        let cs_lp = read_encoded(&mut cur, call_site_encoding, field_vma, is_64bit, read_at)?;
        let cs_action = cur.uleb128()?;

        let landing_pad = (cs_lp != 0).then(|| lp_start + cs_lp);
        let action_chain = if cs_action == 0 {
            Vec::new()
        } else {
            let chain =
                resolve_action_chain(data, action_table_start, usize::try_from(cs_action).ok()?)?;
            if let Some(&m) = chain.iter().filter(|f| **f > 0).max() {
                max_filter = max_filter.max(m);
            }
            chain
        };

        call_sites.push(LsdaCallSite {
            start: region_start + cs_start,
            end: region_start + cs_start + cs_len,
            landing_pad,
            action_chain,
        });
    }

    let type_table = match ttype_base {
        Some(ttype_base) if max_filter > 0 => resolve_type_table(
            ttype_base,
            ttype_encoding,
            max_filter,
            is_64bit,
            read_at,
            symbol_at,
        )?,
        _ => Vec::new(),
    };

    Some(LsdaInfo {
        lp_start,
        call_sites,
        type_table,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `risky()`'s LSDA from `testdata/x64_dyn_lsda_test.elf` (non-PIE),
    /// bytes cross-checked via `objdump -s -j .gcc_except_table` +
    /// `readelf --debug-dump=frames`: no `catch` clause of its own (just
    /// the temporary-object cleanup for constructing the thrown
    /// `std::runtime_error`), so `ttype_encoding` is `DW_EH_PE_omit` and
    /// every `cs_action` is `0`.
    #[test]
    fn parse_lsda_decodes_cleanup_only_call_sites() {
        let data: [u8; 12] = [
            0xff, 0xff, 0x01, 0x08, 0x29, 0x05, 0x47, 0x00, 0x3b, 0x22, 0x00, 0x00,
        ];
        let region_start = 0x4011a6;
        let info = parse_lsda(&data, 0x402160, region_start, true, &|_, _| None, &|_| None)
            .expect("parse risky()'s LSDA");

        assert_eq!(info.lp_start, region_start); // lpstart_encoding omitted
        assert!(info.type_table.is_empty());
        assert_eq!(
            info.call_sites,
            vec![
                LsdaCallSite {
                    start: region_start + 0x29,
                    end: region_start + 0x29 + 0x05,
                    landing_pad: Some(region_start + 0x47),
                    action_chain: vec![], // cs_action == 0: cleanup, no catch
                },
                LsdaCallSite {
                    start: region_start + 0x3b,
                    end: region_start + 0x3b + 0x22,
                    landing_pad: None, // cs_lp == 0
                    action_chain: vec![],
                },
            ]
        );
    }

    /// `guarded()`'s LSDA from the same fixture: `catch (const
    /// std::runtime_error&)`, so it has a real type table this time.
    /// `ttype_encoding` is `DW_EH_PE_udata4` (absolute, non-PIE), and the
    /// type-table entry's raw bytes (`c0 3d 40 00` LE = `0x403dc0`) match
    /// exactly the `R_X86_64_COPY` relocation `readelf -r` reports for
    /// `_ZTISt13runtime_error@GLIBCXX_3.4` at that address.
    #[test]
    fn parse_lsda_resolves_catch_type_table_absolute() {
        let data: [u8; 20] = [
            0xff, 0x03, 0x11, 0x01, 0x08, 0x17, 0x05, 0x24, 0x01, 0x2d, 0x05, 0x00, 0x00, 0x01,
            0x00, 0x00, 0xc0, 0x3d, 0x40, 0x00,
        ];
        let lsda_vma = 0x40216c;
        let region_start = 0x40120c;
        let read_at = |addr: u64, len: usize| {
            let offset = addr.checked_sub(lsda_vma)?;
            data.get(offset as usize..offset as usize + len)
                .map(<[u8]>::to_vec)
        };
        let info = parse_lsda(&data, lsda_vma, region_start, true, &read_at, &|_| None)
            .expect("parse guarded()'s LSDA");

        assert_eq!(info.lp_start, region_start);
        assert_eq!(
            info.call_sites,
            vec![
                LsdaCallSite {
                    start: region_start + 0x17,
                    end: region_start + 0x17 + 0x05,
                    landing_pad: Some(region_start + 0x24),
                    action_chain: vec![1], // catches type-table filter 1
                },
                LsdaCallSite {
                    start: region_start + 0x2d,
                    end: region_start + 0x2d + 0x05,
                    landing_pad: None,
                    action_chain: vec![],
                },
            ]
        );
        assert_eq!(
            info.type_table,
            vec![LsdaTypeEntry {
                address: 0x403dc0,
                symbol: None, // no relocation table wired into this unit test
            }]
        );
    }
}

#[cfg(test)]
mod action_chain_tests {
    use super::*;

    /// `guarded()`'s LSDA from `testdata/x64_cpp_eh_dyn.elf` (g++ -O0, non-PIE):
    /// three `catch` clauses, so the call site's action chain is
    /// `(1, -3) -> (2, -3) -> (3, 0)`. Each displacement is relative to *its own
    /// field* (the Itanium ABI and libgcc's `unwind-c.c`), so `-3` from the
    /// field of record 1 lands on record 2. `ttype_encoding` is `udata4`.
    const GUARDED: [u8; 32] = [
        0xff, 0x03, 0x1d, 0x01, 0x08, 0x11, 0x05, 0x1d, 0x05, 0x38, 0x05, 0x00, 0x00, 0x03, 0x00,
        0x02, 0x7d, 0x01, 0x7d, 0x00, 0x40, 0x3d, 0x40, 0x00, 0xa8, 0x3d, 0x40, 0x00, 0x10, 0x20,
        0x40, 0x00,
    ];
    const LSDA_VMA: u64 = 0x402190;
    const REGION: u64 = 0x40125c;

    fn parse() -> Option<LsdaInfo> {
        let read_at = |addr: u64, len: usize| {
            let offset = usize::try_from(addr.checked_sub(LSDA_VMA)?).ok()?;
            GUARDED.get(offset..offset + len).map(<[u8]>::to_vec)
        };
        let symbol_at = |addr: u64| match addr {
            0x403da8 => Some("typeinfo for std::exception".to_string()),
            0x403d40 => Some("typeinfo for int".to_string()),
            _ => None,
        };
        parse_lsda(&GUARDED, LSDA_VMA, REGION, true, &read_at, &symbol_at)
    }

    #[test]
    fn a_chain_of_several_catch_clauses_is_followed_to_its_end() {
        let info = parse().expect("a multi-catch LSDA must parse, not be dropped");
        assert_eq!(info.call_sites[0].action_chain, vec![1, 2, 3]);
        assert_eq!(info.call_sites[0].landing_pad, Some(REGION + 0x1d));
    }

    #[test]
    fn every_filter_of_the_chain_gets_its_type_table_entry() {
        let info = parse().expect("parse");
        let addresses: Vec<u64> = info.type_table.iter().map(|t| t.address).collect();
        // filter N is stored at ttype_base - 4 * N
        assert_eq!(addresses, vec![0x402010, 0x403da8, 0x403d40]);
        let symbols: Vec<Option<&str>> = info
            .type_table
            .iter()
            .map(|t| t.symbol.as_deref())
            .collect();
        assert_eq!(
            symbols,
            vec![
                None,
                Some("typeinfo for std::exception"),
                Some("typeinfo for int")
            ]
        );
    }

    /// End to end on the real file: `guarded()` has three `catch` clauses and
    /// used to be missing from `eh_lsda` altogether, taking its landing pad
    /// with it.
    #[test]
    fn a_multi_catch_function_is_present_in_the_loaded_binary() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata/x64_cpp_eh_dyn.elf");
        let binary = crate::loader::LoadedBinary::from_file(&path).expect("load fixture");
        let guarded = binary
            .eh_lsda
            .get(&REGION)
            .expect("guarded()'s LSDA is in eh_lsda");
        assert_eq!(guarded.call_sites[0].action_chain, vec![1, 2, 3]);
        let symbols: Vec<Option<&str>> = guarded
            .type_table
            .iter()
            .map(|t| t.symbol.as_deref())
            .collect();
        assert_eq!(symbols[1], Some("typeinfo for std::exception"));
        assert_eq!(symbols[2], Some("typeinfo for int"));
        // risky()'s cleanup-only LSDA, found before the fix too, stays.
        assert!(binary.eh_lsda.contains_key(&0x4011a6));
    }

    /// The control the existing fixture tests rely on: a one-record chain
    /// (`next == 0`) is unaffected by which origin the displacement uses.
    #[test]
    fn a_single_record_chain_is_unchanged() {
        assert_eq!(resolve_action_chain(&[0x01, 0x00], 0, 1), Some(vec![1]));
    }

    /// A displacement walking out of the table ends the walk instead of
    /// reading garbage.
    #[test]
    fn a_chain_that_leaves_the_table_is_refused() {
        // record (1, -100): the "next" record would be before the table
        assert_eq!(resolve_action_chain(&[0x01, 0x9c, 0x7f], 0, 1), None);
    }
}
