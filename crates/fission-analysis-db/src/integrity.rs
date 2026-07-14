use super::{FactConfidence, FactSource, FunctionId, MemoryBlockId, ProgramSnapshot};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotTable {
    MemoryBlocks,
    Functions,
    Symbols,
    Relocations,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotIntegrityReport {
    pub issues: Vec<SnapshotIntegrityIssue>,
}

impl SnapshotIntegrityReport {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
    let mut inventory = FactInventory {
        memory_blocks: snapshot.memory_blocks.len(),
        functions: snapshot.functions.len(),
        symbols: snapshot.symbols.len(),
        relocations: snapshot.relocations.len(),
        ..FactInventory::default()
    };
    inventory.total =
        inventory.memory_blocks + inventory.functions + inventory.symbols + inventory.relocations;

    let provenances = snapshot
        .memory_blocks
        .iter()
        .map(|row| &row.provenance)
        .chain(snapshot.functions.iter().map(|row| &row.provenance))
        .chain(snapshot.symbols.iter().map(|row| &row.provenance))
        .chain(snapshot.relocations.iter().map(|row| &row.provenance));
    for provenance in provenances {
        *inventory.by_source.entry(provenance.source).or_default() += 1;
        *inventory
            .by_confidence
            .entry(provenance.confidence)
            .or_default() += 1;
    }
    inventory
}

pub(super) fn integrity_report(snapshot: &ProgramSnapshot) -> SnapshotIntegrityReport {
    let mut report = SnapshotIntegrityReport::default();
    if snapshot.schema != super::PROGRAM_SNAPSHOT_SCHEMA {
        report.issues.push(SnapshotIntegrityIssue::SchemaMismatch {
            expected: super::PROGRAM_SNAPSHOT_SCHEMA.to_string(),
            actual: snapshot.schema.to_string(),
        });
    }

    let has_facts = !snapshot.memory_blocks.is_empty()
        || !snapshot.functions.is_empty()
        || !snapshot.symbols.is_empty()
        || !snapshot.relocations.is_empty();
    for (field, value) in [
        ("content_hash", snapshot.binary.content_hash.as_str()),
        ("format", snapshot.binary.format.as_str()),
    ] {
        if has_facts && value.trim().is_empty() {
            report
                .issues
                .push(SnapshotIntegrityIssue::MissingBinaryIdentity {
                    field: field.to_string(),
                });
        }
    }

    for (index, block) in snapshot.memory_blocks.iter().enumerate() {
        validate_id(&mut report, SnapshotTable::MemoryBlocks, index, block.id.0);
        validate_range(
            &mut report,
            SnapshotTable::MemoryBlocks,
            index,
            block.start,
            block.size,
        );
    }
    for (index, function) in snapshot.functions.iter().enumerate() {
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
        let Some(block) = snapshot
            .memory_blocks
            .iter()
            .find(|block| block.id == block_id)
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
        let contains_entry = block.start <= function.entry
            && block
                .start
                .checked_add(block.size)
                .is_some_and(|end| function.entry < end);
        if !contains_entry {
            report
                .issues
                .push(SnapshotIntegrityIssue::FunctionOutsideMemoryBlock {
                    function: function.id,
                    memory_block: block_id,
                });
        }
    }
    for (index, symbol) in snapshot.symbols.iter().enumerate() {
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
    for (index, relocation) in snapshot.relocations.iter().enumerate() {
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
        if let Some(symbol_id) = relocation.symbol
            && !snapshot.symbols.iter().any(|row| row.id == symbol_id)
        {
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

    validate_canonical_order(&mut report, snapshot);
    report
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

fn validate_canonical_order(report: &mut SnapshotIntegrityReport, snapshot: &ProgramSnapshot) {
    validate_order(
        report,
        SnapshotTable::MemoryBlocks,
        snapshot.memory_blocks.windows(2).map(|pair| {
            (
                (&pair[0].start, &pair[0].name, &pair[0].file_offset),
                (&pair[1].start, &pair[1].name, &pair[1].file_offset),
            )
        }),
    );
    validate_order(
        report,
        SnapshotTable::Functions,
        snapshot.functions.windows(2).map(|pair| {
            (
                (&pair[0].entry, &pair[0].name, &pair[0].size),
                (&pair[1].entry, &pair[1].name, &pair[1].size),
            )
        }),
    );
    validate_order(
        report,
        SnapshotTable::Symbols,
        snapshot.symbols.windows(2).map(|pair| {
            (
                (&pair[0].address, &pair[0].kind, &pair[0].name),
                (&pair[1].address, &pair[1].kind, &pair[1].name),
            )
        }),
    );
    validate_order(
        report,
        SnapshotTable::Relocations,
        snapshot.relocations.windows(2).map(|pair| {
            (
                (
                    &pair[0].address,
                    &pair[0].relocation_type,
                    &pair[0].symbol_name,
                ),
                (
                    &pair[1].address,
                    &pair[1].relocation_type,
                    &pair[1].symbol_name,
                ),
            )
        }),
    );
}

fn validate_order<T: Ord>(
    report: &mut SnapshotIntegrityReport,
    table: SnapshotTable,
    pairs: impl Iterator<Item = (T, T)>,
) {
    for (previous_index, (left, right)) in pairs.enumerate() {
        if left > right {
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
