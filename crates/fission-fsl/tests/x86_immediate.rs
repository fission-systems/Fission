#![cfg(feature = "x86")]
use fission_fsl::{
    x86::{X86Package, X86Program},
    ExecutionStatus, MachineState, OutputLayer,
};
fn package() -> X86Package {
    X86Package::compile(
        include_str!("../specs/x86/scalar-immediate.fslx"),
        include_str!("../specs/x86/scalar-immediate-bodies.fsl"),
    )
    .unwrap()
}
fn state() -> MachineState {
    let mut r = vec![0; 17];
    r[4] = 0x2010;
    r[7] = 0x2000;
    MachineState {
        registers: r,
        flags: vec![1; 12],
    }
}
#[test]
fn versioned_split_binding_and_legacy_refusal() {
    let p = package();
    let bytes = p.encode_binary().unwrap();
    assert_eq!(&bytes[..8], b"FSLXPKG2");
    assert_eq!(
        X86Package::decode_binary(&bytes)
            .unwrap()
            .encode_binary()
            .unwrap(),
        bytes
    );
    let mut downgrade = bytes;
    downgrade[7] = b'1';
    assert!(X86Package::decode_binary(&downgrade).is_err());
    let old = X86Package::compile(
        include_str!("../specs/x86/scalar-stateful.fslx"),
        include_str!("../specs/x86/scalar-stateful-bodies.fsl"),
    )
    .unwrap();
    assert_eq!(&old.encode_binary().unwrap()[..8], b"FSLXPKG1");
    assert!(X86Program::lift(&old, 64, 0x1000, &[0x48, 0xc7, 0x47, 8, 1, 0, 0, 0, 0xc3]).is_err());
}
#[test]
fn independent_displacement_literal_widths_and_flag_preservation() {
    let p = package();
    for (mode, raw, bits, want, offset) in [
        (
            64,
            vec![0x48, 0xc7, 0x47, 8, 0xff, 0xff, 0xff, 0xff, 0xc3],
            64,
            u64::MAX,
            8,
        ),
        (
            64,
            vec![0xc7, 0x47, 8, 0xff, 0xff, 0xff, 0xff, 0xc3],
            32,
            0xffff_ffff,
            8,
        ),
        (64, vec![0x66, 0xc7, 0x47, 8, 0, 0x80, 0xc3], 16, 0x8000, 8),
        (
            32,
            vec![0xc7, 0x47, 8, 0x78, 0x56, 0x34, 0x12, 0xc3],
            32,
            0x12345678,
            8,
        ),
        (16, vec![0xc7, 0x47, 8, 0xcd, 0xab, 0xc3], 16, 0xabcd, 8),
        (
            64,
            vec![0x48, 0xc7, 0x47, 0xfc, 0, 0, 0, 0x80, 0xc3],
            64,
            0xffff_ffff_8000_0000,
            0,
        ),
    ] {
        let mut s = state();
        let base = if offset == 0 { 0x1ffc } else { 0x2000 };
        let mut memory = vec![0x55; 32];
        memory[0x2010 - base as usize..0x2010 - base as usize + (mode / 8) as usize]
            .copy_from_slice(&0xbeefu64.to_le_bytes()[..(mode / 8) as usize]);
        if mode == 16 {
            s.registers[3] = 0x2000;
        }
        let q = X86Program::lift(&p, mode, 0x1000, &raw).unwrap();
        assert_eq!(
            q.execute_mut(&mut s, &mut memory, base, 100).unwrap(),
            (ExecutionStatus::Success, Some(0xbeef))
        );
        assert_eq!(
            &memory[offset..offset + (bits / 8)],
            &want.to_le_bytes()[..bits / 8]
        );
        assert_eq!(s.flags, vec![1; 12]);
        let fir = q.emit(OutputLayer::Fir).unwrap();
        assert!(fir.contains("displacement"));
        assert!(fir.contains("literal"));
    }
}
#[test]
fn sib_rip_relative_and_address_override_consume_literal_before_next_pc() {
    let p = package();
    for (raw, base, mut s, offset) in [
        (
            vec![0x48, 0xc7, 0x44, 0x87, 0xfc, 1, 0, 0, 0, 0xc3],
            0x2000,
            state(),
            4,
        ),
        (
            vec![0x48, 0xc7, 0x05, 0xf5, 0x0f, 0, 0, 1, 0, 0, 0, 0xc3],
            0x2000,
            state(),
            0,
        ),
        (
            vec![0x67, 0x48, 0xc7, 0x47, 8, 1, 0, 0, 0, 0xc3],
            0x2000,
            state(),
            8,
        ),
    ] {
        s.registers[0] = 2;
        if raw[0] == 0x67 {
            s.registers[7] |= 1u64 << 32;
        }
        let mut m = vec![0x55; 32];
        m[16..24].copy_from_slice(&0xbeefu64.to_le_bytes());
        let q = X86Program::lift(&p, 64, 0x1000, &raw).unwrap();
        assert_eq!(
            q.execute_mut(&mut s, &mut m, base, 100).unwrap().0,
            ExecutionStatus::Success
        );
        assert_eq!(&m[offset..offset + 8], &1u64.to_le_bytes());
    }
}
#[test]
fn unsupported_immediates_truncation_and_late_failure_refuse_transactionally() {
    let p = package();
    for raw in [
        vec![0x48, 0x83, 0x07, 1, 0xc3],
        vec![0x48, 0xc7, 0x47, 8, 1, 0, 0],
        vec![0xf0, 0xc7, 0x07, 1, 0, 0, 0, 0xc3],
    ] {
        assert!(X86Program::lift(&p, 64, 0x1000, &raw).is_err());
    }
    let q = X86Program::lift(&p, 64, 0x1000, &[0x48, 0xc7, 0x47, 8, 1, 0, 0, 0, 0xc3]).unwrap();
    let mut s = state();
    let original = s.clone();
    let mut m = vec![0x55; 16];
    let before = m.clone();
    for budget in [1, 100] {
        assert_eq!(
            q.execute_mut(&mut s, &mut m, 0x2000, budget).unwrap().0,
            ExecutionStatus::InvalidState
        );
        assert_eq!(s, original);
        assert_eq!(m, before);
    }
}
#[test]
fn split_schema_is_validated_for_optional_bodies_and_fixed_bits() {
    let manifest = include_str!("../specs/x86/scalar-immediate.fslx");
    let source = include_str!("../specs/x86/scalar-immediate-bodies.fsl");
    let bad = source.replacen(
        "field literal offset 86 bits 32;",
        "field literal offset 86 bits 31;",
        1,
    );
    assert!(X86Package::compile(manifest, &bad).is_err());
    let bad = source.replacen(
        "mask 0xffc00000000000000000000000000000",
        "mask 0xffe00000000000000000000000000000",
        1,
    );
    assert!(X86Package::compile(manifest, &bad).is_err());
    let mut raw = package().encode_binary().unwrap();
    raw[7] = b'3';
    assert!(X86Package::decode_binary(&raw).is_err());
}
