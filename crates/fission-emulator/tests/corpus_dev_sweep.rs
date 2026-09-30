//! How much of our own dev corpus runs to completion.
//!
//! Ignored by default: it is a measurement over ~90 binaries and takes a
//! minute. Run it with
//!
//! ```text
//! cargo test --release -p fission-emulator --test corpus_dev_sweep -- --ignored --nocapture
//! ```
//!
//! # Why this corpus and no other
//!
//! Every binary here was compiled from sources in this repository. The
//! benchmark and evalkit corpora are not usable for this and never will be:
//! they contain malware compiled from source, and the rule for them is static
//! analysis only -- never executed, in an emulator least of all. Dynamic
//! analysis gets pointed at code we built ourselves.
//!
//! What it reports is a surface, not a pass mark: which binaries reach an exit
//! and which OS calls the rest are still waiting on. The misses are the work
//! queue -- that is how every stub in the Windows layer got written.

#[path = "common/hle_coverage.rs"]
mod hle_coverage;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{LinuxEnv, OsEnvironment, WindowsEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

/// Per binary. Two million was enough for programs written to exercise one
/// construct; duktape needs 2.2M to reach its prompt and exit, and at two
/// million it read as "did not exit" -- indistinguishable from the hang it
/// actually had, until the memset fix, a moment earlier.
const MAX_INST: u64 = 10_000_000;

/// Which binaries to sweep.
///
/// `FISSION_SWEEP_ROOT` re-points this. The dev corpus is programs we
/// compiled to exercise one construct each, which is the wrong shape for
/// asking "what is missing": a real program calls things our test programs
/// never do, and the missing-API tally is only as honest as the input.
///
/// Never point this at the DecBench/evalkit corpus. Those binaries include
/// malware compiled from source and are static-analysis-only -- this sweep
/// *executes* what it is given.
fn corpus() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("FISSION_SWEEP_ROOT") {
        let path = PathBuf::from(root);
        return path.is_dir().then_some(path);
    }
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries");
    path.is_dir().then_some(path)
}

/// What one binary did.
struct Outcome {
    report: hle_coverage::HleCoverageRecord,
    /// Opcodes the JIT encountered and lowered to nothing.
    unimplemented: Vec<(String, u64)>,
    /// CALLOTHER names no environment answered.
    unhandled_userops: Vec<(String, u64)>,
}

fn run_one(path: &Path, root: &Path) -> Outcome {
    let name = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let fail = |e: String| Outcome {
        report: hle_coverage::failed_record(&name, e),
        unimplemented: Vec::new(),
        unhandled_userops: Vec::new(),
    };

    let binary = match LoadedBinary::from_file(path) {
        Ok(b) => b,
        Err(e) => return fail(format!("load: {e}")),
    };
    let Some(load_spec) = binary.load_spec().cloned() else {
        return fail("no load spec".into());
    };
    let Ok(arch) = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
    else {
        return fail("arch".into());
    };
    let Ok(frontends) = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
    else {
        return fail("frontend".into());
    };
    let Some(sleigh) = frontends.into_iter().next() else {
        return fail("no frontend".into());
    };

    let mut state = MachineState::new();
    let is_pe = path.extension().is_some_and(|e| e == "exe");
    let guest_os = if is_pe { "windows" } else { "linux" };
    let env: Box<dyn OsEnvironment> = if is_pe {
        Box::new(WindowsEnv::new())
    } else {
        Box::new(LinuxEnv::new())
    };
    let image = if is_pe {
        fission_emulator::os::windows::loader::load_pe(&mut state, &binary).map(Ok)
    } else {
        fission_emulator::os::linux::loader::load_elf(&mut state, &binary).map(Err)
    };
    let image = match image {
        Ok(i) => i,
        Err(e) => return fail(format!("image: {e}")),
    };

    let mut emu = match Emulator::new(state, binary, sleigh, arch, env) {
        Ok(e) => e.with_max_inst(Some(MAX_INST)),
        Err(e) => return fail(format!("emulator: {e}")),
    };
    let applied = match image {
        Ok(pe) => emu.apply_windows_image(pe),
        Err(elf) => emu.apply_linux_image(elf),
    };
    if let Err(e) = applied {
        return fail(format!("apply: {e}"));
    }
    let error = emu.run().err().map(|e| format!("run: {e}"));
    emu.metrics.instructions = emu.inst_count;
    Outcome {
        report: hle_coverage::record_from_run(
            &name,
            guest_os,
            &emu.arch,
            emu.halt_requested,
            emu.exit_code,
            error,
            &emu.metrics,
        ),
        unimplemented: emu
            .metrics
            .unimplemented_opcodes
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        unhandled_userops: emu
            .metrics
            .unhandled_userops
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
    }
}

#[test]
#[ignore = "a measurement over the whole dev corpus, not an assertion"]
fn how_much_of_the_dev_corpus_runs() {
    let Some(root) = corpus() else {
        eprintln!("skipping: dev corpus not present");
        return;
    };

    let mut binaries: Vec<PathBuf> = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if is_candidate(&path) {
                binaries.push(path);
            }
        }
    }
    binaries.sort();

    let mut exited = 0usize;
    let mut clean = 0usize;
    let mut wanted: BTreeMap<String, u64> = BTreeMap::new();
    let mut unimplemented: BTreeMap<String, u64> = BTreeMap::new();
    let mut unhandled: BTreeMap<String, u64> = BTreeMap::new();
    let mut unknown_syscalls: BTreeMap<(String, u64), u64> = BTreeMap::new();
    let mut report_rows = Vec::new();
    for path in &binaries {
        let outcome = run_one(path, &root);
        let row = outcome.report;
        if row.process_status == "process_exit" {
            exited += 1;
            let has_unknown = row
                .syscalls
                .iter()
                .any(|syscall| syscall.unhandled_count > 0);
            if row.api_misses.is_empty() && !has_unknown {
                clean += 1;
            }
        }
        for miss in &row.api_misses {
            *wanted.entry(miss.name.clone()).or_default() += miss.count;
        }
        for (op, n) in &outcome.unimplemented {
            *unimplemented.entry(op.clone()).or_default() += n;
        }
        for (op, n) in &outcome.unhandled_userops {
            *unhandled.entry(op.clone()).or_default() += n;
        }
        for syscall in &row.syscalls {
            if syscall.unhandled_count > 0 {
                *unknown_syscalls
                    .entry((syscall.name.clone(), syscall.guest_number))
                    .or_default() += syscall.unhandled_count;
            }
        }
        eprintln!(
            "  {:<56} {:<34} inst={:<9} {} (exit={:?}, unknown={})",
            row.binary,
            row.guest_abi,
            row.instructions,
            row.process_status,
            row.exit_code,
            row.syscalls
                .iter()
                .map(|syscall| syscall.unhandled_count)
                .sum::<u64>()
        );
        report_rows.push(row);
    }

    eprintln!(
        "\n{exited} of {} exited, {clean} of those with no unimplemented syscall or API",
        binaries.len()
    );
    if unimplemented.is_empty() {
        eprintln!("no p-code opcode was lowered to nothing");
    } else {
        eprintln!("opcodes lowered to nothing (each one is a wrong answer):");
        let mut ranked: Vec<_> = unimplemented.into_iter().collect();
        ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (op, n) in &ranked {
            eprintln!("  {n:>6}  {op}");
        }
    }
    if unhandled.is_empty() {
        eprintln!("every CALLOTHER reached was answered");
    } else {
        eprintln!("CALLOTHERs answered with a zero (each one is a wrong value):");
        let mut ranked: Vec<_> = unhandled.into_iter().collect();
        ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for (op, n) in ranked.iter().take(15) {
            eprintln!("  {n:>6}  {op}");
        }
    }
    if unknown_syscalls.is_empty() {
        eprintln!("every syscall reached had a handler");
    } else {
        eprintln!("syscalls with no handler:");
        let mut ranked: Vec<_> = unknown_syscalls.into_iter().collect();
        ranked.sort_by(|(a, count_a), (b, count_b)| count_b.cmp(count_a).then_with(|| a.cmp(b)));
        for ((name, num), n) in ranked.iter().take(15) {
            eprintln!("  {n:>6}  {name} (guest #{num})");
        }
    }
    eprintln!("APIs the rest are waiting on, most wanted first:");
    let mut ranked: Vec<_> = wanted.into_iter().collect();
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (api, n) in ranked.iter().take(25) {
        eprintln!("  {n:>3}  {api}");
    }

    let report = hle_coverage::HleCoverageReport {
        schema_version: 1,
        binaries: report_rows,
    };
    if let Some(path) = std::env::var_os("FISSION_HLE_REPORT_PATH") {
        let json = serde_json::to_vec_pretty(&report).expect("serialize HLE coverage report");
        std::fs::write(&path, json).expect("write requested HLE coverage report");
        eprintln!(
            "wrote deterministic HLE coverage report to {}",
            Path::new(&path).display()
        );
    }
}

fn is_candidate(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some("exe") => true,
        // ELFs in this corpus carry no extension.
        None => std::fs::read(path)
            .ok()
            .is_some_and(|b| b.starts_with(b"\x7fELF")),
        _ => false,
    }
}
