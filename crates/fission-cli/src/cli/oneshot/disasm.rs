use fission_core::DISASM_READ_WINDOW;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodedInstruction, RuntimeSleighFrontend};
use std::io::{self, Write};

/// Thin CLI-side wrapper over the shared `fission_decompiler::disasm`
/// primitive (moved there so more than one caller can
/// share the same implementation instead of the CLI having the only copy).
/// Keeps this module's own `(name, func_start, needs_boundary_detection,
/// Vec<(addr, bytes_hex, text)>)` shape so the text/JSON renderers below
/// didn't need to change.
fn collect_function_instructions(
    binary: &LoadedBinary,
    _data: &[u8],
    addr: u64,
) -> io::Result<(String, u64, bool, Vec<(u64, String, String, String)>)> {
    let func = binary.function_at(addr).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No function found at address 0x{addr:x}"),
        )
    })?;
    let func_start = func.address;
    let needs_boundary_detection = func.size == 0;

    let rows =
        fission_decompiler::disasm::disassemble_function(binary, addr).map_err(to_io_error)?;
    let instructions = rows
        .into_iter()
        .map(|row| {
            (
                row.address,
                row.bytes_hex,
                row.text,
                row.refers_to.unwrap_or_default(),
            )
        })
        .collect();

    Ok((
        func.name.clone(),
        func_start,
        needs_boundary_detection,
        instructions,
    ))
}

pub(super) fn render_function_disassembly_text(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
) -> io::Result<String> {
    let (name, func_start, needs_boundary_detection, instructions) =
        collect_function_instructions(binary, data, addr)?;

    let mut out = String::new();
    if needs_boundary_detection {
        out.push_str(&format!(
            "Function: {} at 0x{:x} (size: auto-detected)\n",
            name, func_start
        ));
    } else {
        let size = binary.function_at(addr).map(|f| f.size).unwrap_or_default();
        out.push_str(&format!(
            "Function: {} at 0x{:x} (size: {} bytes)\n",
            name, func_start, size
        ));
    }
    out.push_str(&format!("{:>18}  {:24}  Instruction\n", "Address", "Bytes"));
    out.push_str(&format!("{:─<70}\n", ""));
    for (ip, bytes, mnemonic, note) in &instructions {
        if note.is_empty() {
            out.push_str(&format!("  0x{:012x}  {:24}  {}\n", ip, bytes, mnemonic));
        } else {
            out.push_str(&format!(
                "  0x{:012x}  {:24}  {:<34}  ; {}\n",
                ip, bytes, mnemonic, note
            ));
        }
    }
    out.push_str(&format!("\nTotal instructions: {}\n", instructions.len()));
    Ok(out)
}

pub(super) fn disassemble(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
    count: usize,
    json: bool,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    let frontend = runtime_frontend_for_binary(binary)?;
    let address_state = frontend.normalize_low_bit_code_address(addr);
    let decode_addr = address_state.address;

    // Find the section containing this address
    let section = binary.section_containing_for_execution(decode_addr);

    let (bytes, base) = if let Some(sec) = section {
        // Calculate offset within section
        let offset = (decode_addr - sec.virtual_address) as usize;
        let file_offset = sec.file_offset as usize + offset;
        let remaining = (sec.virtual_size as usize).saturating_sub(offset);
        let len = remaining
            .min(DISASM_READ_WINDOW)
            .min(data.len().saturating_sub(file_offset));

        if file_offset + len <= data.len() {
            (&data[file_offset..file_offset + len], decode_addr)
        } else {
            let message = format!("Address 0x{decode_addr:x} is outside file bounds");
            if json {
                let mut stdout = io::stdout().lock();
                let _ = writeln!(stdout, "{}", serde_json::json!({ "error": message }));
            } else {
                eprintln!("Error: {message}");
            }
            return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
        }
    } else {
        let message = format!("Address 0x{decode_addr:x} not in any section");
        if json {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{}", serde_json::json!({ "error": message }));
        } else {
            eprintln!("Error: {message}");
        }
        return Err(io::Error::new(io::ErrorKind::InvalidInput, message));
    };

    let context_override = fission_decompiler::disasm::decode_context_for_address(
        binary,
        &frontend,
        address_state.context_override,
    );
    let instructions = frontend
        .decode_window_with_context_override(bytes, base, count, context_override)
        .map_err(to_io_error)?
        .into_iter()
        .map(|instr| {
            (
                instr.address,
                format_instruction_bytes(&instr),
                instr.instruction_text(),
            )
        })
        .collect::<Vec<_>>();

    if json {
        let instr_json: Vec<serde_json::Value> = instructions
            .iter()
            .map(|(ip, bytes, mnemonic)| {
                serde_json::json!({
                    "address": format!("0x{:x}", ip),
                    "bytes": bytes,
                    "instruction": mnemonic,
                })
            })
            .collect();
        let json_output = serde_json::to_string_pretty(&instr_json).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?;
        writeln!(stdout, "{}", json_output)?;
    } else {
        writeln!(stdout, "Disassembly at 0x{:x}:", addr)?;
        writeln!(stdout, "{:>18}  {:24}  Instruction", "Address", "Bytes")?;
        writeln!(stdout, "{:─<70}", "")?;
        for (ip, bytes, mnemonic) in &instructions {
            writeln!(stdout, "  0x{:012x}  {:24}  {}", ip, bytes, mnemonic)?;
        }
    }
    Ok(())
}

fn runtime_frontend_for_binary(binary: &LoadedBinary) -> io::Result<RuntimeSleighFrontend> {
    let load_spec = binary.load_spec().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::Unsupported,
            format!("missing Ghidra load spec for '{}'", binary.path),
        )
    })?;
    RuntimeSleighFrontend::new_for_load_spec(load_spec).map_err(to_io_error)
}

fn to_io_error<E>(err: E) -> io::Error
where
    E: std::fmt::Display,
{
    io::Error::new(io::ErrorKind::Other, err.to_string())
}

fn format_instruction_bytes(instruction: &DecodedInstruction) -> String {
    instruction
        .bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Disassemble entire function at given address (function boundaries)
pub(super) fn disassemble_function(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
    json: bool,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();

    let (name, func_start, needs_boundary_detection, instructions) =
        collect_function_instructions(binary, data, addr)?;

    if json {
        let result = serde_json::json!({
            "function": {
                "name": &name,
                "address": format!("0x{:x}", func_start),
                "size": if needs_boundary_detection {
                    "unknown (stopped at RET)".to_string()
                } else {
                    binary.function_at(addr).map(|f| f.size).unwrap_or_default().to_string()
                },
            },
            "instructions": instructions
                .iter()
                .map(|(ip, bytes, mnemonic, note)| {
                    let mut row = serde_json::json!({
                        "address": format!("0x{:x}", ip),
                        "bytes": bytes,
                        "instruction": mnemonic,
                    });
                    if !note.is_empty() {
                        row["refers_to"] = serde_json::json!(note);
                    }
                    row
                })
                .collect::<Vec<_>>(),
        });
        let json_output = serde_json::to_string_pretty(&result).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?;
        writeln!(stdout, "{}", json_output)?;
    } else {
        write!(
            stdout,
            "{}",
            render_function_disassembly_text(binary, data, addr)?
        )?;
    }
    Ok(())
}

/// Convert disassembled instruction tuples into canonical `DisasmResponse`.
pub fn to_disasm_response(
    instructions: &[(u64, String, String, String)],
) -> fission_analysis_protocol::DisasmResponse {
    let rows = instructions
        .iter()
        .map(
            |(ip, bytes, text, note)| fission_analysis_protocol::DisasmRow {
                address: *ip,
                bytes_hex: bytes.clone(),
                text: text.clone(),
                target_addr: None,
                refers_to: if note.is_empty() {
                    None
                } else {
                    Some(note.clone())
                },
            },
        )
        .collect();
    fission_analysis_protocol::DisasmResponse { rows }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{BinaryLoadSpec, DataBuffer, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn disassemble_unmapped_address_returns_error_without_exit() {
        let binary = LoadedBinaryBuilder::new("test.bin".into(), DataBuffer::Heap(vec![0x90; 16]))
            .format("ELF")
            .image_base(0x1000)
            .load_spec(BinaryLoadSpec::new(
                "ELF",
                0x1000,
                "x86:LE:64:default",
                "gcc",
                "synthetic-test",
            ))
            .add_section(SectionInfo {
                name: ".text".into(),
                virtual_address: 0x1000,
                virtual_size: 16,
                file_offset: 0,
                file_size: 16,
                is_readable: true,
                is_writable: false,
                is_executable: true,
            })
            .build()
            .expect("build fixture");

        let err = disassemble(&binary, binary.data.as_slice(), 0x9999, 10, true).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("not in any section"));
    }
}
