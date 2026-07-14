//! Typed, immutable program metadata shared by Fission analysis layers.
//!
//! `fission-loader` owns format parsing. This crate converts those raw facts
//! into a deterministic snapshot with stable table IDs and explicit
//! provenance. It intentionally contains no binary parser or NIR pass.

use fission_loader::{FunctionInfo, LoadedBinary};
use serde::Serialize;
use std::collections::BTreeMap;

mod integrity;
pub use integrity::{
    FactInventory, SnapshotIntegrityIssue, SnapshotIntegrityReport, SnapshotTable,
};

pub const PROGRAM_SNAPSHOT_SCHEMA: &str = "fission-program-snapshot-v1";

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);
    };
}

typed_id!(MemoryBlockId);
typed_id!(FunctionId);
typed_id!(SymbolId);
typed_id!(RelocationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactConfidence {
    Proven,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactSource {
    Loader,
    SectionTable,
    ImportTable,
    ExportTable,
    SymbolTable,
    DebugInfo,
    RelocationTable,
    StaticDiscovery,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provenance {
    pub source: FactSource,
    pub confidence: FactConfidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BinaryMetadata {
    pub content_hash: String,
    pub format: String,
    pub bitness: u8,
    pub image_base: u64,
    pub entry_point: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endian: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compiler_spec_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryBlock {
    pub id: MemoryBlockId,
    pub name: String,
    pub start: u64,
    /// Bytes present in the mapped address space.
    pub size: u64,
    /// Format-declared in-memory size before initialized data expansion.
    pub virtual_size: u64,
    pub file_offset: u64,
    pub file_size: u64,
    pub permissions: Permissions,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionKind {
    Code,
    Import,
    ImportThunk,
    Export,
    DebugSymbol,
    Thunk,
    Discovered,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionRecord {
    pub id: FunctionId,
    pub name: String,
    pub entry: u64,
    pub size: u64,
    pub kind: FunctionKind,
    pub is_import: bool,
    pub is_export: bool,
    pub is_thunk: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thunk_target: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_block: Option<MemoryBlockId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_library: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Import,
    Export,
    Function,
    Data,
    RelocationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SymbolRecord {
    pub id: SymbolId,
    pub name: String,
    pub address: u64,
    pub kind: SymbolKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RelocationRecord {
    pub id: RelocationId,
    pub address: u64,
    pub relocation_type: u32,
    pub size: u8,
    pub addend: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<SymbolId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramSnapshot {
    pub schema: &'static str,
    pub binary: BinaryMetadata,
    pub memory_blocks: Vec<MemoryBlock>,
    pub functions: Vec<FunctionRecord>,
    pub symbols: Vec<SymbolRecord>,
    pub relocations: Vec<RelocationRecord>,
}

impl ProgramSnapshot {
    pub fn empty() -> Self {
        Self {
            schema: PROGRAM_SNAPSHOT_SCHEMA,
            binary: BinaryMetadata {
                content_hash: String::new(),
                format: String::new(),
                bitness: 0,
                image_base: 0,
                entry_point: 0,
                processor: None,
                endian: None,
                abi: None,
                language_id: None,
                compiler_spec_id: None,
            },
            memory_blocks: Vec::new(),
            functions: Vec::new(),
            symbols: Vec::new(),
            relocations: Vec::new(),
        }
    }

    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let memory_blocks = build_memory_blocks(binary);
        let functions = build_functions(&binary.functions, &memory_blocks);
        let symbols = build_symbols(binary, &functions);
        let relocations = build_relocations(binary, &symbols);
        let architecture = binary.architecture.as_ref();
        let load_spec = binary.load_spec.as_ref();

        Self {
            schema: PROGRAM_SNAPSHOT_SCHEMA,
            binary: BinaryMetadata {
                content_hash: binary.hash.clone(),
                format: binary.format.clone(),
                bitness: if binary.is_64bit { 64 } else { 32 },
                image_base: binary.image_base,
                entry_point: binary.entry_point,
                processor: architecture.map(|value| value.processor.clone()),
                endian: architecture.map(|value| value.endian.clone()),
                abi: architecture.and_then(|value| value.abi.clone()),
                language_id: load_spec.map(|value| value.pair.language_id.as_str().to_string()),
                compiler_spec_id: load_spec
                    .map(|value| value.pair.compiler_spec_id.as_str().to_string()),
            },
            memory_blocks,
            functions,
            symbols,
            relocations,
        }
    }

    pub fn try_from_loaded_binary(binary: &LoadedBinary) -> Result<Self, SnapshotIntegrityReport> {
        let snapshot = Self::from_loaded_binary(binary);
        let report = snapshot.integrity_report();
        if report.is_valid() {
            Ok(snapshot)
        } else {
            Err(report)
        }
    }

    pub fn function_at(&self, entry: u64) -> Option<&FunctionRecord> {
        let index = self
            .functions
            .partition_point(|function| function.entry < entry);
        self.functions
            .get(index)
            .filter(|function| function.entry == entry)
    }

    pub fn functions_at(&self, entry: u64) -> impl Iterator<Item = &FunctionRecord> {
        self.functions
            .iter()
            .skip_while(move |function| function.entry < entry)
            .take_while(move |function| function.entry == entry)
    }

    /// Return the most specific function range containing `address`.
    ///
    /// Exact entries win. For overlapping non-zero ranges, the smallest range
    /// wins and stable IDs break ties, keeping the query deterministic.
    pub fn function_containing(&self, address: u64) -> Option<&FunctionRecord> {
        self.function_at(address).or_else(|| {
            self.functions
                .iter()
                .filter(|function| {
                    function.size != 0
                        && function.entry < address
                        && function
                            .entry
                            .checked_add(function.size)
                            .is_some_and(|end| address < end)
                })
                .min_by_key(|function| (function.size, function.entry, function.id))
        })
    }

    pub fn memory_block_containing(&self, address: u64) -> Option<&MemoryBlock> {
        self.memory_blocks.iter().find(|block| {
            address >= block.start
                && block
                    .start
                    .checked_add(block.size)
                    .is_some_and(|end| address < end)
        })
    }

    pub fn symbols_at(&self, address: u64) -> impl Iterator<Item = &SymbolRecord> {
        self.symbols
            .iter()
            .filter(move |symbol| symbol.address == address)
    }

    pub fn relocations_at(&self, address: u64) -> impl Iterator<Item = &RelocationRecord> {
        self.relocations
            .iter()
            .skip_while(move |relocation| relocation.address < address)
            .take_while(move |relocation| relocation.address == address)
    }

    pub fn memory_blocks_overlapping(
        &self,
        start: u64,
        end: u64,
    ) -> impl Iterator<Item = &MemoryBlock> {
        self.memory_blocks.iter().filter(move |block| {
            start < end
                && block.start < end
                && block
                    .start
                    .checked_add(block.size)
                    .is_some_and(|block_end| start < block_end)
        })
    }

    pub fn fact_inventory(&self) -> FactInventory {
        integrity::fact_inventory(self)
    }

    /// Validate the immutable snapshot contract without consulting parser or
    /// decompiler state. Consumers can fail closed before caching these facts.
    pub fn integrity_report(&self) -> SnapshotIntegrityReport {
        integrity::integrity_report(self)
    }
}

impl Default for ProgramSnapshot {
    fn default() -> Self {
        Self::empty()
    }
}

fn build_memory_blocks(binary: &LoadedBinary) -> Vec<MemoryBlock> {
    let mut sorted = binary.sections.clone();
    sorted.sort_by(|left, right| {
        left.virtual_address
            .cmp(&right.virtual_address)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.file_offset.cmp(&right.file_offset))
    });

    let mut blocks = Vec::with_capacity(sorted.len() + 1);
    if is_pe_format(&binary.format) {
        let first_section_start = sorted.first().map(|section| section.virtual_address);
        let first_raw_offset = sorted
            .iter()
            .filter_map(|section| (section.file_offset != 0).then_some(section.file_offset))
            .min();
        if let (Some(section_start), Some(raw_size)) = (first_section_start, first_raw_offset) {
            let address_gap = section_start.saturating_sub(binary.image_base);
            if binary.image_base != 0 && raw_size != 0 && raw_size <= address_gap {
                blocks.push(MemoryBlock {
                    id: MemoryBlockId(0),
                    name: "Headers".into(),
                    start: binary.image_base,
                    size: raw_size,
                    virtual_size: raw_size,
                    file_offset: 0,
                    file_size: raw_size,
                    permissions: Permissions {
                        read: true,
                        write: false,
                        execute: false,
                    },
                    provenance: Provenance {
                        source: FactSource::Loader,
                        confidence: FactConfidence::Proven,
                        detail: Some("PE mapped headers".into()),
                    },
                });
            }
        }
    }

    blocks.extend(sorted.into_iter().map(|section| MemoryBlock {
        id: MemoryBlockId(0),
        name: section.name,
        start: section.virtual_address,
        size: section.virtual_size.max(section.file_size),
        virtual_size: section.virtual_size,
        file_offset: section.file_offset,
        file_size: section.file_size,
        permissions: Permissions {
            read: section.is_readable,
            write: section.is_writable,
            execute: section.is_executable,
        },
        provenance: Provenance {
            source: FactSource::SectionTable,
            confidence: FactConfidence::Proven,
            detail: None,
        },
    }));
    for (index, block) in blocks.iter_mut().enumerate() {
        block.id = MemoryBlockId(index as u32);
    }
    blocks
}

fn is_pe_format(format: &str) -> bool {
    let normalized = format.trim().to_ascii_lowercase();
    normalized == "pe" || normalized == "portable executable"
}

fn build_functions(functions: &[FunctionInfo], blocks: &[MemoryBlock]) -> Vec<FunctionRecord> {
    let mut sorted = functions.to_vec();
    sorted.retain(|function| {
        !function.is_import
            || blocks.iter().any(|block| {
                block.permissions.execute
                    && function.address >= block.start
                    && function.address < block.start.saturating_add(block.size)
            })
    });
    sorted.sort_by(|left, right| {
        left.address
            .cmp(&right.address)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.size.cmp(&right.size))
    });
    sorted
        .into_iter()
        .enumerate()
        .map(|(index, function)| {
            let source = fact_source(function.origin.as_deref());
            let kind = function_kind(&function);
            let block = blocks.iter().find(|block| {
                function.address >= block.start
                    && function.address < block.start.saturating_add(block.size)
            });
            FunctionRecord {
                id: FunctionId(index as u32),
                name: function.name,
                entry: function.address,
                size: function.size,
                kind,
                is_import: function.is_import,
                is_export: function.is_export,
                is_thunk: function.is_thunk_like,
                thunk_target: function.thunk_target,
                memory_block: block.map(|value| value.id),
                external_library: function.external_library,
                provenance: Provenance {
                    source,
                    confidence: confidence_for_source(source),
                    detail: function.origin,
                },
            }
        })
        .collect()
}

fn build_symbols(binary: &LoadedBinary, functions: &[FunctionRecord]) -> Vec<SymbolRecord> {
    let mut rows: BTreeMap<(u64, SymbolKind, String), (Option<u64>, Option<String>, FactSource)> =
        BTreeMap::new();

    for function in functions {
        if function.name.is_empty() {
            continue;
        }
        let kind = if function.is_import {
            SymbolKind::Import
        } else if function.is_export {
            SymbolKind::Export
        } else {
            SymbolKind::Function
        };
        rows.insert(
            (function.entry, kind, function.name.clone()),
            (
                (function.size != 0).then_some(function.size),
                binary.symbol_versions.get(&function.entry).cloned(),
                function.provenance.source,
            ),
        );
    }
    for (address, name) in &binary.iat_symbols {
        rows.insert(
            (*address, SymbolKind::Import, name.clone()),
            (
                None,
                binary.symbol_versions.get(address).cloned(),
                FactSource::ImportTable,
            ),
        );
    }
    for (address, name) in &binary.global_symbols {
        rows.insert(
            (*address, SymbolKind::Data, name.clone()),
            (
                binary.global_symbol_sizes.get(address).copied(),
                binary.symbol_versions.get(address).cloned(),
                FactSource::SymbolTable,
            ),
        );
    }
    rows.into_iter()
        .enumerate()
        .map(
            |(index, ((address, kind, name), (size, version, source)))| SymbolRecord {
                id: SymbolId(index as u32),
                name,
                address,
                kind,
                size,
                version,
                provenance: Provenance {
                    source,
                    confidence: confidence_for_source(source),
                    detail: None,
                },
            },
        )
        .collect()
}

fn build_relocations(binary: &LoadedBinary, symbols: &[SymbolRecord]) -> Vec<RelocationRecord> {
    let symbol_ids: BTreeMap<(u64, &str), SymbolId> = symbols
        .iter()
        .map(|symbol| ((symbol.address, symbol.name.as_str()), symbol.id))
        .collect();
    let mut rows = binary.relocations.clone();
    rows.sort_by(|left, right| {
        left.address
            .cmp(&right.address)
            .then_with(|| left.r_type.cmp(&right.r_type))
            .then_with(|| left.symbol_name.cmp(&right.symbol_name))
    });
    rows.into_iter()
        .enumerate()
        .map(|(index, relocation)| {
            let symbol = relocation.symbol_name.as_deref().and_then(|name| {
                symbol_ids
                    .get(&(relocation.address, name))
                    .copied()
                    .or_else(|| {
                        symbols
                            .iter()
                            .find(|value| value.name == name)
                            .map(|value| value.id)
                    })
            });
            RelocationRecord {
                id: RelocationId(index as u32),
                address: relocation.address,
                relocation_type: relocation.r_type,
                size: relocation.size,
                addend: relocation.addend,
                symbol,
                symbol_name: relocation.symbol_name,
                provenance: Provenance {
                    source: FactSource::RelocationTable,
                    confidence: FactConfidence::Proven,
                    detail: None,
                },
            }
        })
        .collect()
}

fn function_kind(function: &FunctionInfo) -> FunctionKind {
    match function
        .kind
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("code") => FunctionKind::Code,
        Some("import") => FunctionKind::Import,
        Some("import_thunk") => FunctionKind::ImportThunk,
        Some("export") => FunctionKind::Export,
        Some("debug_symbol") => FunctionKind::DebugSymbol,
        Some("thunk") => FunctionKind::Thunk,
        Some("discovered") => FunctionKind::Discovered,
        _ if function.is_import && function.is_thunk_like => FunctionKind::ImportThunk,
        _ if function.is_import => FunctionKind::Import,
        _ if function.is_export => FunctionKind::Export,
        _ if function.is_thunk_like => FunctionKind::Thunk,
        _ => FunctionKind::Unknown,
    }
}

fn fact_source(origin: Option<&str>) -> FactSource {
    let origin = origin.unwrap_or_default().to_ascii_lowercase();
    if origin.contains("import") || origin.contains("iat") {
        FactSource::ImportTable
    } else if origin.contains("export") {
        FactSource::ExportTable
    } else if origin.contains("dwarf") || origin.contains("pdb") || origin.contains("debug") {
        FactSource::DebugInfo
    } else if origin.contains("symbol") {
        FactSource::SymbolTable
    } else if origin.contains("discover") || origin.contains("candidate") {
        FactSource::StaticDiscovery
    } else if origin.is_empty() {
        FactSource::Unknown
    } else {
        FactSource::Loader
    }
}

fn confidence_for_source(source: FactSource) -> FactConfidence {
    match source {
        FactSource::SectionTable
        | FactSource::ImportTable
        | FactSource::ExportTable
        | FactSource::DebugInfo
        | FactSource::RelocationTable => FactConfidence::Proven,
        FactSource::SymbolTable | FactSource::Loader => FactConfidence::High,
        FactSource::StaticDiscovery => FactConfidence::Medium,
        FactSource::Unknown => FactConfidence::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, RelocationEntry};
    use fission_loader::SectionInfo;

    fn fixture(reverse: bool) -> LoadedBinary {
        let functions = vec![
            FunctionInfo {
                name: "second".into(),
                address: 0x1010,
                size: 8,
                origin: Some("static_discovery".into()),
                ..FunctionInfo::default()
            },
            FunctionInfo {
                name: "first".into(),
                address: 0x1000,
                size: 16,
                is_export: true,
                origin: Some("export_table".into()),
                kind: Some("export".into()),
                ..FunctionInfo::default()
            },
        ];
        let sections = vec![SectionInfo {
            name: ".text".into(),
            virtual_address: 0x1000,
            virtual_size: 0x100,
            file_offset: 0x200,
            file_size: 0x120,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        }];
        let ordered = if reverse {
            functions.into_iter().rev().collect::<Vec<_>>()
        } else {
            functions
        };
        LoadedBinaryBuilder::new("fixture.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("PE")
            .image_base(0x1000)
            .entry_point(0x1000)
            .is_64bit(true)
            .add_functions(ordered)
            .add_sections(sections)
            .add_global_symbol(0x1020, "global_value".into())
            .add_relocations([RelocationEntry {
                address: 0x1020,
                r_type: 3,
                size: 8,
                addend: 0,
                symbol_name: Some("global_value".into()),
            }])
            .build()
            .expect("fixture should build")
    }

    #[test]
    fn snapshot_ids_are_deterministic() {
        let left = ProgramSnapshot::from_loaded_binary(&fixture(false));
        let right = ProgramSnapshot::from_loaded_binary(&fixture(true));
        assert_eq!(
            serde_json::to_string(&left).unwrap(),
            serde_json::to_string(&right).unwrap()
        );
        assert_eq!(left.function_at(0x1000).unwrap().id, FunctionId(0));
        assert_eq!(
            left.memory_block_containing(0x1010).unwrap().id,
            MemoryBlockId(0)
        );
        assert_eq!(left.memory_blocks[0].size, 0x120);
        assert_eq!(left.memory_blocks[0].virtual_size, 0x100);
    }

    #[test]
    fn snapshot_preserves_provenance_and_links() {
        let snapshot = ProgramSnapshot::from_loaded_binary(&fixture(false));
        let first = snapshot.function_at(0x1000).unwrap();
        assert_eq!(first.provenance.source, FactSource::ExportTable);
        assert_eq!(first.provenance.confidence, FactConfidence::Proven);
        assert_eq!(first.memory_block, Some(MemoryBlockId(0)));
        assert_eq!(snapshot.relocations.len(), 1);
        assert!(snapshot.relocations[0].symbol.is_some());
    }

    #[test]
    fn snapshot_integrity_and_inventory_cover_all_fact_tables() {
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&fixture(false))
            .expect("fixture snapshot should satisfy the canonical contract");
        let report = snapshot.integrity_report();
        assert!(report.is_valid(), "{:#?}", report.issues);

        let inventory = snapshot.fact_inventory();
        assert_eq!(inventory.total, 7);
        assert_eq!(inventory.memory_blocks, 1);
        assert_eq!(inventory.functions, 2);
        assert_eq!(inventory.symbols, 3);
        assert_eq!(inventory.relocations, 1);
        assert_eq!(
            inventory.by_source.get(&FactSource::StaticDiscovery),
            Some(&2)
        );
        assert_eq!(
            inventory.by_confidence.get(&FactConfidence::Medium),
            Some(&2)
        );
    }

    #[test]
    fn snapshot_range_queries_are_deterministic() {
        let snapshot = ProgramSnapshot::from_loaded_binary(&fixture(false));
        assert_eq!(snapshot.functions_at(0x1000).count(), 1);
        assert_eq!(
            snapshot
                .function_containing(0x1008)
                .map(|row| row.name.as_str()),
            Some("first")
        );
        assert_eq!(
            snapshot
                .function_containing(0x1012)
                .map(|row| row.name.as_str()),
            Some("second")
        );
        assert_eq!(snapshot.relocations_at(0x1020).count(), 1);
        assert_eq!(
            snapshot.memory_blocks_overlapping(0x1010, 0x1030).count(),
            1
        );
        assert_eq!(
            snapshot.memory_blocks_overlapping(0x1030, 0x1030).count(),
            0
        );
    }

    #[test]
    fn snapshot_integrity_reports_corrupt_ids_links_and_ranges() {
        let mut snapshot = ProgramSnapshot::from_loaded_binary(&fixture(false));
        snapshot.functions[0].id = FunctionId(9);
        snapshot.functions[0].memory_block = Some(MemoryBlockId(9));
        snapshot.relocations[0].symbol = Some(SymbolId(9));
        snapshot.memory_blocks[0].start = u64::MAX - 1;
        snapshot.memory_blocks[0].size = 8;

        let report = snapshot.integrity_report();
        assert!(!report.is_valid());
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::InvalidTableId {
                table: SnapshotTable::Functions,
                ..
            }
        )));
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::AddressRangeOverflow {
                table: SnapshotTable::MemoryBlocks,
                ..
            }
        )));
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DanglingReference {
                table: SnapshotTable::Functions,
                target_table: SnapshotTable::MemoryBlocks,
                ..
            }
        )));
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DanglingReference {
                table: SnapshotTable::Relocations,
                target_table: SnapshotTable::Symbols,
                ..
            }
        )));
    }

    #[test]
    fn non_executable_import_slots_are_symbols_not_functions() {
        let binary = LoadedBinaryBuilder::new("imports.exe".into(), DataBuffer::Heap(vec![0; 16]))
            .format("PE")
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".idata".into(),
                virtual_address: 0x3000,
                virtual_size: 0x100,
                file_offset: 0,
                file_size: 0x100,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            })
            .add_function(FunctionInfo {
                name: "example.dll!imported".into(),
                address: 0x3010,
                is_import: true,
                kind: Some("import".into()),
                ..FunctionInfo::default()
            })
            .add_iat_symbol(0x3010, "example.dll!imported".into())
            .build()
            .expect("fixture should build");

        let snapshot = ProgramSnapshot::from_loaded_binary(&binary);
        assert!(snapshot.functions.is_empty());
        assert!(snapshot
            .symbols
            .iter()
            .any(|symbol| { symbol.address == 0x3010 && symbol.kind == SymbolKind::Import }));
    }

    #[test]
    fn pe_snapshot_maps_file_headers_before_first_section() {
        let binary = LoadedBinaryBuilder::new("headers.exe".into(), DataBuffer::Heap(vec![0; 16]))
            .format("PE")
            .image_base(0x140000000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".into(),
                virtual_address: 0x140001000,
                virtual_size: 0x1000,
                file_offset: 0x600,
                file_size: 0x800,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("fixture should build");

        let snapshot = ProgramSnapshot::from_loaded_binary(&binary);
        let headers = &snapshot.memory_blocks[0];
        assert_eq!(headers.name, "Headers");
        assert_eq!(headers.start, 0x140000000);
        assert_eq!(headers.size, 0x600);
        assert_eq!(headers.file_size, 0x600);
        assert_eq!(
            headers.permissions,
            Permissions {
                read: true,
                write: false,
                execute: false,
            }
        );
        assert_eq!(headers.provenance.source, FactSource::Loader);
        assert_eq!(snapshot.memory_blocks[1].name, ".text");
    }
}
