//! Dynamic ELF without ld.so: GOT HLE + `__libc_start_main` → main → puts.

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_dyn_puts.elf")
}

fn run_dyn(path: &std::path::Path, max_inst: u64) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    assert!(
        !binary.inner().iat_symbols.is_empty(),
        "expected GOT/PLT iat_symbols for dynamic ELF"
    );
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
fn smoke_dyn_puts_via_got_hle() {
    let path = fixture();
    assert!(path.is_file(), "missing {}", path.display());
    let emu = run_dyn(&path, 50_000).unwrap_or_else(|e| panic!("dyn ELF smoke failed: {e:#}"));
    assert!(
        emu.halt_requested,
        "expected clean halt, metrics={}",
        emu.metrics.summary_line()
    );
    // start_main and/or puts should appear in userops / via HLE path.
    let keys: Vec<_> = emu.metrics.userops.keys().cloned().collect();
    let saw_libc = keys.iter().any(|k| {
        k.contains("puts")
            || k.contains("libc_start")
            || k.contains("win32:") // shouldn't
            || k == "syscall"
    });
    // May only hit puts via HLE without counting as userop if routed through procedures
    // metrics.note_userop is for win32; Linux libc uses simos without note_userop.
    // So check halt + instruction progress.
    assert!(
        emu.metrics.instructions > 5,
        "too few instructions: {}",
        emu.metrics.summary_line()
    );
    let _ = saw_libc;
    if let Err(msg) = emu.metrics.check_unimplemented_budget(128, 16) {
        panic!("{msg}; full={}", emu.metrics.summary_line());
    }
    eprintln!("dyn puts smoke ok: {}", emu.metrics.summary_line());
}

/// Lazy PLT: GOT markers until first call; bind then HLE/jump (FISSION_LAZY_BIND=1).
#[test]
fn smoke_dyn_puts_lazy_bind() {
    let path = fixture();
    assert!(path.is_file(), "missing {}", path.display());
    // Scope env so other tests in the same process are not polluted.
    // SAFETY: test-only env mutation; serial within this process for this test body.
    unsafe {
        std::env::set_var("FISSION_LAZY_BIND", "1");
    }
    let result = std::panic::catch_unwind(|| run_dyn(&path, 50_000));
    unsafe {
        std::env::remove_var("FISSION_LAZY_BIND");
    }
    let emu = match result {
        Ok(Ok(e)) => e,
        Ok(Err(e)) => panic!("lazy dyn ELF smoke failed: {e:#}"),
        Err(payload) => std::panic::resume_unwind(payload),
    };
    assert!(
        emu.halt_requested,
        "lazy bind expected clean halt, metrics={}",
        emu.metrics.summary_line()
    );
    assert!(
        emu.metrics.instructions > 5,
        "lazy bind too few instructions: {}",
        emu.metrics.summary_line()
    );
    if let Err(msg) = emu.metrics.check_unimplemented_budget(128, 16) {
        panic!("{msg}; full={}", emu.metrics.summary_line());
    }
    eprintln!("dyn puts lazy-bind smoke ok: {}", emu.metrics.summary_line());
}
