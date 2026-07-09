//! Optional end-to-end smoke: run a static Linux x86_64 hello binary in the sandbox.
//!
//! Skipped when the binary is absent (CI without fixtures). Produce one with:
//!
//! ```bash
//! zig cc -target x86_64-linux-musl -O0 -o /tmp/fission-emu-test/hello_linux_x64 hello.c
//! export FISSION_SMOKE_ELF=/tmp/fission-emu-test/hello_linux_x64
//! cargo nextest run -p fission-emulator smoke_linux_hello
//! ```

use std::path::PathBuf;

use anyhow::{Context, Result};
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn smoke_elf_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FISSION_SMOKE_ELF") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let default = PathBuf::from("/tmp/fission-emu-test/hello_linux_x64");
    if default.is_file() {
        Some(default)
    } else {
        None
    }
}

fn run_smoke(path: &std::path::Path) -> Result<()> {
    let binary = LoadedBinary::from_file(path)
        .with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary)?;

    let load_spec = binary
        .load_spec()
        .context("binary missing load_spec")?
        .clone();
    let frontends =
        RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?;
    let sleigh = frontends
        .into_iter()
        .next()
        .context("no Sleigh frontend")?;

    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary))
        .with_context(|| format!("arch {lang_id}"))?;
    let os = Box::new(LinuxEnv::new());

    let mut emu = Emulator::new(state, binary, sleigh, arch, os)?
        .with_max_inst(Some(50_000));
    emu.apply_linux_image(info)?;
    emu.run()?;

    assert!(
        emu.halt_requested || emu.metrics.exit_reason.as_deref() == Some("halt"),
        "expected clean halt, metrics={}",
        emu.metrics.summary_line()
    );
    assert!(
        emu.inst_count > 100 && emu.inst_count < 50_000,
        "unexpected inst_count={}",
        emu.inst_count
    );
    // write(1,...) and exit should have been seen on a normal hello.
    assert!(
        emu.metrics.syscalls.contains_key(&1) || emu.metrics.syscalls.contains_key(&60),
        "expected write and/or exit syscalls, got {:?}",
        emu.metrics.syscalls
    );
    eprintln!("smoke ok: {}", emu.metrics.summary_line());
    Ok(())
}

#[test]
fn smoke_linux_static_hello() {
    let Some(path) = smoke_elf_path() else {
        eprintln!("skip smoke_linux_static_hello: set FISSION_SMOKE_ELF or build /tmp/fission-emu-test/hello_linux_x64");
        return;
    };
    run_smoke(&path).unwrap_or_else(|e| panic!("smoke failed: {e:#}"));
}
