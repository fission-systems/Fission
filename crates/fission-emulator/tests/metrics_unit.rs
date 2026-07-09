use fission_emulator::metrics::{is_jit_supported, EmulatorMetrics};
use fission_pcode::ir::PcodeOpcode;

#[test]
fn metrics_tracks_unimplemented() {
    let mut m = EmulatorMetrics::default();
    m.note_unimplemented(PcodeOpcode::CPoolRef);
    m.note_unimplemented(PcodeOpcode::CPoolRef);
    m.note_unimplemented(PcodeOpcode::New);
    m.note_syscall(1);
    m.note_syscall(60);
    m.note_userop("syscall");
    assert_eq!(m.unimplemented_opcodes.get("CPoolRef"), Some(&2));
    assert_eq!(m.syscalls.get(&1), Some(&1));
    let top = m.top_unimplemented(1);
    assert_eq!(top[0].0, "CPoolRef");
    assert!(m.summary_line().contains("CPoolRef=2"));
    assert_eq!(m.unimplemented_total(), 3);
    assert_eq!(m.unimplemented_kinds(), 2);
}

#[test]
fn unimplemented_budget_gate() {
    let mut m = EmulatorMetrics::default();
    assert!(m.check_unimplemented_budget(0, 0).is_ok());
    m.note_unimplemented(PcodeOpcode::CPoolRef);
    assert!(m.check_unimplemented_budget(0, 0).is_err());
    assert!(m.check_unimplemented_budget(1, 1).is_ok());
    m.note_unimplemented(PcodeOpcode::New);
    assert!(m.check_unimplemented_budget(2, 1).is_err()); // kinds=2 > 1
    assert!(m.check_unimplemented_budget(2, 2).is_ok());
    let err = m.check_unimplemented_budget(0, 0).unwrap_err();
    assert!(err.contains("budget exceeded"), "{err}");
}

#[test]
fn piece_and_lzcount_are_supported() {
    assert!(is_jit_supported(PcodeOpcode::Piece));
    assert!(is_jit_supported(PcodeOpcode::LzCount));
    assert!(is_jit_supported(PcodeOpcode::Extract));
    assert!(is_jit_supported(PcodeOpcode::Insert));
    assert!(!is_jit_supported(PcodeOpcode::CPoolRef));
    assert!(!is_jit_supported(PcodeOpcode::New));
}
