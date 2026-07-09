//! End-to-end smoke against the checked-in minimal ELF fixture (no libc).
//!
//! Optional larger binaries: set `FISSION_SMOKE_ELF=/path/to/binary` explicitly.
//! Auto-discovery of `/tmp` paths is intentionally avoided so local nextest stays bounded.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn fixture_elf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf")
}

/// Opt-in only: never auto-pick developer `/tmp` trees (those can hang for minutes).
fn optional_large_elf() -> Option<PathBuf> {
    let p = std::env::var("FISSION_SMOKE_ELF").ok()?;
    let pb = PathBuf::from(p);
    pb.is_file().then_some(pb)
}

struct SmokeExpect {
    max_inst: u64,
    require_write: bool,
    /// Strict zero-gap budget for the tiny CI fixture; looser for large libc paths.
    max_unimpl_events: u64,
    max_unimpl_kinds: usize,
}

fn run_binary(path: &Path, expect: SmokeExpect) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;

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
    let os = Box::new(LinuxEnv::new());

    let mut emu = Emulator::new(state, binary, sleigh, arch, os)?.with_max_inst(Some(expect.max_inst));
    emu.apply_linux_image(info)?;
    emu.run()?;

    assert!(
        emu.halt_requested,
        "expected clean halt, metrics={}",
        emu.metrics.summary_line()
    );
    emu.metrics
        .check_unimplemented_budget(expect.max_unimpl_events, expect.max_unimpl_kinds)
        .map_err(|e| anyhow::anyhow!("{e}; full={}", emu.metrics.summary_line()))?;
    if expect.require_write {
        assert!(
            emu.metrics.syscalls.get(&1).copied().unwrap_or(0) >= 1,
            "expected write syscall, got {:?}",
            emu.metrics.syscalls
        );
    }
    assert!(
        emu.metrics.syscalls.contains_key(&60) || emu.metrics.syscalls.contains_key(&231),
        "expected exit/exit_group, got {:?}",
        emu.metrics.syscalls
    );
    eprintln!(
        "smoke ok ({}): {}",
        path.display(),
        emu.metrics.summary_line()
    );
    Ok(emu)
}

#[test]
fn smoke_ci_fixture_hello_sys() {
    let path = fixture_elf();
    assert!(
        path.is_file(),
        "missing checked-in fixture {}",
        path.display()
    );
    run_binary(
        &path,
        SmokeExpect {
            max_inst: 64,
            require_write: true,
            max_unimpl_events: 0,
            max_unimpl_kinds: 0,
        },
    )
    .unwrap_or_else(|e| panic!("fixture smoke failed: {e:#}"));
}

/// Large musl/CRT hello — opt-in via `FISSION_SMOKE_ELF`. Not part of default CI.
#[test]
fn smoke_optional_musl_hello() {
    let Some(path) = optional_large_elf() else {
        eprintln!("skip optional musl hello (set FISSION_SMOKE_ELF=/path/to/elf to enable)");
        return;
    };
    run_binary(
        &path,
        SmokeExpect {
            max_inst: 50_000,
            require_write: true,
            // Large CRT paths still exercise unimplemented ops; gate is soft.
            max_unimpl_events: 256,
            max_unimpl_kinds: 16,
        },
    )
    .unwrap_or_else(|e| panic!("musl smoke failed: {e:#}"));
}
