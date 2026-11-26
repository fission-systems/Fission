use super::{
    BinaryMetadata, FactConfidence, FactSource, FunctionId, FunctionRecord, MemoryBlock,
    MemoryBlockId, ProgramSnapshot, RelocationRecord, SymbolRecord,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTable {
    MemoryBlocks,
    Functions,
    Symbols,
    Relocations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SnapshotIntegrityIssue {
    SchemaMismatch {
        expected: String,
        actual: String,
    },
    MissingBinaryIdentity {
        field: String,
    },
    InvalidTableId {
        table: SnapshotTable,
        index: usize,
        actual: u32,
    },
    NonCanonicalOrder {
        table: SnapshotTable,
        previous_index: usize,
        current_index: usize,
    },
    AddressRangeOverflow {
        table: SnapshotTable,
        index: usize,
        start: u64,
        size: u64,
    },
    DanglingReference {
        table: SnapshotTable,
        index: usize,
        field: String,
        target_table: SnapshotTable,
        target_id: u32,
    },
    FunctionOutsideMemoryBlock {
        function: FunctionId,
        memory_block: MemoryBlockId,
    },
    DerivedIndexMismatch {
        table: SnapshotTable,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotIntegrityReport {
    pub issues: Vec<SnapshotIntegrityIssue>,
}

impl SnapshotIntegrityReport {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

impl fmt::Display for SnapshotIntegrityReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "program snapshot integrity check failed ({} issue(s)): {:?}",
            self.issues.len(),
            self.issues
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactInventory {
    pub total: usize,
    pub memory_blocks: usize,
    pub functions: usize,
    pub symbols: usize,
    pub relocations: usize,
    pub by_source: BTreeMap<FactSource, usize>,
    pub by_confidence: BTreeMap<FactConfidence, usize>,
}

pub(super) fn fact_inventory(snapshot: &ProgramSnapshot) -> FactInventory {
    fact_inventory_from_tables(
        &snapshot.memory_blocks,
        &snapshot.functions,
        &snapshot.symbols,
        &snapshot.relocations,
    )
}

pub(super) fn fact_inventory_from_tables(
    memory_blocks: &[super::MemoryBlock],
    functions: &[super::FunctionRecord],
    symbols: &[super::SymbolRecord],
    relocations: &[super::RelocationRecord],
) -> FactInventory {
    let mut inventory = FactInventory {
        memory_blocks: memory_blocks.len(),
        functions: functions.len(),
        symbols: symbols.len(),
        relocations: relocations.len(),
        ..FactInventory::default()
    };
    inventory.total =
        inventory.memory_blocks + inventory.functions + inventory.symbols + inventory.relocations;

    let provenances = memory_blocks
        .iter()
        .map(|row| &row.provenance)
        .chain(functions.iter().map(|row| &row.provenance))
        .chain(symbols.iter().map(|row| &row.provenance))
        .chain(relocations.iter().map(|row| &row.provenance));
    for provenance in provenances {
        *inventory.by_source.entry(provenance.source).or_default() += 1;
        *inventory
            .by_confidence
            .entry(provenance.confidence)
            .or_default() += 1;
    }
    inventory
}

/// Borrowed view of the canonical tables, shared by the publication boundary
/// (staged rows) and the audit of a published snapshot.
pub(crate) struct SnapshotTables<'a> {
    pub schema: &'a str,
    pub binary: &'a BinaryMetadata,
    pub memory_blocks: &'a [MemoryBlock],
    pub functions: &'a [FunctionRecord],
    pub symbols: &'a [SymbolRecord],
    pub relocations: &'a [RelocationRecord],
}

/// Full audit of a published snapshot: every table invariant, plus an
/// independent re-derivation of the cached interval indexes.
pub(super) fn integrity_report(snapshot: &ProgramSnapshot) -> SnapshotIntegrityReport {
    let mut report = validate_tables(&SnapshotTables {
        schema: &snapshot.schema,
        binary: &snapshot.binary,
        memory_blocks: &snapshot.memory_blocks,
        functions: &snapshot.functions,
        symbols: &snapshot.symbols,
        relocations: &snapshot.relocations,
    });
    validate_derived_indexes(&mut report, snapshot);
    report
}

/// Row ID to position, for resolving foreign keys in O(1). In a valid table
/// the ID is the position, so no map is built; otherwise (already reported
/// as `InvalidTableId`) the first row carrying an ID answers for it.
enum IdLookup {
    Dense(usize),
    Map(HashMap<u32, usize>),
}

impl IdLookup {
    fn new(ids: impl ExactSizeIterator<Item = u32> + Clone) -> Self {
        let len = ids.len();
        if ids
            .clone()
            .enumerate()
            .all(|(index, id)| usize::try_from(id).ok() == Some(index))
        {
            return Self::Dense(len);
        }
        let mut map = HashMap::with_capacity(len);
        for (index, id) in ids.enumerate() {
            map.entry(id).or_insert(index);
        }
        Self::Map(map)
    }

    fn position(&self, id: u32) -> Option<usize> {
        match self {
            Self::Dense(len) => usize::try_from(id).ok().filter(|index| index < len),
            Self::Map(map) => map.get(&id).copied(),
        }
    }
}

/// Every invariant of the canonical tables themselves (schema, identity,
/// IDs, ranges, foreign keys, canonical order). Derived indexes are not
/// consulted.
pub(crate) fn validate_tables(tables: &SnapshotTables<'_>) -> SnapshotIntegrityReport {
    let mut report = SnapshotIntegrityReport::default();
    if tables.schema != super::PROGRAM_SNAPSHOT_SCHEMA {
        report.issues.push(SnapshotIntegrityIssue::SchemaMismatch {
            expected: super::PROGRAM_SNAPSHOT_SCHEMA.to_string(),
            actual: tables.schema.to_string(),
        });
    }

    let has_facts = !tables.memory_blocks.is_empty()
        || !tables.functions.is_empty()
        || !tables.symbols.is_empty()
        || !tables.relocations.is_empty();
    for (field, value) in [
        ("content_hash", tables.binary.content_hash.as_str()),
        ("format", tables.binary.format.as_str()),
    ] {
        if has_facts && value.trim().is_empty() {
            report
                .issues
                .push(SnapshotIntegrityIssue::MissingBinaryIdentity {
                    field: field.to_string(),
                });
        }
    }

    for (index, block) in tables.memory_blocks.iter().enumerate() {
        validate_id(&mut report, SnapshotTable::MemoryBlocks, index, block.id.0);
        validate_range(
            &mut report,
            SnapshotTable::MemoryBlocks,
            index,
            block.start,
            block.size,
        );
    }
    let blocks_by_id = IdLookup::new(tables.memory_blocks.iter().map(|block| block.id.0));
    for (index, function) in tables.functions.iter().enumerate() {
        validate_id(&mut report, SnapshotTable::Functions, index, function.id.0);
        validate_range(
            &mut report,
            SnapshotTable::Functions,
            index,
            function.entry,
            function.size,
        );
        let Some(block_id) = function.memory_block else {
            continue;
        };
        let Some(block) = blocks_by_id
            .position(block_id.0)
            .map(|position| &tables.memory_blocks[position])
        else {
            report
                .issues
                .push(SnapshotIntegrityIssue::DanglingReference {
                    table: SnapshotTable::Functions,
                    index,
                    field: "memory_block".to_string(),
                    target_table: SnapshotTable::MemoryBlocks,
                    target_id: block_id.0,
                });
            continue;
        };
        if !crate::block_contains_function(block, function.entry, function.size) {
            report
                .issues
                .push(SnapshotIntegrityIssue::FunctionOutsideMemoryBlock {
                    function: function.id,
                    memory_block: block_id,
                });
        }
    }
    for (index, symbol) in tables.symbols.iter().enumerate() {
        validate_id(&mut report, SnapshotTable::Symbols, index, symbol.id.0);
        if let Some(size) = symbol.size {
            validate_range(
                &mut report,
                SnapshotTable::Symbols,
                index,
                symbol.address,
                size,
            );
        }
    }
    let symbols_by_id = IdLookup::new(tables.symbols.iter().map(|symbol| symbol.id.0));
    for (index, relocation) in tables.relocations.iter().enumerate() {
        validate_id(
            &mut report,
            SnapshotTable::Relocations,
            index,
            relocation.id.0,
        );
        validate_range(
            &mut report,
            SnapshotTable::Relocations,
            index,
            relocation.address,
            u64::from(relocation.size),
        );
        if let Some(symbol_id) = relocation.symbol {
            if symbols_by_id.position(symbol_id.0).is_some() {
                continue;
            }
            report
                .issues
                .push(SnapshotIntegrityIssue::DanglingReference {
                    table: SnapshotTable::Relocations,
                    index,
                    field: "symbol".to_string(),
                    target_table: SnapshotTable::Symbols,
                    target_id: symbol_id.0,
                });
        }
    }

    validate_order(
        &mut report,
        SnapshotTable::MemoryBlocks,
        tables.memory_blocks,
        crate::memory_block_order,
    );
    validate_order(
        &mut report,
        SnapshotTable::Functions,
        tables.functions,
        crate::function_order,
    );
    validate_order(
        &mut report,
        SnapshotTable::Symbols,
        tables.symbols,
        crate::symbol_order,
    );
    validate_order(
        &mut report,
        SnapshotTable::Relocations,
        tables.relocations,
        crate::relocation_order,
    );
    report
}

fn validate_derived_indexes(report: &mut SnapshotIntegrityReport, snapshot: &ProgramSnapshot) {
    if snapshot.memory_block_intervals
        != crate::build_memory_block_intervals(&snapshot.memory_blocks)
    {
        report
            .issues
            .push(SnapshotIntegrityIssue::DerivedIndexMismatch {
                table: SnapshotTable::MemoryBlocks,
            });
    }
    if snapshot.function_intervals != crate::build_function_intervals(&snapshot.functions) {
        report
            .issues
            .push(SnapshotIntegrityIssue::DerivedIndexMismatch {
                table: SnapshotTable::Functions,
            });
    }
}

fn validate_id(
    report: &mut SnapshotIntegrityReport,
    table: SnapshotTable,
    index: usize,
    actual: u32,
) {
    if usize::try_from(actual).ok() != Some(index) {
        report.issues.push(SnapshotIntegrityIssue::InvalidTableId {
            table,
            index,
            actual,
        });
    }
}

fn validate_range(
    report: &mut SnapshotIntegrityReport,
    table: SnapshotTable,
    index: usize,
    start: u64,
    size: u64,
) {
    if start.checked_add(size).is_none() {
        report
            .issues
            .push(SnapshotIntegrityIssue::AddressRangeOverflow {
                table,
                index,
                start,
                size,
            });
    }
}

fn validate_order<T>(
    report: &mut SnapshotIntegrityReport,
    table: SnapshotTable,
    rows: &[T],
    order: impl Fn(&T, &T) -> Ordering,
) {
    for (previous_index, pair) in rows.windows(2).enumerate() {
        if order(&pair[0], &pair[1]) == Ordering::Greater {
            report
                .issues
                .push(SnapshotIntegrityIssue::NonCanonicalOrder {
                    table,
                    previous_index,
                    current_index: previous_index + 1,
                });
        }
    }
}
