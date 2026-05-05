use fission_loader::detector;
use fission_loader::loader::function_view::{
    canonical_exports_sorted, canonical_imports_sorted, canonical_view_counts,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use serde_json::Value;
use std::io::{self, Write};

pub(super) fn print_binary_info(
    binary: &LoadedBinary,
    json: bool,
    include_detections: bool,
    include_identity: bool,
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
        writeln!(stdout, "║ Path:       {:<46} ║", truncate(&binary.path, 46))?;
        writeln!(stdout, "║ Format:     {:<46} ║", &binary.format)?;

        let arch_display = binary
            .architecture
            .as_ref()
            .map(|arch| format!("{} {}-bit ({})", arch.processor, arch.bitness, arch.variant))
            .unwrap_or_else(|| "unknown".to_string());

        writeln!(stdout, "║ Arch:       {:<46} ║", arch_display)?;
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
            writeln!(stdout, "\x1b[1;35mDetections\x1b[0m (heuristics + DiE)")?;
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
                writeln!(
                    stdout,
                    "  detections={} (see --identity --json for evidence)",
                    rep.detections.len()
                )?;
            }
        }
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("...{}", &s[s.len() - max + 3..])
    }
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
            "{:<12} {:>16} {:>10} {:>16} {:>10} {:>5}",
            "Name", "VirtAddr", "VirtSize", "FileOffset", "FileSize", "Flags"
        )?;
        writeln!(stdout, "{:─<75}", "")?;
        for sec in &binary.sections {
            let flags = format!(
                "{}{}{}",
                if sec.is_readable { "R" } else { "-" },
                if sec.is_writable { "W" } else { "-" },
                if sec.is_executable { "X" } else { "-" }
            );
            writeln!(
                stdout,
                "{:<12} {:>16} {:>10} {:>16} {:>10} {:>5}",
                truncate(&sec.name, 12),
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

pub(super) fn print_imports(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let imports: Vec<&FunctionInfo> = canonical_imports_sorted(binary);

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
