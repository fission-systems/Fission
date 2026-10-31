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
pub struct SearchQuery {
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

    let mut hits = collect_hits(binary, query, &patterns);

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

/// Every hit, in address order, at most `query.limit` of them.
fn collect_hits(
    binary: &LoadedBinary,
    query: &SearchQuery,
    patterns: &[(String, Vec<PatternByte>)],
) -> Vec<Hit> {
    let mut hits = Vec::new();
    let data = binary.data.as_slice();
    'outer: for section in searchable_sections(binary, &query.sections) {
        let start = section.file_offset as usize;
        let end = start.saturating_add(section.file_size.min(section.virtual_size) as usize);
        let Some(window) = data.get(start..end.min(data.len())) else {
            continue;
        };
        let mut found = Vec::new();
        for (label, pattern) in patterns {
            for offset in matches_in(window, pattern) {
                found.push(Hit {
                    address: section.virtual_address + offset as u64,
                    width: pattern.len(),
                    section: section.name.clone(),
                    label: (patterns.len() > 1).then(|| label.clone()),
                    referrers: Vec::new(),
                });
            }
        }
        // A four-byte value and its eight-byte form hit the same address when
        // the upper half is zero, which is the common case for an address.
        // Report the wider one and drop the narrower -- before the limit is
        // applied, or a pass over the narrow pattern can use the whole limit
        // and the wide matches are never seen, nor the duplicates removed.
        found.sort_by(|left, right| {
            left.address
                .cmp(&right.address)
                .then_with(|| right.width.cmp(&left.width))
        });
        found.dedup_by_key(|hit| hit.address);
        hits.extend(found);
        if hits.len() >= query.limit {
            hits.truncate(query.limit);
            break 'outer;
        }
    }
    hits.sort_by(|left, right| left.address.cmp(&right.address));
    hits
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
        let fits = |width: usize| width >= 8 || value >> (8 * width) == 0;
        let widths: Vec<usize> = match query.value_size {
            // Both common pointer widths, but only those the value fits in:
            // the low 32 bits of a 64-bit address are some other number.
            0 => [4, 8].into_iter().filter(|width| fits(*width)).collect(),
            width if (1..=8).contains(&width) && fits(width) => vec![width],
            width if (1..=8).contains(&width) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("value 0x{value:x} does not fit in --value-size {width}"),
                ));
            }
            width => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("--value-size must be between 1 and 8, not {width}"),
                ));
            }
        };
        for width in widths {
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
        // Hex digits and `?` are ASCII. Checked before the token is cut into
        // byte pairs: a multi-byte character cut in half is not valid UTF-8,
        // and an empty string vacuously "is all wildcards".
        if !token.is_ascii() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("`{token}` is not a hex byte or `??`"),
            ));
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
        // An unmapped section (`.symtab`, `.debug_*`) reports address zero, so
        // its offsets would be reported as addresses that are not there.
        .filter(|section| section.file_size > 0 && section.is_addressable())
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
    use super::{SearchQuery, build_patterns, matches_in, parse_hex_pattern};

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

    #[test]
    fn a_multibyte_character_is_not_a_run_of_wildcards() {
        // Cut into byte pairs, a four-byte emoji is two invalid halves, which
        // an empty string "all wildcards" check used to accept as `?? ??`.
        assert!(parse_hex_pattern("\u{1F525}").is_err());
        assert!(parse_hex_pattern("48 \u{e9}9").is_err());
        assert!(parse_hex_pattern("48 ?? 90").is_ok());
    }

    fn value_query(value: u64, value_size: usize) -> SearchQuery {
        SearchQuery {
            bytes: None,
            text: None,
            value: Some(value),
            value_size,
            sections: Vec::new(),
            limit: 100,
            with_xrefs: false,
        }
    }

    #[test]
    fn a_value_is_never_searched_as_its_truncated_low_half() {
        let widths = |value, size| {
            build_patterns(&value_query(value, size), false).map(|patterns| {
                patterns
                    .iter()
                    .map(|(_, bytes)| bytes.len())
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(widths(0x1000, 0).unwrap(), vec![4, 8]);
        // 0x140004000 does not fit in four bytes: only the eight-byte form.
        assert_eq!(widths(0x1_4000_4000, 0).unwrap(), vec![8]);
        assert!(widths(0x1_4000_4000, 4).is_err());
        assert_eq!(widths(0xff, 1).unwrap(), vec![1]);
        assert!(widths(0x100, 1).is_err());
        // An out-of-range width is a usage error, not "nothing to search for".
        assert!(widths(1, 0).is_ok());
        assert!(widths(1, 9).is_err());
    }

    fn binary_with(bytes: Vec<u8>, mapped: bool) -> fission_loader::loader::LoadedBinary {
        use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
        let size = bytes.len() as u64;
        LoadedBinaryBuilder::new("search.bin".into(), DataBuffer::Heap(bytes))
            .format("ELF")
            .entry_point(0x1000)
            .add_section(SectionInfo {
                name: ".data".into(),
                virtual_address: if mapped { 0x1000 } else { 0 },
                virtual_size: size,
                file_offset: 0,
                file_size: size,
                is_executable: false,
                is_readable: mapped,
                is_writable: false,
            })
            .build()
            .unwrap()
    }

    #[test]
    fn the_limit_counts_distinct_addresses_and_keeps_the_wider_hit() {
        // Four copies of 0x1000 as an eight-byte value. Searched as 4 and 8
        // bytes every copy matches twice; with a limit of two, the narrow pass
        // used to take both slots and report them as four-byte hits.
        let mut bytes = Vec::new();
        for _ in 0..4 {
            bytes.extend_from_slice(&0x1000u64.to_le_bytes());
        }
        let binary = binary_with(bytes, true);
        let mut query = value_query(0x1000, 0);
        query.limit = 2;
        let patterns = build_patterns(&query, false).unwrap();
        let hits = super::collect_hits(&binary, &query, &patterns);
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|hit| hit.width == 8), "wider hit wins");
        assert!(hits[0].address < hits[1].address);
    }

    #[test]
    fn an_unmapped_section_is_not_searched() {
        // A section that is in the file but not loaded has no address; its
        // offsets must not be reported as if they were.
        let binary = binary_with(vec![0x90; 16], false);
        let query = SearchQuery {
            bytes: Some("90 90".into()),
            ..value_query(0, 0)
        };
        let query = SearchQuery {
            value: None,
            ..query
        };
        let patterns = build_patterns(&query, false).unwrap();
        assert!(super::collect_hits(&binary, &query, &patterns).is_empty());
    }
}
