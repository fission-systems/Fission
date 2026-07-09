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
