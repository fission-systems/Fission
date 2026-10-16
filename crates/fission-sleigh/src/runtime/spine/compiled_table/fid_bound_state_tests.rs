use super::*;

/// A local before/after probe, including complete successful decoded/lifted
/// tuples and complete error chains. No corpus identity or oracle is involved.
#[test]
fn fid_bound_state_decode_matrix_is_repeatable() {
    let entries = [
        ("x86", "x86.slaspec"),
        ("x86", "x86-64.slaspec"),
        ("AARCH64", "AARCH64.slaspec"),
        ("ARM", "ARM8m_le.slaspec"),
    ];
    let mut records = Vec::new();
    let mut successes = 0;
    let mut errors = 0;
    let mut traced_errors = 0;
    for (arch, file) in entries {
        let compiled = crate::compiler::compile_frontend_for_entry_spec(
            &crate::compiler::spec_root_for_arch(arch).join(file),
        )
        .unwrap();
        let mut seed = 0x2498_137a_5c60_e1bdu64;
        for index in 0..256 {
            let length = 1 + index % 16;
            let bytes: Vec<_> = (0..length)
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 32) as u8
                })
                .collect();
            let render = || {
                match crate::runtime::spine::compiled_table::decode_instruction_and_lift_with_context_override(
                    &compiled, &bytes, 0x1000, None,
                ) {
                    Ok(tuple) => (true, format!("{tuple:?}")),
                    Err(err) => (
                        false,
                        err.chain().map(ToString::to_string).collect::<Vec<_>>().join("\ncaused by: "),
                    ),
                }
            };
            let first = render();
            assert_eq!(first, render(), "{file} matrix case {index}");
            successes += usize::from(first.0);
            errors += usize::from(!first.0);
            traced_errors += usize::from(first.1.contains("sleigh parser path:"));
            let trace_render =
                || match crate::runtime::spine::compiled_table::decode_instruction_raw_state(
                    &compiled, &bytes, 0x1000,
                ) {
                    Ok(state) => (true, format!("{state:?}")),
                    Err(err) => (
                        false,
                        err.chain()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\ncaused by: "),
                    ),
                };
            if let Ok(state) = decode_instruction_raw_state(&compiled, &bytes, 0x1000) {
                let completed = match lift_existing_bound_instruction_with_details(
                    &compiled, &bytes, 0x1000, &state,
                ) {
                    Ok(tuple) => (true, format!("{tuple:?}")),
                    Err(err) => (
                        false,
                        err.chain()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("\ncaused by: "),
                    ),
                };
                assert_eq!(first, completed, "{file} bound completion case {index}");
            }
            let traces = trace_render();
            assert_eq!(traces, trace_render());
            records.push((file, bytes, first, traces));
        }
    }
    assert!(successes > 0 && errors > 0 && traced_errors > 0);
    // Explicitly requested test-only output for the matched local baseline and
    // candidate run; ordinary CI writes no extra artifact.
    if let Some(path) = std::env::var_os("FISSION_TEST_FID_BOUND_RECORDS") {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        serde_json::to_writer(&mut writer, &records).unwrap();
        writer.finish().unwrap();
    }
    eprintln!("bind matrix: {} records / {successes} successes / {errors} errors / {traced_errors} traced errors", records.len());
}

#[test]
fn fid_bound_state_hash_and_resolver_order_matrix_is_repeatable() {
    let mut records = Vec::new();
    let mut measured = 0;
    let mut hashed = 0;
    for (arch, file) in [
        ("x86", "x86.slaspec"),
        ("x86", "x86-64.slaspec"),
        ("AARCH64", "AARCH64.slaspec"),
        ("ARM", "ARM8m_le.slaspec"),
    ] {
        let compiled = crate::compiler::compile_frontend_for_entry_spec(
            &crate::compiler::spec_root_for_arch(arch).join(file),
        )
        .unwrap();
        let mut seed = 0x2498_137a_5c60_e1bdu64;
        for index in 0..128 {
            let bytes: Vec<_> = (0..(1 + index % 16))
                .map(|_| {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (seed >> 32) as u8
                })
                .collect();
            let mut extent = Vec::new();
            let mut address = 0x1000;
            for _ in 0..8 {
                let Ok((instruction, _, _, _)) = decode_instruction_and_lift_with_context_override(
                    &compiled, &bytes, address, None,
                ) else {
                    break;
                };
                address += instruction.length as u64;
                extent.push(instruction);
            }
            let mut outcomes = Vec::new();
            if extent.len() == 8 {
                measured += 1;
                for resolves in [false, true] {
                    let run = || {
                        let calls = std::cell::RefCell::new(Vec::new());
                        // A deterministic callback control, not a register or
                        // Ghidra oracle; both implementations see the same API.
                        let resolver = |name: &str| {
                            calls.borrow_mut().push(name.to_string());
                            resolves.then(|| {
                                name.bytes().fold(0u16, |h, b| {
                                    h.wrapping_mul(31).wrapping_add(u16::from(b))
                                }) as i64
                            })
                        };
                        let hash =
                            fid_hash::compute_fid_hashes(&compiled, &extent, &resolver).map(|h| {
                                (h.full_count, h.full_hash, h.specific_count, h.specific_hash)
                            });
                        (hash, calls.into_inner())
                    };
                    let first = run();
                    assert_eq!(first, run());
                    hashed += usize::from(first.0.is_some());
                    outcomes.push((resolves, first));
                }
            }
            records.push((file, bytes, outcomes));
        }
    }
    assert!(measured > 0 && hashed > 0);
    if let Some(path) = std::env::var_os("FISSION_TEST_FID_HASH_RECORDS") {
        std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    }
    eprintln!(
        "FID hash matrix: {} inputs / {measured} eight-instruction extents / {hashed} hashes",
        records.len()
    );
}

#[test]
fn fid_bound_state_completion_preserves_memory_context_and_delay_details() {
    for (arch, file, cases) in [
        (
            "x86",
            "x86-64.slaspec",
            vec![
                vec![0x48, 0x89, 0xc8],
                vec![0x48, 0x8b, 0x44, 0x8b, 0x10],
                vec![0x48, 0x8b, 0x05, 0x11, 0, 0, 0],
                vec![0xf3, 0xa4],
            ],
        ),
        (
            "AARCH64",
            "AARCH64.slaspec",
            vec![vec![0xfd, 0x7b, 0xbf, 0xa9], vec![0xfd, 0x7b, 0xc1, 0xa8]],
        ),
        ("ARM", "ARM8m_le.slaspec", vec![vec![0x88, 0xbf]]),
        (
            "MIPS",
            "mips32be.slaspec",
            vec![
                vec![0x10, 0, 0, 1, 0, 0, 0, 0],
                vec![0x10, 0, 0, 1], // truncated delay slot must fail equally
            ],
        ),
    ] {
        let compiled = crate::compiler::compile_frontend_for_entry_spec(
            &crate::compiler::spec_root_for_arch(arch).join(file),
        )
        .unwrap();
        for bytes in cases {
            let state = decode_instruction_raw_state(&compiled, &bytes, 0x1000).unwrap();
            let original =
                decode_instruction_and_lift_with_context_override(&compiled, &bytes, 0x1000, None);
            let completed =
                lift_existing_bound_instruction_with_details(&compiled, &bytes, 0x1000, &state);
            // Anyhow's Debug also contains the host backtrace when enabled.
            // Compare complete results/error chains, excluding call-site stacks.
            let original_record = original
                .as_ref()
                .map(|value| format!("{value:?}"))
                .map_err(|err| err.chain().map(ToString::to_string).collect::<Vec<_>>());
            let completed_record = completed
                .as_ref()
                .map(|value| format!("{value:?}"))
                .map_err(|err| err.chain().map(ToString::to_string).collect::<Vec<_>>());
            assert_eq!(original_record, completed_record, "{file} {bytes:x?}");
            if file == "ARM8m_le.slaspec" {
                assert!(!completed
                    .as_ref()
                    .unwrap()
                    .3
                    .pending_context_commits
                    .is_empty());
            } else if file == "mips32be.slaspec" && bytes.len() == 4 {
                assert!(completed.is_err());
            } else {
                assert!(completed.is_ok());
            }
        }
    }
}

#[test]
fn fid_bound_state_completion_still_validates_commits_lengths_and_display() {
    let compiled = crate::compiler::compile_frontend_for_entry_spec(
        &crate::compiler::spec_root_for_arch("x86").join("x86-64.slaspec"),
    )
    .unwrap();
    let bytes = [0x90];
    let state = decode_instruction_raw_state(&compiled, &bytes, 0x1000).unwrap();
    let mut invalid_display = state.clone();
    let display = std::sync::Arc::make_mut(
        &mut std::sync::Arc::make_mut(&mut invalid_display.constructor).display_template,
    );
    display.flowthru_operand_index = None;
    display.first_whitespace = Some(display.pieces.len() + 1);
    // This state emits normally; full decode must still reject its description.
    assert!(emit_pcode_for_state_with_bytes(
        &compiled,
        0x1000,
        &bytes,
        0x1000,
        &invalid_display,
        FlowEmitOptions {
            instruction_length: Some(1),
            ..Default::default()
        },
    )
    .is_ok());
    let err =
        lift_existing_bound_instruction_with_details(&compiled, &bytes, 0x1000, &invalid_display)
            .unwrap_err();
    assert!(err.to_string().contains("first_whitespace"));

    let mut invalid_commit = state.clone();
    std::sync::Arc::make_mut(&mut invalid_commit.constructor)
        .context_commits
        .push(crate::compiler::CompiledContextCommit {
            symbol_id: 0,
            target: CompiledContextCommitTarget::OperandHandle {
                hand_index: u32::MAX,
            },
            word_index: 0,
            mask: 1,
        });
    assert!(lift_existing_bound_instruction_with_details(
        &compiled,
        &bytes,
        0x1000,
        &invalid_commit,
    )
    .is_err());
    let mut invalid_length = state;
    invalid_length.length = bytes.len() + 1;
    assert!(lift_existing_bound_instruction_with_details(
        &compiled,
        &bytes,
        0x1000,
        &invalid_length,
    )
    .is_err());
}
