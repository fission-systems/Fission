use std::sync::Arc;

use fission_analysis_db::{
    CheckedProgramView, IdentityField, ProgramJoinError, ProgramJoinMismatch, ProgramSnapshot,
    ProgramSnapshotBuilder, ProgramViewIdentity, SnapshotPublication,
};
use fission_loader::loader::types::{
    ArchitectureDescriptor, BinaryLoadSpec, DwarfFrameBase, DwarfFunctionInfo, InferredTypeInfo,
    SectionInfo, UserSignature,
};
use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder};
use fission_static::analysis::decomp::facts::FactStore;

fn build_elf64_fixture(
    path: &str,
    bytes: Vec<u8>,
    image_base: u64,
    fn_name: &str,
    dwarf_name: Option<&str>,
    inferred_type_name: Option<&str>,
) -> LoadedBinary {
    let mut builder = LoadedBinaryBuilder::new(path.to_string(), DataBuffer::Heap(bytes))
        .format("ELF")
        .is_64bit(true)
        .image_base(image_base)
        .entry_point(image_base + 0x1000)
        .arch_spec("x86:LE:64:default")
        .architecture(ArchitectureDescriptor::new(
            "x86",
            "little",
            64,
            "default",
            None,
            "EM_X86_64",
        ))
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: image_base + 0x1000,
            virtual_size: 0x200,
            file_offset: 0,
            file_size: 0x200,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_function(FunctionInfo {
            name: fn_name.to_string(),
            address: image_base + 0x1000,
            size: 0x20,
            ..Default::default()
        });

    if let Some(ty_name) = inferred_type_name {
        builder = builder.add_inferred_types([InferredTypeInfo {
            name: ty_name.to_string(),
            mangled_name: String::new(),
            kind: "struct".to_string(),
            source: "dwarf".to_string(),
            fields: Vec::new(),
            size: 8,
            metadata_address: image_base + 0x1000,
        }]);
    }

    let mut binary = builder.build().expect("build fixture binary");
    if let Some(dname) = dwarf_name {
        Arc::make_mut(&mut binary.dwarf_functions).insert(
            image_base + 0x1000,
            DwarfFunctionInfo {
                address: image_base + 0x1000,
                name: dname.to_string(),
                return_type: Some("int".to_string()),
                params: Vec::new(),
                variadic: false,
                local_vars: Vec::new(),
                frame_base: DwarfFrameBase::Unknown,
                size: 0x20,
            },
        );
    }
    binary
}

#[test]
fn rejects_cross_program_overlapping_va_join_and_never_merges_facts() {
    let va = 0x401000_u64;
    let bin_a = build_elf64_fixture(
        "/tmp/bin_a.elf",
        vec![0x90; 16],
        0x400000,
        "alpha_handler",
        Some("alpha_dwarf_fn"),
        Some("AlphaCtx"),
    );
    let bin_b = build_elf64_fixture(
        "/tmp/bin_b.elf",
        vec![0xc3; 16],
        0x400000,
        "beta_handler",
        Some("beta_dwarf_fn"),
        Some("BetaCtx"),
    );

    let snap_a = Arc::new(ProgramSnapshot::try_from_loaded_binary(&bin_a).expect("snap_a valid"));

    // Checked join MUST reject joining bin_b with snap_a despite identical image_base & VA 0x401000.
    let err = FactStore::try_from_program(&bin_b, Arc::clone(&snap_a))
        .expect_err("distinct binary hashes must reject join");
    let ProgramJoinError::IdentityMismatch(ref mismatches) = err else {
        panic!("expected IdentityMismatch, got {err:?}");
    };
    assert!(
        mismatches
            .iter()
            .any(|m| matches!(m, ProgramJoinMismatch::ContentHashMismatch { .. }))
    );

    // Compatibility wrapper `from_program` MUST fail closed with empty evidence instead of merging!
    let failed_store = FactStore::from_program(&bin_b, snap_a);
    assert!(!failed_store.is_join_valid());
    assert!(failed_store.join_error().is_some());
    assert_eq!(failed_store.resolved_name(va), None);
    assert!(failed_store.dwarf_function(va).is_none());
    assert!(failed_store.merged_inferred_types(va).is_empty());
}

#[test]
fn rejects_same_bytes_at_different_image_base_or_load_spec() {
    let same_bytes = vec![0x55, 0x48, 0x89, 0xe5, 0xc3];

    let bin_low = build_elf64_fixture(
        "same.elf",
        same_bytes.clone(),
        0x400000,
        "entry_fn",
        None,
        Some("LowType"),
    );
    let snap_low =
        Arc::new(ProgramSnapshot::try_from_loaded_binary(&bin_low).expect("snap_low valid"));

    // 1. Identical content hash, shifted image base (ASLR / relocatable rebase).
    let bin_high = build_elf64_fixture(
        "same.elf",
        same_bytes.clone(),
        0x1000_0000,
        "entry_fn",
        None,
        Some("HighType"),
    );
    let err_base = FactStore::try_from_program(&bin_high, Arc::clone(&snap_low))
        .expect_err("rebased image must reject stale snapshot");
    let ProgramJoinError::IdentityMismatch(mismatches) = err_base else {
        panic!("expected IdentityMismatch, got {err_base:?}");
    };
    assert!(
        mismatches
            .iter()
            .any(|m| matches!(m, ProgramJoinMismatch::ImageBaseMismatch { .. }))
    );
    assert!(
        mismatches
            .iter()
            .any(|m| matches!(m, ProgramJoinMismatch::LoadMappingMismatch { .. }))
    );

    // 2. Identical content hash and image base, different load spec / compiler spec.
    let bin_win = LoadedBinaryBuilder::new("same.elf".to_string(), DataBuffer::Heap(same_bytes))
        .format("ELF")
        .is_64bit(true)
        .image_base(0x400000)
        .entry_point(0x401000)
        .load_spec(BinaryLoadSpec::new(
            "ELF",
            0x400000,
            "x86:LE:64:default",
            "windows",
            "test",
        ))
        .architecture(ArchitectureDescriptor::new(
            "x86",
            "little",
            64,
            "default",
            None,
            "EM_X86_64",
        ))
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x401000,
            virtual_size: 0x200,
            file_offset: 0,
            file_size: 0x200,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_function(FunctionInfo {
            name: "entry_fn".to_string(),
            address: 0x401000,
            size: 0x20,
            ..Default::default()
        })
        .build()
        .expect("bin_win");
    let err_spec = FactStore::try_from_program(&bin_win, snap_low)
        .expect_err("different compiler spec must reject join");
    let ProgramJoinError::IdentityMismatch(spec_mismatches) = err_spec else {
        panic!("expected IdentityMismatch, got {err_spec:?}");
    };
    assert!(spec_mismatches.iter().any(|m| matches!(
        m,
        ProgramJoinMismatch::LoadSpecMismatch {
            field: "compiler_spec_id",
            ..
        }
    )));
}

#[test]
fn rejects_updated_project_decisions_and_allows_moved_matching_binary() {
    let mut binary = build_elf64_fixture(
        "/original/dir/target.elf",
        vec![0x48, 0x83, 0xc0, 0x01, 0xc3],
        0x400000,
        "target_fn",
        None,
        None,
    );
    let snap_v1 = Arc::new(ProgramSnapshot::try_from_loaded_binary(&binary).expect("snap_v1"));

    // Updating a project decision invalidates joining against `snap_v1`.
    binary.user_signatures.insert(
        0x401000,
        UserSignature {
            return_type: Some("long".to_string()),
            param_types: vec!["int".to_string()],
            param_names: vec!["count".to_string()],
        },
    );
    let err_decision = FactStore::try_from_program(&binary, Arc::clone(&snap_v1))
        .expect_err("updated project decisions must reject prior snapshot");
    let ProgramJoinError::IdentityMismatch(mismatches) = err_decision else {
        panic!("expected IdentityMismatch, got {err_decision:?}");
    };
    assert!(mismatches.iter().any(|m| matches!(
        m,
        ProgramJoinMismatch::ProjectDecisionRevisionMismatch { .. }
    )));

    // Republishing snapshot after project update succeeds and binds the decision revision.
    let snap_v2 = Arc::new(ProgramSnapshot::try_from_loaded_binary(&binary).expect("snap_v2"));
    assert_ne!(
        snap_v1.identity().project_decision_revision,
        snap_v2.identity().project_decision_revision
    );
    let ok_v2 = FactStore::try_from_program(&binary, Arc::clone(&snap_v2))
        .expect("matching updated snapshot succeeds");
    assert!(ok_v2.is_join_valid());
    assert_eq!(ok_v2.resolved_name(0x401000), Some("target_fn"));

    // Moving the binary to a different path with identical identity & decisions still joins cleanly!
    let mut moved_binary = build_elf64_fixture(
        "/relocated/copy/renamed.elf",
        vec![0x48, 0x83, 0xc0, 0x01, 0xc3],
        0x400000,
        "target_fn",
        None,
        None,
    );
    moved_binary.user_signatures.insert(
        0x401000,
        UserSignature {
            return_type: Some("long".to_string()),
            param_types: vec!["int".to_string()],
            param_names: vec!["count".to_string()],
        },
    );
    let moved_store = FactStore::try_from_program(&moved_binary, snap_v2)
        .expect("matching binary at different path should still share canonical snapshot");
    assert_eq!(moved_store.resolved_name(0x401000), Some("target_fn"));
}

#[test]
fn rejects_invalid_partial_and_empty_snapshots_and_shares_valid_arc() {
    let valid_bin = build_elf64_fixture(
        "valid.elf",
        vec![0x90, 0xc3],
        0x400000,
        "valid_fn",
        Some("valid_dwarf"),
        None,
    );

    // 1. Empty snapshot has Unavailable required identity fields -> MUST reject even against real binary.
    let empty_snap = Arc::new(ProgramSnapshot::empty());
    let empty_err = FactStore::try_from_program(&valid_bin, empty_snap)
        .expect_err("empty snapshot must never match real binary");
    assert!(matches!(empty_err, ProgramJoinError::IdentityMismatch(_)));

    // 2. Partial publication MUST fail CheckedProgramView/FactStore publication join.
    let mut bad_meta = ProgramSnapshotBuilder::from_loaded_binary(&valid_bin)
        .build()
        .expect("base")
        .binary()
        .clone();
    bad_meta.content_hash = String::new();
    let mut partial_builder = ProgramSnapshotBuilder::from_loaded_binary(&valid_bin);
    partial_builder
        .apply_delta(fission_analysis_db::ProgramSnapshotDelta::ReplaceBinaryMetadata(bad_meta))
        .expect("delta");
    let pub_partial = partial_builder.publish();
    assert!(matches!(pub_partial, SnapshotPublication::Partial(_)));
    let partial_err = FactStore::try_from_publication(&valid_bin, pub_partial)
        .expect_err("partial snapshot publication must be rejected");
    assert!(matches!(partial_err, ProgramJoinError::InvalidSnapshot(_)));

    // 3. Valid CheckedProgramView reuse shares exact Arc<ProgramSnapshot> cleanly.
    let snap = Arc::new(ProgramSnapshot::try_from_loaded_binary(&valid_bin).expect("valid"));
    assert!(matches!(
        ProgramViewIdentity::from_loaded_binary(&valid_bin).analysis_revision,
        IdentityField::Unavailable
    ));
    assert!(snap.identity().analysis_revision.is_available());

    let view = CheckedProgramView::new(&valid_bin, snap).expect("checked view");
    let store_1 = FactStore::from_checked_view(&view, false);
    let store_2 = FactStore::from_checked_view(&view, true);
    assert!(Arc::ptr_eq(&store_1.program_arc(), &store_2.program_arc()));
    assert!(store_1.is_join_valid());
    assert_eq!(store_1.resolved_name(0x401000), Some("valid_fn"));
    assert_eq!(
        store_1.dwarf_function(0x401000).map(|d| d.name.as_str()),
        Some("valid_dwarf")
    );
}
