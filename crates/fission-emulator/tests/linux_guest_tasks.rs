use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_guest_futex.elf")
}

fn run_guest(path: &Path) -> Result<Emulator> {
    let binary = LoadedBinary::from_file(path)?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;
    let load_spec = binary.load_spec().context("missing load_spec")?.clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?
        .into_iter()
        .next()
        .context("no Sleigh frontend")?;
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))?;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))?
        .with_max_inst(Some(20_000));
    emu.apply_linux_image(info)?;
    emu.run()?;
    Ok(emu)
}

#[test]
fn cloned_guest_waits_for_a_shared_futex_wake() {
    let path = fixture();
    assert!(path.is_file(), "missing {}", path.display());
    let emu =
        run_guest(&path).unwrap_or_else(|error| panic!("guest task fixture failed: {error:#}"));

    assert!(
        emu.halt_requested,
        "expected clean halt: {}",
        emu.metrics.summary_line()
    );
    assert_eq!(emu.exit_code, Some(0), "{}", emu.metrics.summary_line());
    assert_eq!(emu.metrics.exit_reason.as_deref(), Some("halt"));
    assert_eq!(
        emu.metrics.syscalls.get(&56),
        Some(&1),
        "clone not observed"
    );
    assert_eq!(
        emu.metrics.syscalls.get(&202),
        Some(&2),
        "wait and wake not observed"
    );
    assert_eq!(
        emu.metrics.syscalls.get(&60),
        Some(&2),
        "both guest tasks must exit"
    );
}
