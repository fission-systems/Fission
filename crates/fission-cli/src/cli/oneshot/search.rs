//! Find where something is, when you only know what it looks like.
//!
//! Every other command starts from an address: `hex` shows the bytes there,
//! `xrefs` says who points at it, `disasm` decodes it. Nothing answered the
//! question that comes first -- *where is this?* -- so a magic constant, a
//! crypto table, a signature's bytes, or a pointer to a known address could
//! not be located at all.
//!
//! Results are addresses, so they feed straight into the commands that take
//! one.

use fission_loader::loader::{LoadedBinary, SectionInfo};
use std::io::{self, Write};

/// What to look for, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SearchQuery {
    /// Hex bytes, `??` for any byte: `48 8b ?? 24`.
    pub bytes: Option<String>,
    /// Literal text, matched as ASCII.
    pub text: Option<String>,
    /// A value, matched as its little- and big-endian encodings.
    pub value: Option<u64>,
    /// Width in bytes for `value`; 0 means "try 4 and 8".
    pub value_size: usize,
    /// Restrict to these sections when non-empty.
    pub sections: Vec<String>,
    /// Stop after this many hits.
    pub limit: usize,
    /// Report the function that references each hit (runs disassembly).
    pub with_xrefs: bool,
}

/// One byte of a pattern: a value, or any byte.
type PatternByte = Option<u8>;

struct Hit {
    address: u64,
    /// How many bytes matched, so a wider hit can win over a narrower one at
    /// the same address.
    width: usize,
    section: String,
    /// Which of the query's patterns matched, when more than one was tried.
    label: Option<String>,
    referrers: Vec<u64>,
}

pub(super) fn run_search(binary: &LoadedBinary, query: &SearchQuery, json: bool) -> io::Result<()> {
    // The loader knows the byte order; `--value` uses it rather than guessing.
    let big_endian = binary
        .sleigh_language_id()
        .is_some_and(|language| language.contains(":BE:"));
    let patterns = build_patterns(query, big_endian)?;
    if patterns.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "nothing to search for: pass --bytes, --text or --value",
        ));
    }

    let mut hits = Vec::new();
    let data = binary.data.as_slice();
    'outer: for section in searchable_sections(binary, &query.sections) {
        let start = section.file_offset as usize;
        let end = start.saturating_add(section.file_size.min(section.virtual_size) as usize);
        let Some(window) = data.get(start..end.min(data.len())) else {
            continue;
        };
        for (label, pattern) in &patterns {
            for offset in matches_in(window, pattern) {
                hits.push(Hit {
                    address: section.virtual_address + offset as u64,
                    width: pattern.len(),
                    section: section.name.clone(),
                    label: (patterns.len() > 1).then(|| label.clone()),
                    referrers: Vec::new(),
                });
                if hits.len() >= query.limit {
                    break 'outer;
                }
            }
        }
    }
    // A four-byte value and its eight-byte form hit the same address when the
    // upper half is zero, which is the common case for an address. Report the
    // wider one and drop the narrower.
    hits.sort_by(|left, right| {
        left.address
            .cmp(&right.address)
            .then_with(|| right.width.cmp(&left.width))
    });
    hits.dedup_by_key(|hit| hit.address);

    if query.with_xrefs {
        attach_referrers(binary, &mut hits);
    }

    let mut stdout = io::stdout().lock();
    if json {
        let rows: Vec<serde_json::Value> = hits
            .iter()
            .map(|hit| {
                let mut row = serde_json::json!({
                    "address": format!("0x{:x}", hit.address),
                    "section": hit.section,
                });
                if let Some(label) = &hit.label {
                    row["matched"] = serde_json::json!(label);
                }
                if query.with_xrefs {
                    row["referrers"] = serde_json::json!(
                        hit.referrers
                            .iter()
                            .map(|addr| serde_json::json!({
                                "address": format!("0x{addr:x}"),
                                "name": function_label(binary, *addr),
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
                format!("JSON serialization failed: {e}")
            ))?
        )?;
        return Ok(());
    }

    if hits.len() >= query.limit {
        writeln!(
            stdout,
            "{} matches (stopped at --limit {}):",
            hits.len(),
            query.limit
        )?;
    } else {
        writeln!(stdout, "{} matches:", hits.len())?;
    }
    writeln!(
        stdout,
        "{:>18}  {:<12}  {:<28}  {}",
        "Address", "Section", "Referenced by", "Matched"
    )?;
    writeln!(stdout, "{:─<90}", "")?;
    for hit in &hits {
        let referrers = if hit.referrers.is_empty() {
            String::new()
        } else {
            let shown = hit
                .referrers
                .iter()
                .take(2)
                .map(|addr| function_label(binary, *addr))
                .collect::<Vec<_>>()
                .join(" ");
            if hit.referrers.len() > 2 {
                format!("{shown} +{}", hit.referrers.len() - 2)
            } else {
                shown
            }
        };
        writeln!(
            stdout,
            "  0x{:012x}  {:<12}  {:<28}  {}",
            hit.address,
            hit.section,
            referrers,
            hit.label.as_deref().unwrap_or("")
        )?;
    }
    Ok(())
}

/// The patterns a query asks for, each with a name for the output column.
fn build_patterns(
    query: &SearchQuery,
    big_endian: bool,
) -> io::Result<Vec<(String, Vec<PatternByte>)>> {
    let mut patterns = Vec::new();
    if let Some(spec) = &query.bytes {
        let pattern = parse_hex_pattern(spec)?;
        patterns.push((spec.clone(), pattern));
    }
    if let Some(text) = &query.text {
        if text.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--text is empty",
            ));
        }
        patterns.push((
            format!("{text:?}"),
            text.bytes().map(Some).collect::<Vec<_>>(),
        ));
    }
    if let Some(value) = query.value {
        // The binary's own byte order, not both: searching the wrong one turns
        // up coincidences rather than pointers, and the loader already knows
        // which this is.
        let widths: Vec<usize> = match query.value_size {
            0 => vec![4, 8],
            width => vec![width],
        };
        for width in widths.into_iter().filter(|width| (1..=8).contains(width)) {
            let (label, encoded) = if big_endian {
                ("be", value.to_be_bytes()[8 - width..].to_vec())
            } else {
                ("le", value.to_le_bytes()[..width].to_vec())
            };
            patterns.push((
                format!("0x{value:x} {label}{width}"),
                encoded.into_iter().map(Some).collect(),
            ));
        }
    }
    Ok(patterns)
}

/// Parse `48 8b ?? 24` into bytes and wildcards.
///
/// A token is either a wildcard or an even-length run of hex digits. A lone
/// digit is a typo, not a byte: `48 8b 0` used to parse as three bytes ending
/// in `0x00` and search for something the caller never wrote.
fn parse_hex_pattern(spec: &str) -> io::Result<Vec<PatternByte>> {
    let mut pattern = Vec::new();
    for token in spec.split_whitespace() {
        if token == "??" || token == "?" {
            pattern.push(None);
            continue;
        }
        if token.len() % 2 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{token}` has an odd number of hex digits"),
            ));
        }
        for pair in token.as_bytes().chunks(2) {
            let text = std::str::from_utf8(pair).unwrap_or_default();
            // A wildcard inside a run: `48??24`.
            if text.chars().all(|c| c == '?') {
                pattern.push(None);
                continue;
            }
            pattern.push(Some(u8::from_str_radix(text, 16).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("`{text}` is not a hex byte or `??`"),
                )
            })?));
        }
    }
    if pattern.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the byte pattern is empty",
        ));
    }
    Ok(pattern)
}

/// Offsets in `window` where `pattern` matches.
fn matches_in(window: &[u8], pattern: &[PatternByte]) -> Vec<usize> {
    let mut out = Vec::new();
    if pattern.is_empty() || window.len() < pattern.len() {
        return out;
    }
    // Anchor on the first non-wildcard byte so a pattern that starts with `??`
    // still narrows the scan instead of testing every offset.
    let anchor = pattern.iter().position(Option::is_some);
    for start in 0..=window.len() - pattern.len() {
        if let Some(index) = anchor
            && window[start + index] != pattern[index].expect("anchor is a value")
        {
            continue;
        }
        if pattern
            .iter()
            .enumerate()
            .all(|(i, byte)| byte.is_none_or(|b| window[start + i] == b))
        {
            out.push(start);
        }
    }
    out
}

/// The sections to scan, honouring a `--section` filter.
fn searchable_sections<'a>(binary: &'a LoadedBinary, wanted: &[String]) -> Vec<&'a SectionInfo> {
    binary
        .sections
        .iter()
        .filter(|section| section.file_size > 0)
        .filter(|section| wanted.is_empty() || wanted.iter().any(|name| *name == section.name))
        .collect()
}

/// Attach the functions whose code reads each hit.
fn attach_referrers(binary: &LoadedBinary, hits: &mut [Hit]) {
    use fission_static::analysis::build_xref_index;
    use fission_static::analysis::xref_index::{XrefKind, XrefSourceCategory};

    let mut by_address: std::collections::HashMap<u64, usize> = std::collections::HashMap::new();
    for (index, hit) in hits.iter().enumerate() {
        by_address.entry(hit.address).or_insert(index);
    }
    let xrefs = build_xref_index(binary, true);
    for record in &xrefs.refs {
        if !matches!(
            record.kind,
            XrefKind::DataRead | XrefKind::DataWrite | XrefKind::Relocation
        ) {
            continue;
        }
        let Some(target) = record.target.address else {
            continue;
        };
        let Some(&index) = by_address.get(&target) else {
            continue;
        };
        let XrefSourceCategory::Instruction { enclosing_function } = record.source.category else {
            continue;
        };
        let referrer = enclosing_function.unwrap_or(record.source.address);
        let referrers = &mut hits[index].referrers;
        if !referrers.contains(&referrer) {
            referrers.push(referrer);
        }
    }
    for hit in hits.iter_mut() {
        hit.referrers.sort_unstable();
    }
}

/// A function's name, or its address when discovery never named it.
fn function_label(binary: &LoadedBinary, address: u64) -> String {
    match binary.function_at(address) {
        Some(function) if !function.name.is_empty() => function.name.clone(),
        _ => format!("0x{address:x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{matches_in, parse_hex_pattern};

    #[test]
    fn a_wildcard_matches_any_byte_in_that_position() {
        let pattern = parse_hex_pattern("48 8b ?? 24").expect("parses");
        let window = [0x48, 0x8b, 0x04, 0x24, 0x48, 0x8b, 0xff, 0x24];
        assert_eq!(matches_in(&window, &pattern), vec![0, 4]);
    }

    #[test]
    fn a_run_of_digits_is_the_same_as_spaced_bytes() {
        assert_eq!(
            parse_hex_pattern("488b0424").expect("parses"),
            parse_hex_pattern("48 8b 04 24").expect("parses")
        );
    }

    #[test]
    fn a_pattern_that_opens_with_a_wildcard_still_matches() {
        // The scan anchors on the first real byte; a leading wildcard must not
        // shift where the match is reported.
        let pattern = parse_hex_pattern("?? 90 90").expect("parses");
        let window = [0x00, 0x11, 0x90, 0x90, 0x22];
        assert_eq!(matches_in(&window, &pattern), vec![1]);
    }

    #[test]
    fn a_malformed_pattern_is_rejected_rather_than_ignored() {
        assert!(parse_hex_pattern("zz").is_err());
        // A lone digit is a typo, not the byte `0x00`.
        assert!(parse_hex_pattern("48 8b 0").is_err());
        assert!(parse_hex_pattern("   ").is_err());
        // A wildcard inside an unspaced run.
        assert_eq!(
            parse_hex_pattern("48??24").expect("parses"),
            parse_hex_pattern("48 ?? 24").expect("parses")
        );
    }
}
