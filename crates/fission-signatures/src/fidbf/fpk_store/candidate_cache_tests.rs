use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

// Independent pre-cache implementations retained only for differential tests.
fn uncached_matches(
    db: &LazyFidDatabase,
    full_hash: u64,
    specific_hash: u64,
    context: FidRelationContext<'_>,
) -> Vec<FidbfMatch> {
    let Ok(records) = db.functions.records_by_key(full_hash) else {
        return Vec::new();
    };
    let mut results: Vec<FidbfMatch> = records
        .iter()
        .filter_map(|line| decode_function_for_match(line))
        .filter(|f| !f.auto_fail)
        .filter(|f| !f.force_specific || f.specific_hash == specific_hash)
        .filter_map(|f| {
            let (child_score, parent_score) = db.relation_scores(&f, context);
            if f.force_relation && child_score == 0 {
                return None;
            }
            let score = (f.code_unit_size as f32
                + if f.specific_hash == specific_hash {
                    10.0
                } else {
                    0.0
                })
                + child_score as f32
                + parent_score as f32;
            let score = score.min(100.0);
            if !f.auto_pass && score < FID_ACCEPT_THRESHOLD {
                return None;
            }
            let family = db
                .libraries
                .iter()
                .find(|l| l.key == f.library_id)
                .map(|l| l.family_name.clone())
                .unwrap_or_default();
            Some(FidbfMatch {
                name: f.name,
                library_family: family,
                score,
                specific_matched: f.specific_hash == specific_hash,
            })
        })
        .collect();
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    results
}

fn uncached_force_candidate(db: &LazyFidDatabase, full_hash: u64, specific_hash: u64) -> bool {
    let Ok(records) = db.functions.records_by_key(full_hash) else {
        return false;
    };
    records
        .iter()
        .filter_map(|line| decode_function_for_match(line))
        .any(|func| {
            func.force_relation
                && !func.auto_fail
                && (!func.force_specific || func.specific_hash == specific_hash)
                && (func.auto_pass
                    || (func.code_unit_size as f32
                        + if func.specific_hash == specific_hash {
                            10.0
                        } else {
                            0.0
                        })
                    .min(100.0)
                        >= FID_ACCEPT_THRESHOLD)
        })
}

fn row(name: &str) -> String {
    format!(
        "fffffffffffffffe|{}|1|abcdef|123|14|ffffffff00001234|1|7|domain|255|0|0|3",
        esc(name)
    )
}

#[test]
fn fid_candidate_cache_reuses_positive_fields_order_and_owned_resource_identity() {
    let cache = MatchCandidateCache::default();
    let calls = AtomicUsize::new(0);
    let lines = vec![
        row("한글|name\n\\"),
        "bad row".into(),
        row("duplicate"),
        row("duplicate"),
    ];
    let first = cache
        .get_or_load(0xabcdef, || {
            assert!(cache.0.try_lock().is_ok());
            calls.fetch_add(1, Ordering::Relaxed);
            Ok(lines.clone())
        })
        .unwrap();
    let expected: Vec<_> = lines
        .iter()
        .filter_map(|r| decode_function_for_match(r))
        .collect();
    assert_eq!(format!("{first:?}"), format!("{expected:?}"));
    let warm = cache
        .get_or_load(0xabcdef, || {
            panic!("positive archive candidates must be reused")
        })
        .unwrap();
    assert!(Arc::ptr_eq(&first, &warm));
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    let other = MatchCandidateCache::default();
    let distinct = other
        .get_or_load(0xabcdef, || Ok(vec![row("other database")]))
        .unwrap();
    assert_eq!(distinct[0].name, "other database");
    assert_ne!(first[0].name, distinct[0].name);
}

#[test]
fn fid_candidate_cache_does_not_retain_missing_malformed_or_error_results() {
    let cache = MatchCandidateCache::default();
    let calls = AtomicUsize::new(0);
    for _ in 0..3 {
        assert!(
            cache
                .get_or_load(1, || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Ok(vec![])
                })
                .unwrap()
                .is_empty()
        );
        assert!(
            cache
                .get_or_load(2, || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Ok(vec!["malformed".into()])
                })
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            cache
                .get_or_load(3, || {
                    calls.fetch_add(1, Ordering::Relaxed);
                    Err(FpkError::Corrupt("retry".into()))
                })
                .unwrap_err()
                .to_string(),
            "fpk corrupt: retry"
        );
    }
    assert_eq!(calls.load(Ordering::Relaxed), 9);
    assert!(cache.0.lock().unwrap().is_empty());
    assert_eq!(
        cache.get_or_load(3, || Ok(vec![row("recovered")])).unwrap()[0].name,
        "recovered"
    );
}

#[test]
fn fid_candidate_cache_concurrent_positive_insertion_shares_the_first_complete_value() {
    let cache = MatchCandidateCache::default();
    let barrier = std::sync::Barrier::new(8);
    let results = std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..8 {
            let cache = &cache;
            let barrier = &barrier;
            handles.push(scope.spawn(move || {
                barrier.wait();
                cache
                    .get_or_load(0xabcdef, || {
                        std::thread::sleep(std::time::Duration::from_millis(5));
                        Ok(vec![row("same"), row("ordered")])
                    })
                    .unwrap()
            }));
        }
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(results.iter().all(|r| Arc::ptr_eq(r, &results[0])));
    assert_eq!(results[0][0].name, "same");
    assert_eq!(results[0][1].name, "ordered");
    assert_eq!(cache.0.lock().unwrap().len(), 1);
}

#[test]
fn fid_candidate_cache_poisoning_keeps_uncached_lookup_available() {
    let cache = MatchCandidateCache::default();
    assert!(
        std::panic::catch_unwind(|| {
            let _guard = cache.0.lock().unwrap();
            panic!("poison test");
        })
        .is_err()
    );
    let calls = AtomicUsize::new(0);
    for _ in 0..2 {
        let result = cache
            .get_or_load(9, || {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(vec![row("still available")])
            })
            .unwrap();
        assert_eq!(result[0].name, "still available");
    }
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn fid_candidate_cache_preserves_every_flag_specific_hash_and_relation_context() {
    let child = 0xdead_beefu64;
    let parent = 0xcafe_babeu64;
    let mut functions = Vec::new();
    let mut relations = Vec::new();
    for flags in 0..16 {
        for units in [0, 4, 9, 10, 19, 20, 90, 100] {
            let key = functions.len() as i64 + 1;
            functions.push(FidbfFunction {
                key,
                library_id: if key % 3 == 0 { 99 } else { 1 },
                name: format!("name|{}\n\\한글", key % 5),
                full_hash: 0xabcdef,
                specific_hash: if key % 2 == 0 { 0x123 } else { 0x456 },
                code_unit_size: units,
                entry_point: 0xffff_ffff_0000_1234,
                has_terminator: true,
                specific_hash_additional_size: 7,
                domain_path: "preserved by full decoder".into(),
                flags: 255,
                auto_pass: flags & 1 != 0,
                auto_fail: flags & 2 != 0,
                force_specific: flags & 4 != 0,
                force_relation: flags & 8 != 0,
            });
            if key % 2 == 0 {
                relations.push(FidbfRelation {
                    key: relation_smash(key, child),
                    relation_type: FidbfRelationType::Superior,
                });
            }
            if key % 3 == 0 {
                relations.push(FidbfRelation {
                    key: relation_smash(key, parent),
                    relation_type: FidbfRelationType::Inferior,
                });
            }
        }
    }
    let database = FidbfDatabase::new(
        "test".into(),
        vec![FidbfLibrary {
            key: 1,
            family_name: "library".into(),
            version: String::new(),
            variant: String::new(),
            ghidra_version: String::new(),
            language_id: String::new(),
            language_version: 0,
            language_minor_version: 0,
            compiler_spec_id: String::new(),
        }],
        functions,
        relations,
    );
    let images = encode(&database);
    let dir = std::env::temp_dir().join(format!("fid_candidate_matrix_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, bytes) in [
        ("lib", &images.libraries),
        ("fn", &images.functions),
        ("rel", &images.relations),
        ("dom", &images.domain_paths),
    ] {
        std::fs::write(dir.join(format!("test.{name}.fpk")), bytes).unwrap();
    }
    let lazy = LazyFidDatabase::open(&dir.join("test.fidbf")).unwrap();
    let children = [(child, 4), (child, 4)];
    let parents = [(parent, 16), (parent, 16)];
    let contexts = [
        FidRelationContext::default(),
        FidRelationContext {
            children: &children,
            parents: &[],
        },
        FidRelationContext {
            children: &[],
            parents: &parents,
        },
        FidRelationContext {
            children: &children,
            parents: &parents,
        },
    ];
    for _ in 0..3 {
        for full_hash in [0xabcdef, 0, 0xdead_dead] {
            for specific in [0x123, 0x456, 0] {
                assert_eq!(
                    lazy.has_force_relation_candidate(full_hash, specific),
                    uncached_force_candidate(&lazy, full_hash, specific)
                );
                for context in contexts {
                    let expected = uncached_matches(&lazy, full_hash, specific, context);
                    let actual =
                        lazy.identify_by_hashes_with_relations(full_hash, specific, context);
                    assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
                    let mut owned = actual;
                    if let Some(first) = owned.first_mut() {
                        first.name.push('!');
                    }
                    assert_eq!(
                        format!(
                            "{:?}",
                            lazy.identify_by_hashes_with_relations(full_hash, specific, context)
                        ),
                        format!("{expected:?}")
                    );
                }
            }
        }
    }
    assert_eq!(lazy.candidates.0.lock().unwrap().len(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
