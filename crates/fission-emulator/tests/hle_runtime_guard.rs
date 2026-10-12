//! JIT execution of the supported HLE runtime guard and its failure path.
use fission_emulator::os::linux::{DynlinkMode, ProcessArgs, loader};
use fission_emulator::{MachineState, arch::ArchInfo, core::Emulator, os::LinuxEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::path::PathBuf;

fn run(arg: &str) -> Emulator {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_hle_guard.elf");
    let binary = LoadedBinary::from_file(&path).unwrap();
    let mut state = MachineState::new();
    let info = loader::load_elf_with_args(
        &mut state,
        &binary,
        &ProcessArgs {
            argv: vec!["guard-fixture".into(), arg.into()],
            envp: vec![],
        },
    )
    .unwrap();
    assert_eq!(info.dynlink.mode, DynlinkMode::HleGot);
    let spec = binary.load_spec().unwrap().clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&spec)
        .unwrap()
        .remove(0);
    let arch = ArchInfo::from_language_id(spec.pair.language_id.as_str(), Some(&binary)).unwrap();
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .unwrap()
        .with_max_inst(Some(1000));
    emu.apply_linux_image(info).unwrap();
    emu.run().unwrap();
    assert!(emu.halt_requested);
    assert_eq!(emu.metrics.hle_miss_total(), 0);
    assert_eq!(emu.metrics.unknown_syscalls.values().sum::<u64>(), 0);
    emu.metrics.check_unimplemented_budget(0, 0).unwrap();
    assert_ne!(emu.fs_base, 0);
    emu
}

#[test]
fn jit_preserves_guard_and_main_verdict() {
    let emu = run("normal");
    assert_eq!(emu.exit_code, Some(7));
}

#[test]
fn jit_guard_corruption_cannot_resume_after_failure() {
    let emu = run("!");
    assert_eq!(emu.exit_code, Some(134));
}

#[test]
fn console_capture_preserves_bytes_and_reports_unavailable_streams() {
    use fission_emulator::os::vfs::SimVFS;
    use fission_emulator::{EmulatorMetrics, SandboxMetricsReport};
    let mut vfs = SimVFS::new();
    vfs.write(1, &[0, 255, 10]).unwrap();
    vfs.write(2, b"diagnostic").unwrap();
    let report = SandboxMetricsReport::from_run(
        "fixture",
        "ELF64",
        true,
        0,
        EmulatorMetrics::default(),
        None,
    )
    .with_guest_output(&vfs);
    let output = report.guest_output.as_ref().unwrap();
    assert_eq!(output.stdout.as_deref(), Some(&[0, 255, 10][..]));
    assert_eq!(output.stderr.as_deref(), Some(b"diagnostic".as_slice()));
    assert!(output.stdout_error.is_none());
    let json = report.to_json_pretty().unwrap();
    let restored: SandboxMetricsReport = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.guest_output, report.guest_output);
    vfs.files.remove(&1);
    let report = SandboxMetricsReport::from_run(
        "fixture",
        "ELF64",
        true,
        0,
        EmulatorMetrics::default(),
        None,
    )
    .with_guest_output(&vfs);
    let output = report.guest_output.unwrap();
    assert!(output.stdout.is_none());
    assert!(output.stdout_error.is_some());
    assert_eq!(output.stderr.as_deref(), Some(b"diagnostic".as_slice()));
}
