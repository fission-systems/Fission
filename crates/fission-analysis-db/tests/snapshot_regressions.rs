use fission_analysis_db::{
    FactSource, MemoryBlockId, ProgramSnapshot, SnapshotIntegrityIssue, SnapshotTable, SymbolId,
    SymbolKind,
};
use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, RelocationEntry};
use fission_loader::{FunctionInfo, LoaderSymbolInfo, LoaderSymbolKind, SectionInfo};

fn fixture() -> fission_loader::LoadedBinary {
    LoadedBinaryBuilder::new("fixture.bin".into(), DataBuffer::Heap(vec![0; 16]))
        .format("PE")
        .image_base(0x1000)
        .entry_point(0x1000)
        .is_64bit(true)
        .add_function(FunctionInfo {
            name: "first".into(),
            address: 0x1000,
            size: 16,
            is_export: true,
            origin: Some("export_table".into()),
            kind: Some("export".into()),
            ..FunctionInfo::default()
        })
        .add_function(FunctionInfo {
            name: "second".into(),
            address: 0x1010,
            size: 8,
            origin: Some("static_discovery".into()),
            ..FunctionInfo::default()
        })
        .add_section(SectionInfo {
            name: ".text".into(),
            virtual_address: 0x1000,
            virtual_size: 0x100,
            file_offset: 0x200,
            file_size: 0x120,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
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
fn relocation_only_symbols_are_preserved_and_linked() {
    let mut binary = fixture();
    binary.inner_mut().global_symbols.remove(&0x1020);
    binary
        .inner_mut()
        .relocation_symbols
        .insert(0x1020, "global_value".into());

    let snapshot = ProgramSnapshot::from_loaded_binary(&binary);
    let target = snapshot
        .symbols
        .iter()
        .find(|symbol| {
            symbol.address == 0x1020
                && symbol.name == "global_value"
                && symbol.kind == SymbolKind::RelocationTarget
        })
        .expect("relocation-only target should enter the canonical symbol table");

    assert_eq!(target.size, None);
    assert_eq!(target.provenance.source, FactSource::RelocationTable);
    assert_eq!(snapshot.relocations[0].symbol, Some(target.id));
}

#[test]
fn code_labels_do_not_inherit_coincident_data_sizes() {
    let mut binary = LoadedBinaryBuilder::new("labels.exe".into(), DataBuffer::Heap(vec![0; 16]))
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
        .build()
        .expect("fixture should build");
    binary.inner_mut().global_symbol_sizes.insert(0x1020, 0x40);

    let snapshot = ProgramSnapshot::from_loaded_binary(&binary);
    let label = snapshot
        .symbols
        .iter()
        .find(|symbol| symbol.address == 0x1020 && symbol.kind == SymbolKind::Label)
        .expect("loader code label should reach the canonical symbol table");

    assert_eq!(label.size, None);
}

#[test]
fn integrity_rejects_functions_that_overrun_their_memory_block() {
    let mut snapshot = ProgramSnapshot::from_loaded_binary(&fixture());
    let (function_id, block_id) = {
        let function = snapshot
            .functions
            .iter_mut()
            .find(|function| function.entry == 0x1010)
            .expect("fixture function should exist");
        function.size = 0x200;
        (
            function.id,
            function
                .memory_block
                .expect("fixture function should belong to a memory block"),
        )
    };

    let report = snapshot.integrity_report();
    assert!(report.issues.iter().any(|issue| matches!(
        issue,
        SnapshotIntegrityIssue::FunctionOutsideMemoryBlock {
            function,
            memory_block,
        } if *function == function_id && *memory_block == block_id
    )));
    assert_eq!(block_id, MemoryBlockId(0));
}

#[test]
fn relocation_fk_validation_uses_symbol_identity_not_vector_position() {
    let mut snapshot = ProgramSnapshot::from_loaded_binary(&fixture());
    let original_id = snapshot.relocations[0]
        .symbol
        .expect("fixture relocation should resolve a symbol");
    let sparse_id = SymbolId(42);

    snapshot
        .symbols
        .iter_mut()
        .find(|symbol| symbol.id == original_id)
        .expect("referenced symbol should exist")
        .id = sparse_id;
    snapshot.relocations[0].symbol = Some(sparse_id);

    let report = snapshot.integrity_report();
    assert!(report.issues.iter().any(|issue| matches!(
        issue,
        SnapshotIntegrityIssue::InvalidTableId {
            table: SnapshotTable::Symbols,
            actual: 42,
            ..
        }
    )));
    assert!(!report.issues.iter().any(|issue| matches!(
        issue,
        SnapshotIntegrityIssue::DanglingReference {
            table: SnapshotTable::Relocations,
            field,
            target_table: SnapshotTable::Symbols,
            target_id: 42,
            ..
        } if field == "symbol"
    )));
}
