//! DIE (Detect-It-Easy) Compatible Signature Engine
//!
//! Loads and matches signatures from JSON files compatible with DIE format.
//! Supports: section names, strings, entry point patterns, imports, rich headers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{Confidence, Detection, DetectionResult, DetectionType};
use crate::loader::LoadedBinary;
use fission_core::PAGE_SIZE;

/// A single rule within a signature
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum SignatureRule {
    #[serde(rename = "section_name")]
    SectionName { name: String },

    #[serde(rename = "string")]
    StringMatch { value: String },

    #[serde(rename = "ep_pattern")]
    EpPattern {
        arch: Option<String>,
        pattern: String,
    },

    #[serde(rename = "import")]
    Import { function: String },

    #[serde(rename = "rich_header")]
    RichHeader { present: bool },
}

/// A complete signature entry
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Signature {
    pub name: String,
    #[serde(rename = "type")]
    pub sig_type: String,
    pub rules: Vec<SignatureRule>,
}

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

    /// Load from default path using PathConfig
    pub fn load_default() -> Option<Self> {
        // Try PathConfig first (centralized path resolution)
        if let Some(path) = fission_core::PATHS.get_die_signatures_path() {
            if let Ok(db) = Self::load(&path) {
                return Some(db);
            }
        }

        // Fallback: search upward from current directory and executable path.
        let suffix = Path::new("utils")
            .join("signatures")
            .join("die")
            .join("pe_signatures.json");

        let mut search_roots = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            search_roots.push(cwd);
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(parent) = exe.parent()
        {
            search_roots.push(parent.to_path_buf());
        }

        for root in search_roots {
            for dir in root.ancestors() {
                let candidate = dir.join(&suffix);
                if let Ok(db) = Self::load(&candidate) {
                    return Some(db);
                }
            }
        }

        None
    }
}

/// DIE-compatible signature matcher
pub struct DieMatcher {
    database: SignatureDatabase,
    section_cache: HashMap<String, bool>,
    string_cache: HashMap<String, bool>,
}

impl DieMatcher {
    pub fn new(database: SignatureDatabase) -> Self {
        Self {
            database,
            section_cache: HashMap::new(),
            string_cache: HashMap::new(),
        }
    }

    /// Match binary against all signatures
    pub fn match_binary(&mut self, binary: &LoadedBinary) -> Vec<Detection> {
        let mut detections = Vec::new();

        // Pre-build caches for faster matching
        self.build_section_cache(binary);
        self.build_string_cache(binary);

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
        // We'll do lazy string matching instead of pre-caching all strings
    }

    fn match_signature(&self, binary: &LoadedBinary, sig: &Signature) -> Option<Detection> {
        let mut matched_rules = 0;
        let total_rules = sig.rules.len();

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
        let confidence = if ratio >= 0.8 {
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
            "framework" => DetectionType::Library,
            _ => DetectionType::Library,
        };

        Some(
            Detection::new(detection_type, &sig.name, None, confidence).with_details(format!(
                "DIE: {}/{} rules matched",
                matched_rules, total_rules
            )),
        )
    }

    fn match_rule(&self, binary: &LoadedBinary, rule: &SignatureRule) -> bool {
        match rule {
            SignatureRule::SectionName { name } => {
                self.section_cache.contains_key(&name.to_lowercase())
                    || self.section_cache.contains_key(name)
            }

            SignatureRule::StringMatch { value } => {
                // Search in binary data
                Self::contains_string(binary.data.as_slice(), value)
            }

            SignatureRule::EpPattern { arch, pattern } => {
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
                self.match_ep_pattern(binary, pattern)
            }

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

    fn match_ep_pattern(&self, binary: &LoadedBinary, pattern: &str) -> bool {
        // Convert pattern string to bytes with wildcards
        // Pattern format: "60 BE ?? ?? ?? ?? 8D BE"
        let pattern_bytes = Self::parse_pattern(pattern);
        if pattern_bytes.is_empty() {
            return false;
        }

        // Get entry point offset
        let ep_rva = binary.entry_point;
        // Find section containing EP
        let ep_data = binary
            .sections
            .iter()
            .find(|s| {
                ep_rva >= s.virtual_address && ep_rva < s.virtual_address + s.virtual_size as u64
            })
            .and_then(|s| {
                let offset = (ep_rva - s.virtual_address) as usize;
                let file_offset = s.file_offset as usize + offset;
                if file_offset + 64 <= binary.data.as_slice().len() {
                    Some(&binary.data.as_slice()[file_offset..file_offset + 64])
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

    fn parse_pattern(pattern: &str) -> Vec<Option<u8>> {
        pattern
            .split_whitespace()
            .map(|s| {
                if s == "??" || s.to_lowercase() == "xx" {
                    None
                } else {
                    u8::from_str_radix(s, 16).ok()
                }
            })
            .collect()
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
    fn test_match_pattern_bytes() {
        let data = [0x60, 0xBE, 0x12, 0x34, 0x8D, 0x00];
        let pattern = vec![Some(0x60), Some(0xBE), None, None, Some(0x8D)];
        assert!(DieMatcher::match_pattern_bytes(&data, &pattern));
    }
}
