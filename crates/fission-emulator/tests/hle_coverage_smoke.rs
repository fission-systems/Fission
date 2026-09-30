#[path = "common/hle_coverage.rs"]
mod hle_coverage;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    binaries: Vec<ManifestBinary>,
}

#[derive(Debug, Deserialize)]
struct ManifestBinary {
    file: String,
    guest_os: String,
    guest_abi: String,
    max_instructions: u64,
    max_unknown_syscalls: u64,
    max_hle_misses: u64,
}

fn manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/hle_smoke_manifest.json")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join(name)
}

fn run_case(path: &Path, spec: &ManifestBinary) -> Result<Emulator> {
    let binary =
        LoadedBinary::from_file(path).with_context(|| format!("load {}", path.display()))?;
    let mut state = MachineState::new();
    let args = fission_emulator::os::linux::ProcessArgs {
        argv: vec![path.to_string_lossy().into_owned()],
        ..fission_emulator::os::linux::ProcessArgs::default()
    };
    let image =
        fission_emulator::os::linux::loader::load_elf_with_args(&mut state, &binary, &args)?;
    let load_spec = binary.load_spec().context("missing load spec")?.clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)?
        .into_iter()
        .next()
        .context("missing Sleigh frontend")?;
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))?;
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))?
        .with_max_inst(Some(spec.max_instructions));
    emu.apply_linux_image(image)?;
    emu.run()?;
    Ok(emu)
}

#[test]
fn bounded_hle_smoke_manifest_reports_and_budgets_each_guest() {
    let manifest: Manifest = serde_json::from_slice(
        &std::fs::read(manifest_path()).expect("read checked-in HLE smoke manifest"),
    )
    .expect("parse checked-in HLE smoke manifest");
    assert_eq!(manifest.schema_version, 1);
    assert!(!manifest.binaries.is_empty());

    let mut rows = Vec::new();
    for spec in &manifest.binaries {
        let path = fixture_path(&spec.file);
        assert!(path.is_file(), "missing fixture {}", path.display());
        let mut emu =
            run_case(&path, spec).unwrap_or_else(|error| panic!("{} failed: {error:#}", spec.file));
        assert!(
            emu.halt_requested,
            "{} did not terminate: {}",
            spec.file,
            emu.metrics.summary_line()
        );
        assert_eq!(emu.exit_code, Some(0), "{} exit code", spec.file);
        assert!(
            emu.inst_count <= spec.max_instructions,
            "{} exceeded instruction budget: {} > {}",
            spec.file,
            emu.inst_count,
            spec.max_instructions
        );
        emu.metrics
            .check_hle_budget(spec.max_hle_misses, spec.max_unknown_syscalls)
            .unwrap_or_else(|error| panic!("{}: {error}", spec.file));

        let row = hle_coverage::record_from_run(
            &spec.file,
            &spec.guest_os,
            &emu.arch,
            emu.halt_requested,
            emu.exit_code,
            None,
            &emu.metrics,
        );
        assert_eq!(row.guest_abi, spec.guest_abi, "{} ABI", spec.file);
        assert_eq!(row.process_status, "process_exit", "{} status", spec.file);
        if spec.file == "linux_aarch64_readlinkat.elf" {
            assert_readlink_output(&mut emu);
            let readlink = row
                .syscalls
                .iter()
                .find(|syscall| syscall.guest_number == 78)
                .expect("readlinkat in coverage row");
            assert_eq!(readlink.name, "readlinkat");
            assert_eq!(readlink.unhandled_count, 0);
        }
        rows.push(row);
    }
    rows.sort_by(|a, b| a.binary.cmp(&b.binary));
    let report = hle_coverage::HleCoverageReport {
        schema_version: 1,
        binaries: rows,
    };
    let json = serde_json::to_string_pretty(&report).expect("serialize deterministic HLE report");
    println!("HLE_COVERAGE_REPORT_BEGIN\n{json}\nHLE_COVERAGE_REPORT_END");
    if let Some(path) = std::env::var_os("FISSION_HLE_REPORT_PATH") {
        std::fs::write(path, &json).expect("write requested HLE coverage report");
    }
}

fn assert_readlink_output(emu: &mut Emulator) {
    for number in [78, 94] {
        assert_eq!(
            emu.metrics
                .guest_syscalls
                .get(&number)
                .copied()
                .unwrap_or(0),
            1,
            "expected one guest syscall {number}: {}",
            emu.metrics.summary_line()
        );
    }
    assert_eq!(emu.metrics.unknown_syscall_total(), 0);
    let execfn = emu
        .image_info
        .as_ref()
        .expect("Linux image information")
        .execfn
        .clone();
    let basename = Path::new(&execfn)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("guest");
    let expected = format!("/fission-guest/{basename}").into_bytes();
    assert_eq!(
        read_symbol_u64(emu, "readlink_result"),
        expected.len() as u64
    );
    assert_eq!(
        read_symbol(emu, "readlink_buffer", expected.len()),
        expected
    );
}

fn read_symbol(emu: &mut Emulator, symbol: &str, size: usize) -> Vec<u8> {
    let address = emu
        .binary
        .inner()
        .global_symbols
        .iter()
        .find_map(|(&address, name)| (name == symbol).then_some(address))
        .unwrap_or_else(|| panic!("fixture symbol {symbol} missing"));
    let ram = emu.state.ram_space();
    emu.state
        .read_space(ram, address, size)
        .unwrap_or_else(|error| panic!("read fixture symbol {symbol}: {error}"))
}

fn read_symbol_u64(emu: &mut Emulator, symbol: &str) -> u64 {
    u64::from_le_bytes(
        read_symbol(emu, symbol, 8)
            .try_into()
            .expect("eight-byte symbol value"),
    )
}
