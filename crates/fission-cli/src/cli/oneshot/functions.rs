use fission_loader::loader::LoadedBinary;
use std::io::{self, Write};

pub(super) fn print_function_list(binary: &LoadedBinary, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    if json {
        let funcs: Vec<serde_json::Value> = binary
            .functions
            .iter()
            .map(|f| {
                serde_json::json!({
                    "address": format!("0x{:x}", f.address),
                    "name": f.name,
                    "size": f.size,
                    "is_import": f.is_import,
                    "is_export": f.is_export,
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
        writeln!(stdout, "Functions ({}):", binary.functions.len())?;
        writeln!(stdout, "{:>18}  {:>8}  Name", "Address", "Size")?;
        writeln!(stdout, "{:─<60}", "")?;
        for func in &binary.functions {
            let marker = if func.is_import {
                " [import]"
            } else if func.is_export {
                " [export]"
            } else {
                ""
            };
            writeln!(
                stdout,
                "  0x{:012x}  {:>6}  {}{}",
                func.address, func.size, func.name, marker
            )?;
        }
    }
    Ok(())
}
