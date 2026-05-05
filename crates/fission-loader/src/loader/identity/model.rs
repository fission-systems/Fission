//! Canonical binary identity report types (serde for CLI / tooling).

use crate::detector::Confidence;
use serde::Serialize;

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

pub const IDENTITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub struct BinaryIdentityReport {
    pub schema_version: u32,
    pub detections: Vec<IdentityDetection>,
    pub section_entropy: Vec<SectionEntropy>,
    pub overlay: Option<OverlayInfo>,
    pub rich_header: Option<PeRichHeaderSummary>,
    pub summary: BinaryIdentitySummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityDetection {
    pub kind: IdentityKind,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub confidence: Confidence,
    pub source: IdentitySource,
    pub evidence: Vec<IdentityEvidence>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub enum IdentitySource {
    LoaderHeader,
    ImportTable,
    SectionName,
    StringPattern,
    RichHeader,
    SymbolTable,
    DebugInfo,
    Entropy,
    Overlay,
    DieRule,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityEvidence {
    pub source: IdentitySource,
    pub location: EvidenceLocation,
    pub description: String,
    pub matched: String,
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
}
