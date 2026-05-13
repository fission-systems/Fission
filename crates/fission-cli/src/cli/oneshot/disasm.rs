use fission_core::{DISASM_READ_WINDOW, PAGE_SIZE};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodedFlowKind, DecodedInstruction, RuntimeSleighFrontend};
use std::io::{self, Write};

fn collect_function_instructions(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
) -> io::Result<(String, u64, bool, Vec<(u64, String, String)>)> {
    let frontend = runtime_frontend_for_binary(binary)?;
    let func = binary.function_at(addr).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("No function found at address 0x{addr:x}"),
        )
    })?;
    let func_start = func.address;
    let address_state = frontend.normalize_low_bit_code_address(func_start);
    let decode_start = address_state.address;
    let mut func_size = func.size;
    let needs_boundary_detection = func_size == 0;

    if needs_boundary_detection {
        let all_functions: Vec<_> = binary
            .functions
            .iter()
            .filter(|f| f.address > func_start)
            .collect();

        if let Some(next_func) = all_functions.iter().min_by_key(|f| f.address) {
            func_size = next_func.address - func_start;
        } else {
            func_size = PAGE_SIZE as u64;
        }
    }

    let section = binary.section_containing_for_execution(decode_start);

    let (bytes, base) = if let Some(sec) = section {
        let offset = (decode_start - sec.virtual_address) as usize;
        let file_offset = sec.file_offset as usize + offset;
        let remaining = (sec.virtual_size as usize).saturating_sub(offset);
        let len = remaining
            .min(func_size as usize)
            .min(data.len().saturating_sub(file_offset));

        if file_offset + len <= data.len() {
            (&data[file_offset..file_offset + len], decode_start)
        } else {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("Function at 0x{decode_start:x} is outside file bounds"),
            ));
        }
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Function at 0x{decode_start:x} not in any section"),
        ));
    };

    let mut instructions = Vec::new();
    let func_end = decode_start + func_size;
    let decoded = frontend
        .decode_window_with_context_override(
            bytes,
            base,
            usize::MAX,
            address_state.context_override,
        )
        .map_err(to_io_error)?;

    for instr in decoded {
        if instr.address >= func_end {
            break;
        }
        instructions.push((
            instr.address,
            format_instruction_bytes(&instr),
            instr.instruction_text(),
        ));
        if needs_boundary_detection && instr.flow_kind == DecodedFlowKind::Return {
            break;
        }
    }

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
    for (ip, bytes, mnemonic) in &instructions {
        out.push_str(&format!("  0x{:012x}  {:24}  {}\n", ip, bytes, mnemonic));
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
            eprintln!("Error: Address 0x{:x} is outside file bounds", decode_addr);
            std::process::exit(1);
        }
    } else {
        eprintln!("Error: Address 0x{:x} not in any section", decode_addr);
        std::process::exit(1);
    };

    let instructions = frontend
        .decode_window_with_context_override(bytes, base, count, address_state.context_override)
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
                .map(|(ip, bytes, mnemonic)| {
                    serde_json::json!({
                        "address": format!("0x{:x}", ip),
                        "bytes": bytes,
                        "instruction": mnemonic,
                    })
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
