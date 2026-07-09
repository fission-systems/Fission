//! PE CRT/minimal path smokes: ExitProcess + WriteFile fixtures.

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::WindowsEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture_pe(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata").join(name)
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
    let path = fixture_pe("win_x64_exit.exe");
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
    assert!(
        emu.metrics.userops.keys().any(|k| k.contains("ExitProcess")),
        "expected ExitProcess HLE, userops={:?}",
        emu.metrics.userops
    );
    eprintln!("pe exit smoke ok: {}", emu.metrics.summary_line());
}

#[test]
fn smoke_pe_write_file() {
    let path = fixture_pe("win_x64_write.exe");
    assert!(path.is_file(), "missing PE WriteFile fixture {}", path.display());
    let emu = run_pe(&path, 10_000).unwrap_or_else(|e| panic!("PE WriteFile smoke failed: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected ExitProcess after WriteFile, metrics={}",
        emu.metrics.summary_line()
    );
    if let Err(msg) = emu.metrics.check_unimplemented_budget(64, 8) {
        panic!("{msg}; full={}", emu.metrics.summary_line());
    }
    let saw_write = emu
        .metrics
        .userops
        .keys()
        .any(|k| k.contains("WriteFile") || k.contains("WriteConsole"));
    let saw_std = emu
        .metrics
        .userops
        .keys()
        .any(|k| k.contains("GetStdHandle"));
    assert!(
        saw_write || saw_std,
        "expected WriteFile/GetStdHandle HLE, userops={:?}",
        emu.metrics.userops
    );
    eprintln!("pe write smoke ok: {}", emu.metrics.summary_line());
}
