//! Extracted strings, addressed the way the rest of the tool addresses things.
//!
//! A string is a lead: you read one that looks interesting and then ask who
//! uses it. That was not possible here. This listed a file offset and the text
//! and nothing else, while `list`, `disasm`, `xrefs` and `decomp` all speak
//! virtual addresses -- so a string could not be carried to any other command,
//! and the most common move in reverse engineering had no starting point.
//!
//! The address and the section come from the loader and cost nothing. The
//! referring function costs a disassembly pass, so it is behind `--xrefs`.

use fission_loader::loader::{LoadedBinary, SectionInfo};
use std::collections::HashMap;
use std::io::{self, Write};

/// One extracted string with whatever the loader can say about where it lives.
struct FoundString {
    file_offset: usize,
    text: String,
    /// The mapped address, when the offset falls inside a section's file image.
    ///
    /// Bytes before the first section -- the PE header, where the section
    /// *names* themselves are stored -- are never loaded at an address, which
    /// is exactly what distinguishes `.rdata` the string from `.rdata` the
    /// section.
    address: Option<u64>,
    section: Option<String>,
    /// Whether the section holding it is executable.
    executable: bool,
    /// Function entry points whose code reads this string.
    referrers: Vec<u64>,
}

/// How the caller wants the listing narrowed.
pub(super) struct StringsView {
    /// Report the functions that read each string (runs disassembly).
    pub with_xrefs: bool,
    /// Keep only strings in these sections, when non-empty.
    pub sections: Vec<String>,
}

pub(super) fn print_strings(
    binary: &LoadedBinary,
    data: &[u8],
    min_len: usize,
    view: &StringsView,
    json: bool,
) -> io::Result<()> {
    let with_xrefs = view.with_xrefs;
    let mut stdout = io::stdout().lock();
    let mut strings = scan(data, min_len);
    locate(binary, &mut strings);
    if !view.sections.is_empty() {
        strings.retain(|found| {
            found
                .section
                .as_deref()
                .is_some_and(|name| view.sections.iter().any(|wanted| wanted == name))
        });
    }
    if with_xrefs {
        attach_referrers(binary, &mut strings);
        // A referenced string is a lead; the rest are the haystack it was
        // buried in. A 2.5MB Go binary yields 15,844 strings and 330 of them
        // are read by code, and in address order those 330 are unfindable.
        strings.sort_by_key(|found| (found.referrers.is_empty(), found.address, found.file_offset));
    }

    if json {
        let rows: Vec<serde_json::Value> = strings
            .iter()
            .map(|found| {
                let mut row = serde_json::json!({
                    "file_offset": format!("0x{:x}", found.file_offset),
                    "string": found.text,
                });
                if let Some(address) = found.address {
                    row["address"] = serde_json::json!(format!("0x{address:x}"));
                }
                if let Some(section) = &found.section {
                    row["section"] = serde_json::json!(section);
                }
                if with_xrefs {
                    row["referrers"] = serde_json::json!(
                        found
                            .referrers
                            .iter()
                            .map(|addr| serde_json::json!({
                                "address": format!("0x{addr:x}"),
                                "name": referrer_label(binary, *addr),
                            }))
                            .collect::<Vec<_>>()
                    );
                }
                row
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&rows).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
        return Ok(());
    }

    let referenced = strings.iter().filter(|s| !s.referrers.is_empty()).count();
    if with_xrefs {
        writeln!(
            stdout,
            "Strings ({} found, min length {}; {} referenced by code):",
            strings.len(),
            min_len,
            referenced
        )?;
    } else {
        writeln!(
            stdout,
            "Strings ({} found, min length {}):",
            strings.len(),
            min_len
        )?;
    }
    writeln!(
        stdout,
        "{:>18}  {:<10}  {:<28}  String",
        "Address", "Section", "Referenced by"
    )?;
    writeln!(stdout, "{:─<100}", "")?;
    for found in &strings {
        let address = match found.address {
            Some(address) => format!("0x{address:012x}"),
            // Nothing maps it, so there is no address to give; the file offset
            // is all there is and the column says so rather than inventing one.
            None => format!("@{:<9x}", found.file_offset),
        };
        let section = found.section.as_deref().unwrap_or("-");
        let referrers = if found.referrers.is_empty() {
            String::new()
        } else {
            let shown = found
                .referrers
                .iter()
                .take(2)
                .map(|addr| referrer_label(binary, *addr))
                .collect::<Vec<_>>()
                .join(" ");
            if found.referrers.len() > 2 {
                format!("{shown} +{}", found.referrers.len() - 2)
            } else {
                shown
            }
        };
        let text = if found.text.len() > 60 {
            format!("{}...", &found.text[..57])
        } else {
            found.text.clone()
        };
        writeln!(
            stdout,
            "{address:>18}  {section:<10}  {referrers:<28}  {text}"
        )?;
    }
    Ok(())
}

/// A referring function's name, or the address when there is no name for it.
///
/// The xref record often has no enclosing function -- on a Go binary most
/// data references do not -- and the instruction address stands in. The loader
/// can usually still say which function contains it, and a name is what makes
/// the column readable, so ask before falling back to hex.
fn referrer_label(binary: &LoadedBinary, address: u64) -> String {
    match binary.function_at(address) {
        Some(function) if !function.name.is_empty() => function.name.clone(),
        _ => format!("0x{address:x}"),
    }
}

/// Printable ASCII runs of at least `min_len` bytes.
fn scan(data: &[u8], min_len: usize) -> Vec<FoundString> {
    let mut strings = Vec::with_capacity((data.len() / 1024).max(100));
    let mut current: Vec<u8> = Vec::with_capacity(256);
    let mut start = 0usize;

    let flush = |current: &mut Vec<u8>, start: usize, strings: &mut Vec<FoundString>| {
        if current.len() >= min_len {
            // SAFETY: only bytes in 0x20..0x7f were pushed, all valid UTF-8.
            let text = unsafe { String::from_utf8_unchecked(std::mem::take(current)) };
            strings.push(FoundString {
                file_offset: start,
                text,
                address: None,
                section: None,
                executable: false,
                referrers: Vec::new(),
            });
        }
        current.clear();
    };

    for (i, &byte) in data.iter().enumerate() {
        if (0x20..0x7f).contains(&byte) {
            if current.is_empty() {
                start = i;
            }
            current.push(byte);
        } else {
            flush(&mut current, start, &mut strings);
        }
    }
    flush(&mut current, start, &mut strings);
    strings
}

/// Map each string's file offset to the section that loads it, and its address.
fn locate(binary: &LoadedBinary, strings: &mut [FoundString]) {
    let mut sections: Vec<&SectionInfo> = binary
        .sections
        .iter()
        .filter(|section| section.file_size > 0)
        .collect();
    sections.sort_by_key(|section| section.file_offset);

    for found in strings.iter_mut() {
        let offset = found.file_offset as u64;
        let Ok(index) = sections.binary_search_by(|section| {
            if offset < section.file_offset {
                std::cmp::Ordering::Greater
            } else if offset >= section.file_offset + section.file_size {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        }) else {
            continue;
        };
        let section = sections[index];
        found.section = Some(section.name.clone());
        found.executable = section.is_executable;
        found.address = Some(section.virtual_address + (offset - section.file_offset));
    }
}

/// Attach the entry points of functions whose code reads each string.
///
/// The xref index already resolves data references to their targets; what it
/// does not do is relate them to string extents, because it records a string
/// literal as a reference to itself. Joining the two is the whole feature:
/// on the measurement binary it puts a referring function on 16 of 52 strings.
fn attach_referrers(binary: &LoadedBinary, strings: &mut [FoundString]) {
    use fission_static::analysis::build_xref_index;
    use fission_static::analysis::xref_index::{XrefKind, XrefSourceCategory};

    // Address -> index, for every byte a string covers, so a reference into
    // the middle of one (a suffix pointer, or a `+ 4` past a prefix) still
    // finds it.
    let mut by_address: HashMap<u64, usize> = HashMap::new();
    for (index, found) in strings.iter().enumerate() {
        let Some(address) = found.address else {
            continue;
        };
        // A printable run inside code is a coincidence -- `AWAVAUATUWVSH` is
        // the x86-64 prologue pushing the callee-saved registers -- and a
        // reference to it is a pointer to the function, not a string read.
        // Left in the listing, kept out of the leads.
        if found.executable {
            continue;
        }
        for offset in 0..=found.text.len() as u64 {
            by_address.entry(address + offset).or_insert(index);
        }
    }

    let index = build_xref_index(binary, true);
    for record in &index.refs {
        // Only reads of the bytes count. A `call` whose target happens to
        // begin with printable bytes -- `AWAVAUATUWVSH`, an x86-64 prologue
        // pushing the callee-saved registers -- is control flow reaching a
        // function, not code reading a string.
        if !matches!(
            record.kind,
            XrefKind::DataRead | XrefKind::DataWrite | XrefKind::Relocation
        ) {
            continue;
        }
        let Some(target) = record.target.address else {
            continue;
        };
        let Some(&string_index) = by_address.get(&target) else {
            continue;
        };
        let XrefSourceCategory::Instruction { enclosing_function } = record.source.category else {
            continue;
        };
        // Without an enclosing function the reference is still real, but there
        // is no name to hand back, so the instruction address stands in.
        let referrer = enclosing_function.unwrap_or(record.source.address);
        let referrers = &mut strings[string_index].referrers;
        if !referrers.contains(&referrer) {
            referrers.push(referrer);
        }
    }
    for found in strings.iter_mut() {
        found.referrers.sort_unstable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    /// A PE whose header holds the section names and whose `.rdata` holds one
    /// real string -- the shape that made the old listing unusable, since both
    /// came back as bare file offsets with nothing to tell them apart.
    fn binary_with_header_and_rdata() -> (LoadedBinary, Vec<u8>) {
        let mut data = vec![0u8; 0x400];
        data[0x100..0x106].copy_from_slice(b".rdata");
        data[0x200..0x211].copy_from_slice(b"realworld-fixture");
        let binary =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(data.clone()))
                .image_base(0x140000000)
                .entry_point(0x140001000)
                .is_64bit(true)
                .format("PE")
                .add_section(SectionInfo {
                    name: ".rdata".to_string(),
                    virtual_address: 0x140004000,
                    virtual_size: 0x200,
                    file_offset: 0x200,
                    file_size: 0x200,
                    is_executable: false,
                    is_writable: false,
                    is_readable: true,
                })
                .build()
                .expect("build test binary");
        (binary, data)
    }

    #[test]
    fn a_string_in_a_section_carries_the_address_it_loads_at() {
        let (binary, data) = binary_with_header_and_rdata();
        let mut strings = scan(&data, 6);
        locate(&binary, &mut strings);
        let found = strings
            .iter()
            .find(|s| s.text == "realworld-fixture")
            .expect("string scanned");
        // File offset 0x200 is the section's first byte, so it loads at the
        // section's own address -- the one every other command speaks.
        assert_eq!(found.address, Some(0x140004000));
        assert_eq!(found.section.as_deref(), Some(".rdata"));
    }

    #[test]
    fn a_printable_run_inside_code_is_marked_as_such() {
        let mut data = vec![0u8; 0x400];
        // The x86-64 prologue that pushes the callee-saved registers reads as
        // `AWAVAUATUWVSH`, and it is what put two code addresses at the top of
        // the lead list until references to executable storage stopped
        // counting.
        data[0x000..0x00d].copy_from_slice(b"AWAVAUATUWVSH");
        let binary =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(data.clone()))
                .image_base(0x140000000)
                .entry_point(0x140001000)
                .is_64bit(true)
                .format("PE")
                .add_section(SectionInfo {
                    name: ".text".to_string(),
                    virtual_address: 0x140001000,
                    virtual_size: 0x100,
                    file_offset: 0,
                    file_size: 0x100,
                    is_executable: true,
                    is_writable: false,
                    is_readable: true,
                })
                .build()
                .expect("build test binary");
        let mut strings = scan(&data, 6);
        locate(&binary, &mut strings);
        let found = strings
            .iter()
            .find(|s| s.text == "AWAVAUATUWVSH")
            .expect("string scanned");
        assert!(found.executable, "a run in .text must be marked executable");
    }

    #[test]
    fn a_string_in_the_header_has_no_address_to_give() {
        let (binary, data) = binary_with_header_and_rdata();
        let mut strings = scan(&data, 6);
        locate(&binary, &mut strings);
        let found = strings
            .iter()
            .find(|s| s.text == ".rdata")
            .expect("string scanned");
        // Nothing maps the header, so there is no address. Inventing one from
        // the image base would put a section *name* at a plausible-looking
        // address the loader never reads.
        assert_eq!(found.address, None);
        assert_eq!(found.section, None);
    }
}
