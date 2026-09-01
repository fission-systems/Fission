use fission_loader::detector;
use fission_loader::loader::function_view::{
    canonical_exports_sorted, canonical_imports_sorted, canonical_view_counts,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::build_xref_index;
use serde_json::Value;
use std::io::{self, Write};

pub(super) fn print_binary_info(
    binary: &LoadedBinary,
    json: bool,
    include_detections: bool,
    include_identity: bool,
    include_xrefs: bool,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let (arch_json, bits) = binary
        .architecture
        .as_ref()
        .map(|arch| {
            (
                match arch.processor.as_str() {
                    "AARCH64" => "arm64".to_string(),
                    "ARM" => "arm".to_string(),
                    "x86" if arch.bitness == 64 => "x86_64".to_string(),
                    "x86" => "x86".to_string(),
                    other => other.to_ascii_lowercase(),
                },
                arch.bitness,
            )
        })
        .unwrap_or_else(|| ("unknown".to_string(), if binary.is_64bit { 64 } else { 32 }));

    if json {
        let counts = canonical_view_counts(binary);
        let mut payload = serde_json::json!({
            "path": binary.path,
            "format": binary.format,
            "arch": arch_json,
            "bits": bits,
            "entry": format!("0x{:x}", binary.entry_point),
            "image_base": format!("0x{:x}", binary.image_base),
            "sections": binary.sections.len(),
            "functions": counts.functions,
            "imports": counts.imports,
            "exports": counts.exports,
        });
        if include_detections {
            let dr = detector::detect(binary);
            let detections: Vec<Value> = dr
                .detections
                .iter()
                .map(|d| {
                    serde_json::json!({
                        "detection_type": d.detection_type.to_string(),
                        "name": &d.name,
                        "version": &d.version,
                        "details": &d.details,
                        "confidence": d.confidence.to_string(),
                    })
                })
                .collect();
            if let Value::Object(ref mut map) = payload {
                map.insert("detections".to_string(), Value::Array(detections));
            }
        }
        if include_identity {
            if let Some(ref rep) = binary.identity_report {
                let id_json = serde_json::to_value(rep).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("identity JSON serialization failed: {e}"),
                    )
                })?;
                if let Value::Object(ref mut map) = payload {
                    map.insert("identity".to_string(), id_json);
                }
            }
        }
        if include_xrefs {
            let idx = build_xref_index(binary, true);
            let summary = idx.summary();
            if let Value::Object(ref mut map) = payload {
                map.insert(
                    "xrefs".to_string(),
                    serde_json::json!({ "summary": summary }),
                );
            }
        }
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
    } else {
        writeln!(
            stdout,
            "\x1b[1;36m╔══════════════════════════════════════════════════════════╗\x1b[0m"
        )?;
        writeln!(
            stdout,
            "\x1b[1;36m║\x1b[0m          \x1b[1;35m📊 BINARY INFORMATION\x1b[0m                    \x1b[1;36m║\x1b[0m"
        )?;
        writeln!(
            stdout,
            "\x1b[1;36m╠══════════════════════════════════════════════════════════╣\x1b[0m"
        )?;
        writeln!(stdout, "║ Path:       {} ║", fit(&binary.path, 46))?;
        writeln!(stdout, "║ Format:     {} ║", fit(&binary.format, 46))?;

        let arch_display = binary
            .architecture
            .as_ref()
            .map(|arch| format!("{} {}-bit ({})", arch.processor, arch.bitness, arch.variant))
            .unwrap_or_else(|| "unknown".to_string());

        writeln!(stdout, "║ Arch:       {} ║", fit(&arch_display, 46))?;

        // What every later command actually decodes and decompiles with.
        //
        // The tool has three answers to "what compiled this": `--detections`
        // says `GCC 15.2.0`, `--identity` says `MinGW`, and the pipeline picks
        // a Ghidra compiler spec by yet another route. Only the last one
        // changes any output, and it was the one nothing printed -- an
        // operator seeing a wrong calling convention had no way to check what
        // convention was chosen.
        let language_display = match (binary.sleigh_language_id(), binary.get_ghidra_compiler_id())
        {
            (Some(language), Some(compiler)) => format!("{language}  /  {compiler}"),
            (Some(language), None) => language.to_string(),
            (None, Some(compiler)) => format!("(no language)  /  {compiler}"),
            (None, None) => "unresolved".to_string(),
        };
        writeln!(
            stdout,
            "║ Language:   {:<46} ║",
            truncate(&language_display, 46)
        )?;
        writeln!(
            stdout,
            "║ Entry:      {:<46} ║",
            format!("0x{:x}", binary.entry_point)
        )?;
        writeln!(
            stdout,
            "║ Image Base: {:<46} ║",
            format!("0x{:x}", binary.image_base)
        )?;
        writeln!(
            stdout,
            "╠══════════════════════════════════════════════════════════╣"
        )?;
        writeln!(
            stdout,
            "║ Sections:   {:<10} Functions: {:<10} IAT: {:<7} ║",
            binary.sections.len(),
            canonical_view_counts(binary).functions,
            binary.iat_symbols.len()
        )?;
        writeln!(
            stdout,
            "║ Imports:    {:<10} Exports:   {:<24} ║",
            canonical_view_counts(binary).imports,
            canonical_view_counts(binary).exports
        )?;
        writeln!(
            stdout,
            "\x1b[1;36m╚══════════════════════════════════════════════════════════╝\x1b[0m"
        )?;

        if include_detections {
            let dr = detector::detect(binary);
            writeln!(stdout)?;
            writeln!(
                stdout,
                "\x1b[1;36m──────────────────────────────────────────────────────────\x1b[0m"
            )?;
            writeln!(stdout, "\x1b[1;35mDetections\x1b[0m (rules + DiE)")?;
            if dr.detections.is_empty() {
                writeln!(stdout, "  (none)")?;
            } else {
                for d in &dr.detections {
                    writeln!(stdout, "  {}", d.display())?;
                    if let Some(ref details) = d.details {
                        writeln!(stdout, "    {}", truncate(details, 72))?;
                    }
                }
            }
        }

        if include_identity {
            if let Some(ref rep) = binary.identity_report {
                writeln!(stdout)?;
                writeln!(
                    stdout,
                    "\x1b[1;36m──────────────────────────────────────────────────────────\x1b[0m"
                )?;
                writeln!(
                    stdout,
                    "\x1b[1;35mIdentity\x1b[0m (loader provenance / hints)"
                )?;
                let s = &rep.summary;
                writeln!(
                    stdout,
                    "  packed_score={:.2} overlay={} high_entropy_exec_sections={} aggregate_confidence={}",
                    s.packed_score, s.has_overlay, s.high_entropy_executable_sections, s.confidence
                )?;
                if let Some(ref c) = s.likely_compiler {
                    writeln!(stdout, "  likely_compiler: {c}")?;
                }
                if let Some(ref l) = s.likely_language {
                    writeln!(stdout, "  likely_language: {l}")?;
                }
                if let Some(ref p) = s.likely_packer {
                    writeln!(stdout, "  likely_packer: {p}")?;
                }
                if let Some(ref r) = rep.resources {
                    writeln!(
                        stdout,
                        "  identity.resources: die_pe_json={} patterns={} win_api_txt={} fid_bf_count={:?}",
                        r.die_pe_json_present,
                        r.pattern_json_count.unwrap_or(0),
                        r.win_typeinfo_present,
                        r.fid_bf_count
                    )?;
                }
                if let Some(ref dc) = rep.die_compat {
                    writeln!(
                        stdout,
                        "  identity.die_compat: rules {}/{} supported, sigs matched {}",
                        dc.rules_supported, dc.rules_seen, dc.signatures_matched
                    )?;
                }
                if let Some(ref wc) = rep.winapi_catalog {
                    writeln!(
                        stdout,
                        "  identity.winapi_catalog: IAT symbols {} catalog hits {} misses {}",
                        wc.symbols_considered, wc.symbols_in_catalog, wc.symbols_not_in_catalog
                    )?;
                }
                writeln!(
                    stdout,
                    "  detections={} (see --identity --json for evidence)",
                    rep.detections.len()
                )?;
            }
        }

        if include_xrefs {
            let idx = build_xref_index(binary, true);
            let sum = idx.summary();
            writeln!(stdout)?;
            writeln!(
                stdout,
                "\x1b[1;36m──────────────────────────────────────────────────────────\x1b[0m"
            )?;
            writeln!(stdout, "\x1b[1;35mXrefs\x1b[0m (canonical index)")?;
            writeln!(
                stdout,
                "  total={} calls={} jumps={} data={} imports={} exports={} strings={} globals={} relocations={}",
                sum.total,
                sum.calls,
                sum.jumps,
                sum.data,
                sum.imports,
                sum.exports,
                sum.strings,
                sum.globals,
                sum.relocations
            )?;
            if let Some(ref note) = sum.relocation_note {
                writeln!(stdout, "  note: {}", note)?;
            }
        }
    }
    Ok(())
}

/// Fit `text` into exactly `width` terminal columns, keeping its tail.
///
/// The previous version indexed by *bytes* into a `&str`, so a path with any
/// multi-byte character long enough to need truncating panicked outright:
/// `start byte index 12 is not a char boundary`. `fission_cli info` crashed on
/// its own `Path:` line for any non-ASCII path past about fifty bytes.
///
/// Columns rather than characters because the panel is a drawn box: a Hangul
/// or CJK glyph occupies two, so counting characters left the right border
/// short by one column per wide glyph.
fn fit(text: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let char_width = |c: char| UnicodeWidthChar::width(c).unwrap_or(0);
    let total: usize = text.chars().map(char_width).sum();
    if total <= width {
        return format!("{text}{}", " ".repeat(width - total));
    }
    // Keep the tail -- the file name matters more than the directories above
    // it -- behind a leading ellipsis.
    let mut kept: Vec<char> = Vec::new();
    let mut used = 0usize;
    for c in text.chars().rev() {
        let w = char_width(c);
        if used + w > width.saturating_sub(3) {
            break;
        }
        used += w;
        kept.push(c);
    }
    kept.reverse();
    let tail: String = kept.into_iter().collect();
    format!("...{tail}{}", " ".repeat(width - 3 - used))
}

fn truncate(s: &str, max: usize) -> String {
    fit(s, max).trim_end().to_string()
}

pub(super) fn print_sections(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        let sections: Vec<serde_json::Value> = binary
            .sections
            .iter()
            .map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "virtual_address": format!("0x{:x}", s.virtual_address),
                    "virtual_size": s.virtual_size,
                    "file_offset": format!("0x{:x}", s.file_offset),
                    "file_size": s.file_size,
                    "executable": s.is_executable,
                    "readable": s.is_readable,
                    "writable": s.is_writable,
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&sections).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
    } else {
        writeln!(stdout, "Sections ({}):", binary.sections.len())?;
        writeln!(
            stdout,
            "{:<20} {:>16} {:>10} {:>16} {:>10} {:>5}",
            "Name", "VirtAddr", "VirtSize", "FileOffset", "FileSize", "Flags"
        )?;
        writeln!(stdout, "{:─<83}", "")?;
        for sec in &binary.sections {
            let flags = format!(
                "{}{}{}",
                if sec.is_readable { "R" } else { "-" },
                if sec.is_writable { "W" } else { "-" },
                if sec.is_executable { "X" } else { "-" }
            );
            writeln!(
                stdout,
                "{:<20} {:>16} {:>10} {:>16} {:>10} {:>5}",
                truncate(&sec.name, 20),
                format!("0x{:x}", sec.virtual_address),
                sec.virtual_size,
                format!("0x{:x}", sec.file_offset),
                sec.file_size,
                flags
            )?;
        }
    }
    Ok(())
}

pub(super) fn print_imports(binary: &LoadedBinary, with_xrefs: bool, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let imports: Vec<&FunctionInfo> = canonical_imports_sorted(binary);
    let users = if with_xrefs {
        import_users(binary, &imports)
    } else {
        std::collections::HashMap::new()
    };

    if json {
        let funcs: Vec<serde_json::Value> = imports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "origin": f.origin,
                    "kind": f.kind,
                    "source_section": f.source_section,
                    "external_library": f.external_library,
                    "is_thunk_like": f.is_thunk_like,
                    "thunk_target": f.thunk_target.map(|target| format!("0x{target:x}")),
                    "used_by": users.get(&f.address).map(|callers| {
                        callers
                            .iter()
                            .map(|addr| serde_json::json!({
                                "address": format!("0x{addr:x}"),
                                "name": function_label(binary, *addr),
                            }))
                            .collect::<Vec<_>>()
                    }),
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&funcs).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
    } else if with_xrefs {
        writeln!(stdout, "Imported Functions ({}):", imports.len())?;
        writeln!(stdout, "{:>18}  {:<46}  Used by", "Address", "Name")?;
        writeln!(stdout, "{:─<100}", "")?;
        for func in imports {
            let callers = users
                .get(&func.address)
                .map(|callers| {
                    let shown = callers
                        .iter()
                        .take(2)
                        .map(|addr| function_label(binary, *addr))
                        .collect::<Vec<_>>()
                        .join(" ");
                    if callers.len() > 2 {
                        format!("{shown} +{}", callers.len() - 2)
                    } else {
                        shown
                    }
                })
                .unwrap_or_default();
            writeln!(
                stdout,
                "  0x{:012x}  {:<46}  {}",
                func.address, func.name, callers
            )?;
        }
    } else {
        writeln!(stdout, "Imported Functions ({}):", imports.len())?;
        writeln!(stdout, "{:>18}  Name", "Address")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in imports {
            writeln!(stdout, "  0x{:012x}  {}", func.address, func.name)?;
        }
    }
    Ok(())
}

pub(super) fn print_exports(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let exports: Vec<&FunctionInfo> = canonical_exports_sorted(binary);

    if json {
        let funcs: Vec<serde_json::Value> = exports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                    "origin": f.origin,
                    "kind": f.kind,
                    "source_section": f.source_section,
                    "external_library": f.external_library,
                    "is_thunk_like": f.is_thunk_like,
                    "thunk_target": f.thunk_target.map(|target| format!("0x{target:x}")),
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&funcs).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
    } else {
        writeln!(stdout, "Exported Functions ({}):", exports.len())?;
        writeln!(stdout, "{:>18}  {:>8}  Name", "Address", "Size")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in exports {
            writeln!(
                stdout,
                "  0x{:012x}  {:>6}  {}",
                func.address, func.size, func.name
            )?;
        }
    }
    Ok(())
}

/// Which functions call each import.
///
/// The import list says what a binary is able to do -- `VirtualProtect`,
/// `Sleep`, a crypto entry point -- and the next question is always which of
/// its functions does it. The xref index has the calls and records the import
/// slot as a reference to itself, so nothing joined the two.
///
/// A call can reach the slot directly or through a thunk that jumps via it,
/// and the loader already recognises those and records the slot in
/// `thunk_target`, so both are followed. On the binaries measured the thunk
/// hop adds nothing -- every import whose thunk is recognised turns out to be
/// called directly too -- but the hop costs one lookup and the shape it covers
/// is real, so it stays.
///
/// Counting *every* reference instead is much worse: it picks up the thunk's
/// own jump, which has no enclosing function, and the address then lands on
/// whatever the range scan says covers it -- `__p__environ` claimed to use
/// nearly every import in the binary. So a user is a call, from a function
/// that could be named.
///
/// Eight imports of 39 get a caller on the fixture binary, nine of 36 on the
/// corpus ones. The rest are reached only from code the discovery pass never
/// made a function of, and saying nothing is the honest answer there.
fn import_users(
    binary: &LoadedBinary,
    imports: &[&FunctionInfo],
) -> std::collections::HashMap<u64, Vec<u64>> {
    use fission_static::analysis::build_xref_index;
    use fission_static::analysis::xref_index::{XrefKind, XrefSourceCategory};

    let slots: std::collections::HashSet<u64> = imports.iter().map(|f| f.address).collect();
    // Thunk entry -> the import slot it jumps through.
    let thunks: std::collections::HashMap<u64, u64> = binary
        .functions
        .iter()
        .filter_map(|f| {
            f.thunk_target
                .filter(|target| slots.contains(target))
                .map(|target| (f.address, target))
        })
        .collect();

    let mut users: std::collections::HashMap<u64, Vec<u64>> = std::collections::HashMap::new();
    let index = build_xref_index(binary, true);
    for record in &index.refs {
        if record.kind != XrefKind::Call {
            continue;
        }
        let Some(target) = record.target.address else {
            continue;
        };
        let slot = if slots.contains(&target) {
            target
        } else if let Some(&slot) = thunks.get(&target) {
            slot
        } else {
            continue;
        };
        // Without an enclosing function there is no caller to name, and
        // guessing from the instruction address misattributes it.
        let XrefSourceCategory::Instruction {
            enclosing_function: Some(user),
        } = record.source.category
        else {
            continue;
        };
        let entry = users.entry(slot).or_default();
        if !entry.contains(&user) {
            entry.push(user);
        }
    }
    for callers in users.values_mut() {
        callers.sort_unstable();
    }
    users
}

/// A function's name, or its address when discovery never named it.
fn function_label(binary: &LoadedBinary, address: u64) -> String {
    match binary.function_at(address) {
        Some(function) if !function.name.is_empty() => function.name.clone(),
        _ => format!("0x{address:x}"),
    }
}

#[cfg(test)]
mod fit_tests {
    use super::fit;

    /// The panel is a drawn box, so every field must occupy the same columns.
    #[test]
    fn a_wide_glyph_costs_two_columns() {
        // Hangul renders double-width; counting characters left the border a
        // column short for each one.
        assert_eq!(fit("가나", 6), "가나  ");
        assert_eq!(fit("ab", 6), "ab    ");
    }

    /// Byte indexing panicked here: `start byte index 12 is not a char
    /// boundary`. `fission_cli info` crashed on its own `Path:` line for any
    /// non-ASCII path long enough to need truncating.
    #[test]
    fn truncating_a_multi_byte_string_keeps_its_tail_and_does_not_panic() {
        let path = format!("/tmp/{}/x.exe", "가".repeat(13));
        let fitted = fit(&path, 20);
        assert!(fitted.starts_with("..."), "{fitted}");
        assert!(fitted.trim_end().ends_with("/x.exe"), "{fitted}");
        let width: usize = fitted
            .chars()
            .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        assert_eq!(width, 20, "{fitted}");
    }
}
