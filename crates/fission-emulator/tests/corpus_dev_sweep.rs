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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::{LinuxEnv, OsEnvironment, WindowsEnv};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

const MAX_INST: u64 = 2_000_000;

fn corpus() -> Option<PathBuf> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries");
    path.is_dir().then_some(path)
}

/// What one binary did.
struct Outcome {
    halted: bool,
    exit_reason: Option<String>,
    inst: u64,
    misses: Vec<String>,
    /// Opcodes an engine met and lowered to nothing. A run with any of these
    /// produced a wrong answer somewhere, silently.
    unimplemented: Vec<(String, u64)>,
    error: Option<String>,
}

fn run_one(path: &Path) -> Outcome {
    let fail = |e: String| Outcome {
        halted: false,
        exit_reason: None,
        inst: 0,
        misses: Vec::new(),
        unimplemented: Vec::new(),
        error: Some(e),
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

    match emu.run() {
        Ok(()) => Outcome {
            halted: emu.halt_requested,
            exit_reason: emu.metrics.exit_reason.clone(),
            inst: emu.inst_count,
            misses: emu.metrics.hle_misses.keys().cloned().collect(),
            unimplemented: emu
                .metrics
                .unimplemented_opcodes
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            error: None,
        },
        Err(e) => Outcome {
            halted: emu.halt_requested,
            exit_reason: emu.metrics.exit_reason.clone(),
            inst: emu.inst_count,
            misses: emu.metrics.hle_misses.keys().cloned().collect(),
            unimplemented: emu
                .metrics
                .unimplemented_opcodes
                .iter()
                .map(|(k, v)| (k.clone(), *v))
                .collect(),
            error: Some(format!("run: {e}")),
        },
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
    let mut stack = vec![root];
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

    let mut halted = 0usize;
    let mut clean = 0usize;
    let mut wanted: BTreeMap<String, usize> = BTreeMap::new();
    let mut unimplemented: BTreeMap<String, u64> = BTreeMap::new();
    for path in &binaries {
        let outcome = run_one(path);
        if outcome.halted {
            halted += 1;
            if outcome.misses.is_empty() {
                clean += 1;
            }
        }
        for miss in &outcome.misses {
            *wanted.entry(miss.clone()).or_default() += 1;
        }
        for (op, n) in &outcome.unimplemented {
            *unimplemented.entry(op.clone()).or_default() += n;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        let status = match (&outcome.error, outcome.halted) {
            (Some(e), _) => e.clone(),
            (None, true) => format!("halted ({})", outcome.exit_reason.as_deref().unwrap_or("-")),
            (None, false) => format!(
                "did not exit ({})",
                outcome.exit_reason.as_deref().unwrap_or("-")
            ),
        };
        eprintln!("  {name:<48} inst={:<9} {status}", outcome.inst);
    }

    eprintln!(
        "\n{halted} of {} halted, {clean} of those with no unimplemented API",
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
    eprintln!("APIs the rest are waiting on, most wanted first:");
    let mut ranked: Vec<_> = wanted.into_iter().collect();
    ranked.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (api, n) in ranked.iter().take(25) {
        eprintln!("  {n:>3}  {api}");
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
