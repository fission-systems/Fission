//! String cross-reference analysis.
//!
//! Combines string extraction with xref analysis to find which functions reference specific strings.

use super::strings::{ExtractedString, StringType};
use super::xrefs::{Xref, XrefDatabase};
use fission_loader::loader::LoadedBinary;
use std::collections::HashMap;

/// A string with its cross-references
#[derive(Debug, Clone)]
pub struct StringWithXrefs {
    /// The string itself
    pub string: ExtractedString,
    /// Cross-references to this string
    pub xrefs: Vec<Xref>,
}

/// Result of string cross-reference analysis
#[derive(Debug, Clone)]
pub struct StringXrefAnalysis {
    /// All strings with their references
    pub strings: Vec<StringWithXrefs>,
    /// Quick lookup: string content -> StringWithXrefs
    pub by_content: HashMap<String, Vec<usize>>,
}

impl StringXrefAnalysis {
    /// Create a new empty analysis
    pub fn new() -> Self {
        Self {
            strings: Vec::new(),
            by_content: HashMap::new(),
        }
    }

    /// Add a string with its xrefs
    pub fn add(&mut self, string: ExtractedString, xrefs: Vec<Xref>) {
        let content = string.content.clone();
        let index = self.strings.len();

        self.strings.push(StringWithXrefs { string, xrefs });
        self.by_content
            .entry(content)
            .or_insert_with(Vec::new)
            .push(index);
    }

    /// Find strings by content (exact match)
    pub fn find_by_content(&self, search: &str) -> Vec<&StringWithXrefs> {
        self.by_content
            .get(search)
            .map(|indices| indices.iter().map(|&i| &self.strings[i]).collect())
            .unwrap_or_default()
    }

    /// Find strings by partial match (contains)
    pub fn find_by_partial(&self, search: &str) -> Vec<&StringWithXrefs> {
        self.strings
            .iter()
            .filter(|s| s.string.content.contains(search))
            .collect()
    }

    /// Find strings by regex pattern
    pub fn find_by_regex(&self, pattern: &str) -> Result<Vec<&StringWithXrefs>, regex::Error> {
        let re = regex::Regex::new(pattern)?;
        Ok(self
            .strings
            .iter()
            .filter(|s| re.is_match(&s.string.content))
            .collect())
    }

    /// Get all strings with references (excludes unreferenced strings)
    pub fn referenced_strings(&self) -> Vec<&StringWithXrefs> {
        self.strings
            .iter()
            .filter(|s| !s.xrefs.is_empty())
            .collect()
    }

    /// Get all strings without references
    pub fn unreferenced_strings(&self) -> Vec<&StringWithXrefs> {
        self.strings.iter().filter(|s| s.xrefs.is_empty()).collect()
    }

    /// Get statistics
    pub fn stats(&self) -> StringXrefStats {
        let total = self.strings.len();
        let referenced = self.referenced_strings().len();
        let unreferenced = total - referenced;

        let ascii_count = self
            .strings
            .iter()
            .filter(|s| s.string.string_type == StringType::Ascii)
            .count();
        let unicode_count = total - ascii_count;

        let total_xrefs: usize = self.strings.iter().map(|s| s.xrefs.len()).sum();

        StringXrefStats {
            total_strings: total,
            referenced_strings: referenced,
            unreferenced_strings: unreferenced,
            ascii_strings: ascii_count,
            unicode_strings: unicode_count,
            total_xrefs,
        }
    }
}

/// Statistics about string xrefs
#[derive(Debug, Clone)]
pub struct StringXrefStats {
    pub total_strings: usize,
    pub referenced_strings: usize,
    pub unreferenced_strings: usize,
    pub ascii_strings: usize,
    pub unicode_strings: usize,
    pub total_xrefs: usize,
}

/// Analyze string cross-references in a binary
pub fn analyze_string_xrefs(binary: &LoadedBinary, min_length: usize) -> StringXrefAnalysis {
    let mut analysis = StringXrefAnalysis::new();

    // Extract strings from all sections
    let mut all_strings = Vec::new();
    for section in &binary.sections {
        let start = section.file_offset as usize;
        let end = start + section.file_size as usize;
        if let Some(data) = binary.data.as_slice().get(start..end) {
            let base_addr = section.virtual_address;
            let strings = super::strings::extract_strings(data, base_addr, min_length);
            all_strings.extend(strings);
        }
    }

    // Build xref database
    let xref_db = XrefDatabase::build_from_binary(binary);

    // For each string, find references to its address
    for string in all_strings {
        let addr = string.address;
        let xrefs = xref_db.get_refs_to(addr).to_vec();
        analysis.add(string, xrefs);
    }

    analysis
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_xref_analysis() {
        let mut analysis = StringXrefAnalysis::new();

        let string = ExtractedString {
            address: 0x1000,
            content: "test".to_string(),
            string_type: StringType::Ascii,
            length: 4,
        };

        analysis.add(string, vec![]);

        assert_eq!(analysis.strings.len(), 1);
        assert_eq!(analysis.find_by_content("test").len(), 1);
    }

    #[test]
    fn test_partial_search() {
        let mut analysis = StringXrefAnalysis::new();

        let strings = vec![
            ("Hello, World!", 0x1000),
            ("Test Hello", 0x2000),
            ("Goodbye", 0x3000),
        ];

        for (content, addr) in strings {
            analysis.add(
                ExtractedString {
                    address: addr,
                    content: content.to_string(),
                    string_type: StringType::Ascii,
                    length: content.len(),
                },
                vec![],
            );
        }

        let results = analysis.find_by_partial("Hello");
        assert_eq!(results.len(), 2);
    }
}
