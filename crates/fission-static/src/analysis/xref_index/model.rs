//! Canonical xref record types (loader + disassembly + future pcode layers).

use std::collections::BTreeMap;

use serde::Serialize;

pub type XrefId = u32;

/// Where the xref originates (minimal first revision — extend toward Ghidra `Reference` parity).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct XrefSource {
    pub address: u64,
    pub category: XrefSourceCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefSourceCategory {
    Instruction {
        enclosing_function: Option<u64>,
    },
    Data {
        section: String,
    },
    #[serde(rename = "loader_metadata")]
    LoaderMetadata {
        label: String,
    },
}

/// Resolved target (VA when known + optional symbol).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct XrefTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefKind {
    Call,
    Jump,
    ConditionalJump,
    DataRead,
    DataWrite,
    StringRef,
    ImportRef,
    ExportRef,
    GlobalSymbol,
    Relocation,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XrefSourceLayer {
    Loader,
    Relocation,
    SymbolTable,
    Disassembly,
    Pcode,
    StaticAnalyzer,
    DebugInfo,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct XrefEvidence {
    pub layer: XrefSourceLayer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction_mnemonic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pcode_op: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relocation_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct XrefRecord {
    pub id: XrefId,
    pub source: XrefSource,
    pub target: XrefTarget,
    pub kind: XrefKind,
    pub confidence: fission_loader::Confidence,
    pub evidence: XrefEvidence,
}

/// Aggregated buckets for CLI / benchmarks.
#[derive(Debug, Clone, Default, Serialize)]
pub struct XrefIndexSummary {
    pub total: usize,
    pub imports: usize,
    pub exports: usize,
    pub strings: usize,
    pub globals: usize,
    pub relocations: usize,
    pub calls: usize,
    pub jumps: usize,
    pub data: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_kind: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub by_layer: BTreeMap<String, usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relocation_note: Option<String>,
}

/// Per-function xref slices (IDs into parent [`super::XrefIndex::refs`]).
#[derive(Debug, Clone, Default, Serialize)]
pub struct FunctionXrefsSummary {
    pub function_address: u64,
    pub calls_out: Vec<XrefId>,
    pub callers: Vec<XrefId>,
    pub jumps_out: Vec<XrefId>,
    pub strings: Vec<XrefId>,
    pub globals_read: Vec<XrefId>,
    pub globals_written: Vec<XrefId>,
    pub imports_used: Vec<XrefId>,
}
