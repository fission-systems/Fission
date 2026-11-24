//! A register read after a branch *inside* one instruction.
//!
//! AArch64 `csel` lifts to a p-code `CBRANCH` that skips the copy of its
//! second operand. The JIT cached a register in a Cranelift variable at its
//! first read -- and if that first read sat in the skipped arm, the variable
//! had no definition on the taken path, so a later read of the same register
//! got Cranelift's implicit zero. `cmp w1,#2; mov w8,#2; csel w10,wzr,w8,eq;
//! mov w0,w8; ret` returned 0 where the hardware returns 2.
//!
//! The expected values are native: the same three sequences, built as naked
//! functions and run on an AArch64 host, return
//!
//! ```text
//! b=1 h=2 k=2 m=0
//! b=2 h=2 k=0 m=2
//! b=3 h=2 k=2 m=0
//! ```
//!
//! Both engines are checked: the interpreter is the control that shows the
//! code and the expected values are right, and the JIT is what was wrong.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::{Emulator, RunOutcome};
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Where `ret` lands. Nothing is mapped there; the run stops on arrival.
const SENTINEL: u64 = 0xDEAD_0000;

fn aarch64(interpret: bool) -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/aarch64_control_flow.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu =
        Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emulator");
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// Run `code` at the image entry as `f(a, b)` and return `w0`.
fn call(code: &[u32], a: u64, b: u64, interpret: bool) -> u64 {
    let mut emu = aarch64(interpret);
    let entry = emu.pc;
    let bytes: Vec<u8> = code.iter().flat_map(|w| w.to_le_bytes()).collect();
    emu.state
        .write_ram_unprotected(entry, &bytes)
        .expect("write code");
    emu.write_register_u64("x0", a).unwrap();
    emu.write_register_u64("x1", b).unwrap();
    // Garbage in the registers the sequence writes, so a stale or zero read
    // cannot pass by accident.
    emu.write_register_u64("x8", 0x5555_5555_5555_5555).unwrap();
    emu.write_register_u64("x10", 0x6666_6666_6666_6666)
        .unwrap();
    emu.write_register_u64("x30", SENTINEL).unwrap();
    let outcome = emu.run_until_pc(SENTINEL).expect("run");
    assert!(matches!(outcome, RunOutcome::Returned), "{outcome:?}");
    emu.read_register_u64("x0").unwrap() & 0xFFFF_FFFF
}

const CMP_W1_2: u32 = 0x7100_083f;
const MOV_W8_2: u32 = 0x5280_0048;
const CSEL_W10_WZR_W8_EQ: u32 = 0x1a88_03ea;
const CSEL_W10_W8_WZR_EQ: u32 = 0x1a9f_010a;
const MOV_W0_W8: u32 = 0x2a08_03e0;
const MOV_W0_W10: u32 = 0x2a0a_03e0;
const RET: u32 = 0xd65f_03c0;

const H: [u32; 5] = [CMP_W1_2, MOV_W8_2, CSEL_W10_WZR_W8_EQ, MOV_W0_W8, RET];
const K: [u32; 5] = [CMP_W1_2, MOV_W8_2, CSEL_W10_WZR_W8_EQ, MOV_W0_W10, RET];
const M: [u32; 5] = [CMP_W1_2, MOV_W8_2, CSEL_W10_W8_WZR_EQ, MOV_W0_W10, RET];

/// (b, h, k, m) as the hardware computes them.
const NATIVE: [(u64, u64, u64, u64); 3] = [(1, 2, 2, 0), (2, 2, 0, 2), (3, 2, 2, 0)];

fn check(interpret: bool) {
    let engine = if interpret { "interpreter" } else { "JIT" };
    for (b, h, k, m) in NATIVE {
        assert_eq!(call(&H, 1, b, interpret), h, "{engine}: h(1, {b})");
        assert_eq!(call(&K, 1, b, interpret), k, "{engine}: k(1, {b})");
        assert_eq!(call(&M, 1, b, interpret), m, "{engine}: m(1, {b})");
    }
}

#[test]
fn the_interpreter_matches_native_csel() {
    check(true);
}

#[test]
fn the_jit_matches_native_csel() {
    check(false);
}
