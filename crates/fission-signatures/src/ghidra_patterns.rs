//! Ghidra XML Function-Start Pattern Parser
//!
//! Parses Ghidra's `<patternlist>` XML format (e.g. `x86-64win_patterns.xml`)
//! using `quick-xml` and produces [`GhidraFuncPattern`] entries for the
//! function-discovery pipeline.

use fission_core::resources::ResourceProvider;
use quick_xml::Reader;
use quick_xml::events::Event;

/// One byte of a ditted pattern: the bits that must match (`mask`) and the
/// values they must have (`value`, already reduced to the masked bits).
///
/// Ghidra's `DittedBitSequence` is a bitmask, not a per-byte wildcard: a hex
/// token such as `0x1.` or a binary token such as `01...100` fixes some bits
/// of a byte and leaves the rest free. Modelling it as `Option<u8>` could only
/// say "all eight bits" or "none of them", which is why a partial byte used to
/// be widened to "none" -- and a pattern that is looser than the file says
/// reports function starts the file never described.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternByte {
    pub value: u8,
    pub mask: u8,
}

impl PatternByte {
    /// A byte with no constrained bit.
    pub const ANY: PatternByte = PatternByte { value: 0, mask: 0 };

    /// A byte with every bit constrained.
    pub const fn exact(value: u8) -> Self {
        PatternByte { value, mask: 0xff }
    }

    /// The byte's value if all eight bits are constrained, else `None`.
    ///
    /// Callers that index patterns by a literal prefix use this: a
    /// partially-constrained byte cannot be part of a literal.
    pub const fn fixed(self) -> Option<u8> {
        if self.mask == 0xff {
            Some(self.value)
        } else {
            None
        }
    }

    /// Whether any bit is constrained.
    pub const fn is_constrained(self) -> bool {
        self.mask != 0
    }

    pub const fn matches(self, byte: u8) -> bool {
        byte & self.mask == self.value
    }
}

/// A single parsed Ghidra function-start pattern.
///
/// A match requires:
/// 1. `pre_bytes` match immediately **before** the candidate address (right-aligned).
/// 2. `post_bytes` match starting **at** the candidate address.
#[derive(Debug, Clone)]
pub struct GhidraFuncPattern {
    pub pre_bytes: Vec<PatternByte>,
    pub post_bytes: Vec<PatternByte>,
    pub label: Option<String>,
    pub after_cond: Option<String>,
    pub valid_code_min: Option<usize>,
    pub valid_code_max: Option<usize>,
    pub contiguous: bool,
    pub is_thunk: bool,
}

impl GhidraFuncPattern {
    /// Whether this pattern constrains any byte at all.
    ///
    /// One that does not matches every address it is offered, which is not a
    /// pattern -- it is a scan of the whole section. Ghidra's ARM files are
    /// where this bites: nearly every post-pattern there opens with a
    /// wildcard byte (`........ 0xb5`), and one that reduces to nothing but
    /// wildcards reported a function start at all 147,503 bytes of `nuttx`'s
    /// `.text`.
    pub fn constrains_any_byte(&self) -> bool {
        self.pre_bytes
            .iter()
            .chain(self.post_bytes.iter())
            .any(|b| b.is_constrained())
    }

    pub fn matches(&self, data: &[u8], data_base: u64, addr: u64) -> bool {
        if !self.constrains_any_byte() {
            return false;
        }
        if addr < data_base {
            return false;
        }
        let offset = (addr - data_base) as usize;

        let pre_len = self.pre_bytes.len();
        if pre_len > 0 {
            if offset < pre_len {
                return false;
            }
            let pre_slice = &data[offset - pre_len..offset];
            for (pat, &byte) in self.pre_bytes.iter().zip(pre_slice) {
                if !pat.matches(byte) {
                    return false;
                }
            }
        }

        let post_len = self.post_bytes.len();
        if post_len > 0 {
            if offset + post_len > data.len() {
                return false;
            }
            let post_slice = &data[offset..offset + post_len];
            for (pat, &byte) in self.post_bytes.iter().zip(post_slice) {
                if !pat.matches(byte) {
                    return false;
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
        ("x86-64win" | "x86-64", Some("gcc")) => &["ghidra/x86-64gcc_patterns.xml"],
        ("x86-64win" | "x86-64", Some("windows") | Some("borlandcpp")) => {
            &["ghidra/x86-64win_patterns.xml"]
        }
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
        ("x86win" | "x86", Some("borlanddelphi")) => &["ghidra/x86delphi_patterns.xml"],
        ("x86win" | "x86", _) => &[
            "ghidra/x86win_patterns.xml",
            "ghidra/x86gcc_patterns.xml",
            "ghidra/x86win_prepatterns.xml",
            "ghidra/x86gcc_prepatterns.xml",
        ],
        // Non-x86 architectures: mapping taken directly from each
        // processor's own patternconstraints.xml/prepatternconstraints.xml
        // under vendor/ghidra (e.g. Ghidra/Processors/ARM/data/patterns/),
        // not guessed. Most processors don't branch on compiler_id at all
        // (their constraint file has no <compiler> tag) -- AARCH64 and
        // PowerPC are the exceptions actually present in the source data.
        ("ARM_LE", _) => &[
            "ghidra/ARM_LE_patterns.xml",
            "ghidra/ARM_switch_patterns.xml",
        ],
        ("ARM_BE", _) => &[
            "ghidra/ARM_BE_patterns.xml",
            "ghidra/ARM_switch_patterns.xml",
        ],
        ("AARCH64", Some("windows")) => &[
            "ghidra/AARCH64_LE_patterns.xml",
            "ghidra/AARCH64_win_patterns.xml",
        ],
        ("AARCH64", _) => &["ghidra/AARCH64_LE_patterns.xml"],
        ("MIPS_BE", _) => &["ghidra/MIPS_BE_patterns.xml"],
        ("MIPS_LE", _) => &["ghidra/MIPS_LE_patterns.xml"],
        ("PowerPC_BE", _) => &[
            "ghidra/PPC_BE_patterns.xml",
            "ghidra/PPC_BE_prepatterns.xml",
        ],
        ("PowerPC_LE", _) => &[
            "ghidra/PPC_LE_patterns.xml",
            "ghidra/PPC_LE_prepatterns.xml",
        ],
        ("Sparc", _) => &["ghidra/SPARC_patterns.xml"],
        ("SuperH4", _) => &["ghidra/SuperH4_patterns.xml"],
        ("V850", _) => &["ghidra/V850_patterns.xml"],
        ("68000", _) => &["ghidra/68000_patterns.xml"],
        ("avr8", _) => &["ghidra/AVR8_patterns.xml"],
        ("Loongarch", _) => &["ghidra/loongarch_patterns.xml"],
        ("NDS32", _) => &["ghidra/nds32_patterns.xml"],
        ("PA-RISC", _) => &["ghidra/pa-risc_patterns.xml"],
        ("RISCV", _) => &["ghidra/riscv_gc_patterns.xml"],
        ("tricore", _) => &["ghidra/tricore_patterns.xml"],
        ("Xtensa", _) => &["ghidra/xtensa_patterns.xml"],
        _ => return Vec::new(),
    };

    let provider = ResourceProvider::global();
    let paths = provider.paths();
    let mut all: Vec<GhidraFuncPattern> = Vec::new();

    for &file in files {
        let Some(path) = paths.get_pattern_file(file) else {
            continue;
        };
        let xml = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "[fission-signatures] Failed to read {}: {e}",
                    path.display()
                );
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
    enum Ctx {
        Root,
        PatternPairs,
        PrePatterns,
        PostPatterns,
        StandalonePattern,
    }

    let mut ctx = Ctx::Root;

    // patternpairs accumulator
    let mut pp_pre: Vec<Vec<PatternByte>> = Vec::new();
    let mut pp_post: Vec<Vec<PatternByte>> = Vec::new();
    let mut pp_has_funcstart = false;
    let mut pp_after_cond: Option<String> = None;
    let mut pp_valid_code_min: Option<usize> = None;
    let mut pp_valid_code_max: Option<usize> = None;
    let mut pp_contiguous = false;
    let mut pp_is_thunk = false;

    // standalone <pattern> accumulator
    let mut sp_pre: Vec<PatternByte> = Vec::new();
    let mut sp_post: Vec<PatternByte> = Vec::new();
    let mut sp_label: Option<String> = None;
    let mut sp_after_cond: Option<String> = None;
    let mut sp_valid_code_min: Option<usize> = None;
    let mut sp_valid_code_max: Option<usize> = None;
    let mut sp_contiguous = false;
    let mut sp_is_thunk = false;
    let mut sp_has_funcstart = false;
    let mut sp_star_found = false; // whether we parsed pre*post inline

    // which part of <data> content we're collecting
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum DataSlot {
        None,
        PpPre,
        PpPost,
        SpInline,
    }
    let mut data_slot = DataSlot::None;
    let mut data_text = String::new();

    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(format!("XML error: {e}")),
            Ok(Event::Eof) => break,

            // ── opening tags ──────────────────────────────────────────────
            Ok(Event::Start(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "patternpairs" => {
                        ctx = Ctx::PatternPairs;
                        pp_pre.clear();
                        pp_post.clear();
                        pp_has_funcstart = false;
                        pp_after_cond = None;
                        pp_valid_code_min = None;
                        pp_valid_code_max = None;
                        pp_contiguous = false;
                        pp_is_thunk = false;
                    }
                    "prepatterns" if ctx == Ctx::PatternPairs => {
                        ctx = Ctx::PrePatterns;
                    }
                    "postpatterns" if ctx == Ctx::PatternPairs => {
                        ctx = Ctx::PostPatterns;
                    }
                    "pattern" => {
                        ctx = Ctx::StandalonePattern;
                        sp_pre.clear();
                        sp_post.clear();
                        sp_label = None;
                        sp_after_cond = None;
                        sp_valid_code_min = None;
                        sp_valid_code_max = None;
                        sp_contiguous = false;
                        sp_is_thunk = false;
                        sp_has_funcstart = false;
                        sp_star_found = false;
                    }
                    "data" => {
                        data_text.clear();
                        data_slot = match ctx {
                            Ctx::PrePatterns => DataSlot::PpPre,
                            Ctx::PostPatterns => DataSlot::PpPost,
                            Ctx::StandalonePattern => DataSlot::SpInline,
                            _ => DataSlot::None,
                        };
                    }
                    "funcstart" | "possiblefuncstart" => {
                        let label = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"label")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let after = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"after")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let validcode = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcode")
                            .and_then(|a| {
                                std::str::from_utf8(a.value.as_ref())
                                    .ok()
                                    .and_then(|s| s.parse::<usize>().ok())
                            });
                        let validcodemax = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcodemax")
                            .and_then(|a| {
                                std::str::from_utf8(a.value.as_ref())
                                    .ok()
                                    .and_then(|s| s.parse::<usize>().ok())
                            });
                        let contiguous = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"contiguous")
                            .is_some_and(|a| a.value.as_ref().eq_ignore_ascii_case(b"true"));
                        let thunk = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"thunk")
                            .is_some_and(|a| a.value.as_ref().eq_ignore_ascii_case(b"true"));

                        match ctx {
                            Ctx::PostPatterns => {
                                pp_has_funcstart = true;
                                if after.is_some() {
                                    pp_after_cond = after;
                                }
                                if validcode.is_some() {
                                    pp_valid_code_min = validcode;
                                }
                                if validcodemax.is_some() {
                                    pp_valid_code_max = validcodemax;
                                }
                                if contiguous {
                                    pp_contiguous = true;
                                }
                                if thunk {
                                    pp_is_thunk = true;
                                }
                            }
                            Ctx::StandalonePattern => {
                                sp_has_funcstart = true;
                                if label.is_some() {
                                    sp_label = label;
                                }
                                if after.is_some() {
                                    sp_after_cond = after;
                                }
                                if validcode.is_some() {
                                    sp_valid_code_min = validcode;
                                }
                                if validcodemax.is_some() {
                                    sp_valid_code_max = validcodemax;
                                }
                                if contiguous {
                                    sp_contiguous = true;
                                }
                                if thunk {
                                    sp_is_thunk = true;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // ── self-closing tags ─────────────────────────────────────────
            Ok(Event::Empty(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                match tag.as_str() {
                    "funcstart" | "possiblefuncstart" => {
                        let label = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"label")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let after = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"after")
                            .and_then(|a| String::from_utf8(a.value.to_vec()).ok());
                        let validcode = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcode")
                            .and_then(|a| {
                                std::str::from_utf8(a.value.as_ref())
                                    .ok()
                                    .and_then(|s| s.parse::<usize>().ok())
                            });
                        let validcodemax = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"validcodemax")
                            .and_then(|a| {
                                std::str::from_utf8(a.value.as_ref())
                                    .ok()
                                    .and_then(|s| s.parse::<usize>().ok())
                            });
                        let contiguous = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"contiguous")
                            .is_some_and(|a| a.value.as_ref().eq_ignore_ascii_case(b"true"));
                        let thunk = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"thunk")
                            .is_some_and(|a| a.value.as_ref().eq_ignore_ascii_case(b"true"));

                        match ctx {
                            Ctx::PostPatterns => {
                                pp_has_funcstart = true;
                                if after.is_some() {
                                    pp_after_cond = after;
                                }
                                if validcode.is_some() {
                                    pp_valid_code_min = validcode;
                                }
                                if validcodemax.is_some() {
                                    pp_valid_code_max = validcodemax;
                                }
                                if contiguous {
                                    pp_contiguous = true;
                                }
                                if thunk {
                                    pp_is_thunk = true;
                                }
                            }
                            Ctx::StandalonePattern => {
                                sp_has_funcstart = true;
                                if label.is_some() {
                                    sp_label = label;
                                }
                                if after.is_some() {
                                    sp_after_cond = after;
                                }
                                if validcode.is_some() {
                                    sp_valid_code_min = validcode;
                                }
                                if validcodemax.is_some() {
                                    sp_valid_code_max = validcodemax;
                                }
                                if contiguous {
                                    sp_contiguous = true;
                                }
                                if thunk {
                                    sp_is_thunk = true;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            // ── text content ─────────────────────────────────────────────
            // A <data> element can hold several text runs: Ghidra annotates
            // multi-instruction patterns one line at a time with XML comments,
            // and the reader reports the text between two comments as separate
            // events. Collect them all and read the sequence when the element
            // closes, not on the first run.
            Ok(Event::Text(ref e)) => {
                if data_slot == DataSlot::None {
                    continue;
                }
                let decoded = e.decode().unwrap_or_default();
                let text = decoded.trim();
                if !text.is_empty() {
                    data_text.push(' ');
                    data_text.push_str(text);
                }
            }

            // ── closing tags ──────────────────────────────────────────────
            Ok(Event::End(ref e)) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_ascii_lowercase();
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
                                        valid_code_max: pp_valid_code_max,
                                        contiguous: pp_contiguous,
                                        is_thunk: pp_is_thunk,
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
                                            valid_code_max: pp_valid_code_max,
                                            contiguous: pp_contiguous,
                                            is_thunk: pp_is_thunk,
                                        });
                                    }
                                }
                            }
                        }
                        ctx = Ctx::Root;
                        pp_pre.clear();
                        pp_post.clear();
                        pp_has_funcstart = false;
                        pp_after_cond = None;
                        pp_valid_code_min = None;
                        pp_valid_code_max = None;
                        pp_contiguous = false;
                        pp_is_thunk = false;
                    }
                    "prepatterns" if ctx == Ctx::PrePatterns => {
                        ctx = Ctx::PatternPairs;
                    }
                    "postpatterns" if ctx == Ctx::PostPatterns => {
                        ctx = Ctx::PatternPairs;
                    }
                    "data" => {
                        let text = std::mem::take(&mut data_text);
                        let text = text.trim();
                        if !text.is_empty() {
                            match data_slot {
                                DataSlot::PpPre => {
                                    let p = parse_data_string(text);
                                    if !p.is_empty() {
                                        pp_pre.push(p);
                                    }
                                }
                                DataSlot::PpPost => {
                                    let p = parse_data_string(text);
                                    if !p.is_empty() {
                                        pp_post.push(p);
                                    }
                                }
                                DataSlot::SpInline => {
                                    // Inline `*` syntax: pre_bytes * post_bytes
                                    if let Some(star_pos) = text.find('*') {
                                        let pre_text = text[..star_pos].trim();
                                        sp_pre = parse_data_string(pre_text);
                                        sp_post = parse_data_string(text[star_pos + 1..].trim());
                                        // A malformed prefix would otherwise leave an empty one,
                                        // i.e. a pattern with no context requirement at all.
                                        if sp_pre.is_empty() && !pre_text.is_empty() {
                                            sp_post.clear();
                                        }
                                        sp_star_found = true;
                                    } else {
                                        sp_post = parse_data_string(text);
                                    }
                                }
                                DataSlot::None => {}
                            }
                        }
                        data_slot = DataSlot::None;
                    }
                    "pattern" => {
                        if sp_has_funcstart && !sp_post.is_empty() {
                            out.push(GhidraFuncPattern {
                                pre_bytes: if sp_star_found {
                                    sp_pre.clone()
                                } else {
                                    vec![]
                                },
                                post_bytes: sp_post.clone(),
                                label: sp_label.take(),
                                after_cond: sp_after_cond.take(),
                                valid_code_min: sp_valid_code_min.take(),
                                valid_code_max: sp_valid_code_max.take(),
                                contiguous: sp_contiguous,
                                is_thunk: sp_is_thunk,
                            });
                        }
                        ctx = Ctx::Root;
                        sp_pre.clear();
                        sp_post.clear();
                        sp_label = None;
                        sp_after_cond = None;
                        sp_valid_code_min = None;
                        sp_valid_code_max = None;
                        sp_contiguous = false;
                        sp_is_thunk = false;
                        sp_has_funcstart = false;
                        sp_star_found = false;
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

/// Parse one ditted byte sequence (`0x..d.4de2 ....0000 .1...... 00101101`).
///
/// This is Ghidra's `DittedBitSequence` text form: a token opening with `0x`
/// is hex, two characters per byte, each of them a nibble that may be `.`;
/// any other token is binary, eight characters per byte, each a bit that may
/// be `.`. Bytes are in memory order -- Ghidra does not byte-swap them for
/// little-endian targets, so `0x0000a0e1` is the bytes `00 00 a0 e1`.
///
/// A malformed token (odd nibble count, a bit run that is not a whole number
/// of bytes, a character outside the alphabet) makes the whole sequence
/// empty. Keeping the bytes around it would shift every later byte to the
/// wrong offset, which is a different pattern from the one written.
pub fn parse_data_string(s: &str) -> Vec<PatternByte> {
    let mut out = Vec::new();
    // `#` starts a comment that runs to the end of the line.
    let text = s.lines().filter_map(|line| line.split('#').next());
    for token in text.flat_map(str::split_whitespace) {
        if !parse_token(token, &mut out) {
            return Vec::new();
        }
    }
    out
}

/// One whitespace-delimited token: a run of bit bytes, a `0x` hex run, or a
/// bit run followed directly by a hex run (`01.01...0x..` occurs in the
/// shipped ARM_BE file, where the author left out the space; Ghidra reads such
/// a token as garbage, so the intent is the only sensible reading).
fn parse_token(mut token: &str, out: &mut Vec<PatternByte>) -> bool {
    loop {
        if let Some(nibbles) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            return parse_hex_run(nibbles, out);
        }
        if token.is_empty() {
            return true;
        }
        let Some((byte, rest)) = token.split_at_checked(8) else {
            return false;
        };
        if !parse_bit_byte(byte, out) {
            return false;
        }
        token = rest;
    }
}

/// A nibble as `(value, mask)`: a hex digit fixes all four bits, `.` none.
fn nibble(c: char) -> Option<(u8, u8)> {
    match c {
        '.' => Some((0, 0)),
        c => c.to_digit(16).map(|d| (d as u8, 0xf)),
    }
}

fn parse_hex_run(nibbles: &str, out: &mut Vec<PatternByte>) -> bool {
    let chars: Vec<char> = nibbles.chars().collect();
    if chars.is_empty() || chars.len() % 2 != 0 {
        return false;
    }
    for pair in chars.chunks(2) {
        let (Some((hi, hi_mask)), Some((lo, lo_mask))) = (nibble(pair[0]), nibble(pair[1])) else {
            return false;
        };
        out.push(PatternByte {
            value: hi << 4 | lo,
            mask: hi_mask << 4 | lo_mask,
        });
    }
    true
}

fn parse_bit_byte(bits: &str, out: &mut Vec<PatternByte>) -> bool {
    let (mut value, mut mask) = (0u8, 0u8);
    let mut count = 0;
    for c in bits.chars() {
        value <<= 1;
        mask <<= 1;
        count += 1;
        match c {
            '0' => mask |= 1,
            '1' => {
                value |= 1;
                mask |= 1;
            }
            '.' => {}
            _ => return false,
        }
    }
    if count != 8 {
        return false;
    }
    out.push(PatternByte { value, mask });
    true
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(bytes: &[u8]) -> Vec<PatternByte> {
        bytes.iter().map(|&b| PatternByte::exact(b)).collect()
    }

    fn pb(value: u8, mask: u8) -> PatternByte {
        PatternByte { value, mask }
    }

    #[test]
    fn loads_patterns_for_every_non_x86_architecture() {
        // Confirms the pattern files copied from vendor/ghidra's per-
        // processor patternconstraints.xml actually parse into non-empty
        // pattern sets, not just that the file paths resolve.
        for (tag, compiler) in [
            ("ARM_LE", None),
            ("ARM_BE", None),
            ("AARCH64", None),
            ("AARCH64", Some("windows")),
            ("MIPS_BE", None),
            ("MIPS_LE", None),
            ("PowerPC_BE", None),
            ("PowerPC_LE", None),
            ("Sparc", None),
            ("SuperH4", None),
            ("V850", None),
            ("68000", None),
            ("avr8", None),
            ("Loongarch", None),
            ("NDS32", None),
            ("PA-RISC", None),
            ("RISCV", None),
            ("tricore", None),
            ("Xtensa", None),
        ] {
            let patterns = load_ghidra_patterns(tag, compiler);
            assert!(
                !patterns.is_empty(),
                "expected non-empty patterns for {tag}/{compiler:?}"
            );
        }
    }

    #[test]
    fn test_parse_data_fixed_bytes() {
        assert_eq!(
            parse_data_string("0x4883ec"),
            vec![
                PatternByte::exact(0x48),
                PatternByte::exact(0x83),
                PatternByte::exact(0xec)
            ]
        );
    }

    #[test]
    fn test_parse_data_wildcard() {
        assert_eq!(
            parse_data_string("0xc3 ........"),
            vec![PatternByte::exact(0xc3), PatternByte::ANY]
        );
    }

    #[test]
    fn test_parse_data_bitmask() {
        assert_eq!(
            parse_data_string("0x55 01...100"),
            vec![
                PatternByte::exact(0x55),
                PatternByte {
                    value: 0b0100_0100,
                    mask: 0b1100_0111
                }
            ]
        );
    }

    #[test]
    fn a_wildcard_nibble_constrains_only_the_other_nibble() {
        // `0x..` is one fully free byte; `0x1.ff2fe1` is four bytes whose
        // first one fixes only its high nibble. Neither used to survive: the
        // parser stopped at the first `.`, dropped the token, and every byte
        // after it slid to a lower offset.
        assert_eq!(parse_data_string("0x.."), vec![PatternByte::ANY]);
        assert_eq!(
            parse_data_string("0x1.ff2fe1"),
            vec![
                pb(0x10, 0xf0),
                PatternByte::exact(0xff),
                PatternByte::exact(0x2f),
                PatternByte::exact(0xe1)
            ]
        );
        assert_eq!(
            parse_data_string("0x......ea"),
            vec![
                PatternByte::ANY,
                PatternByte::ANY,
                PatternByte::ANY,
                PatternByte::exact(0xea)
            ]
        );
        assert_eq!(parse_data_string("0x.f"), vec![pb(0x0f, 0x0f)]);
    }

    #[test]
    fn binary_tokens_keep_the_bits_they_fix() {
        // `11101101` is 0xed, not a wildcard.
        assert_eq!(parse_data_string("11101101"), exact(&[0xed]));
        assert_eq!(parse_data_string("000....."), vec![pb(0x00, 0xe0)]);
        assert_eq!(parse_data_string("....0000"), vec![pb(0x00, 0x0f)]);
        assert_eq!(parse_data_string(".......0"), vec![pb(0x00, 0x01)]);
        assert_eq!(parse_data_string("........"), vec![PatternByte::ANY]);
        // One bit-run token can span several bytes; whitespace is optional.
        assert_eq!(
            parse_data_string("01...100..100100"),
            vec![pb(0x44, 0xc7), pb(0x24, 0x3f)]
        );
    }

    #[test]
    fn hex_and_binary_tokens_can_be_mixed_and_keep_their_offsets() {
        assert_eq!(
            parse_data_string("0xe92d 1....... 0x.."),
            vec![
                PatternByte::exact(0xe9),
                PatternByte::exact(0x2d),
                pb(0x80, 0x80),
                PatternByte::ANY
            ]
        );
    }

    #[test]
    fn a_bit_run_may_be_followed_directly_by_a_hex_run() {
        assert_eq!(
            parse_data_string("01.01...0x..     0xb5 ....0000"),
            vec![
                pb(0x48, 0xd8),
                PatternByte::ANY,
                PatternByte::exact(0xb5),
                pb(0x00, 0x0f)
            ]
        );
    }

    #[test]
    fn a_hash_comment_runs_to_the_end_of_the_line() {
        assert_eq!(
            parse_data_string("0x....823d\t# addis r12,r2,0x####\n 0x..0041f8  # std r2,0x##(r1)"),
            vec![
                PatternByte::ANY,
                PatternByte::ANY,
                PatternByte::exact(0x82),
                PatternByte::exact(0x3d),
                PatternByte::ANY,
                PatternByte::exact(0x00),
                PatternByte::exact(0x41),
                PatternByte::exact(0xf8)
            ]
        );
    }

    #[test]
    fn bytes_are_in_memory_order_not_swapped() {
        // Ghidra's pattern files are already in memory order for
        // little-endian targets (ARM `mov r0, r0` is 0xe1a00000, stored
        // 00 00 a0 e1), so `0x0000a0e1` must stay as written.
        assert_eq!(
            parse_data_string("0x0000a0e1"),
            exact(&[0x00, 0x00, 0xa0, 0xe1])
        );
    }

    #[test]
    fn a_malformed_token_empties_the_sequence_instead_of_shifting_it() {
        // Odd nibble count, a bit run that is not whole bytes, and a stray
        // character each describe no byte layout; keeping the rest would put
        // later bytes at the wrong offsets.
        assert!(parse_data_string("0x48 0x1").is_empty());
        assert!(parse_data_string("0x48 0111").is_empty());
        assert!(parse_data_string("0x48 0xzz").is_empty());
        assert!(parse_data_string("0x").is_empty());
        assert!(parse_data_string("hello").is_empty());
        assert!(parse_data_string("").is_empty());
    }

    #[test]
    fn a_malformed_prefix_drops_the_inline_pattern() {
        let xml = r#"<patternlist>
          <pattern>
            <data>0x1 * 0x4883ec</data>
            <funcstart/>
          </pattern>
        </patternlist>"#;
        assert!(parse_ghidra_pattern_xml(xml).unwrap().is_empty());
    }

    #[test]
    fn partial_bytes_are_matched_on_their_constrained_bits() {
        let pat = GhidraFuncPattern {
            pre_bytes: vec![],
            post_bytes: parse_data_string("0x1. 01...100"),
            label: None,
            after_cond: None,
            valid_code_min: None,
            valid_code_max: None,
            contiguous: false,
            is_thunk: false,
        };
        assert!(pat.matches(&[0x1a, 0x44], 0, 0));
        assert!(pat.matches(&[0x1f, 0x7c], 0, 0));
        assert!(!pat.matches(&[0x2a, 0x44], 0, 0), "high nibble differs");
        assert!(!pat.matches(&[0x1a, 0x45], 0, 0), "a fixed low bit differs");
    }

    fn strip_xml_comments(xml: &str) -> String {
        let mut out = String::new();
        let mut rest = xml;
        while let Some(start) = rest.find("<!--") {
            out.push_str(&rest[..start]);
            match rest[start..].find("-->") {
                Some(end) => rest = &rest[start + end + 3..],
                None => return out,
            }
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn every_shipped_pattern_file_parses_without_dropping_a_pattern() {
        // Parsing the checked-in files must never hit the malformed path: a
        // dropped token is a pattern that silently stopped existing.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../utils/signatures/patterns/ghidra");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut checked = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("xml") {
                continue;
            }
            let xml = strip_xml_comments(&std::fs::read_to_string(&path).unwrap());
            let mut tokens = 0;
            for chunk in xml.split("<data>").skip(1) {
                let Some((data, _)) = chunk.split_once("</data>") else {
                    continue;
                };
                for side in data.split('*') {
                    let side = side.trim();
                    if side.is_empty() {
                        continue;
                    }
                    tokens += 1;
                    assert!(
                        !parse_data_string(side).is_empty(),
                        "{} has a <data> side that does not parse: {side:?}",
                        path.display()
                    );
                }
            }
            if tokens > 0 {
                checked += 1;
            }
        }
        assert!(checked > 0, "no pattern file with <data> was found");
    }

    #[test]
    fn comments_inside_a_data_element_do_not_end_the_sequence() {
        // Ghidra's PowerPC files annotate a multi-instruction pattern one
        // line at a time. The XML reader hands back the text between two
        // comments as separate events, and only the first used to be read, so
        // an eight-instruction pattern became its first instruction.
        let xml = r#"<patternlist>
          <pattern>
            <data>
              0x280041f8      <!-- std r2,0x28(r1) -->
              0xff 0xff ...00010 001111..   <!-- subis -->
              ........ ........ ........ 0xe9
            </data>
            <funcstart after="defined" thunk="true"/>
          </pattern>
        </patternlist>"#;
        let pats = parse_ghidra_pattern_xml(xml).unwrap();
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].post_bytes.len(), 12, "{:?}", pats[0].post_bytes);
        assert_eq!(pats[0].post_bytes[4], PatternByte::exact(0xff));
        assert_eq!(pats[0].post_bytes[11], PatternByte::exact(0xe9));
    }

    #[test]
    fn test_pattern_matches() {
        let pat = GhidraFuncPattern {
            pre_bytes: vec![PatternByte::exact(0xcc)],
            post_bytes: vec![
                PatternByte::exact(0x48),
                PatternByte::exact(0x83),
                PatternByte::exact(0xec),
            ],
            label: None,
            after_cond: None,
            valid_code_min: None,
            valid_code_max: None,
            contiguous: false,
            is_thunk: false,
        };
        let data = vec![0xcc, 0x48, 0x83, 0xec];
        assert!(pat.matches(&data, 0x1000, 0x1001));
        assert!(!pat.matches(&data, 0x1000, 0x1000));
        assert!(!pat.matches(&data, 0x1000, 0x1002));
    }

    #[test]
    fn test_parse_funcstart_extended_attributes() {
        let xml = r#"<patternlist>
          <pattern>
            <data>0xe92d4010</data>
            <funcstart after="ptr" validcode="10" validcodemax="20" contiguous="true" thunk="true"/>
          </pattern>
        </patternlist>"#;
        let pats = parse_ghidra_pattern_xml(xml).unwrap();
        assert_eq!(pats.len(), 1);
        assert_eq!(pats[0].after_cond.as_deref(), Some("ptr"));
        assert_eq!(pats[0].valid_code_min, Some(10));
        assert_eq!(pats[0].valid_code_max, Some(20));
        assert!(pats[0].contiguous);
        assert!(pats[0].is_thunk);
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
        assert_eq!(
            pats.len(),
            1,
            "expected 1 pattern, got {}: {pats:?}",
            pats.len()
        );
        assert_eq!(pats[0].pre_bytes, exact(&[0xcc]));
        assert_eq!(pats[0].post_bytes, exact(&[0x48, 0x83, 0xec]));
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
        assert_eq!(pats[0].pre_bytes, exact(&[0xcc]));
        assert_eq!(pats[0].post_bytes, exact(&[0x48, 0x83, 0xec]));
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
