use std::io::{self, Write};

pub(super) fn print_strings(data: &[u8], min_len: usize, json: bool) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    // Pre-allocate with estimated capacity (heuristic: ~1 string per 1KB of data)
    let estimated_strings = data.len() / 1024;
    let mut strings: Vec<(usize, String)> = Vec::with_capacity(estimated_strings.max(100));

    // Pre-allocate buffer with reasonable capacity to reduce reallocations
    let mut current_bytes: Vec<u8> = Vec::with_capacity(256);
    let mut start_offset = 0;

    for (i, &byte) in data.iter().enumerate() {
        if (0x20..0x7f).contains(&byte) {
            if current_bytes.is_empty() {
                start_offset = i;
            }
            current_bytes.push(byte);
        } else {
            if current_bytes.len() >= min_len {
                // SAFETY: We only pushed bytes in 0x20-0x7E range, which are valid ASCII/UTF-8
                let value =
                    unsafe { String::from_utf8_unchecked(std::mem::take(&mut current_bytes)) };
                strings.push((start_offset, value));
            }
            current_bytes.clear();
        }
    }
    // Don't forget last string
    if current_bytes.len() >= min_len {
        let value = unsafe { String::from_utf8_unchecked(current_bytes) };
        strings.push((start_offset, value));
    }

    if json {
        let str_json: Vec<serde_json::Value> = strings
            .iter()
            .map(|(off, s)| {
                serde_json::json!({
                    "offset": format!("0x{:x}", off),
                    "string": s,
                })
            })
            .collect();
        writeln!(
            stdout,
            "{}",
            serde_json::to_string_pretty(&str_json).map_err(|e| io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e)
            ))?
        )?;
    } else {
        writeln!(
            stdout,
            "Strings ({} found, min length {}):",
            strings.len(),
            min_len
        )?;
        writeln!(stdout, "{:>12}  String", "Offset")?;
        writeln!(stdout, "{:─<60}", "")?;
        for (off, s) in &strings {
            // Truncate long strings for display
            if s.len() > 60 {
                writeln!(stdout, "  0x{:08x}  {}...", off, &s[..57])?;
            } else {
                writeln!(stdout, "  0x{:08x}  {}", off, s)?;
            }
        }
    }
    Ok(())
}
