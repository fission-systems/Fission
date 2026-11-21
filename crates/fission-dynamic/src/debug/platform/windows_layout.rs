//! Byte layouts of Windows structures, read and written without a process.
//!
//! The live Windows backend reads these structures out of a debuggee, and
//! until now cast its byte buffers to `#[repr(C)]` structs: an unaligned
//! `&[u32]` over a `Vec<u8>`, a header pointer at a file offset nobody checked,
//! an index computed as `count - 1` with `count` zero. Every rule here works on
//! byte offsets with checked arithmetic instead, and is compiled on every host
//! so that its tests run where the Windows debugger cannot.

/// What a PE image's headers say, by byte offset into the image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeHeaders {
    /// Offset of the `PE\0\0` signature (`e_lfanew`).
    pub nt_offset: usize,
    pub number_of_sections: u16,
    /// `PE32+` (64-bit) rather than `PE32`.
    pub pe32_plus: bool,
    /// Offset of the optional header.
    pub optional_offset: usize,
    /// Offset of the section table: after the optional header, whose size the
    /// file header gives. A fixed `size_of::<IMAGE_NT_HEADERS64>()` is wrong
    /// for every `PE32` image and for any image with a non-default size.
    pub section_table_offset: usize,
    pub section_alignment: u32,
    pub file_alignment: u32,
    pub size_of_headers: u32,
    pub number_of_rva_and_sizes: u32,
}

const SECTION_HEADER_SIZE: usize = 40;
const IMPORT_DESCRIPTOR_SIZE: usize = 20;
const DATA_DIRECTORY_IMPORT: usize = 1;
const DATA_DIRECTORY_IAT: usize = 12;

pub(crate) fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

pub(crate) fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

pub(crate) fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    Some(u64::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
}

fn write_bytes(bytes: &mut [u8], offset: usize, value: &[u8]) -> Result<(), String> {
    let end = offset
        .checked_add(value.len())
        .ok_or_else(|| format!("offset 0x{offset:x} overflows"))?;
    bytes
        .get_mut(offset..end)
        .ok_or_else(|| format!("a write at 0x{offset:x} runs past the image"))?
        .copy_from_slice(value);
    Ok(())
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) -> Result<(), String> {
    write_bytes(bytes, offset, &value.to_le_bytes())
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), String> {
    write_bytes(bytes, offset, &value.to_le_bytes())
}

/// `count` little-endian 32-bit words from the start of `bytes`.
///
/// A read that came back short is an error, not a shorter table: the old code
/// built a slice of `count` words over whatever had been read.
pub(crate) fn u32_table(bytes: &[u8], count: usize) -> Result<Vec<u32>, String> {
    let needed = count
        .checked_mul(4)
        .ok_or_else(|| format!("a table of {count} words is too large"))?;
    let bytes = bytes
        .get(..needed)
        .ok_or_else(|| format!("the table needs {needed} bytes, {} were read", bytes.len()))?;
    Ok(bytes
        .chunks_exact(4)
        .map(|word| u32::from_le_bytes(word.try_into().expect("four bytes")))
        .collect())
}

/// `count` little-endian 16-bit words from the start of `bytes`.
pub(crate) fn u16_table(bytes: &[u8], count: usize) -> Result<Vec<u16>, String> {
    let needed = count
        .checked_mul(2)
        .ok_or_else(|| format!("a table of {count} halfwords is too large"))?;
    let bytes = bytes
        .get(..needed)
        .ok_or_else(|| format!("the table needs {needed} bytes, {} were read", bytes.len()))?;
    Ok(bytes
        .chunks_exact(2)
        .map(|half| u16::from_le_bytes(half.try_into().expect("two bytes")))
        .collect())
}

/// The most entries an export table is read with. The largest system DLLs
/// export a few thousand functions; a count far beyond that is a corrupt or
/// hostile header, and taking it at its word allocates gigabytes.
pub(crate) const MAX_EXPORT_ENTRIES: u32 = 1 << 20;

/// One export: its ordinal, its RVA, and the index into the name tables of the
/// name it is exported under, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExportSlot {
    pub ordinal: u32,
    pub rva: u32,
    pub name_index: Option<usize>,
}

/// Join the three export tables: function RVAs, and the parallel name and
/// name-ordinal tables. An empty function slot (RVA 0) is no export; a name
/// whose ordinal points past the function table names nothing.
pub(crate) fn export_slots(
    ordinal_base: u32,
    function_rvas: &[u32],
    name_ordinals: &[u16],
) -> Vec<ExportSlot> {
    let mut names = vec![None; function_rvas.len()];
    for (name_index, ordinal) in name_ordinals.iter().enumerate() {
        if let Some(slot) = names.get_mut(*ordinal as usize) {
            slot.get_or_insert(name_index);
        }
    }
    function_rvas
        .iter()
        .enumerate()
        .filter(|(_, rva)| **rva != 0)
        .map(|(index, rva)| ExportSlot {
            ordinal: ordinal_base.wrapping_add(index as u32),
            rva: *rva,
            name_index: names[index],
        })
        .collect()
}

impl PeHeaders {
    /// Parse the headers at the start of `bytes`, which must hold at least the
    /// DOS header, the NT headers and the section table.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        if read_u16(bytes, 0) != Some(0x5a4d) {
            return Err("no MZ signature".to_string());
        }
        let e_lfanew = read_u32(bytes, 0x3c).ok_or("the DOS header is truncated")?;
        // `e_lfanew` is a `LONG`; a negative one sign-extended to an address
        // somewhere near the top of memory.
        if e_lfanew > i32::MAX as u32 {
            return Err(format!("e_lfanew 0x{e_lfanew:x} is negative"));
        }
        let nt_offset = e_lfanew as usize;
        if read_u32(bytes, nt_offset) != Some(0x0000_4550) {
            return Err(format!("no PE signature at 0x{nt_offset:x}"));
        }
        let file_header = nt_offset + 4;
        let number_of_sections =
            read_u16(bytes, file_header + 2).ok_or("the file header is truncated")?;
        let size_of_optional_header =
            read_u16(bytes, file_header + 16).ok_or("the file header is truncated")? as usize;
        let optional_offset = file_header + 20;
        let magic = read_u16(bytes, optional_offset).ok_or("the optional header is truncated")?;
        let pe32_plus = match magic {
            0x10b => false,
            0x20b => true,
            other => return Err(format!("unknown optional header magic 0x{other:x}")),
        };
        let rva_count_offset = optional_offset + if pe32_plus { 108 } else { 92 };
        let field = |offset: usize| {
            read_u32(bytes, offset).ok_or_else(|| "the optional header is truncated".to_string())
        };
        let headers = Self {
            nt_offset,
            number_of_sections,
            pe32_plus,
            optional_offset,
            section_table_offset: optional_offset + size_of_optional_header,
            section_alignment: field(optional_offset + 32)?,
            file_alignment: field(optional_offset + 36)?,
            size_of_headers: field(optional_offset + 60)?,
            number_of_rva_and_sizes: field(rva_count_offset)?,
        };
        let table_end = headers.section_offset(number_of_sections as usize)?;
        if table_end > bytes.len() {
            return Err(format!(
                "the section table runs to 0x{table_end:x}, past the 0x{:x} bytes there are",
                bytes.len()
            ));
        }
        Ok(headers)
    }

    /// Offset of section header `index`.
    pub(crate) fn section_offset(&self, index: usize) -> Result<usize, String> {
        index
            .checked_mul(SECTION_HEADER_SIZE)
            .and_then(|offset| offset.checked_add(self.section_table_offset))
            .ok_or_else(|| format!("section {index} is out of range"))
    }

    fn data_directory_offset(&self, index: usize) -> Option<usize> {
        if index as u32 >= self.number_of_rva_and_sizes {
            return None;
        }
        let first = self.optional_offset + if self.pe32_plus { 112 } else { 96 };
        Some(first + index * 8)
    }
}

/// One import to put into a rebuilt import table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSpec {
    pub module: String,
    /// Imported by name, or by `ordinal` when `None`.
    pub function: Option<String>,
    pub ordinal: u16,
}

fn align_up(value: u64, alignment: u64) -> Option<u64> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    value
        .checked_add(alignment - 1)
        .map(|v| v & !(alignment - 1))
}

/// Append a section holding a fresh import table for `imports` to the PE image
/// in `image`, and point the image's import directory at it.
///
/// The image is checked before anything is written: a header that cannot hold
/// another section header, a file with no sections, alignments that are not
/// powers of two, and offsets past the end are refused rather than written
/// through.
pub(crate) fn append_import_section(
    image: &mut Vec<u8>,
    imports: &[ImportSpec],
) -> Result<(), String> {
    let headers = PeHeaders::parse(image)?;
    if headers.number_of_sections == 0 {
        return Err("the image has no sections to place a new one after".to_string());
    }
    let import_dir = headers
        .data_directory_offset(DATA_DIRECTORY_IMPORT)
        .ok_or("the image has no import data directory")?;
    let file_alignment = u64::from(headers.file_alignment);
    let section_alignment = u64::from(headers.section_alignment);
    align_up(0, file_alignment).ok_or("the file alignment is not a power of two")?;
    align_up(0, section_alignment).ok_or("the section alignment is not a power of two")?;

    let new_header = headers.section_offset(headers.number_of_sections as usize)?;
    let new_header_end = new_header + SECTION_HEADER_SIZE;
    if new_header_end > headers.size_of_headers as usize || new_header_end > image.len() {
        return Err("there is no room in the headers for another section header".to_string());
    }

    // The new section goes after everything the existing sections occupy.
    let mut image_end = 0u64;
    for index in 0..headers.number_of_sections as usize {
        let at = headers.section_offset(index)?;
        let virtual_size = read_u32(image, at + 8).ok_or("a section header is truncated")?;
        let virtual_address = read_u32(image, at + 12).ok_or("a section header is truncated")?;
        let raw_size = read_u32(image, at + 16).ok_or("a section header is truncated")?;
        let end = u64::from(virtual_address) + u64::from(virtual_size.max(raw_size));
        image_end = image_end.max(end);
    }
    let section_rva = align_up(image_end, section_alignment).ok_or("the image is too large")?;

    // Layout inside the section: descriptors, then per module its name, its
    // lookup table, its address table and its hint/name entries.
    let thunk_size = if headers.pe32_plus { 8 } else { 4 };
    let ordinal_flag: u64 = if headers.pe32_plus { 1 << 63 } else { 1 << 31 };
    let mut modules: Vec<&str> = imports.iter().map(|i| i.module.as_str()).collect();
    modules.sort_unstable();
    modules.dedup();
    let descriptors_size = (modules.len() + 1) * IMPORT_DESCRIPTOR_SIZE;
    let mut data: Vec<u8> = vec![0; descriptors_size];
    let rva_of = |offset: usize| -> Result<u32, String> {
        u32::try_from(section_rva + offset as u64)
            .map_err(|_| "the new section lies past 4 GiB".to_string())
    };
    for (module_index, module) in modules.iter().enumerate() {
        let entries: Vec<&ImportSpec> = imports.iter().filter(|i| i.module == *module).collect();
        let name_offset = data.len();
        data.extend_from_slice(module.as_bytes());
        data.push(0);
        data.resize(align_up(data.len() as u64, 8).expect("8") as usize, 0);
        let lookup_offset = data.len();
        let table_size = (entries.len() + 1) * thunk_size;
        data.resize(lookup_offset + 2 * table_size, 0);
        let address_offset = lookup_offset + table_size;
        for (slot, entry) in entries.iter().enumerate() {
            let thunk = match &entry.function {
                Some(function) => {
                    data.resize(align_up(data.len() as u64, 2).expect("2") as usize, 0);
                    let hint_name = data.len();
                    data.extend_from_slice(&0u16.to_le_bytes());
                    data.extend_from_slice(function.as_bytes());
                    data.push(0);
                    u64::from(rva_of(hint_name)?)
                }
                None => ordinal_flag | u64::from(entry.ordinal),
            };
            let bytes = thunk.to_le_bytes();
            for table in [lookup_offset, address_offset] {
                let at = table + slot * thunk_size;
                data[at..at + thunk_size].copy_from_slice(&bytes[..thunk_size]);
            }
        }
        let descriptor = module_index * IMPORT_DESCRIPTOR_SIZE;
        write_u32(&mut data, descriptor, rva_of(lookup_offset)?)?;
        write_u32(&mut data, descriptor + 12, rva_of(name_offset)?)?;
        write_u32(&mut data, descriptor + 16, rva_of(address_offset)?)?;
    }
    let virtual_size = u32::try_from(data.len()).map_err(|_| "the import table is too large")?;
    let raw_size = align_up(data.len() as u64, file_alignment).ok_or("too large")?;
    data.resize(raw_size as usize, 0);

    // The section's bytes start on a file-alignment boundary.
    let raw_pointer = align_up(image.len() as u64, file_alignment).ok_or("too large")?;
    let raw_pointer_u32 =
        u32::try_from(raw_pointer).map_err(|_| "the file is larger than 4 GiB".to_string())?;
    let section_rva_u32 = rva_of(0)?;
    let size_of_image = align_up(
        section_rva + raw_size.max(virtual_size as u64),
        section_alignment,
    )
    .and_then(|size| u32::try_from(size).ok())
    .ok_or("the image would be larger than 4 GiB")?;

    // Nothing above wrote to the image; from here on it is all checked writes.
    let mut header = [0u8; SECTION_HEADER_SIZE];
    header[..8].copy_from_slice(b".fission");
    header[8..12].copy_from_slice(&virtual_size.to_le_bytes());
    header[12..16].copy_from_slice(&section_rva_u32.to_le_bytes());
    header[16..20].copy_from_slice(&(raw_size as u32).to_le_bytes());
    header[20..24].copy_from_slice(&raw_pointer_u32.to_le_bytes());
    header[36..40].copy_from_slice(&0xC000_0040u32.to_le_bytes());
    write_bytes(image, new_header, &header)?;
    write_u16(image, headers.nt_offset + 6, headers.number_of_sections + 1)?;
    write_u32(image, headers.optional_offset + 56, size_of_image)?;
    write_u32(image, import_dir, section_rva_u32)?;
    write_u32(image, import_dir + 4, descriptors_size as u32)?;
    if let Some(iat_dir) = headers.data_directory_offset(DATA_DIRECTORY_IAT) {
        write_u32(image, iat_dir, 0)?;
        write_u32(image, iat_dir + 4, 0)?;
    }
    image.resize(raw_pointer as usize, 0);
    image.extend_from_slice(&data);
    Ok(())
}

/// One record of a thread's SEH chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SehLink {
    /// Where the record is.
    pub address: u64,
    pub next: u64,
    pub handler: u64,
}

/// Walk an SEH chain from its head, `NtTib.ExceptionList` -- the pointer
/// stored *at* the TEB, not the TEB itself.
///
/// `read_pointer` reads one pointer of the target's width. The chain ends at a
/// null pointer, at the all-ones end marker of either width, at a record that
/// cannot be read, at a record seen before, or after `max_depth` records.
pub(crate) fn walk_seh_chain(
    head: u64,
    pointer_size: u64,
    max_depth: usize,
    read_pointer: impl Fn(u64) -> Option<u64>,
) -> Vec<SehLink> {
    let end_marker = if pointer_size == 4 {
        0xFFFF_FFFF
    } else {
        u64::MAX
    };
    let mut links = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut current = head;
    while links.len() < max_depth
        && current != 0
        && current != end_marker
        && current != 0xFFFF_FFFF
        && seen.insert(current)
    {
        let Some(handler_at) = current.checked_add(pointer_size) else {
            break;
        };
        let (Some(next), Some(handler)) = (read_pointer(current), read_pointer(handler_at)) else {
            break;
        };
        links.push(SehLink {
            address: current,
            next,
            handler,
        });
        current = next;
    }
    links
}

/// Fields of the 64-bit PEB, by offset.
pub(crate) mod peb64 {
    pub const BEING_DEBUGGED: u64 = 0x02;
    /// `0x08` is `Mutant`; the image base is the next pointer.
    pub const IMAGE_BASE_ADDRESS: u64 = 0x10;
    pub const LDR: u64 = 0x18;
    pub const PROCESS_PARAMETERS: u64 = 0x20;
    /// The default heap itself, not an array of heaps (`ProcessHeaps` is at
    /// `0xF0`).
    pub const PROCESS_HEAP: u64 = 0x30;
    pub const NT_GLOBAL_FLAG: u64 = 0xBC;
}

/// Fields of a 64-bit `_HEAP` (Windows Vista and later), by offset.
pub(crate) mod heap64 {
    pub const FLAGS: u64 = 0x70;
    pub const FORCE_FLAGS: u64 = 0x74;
    /// `HEAP_GROWABLE`: what `Flags` holds for a heap created without a
    /// debugger.
    pub const GROWABLE: u32 = 0x2;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE32+ (or PE32) image: headers in the first 0x400 bytes, one
    /// section at file offset 0x400.
    fn image(pe32_plus: bool, sections: u16) -> Vec<u8> {
        let mut bytes = vec![0u8; 0x600];
        bytes[0..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        let file_header = 0x84;
        bytes[file_header..file_header + 2].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[file_header + 2..file_header + 4].copy_from_slice(&sections.to_le_bytes());
        let optional_size: u16 = if pe32_plus { 0xF0 } else { 0xE0 };
        bytes[file_header + 16..file_header + 18].copy_from_slice(&optional_size.to_le_bytes());
        let optional = file_header + 20;
        let magic: u16 = if pe32_plus { 0x20b } else { 0x10b };
        bytes[optional..optional + 2].copy_from_slice(&magic.to_le_bytes());
        bytes[optional + 32..optional + 36].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[optional + 36..optional + 40].copy_from_slice(&0x200u32.to_le_bytes());
        bytes[optional + 56..optional + 60].copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[optional + 60..optional + 64].copy_from_slice(&0x400u32.to_le_bytes());
        let rva_count = optional + if pe32_plus { 108 } else { 92 };
        bytes[rva_count..rva_count + 4].copy_from_slice(&16u32.to_le_bytes());
        let table = optional + optional_size as usize;
        for index in 0..sections as usize {
            let at = table + index * 40;
            bytes[at..at + 8].copy_from_slice(b".text\0\0\0");
            bytes[at + 8..at + 12].copy_from_slice(&0x180u32.to_le_bytes());
            bytes[at + 12..at + 16]
                .copy_from_slice(&(0x1000u32 * (index as u32 + 1)).to_le_bytes());
            bytes[at + 16..at + 20].copy_from_slice(&0x200u32.to_le_bytes());
            bytes[at + 20..at + 24].copy_from_slice(&0x400u32.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn the_section_table_follows_the_optional_header_the_file_header_sizes() {
        let plus = PeHeaders::parse(&image(true, 1)).unwrap();
        assert!(plus.pe32_plus);
        assert_eq!(plus.section_table_offset, 0x98 + 0xF0);
        // A PE32 image's optional header is 0x10 bytes shorter; the fixed
        // `size_of::<IMAGE_NT_HEADERS64>()` put its section table 0x10 bytes
        // too far on.
        let pe32 = PeHeaders::parse(&image(false, 1)).unwrap();
        assert!(!pe32.pe32_plus);
        assert_eq!(pe32.section_table_offset, 0x98 + 0xE0);
        assert_eq!(pe32.file_alignment, 0x200);
        assert_eq!(pe32.section_alignment, 0x1000);
    }

    #[test]
    fn headers_that_point_outside_the_image_are_refused() {
        // Negative e_lfanew.
        let mut bytes = image(true, 1);
        bytes[0x3c..0x40].copy_from_slice(&0x8000_0080u32.to_le_bytes());
        assert!(PeHeaders::parse(&bytes).unwrap_err().contains("negative"));
        // Past the end.
        let mut bytes = image(true, 1);
        bytes[0x3c..0x40].copy_from_slice(&0x7000_0000u32.to_le_bytes());
        assert!(PeHeaders::parse(&bytes).is_err());
        // A section table longer than the buffer.
        let mut bytes = image(true, 1);
        bytes[0x86..0x88].copy_from_slice(&0xFFFFu16.to_le_bytes());
        assert!(PeHeaders::parse(&bytes).is_err());
        // Too short to hold a DOS header.
        assert!(PeHeaders::parse(b"MZ").is_err());
        assert!(PeHeaders::parse(&[]).is_err());
    }

    #[test]
    fn tables_read_short_are_an_error_not_a_shorter_table() {
        let bytes = [1u8, 0, 0, 0, 2, 0, 0, 0, 3, 0];
        assert_eq!(u32_table(&bytes, 2).unwrap(), [1, 2]);
        assert!(
            u32_table(&bytes, 3).is_err(),
            "ten bytes are not three words"
        );
        assert_eq!(u16_table(&bytes, 5).unwrap(), [1, 0, 2, 0, 3]);
        assert!(u16_table(&bytes, 6).is_err());
        assert!(u32_table(&bytes, usize::MAX).is_err(), "no overflow");
        // Read at an odd offset: no alignment is assumed.
        assert_eq!(
            u32_table(&bytes[1..], 2).unwrap(),
            [0x0200_0000, 0x0300_0000]
        );
    }

    #[test]
    fn export_slots_join_the_three_tables() {
        // Three functions, the second empty; two names, one out of range.
        let slots = export_slots(10, &[0x1000, 0, 0x3000], &[2, 7]);
        assert_eq!(
            slots,
            [
                ExportSlot {
                    ordinal: 10,
                    rva: 0x1000,
                    name_index: None
                },
                ExportSlot {
                    ordinal: 12,
                    rva: 0x3000,
                    name_index: Some(0)
                },
            ]
        );
    }

    fn imports() -> Vec<ImportSpec> {
        vec![
            ImportSpec {
                module: "kernel32.dll".into(),
                function: Some("ExitProcess".into()),
                ordinal: 0,
            },
            ImportSpec {
                module: "ws2_32.dll".into(),
                function: None,
                ordinal: 23,
            },
            ImportSpec {
                module: "kernel32.dll".into(),
                function: Some("Sleep".into()),
                ordinal: 0,
            },
        ]
    }

    /// The bytes at `rva` in an image whose new section was appended by
    /// `append_import_section`.
    fn at_rva(image: &[u8], headers: &PeHeaders, rva: u32) -> &[u8] {
        let last = headers
            .section_offset(headers.number_of_sections as usize - 1)
            .unwrap();
        let va = read_u32(image, last + 12).unwrap();
        let raw = read_u32(image, last + 20).unwrap();
        &image[(raw + rva - va) as usize..]
    }

    fn c_string(bytes: &[u8]) -> &str {
        let end = bytes.iter().position(|b| *b == 0).unwrap();
        std::str::from_utf8(&bytes[..end]).unwrap()
    }

    #[test]
    fn an_import_section_is_appended_and_reads_back() {
        for pe32_plus in [true, false] {
            let mut bytes = image(pe32_plus, 1);
            append_import_section(&mut bytes, &imports()).unwrap();
            let headers = PeHeaders::parse(&bytes).unwrap();
            assert_eq!(headers.number_of_sections, 2);
            let new = headers.section_offset(1).unwrap();
            assert_eq!(&bytes[new..new + 8], b".fission");
            let rva = read_u32(&bytes, new + 12).unwrap();
            assert_eq!(rva, 0x2000, "after the first section, section-aligned");
            let raw = read_u32(&bytes, new + 20).unwrap();
            assert_eq!(raw % 0x200, 0, "file-aligned");

            let dir = headers
                .data_directory_offset(DATA_DIRECTORY_IMPORT)
                .unwrap();
            assert_eq!(read_u32(&bytes, dir).unwrap(), rva);
            assert_eq!(read_u32(&bytes, dir + 4).unwrap(), 3 * 20);

            // Two descriptors, sorted by module, then a null one.
            let descriptors = at_rva(&bytes, &headers, rva);
            let thunk = if pe32_plus { 8 } else { 4 };
            let mut seen = Vec::new();
            for index in 0..2 {
                let d = &descriptors[index * 20..];
                let name = c_string(at_rva(&bytes, &headers, read_u32(d, 12).unwrap()));
                let table = at_rva(&bytes, &headers, read_u32(d, 16).unwrap());
                let mut functions = Vec::new();
                for slot in 0.. {
                    let value = if pe32_plus {
                        read_u64(table, slot * thunk).unwrap()
                    } else {
                        u64::from(read_u32(table, slot * thunk).unwrap())
                    };
                    if value == 0 {
                        break;
                    }
                    let flag = if pe32_plus { 1u64 << 63 } else { 1 << 31 };
                    if value & flag != 0 {
                        functions.push(format!("#{}", value & 0xffff));
                    } else {
                        let entry = at_rva(&bytes, &headers, value as u32);
                        functions.push(c_string(&entry[2..]).to_string());
                    }
                }
                seen.push((name.to_string(), functions));
            }
            assert_eq!(
                seen,
                [
                    (
                        "kernel32.dll".to_string(),
                        vec!["ExitProcess".to_string(), "Sleep".to_string()]
                    ),
                    ("ws2_32.dll".to_string(), vec!["#23".to_string()]),
                ],
                "pe32_plus = {pe32_plus}"
            );
            assert!(
                descriptors[40..60].iter().all(|b| *b == 0),
                "null terminator"
            );
        }
    }

    #[test]
    fn an_image_that_cannot_take_a_section_is_left_alone() {
        // No sections: `num_sections - 1` underflowed.
        let mut bytes = image(true, 0);
        let before = bytes.clone();
        assert!(append_import_section(&mut bytes, &imports()).is_err());
        assert_eq!(bytes, before);

        // No room for another section header before the first section.
        let mut bytes = image(true, 1);
        let size_of_headers = 0x98 + 60;
        bytes[size_of_headers..size_of_headers + 4]
            .copy_from_slice(&(0x98u32 + 0xF0 + 40).to_le_bytes());
        let before = bytes.clone();
        assert!(append_import_section(&mut bytes, &imports()).is_err());
        assert_eq!(bytes, before);

        // An alignment that is not a power of two.
        let mut bytes = image(true, 1);
        bytes[0x98 + 36..0x98 + 40].copy_from_slice(&0x300u32.to_le_bytes());
        let before = bytes.clone();
        assert!(append_import_section(&mut bytes, &imports()).is_err());
        assert_eq!(bytes, before);

        // Not a PE file at all, and a truncated one.
        let mut bytes = vec![0u8; 64];
        assert!(append_import_section(&mut bytes, &imports()).is_err());
        let mut bytes = image(true, 1)[..0x100].to_vec();
        assert!(append_import_section(&mut bytes, &imports()).is_err());
    }

    #[test]
    fn the_seh_chain_starts_at_the_pointer_stored_at_the_teb() {
        // TEB at 0x1000 holds ExceptionList = 0x5000; two records, then the
        // end marker.
        let memory: std::collections::HashMap<u64, u64> = [
            (0x1000, 0x5000),
            (0x1008, 0x7fff_0000), // NtTib.StackBase, not a handler
            (0x5000, 0x5100),
            (0x5008, 0x4010_00),
            (0x5100, u64::MAX),
            (0x5108, 0x4020_00),
        ]
        .into_iter()
        .collect();
        let read = |address: u64| memory.get(&address).copied();

        let head = read(0x1000).unwrap();
        let chain = walk_seh_chain(head, 8, 64, read);
        assert_eq!(
            chain.iter().map(|l| l.handler).collect::<Vec<_>>(),
            [0x4010_00, 0x4020_00]
        );
        // Walking from the TEB itself, as before, reads the TEB's first two
        // fields as a record: StackBase as a handler.
        let wrong = walk_seh_chain(0x1000, 8, 64, read);
        assert_eq!(wrong[0].handler, 0x7fff_0000);
    }

    #[test]
    fn an_seh_chain_ends_at_either_marker_a_cycle_or_an_unreadable_record() {
        let read32 = |address: u64| match address {
            0x100 => Some(0xFFFF_FFFF),
            0x104 => Some(0x401000),
            _ => None,
        };
        assert_eq!(walk_seh_chain(0x100, 4, 64, read32).len(), 1);
        // A record pointing at itself.
        let cyclic = |address: u64| match address {
            0x200 => Some(0x200),
            0x208 => Some(0x401000),
            _ => None,
        };
        assert_eq!(walk_seh_chain(0x200, 8, 64, cyclic).len(), 1);
        // Unreadable, and empty.
        assert!(walk_seh_chain(0x300, 8, 64, |_| None).is_empty());
        assert!(walk_seh_chain(0, 8, 64, |_| Some(1)).is_empty());
    }

    #[test]
    fn the_peb_and_heap_offsets_are_the_documented_x64_ones() {
        // `ImageBaseAddress` follows the 8-byte `Mutant`, and `ProcessHeap`
        // is a heap, not the `ProcessHeaps` array.
        assert_eq!(peb64::IMAGE_BASE_ADDRESS, 0x10);
        assert_eq!(peb64::PROCESS_HEAP, 0x30);
        assert_eq!(heap64::FLAGS + 4, heap64::FORCE_FLAGS);
    }
}
