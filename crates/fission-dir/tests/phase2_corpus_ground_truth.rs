//! Ground truth across the corpus, not one function.
//!
//! Runs [`phase2_ground_truth`]'s check -- evaluate the decompiled body, run
//! the real machine code, compare -- over many corpus functions instead of one
//! hand-picked case.
//!
//! # It does not yet guard structuring, and the reason is now specific
//!
//! It was built to, on the reasoning that structuring is semantics-preserving
//! by definition and so should be checkable by execution. Two sabotages were
//! injected to test that -- `while` emitted where `do`/`while` was correct,
//! and **every `if` condition in every function negated** -- and neither was
//! caught.
//!
//! An earlier version of this file blamed the concrete tier's lack of a memory
//! model, and concluded the groundable functions were "close to exactly the
//! functions structuring does not affect". **That was wrong, and measuring it
//! properly says something more useful.** A memory model was added
//! (`eval::Memory`) and coverage did not move at all. Of the functions that do
//! ground, a third *do* contain loops and conditionals -- `list_sum` grounds
//! with one `if` and two loops.
//!
//! What stops the sabotage being caught is the **arguments**. `default_samples`
//! offers integers -- `0`, `1`, `-1`, `i32::MIN` -- and a function like
//! `list_sum` takes a pointer. Every sample but the null one fails the
//! emulator call outright, and the null one returns before the loop body ever
//! runs, so negating its guard changes nothing observable. `list_sum` grounds
//! on exactly one sample of seven.
//!
//! **The prerequisite is a buffer that exists at the same address in both the
//! emulator and `eval::Memory`, passed as the pointer argument.** The
//! interpreter half of that is what the memory model is for; the emulator half
//! is not built. Until it is, `structuring_quality` and the unit tests remain
//! the only guard on structuring correctness, covering only failures already
//! seen.
//!
//! # What it does cover
//!
//! The arithmetic and type pipeline against real machine code, on the corpus's
//! own functions rather than one. Getting it running turned up two real
//! defects in the checker itself (see `ground_truth.rs` and `eval.rs`), which
//! is the usual return on actually executing something.

use fission_dir::report::VerifyOutcome;
use fission_dir::{EmulatorHarness, check_ground_truth, decompile_one, default_samples};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fission-benchmark/corpus/dev/binaries/c")
}

/// Emulator budget per call. Generous enough for the corpus's loops, small
/// enough that a runaway body ends as `Unsupported` rather than hanging.
const STEP_BUDGET: u64 = 200_000;

/// Arity ceiling. `default_samples` is the cartesian product of seven seeds,
/// so each parameter multiplies the emulator calls by seven -- two is 49 for
/// one function, three is 343.
const MAX_ARITY: usize = 2;

/// How many functions to *attempt* before stopping, so this fits a normal test
/// run. Attempts, not successes: decompiling is what costs, and most corpus
/// functions turn out not to be groundable at this tier, so budgeting
/// successes lets the unbounded majority set the runtime.
///
/// Selection is deterministic (binaries sorted by path, functions by address),
/// so the same set is checked every run and a regression cannot hide behind
/// sampling luck.
///
/// `FISSION_DIR_FULL_CORPUS=1` removes the cap for a full sweep.
const DEFAULT_ATTEMPT_BUDGET: usize = 250;

fn attempt_budget() -> usize {
    match std::env::var("FISSION_DIR_FULL_CORPUS").as_deref() {
        Ok("1" | "true" | "on" | "yes") => usize::MAX,
        _ => DEFAULT_ATTEMPT_BUDGET,
    }
}

/// Whether `name` is a function the corpus was built to exercise, rather
/// than runtime support linked in beside it.
///
/// Measured: of 900 functions attempted unfiltered, roughly 230 bail on an
/// indirect or unmodelled call and 212 on a raw register or global read --
/// almost all of it mingw's C runtime, which a PE from this corpus carries far
/// more of than it carries test code. Those are not functions this tier will
/// ever ground, and attempting them is what made the budget expire before
/// reaching the code that matters.
///
/// The corpus's own functions are plain C identifiers; runtime support is
/// underscore-prefixed or carries a toolchain prefix.
fn is_corpus_function(name: &str) -> bool {
    const RUNTIME_PREFIXES: [&str; 6] = ["mingw", "msvcrt", "pre_c", "tls", "Dll", "Wmain"];
    !name.starts_with('_')
        && !RUNTIME_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[derive(Default, Debug)]
struct Tally {
    attempted: usize,
    equivalent: usize,
    unsupported: usize,
    diverged: Vec<String>,
}

fn check_binary(path: &PathBuf, tally: &mut Tally, budget: usize) {
    let Ok(binary) = LoadedBinary::from_file(path) else {
        return;
    };
    let facts = FactStore::from_binary(&binary);
    let Ok(mut harness) = EmulatorHarness::build(path, Some(STEP_BUDGET)) else {
        // Architecture or format the emulator does not drive; the corpus has
        // aarch64 and 32-bit targets that this tier cannot reach.
        return;
    };

    let mut functions: Vec<FunctionInfo> = binary.functions.clone();
    functions.sort_by_key(|f| f.address);
    for func in functions {
        if tally.attempted >= budget {
            return;
        }
        if func.address == 0 || func.name.is_empty() || !is_corpus_function(&func.name) {
            continue;
        }
        tally.attempted += 1;
        let Ok(pair) = decompile_one(&binary, &facts, &func) else {
            continue;
        };
        if pair.hir.body.is_empty() || pair.hir.params.len() > MAX_ARITY {
            continue;
        }
        let samples = default_samples(pair.hir.params.len());
        match check_ground_truth(&mut harness, func.address, &pair.prehir, &pair.hir, &samples) {
            VerifyOutcome::Equivalent { checked } if checked > 0 => tally.equivalent += 1,
            VerifyOutcome::Diverged(divergences) => {
                tally.diverged.push(format!(
                    "{}::{} @ {:#x} -- {} divergence(s), first: {:?}",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    func.name,
                    func.address,
                    divergences.len(),
                    divergences.first()
                ));
            }
            // Equivalent-but-nothing-checked and every Unsupported reason land
            // here. Both mean this tier said nothing, which is not a failure.
            _ => tally.unsupported += 1,
        }
    }
}

/// Every function the emulator can drive must agree with its machine code.
///
/// Skips rather than fails when the corpus is absent, matching the other
/// phase tests -- the corpus lives outside this repository.
#[test]
fn corpus_decompilations_match_real_machine_code() {
    let dir = corpus_dir();
    if !dir.exists() {
        eprintln!("skipping: corpus not found at {}", dir.display());
        return;
    }
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("skipping: corpus unreadable at {}", dir.display());
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    paths.sort();

    let budget = attempt_budget();
    let mut tally = Tally::default();
    for path in &paths {
        if tally.attempted >= budget {
            break;
        }
        if path.is_file() {
            check_binary(path, &mut tally, budget);
        }
    }

    eprintln!(
        "ground truth: {} attempted, {} equivalent, {} not checkable at this tier, {} diverged",
        tally.attempted,
        tally.equivalent,
        tally.unsupported,
        tally.diverged.len()
    );
    assert!(
        tally.diverged.is_empty(),
        "decompiled bodies disagree with the real machine code:\n  {}",
        tally.diverged.join("\n  ")
    );
    // A pass with nothing checked would be silent breakage of the harness
    // itself, which is exactly the failure this test exists to notice.
    assert!(
        tally.equivalent > 0,
        "no function was groundable against the emulator -- the harness, not \
         the decompiler, is what this is reporting on"
    );
}
