//! DIE (Detect-It-Easy) Compatible Signature Engine
//!
//! Loads and matches signatures from JSON files compatible with DIE format.
//! Supports: section names, strings, entry point patterns, imports, rich headers.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

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
        use std::sync::OnceLock;
        static PE_JSON_DB: OnceLock<Option<SignatureDatabase>> = OnceLock::new();
        PE_JSON_DB
            .get_or_init(|| {
                let path = fission_core::PATHS.get_die_signatures_path()?;

                // Check disk cache
                let cache_path = loader_cache_dir().map(|d| d.join("die_pe_json_cache.json"));
                if let Some(ref cache_p) = cache_path {
                    if cache_p.exists() {
                        if let (Ok(m_cache), Ok(m_src)) =
                            (fs::metadata(cache_p), fs::metadata(&path))
                        {
                            if let (Ok(t_cache), Ok(t_src)) = (m_cache.modified(), m_src.modified())
                            {
                                if t_cache >= t_src {
                                    if let Ok(content) = fs::read_to_string(cache_p) {
                                        if let Ok(db) = serde_json::from_str::<Self>(&content) {
                                            return Some(db);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Ok(db) = Self::load(&path) {
                    if let Some(ref cache_p) = cache_path {
                        if let Ok(serialized) = serde_json::to_string(&db) {
                            let _ = fs::write(cache_p, serialized);
                        }
                    }
                    Some(db)
                } else {
                    None
                }
            })
            .clone()
    }

    /// Load from default path using [`fission_core::PATHS`] / bundle resolution only (no cwd upward walks).
    pub fn load_default() -> Option<Self> {
        use std::sync::OnceLock;
        static DEFAULT_DB: OnceLock<Option<SignatureDatabase>> = OnceLock::new();
        DEFAULT_DB
            .get_or_init(|| {
                let path = fission_core::PATHS.get_die_signatures_path();
                let mirror_root = detect_it_easy_mirror_root();

                if path.is_none() && mirror_root.is_none() {
                    return None;
                }

                // Check disk cache
                let cache_path = loader_cache_dir().map(|d| d.join("die_default_cache.json"));
                if let Some(ref cache_p) = cache_path {
                    if cache_p.exists() {
                        if let Ok(m_cache) = fs::metadata(cache_p) {
                            if let Ok(t_cache) = m_cache.modified() {
                                let mut cache_valid = true;

                                if let Some(ref p) = path {
                                    if let Ok(m_src) = fs::metadata(p) {
                                        if let Ok(t_src) = m_src.modified() {
                                            if t_src > t_cache {
                                                cache_valid = false;
                                            }
                                        }
                                    }
                                }

                                if cache_valid {
                                    if let Some(ref root) = mirror_root {
                                        let mut sg_files = Vec::new();
                                        for child in ["db", "db_extra", "db_custom"] {
                                            collect_sg_files(&root.join(child), &mut sg_files);
                                        }
                                        for f in &sg_files {
                                            if let Ok(m_src) = fs::metadata(f) {
                                                if let Ok(t_src) = m_src.modified() {
                                                    if t_src > t_cache {
                                                        cache_valid = false;
                                                        break;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if cache_valid {
                                    if let Ok(content) = fs::read_to_string(cache_p) {
                                        if let Ok(db) = serde_json::from_str::<Self>(&content) {
                                            return Some(db);
                                        }
                                    }
                                }
                            }
                        }
                    }
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
                        if let Ok(serialized) = serde_json::to_string(&db) {
                            let _ = fs::write(cache_p, serialized);
                        }
                    }
                    Some(db)
                }
            })
            .clone()
    }

    fn extend_from_detect_it_easy_mirror(&mut self) {
        let Some(root) = detect_it_easy_mirror_root() else {
            return;
        };

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

fn dedup_rules(rules: &mut Vec<SignatureRule>) {
    let mut seen = HashSet::new();
    rules.retain(|rule| {
        let key = match rule {
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
        };
        seen.insert(key)
    });
}

/// DIE-compatible signature matcher
pub struct DieMatcher {
    database: SignatureDatabase,
    section_cache: HashMap<String, bool>,
    string_cache: HashMap<String, bool>,
    ep_pattern_cache: HashMap<String, Vec<Option<u8>>>,
    /// When set, string rules scan only this prefix of the mapped image (identity budgets).
    max_string_scan_bytes: Option<usize>,
    /// When set, entropy primitives hash only this many bytes per section / overlay slice.
    max_entropy_scan_bytes: Option<usize>,
}

impl DieMatcher {
    pub fn new(database: SignatureDatabase) -> Self {
        Self {
            database,
            section_cache: HashMap::new(),
            string_cache: HashMap::new(),
            ep_pattern_cache: HashMap::new(),
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

        // Pre-build caches for faster matching
        self.build_section_cache(binary);
        self.build_string_cache(binary);
        self.build_ep_pattern_cache();

        for sig in &self.database.signatures {
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

    fn build_string_cache(&mut self, _binary: &LoadedBinary) {
        self.string_cache.clear();
        let mut unique_needles = Vec::new();
        let mut seen = HashSet::new();

        for sig in &self.database.signatures {
            for rule in &sig.rules {
                if let SignatureRule::StringMatch { value } = rule
                    && !value.is_empty()
                    && seen.insert(value.clone())
                {
                    unique_needles.push(value.clone());
                }
            }
        }

        if unique_needles.is_empty() {
            return;
        }

        // Evaluate all DIE string rules against the binary in one pass.
        let mut data = _binary.data.as_slice();
        if let Some(cap) = self.max_string_scan_bytes {
            if cap < data.len() {
                data = &data[..cap];
            }
        }
        let escaped = unique_needles
            .iter()
            .map(|needle| regex::escape(needle))
            .collect::<Vec<_>>();

        match regex::bytes::RegexSet::new(&escaped) {
            Ok(set) => {
                let matches = set.matches(data);
                for (idx, needle) in unique_needles.into_iter().enumerate() {
                    self.string_cache.insert(needle, matches.matched(idx));
                }
            }
            Err(_) => {
                // Fallback keeps behavior correct if regex-set compilation fails.
                for needle in unique_needles {
                    self.string_cache
                        .insert(needle.clone(), Self::contains_string(data, &needle));
                }
            }
        }
    }

    fn build_ep_pattern_cache(&mut self) {
        self.ep_pattern_cache.clear();
        for sig in &self.database.signatures {
            for rule in &sig.rules {
                match rule {
                    SignatureRule::EpPattern { pattern, .. }
                    | SignatureRule::FilePattern { pattern, .. }
                    | SignatureRule::OverlayPattern { pattern, .. } => {
                        self.ep_pattern_cache
                            .entry(pattern.clone())
                            .or_insert_with(|| Self::parse_pattern(pattern));
                    }
                    _ => {}
                }
            }
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

        let mut matched_rules = 0;
        let total_rules = sig.rules.len() + sig.unsupported_rule_count;

        if total_rules == 0 {
            return None;
        }

        for rule in &sig.rules {
            if self.match_rule(binary, rule) {
                matched_rules += 1;
            }
        }

        // Require at least one rule match
        if matched_rules == 0 {
            return None;
        }

        // Calculate confidence based on match ratio
        let ratio = matched_rules as f32 / total_rules as f32;
        let confidence = if ratio >= 0.8 && sig.unsupported_rule_count == 0 {
            Confidence::High
        } else if ratio >= 0.5 {
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
                "DIE: {}/{} primitives matched{}{}",
                matched_rules, total_rules, ignored, source
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
                .string_cache
                .get(value)
                .copied()
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
        let pattern_bytes = self
            .ep_pattern_cache
            .get(pattern)
            .cloned()
            .unwrap_or_else(|| Self::parse_pattern(pattern));
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
                ep_rva >= s.virtual_address && ep_rva < s.virtual_address + s.virtual_size as u64
            })
            .and_then(|s| {
                let offset = (ep_rva - s.virtual_address) as usize;
                let file_offset = s.file_offset as usize + offset + extra_offset;
                if file_offset < binary.data.as_slice().len() {
                    let end = std::cmp::min(file_offset + 128, binary.data.as_slice().len());
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
        let pattern_bytes = self
            .ep_pattern_cache
            .get(pattern)
            .cloned()
            .unwrap_or_else(|| Self::parse_pattern(pattern));
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
        let pattern_bytes = self
            .ep_pattern_cache
            .get(pattern)
            .cloned()
            .unwrap_or_else(|| Self::parse_pattern(pattern));
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

fn loader_cache_dir() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|p| p.to_path_buf())?;
    let cache_dir = root.join("target").join("fission-loader");
    let _ = fs::create_dir_all(&cache_dir);
    Some(cache_dir)
}

/// Detect using DIE signatures
pub fn detect_with_die(binary: &LoadedBinary, result: &mut DetectionResult) {
    if let Some(db) = SignatureDatabase::load_default() {
        let mut matcher = DieMatcher::new(db);
        for detection in matcher.match_binary(binary) {
            result.add(detection);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
