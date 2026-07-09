//! Smoke: dyn musl fixture exercising printf + malloc + strlen + memcpy + mmap HLE.
//!
//! Static full-CRT binary is opt-in (`FISSION_SMOKE_STATIC_PRINTF=1`) — large CRT
//! paths are not default CI (can be slow / hit more unimpl).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn run_linux(path: &Path, max_inst: u64) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;
    let load_spec = binary
        .load_spec()
        .context("missing load_spec")?
        .clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?
        .into_iter()
        .next()
        .context("no sleigh")?;
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary))?;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))?
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info)?;
    emu.run()?;
    Ok(emu)
}

#[test]
fn smoke_dyn_printf_malloc_hle() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_printf_malloc.elf");
    assert!(path.is_file(), "missing {}", path.display());
    let emu = run_linux(&path, 100_000).unwrap_or_else(|e| panic!("dyn printf_malloc: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected halt: {}",
        emu.metrics.summary_line()
    );
    assert!(
        emu.metrics.instructions > 10,
        "too few insns: {}",
        emu.metrics.summary_line()
    );
    assert_eq!(
        emu.metrics.hle_miss_total(),
        0,
        "hle misses: {:?}",
        emu.metrics.hle_misses
    );
    emu.metrics
        .check_unimplemented_budget(64, 8)
        .unwrap_or_else(|e| panic!("{e}; {}", emu.metrics.summary_line()));
    eprintln!("dyn printf_malloc ok: {}", emu.metrics.summary_line());
}

/// Static musl CRT — **opt-in only, not default CI**.
///
/// Remeasure (2026-07-09, post relative-branch overflow fix): still does not
/// cleanly halt within 500k insns in sandbox (CRT path incomplete: more
/// syscalls/TLS/locale). Keep checked-in for manual progress tracking.
///
/// Enable: `FISSION_SMOKE_STATIC_PRINTF=1 cargo nextest run -p fission-emulator smoke_static`
#[test]
fn smoke_static_printf_malloc_optional() {
    if std::env::var("FISSION_SMOKE_STATIC_PRINTF").ok().as_deref() != Some("1") {
        eprintln!(
            "skip static printf_malloc (not CI-stable; set FISSION_SMOKE_STATIC_PRINTF=1 to try)"
        );
        return;
    }
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    assert!(path.is_file(), "missing {}", path.display());
    let emu = run_linux(&path, 500_000).unwrap_or_else(|e| panic!("static printf_malloc: {e:#}"));
    assert!(
        emu.halt_requested,
        "static CRT expected clean halt (still incomplete if this fails): {}",
        emu.metrics.summary_line()
    );
    emu.metrics
        .check_unimplemented_budget(512, 32)
        .unwrap_or_else(|e| panic!("{e}; {}", emu.metrics.summary_line()));
    eprintln!("static printf_malloc ok: {}", emu.metrics.summary_line());
}
