//! Row-level quarantine: one malformed row costs that row and the rows that
//! reference it, never the rest of the program, and never silently.

use fission_analysis_db::{
    BinaryMetadata, FactConfidence, FactSource, FunctionId, FunctionKind, FunctionRecord,
    MemoryBlock, MemoryBlockId, PROGRAM_SNAPSHOT_SCHEMA, Permissions, ProgramSnapshot,
    ProgramSnapshotBuilder, Provenance, RejectedRow, RowRejection, SnapshotIntegrityIssue,
    SnapshotPublication, SnapshotTable,
};
use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, RelocationEntry};
use fission_loader::{FunctionInfo, LoadedBinary, SectionInfo};
use std::collections::HashMap;

const BAD_SECTION: u64 = 0xFFFF_FFFF_FFFF_F000;
const BAD_SYMBOL: u64 = 0xFFFF_FFFF_FFFF_FFF8;

fn section(name: &str, start: u64, size: u64, execute: bool) -> SectionInfo {
    SectionInfo {
        name: name.into(),
        virtual_address: start,
        virtual_size: size,
        file_offset: 0,
        file_size: 0,
        is_executable: execute,
        is_readable: true,
        is_writable: !execute,
    }
}

/// A well-formed ELF image plus one section and one data symbol whose ranges
/// run past the end of the address space, and a relocation against that
/// symbol.
fn binary_with_one_overflowing_section() -> LoadedBinary {
    let functions = [("f_one", 0x1000), ("f_two", 0x1010)].map(|(name, address)| FunctionInfo {
        name: name.into(),
        address,
        size: 0x10,
        origin: Some("symbol_table".into()),
        ..FunctionInfo::default()
    });
    LoadedBinaryBuilder::new("quarantine.bin".into(), DataBuffer::Heap(vec![0; 16]))
        .format("ELF")
        .is_64bit(true)
        .entry_point(0x1000)
        .add_sections([
            section(".text", 0x1000, 0x100, true),
            section(".data", 0x2000, 0x100, false),
            section(".bad", BAD_SECTION, 0x2000, false),
        ])
        .add_functions(functions)
        .add_global_symbols(HashMap::from([
            (0x2000, "g_ok".to_string()),
            (BAD_SYMBOL, "g_bad".to_string()),
        ]))
        .add_global_symbol_sizes(HashMap::from([(0x2000, 8), (BAD_SYMBOL, 0x10)]))
        .add_relocations([
            RelocationEntry {
                address: 0x2008,
                r_type: 1,
                size: 8,
                addend: 0,
                symbol_name: Some("g_ok".into()),
                symbol_address: Some(0x2000),
            },
            RelocationEntry {
                address: 0x2010,
                r_type: 1,
                size: 8,
                addend: 0,
                symbol_name: Some("g_bad".into()),
                symbol_address: Some(BAD_SYMBOL),
            },
        ])
        .build()
        .expect("fixture builds")
}

#[test]
fn one_overflowing_section_costs_only_its_rows_and_their_dependents() {
    let binary = binary_with_one_overflowing_section();
    // The strict boundary still refuses the snapshot as a whole.
    assert!(ProgramSnapshot::try_from_loaded_binary(&binary).is_err());

    let outcome = ProgramSnapshot::publish_loaded_binary_quarantining_rows(&binary);
    assert!(!outcome.is_complete());
    let SnapshotPublication::Valid(snapshot) = &outcome.publication else {
        panic!("row-local faults must not fail the whole snapshot: {outcome:?}");
    };
    assert!(snapshot.integrity_report().is_valid());

    // The other facts survive, unaltered.
    let block_names: Vec<_> = snapshot
        .memory_blocks()
        .iter()
        .map(|b| b.name.as_str())
        .collect();
    assert_eq!(block_names, [".text", ".data"]);
    let function_names: Vec<_> = snapshot
        .functions()
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(function_names, ["f_one", "f_two"]);
    assert!(
        snapshot
            .functions()
            .iter()
            .all(|f| f.memory_block == Some(MemoryBlockId(0)))
    );
    assert!(snapshot.symbols().iter().any(|s| s.name == "g_ok"));
    assert!(snapshot.symbols().iter().all(|s| s.name != "g_bad"));
    assert_eq!(snapshot.relocations().len(), 1);
    let survivor = &snapshot.relocations()[0];
    let target = survivor.symbol.expect("g_ok relocation keeps its symbol");
    assert_eq!(snapshot.symbols()[target.0 as usize].name, "g_ok");
    assert_eq!(
        snapshot
            .function_containing(0x1014)
            .map(|f| f.name.as_str()),
        Some("f_two")
    );
    assert_eq!(
        snapshot
            .memory_block_containing(0x2004)
            .map(|b| b.name.as_str()),
        Some(".data")
    );

    // The bad rows are reported with their reasons, dependents included.
    let rejected: Vec<(SnapshotTable, Option<&str>)> = outcome
        .rejected_rows
        .iter()
        .map(|row| (row.table, row.name.as_deref()))
        .collect();
    assert_eq!(
        rejected,
        [
            (SnapshotTable::MemoryBlocks, Some(".bad")),
            (SnapshotTable::Symbols, Some("g_bad")),
            (SnapshotTable::Relocations, Some("g_bad")),
        ]
    );
    let bad_block = &outcome.rejected_rows[0];
    assert_eq!(bad_block.address, BAD_SECTION);
    assert!(matches!(
        bad_block.reasons.as_slice(),
        [RowRejection::Invalid {
            issue: SnapshotIntegrityIssue::AddressRangeOverflow {
                table: SnapshotTable::MemoryBlocks,
                ..
            }
        }]
    ));
    let bad_symbol = &outcome.rejected_rows[1];
    let dependent = &outcome.rejected_rows[2];
    assert_eq!(
        dependent.reasons,
        [RowRejection::ReferencesRejectedRow {
            field: "symbol".into(),
            target_table: SnapshotTable::Symbols,
            target_staged_id: bad_symbol.staged_id,
        }]
    );
    assert_eq!(dependent.address, 0x2010);
}

fn provenance() -> Provenance {
    Provenance {
        source: FactSource::Loader,
        confidence: FactConfidence::High,
        detail: None,
    }
}

fn block(id: u32, name: &str, start: u64, size: u64) -> MemoryBlock {
    MemoryBlock {
        id: MemoryBlockId(id),
        name: name.into(),
        start,
        size,
        virtual_size: size,
        file_offset: 0,
        file_size: 0,
        permissions: Permissions {
            read: true,
            write: false,
            execute: true,
        },
        provenance: provenance(),
    }
}

fn function(id: u32, name: &str, entry: u64, size: u64, block: Option<u32>) -> FunctionRecord {
    FunctionRecord {
        id: FunctionId(id),
        name: name.into(),
        entry,
        size,
        kind: FunctionKind::Code,
        is_import: false,
        is_export: false,
        is_thunk: false,
        thunk_target: None,
        memory_block: block.map(MemoryBlockId),
        external_library: None,
        provenance: provenance(),
    }
}

fn binary_metadata() -> BinaryMetadata {
    BinaryMetadata {
        content_hash: "00".into(),
        format: "ELF".into(),
        bitness: 64,
        image_base: 0,
        entry_point: 0x1000,
        processor: None,
        endian: None,
        abi: None,
        language_id: None,
        compiler_spec_id: None,
    }
}

fn staged(blocks: Vec<MemoryBlock>, functions: Vec<FunctionRecord>) -> ProgramSnapshotBuilder {
    ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary_metadata(),
        blocks,
        functions,
        Vec::new(),
        Vec::new(),
    )
}

#[test]
fn functions_referencing_a_rejected_block_are_dropped_and_reported() {
    let outcome = staged(
        vec![
            block(0, ".text", 0x1000, 0x100),
            block(1, ".bad", BAD_SECTION, 0x2000),
        ],
        vec![
            function(0, "f_ok", 0x1000, 0x10, Some(0)),
            // Overflowing range of its own inside a valid block.
            function(1, "f_wide", 0x1080, u64::MAX, Some(0)),
            // Points into the rejected block.
            function(2, "f_dep", BAD_SECTION + 0x10, 0x10, Some(1)),
        ],
    )
    .publish_quarantining_rows();
    let SnapshotPublication::Valid(snapshot) = &outcome.publication else {
        panic!("{outcome:?}");
    };
    assert_eq!(snapshot.functions().len(), 1);
    assert_eq!(snapshot.functions()[0].name, "f_ok");
    assert_eq!(snapshot.memory_blocks().len(), 1);

    let by_name = |name: &str| -> &RejectedRow {
        outcome
            .rejected_rows
            .iter()
            .find(|row| row.name.as_deref() == Some(name))
            .unwrap_or_else(|| panic!("{name} not reported: {:?}", outcome.rejected_rows))
    };
    assert_eq!(outcome.rejected_rows.len(), 3);
    assert!(by_name("f_wide").reasons.iter().all(|reason| matches!(
        reason,
        RowRejection::Invalid {
            issue: SnapshotIntegrityIssue::AddressRangeOverflow { .. }
                | SnapshotIntegrityIssue::FunctionOutsideMemoryBlock { .. }
        }
    )));
    // Failing containment against the rejected block is a consequence of
    // that block's rejection, so only the reference is reported.
    assert_eq!(
        by_name("f_dep").reasons,
        [RowRejection::ReferencesRejectedRow {
            field: "memory_block".into(),
            target_table: SnapshotTable::MemoryBlocks,
            target_staged_id: 1,
        }]
    );
    assert_eq!(by_name(".bad").table, SnapshotTable::MemoryBlocks);
}

#[test]
fn a_function_naming_a_missing_block_is_rejected_not_repaired() {
    let outcome = staged(
        vec![block(0, ".text", 0x1000, 0x100)],
        vec![
            function(0, "f_ok", 0x1000, 0x10, Some(0)),
            function(1, "f_lost", 0x1010, 0x10, Some(7)),
        ],
    )
    .publish_quarantining_rows();
    let SnapshotPublication::Valid(snapshot) = &outcome.publication else {
        panic!("{outcome:?}");
    };
    // Not re-linked to `.text` and not stripped of its block reference.
    assert!(snapshot.functions().iter().all(|f| f.name != "f_lost"));
    assert!(matches!(
        outcome.rejected_rows.as_slice(),
        [RejectedRow {
            table: SnapshotTable::Functions,
            reasons,
            ..
        }] if matches!(reasons.as_slice(), [RowRejection::Invalid {
            issue: SnapshotIntegrityIssue::DanglingReference { target_id: 7, .. }
        }])
    ));
}

#[test]
fn snapshot_wide_faults_are_not_attributed_to_rows() {
    let outcome = staged(
        vec![
            block(0, ".text", 0x1000, 0x100),
            block(1, ".bad", BAD_SECTION, 0x2000),
        ],
        Vec::new(),
    )
    .with_schema("some-other-schema")
    .publish_quarantining_rows();
    assert!(outcome.rejected_rows.is_empty());
    let SnapshotPublication::Partial(partial) = &outcome.publication else {
        panic!("a schema mismatch must not publish: {outcome:?}");
    };
    assert!(
        partial
            .integrity_report()
            .issues
            .iter()
            .any(|issue| matches!(issue, SnapshotIntegrityIssue::SchemaMismatch { .. }))
    );
}

#[test]
fn a_valid_snapshot_publishes_identically_with_or_without_quarantine() {
    let builder = staged(
        vec![block(0, ".text", 0x1000, 0x100)],
        vec![function(0, "f", 0x1000, 0x10, Some(0))],
    );
    let plain = builder.clone().build().expect("valid");
    let outcome = builder.publish_quarantining_rows();
    assert!(outcome.is_complete());
    assert_eq!(outcome.publication.into_result().expect("valid"), plain);
}

#[test]
fn rejected_rows_serialize_with_their_reasons() {
    let outcome =
        staged(vec![block(0, ".bad", BAD_SECTION, 0x2000)], Vec::new()).publish_quarantining_rows();
    let json = serde_json::to_value(&outcome.rejected_rows).expect("serialize");
    assert_eq!(json[0]["table"], "memory_blocks");
    assert_eq!(json[0]["name"], ".bad");
    assert_eq!(json[0]["reasons"][0]["kind"], "invalid");
    assert_eq!(
        json[0]["reasons"][0]["issue"]["kind"],
        "address_range_overflow"
    );
    let back: Vec<RejectedRow> = serde_json::from_value(json).expect("deserialize");
    assert_eq!(back, outcome.rejected_rows);
}
