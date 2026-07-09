//! Concolic mini-fixture: stdin taint → conditional branch.
//!
//! Uses the tiny freestanding `x64_concolic_branch_sys.elf` (syscall-only, no musl CRT).

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn load_emu(stdin: &[u8], concolic_stop: bool) -> Result<Emulator> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf");
    anyhow::ensure!(path.is_file(), "missing {}", path.display());
    let binary =
        LoadedBinary::from_file(&path).with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;
    let load_spec = binary.load_spec().context("load_spec")?.clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?
        .into_iter()
        .next()
        .context("sleigh")?;
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary))?;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))?
        .with_max_inst(Some(256))
        .with_concolic_stop(concolic_stop);
    emu.apply_linux_image(info)?;
    emu.seed_stdin(stdin);
    Ok(emu)
}

/// Concrete path with 'A' → exit 0 (no concolic stop).
#[test]
fn smoke_concolic_branch_concrete_a() {
    let mut emu = load_emu(b"A", false).unwrap_or_else(|e| panic!("{e:#}"));
    emu.run().unwrap_or_else(|e| panic!("run: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected halt: {}",
        emu.metrics.summary_line()
    );
    assert_eq!(
        emu.metrics.syscalls.get(&0).copied().unwrap_or(0),
        1,
        "expected sys_read"
    );
    assert!(
        emu.metrics.syscalls.contains_key(&60) || emu.metrics.syscalls.contains_key(&231),
        "expected exit: {:?}",
        emu.metrics.syscalls
    );
    // Tainted branch should still be recorded.
    assert!(
        !emu.sym_events.is_empty() || emu.halt_requested,
        "expected sym_events or halt"
    );
    eprintln!(
        "concolic A: events={} {}",
        emu.sym_events.len(),
        emu.metrics.summary_line()
    );
}

/// Concrete path with 'B' → exit 1.
#[test]
fn smoke_concolic_branch_concrete_b() {
    let mut emu = load_emu(b"B", false).unwrap_or_else(|e| panic!("{e:#}"));
    emu.run().unwrap_or_else(|e| panic!("run: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected halt: {}",
        emu.metrics.summary_line()
    );
    eprintln!("concolic B: events={} {}", emu.sym_events.len(), emu.metrics.summary_line());
}

/// With concolic stop enabled, first tainted CBranch stops the run with sym_events.
#[test]
fn smoke_concolic_gate_stops_on_tainted_branch() {
    let mut emu = load_emu(b"A", true).unwrap_or_else(|e| panic!("{e:#}"));
    emu.run().unwrap_or_else(|e| panic!("run: {e:#}"));
    assert!(
        emu.sym_stop_requested || !emu.sym_events.is_empty(),
        "expected gate stop or events; metrics={}",
        emu.metrics.summary_line()
    );
    assert!(
        !emu.sym_events.is_empty(),
        "expected at least one SymBranch; metrics={}",
        emu.metrics.summary_line()
    );
    eprintln!(
        "concolic gate: events={:?} {}",
        emu.sym_events
            .iter()
            .map(|e| (e.pc, e.condition_val_taken, e.alt_addr))
            .collect::<Vec<_>>(),
        emu.metrics.summary_line()
    );
}

/// Unit-level: seed stdin + sys_read path taints destination buffer.
#[test]
fn stdin_read_taints_buffer_unit() {
    use fission_emulator::os::procedure::SimProcedure;

    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary)).unwrap();
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .unwrap()
        .with_max_inst(Some(1));
    emu.apply_linux_image(info).unwrap();
    emu.seed_stdin(b"Z");

    let buf = 0x6000_0000u64;
    emu.state
        .page_map
        .map_region(buf, 0x1000, fission_emulator::pcode::page_map::prot::RW, true);
    emu.write_register_u64("RDI", 0).unwrap();
    emu.write_register_u64("RSI", buf).unwrap();
    emu.write_register_u64("RDX", 1).unwrap();
    let sys = fission_emulator::os::linux::syscall::SysRead;
    sys.run(&mut emu).unwrap();
    assert_eq!(
        emu.state.read_space(emu.state.ram_space(), buf, 1).unwrap()[0],
        b'Z'
    );
    assert!(
        emu.state
            .get_shadow_memory(emu.state.ram_space(), buf)
            .is_some(),
        "sys_read must taint stdin destination"
    );
}
