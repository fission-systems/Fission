//! Canonical binary identity report types (serde for CLI / tooling).

use crate::detector::Confidence;
use serde::Serialize;
use std::collections::BTreeMap;

/// Conservative scan budgets for identity analysis (full-binary scans can be abused on huge inputs).
#[derive(Debug, Clone, Copy)]
pub struct IdentityScanLimits {
    /// Cap on raw bytes hashed per entropy estimate (may span multiple sections until exhausted).
    pub max_scan_bytes: usize,
    /// Reserved for future string-based rules (Phase 2+).
    pub max_string_scan_bytes: usize,
    pub max_evidence_items: usize,
    pub max_detections: usize,
}

impl Default for IdentityScanLimits {
    fn default() -> Self {
        Self {
            max_scan_bytes: 64 * 1024 * 1024,
            max_string_scan_bytes: 1024 * 1024,
            max_evidence_items: 256,
            max_detections: 128,
        }
    }
}

pub const IDENTITY_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum IdentityKind {
    Compiler,
    Linker,
    Language,
    Runtime,
    Packer,
    Protector,
    Installer,
    Overlay,
    DebugInfo,
    Signature,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Hash)]
#[serde(rename_all = "PascalCase")]
pub enum IdentitySource {
    LoaderHeader,
    ImportTable,
    SectionName,
    StringPattern,
    SignaturePattern,
    RichHeader,
    SymbolTable,
    DebugInfo,
    Entropy,
    Overlay,
    DieRule,
}

#[derive(Debug, Clone, Serialize)]
pub enum EvidenceLocation {
    FileOffset(u64),
    VirtualAddress(u64),
    Section(String),
    Import(String),
    Symbol(String),
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityEvidence {
    pub source: IdentitySource,
    pub location: EvidenceLocation,
    pub description: String,
    pub matched: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NegativeIdentityEvidence {
    pub source: IdentitySource,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityDetection {
    pub kind: IdentityKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub confidence: Confidence,
    pub source: IdentitySource,
    #[serde(default)]
    pub score: u32,
    pub evidence: Vec<IdentityEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub negative_evidence: Vec<NegativeIdentityEvidence>,
}

/// DIE subset primitive hit recorded before Fission promotion gates (`die_raw_matches`).
#[derive(Debug, Clone, Serialize)]
pub struct DieRawMatch {
    pub rule_id: String,
    pub rule_name: String,
    pub category: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_primitives: Vec<String>,
    pub unsupported_primitives_ignored: usize,
    pub matched_primitive_count: usize,
    pub total_primitive_slots: usize,
    #[serde(default)]
    pub raw_score_bonus: u32,
    pub details: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuppressedDetection {
    pub name: String,
    pub kind: IdentityKind,
    pub reason: String,
    pub raw_score: u32,
}

/// Presence and counts for `/utils`-backed corpora resolved via [`fission_core::PATHS`].
#[derive(Debug, Clone, Serialize)]
pub struct IdentityResourceSummary {
    pub die_corpus_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub die_corpus_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub die_sg_file_count: Option<usize>,
    pub die_pe_json_present: bool,
    pub patterns_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern_json_count: Option<usize>,
    pub win_typeinfo_present: bool,
    pub fid_present: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fid_bf_count: Option<usize>,
}

/// DIE JSON rule corpus indexed against the Phase 2 supported primitive subset.
#[derive(Debug, Clone, Serialize)]
pub struct DieCompatSummary {
    pub rules_seen: usize,
    pub rules_supported: usize,
    pub rules_skipped: usize,
    pub signatures_seen: usize,
    pub signatures_with_supported_rules: usize,
    pub signatures_matched: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub unsupported_primitives: BTreeMap<String, usize>,
}

/// Extra PE facts from a bounded raw-byte parse (failure yields `None` on the report field).
#[derive(Debug, Clone, Serialize)]
pub struct PeIdentitySummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_directory_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tls_callback_count: Option<usize>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub debug_directory_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_section: Option<String>,
}

/// How many IAT symbol names appear in the WinAPI catalog text DB (informational only).
#[derive(Debug, Clone, Serialize)]
pub struct WinApiCatalogSummary {
    pub symbols_considered: usize,
    pub symbols_in_catalog: usize,
    pub symbols_not_in_catalog: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SectionEntropy {
    pub section: String,
    pub entropy: f32,
    pub classification: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayInfo {
    pub file_offset_start: u64,
    pub size: u64,
}

/// PE Rich header summary placeholder — populated in later phases when parsed explicitly.
#[derive(Debug, Clone, Serialize)]
pub struct PeRichHeaderSummary {
    pub present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryIdentitySummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likely_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likely_compiler: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likely_linker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likely_packer: Option<String>,
    pub packed_score: f32,
    pub has_overlay: bool,
    pub high_entropy_executable_sections: usize,
    pub confidence: Confidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compiler_conflicts: Vec<String>,
    #[serde(default)]
    pub weak_signal_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BinaryIdentityReport {
    pub schema_version: u32,
    pub detections: Vec<IdentityDetection>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub die_raw_matches: Vec<DieRawMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suppressed_detections: Vec<SuppressedDetection>,
    pub section_entropy: Vec<SectionEntropy>,
    pub overlay: Option<OverlayInfo>,
    pub rich_header: Option<PeRichHeaderSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<IdentityResourceSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub die_compat: Option<DieCompatSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pe: Option<PeIdentitySummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub winapi_catalog: Option<WinApiCatalogSummary>,
    pub summary: BinaryIdentitySummary,
}
