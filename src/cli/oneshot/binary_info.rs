use crate::analysis::loader::{FunctionInfo, LoadedBinary};
use std::io::{self, Write};

pub(super) fn print_binary_info(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    
    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "path": binary.path,
                "format": binary.format,
                "arch": if binary.is_64bit { "x86_64" } else { "x86" },
                "bits": if binary.is_64bit { 64 } else { 32 },
                "entry": format!("0x{:x}", binary.entry_point),
                "image_base": format!("0x{:x}", binary.image_base),
                "sections": binary.sections.len(),
                "functions": binary.functions.len(),
                "imports": binary.functions.iter().filter(|f| f.is_import).count(),
                "exports": binary.functions.iter().filter(|f| f.is_export).count(),
            }))
            .unwrap()
        )?;
    } else {
        writeln!(stdout, "\x1b[1;36m╔══════════════════════════════════════════════════════════╗\x1b[0m")?;
        writeln!(stdout, "\x1b[1;36m║\x1b[0m          \x1b[1;35m📊 BINARY INFORMATION\x1b[0m                    \x1b[1;36m║\x1b[0m")?;
        writeln!(stdout, "\x1b[1;36m╠══════════════════════════════════════════════════════════╣\x1b[0m")?;
        writeln!(stdout, "║ Path:       {:<46} ║", truncate(&binary.path, 46))?;
        writeln!(stdout, "║ Format:     {:<46} ║", &binary.format)?;
        writeln!(
            stdout,
            "║ Arch:       {:<46} ║",
            if binary.is_64bit {
                "x86_64 (64-bit)"
            } else {
                "x86 (32-bit)"
            }
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
            binary.functions.len(),
            binary.iat_symbols.len()
        )?;
        writeln!(
            stdout,
            "║ Imports:    {:<10} Exports:   {:<24} ║",
            binary.functions.iter().filter(|f| f.is_import).count(),
            binary.functions.iter().filter(|f| f.is_export).count()
        )?;
        writeln!(
            stdout,
            "╚══════════════════════════════════════════════════════════╝"
        )?;
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
            serde_json::to_string_pretty(&sections).unwrap()
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
    let imports: Vec<&FunctionInfo> = binary.functions.iter().filter(|f| f.is_import).collect();

    if json {
        let funcs: Vec<serde_json::Value> = imports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                })
            })
            .collect();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&funcs).unwrap())?;
    } else {
        writeln!(stdout, "Imported Functions ({}):", imports.len())?;
        writeln!(stdout, "{:>18}  {}", "Address", "Name")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in imports {
            writeln!(stdout, "  0x{:012x}  {}", func.address, func.name)?;
        }
    }
    Ok(())
}

pub(super) fn print_exports(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let exports: Vec<&FunctionInfo> = binary.functions.iter().filter(|f| f.is_export).collect();

    if json {
        let funcs: Vec<serde_json::Value> = exports
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                })
            })
            .collect();
        writeln!(stdout, "{}", serde_json::to_string_pretty(&funcs).unwrap())?;
    } else {
        writeln!(stdout, "Exported Functions ({}):", exports.len())?;
        writeln!(stdout, "{:>18}  {:>8}  {}", "Address", "Size", "Name")?;
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
