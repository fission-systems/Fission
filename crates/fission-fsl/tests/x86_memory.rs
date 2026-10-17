#![cfg(feature = "x86")]
use fission_fsl::{
    x86::{X86Package, X86Program},
    ExecutionStatus, MachineState,
};
fn package() -> X86Package {
    X86Package::compile(
        include_str!("../specs/x86/scalar.fslx"),
        include_str!("../specs/x86/scalar-memory-bodies.fsl"),
    )
    .unwrap()
}
fn state() -> MachineState {
    let mut registers = vec![0; 17];
    registers[4] = 0x2000;
    MachineState {
        registers,
        flags: vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1],
    }
}
#[test]
fn cdecl_stack_dependencies_are_candidates_with_original_origins() {
    let program = X86Program::lift(
        &package(),
        32,
        0x1000,
        &[0x8b, 0x44, 0x24, 8, 0x03, 0x44, 0x24, 4, 0xc3],
    )
    .unwrap();
    let evidence = program.recover_input_evidence().unwrap();
    assert!(evidence.contains("convention=unselected"));
    assert!(evidence.contains("offset: 4, bits: 32"));
    assert!(evidence.contains("offset: 8, bits: 32"));
    assert!(evidence.contains("raw=03442404"));
    assert!(evidence.contains("role=control-or-other"));
}
#[test]
fn memory_read_guard_and_late_ret_failure_preserve_all_caller_state() {
    let program = X86Program::lift(
        &package(),
        32,
        0x1000,
        &[0x8b, 0x44, 0x24, 8, 0x03, 0x44, 0x24, 4, 0xc3],
    )
    .unwrap();
    let mut s = state();
    let original = s.clone();
    assert_eq!(
        program.execute(&mut s, &[0; 11], 0x2000, 100).unwrap().0,
        ExecutionStatus::InvalidState
    );
    assert_eq!(s, original);
    // MOV+ADD can read the supplied args, but RET cannot read below its base.
    assert_eq!(
        program
            .execute(&mut s, &[7, 0, 0, 0, 5, 0, 0, 0], 0x2004, 100)
            .unwrap()
            .0,
        ExecutionStatus::InvalidState
    );
    assert_eq!(s, original);
    let memory = [0xef, 0xbe, 0xad, 0xde, 7, 0, 0, 0, 5, 0, 0, 0];
    assert_eq!(
        program.execute(&mut s, &memory, 0x2000, 100).unwrap(),
        (ExecutionStatus::Success, Some(0xdeadbeef))
    );
    assert_eq!(s.registers[0], 12);
    assert_eq!(s.registers[4], 0x2004);
}
#[test]
fn unknown_control_and_unmigrated_memory_effects_refuse() {
    let p = package();
    let program = X86Program::lift(&p, 32, 0x1000, &[0x74, 0, 0xc3]).unwrap();
    assert!(program.recover_input_evidence().is_err());
    assert!(X86Program::lift(&p, 32, 0x1000, &[0x89, 0x44, 0x24, 4, 0xc3]).is_err());
    assert!(X86Program::lift(&p, 32, 0x1000, &[0xf0, 0x03, 0x44, 0x24, 4, 0xc3]).is_err());
}
