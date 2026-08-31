//! The bytes at an address.
//!
//! Every other command speaks virtual addresses -- `list` gives one, `xrefs`
//! points at one, `disasm` annotates one -- and none of them could show what
//! is actually stored there. Reading a jump table an instruction indexes into,
//! checking a struct's layout, confirming what a data reference points at: all
//! of it meant leaving the tool.

use fission_loader::loader::LoadedBinary;
use std::io::{self, Write};

pub(super) fn print_hex(
    binary: &LoadedBinary,
    address: u64,
    count: usize,
    json: bool,
) -> io::Result<()> {
    let mut stdout = io::stdout().lock();

    let Some(section) = binary.sections.iter().find(|section| {
        let mapped = section.file_size.min(section.virtual_size);
        section
            .virtual_address
            .checked_add(mapped)
            .is_some_and(|end| (section.virtual_address..end).contains(&address))
    }) else {
        // An address outside every mapped section has no bytes to show, and
        // guessing a file offset from the image base would print whatever
        // happened to sit there.
        let message = format!("0x{address:x} is not inside any mapped section");
        if json {
            writeln!(stdout, "{}", serde_json::json!({ "error": message }))?;
        } else {
            writeln!(stdout, "{message}")?;
        }
        return Ok(());
    };

    let offset_in_section = address - section.virtual_address;
    let file_offset = (section.file_offset + offset_in_section) as usize;
    let mapped_end = (section.file_offset + section.file_size.min(section.virtual_size)) as usize;
    let data = binary.data.as_slice();
    let end = file_offset
        .saturating_add(count)
        .min(mapped_end)
        .min(data.len());
    let bytes = data.get(file_offset..end).unwrap_or(&[]);

    if json {
        writeln!(
            stdout,
            "{}",
            serde_json::json!({
                "address": format!("0x{address:x}"),
                "section": section.name,
                "file_offset": format!("0x{file_offset:x}"),
                "length": bytes.len(),
                "bytes": bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(""),
            })
        )?;
        return Ok(());
    }

    writeln!(
        stdout,
        "0x{address:x} in {} (file offset 0x{file_offset:x}), {} bytes:",
        section.name,
        bytes.len()
    )?;
    if bytes.len() < count {
        writeln!(
            stdout,
            "  (stopped at the end of {}; {count} requested)",
            section.name
        )?;
    }
    for (row, chunk) in bytes.chunks(16).enumerate() {
        writeln!(stdout, "  {}", hex_row(address + (row * 16) as u64, chunk))?;
    }
    Ok(())
}

/// One `ADDRESS  hex…  ascii` line.
fn hex_row(address: u64, chunk: &[u8]) -> String {
    let hex: String = chunk
        .iter()
        .enumerate()
        .map(|(index, byte)| {
            // An extra space at the halfway mark, the way every hex dump
            // splits a row -- and it is *extra*: replacing the trailing space
            // rather than adding to it glues the pair together as `6c64 2d`.
            if index == 8 {
                format!(" {byte:02x} ")
            } else {
                format!("{byte:02x} ")
            }
        })
        .collect();
    let ascii: String = chunk
        .iter()
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '.'
            }
        })
        .collect();
    format!("{address:012x}  {hex:<50}  {ascii}")
}

#[cfg(test)]
mod tests {
    use super::hex_row;

    #[test]
    fn the_halfway_split_separates_the_bytes_it_splits() {
        let row = hex_row(0x140004000, b"realworld-fixtu");
        // The byte after the gap must still be its own column: the first
        // version of this borrowed the GUI's dump, which replaced the
        // trailing space instead of adding one, and printed `6f 72  6c64 2d`.
        assert!(row.contains("6f 72 6c  64 2d 66"), "{row}");
        assert!(row.ends_with("realworld-fixtu"), "{row}");
    }

    #[test]
    fn a_short_final_row_keeps_the_ascii_column_aligned() {
        let full = hex_row(0, &[0x41; 16]);
        let short = hex_row(0, &[0x41; 3]);
        let column = |row: &str| row.rfind("  ").map(|i| i + 2);
        assert_eq!(
            column(&full),
            column(&short),
            "full:\n{full}\nshort:\n{short}"
        );
    }
}
