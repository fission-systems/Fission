//! PE CRT/minimal path smoke: checked-in `win_x64_exit.exe` calls ExitProcess(0).

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::WindowsEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture_pe() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/win_x64_exit.exe")
}

fn run_pe(path: &std::path::Path, max_inst: u64) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::windows::loader::load_pe(&mut state, &binary)?;

    let load_spec = binary
        .load_spec()
        .context("binary missing load_spec")?
        .clone();
    let frontends = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?;
    let sleigh = frontends
        .into_iter()
        .next()
        .context("no Sleigh frontend")?;

    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary))
        .with_context(|| format!("arch {lang_id}"))?;
    let os = Box::new(WindowsEnv::new());

    let mut emu = Emulator::new(state, binary, sleigh, arch, os)?.with_max_inst(Some(max_inst));
    emu.apply_windows_image(info)?;
    emu.run()?;
    Ok(emu)
}

#[test]
fn smoke_pe_exit_process() {
    let path = fixture_pe();
    assert!(path.is_file(), "missing PE fixture {}", path.display());
    let emu = run_pe(&path, 5_000).unwrap_or_else(|e| panic!("PE smoke failed: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected ExitProcess halt, metrics={}",
        emu.metrics.summary_line()
    );
    // PE path may still hit a few unimplemented ops; keep a loose budget for now.
    if let Err(msg) = emu.metrics.check_unimplemented_budget(64, 8) {
        panic!("{msg}; full={}", emu.metrics.summary_line());
    }
    eprintln!("pe smoke ok: {}", emu.metrics.summary_line());
}
