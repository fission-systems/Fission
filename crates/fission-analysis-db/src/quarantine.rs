//! Row-level quarantine at the publication boundary.
//!
//! One malformed loader row (a section whose range overflows the address
//! space, say) must not cost every program fact. Violations that are confined
//! to a row reject that row, and every row whose foreign key names a rejected
//! row; the rest is published. Rows are never altered to make them valid:
//! each staged row is either published as staged or reported here with its
//! reasons.

use super::{
    ProgramSnapshotBuilder, SnapshotIntegrityIssue, SnapshotPublication, SnapshotTable, StagingIds,
    UNRESOLVED_ID,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// A staged row that publication left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RejectedRow {
    pub table: SnapshotTable,
    /// The row's ID among the staged rows, before rejected rows were removed.
    pub staged_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Block start, function entry, or symbol / relocation address.
    pub address: u64,
    pub reasons: Vec<RowRejection>,
}

/// Why a row was left out of a publication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RowRejection {
    /// The row itself violates a snapshot invariant.
    Invalid { issue: SnapshotIntegrityIssue },
    /// A foreign key of the row names a row that was rejected.
    ReferencesRejectedRow {
        field: String,
        target_table: SnapshotTable,
        target_staged_id: u32,
    },
}

/// Outcome of [`ProgramSnapshotBuilder::publish_quarantining_rows`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantinedPublication {
    /// `Valid` with every row that was not rejected, or `Partial` when a
    /// violation belongs to the snapshot as a whole.
    pub publication: SnapshotPublication,
    /// Rows left out of `publication`, in table order then staged ID order.
    pub rejected_rows: Vec<RejectedRow>,
}

impl QuarantinedPublication {
    /// Every staged row was published.
    pub fn is_complete(&self) -> bool {
        self.publication.is_valid() && self.rejected_rows.is_empty()
    }
}

/// Issues a single row owns. Everything else (schema, binary identity, row
/// IDs, canonical order, derived indexes) describes the snapshot as a whole.
fn is_row_local(issue: &SnapshotIntegrityIssue) -> bool {
    matches!(
        issue,
        SnapshotIntegrityIssue::AddressRangeOverflow { .. }
            | SnapshotIntegrityIssue::DanglingReference { .. }
            | SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
    )
}

type Rejections = BTreeMap<usize, Vec<RowRejection>>;

/// Remove the rows whose index is in `rejected` (order preserved) and
/// renumber the survivors by position. Returns old index to new ID.
fn retain_and_renumber<T>(
    rows: &mut Vec<T>,
    rejected: &Rejections,
    set_id: impl Fn(&mut T, u32),
) -> HashMap<u32, u32> {
    let mut old_to_new = HashMap::with_capacity(rows.len());
    let mut index = 0usize;
    rows.retain(|_| {
        let keep = !rejected.contains_key(&index);
        index += 1;
        keep
    });
    let mut kept = (0..index).filter(|old| !rejected.contains_key(old));
    for (new, row) in rows.iter_mut().enumerate() {
        let old = kept.next().expect("one survivor per kept index");
        // Survivors are fewer than the staged rows, whose IDs already fit.
        let new = new as u32;
        old_to_new.insert(old as u32, new);
        set_id(row, new);
    }
    old_to_new
}

pub(super) fn publish_quarantining_rows(
    mut builder: ProgramSnapshotBuilder,
) -> QuarantinedPublication {
    let report = builder.validate_tables();
    if report.is_valid() {
        return QuarantinedPublication {
            publication: SnapshotPublication::Valid(builder.seal()),
            rejected_rows: Vec::new(),
        };
    }
    if !report.issues.iter().all(is_row_local) {
        return QuarantinedPublication {
            publication: SnapshotPublication::Partial(builder.into_partial(report)),
            rejected_rows: Vec::new(),
        };
    }

    // No `InvalidTableId` was reported, so every row's ID is its index.
    let mut blocks = Rejections::new();
    let mut functions = Rejections::new();
    let mut symbols = Rejections::new();
    let mut relocations = Rejections::new();
    for issue in report.issues {
        let (rejections, index) = match &issue {
            SnapshotIntegrityIssue::AddressRangeOverflow { table, index, .. }
            | SnapshotIntegrityIssue::DanglingReference { table, index, .. } => (
                match table {
                    SnapshotTable::MemoryBlocks => &mut blocks,
                    SnapshotTable::Functions => &mut functions,
                    SnapshotTable::Symbols => &mut symbols,
                    SnapshotTable::Relocations => &mut relocations,
                },
                *index,
            ),
            SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { function, .. } => {
                (&mut functions, function.0 as usize)
            }
            _ => unreachable!("only row-local issues reach quarantine"),
        };
        rejections
            .entry(index)
            .or_default()
            .push(RowRejection::Invalid { issue });
    }

    // Close over references. Only functions (-> memory block) and relocations
    // (-> symbol) carry foreign keys and nothing references a function or a
    // relocation, so one pass over each referencing table is the full
    // transitive closure.
    for (index, function) in builder.functions.iter().enumerate() {
        let Some(block) = function.memory_block else {
            continue;
        };
        if !blocks.contains_key(&(block.0 as usize)) {
            continue;
        }
        let reasons = functions.entry(index).or_default();
        // Failing containment against a rejected block is a consequence of
        // that block's rejection, not a separate fault of the function.
        reasons.retain(|reason| {
            !matches!(
                reason,
                RowRejection::Invalid {
                    issue: SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
                }
            )
        });
        reasons.push(RowRejection::ReferencesRejectedRow {
            field: "memory_block".to_string(),
            target_table: SnapshotTable::MemoryBlocks,
            target_staged_id: block.0,
        });
    }
    for (index, relocation) in builder.relocations.iter().enumerate() {
        let Some(symbol) = relocation.symbol else {
            continue;
        };
        if symbols.contains_key(&(symbol.0 as usize)) {
            relocations
                .entry(index)
                .or_default()
                .push(RowRejection::ReferencesRejectedRow {
                    field: "symbol".to_string(),
                    target_table: SnapshotTable::Symbols,
                    target_staged_id: symbol.0,
                });
        }
    }

    let mut rejected_rows = Vec::new();
    let mut report_rows =
        |table, rejections: &Rejections, describe: &dyn Fn(usize) -> (Option<String>, u64)| {
            for (&index, reasons) in rejections {
                let (name, address) = describe(index);
                rejected_rows.push(RejectedRow {
                    table,
                    staged_id: index as u32,
                    name,
                    address,
                    reasons: reasons.clone(),
                });
            }
        };
    report_rows(SnapshotTable::MemoryBlocks, &blocks, &|index| {
        let row = &builder.memory_blocks[index];
        (Some(row.name.clone()), row.start)
    });
    report_rows(SnapshotTable::Functions, &functions, &|index| {
        let row = &builder.functions[index];
        (Some(row.name.clone()), row.entry)
    });
    report_rows(SnapshotTable::Symbols, &symbols, &|index| {
        let row = &builder.symbols[index];
        (Some(row.name.clone()), row.address)
    });
    report_rows(SnapshotTable::Relocations, &relocations, &|index| {
        let row = &builder.relocations[index];
        (row.symbol_name.clone(), row.address)
    });

    let block_ids = retain_and_renumber(&mut builder.memory_blocks, &blocks, |row, id| {
        row.id = super::MemoryBlockId(id)
    });
    let symbol_ids = retain_and_renumber(&mut builder.symbols, &symbols, |row, id| {
        row.id = super::SymbolId(id)
    });
    retain_and_renumber(&mut builder.functions, &functions, |row, id| {
        row.id = super::FunctionId(id)
    });
    retain_and_renumber(&mut builder.relocations, &relocations, |row, id| {
        row.id = super::RelocationId(id)
    });
    // Every surviving foreign key names a surviving row (dangling and
    // rejected targets were rejected above), so the remap is total; were it
    // not, the unresolved sentinel makes the final validation fail closed.
    for function in &mut builder.functions {
        if let Some(block) = &mut function.memory_block {
            block.0 = block_ids.get(&block.0).copied().unwrap_or(UNRESOLVED_ID);
        }
    }
    for relocation in &mut builder.relocations {
        if let Some(symbol) = &mut relocation.symbol {
            symbol.0 = symbol_ids.get(&symbol.0).copied().unwrap_or(UNRESOLVED_ID);
        }
    }
    builder.staging_ids = StagingIds::above(
        &builder.memory_blocks,
        &builder.functions,
        &builder.symbols,
        &builder.relocations,
    );

    // Removing rows cannot introduce a violation, but the published snapshot
    // goes through the same validation as any other.
    let publication = builder.publish();
    QuarantinedPublication {
        publication,
        rejected_rows,
    }
}
