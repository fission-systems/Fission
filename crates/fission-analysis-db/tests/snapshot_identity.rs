//! Row identity across typed deltas: a removed row's ID must never be
//! reissued inside the same batch, a delta must name exactly one live row,
//! and every successful publication must answer queries exactly as a plain
//! scan of its tables does.

use fission_analysis_db::{
    BinaryMetadata, FactConfidence, FactSource, FunctionId, FunctionKind, FunctionRecord,
    MemoryBlock, MemoryBlockId, PROGRAM_SNAPSHOT_SCHEMA, Permissions, ProgramSnapshot,
    ProgramSnapshotBuilder, ProgramSnapshotDelta, Provenance, RelocationId, RelocationRecord,
    SnapshotDeltaError, SnapshotIntegrityIssue, SnapshotTable, SnapshotUpdateError, SymbolId,
    SymbolKind, SymbolRecord,
};

fn provenance() -> Provenance {
    Provenance {
        source: FactSource::Loader,
        confidence: FactConfidence::High,
        detail: None,
    }
}

fn binary() -> BinaryMetadata {
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

fn block(id: u32, name: &str, start: u64, size: u64) -> MemoryBlock {
    MemoryBlock {
        id: MemoryBlockId(id),
        name: name.into(),
        start,
        size,
        virtual_size: size,
        file_offset: start,
        file_size: size,
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

fn symbol(id: u32, name: &str, address: u64) -> SymbolRecord {
    SymbolRecord {
        id: SymbolId(id),
        name: name.into(),
        address,
        kind: SymbolKind::Data,
        size: Some(8),
        version: None,
        provenance: provenance(),
    }
}

fn relocation(id: u32, address: u64, symbol: Option<u32>, name: &str) -> RelocationRecord {
    RelocationRecord {
        id: RelocationId(id),
        address,
        relocation_type: 1,
        size: 8,
        addend: 0,
        symbol: symbol.map(SymbolId),
        symbol_name: Some(name.into()),
        provenance: provenance(),
    }
}

/// Two blocks; `f_hi` lives in the highest-ID block and `r` references the
/// highest-ID symbol, which is exactly the row a `max + 1` allocator reissues.
fn base_snapshot() -> ProgramSnapshot {
    ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary(),
        vec![
            block(0, ".text", 0x1000, 0x1000),
            block(1, ".text2", 0x3000, 0x1000),
        ],
        vec![
            function(0, "f_lo", 0x1000, 0x10, Some(0)),
            function(1, "f_hi", 0x3000, 0x10, Some(1)),
        ],
        vec![symbol(0, "a", 0x1008), symbol(1, "b", 0x3008)],
        vec![relocation(0, 0x3010, Some(1), "b")],
    )
    .build()
    .expect("base snapshot is valid")
}

#[test]
fn removing_and_readding_a_block_in_one_batch_does_not_retarget_its_functions() {
    let base = base_snapshot();
    // Same range as the removed block, so containment checks cannot tell the
    // replacement apart from the original.
    let result = base.apply_deltas([
        ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(1)),
        ProgramSnapshotDelta::AddMemoryBlock(block(0, ".replacement", 0x3000, 0x1000)),
    ]);
    if let Ok(snapshot) = &result {
        let f_hi = snapshot
            .functions()
            .iter()
            .find(|function| function.name == "f_hi")
            .expect("f_hi kept");
        let target = f_hi
            .memory_block
            .and_then(|id| snapshot.memory_blocks().get(id.0 as usize));
        panic!(
            "stale reference to the removed block was published; f_hi now points at {:?}",
            target.map(|block| block.name.as_str())
        );
    }
    let report = result
        .expect_err("checked above")
        .integrity_report()
        .cloned()
        .expect("the batch applies; publication must reject it");
    assert!(report.issues.iter().any(|issue| matches!(
        issue,
        SnapshotIntegrityIssue::DanglingReference {
            table: SnapshotTable::Functions,
            target_table: SnapshotTable::MemoryBlocks,
            ..
        }
    )));
}

#[test]
fn removing_and_readding_a_symbol_in_one_batch_does_not_retarget_relocations() {
    let base = base_snapshot();
    let result = base.apply_deltas([
        ProgramSnapshotDelta::RemoveSymbol(SymbolId(1)),
        ProgramSnapshotDelta::AddSymbol(symbol(0, "unrelated", 0x3008)),
    ]);
    if let Ok(snapshot) = &result {
        let target = snapshot.relocations()[0]
            .symbol
            .and_then(|id| snapshot.symbols().get(id.0 as usize));
        panic!(
            "stale relocation symbol reference was published; it now names {:?}",
            target.map(|symbol| symbol.name.as_str())
        );
    }
    let report = result
        .expect_err("checked above")
        .integrity_report()
        .cloned()
        .expect("the batch applies; publication must reject it");
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
fn the_builder_never_reissues_a_removed_staging_id() {
    let mut builder = base_snapshot().to_builder();
    builder
        .apply_delta(ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(1)))
        .expect("block 1 is staged");
    let replacement = builder
        .add_memory_block(block(1, ".replacement", 0x3000, 0x1000))
        .expect("an id is available");
    assert_ne!(replacement, MemoryBlockId(1), "removed id was reissued");
    // The retired id stays unknown: neither deltas nor new references bind to it.
    assert_eq!(
        builder
            .apply_delta(ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(1)))
            .map(|_| ())
            .expect_err("retired id"),
        SnapshotDeltaError::UnknownTarget {
            table: SnapshotTable::MemoryBlocks,
            id: 1
        }
    );
    // A function added against the replacement's returned id resolves to it.
    builder
        .add_function(function(0, "f_new", 0x3020, 0x10, Some(replacement.0)))
        .expect("replacement is live");
}

#[test]
fn staging_id_exhaustion_is_reported_instead_of_reissuing_an_id() {
    let mut builder = ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary(),
        vec![block(u32::MAX - 2, ".seed", 0x1000, 0x10)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let last = builder
        .add_memory_block(block(0, ".x", 0x2000, 0x10))
        .expect("one id left below the reserved sentinel");
    assert_eq!(last, MemoryBlockId(u32::MAX - 1));
    let before = builder.clone();
    assert_eq!(
        builder.add_memory_block(block(0, ".y", 0x3000, 0x10)),
        Err(SnapshotDeltaError::IdSpaceExhausted {
            table: SnapshotTable::MemoryBlocks
        })
    );
    assert_eq!(
        builder, before,
        "a refused delta must leave the builder unchanged"
    );
    // u32::MAX is the canonicalize sentinel for unresolved references and is
    // never issued.
    assert!(
        builder
            .memory_blocks()
            .iter()
            .all(|row| row.id.0 != u32::MAX)
    );
}

#[test]
fn a_refused_delta_in_a_batch_publishes_nothing_and_names_its_position() {
    let base = base_snapshot();
    let error = base
        .apply_deltas([
            ProgramSnapshotDelta::RemoveFunction(FunctionId(0)),
            ProgramSnapshotDelta::RemoveFunction(FunctionId(0)),
        ])
        .expect_err("the second delta names a removed row");
    assert_eq!(
        error,
        SnapshotUpdateError::RejectedDelta {
            index: 1,
            error: SnapshotDeltaError::UnknownTarget {
                table: SnapshotTable::Functions,
                id: 0
            }
        }
    );
}

#[test]
fn deltas_naming_a_duplicated_staged_id_are_ambiguous() {
    let mut builder = ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary(),
        vec![block(3, ".a", 0x1000, 0x100), block(3, ".b", 0x2000, 0x100)],
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let expected = SnapshotDeltaError::AmbiguousTarget {
        table: SnapshotTable::MemoryBlocks,
        id: 3,
        rows: 2,
    };
    assert_eq!(
        builder
            .apply_delta(ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(3)))
            .map(|_| ())
            .unwrap_err(),
        expected
    );
    assert_eq!(
        builder
            .add_function(function(0, "f", 0x1000, 0x10, Some(3)))
            .unwrap_err(),
        expected
    );
}

#[test]
fn deltas_naming_a_missing_row_are_rejected() {
    let base = base_snapshot();
    let missing = [
        ProgramSnapshotDelta::RemoveFunction(FunctionId(9)),
        ProgramSnapshotDelta::UpdateFunctionRange {
            id: FunctionId(9),
            entry: 0x1000,
            size: 4,
        },
        ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(9)),
        ProgramSnapshotDelta::UpdateMemoryBlockRange {
            id: MemoryBlockId(9),
            start: 0x1000,
            size: 4,
        },
        ProgramSnapshotDelta::RemoveSymbol(SymbolId(9)),
        ProgramSnapshotDelta::RemoveRelocation(RelocationId(9)),
    ];
    for delta in missing {
        let result = base.apply_delta(delta.clone());
        assert!(
            matches!(
                result,
                Err(SnapshotUpdateError::RejectedDelta {
                    index: 0,
                    error: SnapshotDeltaError::UnknownTarget { id: 9, .. }
                })
            ),
            "{delta:?} names no row but was not refused: {result:?}"
        );
    }
}

#[test]
fn an_id_from_a_previous_publication_is_not_silently_ignored() {
    let first = base_snapshot();
    let second = first
        .apply_delta(ProgramSnapshotDelta::RemoveFunction(FunctionId(0)))
        .expect("removing an unreferenced function is valid");
    assert_eq!(second.functions().len(), 1);
    // FunctionId(1) was issued by `first`; `second` renumbered its rows.
    assert!(
        second
            .apply_delta(ProgramSnapshotDelta::RemoveFunction(FunctionId(1)))
            .is_err(),
        "a stale ID must be rejected, not treated as an empty delta"
    );
}

#[test]
fn duplicate_staged_ids_with_inbound_references_stay_invalid_after_canonicalize() {
    let builder = ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary(),
        vec![
            block(4, ".a", 0x1000, 0x1000),
            block(4, ".b", 0x3000, 0x1000),
        ],
        vec![function(0, "f", 0x3000, 0x10, Some(4))],
        vec![symbol(7, "x", 0x1008), symbol(7, "y", 0x3008)],
        vec![relocation(0, 0x3010, Some(7), "y")],
    );
    let report = builder
        .canonicalize()
        .build()
        .expect_err("an ambiguous foreign key must not resolve to either row");
    assert!(report.issues.len() >= 2, "{:?}", report.issues);
}

#[test]
fn an_added_row_referencing_a_missing_row_is_rejected() {
    let base = base_snapshot();
    assert!(matches!(
        base.apply_delta(ProgramSnapshotDelta::AddFunction(function(
            0,
            "orphan",
            0x1020,
            0x10,
            Some(42)
        ))),
        Err(SnapshotUpdateError::RejectedDelta {
            error: SnapshotDeltaError::DanglingReference {
                target_table: SnapshotTable::MemoryBlocks,
                target_id: 42,
                ..
            },
            ..
        })
    ));
    assert!(matches!(
        base.apply_delta(ProgramSnapshotDelta::AddRelocation(relocation(
            0,
            0x1030,
            Some(42),
            "nowhere"
        ))),
        Err(SnapshotUpdateError::RejectedDelta {
            error: SnapshotDeltaError::DanglingReference {
                target_table: SnapshotTable::Symbols,
                target_id: 42,
                ..
            },
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Randomised differential: snapshots built from random rows and random delta
// batches must (a) keep every surviving foreign key on the row it named
// before the batch, and (b) answer every query exactly like a table scan.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*: deterministic, dependency-free.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }
}

fn random_snapshot(rng: &mut Rng) -> ProgramSnapshot {
    let block_count = 1 + rng.below(4) as usize;
    let blocks: Vec<MemoryBlock> = (0..block_count)
        .map(|index| {
            // Blocks may overlap: starts on a coarse grid, random sizes.
            let start = 0x1000 * (1 + rng.below(6));
            let size = 0x100 * (1 + rng.below(24));
            block(index as u32, &format!(".b{index}"), start, size)
        })
        .collect();
    let functions: Vec<FunctionRecord> = (0..rng.below(24) as usize)
        .map(|index| {
            let entry = 0x1000 + 0x10 * rng.below(0x600);
            let size = match rng.below(5) {
                0 => 0,
                1 => 0x10 * (1 + rng.below(64)),
                _ => 0x10 * (1 + rng.below(4)),
            };
            let owner = blocks.iter().position(|block| {
                entry >= block.start
                    && entry
                        .checked_add(size.max(1))
                        .is_some_and(|end| end <= block.start + block.size)
            });
            function(
                index as u32,
                &format!("f{index}"),
                entry,
                size,
                owner.map(|index| index as u32),
            )
        })
        .collect();
    let symbols: Vec<SymbolRecord> = (0..rng.below(12) as usize)
        .map(|index| {
            symbol(
                index as u32,
                &format!("s{index}"),
                0x1000 + 8 * rng.below(0xC00),
            )
        })
        .collect();
    let relocations: Vec<RelocationRecord> = (0..rng.below(12) as usize)
        .map(|index| {
            let target = (!symbols.is_empty() && rng.below(3) != 0)
                .then(|| rng.below(symbols.len() as u64) as u32);
            relocation(index as u32, 0x1000 + 8 * rng.below(0xC00), target, "r")
        })
        .collect();
    ProgramSnapshotBuilder::from_raw_parts(
        PROGRAM_SNAPSHOT_SCHEMA.into(),
        binary(),
        blocks,
        functions,
        symbols,
        relocations,
    )
    .canonicalize()
    .build()
    .expect("random rows canonicalize into a valid snapshot")
}

fn random_delta(
    rng: &mut Rng,
    snapshot: &ProgramSnapshot,
    serial: usize,
) -> Vec<ProgramSnapshotDelta> {
    let pick = |rng: &mut Rng, len: usize| rng.below(len as u64 + 1) as u32;
    // Replace the highest-ID row with a look-alike in one batch: the shape in
    // which a `max + 1` allocator hands the removed ID to the new row.
    if rng.below(4) == 0 {
        if let Some(last) = snapshot.memory_blocks().last() {
            return vec![
                ProgramSnapshotDelta::RemoveMemoryBlock(last.id),
                ProgramSnapshotDelta::AddMemoryBlock(block(
                    0,
                    &format!(".n{serial}"),
                    last.start,
                    last.size,
                )),
            ];
        }
    }
    if rng.below(4) == 0 {
        if let Some(last) = snapshot.symbols().last() {
            return vec![
                ProgramSnapshotDelta::RemoveSymbol(last.id),
                ProgramSnapshotDelta::AddSymbol(symbol(0, &format!("n{serial}"), last.address)),
            ];
        }
    }
    vec![match rng.below(9) {
        0 => ProgramSnapshotDelta::RemoveMemoryBlock(MemoryBlockId(pick(
            rng,
            snapshot.memory_blocks().len(),
        ))),
        1 => ProgramSnapshotDelta::AddMemoryBlock(block(
            0,
            &format!(".n{serial}"),
            0x1000 * (1 + rng.below(6)),
            0x100 * (1 + rng.below(24)),
        )),
        2 => ProgramSnapshotDelta::RemoveSymbol(SymbolId(pick(rng, snapshot.symbols().len()))),
        3 => ProgramSnapshotDelta::AddSymbol(symbol(
            0,
            &format!("n{serial}"),
            0x1000 + 8 * rng.below(0xC00),
        )),
        4 => {
            ProgramSnapshotDelta::RemoveFunction(FunctionId(pick(rng, snapshot.functions().len())))
        }
        5 => ProgramSnapshotDelta::UpdateFunctionRange {
            id: FunctionId(pick(rng, snapshot.functions().len())),
            entry: 0x1000 + 0x10 * rng.below(0x600),
            size: 0x10 * rng.below(8),
        },
        6 => ProgramSnapshotDelta::UpdateMemoryBlockRange {
            id: MemoryBlockId(pick(rng, snapshot.memory_blocks().len())),
            start: 0x1000 * (1 + rng.below(6)),
            size: 0x100 * (1 + rng.below(24)),
        },
        7 => ProgramSnapshotDelta::RemoveRelocation(RelocationId(pick(
            rng,
            snapshot.relocations().len(),
        ))),
        _ => ProgramSnapshotDelta::AddFunction(function(
            0,
            &format!("n{serial}"),
            0x1000 + 0x10 * rng.below(0x600),
            0x10 * rng.below(4),
            None,
        )),
    }]
}

/// Name (unique per generated table) of the block a function's foreign key
/// resolves to.
fn block_name_of(snapshot: &ProgramSnapshot, function: &FunctionRecord) -> Option<String> {
    let id = function.memory_block?;
    Some(snapshot.memory_blocks().get(id.0 as usize)?.name.clone())
}

/// Name of the symbol a relocation's foreign key resolves to.
fn symbol_name_of(snapshot: &ProgramSnapshot, relocation: &RelocationRecord) -> Option<String> {
    let id = relocation.symbol?;
    Some(snapshot.symbols().get(id.0 as usize)?.name.clone())
}

/// Every foreign key in `snapshot`, keyed by the referencing row's identity
/// (function name; relocation address + resolved symbol), resolved to the
/// referenced row's name.
fn foreign_keys(
    snapshot: &ProgramSnapshot,
) -> (Vec<(String, Option<String>)>, Vec<Option<String>>) {
    (
        snapshot
            .functions()
            .iter()
            .map(|f| (f.name.clone(), block_name_of(snapshot, f)))
            .collect(),
        snapshot
            .relocations()
            .iter()
            .map(|r| symbol_name_of(snapshot, r))
            .collect(),
    )
}

fn assert_queries_match_scan(snapshot: &ProgramSnapshot) {
    assert!(
        snapshot.integrity_report().is_valid(),
        "{:?}",
        snapshot.integrity_report()
    );
    for address in (0x0ff8..0x8000u64).step_by(8) {
        let expected_function = snapshot
            .functions()
            .iter()
            .filter(|f| f.size > 0 && f.entry <= address && address - f.entry < f.size)
            .min_by_key(|f| (f.size, f.entry, f.id))
            .or_else(|| snapshot.functions().iter().find(|f| f.entry == address));
        assert_eq!(
            snapshot.function_containing(address),
            expected_function,
            "0x{address:x}"
        );
        let expected_block = snapshot
            .memory_blocks()
            .iter()
            .filter(|b| b.start <= address && address - b.start < b.size)
            .min_by_key(|b| (b.start, b.id));
        assert_eq!(
            snapshot.memory_block_containing(address),
            expected_block,
            "0x{address:x}"
        );
        assert_eq!(
            snapshot.function_at(address),
            snapshot.functions().iter().find(|f| f.entry == address)
        );
        assert!(
            snapshot
                .symbols_at(address)
                .eq(snapshot.symbols().iter().filter(|s| s.address == address))
        );
        assert!(
            snapshot.relocations_at(address).eq(snapshot
                .relocations()
                .iter()
                .filter(|r| r.address == address))
        );
    }
}

#[test]
fn random_delta_batches_preserve_identity_and_query_consistency() {
    let mut published = 0;
    for seed in 1..=300u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let mut snapshot = random_snapshot(&mut rng);
        assert_queries_match_scan(&snapshot);
        for round in 0..4 {
            let batch: Vec<_> = (0..1 + rng.below(4) as usize)
                .flat_map(|op| random_delta(&mut rng, &snapshot, round * 8 + op))
                .collect();
            let removes_relocations = batch
                .iter()
                .any(|delta| matches!(delta, ProgramSnapshotDelta::RemoveRelocation(_)));
            let (functions_before, relocations_before) = foreign_keys(&snapshot);
            let Ok(next) = snapshot.apply_deltas(batch.clone()) else {
                continue;
            };
            published += 1;
            assert_queries_match_scan(&next);
            // A successful batch keeps every surviving foreign key on the row it
            // named before the batch.
            let (functions_after, relocations_after) = foreign_keys(&next);
            for (name, target) in &functions_before {
                if let Some((_, after)) = functions_after.iter().find(|(n, _)| n == name) {
                    assert_eq!(
                        after, target,
                        "seed {seed} batch {batch:?} retargeted {name}"
                    );
                }
            }
            if !removes_relocations {
                // Relocations carry no unique name; without removals the
                // multiset of resolved targets must be unchanged.
                let mut before = relocations_before.clone();
                let mut after = relocations_after.clone();
                before.sort();
                after.sort();
                assert_eq!(
                    after, before,
                    "seed {seed} batch {batch:?} retargeted a relocation"
                );
            }
            let json = serde_json::to_string(&next).expect("serialize");
            let back: ProgramSnapshot = serde_json::from_str(&json).expect("round trip");
            assert_eq!(back, next);
            snapshot = next;
        }
    }
    assert!(published > 100, "only {published} batches published");
}
