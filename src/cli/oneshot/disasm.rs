use crate::analysis::loader::LoadedBinary;
use std::io::{self, Write};

pub(super) fn disassemble(
    binary: &LoadedBinary,
    data: &[u8],
    addr: u64,
    count: usize,
    json: bool,
) -> io::Result<()> {
    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};
    let mut stdout = io::stdout().lock();

    // Find the section containing this address
    let section = binary
        .sections
        .iter()
        .find(|s| addr >= s.virtual_address && addr < s.virtual_address + s.virtual_size);

    let (bytes, base) = if let Some(sec) = section {
        // Calculate offset within section
        let offset = (addr - sec.virtual_address) as usize;
        let file_offset = sec.file_offset as usize + offset;
        let remaining = (sec.virtual_size as usize).saturating_sub(offset);
        let len = remaining
            .min(1024)
            .min(data.len().saturating_sub(file_offset));

        if file_offset + len <= data.len() {
            (&data[file_offset..file_offset + len], addr)
        } else {
            eprintln!("Error: Address 0x{:x} is outside file bounds", addr);
            std::process::exit(1);
        }
    } else {
        eprintln!("Error: Address 0x{:x} not in any section", addr);
        std::process::exit(1);
    };

    let decoder_options = if binary.is_64bit { 64 } else { 32 };

    let mut decoder = Decoder::with_ip(decoder_options, bytes, base, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    // Pre-allocate output string buffer to reduce reallocations
    let mut output = String::with_capacity(64);
    // Pre-allocate results vector with requested count
    let mut instructions = Vec::with_capacity(count);

    for _ in 0..count {
        if !decoder.can_decode() {
            break;
        }
        let instr = decoder.decode();
        output.clear();
        formatter.format(&instr, &mut output);

        let bytes_str: String = bytes[instr.ip() as usize - base as usize
            ..instr.ip() as usize - base as usize + instr.len()]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        instructions.push((instr.ip(), bytes_str, output.clone()));
    }

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
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&instr_json).unwrap()
        )?;
    } else {
        writeln!(stdout, "Disassembly at 0x{:x}:", addr)?;
        writeln!(
            stdout,
            "{:>18}  {:24}  Instruction",
            "Address", "Bytes"
        )?;
        writeln!(stdout, "{:─<70}", "")?;
        for (ip, bytes, mnemonic) in &instructions {
            writeln!(stdout, "  0x{:012x}  {:24}  {}", ip, bytes, mnemonic)?;
        }
    }
    Ok(())
}
