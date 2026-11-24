//! Typed, immutable program metadata shared by Fission analysis layers.
//!
//! `fission-loader` owns format parsing. This crate converts those raw facts
//! into a deterministic snapshot with stable table IDs and explicit
//! provenance. It intentionally contains no binary parser or NIR pass.

use fission_loader::{FunctionInfo, LoadedBinary, LoaderSymbolKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

mod integrity;
pub use integrity::{
    FactInventory, SnapshotIntegrityIssue, SnapshotIntegrityReport, SnapshotTable,
};

pub const PROGRAM_SNAPSHOT_SCHEMA: &str = "fission-program-snapshot-v1";

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u32);
    };
}

typed_id!(MemoryBlockId);
typed_id!(FunctionId);
typed_id!(SymbolId);
typed_id!(RelocationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactConfidence {
    Proven,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub source: FactSource,
    pub confidence: FactConfidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Import,
    Export,
    Function,
    Label,
    Data,
    RelocationTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AddressInterval {
    start: u64,
    end: u64,
    index: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct AddressIntervalIndex {
    intervals: Vec<AddressInterval>,
    prefix_max_end: Vec<u64>,
}

impl AddressIntervalIndex {
    fn from_items<T>(items: &[T], mut range: impl FnMut(&T) -> Option<(u64, u64)>) -> Self {
        let mut intervals = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let (start, end) = range(item)?;
                (start < end).then_some(AddressInterval { start, end, index })
            })
            .collect::<Vec<_>>();
        intervals.sort_unstable_by_key(|interval| (interval.start, interval.end, interval.index));

        let mut prefix_max_end = Vec::with_capacity(intervals.len());
        let mut max_end = 0;
        for interval in &intervals {
            max_end = max_end.max(interval.end);
            prefix_max_end.push(max_end);
        }

        Self {
            intervals,
            prefix_max_end,
        }
    }

    fn find_best(
        &self,
        address: u64,
        mut is_better: impl FnMut(usize, Option<usize>) -> bool,
    ) -> Option<usize> {
        let mut cursor = self
            .intervals
            .partition_point(|interval| interval.start <= address);
        let mut best = None;

        while cursor > 0 {
            if self.prefix_max_end[cursor - 1] <= address {
                break;
            }
            cursor -= 1;
            let interval = self.intervals[cursor];
            if address < interval.end && is_better(interval.index, best) {
                best = Some(interval.index);
            }
        }

        best
    }

    fn any_containing(&self, address: u64, mut predicate: impl FnMut(usize) -> bool) -> bool {
        let mut cursor = self
            .intervals
            .partition_point(|interval| interval.start <= address);

        while cursor > 0 {
            if self.prefix_max_end[cursor - 1] <= address {
                break;
            }
            cursor -= 1;
            let interval = self.intervals[cursor];
            if address < interval.end && predicate(interval.index) {
                return true;
            }
        }

        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgramSnapshot {
    schema: String,
    binary: BinaryMetadata,
    memory_blocks: Vec<MemoryBlock>,
    functions: Vec<FunctionRecord>,
    symbols: Vec<SymbolRecord>,
    relocations: Vec<RelocationRecord>,
    #[serde(skip)]
    function_intervals: AddressIntervalIndex,
    #[serde(skip)]
    memory_block_intervals: AddressIntervalIndex,
}

#[derive(Deserialize)]
struct RawProgramSnapshot {
    schema: String,
    binary: BinaryMetadata,
    memory_blocks: Vec<MemoryBlock>,
    functions: Vec<FunctionRecord>,
    symbols: Vec<SymbolRecord>,
    relocations: Vec<RelocationRecord>,
}

impl<'de> serde::Deserialize<'de> for ProgramSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawProgramSnapshot::deserialize(deserializer)?;
        ProgramSnapshotBuilder::from_raw_parts(
            raw.schema,
            raw.binary,
            raw.memory_blocks,
            raw.functions,
            raw.symbols,
            raw.relocations,
        )
        .build()
        .map_err(serde::de::Error::custom)
    }
}

pub(crate) fn build_memory_block_intervals(memory_blocks: &[MemoryBlock]) -> AddressIntervalIndex {
    AddressIntervalIndex::from_items(memory_blocks, |block| {
        block
            .start
            .checked_add(block.size)
            .map(|end| (block.start, end))
    })
}

pub(crate) fn build_function_intervals(functions: &[FunctionRecord]) -> AddressIntervalIndex {
    AddressIntervalIndex::from_items(functions, |function| {
        function
            .entry
            .checked_add(function.size)
            .map(|end| (function.entry, end))
    })
}

/// Diagnostic snapshot returned by [`SnapshotPublication::Partial`] when a staged
/// snapshot fails integrity validation. Tables remain readable for diagnostics,
/// while derived query indexes are withheld so an unvalidated snapshot cannot
/// masquerade as authoritative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartialProgramSnapshot {
    schema: String,
    binary: BinaryMetadata,
    memory_blocks: Vec<MemoryBlock>,
    functions: Vec<FunctionRecord>,
    symbols: Vec<SymbolRecord>,
    relocations: Vec<RelocationRecord>,
    report: SnapshotIntegrityReport,
}

impl PartialProgramSnapshot {
    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn binary(&self) -> &BinaryMetadata {
        &self.binary
    }

    pub fn memory_blocks(&self) -> &[MemoryBlock] {
        &self.memory_blocks
    }

    pub fn functions(&self) -> &[FunctionRecord] {
        &self.functions
    }

    pub fn symbols(&self) -> &[SymbolRecord] {
        &self.symbols
    }

    pub fn relocations(&self) -> &[RelocationRecord] {
        &self.relocations
    }

    pub fn integrity_report(&self) -> &SnapshotIntegrityReport {
        &self.report
    }

    pub fn fact_inventory(&self) -> FactInventory {
        integrity::fact_inventory_from_tables(
            &self.memory_blocks,
            &self.functions,
            &self.symbols,
            &self.relocations,
        )
    }

    pub fn to_builder(&self) -> ProgramSnapshotBuilder {
        ProgramSnapshotBuilder {
            schema: self.schema.clone(),
            binary: self.binary.clone(),
            memory_blocks: self.memory_blocks.clone(),
            functions: self.functions.clone(),
            symbols: self.symbols.clone(),
            relocations: self.relocations.clone(),
        }
    }
}

/// Outcome of the checked publication boundary on [`ProgramSnapshotBuilder::publish`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotPublication {
    Valid(ProgramSnapshot),
    Partial(PartialProgramSnapshot),
}

impl SnapshotPublication {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid(_))
    }

    pub fn into_result(self) -> Result<ProgramSnapshot, SnapshotIntegrityReport> {
        match self {
            Self::Valid(snapshot) => Ok(snapshot),
            Self::Partial(partial) => Err(partial.report),
        }
    }

    pub fn integrity_report(&self) -> SnapshotIntegrityReport {
        match self {
            Self::Valid(snapshot) => snapshot.integrity_report(),
            Self::Partial(partial) => partial.report.clone(),
        }
    }
}

/// Typed delta applied through [`ProgramSnapshotBuilder`] or
/// [`ProgramSnapshot::apply_delta`] to construct a replacement snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramSnapshotDelta {
    ReplaceBinaryMetadata(BinaryMetadata),
    UpdateFunctionRange {
        id: FunctionId,
        entry: u64,
        size: u64,
    },
    AddFunction(FunctionRecord),
    RemoveFunction(FunctionId),
    UpdateMemoryBlockRange {
        id: MemoryBlockId,
        start: u64,
        size: u64,
    },
    AddMemoryBlock(MemoryBlock),
    RemoveMemoryBlock(MemoryBlockId),
    AddSymbol(SymbolRecord),
    RemoveSymbol(SymbolId),
    AddRelocation(RelocationRecord),
    RemoveRelocation(RelocationId),
}

/// Staged builder used to construct or derive replacement snapshots prior to
/// validation at [`Self::publish`] / [`Self::build`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSnapshotBuilder {
    schema: String,
    binary: BinaryMetadata,
    memory_blocks: Vec<MemoryBlock>,
    functions: Vec<FunctionRecord>,
    symbols: Vec<SymbolRecord>,
    relocations: Vec<RelocationRecord>,
}

fn canonicalize_ids<T, K: Ord>(
    rows: &mut [T],
    key: impl Fn(&T) -> K,
    get_id: impl Fn(&T) -> u32,
    set_id: impl Fn(&mut T, u32),
) -> HashMap<u32, u32> {
    let mut id_counts: HashMap<u32, usize> = HashMap::with_capacity(rows.len());
    for row in rows.iter() {
        *id_counts.entry(get_id(row)).or_default() += 1;
    }
    rows.sort_by_key(key);
    let mut old_to_new = HashMap::with_capacity(rows.len());
    for (index, row) in rows.iter_mut().enumerate() {
        let old_id = get_id(row);
        let new_id = index as u32;
        if id_counts.get(&old_id).copied() == Some(1) {
            old_to_new.insert(old_id, new_id);
        }
        set_id(row, new_id);
    }
    old_to_new
}

impl ProgramSnapshotBuilder {
    pub fn new(binary: BinaryMetadata) -> Self {
        Self {
            schema: PROGRAM_SNAPSHOT_SCHEMA.to_string(),
            binary,
            memory_blocks: Vec::new(),
            functions: Vec::new(),
            symbols: Vec::new(),
            relocations: Vec::new(),
        }
    }

    pub fn from_raw_parts(
        schema: String,
        binary: BinaryMetadata,
        memory_blocks: Vec<MemoryBlock>,
        functions: Vec<FunctionRecord>,
        symbols: Vec<SymbolRecord>,
        relocations: Vec<RelocationRecord>,
    ) -> Self {
        Self {
            schema,
            binary,
            memory_blocks,
            functions,
            symbols,
            relocations,
        }
    }

    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let memory_blocks = build_memory_blocks(binary);
        let memory_block_intervals = build_memory_block_intervals(&memory_blocks);
        let functions = build_functions(&binary.functions, &memory_blocks, &memory_block_intervals);
        let symbols = build_symbols(binary, &functions);
        let relocations = build_relocations(binary, &symbols);
        let architecture = binary.architecture.as_ref();
        let load_spec = binary.load_spec.as_ref();

        Self {
            schema: PROGRAM_SNAPSHOT_SCHEMA.to_string(),
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

    pub fn with_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = schema.into();
        self
    }

    pub fn with_binary(mut self, binary: BinaryMetadata) -> Self {
        self.binary = binary;
        self
    }

    pub fn memory_blocks_mut(&mut self) -> &mut Vec<MemoryBlock> {
        &mut self.memory_blocks
    }

    pub fn functions_mut(&mut self) -> &mut Vec<FunctionRecord> {
        &mut self.functions
    }

    pub fn symbols_mut(&mut self) -> &mut Vec<SymbolRecord> {
        &mut self.symbols
    }

    pub fn relocations_mut(&mut self) -> &mut Vec<RelocationRecord> {
        &mut self.relocations
    }

    fn next_raw_id<T>(rows: &[T], get_id: impl Fn(&T) -> u32) -> u32 {
        rows.iter()
            .map(get_id)
            .max()
            .map_or(0, |id| id.saturating_add(1))
    }

    pub fn apply_delta(&mut self, delta: ProgramSnapshotDelta) -> &mut Self {
        match delta {
            ProgramSnapshotDelta::ReplaceBinaryMetadata(binary) => {
                self.binary = binary;
            }
            ProgramSnapshotDelta::UpdateFunctionRange { id, entry, size } => {
                if let Some(row) = self.functions.iter_mut().find(|row| row.id == id) {
                    row.entry = entry;
                    row.size = size;
                }
            }
            ProgramSnapshotDelta::AddFunction(mut function) => {
                function.id = FunctionId(Self::next_raw_id(&self.functions, |row| row.id.0));
                self.functions.push(function);
            }
            ProgramSnapshotDelta::RemoveFunction(id) => {
                self.functions.retain(|row| row.id != id);
            }
            ProgramSnapshotDelta::UpdateMemoryBlockRange { id, start, size } => {
                if let Some(row) = self.memory_blocks.iter_mut().find(|row| row.id == id) {
                    row.start = start;
                    row.size = size;
                }
            }
            ProgramSnapshotDelta::AddMemoryBlock(mut block) => {
                block.id = MemoryBlockId(Self::next_raw_id(&self.memory_blocks, |row| row.id.0));
                self.memory_blocks.push(block);
            }
            ProgramSnapshotDelta::RemoveMemoryBlock(id) => {
                self.memory_blocks.retain(|row| row.id != id);
            }
            ProgramSnapshotDelta::AddSymbol(mut symbol) => {
                symbol.id = SymbolId(Self::next_raw_id(&self.symbols, |row| row.id.0));
                self.symbols.push(symbol);
            }
            ProgramSnapshotDelta::RemoveSymbol(id) => {
                self.symbols.retain(|row| row.id != id);
            }
            ProgramSnapshotDelta::AddRelocation(mut relocation) => {
                relocation.id = RelocationId(Self::next_raw_id(&self.relocations, |row| row.id.0));
                self.relocations.push(relocation);
            }
            ProgramSnapshotDelta::RemoveRelocation(id) => {
                self.relocations.retain(|row| row.id != id);
            }
        }
        self
    }

    /// Sort all four tables into canonical order, reassign sequential row IDs,
    /// and remap foreign keys (mapping deleted/ambiguous targets to `u32::MAX`
    /// so validation detects dangling references instead of silently aliasing).
    pub fn canonicalize(mut self) -> Self {
        let block_map = canonicalize_ids(
            &mut self.memory_blocks,
            |row| (row.start, row.name.clone(), row.file_offset),
            |row| row.id.0,
            |row, id| row.id = MemoryBlockId(id),
        );
        for function in &mut self.functions {
            function.memory_block = function
                .memory_block
                .map(|old| MemoryBlockId(block_map.get(&old.0).copied().unwrap_or(u32::MAX)));
        }

        let symbol_map = canonicalize_ids(
            &mut self.symbols,
            |row| (row.address, row.kind, row.name.clone()),
            |row| row.id.0,
            |row, id| row.id = SymbolId(id),
        );
        for relocation in &mut self.relocations {
            relocation.symbol = relocation
                .symbol
                .map(|old| SymbolId(symbol_map.get(&old.0).copied().unwrap_or(u32::MAX)));
        }

        canonicalize_ids(
            &mut self.functions,
            |row| (row.entry, row.name.clone(), row.size),
            |row| row.id.0,
            |row, id| row.id = FunctionId(id),
        );
        canonicalize_ids(
            &mut self.relocations,
            |row| (row.address, row.relocation_type, row.symbol_name.clone()),
            |row| row.id.0,
            |row, id| row.id = RelocationId(id),
        );
        self
    }

    /// Checked publication boundary: builds derived interval indexes from the
    /// staged rows and validates all snapshot invariants before exposing a
    /// [`ProgramSnapshot`].
    pub fn publish(self) -> SnapshotPublication {
        let memory_block_intervals = build_memory_block_intervals(&self.memory_blocks);
        let function_intervals = build_function_intervals(&self.functions);
        let candidate = ProgramSnapshot {
            schema: self.schema,
            binary: self.binary,
            memory_blocks: self.memory_blocks,
            functions: self.functions,
            symbols: self.symbols,
            relocations: self.relocations,
            function_intervals,
            memory_block_intervals,
        };
        let report = candidate.integrity_report();
        if report.is_valid() {
            SnapshotPublication::Valid(candidate)
        } else {
            SnapshotPublication::Partial(PartialProgramSnapshot {
                schema: candidate.schema,
                binary: candidate.binary,
                memory_blocks: candidate.memory_blocks,
                functions: candidate.functions,
                symbols: candidate.symbols,
                relocations: candidate.relocations,
                report,
            })
        }
    }

    pub fn build(self) -> Result<ProgramSnapshot, SnapshotIntegrityReport> {
        self.publish().into_result()
    }
}

impl ProgramSnapshot {
    pub fn empty() -> Self {
        Self {
            schema: PROGRAM_SNAPSHOT_SCHEMA.to_string(),
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
            function_intervals: AddressIntervalIndex::default(),
            memory_block_intervals: AddressIntervalIndex::default(),
        }
    }

    pub fn builder(binary: BinaryMetadata) -> ProgramSnapshotBuilder {
        ProgramSnapshotBuilder::new(binary)
    }

    pub fn to_builder(&self) -> ProgramSnapshotBuilder {
        ProgramSnapshotBuilder {
            schema: self.schema.clone(),
            binary: self.binary.clone(),
            memory_blocks: self.memory_blocks.clone(),
            functions: self.functions.clone(),
            symbols: self.symbols.clone(),
            relocations: self.relocations.clone(),
        }
    }

    pub fn schema(&self) -> &str {
        &self.schema
    }

    pub fn binary(&self) -> &BinaryMetadata {
        &self.binary
    }

    pub fn memory_blocks(&self) -> &[MemoryBlock] {
        &self.memory_blocks
    }

    pub fn functions(&self) -> &[FunctionRecord] {
        &self.functions
    }

    pub fn symbols(&self) -> &[SymbolRecord] {
        &self.symbols
    }

    pub fn relocations(&self) -> &[RelocationRecord] {
        &self.relocations
    }

    pub fn try_from_loaded_binary(binary: &LoadedBinary) -> Result<Self, SnapshotIntegrityReport> {
        ProgramSnapshotBuilder::from_loaded_binary(binary).build()
    }

    pub fn publish_loaded_binary(binary: &LoadedBinary) -> SnapshotPublication {
        ProgramSnapshotBuilder::from_loaded_binary(binary).publish()
    }

    pub fn apply_delta(
        &self,
        delta: ProgramSnapshotDelta,
    ) -> Result<Self, SnapshotIntegrityReport> {
        self.apply_deltas([delta])
    }

    pub fn apply_deltas(
        &self,
        deltas: impl IntoIterator<Item = ProgramSnapshotDelta>,
    ) -> Result<Self, SnapshotIntegrityReport> {
        let mut builder = self.to_builder();
        for delta in deltas {
            builder.apply_delta(delta);
        }
        builder.canonicalize().build()
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
        let start = self
            .functions
            .partition_point(|function| function.entry < entry);
        let count = self.functions[start..].partition_point(|function| function.entry == entry);
        self.functions[start..start + count].iter()
    }

    /// Return the most specific function range containing `address`.
    ///
    /// For overlapping non-zero ranges, the smallest range wins and stable IDs
    /// break ties, keeping the query deterministic. A sized function that
    /// covers `address` wins, whatever else starts there: an assembly label or
    /// alias of size zero at the same entry (`_start`, say) says where
    /// something begins, not where it ends, and letting it win at the entry made
    /// the answer change between `entry` and `entry + 4` of the one function.
    /// A zero-sized function is returned only when no sized function covers
    /// the address.
    pub fn function_containing(&self, address: u64) -> Option<&FunctionRecord> {
        let functions = &self.functions;
        self.function_intervals
            .find_best(address, |candidate, current| {
                let candidate = &functions[candidate];
                current.is_none_or(|current| {
                    let current = &functions[current];
                    (candidate.size, candidate.entry, candidate.id)
                        < (current.size, current.entry, current.id)
                })
            })
            .and_then(|index| functions.get(index))
            .or_else(|| self.function_at(address))
    }

    pub fn memory_block_containing(&self, address: u64) -> Option<&MemoryBlock> {
        let blocks = &self.memory_blocks;
        self.memory_block_intervals
            .find_best(address, |candidate, current| {
                current.is_none_or(|current| {
                    (blocks[candidate].start, blocks[candidate].id)
                        < (blocks[current].start, blocks[current].id)
                })
            })
            .and_then(|index| blocks.get(index))
    }

    pub fn symbols_at(&self, address: u64) -> impl Iterator<Item = &SymbolRecord> {
        let start = self
            .symbols
            .partition_point(|symbol| symbol.address < address);
        let count = self.symbols[start..].partition_point(|symbol| symbol.address == address);
        self.symbols[start..start + count].iter()
    }

    pub fn relocations_at(&self, address: u64) -> impl Iterator<Item = &RelocationRecord> {
        let start = self
            .relocations
            .partition_point(|relocation| relocation.address < address);
        let count =
            self.relocations[start..].partition_point(|relocation| relocation.address == address);
        self.relocations[start..start + count].iter()
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

pub(crate) fn block_contains_function(block: &MemoryBlock, entry: u64, size: u64) -> bool {
    let Some(end) = block.start.checked_add(block.size) else {
        return false;
    };
    if entry < block.start {
        return false;
    }
    if size == 0 {
        entry < end
    } else {
        entry
            .checked_add(size)
            .is_some_and(|func_end| func_end <= end)
    }
}

fn build_functions(
    functions: &[FunctionInfo],
    blocks: &[MemoryBlock],
    block_intervals: &AddressIntervalIndex,
) -> Vec<FunctionRecord> {
    let mut sorted = functions.to_vec();
    sorted.retain(|function| {
        // Import slots live in IAT/GOT data, so they only need to be mapped;
        // every non-import function is kept as the loader reported it.
        !function.is_import
            || blocks.is_empty()
            || block_intervals.any_containing(function.address, |_| true)
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
            let block = block_intervals
                .find_best(function.address, |candidate, current| {
                    block_contains_function(&blocks[candidate], function.address, function.size)
                        && current.is_none_or(|current| {
                            (blocks[candidate].start, blocks[candidate].id)
                                < (blocks[current].start, blocks[current].id)
                        })
                })
                .and_then(|index| blocks.get(index));
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
    type SymbolKey = (u64, SymbolKind, String);
    type SymbolFacts = (Option<u64>, Option<String>, FactSource);

    let mut rows: BTreeMap<SymbolKey, SymbolFacts> = BTreeMap::new();

    let lookup_version = |name: &str, addr: &u64| {
        binary
            .symbol_version_names
            .get(name)
            .or_else(|| {
                let bare = name.split('@').next().unwrap_or(name);
                binary.symbol_version_names.get(bare)
            })
            .or_else(|| binary.symbol_versions.get(addr))
            .cloned()
    };

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
                lookup_version(&function.name, &function.entry),
                function.provenance.source,
            ),
        );
    }
    for (address, name) in &binary.iat_symbols {
        rows.entry((*address, SymbolKind::Import, name.clone()))
            .and_modify(|facts| {
                if facts.1.is_none() {
                    facts.1 = lookup_version(name, address);
                }
            })
            .or_insert_with(|| (None, lookup_version(name, address), FactSource::ImportTable));
    }
    for (address, name) in &binary.global_symbols {
        rows.insert(
            (*address, SymbolKind::Data, name.clone()),
            (
                binary.global_symbol_sizes.get(address).copied(),
                lookup_version(name, address),
                FactSource::SymbolTable,
            ),
        );
    }
    for symbol in &binary.loader_symbols {
        let (kind, size) = match symbol.kind {
            LoaderSymbolKind::CodeLabel => (SymbolKind::Label, None),
            LoaderSymbolKind::Data => (
                SymbolKind::Data,
                binary.global_symbol_sizes.get(&symbol.address).copied(),
            ),
        };
        rows.insert(
            (symbol.address, kind, symbol.name.clone()),
            (
                size,
                lookup_version(&symbol.name, &symbol.address),
                FactSource::SymbolTable,
            ),
        );
    }
    let existing_addr_names: std::collections::HashSet<(u64, String)> = rows
        .keys()
        .map(|(addr, _, name)| (*addr, name.clone()))
        .collect();
    for (&address, name) in &binary.relocation_symbols {
        if existing_addr_names.contains(&(address, name.clone())) {
            continue;
        }
        rows.entry((address, SymbolKind::RelocationTarget, name.clone()))
            .or_insert_with(|| {
                (
                    binary.global_symbol_sizes.get(&address).copied(),
                    lookup_version(name, &address),
                    FactSource::RelocationTable,
                )
            });
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
    let mut symbol_ids_at_address: HashMap<(u64, &str), SymbolId> =
        HashMap::with_capacity(symbols.len());
    for symbol in symbols {
        symbol_ids_at_address
            .entry((symbol.address, symbol.name.as_str()))
            .or_insert(symbol.id);
    }
    let mut first_symbol_id_by_name = HashMap::with_capacity(symbols.len());
    for symbol in symbols {
        first_symbol_id_by_name
            .entry(symbol.name.as_str())
            .or_insert(symbol.id);
    }
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
                // The symbol-table index the relocation carried, resolved to an
                // address by the loader, says which of several same-named
                // symbols it means. Without one, the first of that name.
                relocation
                    .symbol_address
                    .and_then(|address| symbol_ids_at_address.get(&(address, name)).copied())
                    .or_else(|| {
                        symbol_ids_at_address
                            .get(&(relocation.address, name))
                            .copied()
                    })
                    .or_else(|| first_symbol_id_by_name.get(name).copied())
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
    use fission_loader::{LoaderSymbolInfo, LoaderSymbolKind, SectionInfo};

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
                symbol_address: None,
            }])
            .build()
            .expect("fixture should build")
    }

    #[test]
    fn snapshot_ids_are_deterministic() {
        let left = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("left valid");
        let right = ProgramSnapshot::try_from_loaded_binary(&fixture(true)).expect("right valid");
        assert_eq!(
            serde_json::to_string(&left).unwrap(),
            serde_json::to_string(&right).unwrap()
        );
        assert_eq!(left.function_at(0x1000).unwrap().id, FunctionId(0));
        assert_eq!(
            left.memory_block_containing(0x1010).unwrap().id,
            MemoryBlockId(0)
        );
        assert_eq!(left.memory_blocks()[0].size, 0x120);
        assert_eq!(left.memory_blocks()[0].virtual_size, 0x100);
    }

    #[test]
    fn snapshot_preserves_provenance_and_links() {
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
        let first = snapshot.function_at(0x1000).unwrap();
        assert_eq!(first.provenance.source, FactSource::ExportTable);
        assert_eq!(first.provenance.confidence, FactConfidence::Proven);
        assert_eq!(first.memory_block, Some(MemoryBlockId(0)));
        assert_eq!(snapshot.relocations().len(), 1);
        assert!(snapshot.relocations()[0].symbol.is_some());
    }

    /// Two symbols named `dup` (static functions in different files); the
    /// relocation's own symbol address says which one it means.
    #[test]
    fn a_relocation_binds_to_the_same_named_symbol_it_points_at() {
        let binary = LoadedBinaryBuilder::new("fixture.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .image_base(0x1000)
            .entry_point(0x1000)
            .is_64bit(true)
            .add_global_symbol(0x1000, "dup".into())
            .add_global_symbol(0x1010, "dup".into())
            .add_relocations([RelocationEntry {
                address: 0x2000,
                r_type: 1,
                size: 8,
                addend: 0,
                symbol_name: Some("dup".into()),
                symbol_address: Some(0x1010),
            }])
            .build()
            .expect("fixture should build");
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        let bound = snapshot.relocations()[0].symbol.expect("bound to a symbol");
        let symbol = snapshot
            .symbols()
            .iter()
            .find(|symbol| symbol.id == bound)
            .expect("the bound symbol exists");
        assert_eq!(symbol.address, 0x1010);
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
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
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

    /// A zero-sized label at a function's entry used to win the query *at the
    /// entry only*, so `entry` and `entry + 4` of one function disagreed.
    #[test]
    fn a_zero_sized_alias_does_not_shadow_the_function_it_starts() {
        let function = |name: &str, address: u64, size: u64| FunctionInfo {
            name: name.into(),
            address,
            size,
            ..FunctionInfo::default()
        };
        let binary = LoadedBinaryBuilder::new("alias.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .image_base(0x1000)
            .is_64bit(true)
            // Sorts ahead of `real_body` by name, which is what `function_at` returns.
            .add_functions([
                function("_start", 0x1000, 0),
                function("real_body", 0x1000, 256),
            ])
            .add_sections([SectionInfo {
                name: ".text".into(),
                virtual_address: 0x1000,
                virtual_size: 0x100,
                file_offset: 0,
                file_size: 0x100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            }])
            .build()
            .expect("fixture should build");
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        for address in [0x1000u64, 0x1004, 0x10ff] {
            assert_eq!(
                snapshot
                    .function_containing(address)
                    .map(|row| row.name.as_str()),
                Some("real_body"),
                "0x{address:x}"
            );
        }
        // Nothing sized covers it, so the label is the only answer there is.
        let lone = LoadedBinaryBuilder::new("lone.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .image_base(0x1000)
            .is_64bit(true)
            .add_functions([function("_label", 0x2000, 0)])
            .build()
            .expect("fixture should build");
        assert_eq!(
            ProgramSnapshot::try_from_loaded_binary(&lone)
                .expect("valid")
                .function_containing(0x2000)
                .map(|row| row.name.as_str()),
            Some("_label")
        );
    }

    #[test]
    fn address_interval_index_preserves_specificity_and_candidate_filtering() {
        let ranges = [(0x1000, 0x1100), (0x1020, 0x1030), (0x2000, 0x2010)];
        let index = AddressIntervalIndex::from_items(&ranges, |range| Some(*range));

        let best = index.find_best(0x1028, |candidate, current| {
            current.is_none_or(|current| {
                (ranges[candidate].1 - ranges[candidate].0)
                    < (ranges[current].1 - ranges[current].0)
            })
        });
        assert_eq!(best, Some(1));
        assert_eq!(
            index.find_best(0x1050, |candidate, _| candidate == 0),
            Some(0)
        );
        assert!(index.any_containing(0x1028, |candidate| candidate == 1));
        assert!(!index.any_containing(0x1800, |_| true));
    }

    #[test]
    fn snapshot_integrity_reports_corrupt_ids_links_and_ranges() {
        let mut snapshot = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
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
    fn non_executable_import_slots_populate_import_functions_and_symbols() {
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
                origin: Some("pe-import-table".into()),
                kind: Some("import".into()),
                external_library: Some("example.dll".into()),
                ..FunctionInfo::default()
            })
            .add_iat_symbol(0x3010, "example.dll!imported".into())
            .build()
            .expect("fixture should build");

        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        let imported = snapshot
            .function_at(0x3010)
            .expect("IAT import function should be retained in snapshot");
        assert_eq!(imported.kind, FunctionKind::Import);
        assert!(imported.is_import);
        assert_eq!(imported.external_library.as_deref(), Some("example.dll"));
        assert_eq!(imported.provenance.source, FactSource::ImportTable);
        assert!(
            snapshot
                .symbols()
                .iter()
                .any(|symbol| { symbol.address == 0x3010 && symbol.kind == SymbolKind::Import })
        );
    }

    #[test]
    fn typed_loader_code_labels_remain_labels_in_the_snapshot() {
        let binary = LoadedBinaryBuilder::new("labels.exe".into(), DataBuffer::Heap(vec![0; 16]))
            .format("PE")
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".text".into(),
                virtual_address: 0x1000,
                virtual_size: 0x100,
                file_offset: 0,
                file_size: 0x100,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .add_loader_symbols([LoaderSymbolInfo {
                address: 0x1020,
                name: ".l_start".into(),
                kind: LoaderSymbolKind::CodeLabel,
                origin: "pe-coff-symbol-table".into(),
            }])
            .add_global_symbol_sizes([(0x1020, 64)].into())
            .build()
            .expect("fixture should build");

        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        let label = snapshot
            .symbols()
            .iter()
            .find(|symbol| symbol.address == 0x1020)
            .expect("loader label should reach the canonical symbol table");
        assert_eq!(label.name, ".l_start");
        assert_eq!(label.kind, SymbolKind::Label);
        assert_eq!(label.size, None);
        assert_eq!(label.provenance.source, FactSource::SymbolTable);
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

        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        let headers = &snapshot.memory_blocks()[0];
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
        assert_eq!(snapshot.memory_blocks()[1].name, ".text");
    }

    #[test]
    fn imported_and_aliased_symbols_retain_correct_version_strings() {
        let mut ver_names = std::collections::HashMap::new();
        ver_names.insert("puts".to_string(), "GLIBC_2.2.5".to_string());
        ver_names.insert("puts@GLIBC_2.2.5".to_string(), "GLIBC_2.2.5".to_string());
        ver_names.insert("alias_v1".to_string(), "VER_1.0".to_string());
        ver_names.insert("alias_v2".to_string(), "VER_2.0".to_string());

        let binary =
            LoadedBinaryBuilder::new("elf_version_test.bin".into(), DataBuffer::Heap(vec![0; 16]))
                .format("ELF")
                .image_base(0x400000)
                .is_64bit(true)
                .add_iat_symbol(0x404018, "puts".into())
                .add_function(FunctionInfo {
                    name: "alias_v1".into(),
                    address: 0x401000,
                    size: 32,
                    is_export: true,
                    ..FunctionInfo::default()
                })
                .add_function(FunctionInfo {
                    name: "alias_v2".into(),
                    address: 0x401000,
                    size: 32,
                    is_export: true,
                    ..FunctionInfo::default()
                })
                .add_symbol_version_names(ver_names)
                .build()
                .expect("binary should build");

        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");

        // 1. IAT symbol (puts at GOT slot 0x404018) should have version GLIBC_2.2.5
        let puts_sym = snapshot
            .symbols()
            .iter()
            .find(|s| s.name == "puts")
            .expect("puts symbol must exist");
        assert_eq!(puts_sym.kind, SymbolKind::Import);
        assert_eq!(puts_sym.version.as_deref(), Some("GLIBC_2.2.5"));

        // 2. Aliased functions sharing address 0x401000 should retain distinct versions
        let alias1 = snapshot
            .symbols()
            .iter()
            .find(|s| s.name == "alias_v1")
            .expect("alias_v1 must exist");
        assert_eq!(alias1.address, 0x401000);
        assert_eq!(alias1.version.as_deref(), Some("VER_1.0"));

        let alias2 = snapshot
            .symbols()
            .iter()
            .find(|s| s.name == "alias_v2")
            .expect("alias_v2 must exist");
        assert_eq!(alias2.address, 0x401000);
        assert_eq!(alias2.version.as_deref(), Some("VER_2.0"));
    }

    #[test]
    fn snapshot_includes_relocation_symbols_and_links_relocations() {
        let binary = LoadedBinaryBuilder::new("relocs.bin".into(), DataBuffer::Heap(vec![0; 16]))
            .format("ELF")
            .image_base(0x1000)
            .is_64bit(true)
            .add_section(SectionInfo {
                name: ".got".into(),
                virtual_address: 0x2000,
                virtual_size: 0x100,
                file_offset: 0,
                file_size: 0x100,
                is_executable: false,
                is_readable: true,
                is_writable: true,
            })
            .add_relocation_symbols([(0x2010, "external_printf".into())].into())
            .add_relocations([RelocationEntry {
                address: 0x2010,
                r_type: 7,
                size: 8,
                addend: 0,
                symbol_name: Some("external_printf".into()),
                symbol_address: None,
            }])
            .build()
            .expect("fixture should build");

        let snapshot = ProgramSnapshot::try_from_loaded_binary(&binary).expect("valid");
        let sym = snapshot
            .symbols()
            .iter()
            .find(|s| s.name == "external_printf")
            .expect("relocation target symbol must be in snapshot.symbols");
        assert_eq!(sym.address, 0x2010);
        assert_eq!(sym.kind, SymbolKind::RelocationTarget);
        assert_eq!(sym.provenance.source, FactSource::RelocationTable);

        let reloc = &snapshot.relocations()[0];
        assert_eq!(reloc.symbol, Some(sym.id));
        assert_eq!(reloc.symbol_name.as_deref(), Some("external_printf"));
    }

    #[test]
    fn snapshot_integrity_detects_function_overflowing_memory_block() {
        let mut builder = ProgramSnapshot::try_from_loaded_binary(&fixture(false))
            .expect("valid")
            .to_builder();
        let block_end = {
            let block = &builder.memory_blocks[0];
            block.start + block.size
        };
        builder.functions_mut()[0].entry = block_end - 10;
        builder.functions_mut()[0].size = 50;

        let Err(report) = builder.build() else {
            panic!("overflowing function must be rejected at publication");
        };
        assert!(!report.is_valid());
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
        )));
    }

    #[test]
    fn build_functions_and_symbols_preserve_block_containment_import_size_and_primary_symbol_id() {
        let binary = LoadedBinaryBuilder::new("pe_check.exe".into(), DataBuffer::Heap(vec![0; 64]))
            .format("PE")
            .image_base(0x140000000)
            .is_64bit(true)
            .add_sections([
                SectionInfo {
                    name: ".text".into(),
                    virtual_address: 0x140001000,
                    virtual_size: 0x100,
                    file_offset: 0x200,
                    file_size: 0x100,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                },
                SectionInfo {
                    name: ".idata".into(),
                    virtual_address: 0x140002000,
                    virtual_size: 0x100,
                    file_offset: 0x300,
                    file_size: 0x100,
                    is_executable: false,
                    is_readable: true,
                    is_writable: true,
                },
            ])
            .add_functions([
                FunctionInfo {
                    name: "overflow_fn".into(),
                    address: 0x1400010f0,
                    size: 0x40,
                    origin: Some("static_discovery".into()),
                    ..FunctionInfo::default()
                },
                FunctionInfo {
                    name: "kernel32.dll!CreateFileW".into(),
                    address: 0x140002010,
                    size: 8,
                    is_import: true,
                    origin: Some("pe-import-table".into()),
                    ..FunctionInfo::default()
                },
            ])
            .add_iat_symbol(0x140002010, "kernel32.dll!CreateFileW".into())
            .add_relocation_symbols([(0x140002010, "kernel32.dll!CreateFileW".into())].into())
            .add_relocations([RelocationEntry {
                address: 0x140001020,
                r_type: 1,
                size: 8,
                addend: 0,
                symbol_name: Some("kernel32.dll!CreateFileW".into()),
                symbol_address: Some(0x140002010),
            }])
            .build()
            .expect("fixture should build");

        let snapshot =
            ProgramSnapshot::try_from_loaded_binary(&binary).expect("snapshot integrity passes");
        let overflow_fn = snapshot.function_at(0x1400010f0).expect("overflow fn");
        assert_eq!(overflow_fn.memory_block, None);

        let import_sym = snapshot
            .symbols()
            .iter()
            .find(|s| s.address == 0x140002010 && s.kind == SymbolKind::Import)
            .expect("import symbol");
        assert_eq!(import_sym.size, Some(8));
        assert!(
            !snapshot
                .symbols()
                .iter()
                .any(|s| s.address == 0x140002010 && s.kind == SymbolKind::RelocationTarget),
            "existing import symbol should not gain duplicate RelocationTarget"
        );
        assert_eq!(snapshot.relocations()[0].symbol, Some(import_sym.id));
    }

    fn reference_function_containing<'a>(
        snapshot: &'a ProgramSnapshot,
        address: u64,
    ) -> Option<&'a FunctionRecord> {
        let sized = snapshot
            .functions()
            .iter()
            .filter(|f| f.size > 0 && f.entry <= address && address < f.entry + f.size)
            .min_by_key(|f| (f.size, f.entry, f.id));
        sized.or_else(|| snapshot.functions().iter().find(|f| f.entry == address))
    }

    fn reference_memory_block_containing<'a>(
        snapshot: &'a ProgramSnapshot,
        address: u64,
    ) -> Option<&'a MemoryBlock> {
        snapshot
            .memory_blocks()
            .iter()
            .filter(|b| b.start <= address && address < b.start + b.size)
            .min_by_key(|b| (b.start, b.id))
    }

    fn assert_queries_match_reference_scan(snapshot: &ProgramSnapshot, probe_addrs: &[u64]) {
        assert!(
            snapshot.integrity_report().is_valid(),
            "snapshot must satisfy integrity contract: {:?}",
            snapshot.integrity_report().issues
        );
        for &addr in probe_addrs {
            assert_eq!(
                snapshot.function_at(addr),
                snapshot.functions().iter().find(|f| f.entry == addr),
                "function_at mismatch at 0x{addr:x}"
            );
            assert_eq!(
                snapshot.functions_at(addr).collect::<Vec<_>>(),
                snapshot
                    .functions()
                    .iter()
                    .filter(|f| f.entry == addr)
                    .collect::<Vec<_>>(),
                "functions_at mismatch at 0x{addr:x}"
            );
            assert_eq!(
                snapshot.function_containing(addr),
                reference_function_containing(snapshot, addr),
                "function_containing mismatch at 0x{addr:x}"
            );
            assert_eq!(
                snapshot.memory_block_containing(addr),
                reference_memory_block_containing(snapshot, addr),
                "memory_block_containing mismatch at 0x{addr:x}"
            );
            assert_eq!(
                snapshot.symbols_at(addr).collect::<Vec<_>>(),
                snapshot
                    .symbols()
                    .iter()
                    .filter(|s| s.address == addr)
                    .collect::<Vec<_>>(),
                "symbols_at mismatch at 0x{addr:x}"
            );
            assert_eq!(
                snapshot.relocations_at(addr).collect::<Vec<_>>(),
                snapshot
                    .relocations()
                    .iter()
                    .filter(|r| r.address == addr)
                    .collect::<Vec<_>>(),
                "relocations_at mismatch at 0x{addr:x}"
            );
        }
    }

    #[test]
    fn stale_derived_index_is_detected_by_integrity_report() {
        let mut fn_corrupt =
            ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
        // Keep canonical order intact (`first` stays ahead of `second` at 0x1010)
        // while breaking correspondence with the cached `function_intervals` index.
        fn_corrupt.functions[0].entry = 0x1004;
        let report = fn_corrupt.integrity_report();
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DerivedIndexMismatch {
                table: SnapshotTable::Functions
            }
        )));

        let mut block_corrupt =
            ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
        block_corrupt.memory_blocks[0].size = 0x200;
        let report = block_corrupt.integrity_report();
        assert!(report.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DerivedIndexMismatch {
                table: SnapshotTable::MemoryBlocks
            }
        )));
    }

    #[test]
    fn typed_deltas_and_builder_mutations_stay_query_consistent_with_reference_scan() {
        let probes: Vec<u64> = (0x0ff0..=0x2100).step_by(4).collect();
        let snapshot = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");
        assert_queries_match_reference_scan(&snapshot, &probes);

        // 1. Update function range and move entry past `second` (triggers re-sort + ID remap).
        let updated = snapshot
            .apply_delta(ProgramSnapshotDelta::UpdateFunctionRange {
                id: FunctionId(0),
                entry: 0x1020,
                size: 16,
            })
            .expect("range update should republish validly");
        assert_eq!(updated.functions()[0].name, "second");
        assert_eq!(updated.functions()[0].id, FunctionId(0));
        assert_eq!(updated.functions()[1].name, "first");
        assert_eq!(updated.functions()[1].id, FunctionId(1));
        assert_eq!(updated.function_containing(0x1004), None);
        assert_eq!(
            updated.function_containing(0x1024).map(|f| f.name.as_str()),
            Some("first")
        );
        assert_queries_match_reference_scan(&updated, &probes);

        // 2. Append a new block, function, symbol, and relocation via typed deltas.
        let with_append = updated
            .apply_deltas([
                ProgramSnapshotDelta::AddMemoryBlock(MemoryBlock {
                    id: MemoryBlockId(99),
                    name: ".extra".into(),
                    start: 0x2000,
                    size: 0x100,
                    virtual_size: 0x100,
                    file_offset: 0x400,
                    file_size: 0x100,
                    permissions: Permissions {
                        read: true,
                        write: false,
                        execute: true,
                    },
                    provenance: Provenance {
                        source: FactSource::StaticDiscovery,
                        confidence: FactConfidence::Medium,
                        detail: None,
                    },
                }),
                ProgramSnapshotDelta::AddFunction(FunctionRecord {
                    id: FunctionId(99),
                    name: "appended_fn".into(),
                    entry: 0x2010,
                    size: 32,
                    kind: FunctionKind::Discovered,
                    is_import: false,
                    is_export: false,
                    is_thunk: false,
                    thunk_target: None,
                    memory_block: None,
                    external_library: None,
                    provenance: Provenance {
                        source: FactSource::StaticDiscovery,
                        confidence: FactConfidence::Medium,
                        detail: None,
                    },
                }),
                ProgramSnapshotDelta::AddSymbol(SymbolRecord {
                    id: SymbolId(99),
                    name: "appended_sym".into(),
                    address: 0x2010,
                    kind: SymbolKind::Function,
                    size: Some(32),
                    version: None,
                    provenance: Provenance {
                        source: FactSource::StaticDiscovery,
                        confidence: FactConfidence::Medium,
                        detail: None,
                    },
                }),
                ProgramSnapshotDelta::AddRelocation(RelocationRecord {
                    id: RelocationId(99),
                    address: 0x2014,
                    relocation_type: 1,
                    size: 8,
                    addend: 0,
                    symbol: None,
                    symbol_name: Some("appended_sym".into()),
                    provenance: Provenance {
                        source: FactSource::RelocationTable,
                        confidence: FactConfidence::Proven,
                        detail: None,
                    },
                }),
            ])
            .expect("appends should publish validly");
        assert_queries_match_reference_scan(&with_append, &probes);

        // 3. Reorder rows manually in builder, canonicalize, and publish.
        let mut reordered_builder = with_append.to_builder();
        reordered_builder.functions_mut().reverse();
        reordered_builder.symbols_mut().reverse();
        reordered_builder.relocations_mut().reverse();
        let recanonical = reordered_builder.canonicalize().build().expect("valid");
        assert_queries_match_reference_scan(&recanonical, &probes);

        // 4. Delete rows via typed deltas and verify queries reflect deletions.
        let deleted = recanonical
            .apply_deltas([
                ProgramSnapshotDelta::RemoveRelocation(RelocationId(1)),
                ProgramSnapshotDelta::RemoveFunction(FunctionId(1)),
            ])
            .expect("deletions should publish validly");
        assert_eq!(deleted.function_containing(0x1024), None);
        assert_eq!(deleted.relocations_at(0x2014).count(), 0);
        assert_queries_match_reference_scan(&deleted, &probes);
    }

    #[test]
    fn invalid_publication_is_rejected_and_surfaces_explicit_partial_status() {
        let valid = ProgramSnapshot::try_from_loaded_binary(&fixture(false)).expect("valid");

        // Unsorted functions without canonicalize fail publication cleanly and expose Partial.
        let mut unsorted = valid.to_builder();
        unsorted.functions_mut().reverse();
        let pub_outcome = unsorted.clone().publish();
        assert!(!pub_outcome.is_valid());
        let SnapshotPublication::Partial(partial) = pub_outcome else {
            panic!("unsorted snapshot must surface Partial publication");
        };
        assert_eq!(partial.functions().len(), 2);
        assert!(
            partial
                .integrity_report()
                .issues
                .iter()
                .any(|issue| matches!(
                    issue,
                    SnapshotIntegrityIssue::NonCanonicalOrder {
                        table: SnapshotTable::Functions,
                        ..
                    }
                ))
        );
        // Partial snapshot can still be repaired cleanly through canonicalize().
        assert!(partial.to_builder().canonicalize().build().is_ok());

        // Removing a referenced block or symbol without updating dependents fails with DanglingReference.
        let err = valid
            .apply_delta(ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(0)))
            .expect_err("removing referenced block must fail validation");
        assert!(err.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DanglingReference {
                table: SnapshotTable::Functions,
                target_table: SnapshotTable::MemoryBlocks,
                ..
            }
        )));

        let err = valid
            .apply_delta(ProgramSnapshotDelta::RemoveSymbol(
                valid.relocations()[0].symbol.expect("linked symbol"),
            ))
            .expect_err("removing referenced symbol must fail validation");
        assert!(err.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::DanglingReference {
                table: SnapshotTable::Relocations,
                target_table: SnapshotTable::Symbols,
                ..
            }
        )));

        // Shrinking a memory block past a function that points to it fails with FunctionOutsideMemoryBlock.
        let err = valid
            .apply_delta(ProgramSnapshotDelta::UpdateMemoryBlockRange {
                id: MemoryBlockId(0),
                start: 0x1000,
                size: 8,
            })
            .expect_err("shrinking block past functions must fail validation");
        assert!(err.issues.iter().any(|issue| matches!(
            issue,
            SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
        )));

        // Schema mismatch is also rejected.
        let err = valid
            .to_builder()
            .with_schema("bad-schema")
            .build()
            .expect_err("wrong schema must fail");
        assert!(
            err.issues
                .iter()
                .any(|issue| matches!(issue, SnapshotIntegrityIssue::SchemaMismatch { .. }))
        );
    }

    #[test]
    fn serialization_omits_ephemeral_indexes_and_deserialization_rebuilds_and_validates() {
        let mut ver_builder = ProgramSnapshot::try_from_loaded_binary(&fixture(false))
            .expect("valid")
            .to_builder();
        ver_builder.apply_delta(ProgramSnapshotDelta::AddFunction(FunctionRecord {
            id: FunctionId(0),
            name: "_alias".into(),
            entry: 0x1000,
            size: 0,
            kind: FunctionKind::Code,
            is_import: false,
            is_export: false,
            is_thunk: false,
            thunk_target: None,
            memory_block: Some(MemoryBlockId(0)),
            external_library: None,
            provenance: Provenance {
                source: FactSource::SymbolTable,
                confidence: FactConfidence::High,
                detail: None,
            },
        }));
        let original = ver_builder.canonicalize().build().expect("valid");
        let mut value = serde_json::to_value(&original).expect("serialize snapshot");
        let obj = value.as_object().expect("JSON object");
        assert_eq!(obj.len(), 6);
        for key in [
            "schema",
            "binary",
            "memory_blocks",
            "functions",
            "symbols",
            "relocations",
        ] {
            assert!(obj.contains_key(key), "missing key {key}");
        }

        let roundtrip: ProgramSnapshot =
            serde_json::from_value(value.clone()).expect("valid JSON should rebuild indexes");
        assert_eq!(roundtrip, original);
        let probes: Vec<u64> = (0x0ff0..=0x1140).step_by(4).collect();
        assert_queries_match_reference_scan(&roundtrip, &probes);

        // Tampering serialized JSON rows out of order must be rejected on deserialization.
        let funcs = value
            .get_mut("functions")
            .and_then(|v| v.as_array_mut())
            .expect("functions array");
        funcs.swap(0, 2);
        assert!(serde_json::from_value::<ProgramSnapshot>(value.clone()).is_err());

        // Tampering schema in JSON must be rejected.
        value["schema"] = serde_json::json!("evil-schema");
        assert!(serde_json::from_value::<ProgramSnapshot>(value).is_err());
    }
}
