use fission_core::architecture::BinaryLoadSpec;
use fission_pcode_ir::{
    BackendIntrinsicCapabilities, CheckedIntrinsicOutcome, ContractViolation,
    ImplementationAvailability, IntrinsicScope, PcodeOp, PcodeOpcode, Varnode,
};
use fission_sleigh::runtime::{
    intrinsic_registry_for_load_spec, sla_spec_version_for_load_spec, userop_map_for_load_spec,
};

fn callother(userop_id: u32, output: Option<Varnode>, args: Vec<Varnode>) -> PcodeOp {
    let mut inputs = vec![Varnode::constant(i64::from(userop_id), 4)];
    inputs.extend(args);
    PcodeOp {
        seq_num: 0,
        opcode: PcodeOpcode::CallOther,
        address: 0x401000,
        output,
        inputs,
        asm_mnemonic: None,
    }
}

fn uniq(offset: u64, size: u32) -> Varnode {
    Varnode {
        space_id: 3,
        offset,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

#[test]
fn sleigh_lifter_provides_version_scoped_intrinsic_contracts_for_x86_and_aarch64() {
    let x86_spec = BinaryLoadSpec::new("ELF", 0, "x86:LE:64:default", "gcc", "unit-test");
    let arm_spec = BinaryLoadSpec::new("ELF", 0, "AARCH64:LE:64:v8A", "gcc", "unit-test");

    let x86_version = sla_spec_version_for_load_spec(&x86_spec).expect("x86-64 .sla version");
    let arm_version = sla_spec_version_for_load_spec(&arm_spec).expect("aarch64 .sla version");
    assert_ne!(
        x86_version, arm_version,
        "distinct .sla binaries must yield distinct version tokens"
    );

    let mut backend = BackendIntrinsicCapabilities::new();
    backend.register(
        "x86",
        "crc32",
        ImplementationAvailability::Verified {
            backend: "sleigh-test".to_string(),
        },
    );
    backend.register(
        "AARCH64",
        "crc32b",
        ImplementationAvailability::Verified {
            backend: "sleigh-test".to_string(),
        },
    );

    let x86_reg =
        intrinsic_registry_for_load_spec(&x86_spec, &backend).expect("x86-64 intrinsic registry");
    let arm_reg =
        intrinsic_registry_for_load_spec(&arm_spec, &backend).expect("aarch64 intrinsic registry");

    let x86_userops = userop_map_for_load_spec(&x86_spec).expect("x86-64 userops");
    let arm_userops = userop_map_for_load_spec(&arm_spec).expect("aarch64 userops");

    let x86_scope = IntrinsicScope::new("x86:LE:64:default", x86_version);
    let arm_scope = IntrinsicScope::new("AARCH64:LE:64:v8A", arm_version);

    // 1. x86 syscall + LOCK + rdtsc + crc32 resolve cleanly with expected effects/proof state.
    let (&syscall_id, _) = x86_userops
        .iter()
        .find(|(_, name)| name.as_str() == "syscall")
        .expect("x86 syscall userop");
    match x86_reg.check_pcode_op(&x86_scope, &callother(syscall_id, None, vec![])) {
        CheckedIntrinsicOutcome::Resolved(call) => {
            assert!(call.effective_effects.writes_memory());
            assert!(call.effective_effects.may_trap());
            assert!(!call.eliminable_when_unused);
        }
        other => panic!("expected resolved x86 syscall contract, got {other:?}"),
    }

    let (&x86_crc_id, _) = x86_userops
        .iter()
        .find(|(_, name)| name.as_str() == "crc32")
        .expect("x86 crc32 userop");
    match x86_reg.check_pcode_op(
        &x86_scope,
        &callother(x86_crc_id, Some(uniq(0, 4)), vec![uniq(8, 4), uniq(16, 4)]),
    ) {
        CheckedIntrinsicOutcome::Resolved(call) => {
            assert!(call.proof_eligible);
            assert!(call.eliminable_when_unused);
        }
        other => panic!("expected verified x86 crc32 contract, got {other:?}"),
    }

    // Wrong width on real x86 crc32 ID degrades cleanly to OpaqueBarrier.
    assert!(matches!(
        x86_reg.check_pcode_op(
            &x86_scope,
            &callother(x86_crc_id, Some(uniq(0, 8)), vec![uniq(8, 4), uniq(16, 4)]),
        ),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::WidthMismatch { .. },
            ..
        }
    ));

    // Undeclared x86 userop (e.g. rdmsr or cpuid) stays an explicit OpaqueBarrier.
    let (&opaque_id, opaque_name) = x86_userops
        .iter()
        .find(|(_, name)| name.as_str() == "rdmsr" || name.as_str() == "cpuid")
        .expect("opaque x86 userop");
    assert!(matches!(
        x86_reg.check_pcode_op(&x86_scope, &callother(opaque_id, None, vec![])),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::UnresolvedUserop { userop_id, name: Some(ref n) },
            ..
        } if userop_id == opaque_id && n == opaque_name
    ));

    // 2. AArch64 CallSupervisor + DataMemoryBarrier + crc32b resolve cleanly.
    let (&crc32b_id, _) = arm_userops
        .iter()
        .find(|(_, name)| name.as_str() == "crc32b")
        .expect("aarch64 crc32b userop");
    match arm_reg.check_pcode_op(
        &arm_scope,
        &callother(
            crc32b_id,
            Some(uniq(0, 4)),
            vec![uniq(8, 4), uniq(16, 4), uniq(24, 4)],
        ),
    ) {
        CheckedIntrinsicOutcome::Resolved(call) => {
            assert!(call.proof_eligible);
            assert!(call.eliminable_when_unused);
        }
        other => panic!("expected verified aarch64 crc32b contract, got {other:?}"),
    }

    let (&svc_id, _) = arm_userops
        .iter()
        .find(|(_, name)| name.as_str() == "CallSupervisor")
        .expect("aarch64 CallSupervisor userop");
    assert!(matches!(
        arm_reg.check_pcode_op(&arm_scope, &callother(svc_id, None, vec![uniq(0, 2)])),
        CheckedIntrinsicOutcome::Resolved(call)
            if call.effective_effects.writes_memory() && !call.eliminable_when_unused
    ));

    // Version invalidation check: stale version string rejects every CallOther to OpaqueBarrier.
    let stale_arm_scope = IntrinsicScope::new("AARCH64:LE:64:v8A", "stale-sha");
    assert!(matches!(
        arm_reg.check_pcode_op(
            &stale_arm_scope,
            &callother(
                crc32b_id,
                Some(uniq(0, 4)),
                vec![uniq(8, 4), uniq(16, 4), uniq(24, 4)],
            ),
        ),
        CheckedIntrinsicOutcome::OpaqueBarrier {
            reason: ContractViolation::VersionSkew { .. },
            ..
        }
    ));
}
