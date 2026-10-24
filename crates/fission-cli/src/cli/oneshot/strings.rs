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
    byte_len: usize,
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
    pub context: bool,
    pub contains: Option<String>,
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
    if let Some(contains) = &view.contains {
        strings.retain(|found| found.text.contains(contains));
    }
    if view.context {
        return print_context(&mut stdout, binary, data, &strings, json);
    }
    if with_xrefs {
        attach_referrers(binary, &mut strings);
        // A referenced string is a lead; the rest are the haystack it was
        // buried in. A 2.5MB Go binary yields 15,844 strings and 330 of them
        // are read by code, and in address order those 330 are unfindable.
        strings.sort_by_key(|found| (found.referrers.is_empty(), found.address, found.file_offset));
    } else {
        strings.sort_by_key(|found| (found.address.is_none(), found.address, found.file_offset));
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
    // The "Referenced by" column is only filled when `--xrefs` asked for the
    // disassembly pass. Printing an empty column without it reads as "nothing
    // references this string", which is a different claim from "not looked
    // for" -- and it cost an agent a wrong conclusion in the crackme pilot.
    if with_xrefs {
        writeln!(
            stdout,
            "{:>18}  {:<10}  {:<28}  String",
            "Address", "Section", "Referenced by"
        )?;
    } else {
        writeln!(stdout, "{:>18}  {:<10}  String", "Address", "Section")?;
    }
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
        if with_xrefs {
            writeln!(
                stdout,
                "{address:>18}  {section:<10}  {referrers:<28}  {text}"
            )?;
        } else {
            writeln!(stdout, "{address:>18}  {section:<10}  {text}")?;
        }
    }
    if !with_xrefs {
        writeln!(
            stdout,
            "hint: --xrefs also reports which code references each string"
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

/// A context is an explicit new envelope; legacy string arrays stay intact.
fn print_context(
    out: &mut impl Write,
    binary: &LoadedBinary,
    _data: &[u8],
    strings: &[FoundString],
    json_out: bool,
) -> io::Result<()> {
    use fission_static::analysis::string_xrefs::string_reference_context;
    use fission_static::analysis::{CallGraph, build_xref_index};
    let index = build_xref_index(binary, true);
    let graph = CallGraph::build_from_xref_index(&binary.functions, &index, 0x40);
    let mut contexts: Vec<_> = strings
        .iter()
        .map(|found| {
            let context = found.address.filter(|_| !found.executable).map(|address| {
                string_reference_context(binary, &index, &graph, address, found.byte_len as u64)
            });
            (found, context)
        })
        .collect();
    contexts.sort_by_key(|(found, context)| {
        (
            context
                .as_ref()
                .is_none_or(|context| context.references.is_empty()),
            found.address,
            found.file_offset,
        )
    });
    let note = |found: &FoundString| {
        if found.executable {
            "printable bytes in executable storage are not treated as a string lead"
        } else {
            "file bytes have no mapped address"
        }
    };
    if json_out {
        let rows: Vec<_> =
            contexts
                .iter()
                .map(|(found, context)| {
                    let context_json = context.as_ref().map(|context| {
                let functions: Vec<_> = context.functions.iter().map(|function| serde_json::json!({
                    "address": format!("0x{:x}", function.function.address),
                    "name": function.function.name,
                    "reference_ids": function.reference_ids,
                    "callers": relation_rows(binary, function.callers),
                    "callees": relation_rows(binary, function.callees),
                })).collect();
                serde_json::json!({"references": context.references, "functions": functions})
            });
                    let mut row = serde_json::json!({
                        "file_offset": format!("0x{:x}", found.file_offset),
                        "string": found.text,
                        "address": found.address.map(|address| format!("0x{address:x}")),
                        "section": found.section,
                        "context": context_json,
                    });
                    if context.is_none() {
                        row["context_note"] = serde_json::json!(note(found));
                    }
                    row
                })
                .collect();
        return writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "analysis": index.analysis, "strings": rows,
            }))?
        );
    }
    writeln!(
        out,
        "string_context: {} string(s); callers/callees are one-hop indexed relationships",
        contexts.len()
    )?;
    writeln!(
        out,
        "xref_analysis: {}",
        serde_json::to_string(&index.analysis)?
    )?;
    for (found, context) in contexts {
        let address = found
            .address
            .map(|address| format!("0x{address:x}"))
            .unwrap_or_else(|| "unmapped".into());
        writeln!(out, "{address}  {:?}", found.text)?;
        let Some(context) = context else {
            writeln!(out, "  {}", note(found))?;
            continue;
        };
        writeln!(out, "  {} indexed reference(s)", context.references.len())?;
        for reference in context.references {
            writeln!(
                out,
                "    ref #{}: 0x{:x} -> {}  {:?}  {:?}",
                reference.id,
                reference.source.address,
                reference
                    .target
                    .address
                    .map(|address| format!("0x{address:x}"))
                    .unwrap_or_else(|| "?".into()),
                reference.kind,
                reference.evidence.layer
            )?;
        }
        for function in context.functions {
            writeln!(
                out,
                "  function 0x{:x}  {}",
                function.function.address, function.function.name
            )?;
            for (relation, edges) in [("callers", function.callers), ("callees", function.callees)]
            {
                writeln!(
                    out,
                    "    {relation}: {} indexed relationship(s)",
                    edges.len()
                )?;
                for edge in edges {
                    let name = binary
                        .function_at_exact(edge.addr)
                        .map(|function| function.name.as_str())
                        .unwrap_or("-");
                    writeln!(out, "      0x{:x}  {name}  x{}", edge.addr, edge.count)?;
                }
            }
        }
    }
    Ok(())
}

fn relation_rows(
    binary: &LoadedBinary,
    edges: &[fission_static::analysis::CallEdge],
) -> Vec<serde_json::Value> {
    edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "address": format!("0x{:x}", edge.addr),
                "name": binary.function_at_exact(edge.addr).map(|function| function.name.as_str()),
                "count": edge.count,
            })
        })
        .collect()
}

/// Printable ASCII and UTF-16LE runs of at least `min_len` characters.
fn scan(data: &[u8], min_len: usize) -> Vec<FoundString> {
    use fission_static::analysis::strings::extract_strings;
    let extracted = extract_strings(data, 0, min_len);
    extracted
        .into_iter()
        .map(|s| FoundString {
            file_offset: s.address as usize,
            byte_len: s.length,
            text: s.content,
            address: None,
            section: None,
            executable: false,
            referrers: Vec::new(),
        })
        .collect()
}

/// Map each string's file offset to the section that loads it, and its address.
fn locate(binary: &LoadedBinary, strings: &mut [FoundString]) {
    let candidate_sections: Vec<&SectionInfo> = binary
        .sections
        .iter()
        .filter(|section| {
            section.file_size > 0 && (section.is_addressable() || section.virtual_address != 0)
        })
        .collect();

    for found in strings.iter_mut() {
        let offset = found.file_offset as u64;
        let best_section = candidate_sections
            .iter()
            .copied()
            .filter(|sec| {
                offset >= sec.file_offset && offset < sec.file_offset.saturating_add(sec.file_size)
            })
            .max_by_key(|sec| {
                (
                    sec.is_addressable(),
                    sec.virtual_address != 0,
                    std::cmp::Reverse(sec.file_size),
                )
            });

        if let Some(section) = best_section {
            found.section = Some(section.name.clone());
            found.executable = section.is_executable;
            found.address = Some(section.virtual_address + (offset - section.file_offset));
        }
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
        for offset in 0..found.byte_len as u64 {
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

    #[test]
    fn utf16le_strings_are_extracted_and_located() {
        let mut data = vec![0u8; 0x400];
        // "UnicodeString" in UTF-16LE
        let wide: Vec<u8> = "UnicodeString"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        data[0x200..0x200 + wide.len()].copy_from_slice(&wide);

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

        let mut strings = scan(&data, 4);
        locate(&binary, &mut strings);
        let found = strings
            .iter()
            .find(|s| s.text == "UnicodeString")
            .expect("UTF-16 string must be extracted");
        assert_eq!(found.address, Some(0x140004000));
        assert_eq!(found.section.as_deref(), Some(".rdata"));
        assert_eq!(found.byte_len, wide.len());
    }

    #[test]
    fn locate_prefers_addressable_section_over_unmapped_overlapping_section() {
        let mut data = vec![0u8; 0x400];
        data[0x200..0x20c].copy_from_slice(b"targetstring");

        let binary =
            LoadedBinaryBuilder::new("test.bin".to_string(), DataBuffer::Heap(data.clone()))
                .image_base(0x140000000)
                .entry_point(0x140001000)
                .is_64bit(true)
                .format("PE")
                // Unmapped overlapping section (e.g. debug/metadata header sharing file offset 0..0x300)
                .add_section(SectionInfo {
                    name: "unmapped_meta".to_string(),
                    virtual_address: 0x0,
                    virtual_size: 0,
                    file_offset: 0x100,
                    file_size: 0x200,
                    is_executable: false,
                    is_writable: false,
                    is_readable: false,
                })
                // Valid loaded section covering 0x200..0x400
                .add_section(SectionInfo {
                    name: ".data".to_string(),
                    virtual_address: 0x140008000,
                    virtual_size: 0x200,
                    file_offset: 0x200,
                    file_size: 0x200,
                    is_executable: false,
                    is_writable: true,
                    is_readable: true,
                })
                .build()
                .expect("build test binary");

        let mut strings = scan(&data, 4);
        locate(&binary, &mut strings);
        let found = strings
            .iter()
            .find(|s| s.text == "targetstring")
            .expect("string found");
        assert_eq!(
            found.address,
            Some(0x140008000),
            "must match loaded section address, not 0x0 + offset"
        );
        assert_eq!(found.section.as_deref(), Some(".data"));
    }
}
