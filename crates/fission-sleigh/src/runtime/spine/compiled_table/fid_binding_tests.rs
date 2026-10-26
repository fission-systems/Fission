use super::*;

fn frontend() -> Arc<CompiledFrontend> {
    Arc::new(crate::compiler::compile_x86_64_frontend().unwrap())
}
fn reset() {
    CACHE.with(|cache| *cache.borrow_mut() = Cache::default());
}
fn hits() -> usize {
    CACHE.with(|cache| cache.borrow().hits)
}
fn rendered(value: Result<Binding>) -> Result<String, String> {
    value
        .map(|state| format!("{:?}", &*state))
        .map_err(|e| format!("{e:#}"))
}
fn native(compiled: &CompiledFrontend, bytes: &[u8], address: u64) -> Result<String, String> {
    decode_instruction_raw_state(compiled, bytes, address)
        .map(|state| format!("{state:?}"))
        .map_err(|e| format!("{e:#}"))
}

#[test]
fn fid_binding_reuse_is_exact_across_addresses_and_preserves_clear() {
    reset();
    let compiled = frontend();
    let bytes = [0x48, 0x89, 0xc8];
    assert!(matches!(
        get_or_decode(&compiled, &bytes, 0x1000, true).unwrap(),
        Binding::Owned(_)
    ));
    let _scope = FrontendScope::enter(&compiled);
    let first = get_or_decode(&compiled, &bytes, 0x1000, true).unwrap();
    assert!(matches!(&first, Binding::Shared(_)));
    super::super::BIND_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            (0, Default::default(), Default::default()),
            Err("stale test entry".into()),
        );
    });
    for address in [0x2000, 0x8000_0000_0000_0100, u64::MAX - 64] {
        let reused = get_or_decode(&compiled, &bytes, address, true).unwrap();
        assert!(super::super::BIND_CACHE.with(|cache| cache.borrow().is_empty()));
        assert_eq!(
            format!("{:?}", &*reused),
            native(&compiled, &bytes, address).unwrap()
        );
        let (Binding::Shared(a), Binding::Shared(b)) = (&first, &reused) else {
            panic!("must share proven state")
        };
        assert!(Arc::ptr_eq(a, b));
    }
    assert!(hits() >= 3);
    assert!(super::super::BIND_CACHE.with(|cache| cache.borrow().is_empty()));
    let before = hits();
    assert!(matches!(
        get_or_decode(&compiled, &bytes, 0x3000, false).unwrap(),
        Binding::Owned(_)
    ));
    assert_eq!(hits(), before);
}

#[test]
fn fid_binding_reuse_rejects_pc_reads_errors_and_address_overflow() {
    reset();
    let compiled = frontend();
    let _scope = FrontendScope::enter(&compiled);
    for bytes in [
        &[0xe8, 0x10, 0, 0, 0][..],
        &[0x48, 0x8b, 0x05, 0x10, 0, 0, 0][..],
    ] {
        for address in [0x1000, 0x4000, u64::MAX - 1] {
            assert_eq!(
                rendered(get_or_decode(&compiled, bytes, address, true)),
                native(&compiled, bytes, address)
            );
        }
    }
    assert_eq!(hits(), 0, "PC-dependent states must not be shared");
    for bytes in [&[][..], &[0x0f][..]] {
        for _ in 0..2 {
            assert_eq!(
                rendered(get_or_decode(&compiled, bytes, 0x1000, true)),
                native(&compiled, bytes, 0x1000)
            );
        }
    }
    let bytes = [0x48, 0x89, 0xc8];
    let (_, witness) =
        binding_address_use::capture(|| decode_instruction_raw_state(&compiled, &bytes, 0x1000));
    assert!(!witness.pc_value_read && witness.max_checked_offset > 0);
    let _ = get_or_decode(&compiled, &bytes, 0x1000, true).unwrap();
    assert_eq!(
        rendered(get_or_decode(&compiled, &bytes, u64::MAX, true)),
        native(&compiled, &bytes, u64::MAX)
    );
    assert!(native(&compiled, &bytes, u64::MAX).is_err());
}

#[test]
fn fid_binding_owner_cow_nested_unwind_and_threads_are_isolated() {
    reset();
    let mut compiled = frontend();
    let bytes = [0x57];
    let weak = Arc::downgrade(&compiled);
    {
        let _scope = FrontendScope::enter(&compiled);
        get_or_decode(&compiled, &bytes, 0x1000, true).unwrap();
        let other = Arc::new((*compiled).clone());
        let _ = std::panic::catch_unwind(|| {
            let _nested = FrontendScope::enter(&other);
            assert_eq!(
                rendered(get_or_decode(&other, &bytes, 0x2000, true)),
                native(&other, &bytes, 0x2000)
            );
            panic!("scope restoration");
        });
        assert_eq!(
            rendered(get_or_decode(&compiled, &bytes, 0x2000, true)),
            native(&compiled, &bytes, 0x2000)
        );
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    assert!(ACTIVE.with(|a| a.borrow().is_none()));
                    let _scope = FrontendScope::enter(&compiled);
                    assert_eq!(
                        rendered(get_or_decode(&compiled, &bytes, 0x3000, true)),
                        native(&compiled, &bytes, 0x3000)
                    );
                })
                .join()
                .unwrap();
        });
    }
    let old = Arc::as_ptr(&compiled);
    Arc::make_mut(&mut compiled)
        .default_context_known_mask
        .set_word(0, 1, 1)
        .unwrap();
    assert_ne!(old, Arc::as_ptr(&compiled));
    {
        let _scope = FrontendScope::enter(&compiled);
        assert_eq!(
            rendered(get_or_decode(&compiled, &bytes, 0x4000, true)),
            native(&compiled, &bytes, 0x4000)
        );
    }
    drop(compiled);
    assert!(weak.upgrade().is_none(), "cache owns no frontend graph");
}

#[test]
fn fid_binding_keys_collide_without_aliasing_and_eviction_releases_state() {
    reset();
    let compiled = frontend();
    let _scope = FrontendScope::enter(&compiled);
    let mut seen = std::collections::HashMap::new();
    let mut collision = None;
    for value in 0..=SLOTS {
        let bytes = vec![0x57, (value & 255) as u8, (value >> 8) as u8];
        if let Some(previous) = seen.insert(slot_for(&bytes), bytes.clone()) {
            collision = Some((previous, bytes));
            break;
        }
    }
    let (a, b) = collision.unwrap();
    let first = get_or_decode(&compiled, &a, 0x1000, true).unwrap();
    let Binding::Shared(state) = &first else {
        panic!("eligible")
    };
    let weak = Arc::downgrade(state);
    drop(first);
    assert_eq!(
        rendered(get_or_decode(&compiled, &b, 0x2000, true)),
        native(&compiled, &b, 0x2000)
    );
    assert!(weak.upgrade().is_none());
    assert_eq!(hits(), 0, "collision must not be a hit");
    assert_eq!(
        rendered(get_or_decode(&compiled, &a, 0x3000, true)),
        native(&compiled, &a, 0x3000)
    );
    for value in 0..1024usize {
        let bytes = [0x57, (value & 255) as u8, (value >> 8) as u8];
        get_or_decode(&compiled, &bytes, 0x1000, true).unwrap();
    }
    CACHE.with(|c| {
        let c = c.borrow();
        assert_eq!(c.entries.len(), SLOTS);
        assert!(c.payload <= PAYLOAD_LIMIT);
        assert_eq!(
            c.payload,
            c.entries.iter().flatten().map(|e| e.payload).sum::<usize>()
        );
    });
}

#[test]
fn fid_binding_retention_and_input_limits_keep_native_fallback() {
    reset();
    let compiled = frontend();
    let _scope = FrontendScope::enter(&compiled);
    let long = vec![0x57; KEY_LIMIT + 1];
    assert!(matches!(
        get_or_decode(&compiled, &long, 0x1000, true).unwrap(),
        Binding::Owned(_)
    ));
    let base = decode_instruction_raw_state(&compiled, &[0x57], 0x1000).unwrap();
    assert!(retained_payload(&base).is_some());
    let mut huge = base.clone();
    huge.construct_nodes.reserve(ENTRY_LIMIT);
    assert!(retained_payload(&huge).is_none());
    let mut huge = base.clone();
    let handle = huge.handles.first_mut().unwrap();
    handle.fixed.space.as_mut().unwrap().name = "x".repeat(ENTRY_LIMIT + 1);
    assert!(retained_payload(&huge).is_none());
    // Total-budget clearing preserves accounting and the current entry.
    let state = Arc::new(base);
    let mut cache = Cache::default();
    for slot in 0..SLOTS {
        cache.insert(
            slot,
            Entry {
                bytes: vec![0x57].into(),
                state: Arc::clone(&state),
                max_checked_offset: 0,
                payload: ENTRY_LIMIT,
            },
        );
        assert!(cache.payload <= PAYLOAD_LIMIT);
    }
    assert_eq!(
        cache.payload,
        cache
            .entries
            .iter()
            .flatten()
            .map(|e| e.payload)
            .sum::<usize>()
    );
}

#[test]
fn fid_binding_witness_merges_discarded_nested_paths_and_unwind() {
    let (_, witness) = binding_address_use::capture(|| {
        binding_address_use::checked_offset(7);
        let _ = binding_address_use::capture(|| {
            binding_address_use::read_pc();
            binding_address_use::checked_offset(19);
            Err::<(), _>("discarded")
        });
        let _ = std::panic::catch_unwind(|| {
            let _ = binding_address_use::capture(|| {
                binding_address_use::checked_offset(23);
                panic!("discarded branch")
            });
        });
        Ok::<_, ()>(())
    });
    assert!(witness.pc_value_read);
    assert_eq!(witness.max_checked_offset, 23);
    let (_, empty) = binding_address_use::capture(|| ());
    assert!(!empty.pc_value_read);
    assert_eq!(empty.max_checked_offset, 0);
}

#[test]
fn fid_binding_four_language_complete_state_differential() {
    for (arch, file) in [
        ("x86", "x86.slaspec"),
        ("x86", "x86-64.slaspec"),
        ("AARCH64", "AARCH64.slaspec"),
        ("ARM", "ARM8m_le.slaspec"),
    ] {
        let compiled = Arc::new(
            crate::compiler::compile_frontend_for_entry_spec(
                &crate::compiler::spec_root_for_arch(arch).join(file),
            )
            .unwrap(),
        );
        let _scope = FrontendScope::enter(&compiled);
        let mut random = 0x4153_5341_5544_4954u64;
        for case in 0..256 {
            let bytes: Vec<_> = (0..1 + case % 16)
                .map(|_| {
                    random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (random >> 32) as u8
                })
                .collect();
            for address in [0x1000, 0x8000_0000_0000_0100, u64::MAX - 2] {
                assert_eq!(
                    rendered(get_or_decode(&compiled, &bytes, address, true)),
                    native(&compiled, &bytes, address),
                    "{file} case{case}"
                );
            }
        }
    }
}

#[test]
fn fid_binding_reentrant_resolver_keeps_hash_and_callback_order() {
    let compiled = frontend();
    let other = Arc::new((*compiled).clone());
    let mut extent = Vec::new();
    for i in 0..8 {
        extent.push(
            decode_instruction_and_lift_with_context_override(
                &compiled,
                &[0x48, 0x89, 0xc8],
                0x1000 + i * 3,
                None,
            )
            .unwrap()
            .0,
        );
    }
    let run = |scoped: bool| {
        let _scope = scoped.then(|| FrontendScope::enter(&compiled));
        let calls = RefCell::new(Vec::new());
        let resolver = |name: &str| {
            calls.borrow_mut().push(name.to_string());
            let _nested = FrontendScope::enter(&other);
            get_or_decode(&other, &[0x57], 0x5000, true).unwrap();
            Some(if name == "RAX" { 0 } else { 8 })
        };
        let hash = fid_hash::compute_fid_hashes(&compiled, &extent, &resolver)
            .map(|h| (h.full_count, h.full_hash, h.specific_count, h.specific_hash));
        (hash, calls.into_inner())
    };
    assert_eq!(run(false), run(true));
}
