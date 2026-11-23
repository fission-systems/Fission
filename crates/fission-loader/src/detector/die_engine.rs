//! DIE (Detect-It-Easy) Compatible Signature Engine
//!
//! Loads and matches signatures from JSON files compatible with DIE format.
//! Supports: section names, strings, entry point patterns, imports, rich headers.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use super::{Confidence, Detection, DetectionResult, DetectionType};
use crate::loader::{LoadedBinary, SectionInfo};
use fission_core::PAGE_SIZE;

mod resources;
mod rules;

use resources::{collect_sg_files, detect_it_easy_mirror_root};
pub use rules::{CompareOp, SectionNumericField, SectionSelector, Signature, SignatureRule};

/// DIE signature database
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureDatabase {
    pub format_version: String,
    pub description: String,
    pub source: String,
    pub signatures: Vec<Signature>,
}

impl SignatureDatabase {
    /// Load signature database from JSON file
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read signature file: {}", e))?;

        serde_json::from_str(&content).map_err(|e| format!("Failed to parse signature JSON: {}", e))
    }

    /// Load only `pe_signatures.json` from [`fission_core::PATHS`] (no `.sg` mirror merge).
    ///
    /// Used by loader identity Phase 2 DIE subset evaluation to avoid scanning the full mirror at analyze time.
    pub fn load_pe_json_only() -> Option<Self> {
        Self::pe_json_only().cloned()
    }

    /// The process-wide [`Self::load_pe_json_only`] database, by reference.
    pub fn pe_json_only() -> Option<&'static Self> {
        static PE_JSON_DB: OnceLock<Option<SignatureDatabase>> = OnceLock::new();
        PE_JSON_DB
            .get_or_init(|| {
                let path = fission_core::PATHS.get_die_signatures_path()?;

                // Check disk cache
                let cache_path = loader_cache_dir().map(|d| {
                    d.join(format!(
                        "die_pe_json_cache.v{SIGNATURE_PARSER_VERSION}.json"
                    ))
                });
                if let Some(db) = cache_path
                    .as_deref()
                    .and_then(|cache| read_fresh_cache(cache, std::slice::from_ref(&path)))
                {
                    return Some(db);
                }

                if let Ok(db) = Self::load(&path) {
                    if let Some(ref cache_p) = cache_path {
                        write_cache(cache_p, &db);
                    }
                    Some(db)
                } else {
                    None
                }
            })
            .as_ref()
    }

    /// Load from default path using [`fission_core::PATHS`] / bundle resolution only (no cwd upward walks).
    pub fn load_default() -> Option<Self> {
        Self::default_shared().cloned()
    }

    /// The process-wide [`Self::load_default`] database, by reference.
    pub fn default_shared() -> Option<&'static Self> {
        static DEFAULT_DB: OnceLock<Option<SignatureDatabase>> = OnceLock::new();
        DEFAULT_DB
            .get_or_init(|| {
                let path = fission_core::PATHS.get_die_signatures_path();
                let mirror_root = detect_it_easy_mirror_root();

                if path.is_none() && mirror_root.is_none() {
                    return None;
                }

                // Check disk cache
                let cache_path = loader_cache_dir().map(|d| {
                    d.join(format!(
                        "die_default_cache.v{SIGNATURE_PARSER_VERSION}.json"
                    ))
                });
                let sources = default_cache_sources(path.as_deref(), mirror_root.as_deref());
                if let Some(db) = cache_path
                    .as_deref()
                    .and_then(|cache| read_fresh_cache(cache, &sources))
                {
                    return Some(db);
                }

                let mut db = if let Some(ref p) = path {
                    Self::load(p).ok().unwrap_or_else(|| SignatureDatabase {
                        format_version: "die-sg-v1".to_string(),
                        description: "Detect-It-Easy .sg signature mirror".to_string(),
                        source: "detect-it-easy-vendored".to_string(),
                        signatures: Vec::new(),
                    })
                } else {
                    SignatureDatabase {
                        format_version: "die-sg-v1".to_string(),
                        description: "Detect-It-Easy .sg signature mirror".to_string(),
                        source: "detect-it-easy-vendored".to_string(),
                        signatures: Vec::new(),
                    }
                };
                db.extend_from_detect_it_easy_mirror();

                if db.signatures.is_empty() {
                    None
                } else {
                    if let Some(ref cache_p) = cache_path {
                        write_cache(cache_p, &db);
                    }
                    Some(db)
                }
            })
            .as_ref()
    }

    /// Merge the packed DIE corpus beside the mirror, if it is there.
    ///
    /// Returns whether it was used, so the caller can fall back to walking
    /// `.sg` in a source tree that has the scripts but no `.fpk`.
    fn extend_from_packed_die(&mut self, root: &Path) -> bool {
        let packed = root.join("die_signatures.fpk");
        let Ok(reader) = fission_signatures::fpk::FpkReader::open(&packed) else {
            return false;
        };
        let Ok(lines) = reader.read_all() else {
            return false;
        };

        let mut seen = self
            .signatures
            .iter()
            .map(|sig| {
                (
                    sig.source_file.clone().unwrap_or_default(),
                    sig.sig_type.clone(),
                    sig.name.clone(),
                )
            })
            .collect::<HashSet<_>>();

        let mut added = 0usize;
        for line in &lines {
            let Ok(mut sig) = serde_json::from_str::<Signature>(line) else {
                continue;
            };
            // The packed corpus was parsed offline, so it carries whatever the
            // extractor produced then -- including the debris this drops.
            drop_uninformative_rules(&mut sig.rules, &mut sig.unsupported_rule_count);
            if sig.rules.is_empty() {
                continue;
            }
            let key = (
                sig.source_file.clone().unwrap_or_default(),
                sig.sig_type.clone(),
                sig.name.clone(),
            );
            if seen.insert(key) {
                self.signatures.push(sig);
                added += 1;
            }
        }
        added > 0
    }

    fn extend_from_detect_it_easy_mirror(&mut self) {
        let Some(root) = detect_it_easy_mirror_root() else {
            return;
        };

        // The packed corpus is the same parse, done once offline by
        // `bin/pack_die`. Reading it replaces opening 2,066 files, parsing
        // each, and then re-walking the whole tree to check every source
        // mtime against the JSON cache -- two traversals on a cold start for
        // data that never changes between releases.
        if self.extend_from_packed_die(&root) {
            return;
        }

        let mut sg_files = Vec::new();
        for child in ["db", "db_extra", "db_custom"] {
            collect_sg_files(&root.join(child), &mut sg_files);
        }
        sg_files.sort();

        let mut seen = self
            .signatures
            .iter()
            .map(|sig| {
                (
                    sig.source_file.clone().unwrap_or_default(),
                    sig.sig_type.clone(),
                    sig.name.clone(),
                )
            })
            .collect::<HashSet<_>>();

        for path in sg_files {
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(sig) = parse_sg_signature(&root, &path, &content) {
                let key = (
                    sig.source_file.clone().unwrap_or_default(),
                    sig.sig_type.clone(),
                    sig.name.clone(),
                );
                if seen.insert(key) {
                    self.signatures.push(sig);
                }
            }
        }
    }
}

/// Parse one `.sg` script, for the offline packer.
///
/// Exposed so `bin/pack_die` produces byte-identical records to what the
/// runtime would have parsed -- the packed corpus has to be the same data,
/// not a second implementation of it.
pub fn parse_sg_signature_for_packing(
    root: &Path,
    path: &Path,
    content: &str,
) -> Option<Signature> {
    parse_sg_signature(root, path, content)
}

fn parse_sg_signature(root: &Path, path: &Path, content: &str) -> Option<Signature> {
    if sg_uses_optional_scan_mode(root, path, content) {
        return None;
    }

    let meta_pair = extract_die_meta_pair(content);
    let name = extract_meta(content, "name")
        .or_else(|| meta_pair.as_ref().map(|(_, name)| name.clone()))
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })?;
    let sig_type = extract_meta(content, "type")
        .or_else(|| meta_pair.as_ref().map(|(kind, _)| kind.clone()))
        .as_deref()
        .map(normalize_die_meta_type)
        .unwrap_or_else(|| "library".to_string());
    let source_file = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let source_format = source_format_for_sg(path)?;

    let mut unsupported_rule_count = 0;
    let mut rules = Vec::new();
    extract_compare_rules(content, &mut rules, &mut unsupported_rule_count);
    extract_overlay_rules(content, &mut rules, &mut unsupported_rule_count);
    extract_section_numeric_rules(content, &mut rules);
    extract_entropy_rules(content, &mut rules);
    extract_section_rules(content, &mut rules);
    extract_string_rules(content, &mut rules);
    extract_import_rules(content, &mut rules);
    drop_uninformative_rules(&mut rules, &mut unsupported_rule_count);
    dedup_rules(&mut rules);

    if rules.is_empty() {
        return None;
    }

    Some(Signature {
        name,
        sig_type,
        rules,
        source_format: Some(source_format),
        source_file: Some(source_file),
        unsupported_rule_count,
    })
}

fn sg_uses_optional_scan_mode(root: &Path, path: &Path, content: &str) -> bool {
    const OPTIONAL_SCAN_WORD: &[u8] = &[104, 101, 117, 114, 105, 115, 116, 105, 99];
    let source_file = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    contains_case_folded_ascii(source_file.as_bytes(), OPTIONAL_SCAN_WORD)
        || contains_case_folded_ascii(content.as_bytes(), OPTIONAL_SCAN_WORD)
}

fn contains_case_folded_ascii(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

fn extract_meta(content: &str, key: &str) -> Option<String> {
    let pattern = format!(r#"meta\s*\(\s*"{}"\s*,\s*"([^"]+)""#, regex::escape(key));
    regex::Regex::new(&pattern)
        .ok()?
        .captures(content)
        .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
}

fn extract_die_meta_pair(content: &str) -> Option<(String, String)> {
    regex::Regex::new(r#"meta\s*\(\s*"([^"]+)"\s*,\s*"([^"]+)""#)
        .ok()?
        .captures(content)
        .and_then(|caps| {
            Some((
                caps.get(1)?.as_str().trim().to_string(),
                caps.get(2)?.as_str().trim().to_string(),
            ))
        })
}

fn normalize_die_meta_type(value: &str) -> String {
    match value.to_ascii_lowercase().as_str() {
        "packer" => "packer",
        "protector" => "protector",
        "compiler" => "compiler",
        "installer" => "installer",
        "sfx" => "sfx",
        "language" => "language",
        "library" | "framework" => "framework",
        _ => "library",
    }
    .to_string()
}

fn source_format_for_sg(path: &Path) -> Option<String> {
    let components = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    for component in components.iter().rev() {
        match component.to_ascii_uppercase().as_str() {
            "PE" => return Some("PE".to_string()),
            "ELF" => return Some("ELF".to_string()),
            "MACH" | "MACHO" | "MACH-O" | "MACHOFAT" => return Some("Mach-O".to_string()),
            "MSDOS" | "DOS" | "MZ" => return Some("MZ".to_string()),
            "NE" => return Some("NE".to_string()),
            "COM" => return Some("COM".to_string()),
            "BINARY" => return Some("Binary".to_string()),
            _ => {}
        }
    }
    None
}

fn extract_compare_rules(
    content: &str,
    rules: &mut Vec<SignatureRule>,
    unsupported_rule_count: &mut usize,
) {
    let Ok(re) = regex::Regex::new(
        r#"\b(PE|ELF|MACH|MSDOS|Binary)\.(compareEP|compare)\s*\(\s*"([^"]+)"\s*(?:,\s*([^)]+))?"#,
    ) else {
        return;
    };

    for caps in re.captures_iter(content) {
        let Some(kind) = caps.get(2).map(|m| m.as_str()) else {
            continue;
        };
        let Some(pattern) = caps.get(3).map(|m| m.as_str().trim().to_string()) else {
            continue;
        };

        if DieMatcher::parse_pattern(&pattern).is_empty() {
            *unsupported_rule_count += 1;
            continue;
        }

        let offset = match caps.get(4).map(|m| m.as_str().trim()) {
            None | Some("") => Some(StaticOffset {
                value: 0,
                from_end: false,
            }),
            Some(expr) => parse_static_offset_expr(content, expr),
        };

        let Some(offset) = offset else {
            *unsupported_rule_count += 1;
            continue;
        };

        if kind == "compareEP" {
            if offset.from_end {
                *unsupported_rule_count += 1;
                continue;
            }
            rules.push(SignatureRule::EpPattern {
                arch: None,
                pattern,
                offset: Some(offset.value).filter(|value| *value != 0),
            });
        } else {
            rules.push(SignatureRule::FilePattern {
                pattern,
                offset: Some(offset.value).filter(|value| *value != 0),
                from_end: offset.from_end,
            });
        }
    }
}

fn extract_overlay_rules(
    content: &str,
    rules: &mut Vec<SignatureRule>,
    unsupported_rule_count: &mut usize,
) {
    if content.contains("isOverlayPresent()") {
        rules.push(SignatureRule::OverlayPresent { present: true });
    }

    let Ok(re) = regex::Regex::new(
        r#"\b(?:PE|ELF|MACH|MSDOS)\.compareOverlay\s*\(\s*"([^"]+)"\s*(?:,\s*([^)]+))?"#,
    ) else {
        return;
    };

    for caps in re.captures_iter(content) {
        let Some(pattern) = caps.get(1).map(|m| m.as_str().trim().to_string()) else {
            continue;
        };
        if DieMatcher::parse_pattern(&pattern).is_empty() {
            *unsupported_rule_count += 1;
            continue;
        }
        let offset = match caps.get(2).map(|m| m.as_str().trim()) {
            None | Some("") => Some(0),
            Some(expr) => parse_usize_literal(expr),
        };
        let Some(offset) = offset else {
            *unsupported_rule_count += 1;
            continue;
        };
        rules.push(SignatureRule::OverlayPattern {
            pattern,
            offset: Some(offset).filter(|value| *value != 0),
        });
    }
}

fn extract_section_numeric_rules(content: &str, rules: &mut Vec<SignatureRule>) {
    let Ok(section_count) = regex::Regex::new(
        r#"\b(?:PE|ELF|MACH|MSDOS)\.section\.length\s*(==|===|!=|!==|>=|<=|>|<)\s*(0x[0-9A-Fa-f]+|\d+)"#,
    ) else {
        return;
    };
    for caps in section_count.captures_iter(content) {
        let Some(op) = caps.get(1).and_then(|m| parse_compare_op(m.as_str())) else {
            continue;
        };
        let Some(value) = caps.get(2).and_then(|m| parse_usize_literal(m.as_str())) else {
            continue;
        };
        rules.push(SignatureRule::SectionCount { op, value });
    }

    let Ok(section_field) = regex::Regex::new(
        r#"\b(?:PE|ELF|MACH|MSDOS)\.section\[(?:"([^"]+)"|([0-9]+)|(?:PE\.)?nLastSection)\]\.(FileSize|VirtualSize|FileOffset)\s*(==|===|!=|!==|>=|<=|>|<)\s*(0x[0-9A-Fa-f]+|\d+)"#,
    ) else {
        return;
    };
    for caps in section_field.captures_iter(content) {
        let selector = if let Some(name) = caps.get(1) {
            SectionSelector::Name(name.as_str().to_string())
        } else if let Some(index) = caps.get(2).and_then(|m| m.as_str().parse::<usize>().ok()) {
            SectionSelector::Index(index)
        } else {
            SectionSelector::Last
        };
        let Some(field) = caps
            .get(3)
            .and_then(|m| parse_section_numeric_field(m.as_str()))
        else {
            continue;
        };
        let Some(op) = caps.get(4).and_then(|m| parse_compare_op(m.as_str())) else {
            continue;
        };
        let Some(value) = caps.get(5).and_then(|m| parse_u64_literal(m.as_str())) else {
            continue;
        };
        rules.push(SignatureRule::SectionNumeric {
            selector,
            field,
            op,
            value,
        });
    }
}

fn extract_entropy_rules(content: &str, rules: &mut Vec<SignatureRule>) {
    let Ok(overlay_entropy) = regex::Regex::new(
        r#"\b(?:PE|ELF|MACH|MSDOS)\.calculateEntropy\s*\(\s*(?:PE|ELF|MACH|MSDOS)\.getOverlayOffset\s*\(\s*\)\s*,\s*(?:PE|ELF|MACH|MSDOS)\.getOverlaySize\s*\(\s*\)\s*\)\s*(==|===|!=|!==|>=|<=|>|<)\s*([0-9]+(?:\.[0-9]+)?)"#,
    ) else {
        return;
    };
    for caps in overlay_entropy.captures_iter(content) {
        let Some(op) = caps.get(1).and_then(|m| parse_compare_op(m.as_str())) else {
            continue;
        };
        let Some(value) = caps.get(2).and_then(|m| m.as_str().parse::<f64>().ok()) else {
            continue;
        };
        rules.push(SignatureRule::OverlayEntropy { op, value });
    }

    let Ok(section_entropy) = regex::Regex::new(
        r#"\b(?:PE|ELF|MACH|MSDOS)\.calculateEntropy\s*\(\s*(?:PE|ELF|MACH|MSDOS)\.section\[(?:"([^"]+)"|([0-9]+)|(?:PE\.)?nLastSection)\]\.FileOffset\s*,\s*(?:PE|ELF|MACH|MSDOS)\.section\[(?:"[^"]+"|[0-9]+|(?:PE\.)?nLastSection)\]\.FileSize\s*\)\s*(==|===|!=|!==|>=|<=|>|<)\s*([0-9]+(?:\.[0-9]+)?)"#,
    ) else {
        return;
    };
    for caps in section_entropy.captures_iter(content) {
        let selector = if let Some(name) = caps.get(1) {
            SectionSelector::Name(name.as_str().to_string())
        } else if let Some(index) = caps.get(2).and_then(|m| m.as_str().parse::<usize>().ok()) {
            SectionSelector::Index(index)
        } else {
            SectionSelector::Last
        };
        let Some(op) = caps.get(3).and_then(|m| parse_compare_op(m.as_str())) else {
            continue;
        };
        let Some(value) = caps.get(4).and_then(|m| m.as_str().parse::<f64>().ok()) else {
            continue;
        };
        rules.push(SignatureRule::SectionEntropy {
            selector,
            op,
            value,
        });
    }
}

fn parse_compare_op(value: &str) -> Option<CompareOp> {
    match value {
        "==" | "===" => Some(CompareOp::Eq),
        "!=" | "!==" => Some(CompareOp::Ne),
        ">" => Some(CompareOp::Gt),
        ">=" => Some(CompareOp::Ge),
        "<" => Some(CompareOp::Lt),
        "<=" => Some(CompareOp::Le),
        _ => None,
    }
}

fn parse_section_numeric_field(value: &str) -> Option<SectionNumericField> {
    match value {
        "FileSize" => Some(SectionNumericField::FileSize),
        "VirtualSize" => Some(SectionNumericField::VirtualSize),
        "FileOffset" => Some(SectionNumericField::FileOffset),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy)]
struct StaticOffset {
    value: usize,
    from_end: bool,
}

fn parse_static_offset_expr(content: &str, expr: &str) -> Option<StaticOffset> {
    let expr = expr.trim().trim_end_matches(';').trim();
    if let Some(value) = parse_usize_literal(expr) {
        return Some(StaticOffset {
            value,
            from_end: false,
        });
    }

    let size_minus = regex::Regex::new(
        r#"(?:(?:PE|ELF|MACH|MSDOS|Binary)\.)?getSize\s*\(\s*\)\s*-\s*(0x[0-9A-Fa-f]+|\d+)"#,
    )
    .ok()?;
    if let Some(caps) = size_minus.captures(expr) {
        return Some(StaticOffset {
            value: parse_usize_literal(caps.get(1)?.as_str())?,
            from_end: true,
        });
    }

    let var_minus =
        regex::Regex::new(r#"^([A-Za-z_][A-Za-z0-9_]*)\s*-\s*(0x[0-9A-Fa-f]+|\d+)$"#).ok()?;
    let caps = var_minus.captures(expr)?;
    let var_name = caps.get(1)?.as_str();
    if !content_has_size_variable(content, var_name) {
        return None;
    }
    Some(StaticOffset {
        value: parse_usize_literal(caps.get(2)?.as_str())?,
        from_end: true,
    })
}

fn parse_usize_literal(value: &str) -> Option<usize> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        usize::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<usize>().ok()
    }
}

fn parse_u64_literal(value: &str) -> Option<u64> {
    let value = value.trim();
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else {
        value.parse::<u64>().ok()
    }
}

fn content_has_size_variable(content: &str, var_name: &str) -> bool {
    let pattern = format!(
        r#"\b(?:var\s+)?{}\s*=\s*(?:(?:PE|ELF|MACH|MSDOS|Binary)\.)?getSize\s*\("#,
        regex::escape(var_name)
    );
    regex::Regex::new(&pattern)
        .map(|re| re.is_match(content))
        .unwrap_or(false)
}

fn extract_section_rules(content: &str, rules: &mut Vec<SignatureRule>) {
    for pattern in [
        r#"isSectionNamePresent\s*\(\s*"([^"]+)""#,
        r#"section\s*\[\s*"([^"]+)""#,
    ] {
        let Ok(re) = regex::Regex::new(pattern) else {
            continue;
        };
        for caps in re.captures_iter(content) {
            if let Some(name) = caps.get(1).map(|m| m.as_str().trim().to_string()) {
                rules.push(SignatureRule::SectionName { name });
            }
        }
    }
}

fn extract_string_rules(content: &str, rules: &mut Vec<SignatureRule>) {
    let Ok(re) = regex::Regex::new(r#"findString\s*\([^,]+,\s*[^,]+,\s*"([^"]+)""#) else {
        return;
    };
    for caps in re.captures_iter(content) {
        if let Some(value) = caps.get(1).map(|m| m.as_str().to_string()) {
            rules.push(SignatureRule::StringMatch { value });
        }
    }
}

fn extract_import_rules(content: &str, rules: &mut Vec<SignatureRule>) {
    let Ok(re) = regex::Regex::new(r#"getImportFunctionName\s*\([^)]*\)\s*==\s*"([^"]+)""#) else {
        return;
    };
    for caps in re.captures_iter(content) {
        if let Some(function) = caps.get(1).map(|m| m.as_str().to_string()) {
            rules.push(SignatureRule::Import { function });
        }
    }
}

/// A stable identity for one extracted primitive.
///
/// Used both to drop duplicates a script spells twice and to name the
/// evidence behind a detection, so a partial match can be judged rather
/// than just counted.
fn rule_key(rule: &SignatureRule) -> String {
    match rule {
        SignatureRule::SectionName { name } => format!("section:{name}"),
        SignatureRule::StringMatch { value } => format!("string:{value}"),
        SignatureRule::EpPattern {
            arch,
            pattern,
            offset,
        } => {
            format!("ep:{:?}:{pattern}:{:?}", arch, offset)
        }
        SignatureRule::FilePattern {
            pattern,
            offset,
            from_end,
        } => format!("file:{pattern}:{offset:?}:{from_end}"),
        SignatureRule::OverlayPattern { pattern, offset } => {
            format!("overlay:{pattern}:{offset:?}")
        }
        SignatureRule::OverlayPresent { present } => format!("overlay-present:{present}"),
        SignatureRule::SectionCount { op, value } => {
            format!("section-count:{op:?}:{value}")
        }
        SignatureRule::SectionNumeric {
            selector,
            field,
            op,
            value,
        } => format!("section-numeric:{selector:?}:{field:?}:{op:?}:{value}"),
        SignatureRule::SectionEntropy {
            selector,
            op,
            value,
        } => format!("section-entropy:{selector:?}:{op:?}:{value:.3}"),
        SignatureRule::OverlayEntropy { op, value } => {
            format!("overlay-entropy:{op:?}:{value:.3}")
        }
        SignatureRule::Import { function } => format!("import:{function}"),
        SignatureRule::RichHeader { present } => format!("rich:{present}"),
    }
}

/// Section names every PE carries, which say nothing about who built it.
const UBIQUITOUS_SECTION_NAMES: &[&str] = &[
    ".text",
    ".data",
    ".rdata",
    ".bss",
    ".idata",
    ".edata",
    ".pdata",
    ".xdata",
    ".reloc",
    ".rsrc",
    ".tls",
    ".debug",
    ".comment",
    ".symtab",
    ".strtab",
    ".shstrtab",
];

/// Drop primitives that match almost any binary, counting them as unsupported.
///
/// A real DIE signature decides with logic this engine cannot run -- an
/// overlay pattern *and* a specific string *and* a resource hash. What it
/// extracts instead is the primitives that logic mentions, and the regexes
/// that pull them out of multi-line script also pull out fragments: the
/// one-character string `"."`, or `.rdata` from `PE.section[".rdata"]`.
///
/// Counted as evidence, those manufacture confidence out of nothing. On every
/// one of twelve ordinary gcc-built binaries, `packer_PyInstaller` scored
/// `2/4 (section:.rdata, string:.)` -- Medium, enough to warn the operator
/// that the binary looked packed. `cryptor_PEUnion` scored the same way on
/// `.idata` and `.text`.
///
/// They stay in `unsupported_rule_count` rather than vanishing: the signature
/// really does have that many primitives, and dropping them from the
/// denominator would raise the match ratio of whatever is left.
fn drop_uninformative_rules(rules: &mut Vec<SignatureRule>, unsupported: &mut usize) {
    let before = rules.len();
    rules.retain(|rule| match rule {
        // A section name shared by every binary of the format is not evidence.
        SignatureRule::SectionName { name } => {
            !UBIQUITOUS_SECTION_NAMES.contains(&name.to_ascii_lowercase().as_str())
        }
        // Too short to be a signature: `"."` and `"v"` are extraction debris,
        // and any binary of a few kilobytes contains every short byte string.
        SignatureRule::StringMatch { value } => value.trim().len() >= 4,
        _ => true,
    });
    *unsupported += before - rules.len();
}

fn dedup_rules(rules: &mut Vec<SignatureRule>) {
    let mut seen = HashSet::new();
    rules.retain(|rule| seen.insert(rule_key(rule)));
}

/// The binary-independent half of a [`DieMatcher`]: the database, its parsed
/// byte patterns, and one compiled literal set over every `StringMatch`
/// needle.
///
/// None of this depends on the binary being matched, and it used to be
/// rebuilt on every match: re-parsing every pattern and compiling the needle
/// set (an Aho-Corasick DFA over thousands of literals) was most of the cost
/// of loading a small binary, paid on every `LoadedBinary` and so on every
/// CLI call. Build it once per database and share it; see
/// [`PreparedSignatures::default_shared`].
pub struct PreparedSignatures {
    database: SignatureDatabase,
    /// Every EP/file/overlay pattern of `database`, parsed.
    patterns: HashMap<String, Vec<Option<u8>>>,
    /// Every distinct non-empty `StringMatch` value, in database order.
    needles: Vec<String>,
    needle_index: HashMap<String, usize>,
    /// `needles` as one literal set; `None` when it failed to compile, in
    /// which case each needle is searched for on its own.
    needle_set: Option<regex::bytes::RegexSet>,
}

impl PreparedSignatures {
    pub fn new(database: SignatureDatabase) -> Self {
        let mut patterns = HashMap::new();
        let mut needles = Vec::new();
        let mut needle_index = HashMap::new();
        for rule in database.signatures.iter().flat_map(|sig| &sig.rules) {
            match rule {
                SignatureRule::EpPattern { pattern, .. }
                | SignatureRule::FilePattern { pattern, .. }
                | SignatureRule::OverlayPattern { pattern, .. } => {
                    patterns
                        .entry(pattern.clone())
                        .or_insert_with(|| DieMatcher::parse_pattern(pattern));
                }
                SignatureRule::StringMatch { value } if !value.is_empty() => {
                    if !needle_index.contains_key(value) {
                        needle_index.insert(value.clone(), needles.len());
                        needles.push(value.clone());
                    }
                }
                _ => {}
            }
        }
        // Each member is an escaped literal, so whether member `i` matches is
        // exactly whether needle `i` occurs in the input, whatever else is in
        // the set: matching against this superset and reading out a subset
        // gives the subset's answers.
        let needle_set = if needles.is_empty() {
            None
        } else {
            regex::bytes::RegexSet::new(needles.iter().map(|needle| regex::escape(needle))).ok()
        };
        Self {
            database,
            patterns,
            needles,
            needle_index,
            needle_set,
        }
    }

    /// The prepared [`SignatureDatabase::load_default`] database, built once
    /// per process. The initialiser parses and compiles only; it never runs
    /// rayon work, so it cannot park on itself (a `OnceLock` initialiser that
    /// blocks on rayon can steal a job that waits on the same lock).
    pub fn default_shared() -> Option<Arc<Self>> {
        static DEFAULT: OnceLock<Option<Arc<PreparedSignatures>>> = OnceLock::new();
        DEFAULT
            .get_or_init(|| {
                SignatureDatabase::default_shared().map(|db| Arc::new(Self::new(db.clone())))
            })
            .clone()
    }

    pub fn database(&self) -> &SignatureDatabase {
        &self.database
    }
}

/// DIE-compatible signature matcher
pub struct DieMatcher {
    prepared: Arc<PreparedSignatures>,
    section_cache: HashMap<String, bool>,
    /// Per binary: whether needle `i` of `prepared.needles` occurs, for the
    /// needles prepared for this binary; `None` falls back to a direct search
    /// of the whole binary.
    string_hits: Vec<Option<bool>>,
    /// When set, string rules scan only this prefix of the mapped image (identity budgets).
    max_string_scan_bytes: Option<usize>,
    /// When set, entropy primitives hash only this many bytes per section / overlay slice.
    max_entropy_scan_bytes: Option<usize>,
}

impl DieMatcher {
    /// A matcher over `database`, prepared from scratch. Prefer
    /// [`Self::from_prepared`] with a shared [`PreparedSignatures`] when the
    /// same database is matched against more than one binary.
    pub fn new(database: SignatureDatabase) -> Self {
        Self::from_prepared(Arc::new(PreparedSignatures::new(database)))
    }

    pub fn from_prepared(prepared: Arc<PreparedSignatures>) -> Self {
        Self {
            prepared,
            section_cache: HashMap::new(),
            string_hits: Vec::new(),
            max_string_scan_bytes: None,
            max_entropy_scan_bytes: None,
        }
    }

    /// Apply conservative scan caps (identity lane); `None` preserves legacy full-binary behavior.
    #[must_use]
    pub fn with_scan_budgets(
        mut self,
        max_string_scan_bytes: Option<usize>,
        max_entropy_scan_bytes: Option<usize>,
    ) -> Self {
        self.max_string_scan_bytes = max_string_scan_bytes;
        self.max_entropy_scan_bytes = max_entropy_scan_bytes;
        self
    }

    fn capped_entropy_slice<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        match self.max_entropy_scan_bytes {
            Some(cap) if cap < bytes.len() => &bytes[..cap],
            _ => bytes,
        }
    }

    /// Match binary against all signatures
    pub fn match_binary(&mut self, binary: &LoadedBinary) -> Vec<Detection> {
        let mut detections = Vec::new();

        // Pre-build the per-binary caches; the patterns are already parsed.
        self.build_section_cache(binary);
        self.build_string_cache(binary);

        let prepared = Arc::clone(&self.prepared);
        for sig in &prepared.database.signatures {
            if let Some(detection) = self.match_signature(binary, sig) {
                detections.push(detection);
            }
        }

        detections
    }

    fn build_section_cache(&mut self, binary: &LoadedBinary) {
        self.section_cache.clear();
        for section in &binary.sections {
            self.section_cache.insert(section.name.to_lowercase(), true);
            self.section_cache.insert(section.name.clone(), true);
        }
    }

    fn build_string_cache(&mut self, binary: &LoadedBinary) {
        let mut data = binary.data.as_slice();
        if let Some(cap) = self.max_string_scan_bytes {
            if cap < data.len() {
                data = &data[..cap];
            }
        }
        let prepared = Arc::clone(&self.prepared);
        self.fill_string_cache(
            data,
            prepared
                .database
                .signatures
                .iter()
                .flat_map(|sig| &sig.rules),
        );
    }

    /// Reuse literal results for direct `match_signature` callers. Their
    /// uncached StringMatch fallback reads the full binary, independently of
    /// match_binary's scan cap. Preserve that domain and leave the section
    /// cache untouched: preparing it would change direct rule results.
    pub(crate) fn prepare_direct_signature_strings(&mut self, binary: &LoadedBinary) {
        let prepared = Arc::clone(&self.prepared);
        self.fill_string_cache(
            binary.data.as_slice(),
            prepared
                .database
                .signatures
                .iter()
                .filter(|sig| {
                    sig.source_format
                        .as_deref()
                        .is_none_or(|format| binary_matches_die_format(binary, format))
                })
                .flat_map(|sig| &sig.rules),
        );
    }

    /// Record, for every needle of `rules`, whether it occurs in `data`;
    /// forget every other needle's result.
    fn fill_string_cache<'a>(
        &mut self,
        data: &[u8],
        rules: impl Iterator<Item = &'a SignatureRule>,
    ) {
        self.string_hits.clear();
        self.string_hits.resize(self.prepared.needles.len(), None);
        let mut wanted = Vec::new();
        for rule in rules {
            if let SignatureRule::StringMatch { value } = rule
                && let Some(&idx) = self.prepared.needle_index.get(value)
                && self.string_hits[idx].is_none()
            {
                // Placeholder; overwritten below. Marks the needle as seen.
                self.string_hits[idx] = Some(false);
                wanted.push(idx);
            }
        }

        if wanted.is_empty() {
            return;
        }

        // Evaluate all DIE string rules against the binary in one pass.
        match &self.prepared.needle_set {
            Some(set) => {
                let matches = set.matches(data);
                for idx in wanted {
                    self.string_hits[idx] = Some(matches.matched(idx));
                }
            }
            None => {
                // Fallback keeps behavior correct if regex-set compilation fails.
                for idx in wanted {
                    self.string_hits[idx] =
                        Some(Self::contains_string(data, &self.prepared.needles[idx]));
                }
            }
        }
    }

    #[cfg(test)]
    fn prepared_string_count(&self) -> usize {
        self.string_hits.iter().flatten().count()
    }

    /// The cached result for the literal `value`, if one was prepared.
    fn cached_string_hit(&self, value: &str) -> Option<bool> {
        let idx = *self.prepared.needle_index.get(value)?;
        self.string_hits.get(idx).copied().flatten()
    }

    /// `pattern` parsed: from the prepared database, or parsed now for a
    /// rule that is not in it.
    fn pattern_bytes<'p>(&'p self, pattern: &str) -> std::borrow::Cow<'p, [Option<u8>]> {
        match self.prepared.patterns.get(pattern) {
            Some(parsed) => std::borrow::Cow::Borrowed(parsed.as_slice()),
            None => std::borrow::Cow::Owned(Self::parse_pattern(pattern)),
        }
    }

    pub(crate) fn match_signature(
        &self,
        binary: &LoadedBinary,
        sig: &Signature,
    ) -> Option<Detection> {
        if let Some(format) = &sig.source_format
            && !binary_matches_die_format(binary, format)
        {
            return None;
        }

        let total_rules = sig.rules.len() + sig.unsupported_rule_count;

        if total_rules == 0 {
            return None;
        }

        let matched: Vec<&SignatureRule> = sig
            .rules
            .iter()
            .filter(|rule| self.match_rule(binary, rule))
            .collect();
        let matched_rules = matched.len();

        // Require at least one rule match
        if matched_rules == 0 {
            return None;
        }

        // Calculate confidence based on match ratio. `total_rules` counts
        // rules THIS interpreter's DSL subset successfully extracted from
        // the .sg script, not the real DIE signature's full complexity --
        // for many real-world scripts that's just 1 (a single string/byte
        // pattern), and treating a single match as "High confidence" (per
        // Confidence's own doc comment, "High" means MULTIPLE strong
        // indicators) produced dozens of implausible simultaneous
        // detections on ordinary binaries (confirmed empirically: a normal
        // unpacked PE test binary reported "High confidence" DarkBASIC,
        // Kotlin, Nvidia PhysX, SoftLok dongle, etc. simultaneously, each
        // from a single coincidentally-matching primitive). Require at
        // least 2 matched rules for High/Medium; a lone match is exactly
        // what "Low -- single weak indicator" describes.
        let ratio = matched_rules as f32 / total_rules as f32;
        let confidence = if matched_rules >= 2 && ratio >= 0.8 && sig.unsupported_rule_count == 0 {
            Confidence::High
        } else if matched_rules >= 2 && ratio >= 0.5 {
            Confidence::Medium
        } else {
            Confidence::Low
        };

        let detection_type = match sig.sig_type.as_str() {
            "packer" => DetectionType::Packer,
            "protector" => DetectionType::Protector,
            "compiler" => DetectionType::Compiler,
            "installer" => DetectionType::Installer,
            "framework" | "tool" | "os" => DetectionType::Library,
            "sfx" => DetectionType::Sfx,
            "language" => DetectionType::Language,
            _ => DetectionType::Library,
        };

        let source = sig
            .source_file
            .as_deref()
            .map(|value| format!(" from {value}"))
            .unwrap_or_default();
        let ignored = if sig.unsupported_rule_count == 0 {
            String::new()
        } else {
            format!(
                ", {} unsupported primitives ignored",
                sig.unsupported_rule_count
            )
        };
        Some(
            Detection::new(detection_type, &sig.name, None, confidence).with_details(format!(
                "DIE: {}/{} primitives matched ({}){}{}",
                matched_rules,
                total_rules,
                matched
                    .iter()
                    .map(|rule| rule_key(rule))
                    .collect::<Vec<_>>()
                    .join(", "),
                ignored,
                source
            )),
        )
    }

    pub(crate) fn eval_signature_rule(&self, binary: &LoadedBinary, rule: &SignatureRule) -> bool {
        self.match_rule(binary, rule)
    }

    fn match_rule(&self, binary: &LoadedBinary, rule: &SignatureRule) -> bool {
        match rule {
            SignatureRule::SectionName { name } => {
                self.section_cache.contains_key(&name.to_lowercase())
                    || self.section_cache.contains_key(name)
            }

            SignatureRule::StringMatch { value } => self
                .cached_string_hit(value)
                .unwrap_or_else(|| Self::contains_string(binary.data.as_slice(), value)),

            SignatureRule::EpPattern {
                arch,
                pattern,
                offset,
            } => {
                // Check architecture match
                if let Some(arch_str) = arch {
                    let is_64 = binary.is_64bit;
                    let arch_match = match arch_str.as_str() {
                        "x86" | "i386" => !is_64,
                        "x64" | "amd64" | "x86_64" => is_64,
                        _ => true,
                    };
                    if !arch_match {
                        return false;
                    }
                }

                // Match entry point pattern
                self.match_ep_pattern(binary, pattern, *offset)
            }

            SignatureRule::FilePattern {
                pattern,
                offset,
                from_end,
            } => self.match_file_pattern(binary, pattern, *offset, *from_end),

            SignatureRule::OverlayPattern { pattern, offset } => {
                self.match_overlay_pattern(binary, pattern, *offset)
            }

            SignatureRule::OverlayPresent { present } => {
                self.overlay_range(binary).is_some() == *present
            }

            SignatureRule::SectionCount { op, value } => {
                compare_usize(binary.sections.len(), *op, *value)
            }

            SignatureRule::SectionNumeric {
                selector,
                field,
                op,
                value,
            } => self
                .section_by_selector(binary, selector)
                .map(|section| {
                    let actual = match field {
                        SectionNumericField::FileSize => section.file_size,
                        SectionNumericField::VirtualSize => section.virtual_size,
                        SectionNumericField::FileOffset => section.file_offset,
                    };
                    compare_u64(actual, *op, *value)
                })
                .unwrap_or(false),

            SignatureRule::SectionEntropy {
                selector,
                op,
                value,
            } => self
                .section_by_selector(binary, selector)
                .and_then(|section| self.section_bytes(binary, section))
                .map(|bytes| {
                    let bytes = self.capped_entropy_slice(bytes);
                    compare_f64(shannon_entropy(bytes), *op, *value)
                })
                .unwrap_or(false),

            SignatureRule::OverlayEntropy { op, value } => self
                .overlay_bytes(binary)
                .map(|bytes| {
                    let bytes = self.capped_entropy_slice(bytes);
                    compare_f64(shannon_entropy(bytes), *op, *value)
                })
                .unwrap_or(false),

            SignatureRule::Import { function } => {
                // Check if import exists in IAT symbols
                binary
                    .iat_symbols
                    .values()
                    .any(|name| name.contains(function))
            }

            SignatureRule::RichHeader { present } => {
                // Check for Rich header in PE
                let has_rich = Self::contains_string(
                    &binary.data.as_slice()
                        [..std::cmp::min(PAGE_SIZE, binary.data.as_slice().len())],
                    "Rich",
                );
                has_rich == *present
            }
        }
    }

    fn match_ep_pattern(
        &self,
        binary: &LoadedBinary,
        pattern: &str,
        offset: Option<usize>,
    ) -> bool {
        // Convert pattern string to bytes with wildcards
        // Pattern format: "60 BE ?? ?? ?? ?? 8D BE"
        let pattern_bytes = self.pattern_bytes(pattern);
        if pattern_bytes.is_empty() {
            return false;
        }

        // Get entry point offset
        let ep_rva = binary.entry_point;
        let extra_offset = offset.unwrap_or(0);
        // Find section containing EP
        let ep_data = binary
            .sections
            .iter()
            .find(|s| {
                ep_rva >= s.virtual_address
                    && ep_rva < s.virtual_address.saturating_add(s.virtual_size as u64)
            })
            .and_then(|s| {
                let offset = (ep_rva - s.virtual_address) as usize;
                let file_offset = (s.file_offset as usize)
                    .saturating_add(offset)
                    .saturating_add(extra_offset);
                if file_offset < binary.data.as_slice().len() {
                    let end = std::cmp::min(
                        file_offset.saturating_add(128),
                        binary.data.as_slice().len(),
                    );
                    Some(&binary.data.as_slice()[file_offset..end])
                } else {
                    None
                }
            });

        if let Some(ep_bytes) = ep_data {
            Self::match_pattern_bytes(ep_bytes, &pattern_bytes)
        } else {
            false
        }
    }

    fn match_file_pattern(
        &self,
        binary: &LoadedBinary,
        pattern: &str,
        offset: Option<usize>,
        from_end: bool,
    ) -> bool {
        let pattern_bytes = self.pattern_bytes(pattern);
        if pattern_bytes.is_empty() {
            return false;
        }

        let data = binary.data.as_slice();
        let offset = offset.unwrap_or(0);
        let Some(file_offset) = (if from_end {
            data.len().checked_sub(offset)
        } else {
            Some(offset)
        }) else {
            return false;
        };

        if file_offset >= data.len() {
            return false;
        }
        Self::match_pattern_bytes(&data[file_offset..], &pattern_bytes)
    }

    fn match_overlay_pattern(
        &self,
        binary: &LoadedBinary,
        pattern: &str,
        offset: Option<usize>,
    ) -> bool {
        let pattern_bytes = self.pattern_bytes(pattern);
        if pattern_bytes.is_empty() {
            return false;
        }
        let Some((overlay_offset, _overlay_size)) = self.overlay_range(binary) else {
            return false;
        };
        let file_offset = overlay_offset.saturating_add(offset.unwrap_or(0));
        let data = binary.data.as_slice();
        if file_offset >= data.len() {
            return false;
        }
        Self::match_pattern_bytes(&data[file_offset..], &pattern_bytes)
    }

    fn section_by_selector<'a>(
        &self,
        binary: &'a LoadedBinary,
        selector: &SectionSelector,
    ) -> Option<&'a SectionInfo> {
        match selector {
            SectionSelector::Index(index) => binary.sections.get(*index),
            SectionSelector::Last => binary.sections.last(),
            SectionSelector::Name(name) => binary
                .sections
                .iter()
                .find(|section| section.name == *name || section.name.eq_ignore_ascii_case(name)),
        }
    }

    fn section_bytes<'a>(
        &self,
        binary: &'a LoadedBinary,
        section: &SectionInfo,
    ) -> Option<&'a [u8]> {
        let start = section.file_offset as usize;
        if start >= binary.data.as_slice().len() || section.file_size == 0 {
            return None;
        }
        let end = start
            .saturating_add(section.file_size as usize)
            .min(binary.data.as_slice().len());
        Some(&binary.data.as_slice()[start..end])
    }

    fn overlay_range(&self, binary: &LoadedBinary) -> Option<(usize, usize)> {
        let mapped_end = binary
            .sections
            .iter()
            .filter(|section| section.file_size > 0)
            .filter_map(|section| {
                (section.file_offset as usize).checked_add(section.file_size as usize)
            })
            .max()
            .unwrap_or(0)
            .min(binary.data.as_slice().len());
        if mapped_end < binary.data.as_slice().len() {
            Some((mapped_end, binary.data.as_slice().len() - mapped_end))
        } else {
            None
        }
    }

    fn overlay_bytes<'a>(&self, binary: &'a LoadedBinary) -> Option<&'a [u8]> {
        let (offset, size) = self.overlay_range(binary)?;
        if size == 0 {
            return None;
        }
        Some(&binary.data.as_slice()[offset..offset + size])
    }

    fn parse_pattern(pattern: &str) -> Vec<Option<u8>> {
        let mut bytes = Vec::new();
        let chars = pattern.chars().collect::<Vec<_>>();
        let mut idx = 0;
        while idx < chars.len() {
            let ch = chars[idx];
            if ch.is_whitespace() {
                idx += 1;
                continue;
            }

            if ch == '\'' {
                idx += 1;
                while idx < chars.len() && chars[idx] != '\'' {
                    bytes.push(Some(chars[idx] as u8));
                    idx += 1;
                }
                if idx < chars.len() && chars[idx] == '\'' {
                    idx += 1;
                }
                continue;
            }

            if idx + 1 >= chars.len() {
                return Vec::new();
            }
            let hi = chars[idx];
            let lo = chars[idx + 1];
            let hi_valid = hi.is_ascii_hexdigit() || matches!(hi, '?' | '.' | '$');
            let lo_valid = lo.is_ascii_hexdigit() || matches!(lo, '?' | '.' | '$');
            if !hi_valid || !lo_valid {
                return Vec::new();
            }
            let token = [hi, lo].iter().collect::<String>();
            bytes.push(Self::parse_pattern_token(&token));
            idx += 2;
        }
        bytes
    }

    fn parse_pattern_token(token: &str) -> Option<u8> {
        if token == "??"
            || token == ".."
            || token == "$$"
            || token.contains('?')
            || token.contains('.')
            || token.contains('$')
            || token.eq_ignore_ascii_case("xx")
        {
            None
        } else {
            u8::from_str_radix(token, 16).ok()
        }
    }

    fn match_pattern_bytes(data: &[u8], pattern: &[Option<u8>]) -> bool {
        if pattern.len() > data.len() {
            return false;
        }

        for (i, p) in pattern.iter().enumerate() {
            if let Some(expected) = p {
                if data[i] != *expected {
                    return false;
                }
            }
            // None = wildcard, always matches
        }
        true
    }

    fn contains_string(data: &[u8], needle: &str) -> bool {
        let needle_bytes = needle.as_bytes();
        if needle_bytes.len() > data.len() {
            return false;
        }

        data.windows(needle_bytes.len())
            .any(|window| window == needle_bytes)
    }
}

fn binary_matches_die_format(binary: &LoadedBinary, format: &str) -> bool {
    let binary_format = binary.format.to_ascii_uppercase();
    match format.to_ascii_uppercase().as_str() {
        "PE" => binary_format.starts_with("PE") || binary_format.contains("COFF"),
        "ELF" => binary_format.starts_with("ELF"),
        "MACH-O" | "MACHO" | "MACH" => binary_format.contains("MACH"),
        "MZ" => binary_format.contains("MZ") || binary_format.contains("DOS"),
        "NE" => binary_format.contains("NE"),
        "COM" => binary_format.contains("COM"),
        "BINARY" => binary_format.contains("BINARY") || binary_format.contains("RAW"),
        _ => true,
    }
}

fn compare_usize(actual: usize, op: CompareOp, expected: usize) -> bool {
    match op {
        CompareOp::Eq => actual == expected,
        CompareOp::Ne => actual != expected,
        CompareOp::Gt => actual > expected,
        CompareOp::Ge => actual >= expected,
        CompareOp::Lt => actual < expected,
        CompareOp::Le => actual <= expected,
    }
}

fn compare_u64(actual: u64, op: CompareOp, expected: u64) -> bool {
    match op {
        CompareOp::Eq => actual == expected,
        CompareOp::Ne => actual != expected,
        CompareOp::Gt => actual > expected,
        CompareOp::Ge => actual >= expected,
        CompareOp::Lt => actual < expected,
        CompareOp::Le => actual <= expected,
    }
}

fn compare_f64(actual: f64, op: CompareOp, expected: f64) -> bool {
    match op {
        CompareOp::Eq => (actual - expected).abs() < f64::EPSILON,
        CompareOp::Ne => (actual - expected).abs() >= f64::EPSILON,
        CompareOp::Gt => actual > expected,
        CompareOp::Ge => actual >= expected,
        CompareOp::Lt => actual < expected,
        CompareOp::Le => actual <= expected,
    }
}

fn shannon_entropy(bytes: &[u8]) -> f64 {
    if bytes.is_empty() {
        return 0.0;
    }
    let mut counts = [0usize; 256];
    for byte in bytes {
        counts[*byte as usize] += 1;
    }
    let len = bytes.len() as f64;
    counts
        .into_iter()
        .filter(|count| *count > 0)
        .map(|count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Bumped whenever signature extraction changes what it produces.
///
/// The parsed database is cached to disk and reused while it is newer than
/// the signature source. The source is a shipped asset that never changes, so
/// a cache written by an older parser is served forever -- a filter added to
/// extraction had no effect at all until the file was deleted by hand, and
/// the cache in the tree at the time was five weeks old. The version is part
/// of the file name so an older build's cache is not overwritten, only
/// ignored.
const SIGNATURE_PARSER_VERSION: u32 = 2;

/// Every file the default database is built from, for the cache check.
///
/// The packed corpus beside the mirror is one of them: when it is present
/// the database is read from it and the `.sg` files are not opened at all.
/// It used to be left out of this list, so replacing `die_signatures.fpk`
/// kept serving the database cached from the old one indefinitely.
fn default_cache_sources(path: Option<&Path>, mirror_root: Option<&Path>) -> Vec<PathBuf> {
    let mut sources: Vec<PathBuf> = path.map(Path::to_path_buf).into_iter().collect();
    if let Some(root) = mirror_root {
        sources.push(root.join("die_signatures.fpk"));
        for child in ["db", "db_extra", "db_custom"] {
            collect_sg_files(&root.join(child), &mut sources);
        }
    }
    sources
}

/// Whether the cache at `cache` was written after every source that exists
/// was last modified.
fn cache_is_fresh(cache: &Path, sources: &[PathBuf]) -> bool {
    let Ok(cached_at) = fs::metadata(cache).and_then(|meta| meta.modified()) else {
        return false;
    };
    sources.iter().all(|source| {
        fs::metadata(source)
            .and_then(|meta| meta.modified())
            .map_or(true, |modified| modified <= cached_at)
    })
}

fn read_fresh_cache(cache: &Path, sources: &[PathBuf]) -> Option<SignatureDatabase> {
    if !cache_is_fresh(cache, sources) {
        return None;
    }
    serde_json::from_str(&fs::read_to_string(cache).ok()?).ok()
}

/// Write the cache whole or not at all.
///
/// `fs::write` truncates the file and then fills it, so a second process
/// reading the cache meanwhile -- or this one, after an interrupted write --
/// saw a truncated JSON document. A sibling temporary file renamed over the
/// cache replaces it in one step.
fn write_cache(cache: &Path, db: &SignatureDatabase) {
    if let Ok(serialized) = serde_json::to_string(db) {
        let _ = fission_core::config_store::write_file_atomically(cache, serialized.as_bytes());
    }
}

fn loader_cache_dir() -> Option<PathBuf> {
    let cache_dir = fission_core::resource_roots::fission_cache_dir().or_else(|| {
        fission_core::utils::find_workspace_root("FISSION_ROOT")
            .map(|r| r.join("target").join("fission-loader"))
    })?;
    let _ = fs::create_dir_all(&cache_dir);
    Some(cache_dir)
}

/// Detect using DIE signatures
pub fn detect_with_die(binary: &LoadedBinary, result: &mut DetectionResult) {
    if let Some(prepared) = PreparedSignatures::default_shared() {
        let mut matcher = DieMatcher::from_prepared(prepared);
        for detection in matcher.match_binary(binary) {
            result.add(detection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal_fixture(data: Vec<u8>) -> LoadedBinary {
        use crate::loader::{DataBuffer, LoadedBinaryBuilder};
        let size = data.len() as u64;
        LoadedBinaryBuilder::new("literal.bin".into(), DataBuffer::Heap(data))
            .format("PE")
            .entry_point(0x1000)
            .add_section(SectionInfo {
                name: ".text".into(),
                virtual_address: 0x1000,
                virtual_size: size,
                file_offset: 0,
                file_size: size,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .unwrap()
    }

    fn literal_database(values: &[&str]) -> SignatureDatabase {
        SignatureDatabase {
            format_version: "fixture".into(),
            description: "fixture".into(),
            source: "fixture".into(),
            signatures: values
                .iter()
                .enumerate()
                .map(|(i, value)| Signature {
                    name: format!("literal-{i}"),
                    sig_type: "compiler".into(),
                    rules: vec![SignatureRule::StringMatch {
                        value: (*value).into(),
                    }],
                    source_format: None,
                    source_file: None,
                    unsupported_rule_count: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn direct_string_cache_equals_scalar_bytes_and_complete_detections() {
        let values = [
            "a", "A", "a\0b", "é", "한글", "[a].*?\\", "line\r\n", "absent", "a",
        ];
        let db = literal_database(&values);
        for data in [
            Vec::new(),
            vec![0xff, 0x80, 0xfe],
            "é 한글 [a].*?\\ line\r\n".as_bytes().to_vec(),
            b"\xff\x80 a\0b A \xfe".to_vec(),
        ] {
            let binary = literal_fixture(data);
            let legacy = DieMatcher::new(db.clone());
            let mut prepared = DieMatcher::new(db.clone());
            prepared.prepare_direct_signature_strings(&binary);
            assert_eq!(prepared.prepared_string_count(), values.len() - 1);
            for (value, sig) in values.iter().zip(&db.signatures) {
                assert_eq!(
                    prepared.cached_string_hit(value).unwrap(),
                    DieMatcher::contains_string(binary.data.as_slice(), value)
                );
                assert_eq!(
                    format!("{:?}", legacy.match_signature(&binary, sig)),
                    format!("{:?}", prepared.match_signature(&binary, sig))
                );
            }
        }
    }

    #[test]
    fn direct_string_cache_preserves_full_domain_without_warming_other_rules() {
        let binary = literal_fixture(b"prefix needle only-elf".to_vec());
        let mut db = literal_database(&["needle", "only-elf"]);
        db.signatures[0].rules.push(SignatureRule::SectionName {
            name: ".text".into(),
        });
        db.signatures[1].source_format = Some("ELF".into());
        let legacy = DieMatcher::new(db.clone()).with_scan_budgets(Some(2), Some(7));
        let mut prepared = DieMatcher::new(db.clone()).with_scan_budgets(Some(2), Some(7));
        prepared.prepare_direct_signature_strings(&binary);
        assert_eq!(prepared.prepared_string_count(), 1);
        assert_eq!(prepared.cached_string_hit("needle"), Some(true));
        assert_eq!(prepared.cached_string_hit("only-elf"), None);
        assert!(prepared.section_cache.is_empty());
        assert_eq!(prepared.max_string_scan_bytes, Some(2));
        assert_eq!(prepared.max_entropy_scan_bytes, Some(7));
        for sig in &db.signatures {
            assert_eq!(
                format!("{:?}", legacy.match_signature(&binary, sig)),
                format!("{:?}", prepared.match_signature(&binary, sig))
            );
        }
        // Ordinary match_binary still applies its existing prefix cap.
        prepared.match_binary(&binary);
        assert_eq!(prepared.cached_string_hit("needle"), Some(false));
        assert_eq!(prepared.max_string_scan_bytes, Some(2));
        // Reusing a matcher for another prepared input never retains old hits.
        let other = literal_fixture(b"different bytes".to_vec());
        prepared.prepare_direct_signature_strings(&other);
        assert_eq!(prepared.cached_string_hit("needle"), Some(false));
    }

    #[test]
    fn test_parse_pattern() {
        let pattern = DieMatcher::parse_pattern("60 BE ?? ?? 8D");
        assert_eq!(pattern.len(), 5);
        assert_eq!(pattern[0], Some(0x60));
        assert_eq!(pattern[1], Some(0xBE));
        assert_eq!(pattern[2], None);
        assert_eq!(pattern[3], None);
        assert_eq!(pattern[4], Some(0x8D));
    }

    #[test]
    fn test_parse_compact_die_pattern() {
        let pattern = DieMatcher::parse_pattern("60BE........8DBE");
        assert_eq!(pattern.len(), 8);
        assert_eq!(pattern[0], Some(0x60));
        assert_eq!(pattern[1], Some(0xBE));
        assert_eq!(pattern[2], None);
        assert_eq!(pattern[5], None);
        assert_eq!(pattern[6], Some(0x8D));
        assert_eq!(pattern[7], Some(0xBE));
    }

    #[test]
    fn test_parse_die_wildcards_and_ascii_literals() {
        let pattern = DieMatcher::parse_pattern("'UPX!'0A$$$$??..");
        assert_eq!(
            pattern,
            vec![
                Some(b'U'),
                Some(b'P'),
                Some(b'X'),
                Some(b'!'),
                Some(0x0A),
                None,
                None,
                None,
                None
            ]
        );
    }

    #[test]
    fn test_match_pattern_bytes() {
        let data = [0x60, 0xBE, 0x12, 0x34, 0x8D, 0x00];
        let pattern = vec![Some(0x60), Some(0xBE), None, None, Some(0x8D)];
        assert!(DieMatcher::match_pattern_bytes(&data, &pattern));
    }

    #[test]
    fn test_parse_detect_it_easy_upx_signature() {
        let Some(root) = fission_core::PATHS.die_mirror_root() else {
            return;
        };
        let path = root.join("db/PE/packer_UPX.2.sg");
        if !path.exists() {
            return;
        }
        let content = fs::read_to_string(&path).expect("UPX DIE signature should be checked in");
        let sig = parse_sg_signature(&root, &path, &content)
            .expect("UPX DIE signature should produce static rules");
        assert_eq!(sig.name, "UPX");
        assert_eq!(sig.sig_type, "packer");
        assert_eq!(sig.source_format.as_deref(), Some("PE"));
        assert!(sig.unsupported_rule_count > 0);
        assert!(sig.rules.iter().any(|rule| {
            matches!(
                rule,
                SignatureRule::EpPattern { pattern, offset: None, .. } if pattern.contains("60e800000000")
            )
        }));
    }

    #[test]
    fn test_parse_sg_skips_optional_scan_mode() {
        let root = Path::new("/tmp/die-root");
        let path = root.join("db/PE/test.2.sg");
        let marker = String::from_utf8(vec![
            105, 115, 72, 101, 117, 114, 105, 115, 116, 105, 99, 83, 99, 97, 110,
        ])
        .expect("ascii marker");
        let content = format!(
            r#"
            meta("packer", "X");
            function detect() {{
                if (PE.{marker}()) {{ bDetected = true; }}
                if (PE.compareEP("60")) {{ bDetected = true; }}
            }}
        "#
        );

        assert!(parse_sg_signature(root, &path, &content).is_none());
    }

    #[test]
    fn test_parse_elf_compare_from_file_end() {
        let root = Path::new("/tmp/die-root");
        let path = root.join("db/ELF/UPX.2.sg");
        let content = r#"
            meta("packer", "UPX");
            function detect() {
                var nSize = ELF.getSize();
                if (ELF.compare("'UPX!'", nSize - 0x24)) { bDetected = true; }
            }
        "#;
        let sig =
            parse_sg_signature(root, &path, content).expect("static ELF compare should parse");
        assert_eq!(sig.source_format.as_deref(), Some("ELF"));
        assert!(sig.rules.iter().any(|rule| {
            matches!(
                rule,
                SignatureRule::FilePattern {
                    pattern,
                    offset: Some(0x24),
                    from_end: true,
                } if pattern == "'UPX!'"
            )
        }));
    }

    #[test]
    fn test_parse_binary_compare_and_unsupported_offset() {
        let root = Path::new("/tmp/die-root");
        let path = root.join("db/Binary/test.2.sg");
        let content = r#"
            meta("packer", "X");
            function detect() {
                if (Binary.compare("e9$$$$", 4)) { bDetected = true; }
                if (Binary.compare("9090", getDynamicOffset())) { bDetected = true; }
            }
        "#;
        let sig =
            parse_sg_signature(root, &path, content).expect("static Binary compare should parse");
        assert_eq!(sig.unsupported_rule_count, 1);
        assert!(sig.rules.iter().any(|rule| {
            matches!(
                rule,
                SignatureRule::FilePattern {
                    pattern,
                    offset: Some(4),
                    from_end: false,
                } if pattern == "e9$$$$"
            )
        }));
    }

    #[test]
    fn test_parse_overlay_section_and_entropy_primitives() {
        let root = Path::new("/tmp/die-root");
        let path = root.join("db/PE/test.2.sg");
        let content = r#"
            meta("packer", "X");
            function detect() {
                if (PE.isOverlayPresent() && PE.compareOverlay("'PK'0304", 4)) { bDetected = true; }
                if (PE.section.length >= 3 && PE.section[".rsrc"].FileSize > 0x1000) { bDetected = true; }
                if (PE.calculateEntropy(PE.section[PE.nLastSection].FileOffset, PE.section[PE.nLastSection].FileSize) > 7.4) { bDetected = true; }
                if (PE.calculateEntropy(PE.getOverlayOffset(), PE.getOverlaySize()) > 7) { bDetected = true; }
            }
        "#;
        let sig =
            parse_sg_signature(root, &path, content).expect("static section facts should parse");
        assert!(
            sig.rules
                .iter()
                .any(|rule| matches!(rule, SignatureRule::OverlayPresent { present: true }))
        );
        assert!(sig.rules.iter().any(|rule| matches!(
            rule,
            SignatureRule::OverlayPattern {
                pattern,
                offset: Some(4)
            } if pattern == "'PK'0304"
        )));
        assert!(sig.rules.iter().any(|rule| matches!(
            rule,
            SignatureRule::SectionCount {
                op: CompareOp::Ge,
                value: 3
            }
        )));
        assert!(sig.rules.iter().any(|rule| matches!(
            rule,
            SignatureRule::SectionNumeric {
                selector: SectionSelector::Name(name),
                field: SectionNumericField::FileSize,
                op: CompareOp::Gt,
                value: 0x1000,
            } if name == ".rsrc"
        )));
        assert!(sig.rules.iter().any(|rule| matches!(
            rule,
            SignatureRule::SectionEntropy {
                selector: SectionSelector::Last,
                op: CompareOp::Gt,
                value,
            } if (*value - 7.4).abs() < f64::EPSILON
        )));
        assert!(sig.rules.iter().any(|rule| matches!(
            rule,
            SignatureRule::OverlayEntropy {
                op: CompareOp::Gt,
                value,
            } if (*value - 7.0).abs() < f64::EPSILON
        )));
    }

    #[test]
    fn test_match_overlay_and_section_numeric_rules() {
        use crate::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

        let data = b"CODE....DATA....PK\x03\x04".to_vec();
        let binary = LoadedBinaryBuilder::new("fixture.bin".to_string(), DataBuffer::Heap(data))
            .format("PE")
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 8,
                file_offset: 0,
                file_size: 8,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_section(SectionInfo {
                name: ".rsrc".to_string(),
                virtual_address: 0x2000,
                virtual_size: 8,
                file_offset: 8,
                file_size: 8,
                is_executable: false,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("fixture binary");

        let sig = Signature {
            name: "overlay-fixture".to_string(),
            sig_type: "installer".to_string(),
            rules: vec![
                SignatureRule::OverlayPresent { present: true },
                SignatureRule::OverlayPattern {
                    pattern: "'PK'0304".to_string(),
                    offset: None,
                },
                SignatureRule::SectionNumeric {
                    selector: SectionSelector::Name(".rsrc".to_string()),
                    field: SectionNumericField::FileSize,
                    op: CompareOp::Eq,
                    value: 8,
                },
            ],
            source_format: Some("PE".to_string()),
            source_file: Some("db/PE/test.1.sg".to_string()),
            unsupported_rule_count: 0,
        };
        let mut matcher = DieMatcher::new(SignatureDatabase {
            format_version: "test".to_string(),
            description: "test".to_string(),
            source: "test".to_string(),
            signatures: vec![sig],
        });
        let detections = matcher.match_binary(&binary);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].confidence, Confidence::High);
    }

    #[test]
    fn test_single_matched_rule_caps_at_low_confidence() {
        // Regression: a signature where this interpreter only extracted one
        // rule from the real .sg script used to score matched=1/total=1 as
        // ratio 1.0 >= 0.8, i.e. "High" confidence -- even though a single
        // matched primitive is exactly what Confidence::Low's own doc
        // comment describes ("single weak indicator"). Confirmed empirically
        // to produce dozens of simultaneous "High confidence" hits
        // (DarkBASIC, Kotlin, Nvidia PhysX, SoftLok dongle, ...) on one
        // ordinary unpacked PE test binary before this fix.
        use crate::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};

        let data = b"CODE....DATA....PK\x03\x04".to_vec();
        let binary = LoadedBinaryBuilder::new("fixture.bin".to_string(), DataBuffer::Heap(data))
            .format("PE")
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 8,
                file_offset: 0,
                file_size: 8,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("fixture binary");

        let sig = Signature {
            name: "single-rule-fixture".to_string(),
            sig_type: "packer".to_string(),
            rules: vec![SignatureRule::OverlayPresent { present: true }],
            source_format: Some("PE".to_string()),
            source_file: Some("db/PE/test.1.sg".to_string()),
            unsupported_rule_count: 0,
        };
        let mut matcher = DieMatcher::new(SignatureDatabase {
            format_version: "test".to_string(),
            description: "test".to_string(),
            source: "test".to_string(),
            signatures: vec![sig],
        });
        let detections = matcher.match_binary(&binary);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].confidence, Confidence::Low);
    }
}

#[cfg(test)]
mod packed_die_tests {
    use super::*;

    /// The packed corpus must be the same parse the `.sg` walk produces.
    ///
    /// `bin/pack_die` calls the same `parse_sg_signature`, so this is checking
    /// that packing and reading back is lossless -- not that two parsers agree.
    #[test]
    fn packed_and_sg_walk_agree() {
        let Some(root) = detect_it_easy_mirror_root() else {
            eprintln!("skipping: no DIE mirror");
            return;
        };
        if !root.join("die_signatures.fpk").exists() {
            eprintln!("skipping: no packed corpus");
            return;
        }

        let empty = || SignatureDatabase {
            format_version: String::new(),
            description: String::new(),
            source: String::new(),
            signatures: Vec::new(),
        };

        let mut packed = empty();
        assert!(
            packed.extend_from_packed_die(&root),
            "packed corpus present but yielded nothing"
        );

        let mut walked = empty();
        let mut sg_files = Vec::new();
        for child in ["db", "db_extra", "db_custom"] {
            collect_sg_files(&root.join(child), &mut sg_files);
        }
        sg_files.sort();
        for path in &sg_files {
            let Ok(content) = fs::read_to_string(path) else {
                continue;
            };
            if let Some(sig) = parse_sg_signature(&root, path, &content) {
                walked.signatures.push(sig);
            }
        }

        if walked.signatures.is_empty() {
            // The `.sg` scripts live in the gitignored `utils/source/`, so a
            // normal checkout (and CI) has the packed corpus and nothing to
            // compare it against. That is the shipped shape, not a failure.
            eprintln!("skipping: no .sg sources to compare against");
            return;
        }
        assert!(
            walked.signatures.len() > 1000,
            "expected the full corpus from the walk, got {}",
            walked.signatures.len()
        );
        assert_eq!(
            packed.signatures.len(),
            walked.signatures.len(),
            "packed and walked signature counts differ"
        );

        let key = |s: &Signature| {
            (
                s.source_file.clone().unwrap_or_default(),
                s.sig_type.clone(),
                s.name.clone(),
                s.rules.len(),
                s.unsupported_rule_count,
            )
        };
        let mut a: Vec<_> = packed.signatures.iter().map(key).collect();
        let mut b: Vec<_> = walked.signatures.iter().map(key).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "packed corpus differs from the .sg walk");
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    use std::io::Read;
    use std::time::{Duration, SystemTime};

    fn database(source: &str) -> SignatureDatabase {
        SignatureDatabase {
            format_version: "die-sg-v1".to_string(),
            description: "test".to_string(),
            source: source.to_string(),
            signatures: Vec::new(),
        }
    }

    fn touch(path: &Path, at: SystemTime) {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open");
        file.set_modified(at).expect("set mtime");
    }

    /// A mirror whose database comes from the packed corpus: replacing
    /// `die_signatures.fpk` after the cache was written makes the cache
    /// stale. The packed corpus was not among the sources checked, so the
    /// old database kept being served.
    #[test]
    fn a_newer_packed_corpus_makes_the_default_cache_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("mirror");
        fs::create_dir_all(root.join("db")).expect("mirror");
        let earlier = SystemTime::now() - Duration::from_secs(3600);
        let later = SystemTime::now();
        touch(&root.join("db").join("packer.sg"), earlier);
        let cache = dir.path().join("cache.json");
        fs::write(&cache, serde_json::to_string(&database("old")).unwrap()).unwrap();
        touch(&cache, earlier + Duration::from_secs(60));

        // Before the corpus changes, the cache is current.
        touch(&root.join("die_signatures.fpk"), earlier);
        let sources = default_cache_sources(None, Some(&root));
        assert!(cache_is_fresh(&cache, &sources));
        assert!(read_fresh_cache(&cache, &sources).is_some());

        // Replace the corpus: the cache is now older than one of its sources.
        touch(&root.join("die_signatures.fpk"), later);
        assert!(!cache_is_fresh(&cache, &sources));
        assert!(read_fresh_cache(&cache, &sources).is_none());
        // The `.sg` files alone -- the old source list -- did not see it.
        let sg_only: Vec<PathBuf> = sources
            .iter()
            .filter(|source| source.extension().is_some_and(|ext| ext == "sg"))
            .cloned()
            .collect();
        assert!(cache_is_fresh(&cache, &sg_only));
    }

    #[test]
    fn a_missing_source_does_not_invalidate_the_cache_but_a_missing_cache_is_stale() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = dir.path().join("cache.json");
        let absent = dir.path().join("absent.fpk");
        assert!(!cache_is_fresh(&cache, &[]));
        fs::write(&cache, "{}").unwrap();
        assert!(cache_is_fresh(&cache, std::slice::from_ref(&absent)));
        // Fresh but not a database: read as a miss, not an error.
        assert!(read_fresh_cache(&cache, &[]).is_none());
    }

    /// The cache is replaced, not rewritten in place. A reader that opened
    /// it before the write still reads the whole old document; `fs::write`
    /// truncated that same file under it.
    #[test]
    fn a_cache_write_replaces_the_file_whole() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cache = dir.path().join("cache.json");
        let old = serde_json::to_string(&database("old")).unwrap();
        fs::write(&cache, &old).unwrap();
        let mut reader = fs::File::open(&cache).expect("open");

        write_cache(&cache, &database("new"));

        let mut seen = String::new();
        reader.read_to_string(&mut seen).expect("read");
        assert_eq!(seen, old, "an open reader saw the file change under it");
        let current: SignatureDatabase =
            serde_json::from_str(&fs::read_to_string(&cache).unwrap()).unwrap();
        assert_eq!(current.source, "new");
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name())
            .filter(|name| name != "cache.json")
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}

#[cfg(test)]
mod prepared_tests {
    //! The prepared half of a matcher (parsed patterns, compiled needle set)
    //! is built once per process and reused for every binary. These check
    //! that reuse changes no result: a matcher reused across every loader
    //! fixture in turn against a matcher prepared from scratch for each, and
    //! the shared needle set against the per-call compilation it replaced.

    use super::*;

    /// Every loader fixture that loads, in name order.
    fn loader_fixtures() -> Vec<(String, LoadedBinary)> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
            .expect("testdata")
            .map(|entry| entry.expect("testdata entry").path())
            .collect();
        paths.sort();
        let fixtures: Vec<_> = paths
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let binary = LoadedBinary::from_bytes(fs::read(&path).ok()?, name.clone()).ok()?;
                Some((name, binary))
            })
            .collect();
        assert!(
            fixtures.len() >= 20,
            "only {} fixtures loaded",
            fixtures.len()
        );
        fixtures
    }

    /// The string cache as it was built before preparation was shared: one
    /// set compiled per call over exactly the needles of `rules`.
    fn per_call_string_cache<'a>(
        data: &[u8],
        rules: impl Iterator<Item = &'a SignatureRule>,
    ) -> HashMap<String, bool> {
        let mut needles = Vec::new();
        let mut seen = HashSet::new();
        for rule in rules {
            if let SignatureRule::StringMatch { value } = rule
                && !value.is_empty()
                && seen.insert(value.clone())
            {
                needles.push(value.clone());
            }
        }
        if needles.is_empty() {
            return HashMap::new();
        }
        let set = regex::bytes::RegexSet::new(needles.iter().map(|n| regex::escape(n)))
            .expect("per-call needle set");
        let matches = set.matches(data);
        needles
            .into_iter()
            .enumerate()
            .map(|(idx, needle)| (needle, matches.matched(idx)))
            .collect()
    }

    #[test]
    fn a_shared_prepared_matcher_detects_exactly_what_a_fresh_one_does() {
        let shared = PreparedSignatures::default_shared().expect("DiE signature database");
        let db = SignatureDatabase::default_shared().expect("DiE signature database");
        // One matcher, reused for every fixture in turn, as a process does.
        let mut reused = DieMatcher::from_prepared(Arc::clone(&shared));
        let mut detections = 0;
        for (name, binary) in loader_fixtures() {
            let fresh = DieMatcher::new(db.clone()).match_binary(&binary);
            let cached = reused.match_binary(&binary);
            assert_eq!(format!("{fresh:?}"), format!("{cached:?}"), "{name}");
            detections += cached.len();
        }
        assert!(detections > 0, "the fixtures exercise no DiE signature");
    }

    #[test]
    fn the_shared_needle_set_answers_as_the_per_call_set_did() {
        let shared = PreparedSignatures::default_shared().expect("DiE signature database");
        let mut reused = DieMatcher::from_prepared(Arc::clone(&shared));
        let (mut checked, mut hits) = (0, 0);
        for (name, binary) in loader_fixtures() {
            // match_binary's domain: every rule.
            reused.build_string_cache(&binary);
            let expected = per_call_string_cache(
                binary.data.as_slice(),
                shared.database.signatures.iter().flat_map(|sig| &sig.rules),
            );
            for needle in &shared.needles {
                assert_eq!(
                    reused.cached_string_hit(needle),
                    expected.get(needle).copied(),
                    "{name}: {needle:?}"
                );
            }
            // The direct callers' domain: only signatures of this format,
            // read out of the set compiled over every needle.
            reused.prepare_direct_signature_strings(&binary);
            let expected = per_call_string_cache(
                binary.data.as_slice(),
                shared
                    .database
                    .signatures
                    .iter()
                    .filter(|sig| {
                        sig.source_format
                            .as_deref()
                            .is_none_or(|format| binary_matches_die_format(&binary, format))
                    })
                    .flat_map(|sig| &sig.rules),
            );
            for needle in &shared.needles {
                assert_eq!(
                    reused.cached_string_hit(needle),
                    expected.get(needle).copied(),
                    "{name} (direct): {needle:?}"
                );
            }
            checked += expected.len();
            hits += expected.values().filter(|hit| **hit).count();
        }
        assert!(checked > 0 && hits > 0, "checked {checked}, hits {hits}");
    }
}
