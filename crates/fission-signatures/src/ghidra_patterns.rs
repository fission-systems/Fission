//! Ghidra XML Function-Start Pattern Parser
//!
//! Parses Ghidra's `<patternlist>` XML format (e.g. `x86-64win_patterns.xml`)
//! using `quick-xml` and produces [`GhidraFuncPattern`] entries for the
//! function-discovery pipeline.

use fission_core::resources::ResourceProvider;
use quick_xml::Reader;
use quick_xml::events::Event;

/// A single parsed Ghidra function-start pattern.
///
/// A match requires:
/// 1. `pre_bytes` match immediately **before** the candidate address (right-aligned).
/// 2. `post_bytes` match starting **at** the candidate address.
#[derive(Debug, Clone)]
pub struct GhidraFuncPattern {
    pub pre_bytes: Vec<Option<u8>>,
    pub post_bytes: Vec<Option<u8>>,
    pub label: Option<String>,
    pub after_cond: Option<String>,
    pub valid_code_min: Option<usize>,
}

impl GhidraFuncPattern {
    pub fn matches(&self, data: &[u8], data_base: u64, addr: u64) -> bool {
        if addr < data_base {
            return false;
        }
        let offset = (addr - data_base) as usize;

        let pre_len = self.pre_bytes.len();
        if pre_len > 0 {
            if offset < pre_len { return false; }
            let pre_slice = &data[offset - pre_len..offset];
            for (i, pat) in self.pre_bytes.iter().enumerate() {
                if let Some(expected) = pat {
                    if pre_slice[i] != *expected { return false; }
                }
            }
        }

        let post_len = self.post_bytes.len();
        if post_len > 0 {
            if offset + post_len > data.len() { return false; }
            let post_slice = &data[offset..offset + post_len];
            for (i, pat) in self.post_bytes.iter().enumerate() {
                if let Some(expected) = pat {
                    if post_slice[i] != *expected { return false; }
                }
            }
        }
        true
    }
}

/// Load all Ghidra function-start patterns relevant for `arch_tag`.
///
/// Since we don't detect the compiler at discovery time, we load **all**
/// patterns for the given architecture (both Windows/MSVC and GCC flavours)
/// and merge them. This mirrors loading every `<patternfile>` listed under
/// the matching `<language>` block in `patternconstraints.xml` and
/// `prepatternconstraints.xml`.
///
/// Returns empty `Vec` on any error (non-fatal).
pub fn load_ghidra_patterns(arch_tag: &str, compiler_id: Option<&str>) -> Vec<GhidraFuncPattern> {
    // Map arch_tag + compiler_id → specific pattern files for that architecture and compiler
    // Derived from patternconstraints.xml + prepatternconstraints.xml
    let files: &[&str] = match (arch_tag, compiler_id) {
        ("x86-64win" | "x86-64", Some("gcc")) => &[
            "ghidra/x86-64gcc_patterns.xml",
        ],
        ("x86-64win" | "x86-64", Some("windows") | Some("borlandcpp")) => &[
            "ghidra/x86-64win_patterns.xml",
        ],
        ("x86-64win" | "x86-64", _) => &[
            "ghidra/x86-64win_patterns.xml",
            "ghidra/x86-64gcc_patterns.xml",
        ],
        ("x86win" | "x86", Some("gcc")) => &[
            "ghidra/x86gcc_patterns.xml",
            "ghidra/x86gcc_prepatterns.xml",
        ],
        ("x86win" | "x86", Some("windows") | Some("borlandcpp")) => &[
            "ghidra/x86win_patterns.xml",
            "ghidra/x86win_prepatterns.xml",
        ],
        ("x86win" | "x86", Some("borlanddelphi")) => &[
            "ghidra/x86delphi_patterns.xml",
        ],
        ("x86win" | "x86", _) => &[
            "ghidra/x86win_patterns.xml",
            "ghidra/x86gcc_patterns.xml",
            "ghidra/x86win_prepatterns.xml",
            "ghidra/x86gcc_prepatterns.xml",
        ],
        _ => return Vec::new(),
    };

    let provider = ResourceProvider::global();
    let paths = provider.paths();
    let mut all: Vec<GhidraFuncPattern> = Vec::new();

    for &file in files {
        let Some(path) = paths.get_pattern_file(file) else { continue; };
        let xml = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[fission-signatures] Failed to read {}: {e}", path.display());
                continue;
            }
        };
        match parse_ghidra_pattern_xml(&xml) {
            Ok(pats) => all.extend(pats),
            Err(e) => eprintln!("[fission-signatures] Parse error {}: {e}", path.display()),
        }
    }

    all
}

// ─── Parser ───────────────────────────────────────────────────────────────

pub fn parse_ghidra_pattern_xml(xml: &str) -> Result<Vec<GhidraFuncPattern>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut out: Vec<GhidraFuncPattern> = Vec::new();
    let mut buf = Vec::new();

    // --- state machine ---
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum Ctx { Root, PatternPairs, PrePatterns, PostPatterns, StandalonePattern }

    let mut ctx = Ctx::Root;

    // patternpairs accumulator
    let mut pp_pre: Vec<Vec<Option<u8>>> = Vec::new();
    let mut pp_post: Vec<Vec<Option<u8>>> = Vec::new();
    let mut pp_has_funcstart = false;
    let mut pp_after_cond: Option<String> = None;
    let mut pp_valid_code_min: Option<usize> = None;

    // standalone <pattern> accumulator
    let mut sp_pre: Vec<Option<u8>> = Vec::new();
    let mut sp_post: Vec<Option<u8>> = Vec::new();
    let mut sp_label: Option<String> = None;
    let mut sp_after_cond: Option<String> = None;
    let mut sp_valid_code_min: Option<usize> = None;
    let mut sp_has_funcstart = false;
    let mut sp_star_found = false; // whether we parsed pre*post inline

    // which part of <data> content we're collecting
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum DataSlot { None, PpPre, PpPost, SpInline }
    let mut data_slot = DataSlot::None;

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(format!("XML error: {e}")),
            Ok(Event::Eof) => break,

            // ── opening tags ──────────────────────────────────────────────
            Ok(Event::Start(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_ascii_lowercase();
                match tag.as_str() {
                    "patternpairs" => {
                        ctx = Ctx::PatternPairs;
                        pp_pre.clear(); pp_post.clear(); pp_has_funcstart = false;
                        pp_after_cond = None; pp_valid_code_min = None;
                    }
                    "prepatterns"  if ctx == Ctx::PatternPairs => { ctx = Ctx::PrePatterns; }
                    "postpatterns" if ctx == Ctx::PatternPairs => { ctx = Ctx::PostPatterns; }
                    "pattern" => {
                        ctx = Ctx::StandalonePattern;
                        sp_pre.clear(); sp_post.clear();
                        sp_label = None; sp_after_cond = None; sp_valid_code_min = None;
                        sp_has_funcstart = false; sp_star_found = false;
                    }
                    "data" => {
                        data_slot = match ctx {
                            Ctx::PrePatterns       => DataSlot::PpPre,
                            Ctx::PostPatterns      => DataSlot::PpPost,
                            Ctx::StandalonePattern => DataSlot::SpInline,
                            _                      => DataSlot::None,
                        };
                    }
                    "funcstart" | "possiblefuncstart" => {
                        let label = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"label")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let after = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"after")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let validcode = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcode")
                            .and_then(|a| std::str::from_utf8(a.value.as_ref()).ok().and_then(|s| s.parse::<usize>().ok()));

                        match ctx {
                            Ctx::PostPatterns      => {
                                pp_has_funcstart = true;
                                if after.is_some() { pp_after_cond = after; }
                                if validcode.is_some() { pp_valid_code_min = validcode; }
                            }
                            Ctx::StandalonePattern => {
                                sp_has_funcstart = true;
                                if label.is_some() { sp_label = label; }
                                if after.is_some() { sp_after_cond = after; }
                                if validcode.is_some() { sp_valid_code_min = validcode; }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // ── self-closing tags ─────────────────────────────────────────
            Ok(Event::Empty(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_ascii_lowercase();
                match tag.as_str() {
                    "funcstart" | "possiblefuncstart" => {
                        let label = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"label")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let after = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"after")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let validcode = e.attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcode")
                            .and_then(|a| std::str::from_utf8(a.value.as_ref()).ok().and_then(|s| s.parse::<usize>().ok()));

                        match ctx {
                            Ctx::PostPatterns      => {
                                pp_has_funcstart = true;
                                if after.is_some() { pp_after_cond = after; }
                                if validcode.is_some() { pp_valid_code_min = validcode; }
                            }
                            Ctx::StandalonePattern => {
                                sp_has_funcstart = true;
                                if label.is_some() { sp_label = label; }
                                if after.is_some() { sp_after_cond = after; }
                                if validcode.is_some() { sp_valid_code_min = validcode; }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // ── text content ─────────────────────────────────────────────
            Ok(Event::Text(ref e)) => {
                let decoded = e.decode().unwrap_or_default();
                let text = decoded.trim();
                if text.is_empty() || data_slot == DataSlot::None { continue; }

                match data_slot {
                    DataSlot::PpPre => {
                        let p = parse_data_string(text);
                        if !p.is_empty() { pp_pre.push(p); }
                    }
                    DataSlot::PpPost => {
                        let p = parse_data_string(text);
                        if !p.is_empty() { pp_post.push(p); }
                    }
                    DataSlot::SpInline => {
                        // Inline `*` syntax: pre_bytes * post_bytes
                        if let Some(star_pos) = text.find('*') {
                            sp_pre  = parse_data_string(text[..star_pos].trim());
                            sp_post = parse_data_string(text[star_pos + 1..].trim());
                            sp_star_found = true;
                        } else {
                            sp_post = parse_data_string(text);
                        }
                    }
                    DataSlot::None => {}
                }
                data_slot = DataSlot::None;
            }

            // ── closing tags ──────────────────────────────────────────────
            Ok(Event::End(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref()).unwrap_or("").to_ascii_lowercase();
                match tag.as_str() {
                    "patternpairs" => {
                        if pp_has_funcstart {
                            if pp_pre.is_empty() {
                                for post in &pp_post {
                                    out.push(GhidraFuncPattern {
                                        pre_bytes: vec![],
                                        post_bytes: post.clone(),
                                        label: None,
                                        after_cond: pp_after_cond.clone(),
                                        valid_code_min: pp_valid_code_min,
                                    });
                                }
                            } else {
                                for pre in &pp_pre {
                                    for post in &pp_post {
                                        out.push(GhidraFuncPattern {
                                            pre_bytes: pre.clone(),
                                            post_bytes: post.clone(),
                                            label: None,
                                            after_cond: pp_after_cond.clone(),
                                            valid_code_min: pp_valid_code_min,
                                        });
                                    }
                                }
                            }
                        }
                        ctx = Ctx::Root;
                        pp_pre.clear(); pp_post.clear(); pp_has_funcstart = false;
                        pp_after_cond = None; pp_valid_code_min = None;
                    }
                    "prepatterns"  => { if ctx == Ctx::PrePatterns  { ctx = Ctx::PatternPairs; } }
                    "postpatterns" => { if ctx == Ctx::PostPatterns  { ctx = Ctx::PatternPairs; } }
                    "data"         => { data_slot = DataSlot::None; }
                    "pattern" => {
                        if sp_has_funcstart && !sp_post.is_empty() {
                            out.push(GhidraFuncPattern {
                                pre_bytes:  if sp_star_found { sp_pre.clone() } else { vec![] },
                                post_bytes: sp_post.clone(),
                                label:      sp_label.take(),
                                after_cond: sp_after_cond.take(),
                                valid_code_min: sp_valid_code_min.take(),
                            });
                        }
                        ctx = Ctx::Root;
                        sp_pre.clear(); sp_post.clear(); sp_label = None;
                        sp_after_cond = None; sp_valid_code_min = None;
                        sp_has_funcstart = false; sp_star_found = false;
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    Ok(out)
}

// ─── Byte-pattern string parser ───────────────────────────────────────────

pub fn parse_data_string(s: &str) -> Vec<Option<u8>> {
    let mut result = Vec::new();
    let mut rest = s;

    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() { break; }

        if rest.starts_with("0x") || rest.starts_with("0X") {
            let hex_start = 2;
            let hex_end = rest[hex_start..]
                .find(|c: char| !c.is_ascii_hexdigit())
                .map(|i| hex_start + i)
                .unwrap_or(rest.len());
            let hex_str = &rest[hex_start..hex_end];
            let mut i = 0;
            while i + 1 < hex_str.len() {
                if let Ok(b) = u8::from_str_radix(&hex_str[i..i + 2], 16) {
                    result.push(Some(b));
                }
                i += 2;
            }
            rest = &rest[hex_end..];
        } else if rest.len() >= 8 {
            let byte_token = &rest[..8];
            if byte_token.chars().all(|c| c == '0' || c == '1' || c == '.') {
                result.push(None); // wildcard
                rest = &rest[8..];
            } else {
                rest = &rest[1..];
            }
        } else {
            rest = &rest[1..];
        }
    }

    result
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_data_fixed_bytes() {
        assert_eq!(parse_data_string("0x4883ec"), vec![Some(0x48), Some(0x83), Some(0xec)]);
    }

    #[test]
    fn test_parse_data_wildcard() {
        assert_eq!(parse_data_string("0xc3 ........"), vec![Some(0xc3), None]);
    }

    #[test]
    fn test_parse_data_bitmask() {
        assert_eq!(parse_data_string("0x55 01...100"), vec![Some(0x55), None]);
    }

    #[test]
    fn test_pattern_matches() {
        let pat = GhidraFuncPattern {
            pre_bytes: vec![Some(0xcc)],
            post_bytes: vec![Some(0x48), Some(0x83), Some(0xec)],
            label: None,
            after_cond: None,
            valid_code_min: None,
        };
        let data = vec![0xcc, 0x48, 0x83, 0xec];
        assert!(pat.matches(&data, 0x1000, 0x1001));
        assert!(!pat.matches(&data, 0x1000, 0x1000));
        assert!(!pat.matches(&data, 0x1000, 0x1002));
    }

    #[test]
    fn test_parse_inline_star_pattern() {
        let xml = r#"<patternlist>
          <pattern>
            <data>0xcc * 0x4883ec</data>
            <funcstart/>
          </pattern>
        </patternlist>"#;
        let pats = parse_ghidra_pattern_xml(xml).unwrap();
        assert_eq!(pats.len(), 1, "expected 1 pattern, got {}: {pats:?}", pats.len());
        assert_eq!(pats[0].pre_bytes, vec![Some(0xcc)]);
        assert_eq!(pats[0].post_bytes, vec![Some(0x48), Some(0x83), Some(0xec)]);
    }

    #[test]
    fn test_parse_patternpairs() {
        let xml = r#"<patternlist>
          <patternpairs totalbits="32" postbits="16">
            <prepatterns>
              <data>0xcc</data>
            </prepatterns>
            <postpatterns>
              <data>0x4883ec</data>
              <funcstart/>
            </postpatterns>
          </patternpairs>
        </patternlist>"#;
        let pats = parse_ghidra_pattern_xml(xml).unwrap();
        assert!(!pats.is_empty(), "expected patterns from patternpairs");
        assert_eq!(pats[0].pre_bytes, vec![Some(0xcc)]);
        assert_eq!(pats[0].post_bytes, vec![Some(0x48), Some(0x83), Some(0xec)]);
    }

    #[test]
    fn test_x86_64win_inline_patterns() {
        let xml = r#"<patternlist>
          <pattern>
            <data>0xcccccc * 0x4883ec</data>
            <funcstart/>
          </pattern>
          <pattern>
            <data>0xcc * 0x554883ec</data>
            <funcstart/>
          </pattern>
        </patternlist>"#;
        let pats = parse_ghidra_pattern_xml(xml).unwrap();
        assert_eq!(pats.len(), 2, "expected 2 patterns, got {}", pats.len());
    }
}
