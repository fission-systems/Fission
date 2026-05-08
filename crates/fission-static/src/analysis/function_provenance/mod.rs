//! Evidence-backed classification: entry point, imports, and explicit thunks.

use fission_loader::loader::{FunctionInfo, LoadedBinary};
use rustc_hash::FxHashMap;
use serde::Serialize;

use super::external_symbol::ExternalSymbolIndex;
pub use fission_loader::Confidence;

/// Coarse provenance bucket for benchmarks, batch policy, and debug bundles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionProvenanceKind {
    Unknown,
    /// Entry point VA for the loaded module.
    StartupOrEntry,
    /// Import table / stub / undefined external placeholder.
    ImportThunkOrStub,
    /// Loader-resolved thunk with a known forward target.
    ForwarderThunk,
}

#[derive(Debug, Clone, Serialize)]
pub struct FunctionProvenanceRecord {
    pub address: u64,
    pub kind: FunctionProvenanceKind,
    pub confidence: Confidence,
    pub evidence: Vec<String>,
}

impl FunctionProvenanceRecord {
    /// Additional batch exclusions on top of `canonical_functions_sorted` filters.
    #[must_use]
    pub fn exclude_from_default_batch_decompile(&self) -> bool {
        matches!(
            self.kind,
            FunctionProvenanceKind::ImportThunkOrStub | FunctionProvenanceKind::ForwarderThunk
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct FunctionProvenanceIndex {
    pub records: FxHashMap<u64, FunctionProvenanceRecord>,
}

fn merge_record(
    existing: FunctionProvenanceRecord,
    incoming: FunctionProvenanceRecord,
) -> FunctionProvenanceRecord {
    match incoming.confidence.cmp(&existing.confidence) {
        std::cmp::Ordering::Greater => incoming,
        std::cmp::Ordering::Less => existing,
        std::cmp::Ordering::Equal => {
            if incoming.evidence.len() >= existing.evidence.len() {
                incoming
            } else {
                existing
            }
        }
    }
}

fn classify_function(
    f: &FunctionInfo,
    entry_point: u64,
    _ext: Option<&ExternalSymbolIndex>,
) -> FunctionProvenanceRecord {
    let mut evidence = Vec::new();

    if f.address == entry_point {
        evidence.push(format!("address_matches_entry_point=0x{:x}", entry_point));
        return FunctionProvenanceRecord {
            address: f.address,
            kind: FunctionProvenanceKind::StartupOrEntry,
            confidence: Confidence::High,
            evidence,
        };
    }

    if f.is_import {
        evidence.push("function_info.is_import=true".into());
        return FunctionProvenanceRecord {
            address: f.address,
            kind: FunctionProvenanceKind::ImportThunkOrStub,
            confidence: Confidence::High,
            evidence,
        };
    }

    if matches!(
        f.kind.as_deref(),
        Some("import" | "import_thunk" | "undefined_external")
    ) {
        evidence.push(format!("function_info.kind={:?}", f.kind));
        return FunctionProvenanceRecord {
            address: f.address,
            kind: FunctionProvenanceKind::ImportThunkOrStub,
            confidence: Confidence::High,
            evidence,
        };
    }

    if f.is_thunk_like && f.thunk_target.is_some() {
        evidence.push(format!(
            "thunk_like=true thunk_target=0x{:x}",
            f.thunk_target.unwrap_or(0)
        ));
        return FunctionProvenanceRecord {
            address: f.address,
            kind: FunctionProvenanceKind::ForwarderThunk,
            confidence: Confidence::Medium,
            evidence,
        };
    }

    evidence.push("no_strong_classifier".into());
    FunctionProvenanceRecord {
        address: f.address,
        kind: FunctionProvenanceKind::Unknown,
        confidence: Confidence::Medium,
        evidence,
    }
}

/// One record per function address (merges duplicate `FunctionInfo` rows by confidence).
#[must_use]
pub fn build_function_provenance_index(
    binary: &LoadedBinary,
    ext: Option<&ExternalSymbolIndex>,
) -> FunctionProvenanceIndex {
    let mut records: FxHashMap<u64, FunctionProvenanceRecord> = FxHashMap::default();
    for f in &binary.functions {
        let next = classify_function(f, binary.entry_point, ext);
        records
            .entry(f.address)
            .and_modify(|e| {
                let merged = merge_record(e.clone(), next.clone());
                *e = merged;
            })
            .or_insert(next);
    }
    FunctionProvenanceIndex { records }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo};

    #[test]
    fn entry_point_classified() {
        let bin = LoadedBinaryBuilder::new("t.exe".to_string(), DataBuffer::Heap(vec![0u8; 32]))
            .format("PE64")
            .entry_point(0x140001000)
            .image_base(0x140000000)
            .add_function(FunctionInfo {
                name: "entry".into(),
                address: 0x140001000,
                size: 16,
                is_export: true,
                is_import: false,
                ..Default::default()
            })
            .build()
            .expect("build");
        let idx = build_function_provenance_index(&bin, None);
        let r = idx.records.get(&0x140001000).unwrap();
        assert_eq!(r.kind, FunctionProvenanceKind::StartupOrEntry);
        assert_eq!(r.confidence, Confidence::High);
    }

    #[test]
    fn runtime_name_without_explicit_evidence_is_unknown() {
        let bin = LoadedBinaryBuilder::new("t.exe".to_string(), DataBuffer::Heap(vec![0u8; 32]))
            .format("PE64")
            .entry_point(0x140001000)
            .image_base(0x140000000)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x140001020,
                virtual_size: 64,
                file_offset: 0,
                file_size: 64,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_function(FunctionInfo {
                name: "__dyn_tls_init".into(),
                address: 0x140001020,
                size: 32,
                is_export: false,
                is_import: false,
                ..Default::default()
            })
            .build()
            .expect("build");
        let idx = build_function_provenance_index(&bin, None);
        let r = idx.records.get(&0x140001020).unwrap();
        assert_eq!(r.kind, FunctionProvenanceKind::Unknown);
        assert!(!r.exclude_from_default_batch_decompile());
    }
}
