//! Typed, immutable program metadata shared by Fission analysis layers.
//!
//! `fission-loader` owns format parsing. This crate converts those raw facts
//! into a deterministic snapshot with canonical table IDs and explicit
//! provenance. It intentionally contains no binary parser or NIR pass.
//!
//! # Row identity
//!
//! A row ID (`FunctionId`, `MemoryBlockId`, `SymbolId`, `RelocationId`) in a
//! published [`ProgramSnapshot`] is that row's canonical table index, and it is
//! scoped to that one snapshot: publishing a replacement renumbers rows, so an
//! ID must not be carried from one snapshot into another. Facts that need to
//! survive republication are keyed by the row's natural key (entry, address,
//! name), not by its ID.
//!
//! Inside a [`ProgramSnapshotBuilder`] rows carry *staging* IDs. A builder
//! never issues the same staging ID twice, not even after the row holding it
//! is removed, so a foreign key left pointing at a removed row stays dangling
//! and is reported at publication instead of binding to a row added later.
//! [`ProgramSnapshotBuilder::canonicalize`] ends the staging scope: it maps
//! every staging ID to its canonical index and turns references that name no
//! unique staged row into dangling references.

use fission_loader::{FunctionInfo, LoadedBinary, LoaderSymbolKind};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::sync::Arc;

mod integrity;
mod quarantine;
pub use integrity::{
    FactInventory, SnapshotIntegrityIssue, SnapshotIntegrityReport, SnapshotTable,
};
pub use quarantine::{QuarantinedPublication, RejectedRow, RowRejection};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum IdentityField<T> {
    Available(T),
    Unavailable,
}

impl<T> IdentityField<T> {
    pub fn as_option(&self) -> Option<&T> {
        match self {
            Self::Available(value) => Some(value),
            Self::Unavailable => None,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available(_))
    }
}

fn non_empty_identity(value: &str) -> IdentityField<String> {
    if value.trim().is_empty() {
        IdentityField::Unavailable
    } else {
        IdentityField::Available(value.to_string())
    }
}

fn opt_non_empty_identity(value: Option<&str>) -> IdentityField<String> {
    value
        .and_then(|s| (!s.trim().is_empty()).then(|| s.to_string()))
        .map_or(IdentityField::Unavailable, IdentityField::Available)
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv_write_bytes(hash: &mut u64, bytes: &[u8]) {
    for &byte in bytes {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

fn fnv_write_u64(hash: &mut u64, value: u64) {
    fnv_write_bytes(hash, &value.to_le_bytes());
}

fn fnv_write_str(hash: &mut u64, value: &str) {
    fnv_write_u64(hash, value.len() as u64);
    fnv_write_bytes(hash, value.as_bytes());
}

fn fnv_write_opt_str(hash: &mut u64, value: Option<&str>) {
    match value {
        Some(text) => {
            fnv_write_bytes(hash, &[1]);
            fnv_write_str(hash, text);
        }
        None => fnv_write_bytes(hash, &[0]),
    }
}

fn load_mapping_digest_for_blocks(blocks: &[MemoryBlock]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    fnv_write_u64(&mut hash, blocks.len() as u64);
    for block in blocks {
        fnv_write_u64(&mut hash, block.start);
        fnv_write_u64(&mut hash, block.size);
        fnv_write_u64(&mut hash, block.virtual_size);
        fnv_write_u64(&mut hash, block.file_offset);
        fnv_write_u64(&mut hash, block.file_size);
        let perm_bits = u8::from(block.permissions.read)
            | (u8::from(block.permissions.write) << 1)
            | (u8::from(block.permissions.execute) << 2);
        fnv_write_bytes(&mut hash, &[perm_bits]);
    }
    hash
}

fn project_decision_state_from_binary(binary: &LoadedBinary) -> (u64, bool) {
    let mut hash = FNV_OFFSET_BASIS;
    let mut has_decisions = !binary.user_signatures.is_empty()
        || !binary.user_comments.is_empty()
        || !binary.user_breakpoints.is_empty()
        || !binary.user_watchpoints.is_empty();

    let mut sig_addrs: Vec<u64> = binary.user_signatures.keys().copied().collect();
    sig_addrs.sort_unstable();
    fnv_write_u64(&mut hash, sig_addrs.len() as u64);
    for addr in sig_addrs {
        let sig = &binary.user_signatures[&addr];
        fnv_write_u64(&mut hash, addr);
        fnv_write_opt_str(&mut hash, sig.return_type.as_deref());
        fnv_write_u64(&mut hash, sig.param_types.len() as u64);
        for ty in &sig.param_types {
            fnv_write_str(&mut hash, ty);
        }
        fnv_write_u64(&mut hash, sig.param_names.len() as u64);
        for name in &sig.param_names {
            fnv_write_str(&mut hash, name);
        }
    }

    let mut comment_addrs: Vec<u64> = binary.user_comments.keys().copied().collect();
    comment_addrs.sort_unstable();
    fnv_write_u64(&mut hash, comment_addrs.len() as u64);
    for addr in comment_addrs {
        fnv_write_u64(&mut hash, addr);
        fnv_write_str(&mut hash, &binary.user_comments[&addr]);
    }

    let mut breakpoints = binary.user_breakpoints.clone();
    breakpoints.sort_unstable();
    fnv_write_u64(&mut hash, breakpoints.len() as u64);
    for bp in breakpoints {
        fnv_write_u64(&mut hash, bp);
    }

    let mut watchpoints = binary.user_watchpoints.clone();
    watchpoints.sort_by_key(|wp| (wp.address, wp.size, wp.on_read, wp.on_write));
    fnv_write_u64(&mut hash, watchpoints.len() as u64);
    for wp in watchpoints {
        fnv_write_u64(&mut hash, wp.address);
        fnv_write_u64(&mut hash, wp.size);
        fnv_write_bytes(&mut hash, &[u8::from(wp.on_read), u8::from(wp.on_write)]);
    }

    let sidecar_path = std::path::Path::new(&binary.path).with_extension("fission.json");
    match std::fs::read(&sidecar_path) {
        Ok(bytes) => {
            has_decisions = true;
            fnv_write_bytes(&mut hash, &[1]);
            fnv_write_u64(&mut hash, bytes.len() as u64);
            fnv_write_bytes(&mut hash, &bytes);
        }
        Err(_) => {
            fnv_write_bytes(&mut hash, &[0]);
        }
    }

    (hash, has_decisions)
}

fn snapshot_analysis_revision(
    schema: &str,
    binary: &BinaryMetadata,
    memory_blocks: &[MemoryBlock],
    functions: &[FunctionRecord],
    symbols: &[SymbolRecord],
    relocations: &[RelocationRecord],
) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    fnv_write_str(&mut hash, schema);
    fnv_write_str(&mut hash, &binary.content_hash);
    fnv_write_str(&mut hash, &binary.format);
    fnv_write_bytes(&mut hash, &[binary.bitness]);
    fnv_write_u64(&mut hash, binary.image_base);
    fnv_write_u64(&mut hash, binary.entry_point);
    fnv_write_opt_str(&mut hash, binary.processor.as_deref());
    fnv_write_opt_str(&mut hash, binary.endian.as_deref());
    fnv_write_opt_str(&mut hash, binary.abi.as_deref());
    fnv_write_opt_str(&mut hash, binary.language_id.as_deref());
    fnv_write_opt_str(&mut hash, binary.compiler_spec_id.as_deref());
    fnv_write_u64(&mut hash, load_mapping_digest_for_blocks(memory_blocks));
    fnv_write_u64(&mut hash, functions.len() as u64);
    for func in functions {
        fnv_write_u64(&mut hash, func.entry);
        fnv_write_u64(&mut hash, func.size);
        fnv_write_str(&mut hash, &func.name);
        fnv_write_u64(
            &mut hash,
            u64::from(func.is_import) | (u64::from(func.is_export) << 1),
        );
    }
    fnv_write_u64(&mut hash, symbols.len() as u64);
    for sym in symbols {
        fnv_write_u64(&mut hash, sym.address);
        fnv_write_str(&mut hash, &sym.name);
    }
    fnv_write_u64(&mut hash, relocations.len() as u64);
    for reloc in relocations {
        fnv_write_u64(&mut hash, reloc.address);
        fnv_write_opt_str(&mut hash, reloc.symbol_name.as_deref());
    }
    hash
}

/// Explicit identity contract binding binary content, effective load mapping
/// and spec, analysis revision, and project decision revision, distinguishing
/// available fields from unavailable ones without comparing host paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramViewIdentity {
    pub content_hash: IdentityField<String>,
    pub format: IdentityField<String>,
    pub bitness: IdentityField<u8>,
    pub image_base: u64,
    pub entry_point: u64,
    pub load_mapping_digest: IdentityField<u64>,
    pub processor: IdentityField<String>,
    pub endian: IdentityField<String>,
    pub abi: IdentityField<String>,
    pub language_id: IdentityField<String>,
    pub compiler_spec_id: IdentityField<String>,
    pub analysis_revision: IdentityField<u64>,
    pub project_decision_revision: IdentityField<u64>,
    pub has_project_decisions: bool,
}

impl ProgramViewIdentity {
    pub fn empty() -> Self {
        Self {
            content_hash: IdentityField::Unavailable,
            format: IdentityField::Unavailable,
            bitness: IdentityField::Unavailable,
            image_base: 0,
            entry_point: 0,
            load_mapping_digest: IdentityField::Unavailable,
            processor: IdentityField::Unavailable,
            endian: IdentityField::Unavailable,
            abi: IdentityField::Unavailable,
            language_id: IdentityField::Unavailable,
            compiler_spec_id: IdentityField::Unavailable,
            analysis_revision: IdentityField::Unavailable,
            project_decision_revision: IdentityField::Unavailable,
            has_project_decisions: false,
        }
    }

    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let blocks = build_memory_blocks(binary);
        let load_mapping_digest = load_mapping_digest_for_blocks(&blocks);
        let (decision_rev, has_decisions) = project_decision_state_from_binary(binary);
        let architecture = binary.architecture.as_ref();
        let load_spec = binary.load_spec.as_ref();
        Self {
            content_hash: non_empty_identity(&binary.hash),
            format: non_empty_identity(&binary.format),
            bitness: IdentityField::Available(if binary.is_64bit { 64 } else { 32 }),
            image_base: binary.image_base,
            entry_point: binary.entry_point,
            load_mapping_digest: IdentityField::Available(load_mapping_digest),
            processor: opt_non_empty_identity(architecture.map(|a| a.processor.as_str())),
            endian: opt_non_empty_identity(architecture.map(|a| a.endian.as_str())),
            abi: opt_non_empty_identity(architecture.and_then(|a| a.abi.as_deref())),
            language_id: opt_non_empty_identity(load_spec.map(|ls| ls.pair.language_id.as_str())),
            compiler_spec_id: opt_non_empty_identity(
                load_spec.map(|ls| ls.pair.compiler_spec_id.as_str()),
            ),
            analysis_revision: IdentityField::Unavailable,
            project_decision_revision: IdentityField::Available(decision_rev),
            has_project_decisions: has_decisions,
        }
    }

    fn from_snapshot_parts(
        schema: &str,
        binary: &BinaryMetadata,
        memory_blocks: &[MemoryBlock],
        functions: &[FunctionRecord],
        symbols: &[SymbolRecord],
        relocations: &[RelocationRecord],
        project_decision_revision: IdentityField<u64>,
        has_project_decisions: bool,
    ) -> Self {
        Self {
            content_hash: non_empty_identity(&binary.content_hash),
            format: non_empty_identity(&binary.format),
            bitness: if binary.bitness == 0 {
                IdentityField::Unavailable
            } else {
                IdentityField::Available(binary.bitness)
            },
            image_base: binary.image_base,
            entry_point: binary.entry_point,
            load_mapping_digest: IdentityField::Available(load_mapping_digest_for_blocks(
                memory_blocks,
            )),
            processor: opt_non_empty_identity(binary.processor.as_deref()),
            endian: opt_non_empty_identity(binary.endian.as_deref()),
            abi: opt_non_empty_identity(binary.abi.as_deref()),
            language_id: opt_non_empty_identity(binary.language_id.as_deref()),
            compiler_spec_id: opt_non_empty_identity(binary.compiler_spec_id.as_deref()),
            analysis_revision: IdentityField::Available(snapshot_analysis_revision(
                schema,
                binary,
                memory_blocks,
                functions,
                symbols,
                relocations,
            )),
            project_decision_revision,
            has_project_decisions,
        }
    }
}

/// One typed mismatch between a [`LoadedBinary`] and a [`ProgramSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramJoinMismatch {
    MissingRequiredIdentity {
        side: &'static str,
        field: &'static str,
    },
    ContentHashMismatch {
        binary: String,
        snapshot: String,
    },
    FormatMismatch {
        binary: String,
        snapshot: String,
    },
    BitnessMismatch {
        binary: u8,
        snapshot: u8,
    },
    ImageBaseMismatch {
        binary: u64,
        snapshot: u64,
    },
    EntryPointMismatch {
        binary: u64,
        snapshot: u64,
    },
    LoadMappingMismatch {
        binary: IdentityField<u64>,
        snapshot: IdentityField<u64>,
    },
    LoadSpecMismatch {
        field: &'static str,
        binary: IdentityField<String>,
        snapshot: IdentityField<String>,
    },
    ProjectDecisionRevisionMismatch {
        binary: IdentityField<u64>,
        snapshot: IdentityField<u64>,
    },
}

/// Failure when validating or joining a [`LoadedBinary`] with a [`ProgramSnapshot`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProgramJoinError {
    InvalidSnapshot(SnapshotIntegrityReport),
    IdentityMismatch(Vec<ProgramJoinMismatch>),
}

impl fmt::Display for ProgramJoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSnapshot(report) => {
                write!(f, "program join rejected invalid snapshot: {report}")
            }
            Self::IdentityMismatch(mismatches) => write!(
                f,
                "program join rejected mismatched binary/snapshot identity ({} mismatch(es)): {:?}",
                mismatches.len(),
                mismatches
            ),
        }
    }
}

impl std::error::Error for ProgramJoinError {}

/// Checked pairing of an immutable [`LoadedBinary`] with a validated canonical
/// [`ProgramSnapshot`] whose program identity, load mapping, spec, and project
/// decisions match. Validation runs once at construction so downstream reuse
/// via shared `Arc`s stays cheap.
#[derive(Debug, Clone)]
pub struct CheckedProgramView<'bin> {
    binary: &'bin LoadedBinary,
    program: Arc<ProgramSnapshot>,
}

impl<'bin> CheckedProgramView<'bin> {
    pub fn new(
        binary: &'bin LoadedBinary,
        program: Arc<ProgramSnapshot>,
    ) -> Result<Self, ProgramJoinError> {
        program.validate_for_loaded_binary(binary)?;
        Ok(Self { binary, program })
    }

    pub fn from_publication(
        binary: &'bin LoadedBinary,
        publication: SnapshotPublication,
    ) -> Result<Self, ProgramJoinError> {
        match publication {
            SnapshotPublication::Valid(snapshot) => Self::new(binary, Arc::new(snapshot)),
            SnapshotPublication::Partial(partial) => Err(ProgramJoinError::InvalidSnapshot(
                partial.integrity_report().clone(),
            )),
        }
    }

    pub fn binary(&self) -> &'bin LoadedBinary {
        self.binary
    }

    pub fn program(&self) -> &ProgramSnapshot {
        &self.program
    }

    pub fn program_arc(&self) -> Arc<ProgramSnapshot> {
        Arc::clone(&self.program)
    }

    pub fn identity(&self) -> &ProgramViewIdentity {
        self.program.identity()
    }
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

#[derive(Debug, Clone, Serialize)]
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
    #[serde(skip)]
    program_identity: Arc<ProgramViewIdentity>,
}

impl PartialEq for ProgramSnapshot {
    fn eq(&self, other: &Self) -> bool {
        self.schema == other.schema
            && self.binary == other.binary
            && self.memory_blocks == other.memory_blocks
            && self.functions == other.functions
            && self.symbols == other.symbols
            && self.relocations == other.relocations
            && self.function_intervals == other.function_intervals
            && self.memory_block_intervals == other.memory_block_intervals
    }
}

impl Eq for ProgramSnapshot {}

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
        ProgramSnapshotBuilder::from_raw_parts(
            self.schema.clone(),
            self.binary.clone(),
            self.memory_blocks.clone(),
            self.functions.clone(),
            self.symbols.clone(),
            self.relocations.clone(),
        )
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
///
/// IDs inside a delta are resolved in the scope it is applied to (a builder's
/// staging IDs, or the IDs of the snapshot `apply_delta` is called on). The
/// `id` field of a row passed to an `Add*` delta is ignored; the builder
/// assigns a fresh one.
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

/// Foreign-key value that [`ProgramSnapshotBuilder::canonicalize`] writes for
/// a reference that names no unique staged row. It is never issued as a
/// staging ID, so it can only ever be reported as dangling.
const UNRESOLVED_ID: u32 = u32::MAX;

/// Monotonic staging-ID source for one builder table. IDs are never reused:
/// removing a row does not move `next` back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IdAllocator {
    next: u32,
}

impl IdAllocator {
    /// First ID above every ID already present (exhausted when that would be
    /// [`UNRESOLVED_ID`]).
    fn above(ids: impl Iterator<Item = u32>) -> Self {
        Self {
            next: ids
                .max()
                .map_or(0, |max| max.saturating_add(1).min(UNRESOLVED_ID)),
        }
    }

    fn allocate(&mut self, table: SnapshotTable) -> Result<u32, SnapshotDeltaError> {
        if self.next == UNRESOLVED_ID {
            return Err(SnapshotDeltaError::IdSpaceExhausted { table });
        }
        let id = self.next;
        self.next += 1;
        Ok(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StagingIds {
    memory_blocks: IdAllocator,
    functions: IdAllocator,
    symbols: IdAllocator,
    relocations: IdAllocator,
}

impl StagingIds {
    fn above(
        memory_blocks: &[MemoryBlock],
        functions: &[FunctionRecord],
        symbols: &[SymbolRecord],
        relocations: &[RelocationRecord],
    ) -> Self {
        Self {
            memory_blocks: IdAllocator::above(memory_blocks.iter().map(|row| row.id.0)),
            functions: IdAllocator::above(functions.iter().map(|row| row.id.0)),
            symbols: IdAllocator::above(symbols.iter().map(|row| row.id.0)),
            relocations: IdAllocator::above(relocations.iter().map(|row| row.id.0)),
        }
    }
}

/// Why [`ProgramSnapshotBuilder::apply_delta`] refused a typed delta. A refused
/// delta leaves the builder unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotDeltaError {
    /// The delta names an ID no staged row carries: it was removed earlier,
    /// it was never issued, or it came from a different snapshot.
    UnknownTarget { table: SnapshotTable, id: u32 },
    /// Several staged rows carry the ID, so the delta cannot say which one.
    AmbiguousTarget {
        table: SnapshotTable,
        id: u32,
        rows: usize,
    },
    /// An added row references an ID that names no live staged row.
    DanglingReference {
        table: SnapshotTable,
        field: String,
        target_table: SnapshotTable,
        target_id: u32,
    },
    /// Every staging ID for the table has been issued.
    IdSpaceExhausted { table: SnapshotTable },
}

impl fmt::Display for SnapshotDeltaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTarget { table, id } => {
                write!(
                    f,
                    "delta names {table:?} id {id}, which no staged row carries"
                )
            }
            Self::AmbiguousTarget { table, id, rows } => {
                write!(
                    f,
                    "delta names {table:?} id {id}, which {rows} staged rows carry"
                )
            }
            Self::DanglingReference {
                table,
                field,
                target_table,
                target_id,
            } => write!(
                f,
                "added {table:?} row references {target_table:?} id {target_id} via `{field}`, \
                 which no live staged row carries"
            ),
            Self::IdSpaceExhausted { table } => {
                write!(f, "no unissued staging id is left for {table:?}")
            }
        }
    }
}

impl std::error::Error for SnapshotDeltaError {}

/// Failure of [`ProgramSnapshot::apply_deltas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotUpdateError {
    /// The delta at `index` in the batch was refused; nothing was published.
    RejectedDelta {
        index: usize,
        error: SnapshotDeltaError,
    },
    /// Every delta applied, but the resulting tables fail validation.
    Integrity(SnapshotIntegrityReport),
}

impl SnapshotUpdateError {
    /// The integrity report, when the batch applied but failed validation.
    pub fn integrity_report(&self) -> Option<&SnapshotIntegrityReport> {
        match self {
            Self::Integrity(report) => Some(report),
            Self::RejectedDelta { .. } => None,
        }
    }
}

impl fmt::Display for SnapshotUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RejectedDelta { index, error } => write!(f, "delta {index} rejected: {error}"),
            Self::Integrity(report) => report.fmt(f),
        }
    }
}

impl std::error::Error for SnapshotUpdateError {}

/// Staged builder used to construct or derive replacement snapshots prior to
/// validation at [`Self::publish`] / [`Self::build`].
///
/// Rows are changed only through typed deltas ([`Self::apply_delta`]), which
/// resolve IDs in the builder's staging scope (see the crate docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramSnapshotBuilder {
    schema: String,
    binary: BinaryMetadata,
    memory_blocks: Vec<MemoryBlock>,
    functions: Vec<FunctionRecord>,
    symbols: Vec<SymbolRecord>,
    relocations: Vec<RelocationRecord>,
    staging_ids: StagingIds,
    project_decision_revision: IdentityField<u64>,
    has_project_decisions: bool,
}

/// Index of the single staged row carrying `id`.
fn unique_row<T>(
    rows: &[T],
    table: SnapshotTable,
    id: u32,
    get_id: impl Fn(&T) -> u32,
) -> Result<usize, SnapshotDeltaError> {
    let mut found = None;
    let mut count = 0;
    for (index, row) in rows.iter().enumerate() {
        if get_id(row) == id {
            count += 1;
            found.get_or_insert(index);
        }
    }
    match (found, count) {
        (Some(index), 1) => Ok(index),
        (None, _) => Err(SnapshotDeltaError::UnknownTarget { table, id }),
        (Some(_), rows) => Err(SnapshotDeltaError::AmbiguousTarget { table, id, rows }),
    }
}

/// Check that an added row's foreign key names exactly one live staged row.
fn resolve_reference<T>(
    rows: &[T],
    table: SnapshotTable,
    field: &str,
    target_table: SnapshotTable,
    target_id: u32,
    get_id: impl Fn(&T) -> u32,
) -> Result<(), SnapshotDeltaError> {
    match unique_row(rows, target_table, target_id, get_id) {
        Ok(_) => Ok(()),
        Err(SnapshotDeltaError::UnknownTarget { .. }) => {
            Err(SnapshotDeltaError::DanglingReference {
                table,
                field: field.to_string(),
                target_table,
                target_id,
            })
        }
        Err(error) => Err(error),
    }
}

/// Sort `rows` into canonical order, renumber them by position, and return
/// the staging-ID to canonical-ID map. Staging IDs carried by more than one
/// row are left out of the map, so references to them become dangling
/// instead of picking one of the rows.
fn canonicalize_ids<T>(
    rows: &mut [T],
    order: impl Fn(&T, &T) -> Ordering,
    get_id: impl Fn(&T) -> u32,
    set_id: impl Fn(&mut T, u32),
) -> HashMap<u32, u32> {
    let mut id_counts: HashMap<u32, usize> = HashMap::with_capacity(rows.len());
    for row in rows.iter() {
        *id_counts.entry(get_id(row)).or_default() += 1;
    }
    rows.sort_by(order);
    let mut old_to_new = HashMap::with_capacity(rows.len());
    for (index, row) in rows.iter_mut().enumerate() {
        let old_id = get_id(row);
        // A table too long for a `u32` index cannot be published; the
        // out-of-range rows get an ID that validation rejects.
        let new_id = u32::try_from(index)
            .ok()
            .filter(|&id| id < UNRESOLVED_ID)
            .unwrap_or(UNRESOLVED_ID);
        if id_counts.get(&old_id).copied() == Some(1) && new_id != UNRESOLVED_ID {
            old_to_new.insert(old_id, new_id);
        }
        set_id(row, new_id);
    }
    old_to_new
}

pub(crate) fn memory_block_order(left: &MemoryBlock, right: &MemoryBlock) -> Ordering {
    (left.start, &left.name, left.file_offset).cmp(&(right.start, &right.name, right.file_offset))
}

pub(crate) fn function_order(left: &FunctionRecord, right: &FunctionRecord) -> Ordering {
    (left.entry, &left.name, left.size).cmp(&(right.entry, &right.name, right.size))
}

pub(crate) fn symbol_order(left: &SymbolRecord, right: &SymbolRecord) -> Ordering {
    (left.address, left.kind, &left.name).cmp(&(right.address, right.kind, &right.name))
}

pub(crate) fn relocation_order(left: &RelocationRecord, right: &RelocationRecord) -> Ordering {
    (left.address, left.relocation_type, &left.symbol_name).cmp(&(
        right.address,
        right.relocation_type,
        &right.symbol_name,
    ))
}

impl ProgramSnapshotBuilder {
    pub fn new(binary: BinaryMetadata) -> Self {
        Self::from_raw_parts(
            PROGRAM_SNAPSHOT_SCHEMA.to_string(),
            binary,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    /// Stage rows exactly as given. Their IDs become staging IDs; duplicates
    /// are allowed here and surface as ambiguous targets or dangling
    /// references later.
    pub fn from_raw_parts(
        schema: String,
        binary: BinaryMetadata,
        memory_blocks: Vec<MemoryBlock>,
        functions: Vec<FunctionRecord>,
        symbols: Vec<SymbolRecord>,
        relocations: Vec<RelocationRecord>,
    ) -> Self {
        let staging_ids = StagingIds::above(&memory_blocks, &functions, &symbols, &relocations);
        Self {
            schema,
            binary,
            memory_blocks,
            functions,
            symbols,
            relocations,
            staging_ids,
            project_decision_revision: IdentityField::Unavailable,
            has_project_decisions: false,
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
        let (decision_rev, has_decisions) = project_decision_state_from_binary(binary);

        let mut builder = Self::from_raw_parts(
            PROGRAM_SNAPSHOT_SCHEMA.to_string(),
            BinaryMetadata {
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
        );
        builder.project_decision_revision = IdentityField::Available(decision_rev);
        builder.has_project_decisions = has_decisions;
        builder
    }

    pub fn with_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = schema.into();
        self
    }

    pub fn with_binary(mut self, binary: BinaryMetadata) -> Self {
        self.binary = binary;
        self
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

    /// Stage a memory block under a fresh staging ID (the row's own `id` is
    /// ignored) and return that ID.
    pub fn add_memory_block(
        &mut self,
        mut block: MemoryBlock,
    ) -> Result<MemoryBlockId, SnapshotDeltaError> {
        let id = MemoryBlockId(
            self.staging_ids
                .memory_blocks
                .allocate(SnapshotTable::MemoryBlocks)?,
        );
        block.id = id;
        self.memory_blocks.push(block);
        Ok(id)
    }

    /// Stage a function under a fresh staging ID. Its `memory_block`, if any,
    /// must name exactly one live staged block.
    pub fn add_function(
        &mut self,
        mut function: FunctionRecord,
    ) -> Result<FunctionId, SnapshotDeltaError> {
        if let Some(block) = function.memory_block {
            resolve_reference(
                &self.memory_blocks,
                SnapshotTable::Functions,
                "memory_block",
                SnapshotTable::MemoryBlocks,
                block.0,
                |row| row.id.0,
            )?;
        }
        let id = FunctionId(
            self.staging_ids
                .functions
                .allocate(SnapshotTable::Functions)?,
        );
        function.id = id;
        self.functions.push(function);
        Ok(id)
    }

    /// Stage a symbol under a fresh staging ID.
    pub fn add_symbol(&mut self, mut symbol: SymbolRecord) -> Result<SymbolId, SnapshotDeltaError> {
        let id = SymbolId(self.staging_ids.symbols.allocate(SnapshotTable::Symbols)?);
        symbol.id = id;
        self.symbols.push(symbol);
        Ok(id)
    }

    /// Stage a relocation under a fresh staging ID. Its `symbol`, if any,
    /// must name exactly one live staged symbol.
    pub fn add_relocation(
        &mut self,
        mut relocation: RelocationRecord,
    ) -> Result<RelocationId, SnapshotDeltaError> {
        if let Some(symbol) = relocation.symbol {
            resolve_reference(
                &self.symbols,
                SnapshotTable::Relocations,
                "symbol",
                SnapshotTable::Symbols,
                symbol.0,
                |row| row.id.0,
            )?;
        }
        let id = RelocationId(
            self.staging_ids
                .relocations
                .allocate(SnapshotTable::Relocations)?,
        );
        relocation.id = id;
        self.relocations.push(relocation);
        Ok(id)
    }

    /// Apply one typed delta in the builder's staging scope.
    ///
    /// Update and remove deltas must name exactly one staged row; a delta
    /// naming a missing row is refused rather than treated as a no-op, since
    /// it was derived from a state the builder does not hold. Add deltas get a
    /// fresh staging ID that no earlier row, live or removed, has carried, and
    /// their foreign keys must name live rows. Removing a row does not touch
    /// rows that reference it: their references stay on the retired ID, which
    /// is never reissued, and publication reports them as dangling. On error
    /// the builder is unchanged.
    pub fn apply_delta(
        &mut self,
        delta: ProgramSnapshotDelta,
    ) -> Result<&mut Self, SnapshotDeltaError> {
        match delta {
            ProgramSnapshotDelta::ReplaceBinaryMetadata(binary) => {
                self.binary = binary;
            }
            ProgramSnapshotDelta::UpdateFunctionRange { id, entry, size } => {
                let index = unique_row(&self.functions, SnapshotTable::Functions, id.0, |row| {
                    row.id.0
                })?;
                let row = &mut self.functions[index];
                row.entry = entry;
                row.size = size;
            }
            ProgramSnapshotDelta::AddFunction(function) => {
                self.add_function(function)?;
            }
            ProgramSnapshotDelta::RemoveFunction(id) => {
                let index = unique_row(&self.functions, SnapshotTable::Functions, id.0, |row| {
                    row.id.0
                })?;
                self.functions.remove(index);
            }
            ProgramSnapshotDelta::UpdateMemoryBlockRange { id, start, size } => {
                let index = unique_row(
                    &self.memory_blocks,
                    SnapshotTable::MemoryBlocks,
                    id.0,
                    |row| row.id.0,
                )?;
                let row = &mut self.memory_blocks[index];
                row.start = start;
                row.size = size;
            }
            ProgramSnapshotDelta::AddMemoryBlock(block) => {
                self.add_memory_block(block)?;
            }
            ProgramSnapshotDelta::RemoveMemoryBlock(id) => {
                let index = unique_row(
                    &self.memory_blocks,
                    SnapshotTable::MemoryBlocks,
                    id.0,
                    |row| row.id.0,
                )?;
                self.memory_blocks.remove(index);
            }
            ProgramSnapshotDelta::AddSymbol(symbol) => {
                self.add_symbol(symbol)?;
            }
            ProgramSnapshotDelta::RemoveSymbol(id) => {
                let index =
                    unique_row(&self.symbols, SnapshotTable::Symbols, id.0, |row| row.id.0)?;
                self.symbols.remove(index);
            }
            ProgramSnapshotDelta::AddRelocation(relocation) => {
                self.add_relocation(relocation)?;
            }
            ProgramSnapshotDelta::RemoveRelocation(id) => {
                let index =
                    unique_row(&self.relocations, SnapshotTable::Relocations, id.0, |row| {
                        row.id.0
                    })?;
                self.relocations.remove(index);
            }
        }
        Ok(self)
    }

    /// Sort all four tables into canonical order, reassign sequential row IDs,
    /// and remap foreign keys. A reference whose staging ID names no row, or
    /// names several, becomes a dangling reference that validation reports;
    /// it never binds to some other row.
    ///
    /// This ends the staging scope: staging IDs handed out before the call
    /// mean nothing afterwards, and new rows are numbered after the canonical
    /// ones.
    pub fn canonicalize(mut self) -> Self {
        let block_map = canonicalize_ids(
            &mut self.memory_blocks,
            memory_block_order,
            |row| row.id.0,
            |row, id| row.id = MemoryBlockId(id),
        );
        for function in &mut self.functions {
            function.memory_block = function
                .memory_block
                .map(|old| MemoryBlockId(block_map.get(&old.0).copied().unwrap_or(UNRESOLVED_ID)));
        }

        let symbol_map = canonicalize_ids(
            &mut self.symbols,
            symbol_order,
            |row| row.id.0,
            |row, id| row.id = SymbolId(id),
        );
        for relocation in &mut self.relocations {
            relocation.symbol = relocation
                .symbol
                .map(|old| SymbolId(symbol_map.get(&old.0).copied().unwrap_or(UNRESOLVED_ID)));
        }

        canonicalize_ids(
            &mut self.functions,
            function_order,
            |row| row.id.0,
            |row, id| row.id = FunctionId(id),
        );
        canonicalize_ids(
            &mut self.relocations,
            relocation_order,
            |row| row.id.0,
            |row, id| row.id = RelocationId(id),
        );
        self.staging_ids = StagingIds::above(
            &self.memory_blocks,
            &self.functions,
            &self.symbols,
            &self.relocations,
        );
        self
    }

    /// Checked publication boundary: validates every table invariant on the
    /// staged rows and, only when they hold, derives the interval indexes and
    /// exposes a [`ProgramSnapshot`].
    ///
    /// The indexes are built once, here, from the rows just validated, and the
    /// published snapshot cannot be mutated afterwards, so publication does
    /// not re-derive them to compare. [`ProgramSnapshot::integrity_report`]
    /// remains the independent audit: it re-derives both indexes from the
    /// rows and compares them with the cached ones.
    pub fn publish(self) -> SnapshotPublication {
        let report = self.validate_tables();
        if report.is_valid() {
            SnapshotPublication::Valid(self.seal())
        } else {
            SnapshotPublication::Partial(self.into_partial(report))
        }
    }

    /// Publication that rejects individual rows instead of the whole snapshot.
    ///
    /// When the only violations are confined to rows (a range that overflows
    /// the address space, a foreign key naming no row, a function outside the
    /// block it names), those rows are left out, as is every row whose foreign
    /// key names a left-out row, and the remaining rows are published. No row
    /// is altered to make it valid: a row is either published exactly as
    /// staged or listed in [`QuarantinedPublication::rejected_rows`] with its
    /// reasons. Violations that belong to the snapshot as a whole (schema,
    /// binary identity, row IDs, canonical order) are not attributable to a
    /// row, so they still yield [`SnapshotPublication::Partial`] with nothing
    /// quarantined.
    pub fn publish_quarantining_rows(self) -> QuarantinedPublication {
        quarantine::publish_quarantining_rows(self)
    }

    fn validate_tables(&self) -> SnapshotIntegrityReport {
        integrity::validate_tables(&integrity::SnapshotTables {
            schema: &self.schema,
            binary: &self.binary,
            memory_blocks: &self.memory_blocks,
            functions: &self.functions,
            symbols: &self.symbols,
            relocations: &self.relocations,
        })
    }

    /// Derive the interval indexes from rows that have just passed
    /// [`Self::validate_tables`] and expose them as a published snapshot.
    fn seal(self) -> ProgramSnapshot {
        let memory_block_intervals = build_memory_block_intervals(&self.memory_blocks);
        let function_intervals = build_function_intervals(&self.functions);
        let program_identity = ProgramViewIdentity::from_snapshot_parts(
            &self.schema,
            &self.binary,
            &self.memory_blocks,
            &self.functions,
            &self.symbols,
            &self.relocations,
            self.project_decision_revision,
            self.has_project_decisions,
        );
        ProgramSnapshot {
            schema: self.schema,
            binary: self.binary,
            memory_blocks: self.memory_blocks,
            functions: self.functions,
            symbols: self.symbols,
            relocations: self.relocations,
            function_intervals,
            memory_block_intervals,
            program_identity: Arc::new(program_identity),
        }
    }

    fn into_partial(self, report: SnapshotIntegrityReport) -> PartialProgramSnapshot {
        PartialProgramSnapshot {
            schema: self.schema,
            binary: self.binary,
            memory_blocks: self.memory_blocks,
            functions: self.functions,
            symbols: self.symbols,
            relocations: self.relocations,
            report,
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
            program_identity: Arc::new(ProgramViewIdentity::empty()),
        }
    }

    pub fn builder(binary: BinaryMetadata) -> ProgramSnapshotBuilder {
        ProgramSnapshotBuilder::new(binary)
    }

    pub fn to_builder(&self) -> ProgramSnapshotBuilder {
        let mut builder = ProgramSnapshotBuilder::from_raw_parts(
            self.schema.clone(),
            self.binary.clone(),
            self.memory_blocks.clone(),
            self.functions.clone(),
            self.symbols.clone(),
            self.relocations.clone(),
        );
        builder.project_decision_revision = self.program_identity.project_decision_revision;
        builder.has_project_decisions = self.program_identity.has_project_decisions;
        builder
    }

    /// Structured identity binding binary content, load mapping, spec,
    /// analysis revision, and project decision revision.
    pub fn identity(&self) -> &ProgramViewIdentity {
        &self.program_identity
    }

    /// Validate that this snapshot is canonically sound and matches the
    /// identity, load mapping, spec, and project decisions of `binary`.
    pub fn validate_for_loaded_binary(
        &self,
        binary: &LoadedBinary,
    ) -> Result<(), ProgramJoinError> {
        let integrity = self.integrity_report();
        if !integrity.is_valid() {
            return Err(ProgramJoinError::InvalidSnapshot(integrity));
        }

        let bin_id = ProgramViewIdentity::from_loaded_binary(binary);
        let snap_id = &self.program_identity;
        let mut mismatches = Vec::new();

        match (&bin_id.content_hash, &snap_id.content_hash) {
            (IdentityField::Available(b), IdentityField::Available(s)) if b == s => {}
            (IdentityField::Available(b), IdentityField::Available(s)) => {
                mismatches.push(ProgramJoinMismatch::ContentHashMismatch {
                    binary: b.clone(),
                    snapshot: s.clone(),
                });
            }
            (IdentityField::Unavailable, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "content_hash",
                });
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "content_hash",
                });
            }
            (IdentityField::Unavailable, _) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "content_hash",
                });
            }
            (_, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "content_hash",
                });
            }
        }

        match (&bin_id.format, &snap_id.format) {
            (IdentityField::Available(b), IdentityField::Available(s)) if b == s => {}
            (IdentityField::Available(b), IdentityField::Available(s)) => {
                mismatches.push(ProgramJoinMismatch::FormatMismatch {
                    binary: b.clone(),
                    snapshot: s.clone(),
                });
            }
            (IdentityField::Unavailable, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "format",
                });
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "format",
                });
            }
            (IdentityField::Unavailable, _) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "format",
                });
            }
            (_, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "format",
                });
            }
        }

        match (bin_id.bitness, snap_id.bitness) {
            (IdentityField::Available(b), IdentityField::Available(s)) if b == s => {}
            (IdentityField::Available(b), IdentityField::Available(s)) => {
                mismatches.push(ProgramJoinMismatch::BitnessMismatch {
                    binary: b,
                    snapshot: s,
                });
            }
            (IdentityField::Unavailable, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "bitness",
                });
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "bitness",
                });
            }
            (IdentityField::Unavailable, _) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "binary",
                    field: "bitness",
                });
            }
            (_, IdentityField::Unavailable) => {
                mismatches.push(ProgramJoinMismatch::MissingRequiredIdentity {
                    side: "snapshot",
                    field: "bitness",
                });
            }
        }

        if bin_id.image_base != snap_id.image_base {
            mismatches.push(ProgramJoinMismatch::ImageBaseMismatch {
                binary: bin_id.image_base,
                snapshot: snap_id.image_base,
            });
        }
        if bin_id.entry_point != snap_id.entry_point {
            mismatches.push(ProgramJoinMismatch::EntryPointMismatch {
                binary: bin_id.entry_point,
                snapshot: snap_id.entry_point,
            });
        }
        if bin_id.load_mapping_digest != snap_id.load_mapping_digest {
            mismatches.push(ProgramJoinMismatch::LoadMappingMismatch {
                binary: bin_id.load_mapping_digest,
                snapshot: snap_id.load_mapping_digest,
            });
        }

        for (field, bin_val, snap_val) in [
            ("processor", &bin_id.processor, &snap_id.processor),
            ("endian", &bin_id.endian, &snap_id.endian),
            ("abi", &bin_id.abi, &snap_id.abi),
            ("language_id", &bin_id.language_id, &snap_id.language_id),
            (
                "compiler_spec_id",
                &bin_id.compiler_spec_id,
                &snap_id.compiler_spec_id,
            ),
        ] {
            if bin_val != snap_val {
                mismatches.push(ProgramJoinMismatch::LoadSpecMismatch {
                    field,
                    binary: bin_val.clone(),
                    snapshot: snap_val.clone(),
                });
            }
        }

        let decision_mismatch = match (
            bin_id.project_decision_revision,
            snap_id.project_decision_revision,
        ) {
            (IdentityField::Available(b), IdentityField::Available(s)) => b != s,
            (IdentityField::Available(_), IdentityField::Unavailable) => {
                bin_id.has_project_decisions
            }
            (IdentityField::Unavailable, IdentityField::Available(_)) => {
                snap_id.has_project_decisions
            }
            (IdentityField::Unavailable, IdentityField::Unavailable) => {
                bin_id.has_project_decisions != snap_id.has_project_decisions
            }
        };
        if decision_mismatch || bin_id.has_project_decisions != snap_id.has_project_decisions {
            mismatches.push(ProgramJoinMismatch::ProjectDecisionRevisionMismatch {
                binary: bin_id.project_decision_revision,
                snapshot: snap_id.project_decision_revision,
            });
        }

        if mismatches.is_empty() {
            Ok(())
        } else {
            Err(ProgramJoinError::IdentityMismatch(mismatches))
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

    /// [`ProgramSnapshotBuilder::publish_quarantining_rows`] over the loader's
    /// facts: one malformed loader row costs that row (and the rows that
    /// reference it), not the whole program.
    pub fn publish_loaded_binary_quarantining_rows(
        binary: &LoadedBinary,
    ) -> QuarantinedPublication {
        ProgramSnapshotBuilder::from_loaded_binary(binary).publish_quarantining_rows()
    }

    pub fn apply_delta(&self, delta: ProgramSnapshotDelta) -> Result<Self, SnapshotUpdateError> {
        self.apply_deltas([delta])
    }

    /// Publish a replacement snapshot with `deltas` applied in order. IDs in
    /// the deltas are this snapshot's IDs; rows added by the batch get fresh
    /// IDs, so a delta cannot refer to a row added earlier in the same batch.
    /// The batch is all-or-nothing: any refused delta or failed validation
    /// publishes nothing.
    pub fn apply_deltas(
        &self,
        deltas: impl IntoIterator<Item = ProgramSnapshotDelta>,
    ) -> Result<Self, SnapshotUpdateError> {
        let mut builder = self.to_builder();
        for (index, delta) in deltas.into_iter().enumerate() {
            builder
                .apply_delta(delta)
                .map_err(|error| SnapshotUpdateError::RejectedDelta { index, error })?;
        }
        builder
            .canonicalize()
            .build()
            .map_err(SnapshotUpdateError::Integrity)
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
        builder.functions[0].entry = block_end - 10;
        builder.functions[0].size = 50;

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
        reordered_builder.functions.reverse();
        reordered_builder.symbols.reverse();
        reordered_builder.relocations.reverse();
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
        unsorted.functions.reverse();
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
        assert!(
            err.integrity_report()
                .expect("validation failure")
                .issues
                .iter()
                .any(|issue| matches!(
                    issue,
                    SnapshotIntegrityIssue::DanglingReference {
                        table: SnapshotTable::Functions,
                        target_table: SnapshotTable::MemoryBlocks,
                        ..
                    }
                ))
        );

        let err = valid
            .apply_delta(ProgramSnapshotDelta::RemoveSymbol(
                valid.relocations()[0].symbol.expect("linked symbol"),
            ))
            .expect_err("removing referenced symbol must fail validation");
        assert!(
            err.integrity_report()
                .expect("validation failure")
                .issues
                .iter()
                .any(|issue| matches!(
                    issue,
                    SnapshotIntegrityIssue::DanglingReference {
                        table: SnapshotTable::Relocations,
                        target_table: SnapshotTable::Symbols,
                        ..
                    }
                ))
        );

        // Shrinking a memory block past a function that points to it fails with FunctionOutsideMemoryBlock.
        let err = valid
            .apply_delta(ProgramSnapshotDelta::UpdateMemoryBlockRange {
                id: MemoryBlockId(0),
                start: 0x1000,
                size: 8,
            })
            .expect_err("shrinking block past functions must fail validation");
        assert!(
            err.integrity_report()
                .expect("validation failure")
                .issues
                .iter()
                .any(|issue| matches!(
                    issue,
                    SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
                ))
        );

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
        ver_builder
            .apply_delta(ProgramSnapshotDelta::AddFunction(FunctionRecord {
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
            }))
            .expect("alias function is staged");
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
