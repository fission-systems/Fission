#![cfg(feature = "x86")]
use fission_fsl::{
    x86::{X86Package, X86Program},
    ExecutionStatus, MachineState,
};
fn package() -> X86Package {
    X86Package::compile(
        include_str!("../specs/x86/scalar-stateful.fslx"),
        include_str!("../specs/x86/scalar-stateful-bodies.fsl"),
    )
    .unwrap()
}
fn state() -> MachineState {
    let mut r = vec![0; 17];
    r[4] = 0x2000;
    r[5] = 0x11223344;
    MachineState {
        registers: r,
        flags: vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1],
    }
}
#[test]
fn frame32_and_stack_store_forwarding_preserve_entry_frame() {
    let p = X86Program::lift(
        &package(),
        32,
        0x1000,
        &[0x55, 0x89, 0xe5, 0x8b, 0x45, 8, 0x03, 0x45, 12, 0x5d, 0xc3],
    )
    .unwrap();
    let evidence = p.recover_input_evidence().unwrap();
    assert!(evidence.contains("exact-store=0"));
    assert!(evidence.contains("offset: 4, bits: 32"));
    let mut s = state();
    let mut memory = vec![0; 32];
    memory[16..20].copy_from_slice(&0xdeadbeefu32.to_le_bytes());
    memory[20..24].copy_from_slice(&7u32.to_le_bytes());
    memory[24..28].copy_from_slice(&5u32.to_le_bytes());
    assert_eq!(
        p.execute_mut(&mut s, &mut memory, 0x1ff0, 100).unwrap(),
        (ExecutionStatus::Success, Some(0xdeadbeef))
    );
    assert_eq!(s.registers[0], 12);
    assert_eq!(s.registers[4], 0x2004);
    assert_eq!(s.registers[5], 0x11223344);
    assert_eq!(&memory[12..16], &0x11223344u32.to_le_bytes());
}
#[test]
fn push_pop_stack_pointer_alias_and_extended_register_contracts() {
    let pkg = package();
    for (raw, slot, value, sp) in [
        (vec![0x54, 0x58, 0xc3], 0, 0x2000, 0x2008),
        (vec![0x54, 0x5c, 0xc3], 0, 0, 0x2008),
        (vec![0x41, 0x50, 0x41, 0x58, 0xc3], 8, 42, 0x2008),
    ] {
        let p = X86Program::lift(&pkg, 64, 0x1000, &raw).unwrap();
        let mut s = state();
        s.registers[8] = 42;
        let mut memory = vec![0; 32];
        memory[16..24].copy_from_slice(&0xdeadbeefu64.to_le_bytes());
        assert_eq!(
            p.execute_mut(&mut s, &mut memory, 0x1ff0, 100).unwrap().0,
            ExecutionStatus::Success
        );
        assert_eq!(s.registers[slot], value);
        assert_eq!(s.registers[4], sp);
    }
    // Modern IA-32 16/32-bit contract: PUSH SP captures the old SP (not 8086).
    for mode in [16, 32] {
        let size = (mode / 8) as usize;
        for pop in [0x58, 0x5c] {
            let p = X86Program::lift(&pkg, mode, 0x1000, &[0x54, pop, 0xc3]).unwrap();
            let mut s = state();
            let mut memory = vec![0; 32];
            memory[16..16 + size].copy_from_slice(&0xbeefu64.to_le_bytes()[..size]);
            assert_eq!(
                p.execute_mut(&mut s, &mut memory, 0x1ff0, 100).unwrap(),
                (ExecutionStatus::Success, Some(0xbeef))
            );
            assert_eq!(s.registers[4], 0x2000 + size as u64);
            assert_eq!(s.registers[0], if pop == 0x58 { 0x2000 } else { 0 });
            assert_eq!(&memory[16 - size..16], &0x2000u64.to_le_bytes()[..size]);
        }
    }
    assert!(X86Program::lift(&pkg, 64, 0x1000, &[0x66, 0x55, 0xc3]).is_err());
    assert!(X86Program::lift(&pkg, 64, 0x1000, &[0x48, 0xc7, 0x07, 1, 0, 0, 0, 0xc3]).is_err());
}
#[test]
fn late_return_and_budget_failure_roll_back_registers_flags_and_memory() {
    let p = X86Program::lift(&package(), 64, 0x1000, &[0x48, 0x89, 0x07, 0xc3]).unwrap();
    let mut s = state();
    s.registers[7] = 0x2010;
    s.registers[0] = 42;
    let original = s.clone();
    let mut memory = vec![0xaa; 8];
    let original_memory = memory.clone();
    assert_eq!(
        p.execute(&mut s, &memory, 0x2010, 100).unwrap().0,
        ExecutionStatus::InvalidState
    );
    for budget in [1, 100] {
        assert_eq!(
            p.execute_mut(&mut s, &mut memory, 0x2010, budget)
                .unwrap()
                .0,
            ExecutionStatus::InvalidState
        );
        assert_eq!(s, original);
        assert_eq!(memory, original_memory);
    }
}
#[test]
fn partial_and_unknown_store_aliases_remain_candidates() {
    let p = X86Program::lift(
        &package(),
        64,
        0x1000,
        &[0x48, 0x89, 0x07, 0x8b, 0x47, 4, 0xc3],
    )
    .unwrap();
    let e = p.recover_input_evidence().unwrap();
    assert!(e.contains("possible-stores=[0]"));
    assert!(e.contains("confidence=overapproximation"));
    let p = X86Program::lift(
        &package(),
        64,
        0x1000,
        &[0x48, 0x89, 0x07, 0x48, 0x8b, 0x06, 0xc3],
    )
    .unwrap();
    assert!(p
        .recover_input_evidence()
        .unwrap()
        .contains("possible-stores=[0]"));
}
