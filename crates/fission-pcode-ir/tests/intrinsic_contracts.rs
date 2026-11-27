use fission_pcode_ir::optimizer::passes::DeadCodeEliminator;
use fission_pcode_ir::space::{REGISTER_SPACE_ID, UNIQUE_SPACE_ID};
use fission_pcode_ir::{
    BackendIntrinsicCapabilities, CheckedIntrinsicOutcome, ContractViolation,
    ImplementationAvailability, IntrinsicControlEffect, IntrinsicDeclaration, IntrinsicEffects,
    IntrinsicEnvironmentEffect, IntrinsicIdentity, IntrinsicRegistry, IntrinsicScope,
    IntrinsicSignature, IntrinsicValueKind, IntrinsicWidthConstraint, OperandDecl, PcodeBasicBlock,
    PcodeFunction, PcodeOp, PcodeOpcode, SemanticProvenance, Varnode,
    build_default_intrinsic_registry,
};
use std::collections::BTreeMap;

fn uniq(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn reg(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: REGISTER_SPACE_ID,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn callother_op(
    seq_num: u32,
    userop: Varnode,
    output: Option<Varnode>,
    args: Vec<Varnode>,
) -> PcodeOp {
    let mut inputs = Vec::with_capacity(1 + args.len());
    inputs.push(userop);
    inputs.extend(args);
    PcodeOp {
        seq_num,
        opcode: PcodeOpcode::CallOther,
        address: 0x1000 + u64::from(seq_num) * 4,
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn make_x86_userops() -> BTreeMap<u32, String> {
    BTreeMap::from([
        (5, "syscall".to_string()),
        (12, "rdtsc".to_string()),
        (17, "LOCK".to_string()),
        (42, "crc32".to_string()),
        (99, "rdmsr".to_string()),
    ])
}

#[test]
fn typed_declarations_enforce_arity_width_output_and_version_contracts() {
    let scope = IntrinsicScope::new("x86:LE:64:default", "spec-v1");
    let mut backend = BackendIntrinsicCapabilities::new();
    backend.register(
        "x86",
        "crc32",
        ImplementationAvailability::Verified {
            backend: "fission-pcode-test".to_string(),
        },
    );
    let registry = build_default_intrinsic_registry(scope.clone(), &make_x86_userops(), &backend);

    // Valid x86 crc32(4B, 1B) -> 4B resolves cleanly as proof-eligible and eliminable.
    let ok_crc = callother_op(
        0,
        Varnode::constant(42, 4),
        Some(uniq(0x10, 4)),
        vec![uniq(0x20, 4), uniq(0x30, 1)],
    );
    match registry.check_pcode_op(&scope, &ok_crc) {
        CheckedIntrinsicOutcome::Resolved(call) => {
            assert!(call.proof_eligible);
            assert!(call.eliminable_when_unused);
            assert!(call.effective_effects.is_pure_value_effect());
        }
        other => panic!("expected resolved crc32 contract, got {other:?}"),
    }

    // Mismatched arity (1 arg instead of 2) -> OpaqueBarrier(ArityMismatch).
    let bad_arity = callother_op(
        1,
        Varnode::constant(42, 4),
        Some(uniq(0x10, 4)),
        vec![uniq(0x20, 4)],
    );
    assert!(matches!(
        registry.check_pcode_op(&scope, &bad_arity),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::ArityMismatch { expected: 2, found: 1, .. },
            effects,
            ..
        } if effects.is_opaque_barrier()
    ));

    // Mismatched output width (MatchesOperand(0) requires 8B when arg 0 is 8B, not 4B).
    let bad_out_width = callother_op(
        2,
        Varnode::constant(42, 4),
        Some(uniq(0x10, 4)),
        vec![uniq(0x20, 8), uniq(0x30, 1)],
    );
    assert!(matches!(
        registry.check_pcode_op(&scope, &bad_out_width),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::WidthMismatch { found_bytes: 4, .. },
            ..
        }
    ));

    // Mismatched input width (3-byte accumulator is invalid for x86 crc32).
    let bad_in_width = callother_op(
        3,
        Varnode::constant(42, 4),
        Some(uniq(0x10, 3)),
        vec![uniq(0x20, 3), uniq(0x30, 1)],
    );
    assert!(matches!(
        registry.check_pcode_op(&scope, &bad_in_width),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::WidthMismatch { found_bytes: 3, .. },
            ..
        }
    ));

    // Missing output on value-returning intrinsic -> MissingOutput barrier.
    let missing_out = callother_op(
        4,
        Varnode::constant(42, 4),
        None,
        vec![uniq(0x20, 4), uniq(0x30, 1)],
    );
    assert!(matches!(
        registry.check_pcode_op(&scope, &missing_out),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::MissingOutput { .. },
            ..
        }
    ));

    // Unexpected output on void barrier (LOCK) -> UnexpectedOutput barrier.
    let unexpected_out = callother_op(5, Varnode::constant(17, 4), Some(uniq(0x10, 4)), Vec::new());
    assert!(matches!(
        registry.check_pcode_op(&scope, &unexpected_out),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::UnexpectedOutput { .. },
            ..
        }
    ));

    // Non-constant userop index -> NonConstantUseropIndex barrier.
    let non_const_id = callother_op(
        6,
        reg(0, 4),
        Some(uniq(0x10, 4)),
        vec![uniq(0x20, 4), uniq(0x30, 1)],
    );
    assert!(matches!(
        registry.check_pcode_op(&scope, &non_const_id),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::NonConstantUseropIndex,
            ..
        }
    ));

    // Unresolved/opaque userop (rdmsr #99) -> UnresolvedUserop barrier.
    let opaque_op = callother_op(7, Varnode::constant(99, 4), Some(uniq(0x10, 8)), vec![]);
    assert!(matches!(
        registry.check_pcode_op(&scope, &opaque_op),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::UnresolvedUserop { userop_id: 99, name: Some(ref n) },
            effects,
            ..
        } if n == "rdmsr" && effects.is_opaque_barrier()
    ));

    // Version skew invalidation: querying with stale version -> OpaqueBarrier(VersionSkew).
    let stale_scope = IntrinsicScope::new("x86:LE:64:default", "spec-v0-stale");
    assert!(matches!(
        registry.check_pcode_op(&stale_scope, &ok_crc),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::VersionSkew { .. },
            effects,
            ..
        } if effects.is_opaque_barrier()
    ));
}

#[test]
fn same_userop_id_or_name_across_languages_is_strictly_isolated() {
    let x86_scope = IntrinsicScope::new("x86:LE:64:default", "v1");
    let arm_scope = IntrinsicScope::new("AARCH64:LE:64:v8A", "v1");

    let mut x86_ops = BTreeMap::new();
    x86_ops.insert(10, "crc32".to_string());
    x86_ops.insert(20, "custom_trap".to_string());

    let mut arm_ops = BTreeMap::new();
    arm_ops.insert(10, "crc32w".to_string());
    arm_ops.insert(20, "CallSupervisor".to_string());

    let mut backend = BackendIntrinsicCapabilities::new();
    backend.register(
        "x86",
        "crc32",
        ImplementationAvailability::Verified {
            backend: "emu".to_string(),
        },
    );
    backend.register(
        "AARCH64",
        "crc32w",
        ImplementationAvailability::Verified {
            backend: "emu".to_string(),
        },
    );

    let x86_reg = build_default_intrinsic_registry(x86_scope.clone(), &x86_ops, &backend);
    let arm_reg = build_default_intrinsic_registry(arm_scope.clone(), &arm_ops, &backend);

    // Same numeric ID 10 means 2-arg x86 crc32 vs 3-arg AArch64 crc32w.
    let op_id10_two_args = callother_op(
        0,
        Varnode::constant(10, 4),
        Some(uniq(0, 4)),
        vec![uniq(8, 4), uniq(16, 4)],
    );
    assert!(matches!(
        x86_reg.check_pcode_op(&x86_scope, &op_id10_two_args),
        CheckedIntrinsicOutcome::Resolved(_)
    ));
    assert!(matches!(
        arm_reg.check_pcode_op(&arm_scope, &op_id10_two_args),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::ArityMismatch {
                expected: 3,
                found: 2,
                ..
            },
            ..
        }
    ));

    // Same registry queried with the wrong language fails with LanguageMismatch.
    assert!(matches!(
        x86_reg.check_pcode_op(&arm_scope, &op_id10_two_args),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::LanguageMismatch { .. },
            ..
        }
    ));
}

#[test]
fn invalid_annotations_and_missing_backend_implementations_cannot_upgrade_assurance() {
    let scope = IntrinsicScope::new("x86:LE:64:default", "v1");

    // 1. Verified provenance without Verified availability is rejected.
    let inconsistent = IntrinsicDeclaration {
        identity: IntrinsicIdentity {
            scope: scope.clone(),
            userop_id: 1,
            name: "crc32".to_string(),
        },
        signature: IntrinsicSignature {
            inputs: vec![
                OperandDecl {
                    name: "a".to_string(),
                    kind: IntrinsicValueKind::BitVector,
                    width: IntrinsicWidthConstraint::ExactBytes(4),
                },
                OperandDecl {
                    name: "b".to_string(),
                    kind: IntrinsicValueKind::BitVector,
                    width: IntrinsicWidthConstraint::ExactBytes(4),
                },
            ],
            output: Some(OperandDecl {
                name: "out".to_string(),
                kind: IntrinsicValueKind::BitVector,
                width: IntrinsicWidthConstraint::MatchesOperand(0),
            }),
        },
        effects: IntrinsicEffects::pure_total(),
        provenance: SemanticProvenance::VerifiedImplementation {
            backend: "emu".to_string(),
            witness: "unit".to_string(),
        },
        availability: ImplementationAvailability::DeclaredOnly,
    };
    assert!(matches!(
        inconsistent.validate(),
        Err(ContractViolation::InvalidEffectAnnotation { .. })
    ));

    // 2. Pure total void operation without output is rejected.
    let pure_void = IntrinsicDeclaration {
        identity: IntrinsicIdentity {
            scope: scope.clone(),
            userop_id: 2,
            name: "noop_claim".to_string(),
        },
        signature: IntrinsicSignature {
            inputs: vec![],
            output: None,
        },
        effects: IntrinsicEffects::pure_total(),
        provenance: SemanticProvenance::Declared,
        availability: ImplementationAvailability::DeclaredOnly,
    };
    assert!(matches!(
        pure_void.validate(),
        Err(ContractViolation::InvalidEffectAnnotation { .. })
    ));

    // 3. Invalid MatchesOperand index out of bounds is rejected.
    let bad_sig = IntrinsicSignature {
        inputs: vec![OperandDecl {
            name: "x".to_string(),
            kind: IntrinsicValueKind::BitVector,
            width: IntrinsicWidthConstraint::ExactBytes(4),
        }],
        output: Some(OperandDecl {
            name: "out".to_string(),
            kind: IntrinsicValueKind::BitVector,
            width: IntrinsicWidthConstraint::MatchesOperand(3),
        }),
    };
    assert!(matches!(
        bad_sig.validate(),
        Err(ContractViolation::InvalidSignature { .. })
    ));

    // 4. Claimed VerifiedImplementation without backend support gets downgraded.
    let mut reg = IntrinsicRegistry::new(scope.clone(), BTreeMap::from([(42, "crc32".into())]));
    let claimed_verified = IntrinsicDeclaration {
        identity: IntrinsicIdentity {
            scope: scope.clone(),
            userop_id: 42,
            name: "crc32".to_string(),
        },
        signature: IntrinsicSignature {
            inputs: vec![
                OperandDecl {
                    name: "acc".to_string(),
                    kind: IntrinsicValueKind::BitVector,
                    width: IntrinsicWidthConstraint::ExactBytes(4),
                },
                OperandDecl {
                    name: "data".to_string(),
                    kind: IntrinsicValueKind::BitVector,
                    width: IntrinsicWidthConstraint::ExactBytes(4),
                },
            ],
            output: Some(OperandDecl {
                name: "crc".to_string(),
                kind: IntrinsicValueKind::BitVector,
                width: IntrinsicWidthConstraint::ExactBytes(4),
            }),
        },
        effects: IntrinsicEffects::pure_total(),
        provenance: SemanticProvenance::VerifiedImplementation {
            backend: "emu".to_string(),
            witness: "claim".to_string(),
        },
        availability: ImplementationAvailability::Verified {
            backend: "emu".to_string(),
        },
    };
    reg.register(claimed_verified).unwrap();

    // Backend reports crc32 as Unimplemented -> registry downgrades it and blocks proof/DCE.
    let mut missing_backend = BackendIntrinsicCapabilities::new();
    missing_backend.register("x86", "crc32", ImplementationAvailability::Unimplemented);
    let issues = reg.validate_against_backend(&missing_backend);
    assert!(matches!(
        issues.as_slice(),
        [ContractViolation::MissingBackendImplementation { name }] if name == "crc32"
    ));

    let op = callother_op(
        0,
        Varnode::constant(42, 4),
        Some(uniq(0, 4)),
        vec![uniq(8, 4), uniq(16, 4)],
    );
    match reg.check_pcode_op(&scope, &op) {
        CheckedIntrinsicOutcome::Resolved(call) => {
            assert!(!call.proof_eligible);
            assert!(!call.eliminable_when_unused);
            assert!(call.effective_effects.is_opaque_barrier());
        }
        other => panic!("expected downgraded call, got {other:?}"),
    }
}

#[test]
fn dead_code_eliminator_consumer_respects_verified_pure_vs_unverified_and_barriers() {
    let scope = IntrinsicScope::new("x86:LE:64:default", "spec-1");
    let mut userops = make_x86_userops();
    userops.insert(77, "div_may_fault".to_string());

    let mut verified_backend = BackendIntrinsicCapabilities::new();
    verified_backend.register(
        "x86",
        "crc32",
        ImplementationAvailability::Verified {
            backend: "pcode-ref".to_string(),
        },
    );
    verified_backend.register(
        "x86",
        "div_may_fault",
        ImplementationAvailability::Verified {
            backend: "pcode-ref".to_string(),
        },
    );
    verified_backend.register(
        "x86",
        "rdtsc",
        ImplementationAvailability::Verified {
            backend: "pcode-ref".to_string(),
        },
    );

    let mut reg_verified =
        build_default_intrinsic_registry(scope.clone(), &userops, &verified_backend);
    // Register a pure-value but trapping operation to prove purity does not imply nontrapping.
    reg_verified
        .register(IntrinsicDeclaration {
            identity: IntrinsicIdentity {
                scope: scope.clone(),
                userop_id: 77,
                name: "div_may_fault".to_string(),
            },
            signature: IntrinsicSignature {
                inputs: vec![
                    OperandDecl {
                        name: "a".into(),
                        kind: IntrinsicValueKind::UnsignedInt,
                        width: IntrinsicWidthConstraint::ExactBytes(4),
                    },
                    OperandDecl {
                        name: "b".into(),
                        kind: IntrinsicValueKind::UnsignedInt,
                        width: IntrinsicWidthConstraint::ExactBytes(4),
                    },
                ],
                output: Some(OperandDecl {
                    name: "q".into(),
                    kind: IntrinsicValueKind::UnsignedInt,
                    width: IntrinsicWidthConstraint::ExactBytes(4),
                }),
            },
            effects: IntrinsicEffects {
                control: IntrinsicControlEffect::MayFault,
                ..IntrinsicEffects::pure_total()
            },
            provenance: SemanticProvenance::VerifiedImplementation {
                backend: "pcode-ref".into(),
                witness: "fixture".into(),
            },
            availability: ImplementationAvailability::Verified {
                backend: "pcode-ref".into(),
            },
        })
        .unwrap();

    let make_program = || PcodeFunction {
        blocks: vec![PcodeBasicBlock {
            index: 0,
            start_address: 0x1000,
            successors: vec![],
            ops: vec![
                // 0: unused verified pure total crc32 -> ELIMINABLE only under verified matching scope
                callother_op(
                    0,
                    Varnode::constant(42, 4),
                    Some(uniq(0x10, 4)),
                    vec![reg(0, 4), reg(8, 1)],
                ),
                // 1: unused pure-value but MayFault op -> MUST KEEP
                callother_op(
                    1,
                    Varnode::constant(77, 4),
                    Some(uniq(0x20, 4)),
                    vec![reg(0, 4), reg(8, 4)],
                ),
                // 2: unused rdtsc (ReadsHardwareState + MayFault) -> MUST KEEP
                callother_op(2, Varnode::constant(12, 4), Some(uniq(0x30, 8)), vec![]),
                // 3: LOCK barrier -> MUST KEEP
                callother_op(3, Varnode::constant(17, 4), None, vec![]),
                // 4: syscall side-effect barrier -> MUST KEEP
                callother_op(4, Varnode::constant(5, 4), None, vec![]),
                // 5: opaque undeclared rdmsr -> MUST KEEP
                callother_op(5, Varnode::constant(99, 4), Some(uniq(0x40, 8)), vec![]),
                PcodeOp {
                    seq_num: 6,
                    opcode: PcodeOpcode::Return,
                    address: 0x1018,
                    output: None,
                    inputs: vec![Varnode::constant(0, 8)],
                    asm_mnemonic: None,
                },
            ],
        }],
    };

    let dce = DeadCodeEliminator::new();

    // A) Without registry -> preserves ALL 7 ops (legacy contract unchanged).
    let mut f_no_reg = make_program();
    let mut mod_a = false;
    dce.eliminate(&mut f_no_reg, &mut mod_a);
    assert!(!mod_a);
    assert_eq!(f_no_reg.blocks[0].ops.len(), 7);

    // B) Unverified declaration (DeclaredOnly) -> preserves ALL 7 ops.
    let reg_unverified = build_default_intrinsic_registry(
        scope.clone(),
        &userops,
        &BackendIntrinsicCapabilities::new(),
    );
    let mut f_unverified = make_program();
    let mut mod_b = false;
    dce.eliminate_with_intrinsics(
        &mut f_unverified,
        &mut mod_b,
        Some((&reg_unverified, &scope)),
    );
    assert!(!mod_b);
    assert_eq!(f_unverified.blocks[0].ops.len(), 7);

    // C) Version skew -> preserves ALL 7 ops.
    let stale_scope = IntrinsicScope::new("x86:LE:64:default", "spec-stale");
    let mut f_skew = make_program();
    let mut mod_c = false;
    dce.eliminate_with_intrinsics(&mut f_skew, &mut mod_c, Some((&reg_verified, &stale_scope)));
    assert!(!mod_c);
    assert_eq!(f_skew.blocks[0].ops.len(), 7);

    // D) Verified matching scope -> removes ONLY dead verified pure-total crc32 (#0); preserves #1..=#5 + Return.
    let mut f_verified = make_program();
    let mut mod_d = false;
    dce.eliminate_with_intrinsics(&mut f_verified, &mut mod_d, Some((&reg_verified, &scope)));
    assert!(mod_d);
    let surviving_seqs: Vec<u32> = f_verified.blocks[0]
        .ops
        .iter()
        .map(|op| op.seq_num)
        .collect();
    assert_eq!(surviving_seqs, vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn effect_lattice_join_and_crc_reference_vectors() {
    // Verify effect lattice conservative join rules.
    let pure = IntrinsicEffects::pure_total();
    let fault = IntrinsicEffects {
        control: IntrinsicControlEffect::MayFault,
        ..pure
    };
    let read_hw = IntrinsicEffects {
        environment: IntrinsicEnvironmentEffect::ReadsHardwareState,
        ..pure
    };
    let barrier = IntrinsicEffects::opaque_barrier();

    let j1 = pure.join(fault);
    assert!(j1.is_pure_value_effect());
    assert!(!j1.is_total_nontrapping());

    let j2 = pure.join(read_hw);
    assert!(!j2.is_pure_value_effect());
    assert_eq!(pure.join(barrier), barrier);

    // Concrete reference fixtures matching the documented x86 Castagnoli (CRC-32C)
    // and AArch64 IEEE 802.3 CRC32B semantics over the standard ASCII test vector b"123456789".
    fn ref_crc32_step(acc: u32, byte: u8, reflected_poly: u32) -> u32 {
        let mut crc = acc ^ u32::from(byte);
        for _ in 0..8 {
            crc = if (crc & 1) != 0 {
                (crc >> 1) ^ reflected_poly
            } else {
                crc >> 1
            };
        }
        crc
    }

    let mut crc32c = !0u32;
    let mut crc32_ieee = !0u32;
    for &b in b"123456789" {
        crc32c = ref_crc32_step(crc32c, b, 0x82F6_3B78);
        crc32_ieee = ref_crc32_step(crc32_ieee, b, 0xEDB8_8320);
    }
    assert_eq!(!crc32c, 0xE306_9283, "x86 CRC32C check vector");
    assert_eq!(!crc32_ieee, 0xCBF4_3926, "AArch64 CRC32B IEEE check vector");
}
