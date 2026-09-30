//! AArch64 ET_DYN image: apply RELATIVE, route imported `puts` through HLE, exit.

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/aarch64_dyn_import.elf")
}

fn run_dyn_aarch64(path: &std::path::Path) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    anyhow::ensure!(
        binary
            .inner()
            .iat_symbols
            .values()
            .any(|name| name == "puts"),
        "fixture must expose the imported puts slot"
    );
    let mut state = MachineState::new();
    let image = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;
    anyhow::ensure!(
        image.dynlink.mode == fission_emulator::os::linux::dynlink::DynlinkMode::HleGot,
        "AArch64 fixture must use the HLE GOT path"
    );
    let load_spec = binary.load_spec().context("missing load_spec")?.clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?
        .into_iter()
        .next()
        .context("no Sleigh frontend for AArch64 fixture")?;
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))?;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))?
        .with_max_inst(Some(10_000));
    emu.apply_linux_image(image)?;
    emu.run()?;
    Ok(emu)
}

#[test]
fn aarch64_dynamic_import_runs_through_hle() {
    let path = fixture();
    assert!(path.is_file(), "missing {}", path.display());
    let emu = run_dyn_aarch64(&path)
        .unwrap_or_else(|error| panic!("AArch64 dynamic HLE smoke failed: {error:#}"));
    assert!(
        emu.halt_requested,
        "expected AArch64 guest exit, metrics={}",
        emu.metrics.summary_line()
    );
    assert!(
        emu.metrics.instructions > 5,
        "too few AArch64 instructions: {}",
        emu.metrics.summary_line()
    );
    assert_eq!(
        emu.metrics.hle_misses.get("puts").copied().unwrap_or(0),
        0,
        "imported puts must resolve to the Linux HLE procedure"
    );
    assert_eq!(
        emu.metrics.unknown_syscall_total(),
        0,
        "AArch64 exit syscall must be recognized"
    );
}
