//! The solver against z3, on formulas neither was written for.
//!
//! Ignored by default: it needs a `z3` binary, run as a separate process --
//! never linked. Run it with
//!
//! ```text
//! cargo test --release -p fission-solver --test differential_z3 -- --ignored --nocapture
//! FISSION_Z3=/opt/homebrew/bin/z3 FISSION_DIFF_CASES=2000 FISSION_DIFF_SEED=7 ...
//! ```
//!
//! Every formula is random, small (4 to 8 bits) and built only from the
//! operations this solver claims to encode, so a disagreement is a bug in one
//! of the two. The solver's own `Unknown` is reported separately: for these
//! operations it should never happen, and when it does it is a missing
//! circuit, not a wrong answer.
//!
//! This is layer 1 of the benchmark -- correctness before speed. An UNSAT
//! verdict cannot check itself, which is exactly what a second solver is for.

use std::io::Write;
use std::process::{Command, Stdio};

use fission_solver::smtlib;
use fission_solver::{SatResult, Solver, SymExpr};

/// xorshift64*: deterministic, so a failing seed reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random term of exactly `width` bits over `vars`.
fn term(rng: &mut Rng, vars: &[SymExpr], width: u32, depth: u32) -> SymExpr {
    if depth == 0 || rng.below(4) == 0 {
        return if rng.below(3) == 0 {
            SymExpr::new_const(rng.below(1 << width), width)
        } else {
            vars[rng.below(vars.len() as u64) as usize].clone()
        };
    }
    let sub = |rng: &mut Rng| Box::new(term(rng, vars, width, depth - 1));
    match rng.below(16) {
        0 => SymExpr::Add(sub(rng), sub(rng)),
        1 => SymExpr::Sub(sub(rng), sub(rng)),
        2 => SymExpr::Mul(sub(rng), sub(rng)),
        3 => SymExpr::Udiv(sub(rng), sub(rng)),
        4 => SymExpr::Urem(sub(rng), sub(rng)),
        5 => SymExpr::Sdiv(sub(rng), sub(rng)),
        6 => SymExpr::Srem(sub(rng), sub(rng)),
        7 => SymExpr::Smod(sub(rng), sub(rng)),
        8 => SymExpr::And(sub(rng), sub(rng)),
        9 => SymExpr::Or(sub(rng), sub(rng)),
        10 => SymExpr::Xor(sub(rng), sub(rng)),
        11 => SymExpr::Shl(sub(rng), sub(rng)),
        12 => SymExpr::Lshr(sub(rng), sub(rng)),
        13 => SymExpr::Ashr(sub(rng), sub(rng)),
        _ => SymExpr::Ite {
            cond: Box::new(boolean(rng, vars, width, depth - 1)),
            t: sub(rng),
            f: sub(rng),
        },
    }
}

/// A random one-bit condition comparing two `width`-bit terms.
fn boolean(rng: &mut Rng, vars: &[SymExpr], width: u32, depth: u32) -> SymExpr {
    let a = Box::new(term(rng, vars, width, depth));
    let b = Box::new(term(rng, vars, width, depth));
    match rng.below(7) {
        0 => SymExpr::Eq(a, b),
        1 => SymExpr::Neq(a, b),
        2 => SymExpr::Ult(a, b),
        3 => SymExpr::Ule(a, b),
        4 => SymExpr::Slt(a, b),
        5 => SymExpr::Sle(a, b),
        _ => SymExpr::Sgt(a, b),
    }
}

fn z3_path() -> Option<String> {
    if let Ok(path) = std::env::var("FISSION_Z3") {
        return Some(path);
    }
    for candidate in [
        "/opt/homebrew/bin/z3",
        "/usr/local/bin/z3",
        "/usr/bin/z3",
        "z3",
    ] {
        let ok = Command::new(candidate)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(candidate.to_string());
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    Sat,
    Unsat,
    Unknown,
}

fn ask_z3(z3: &str, script: &str) -> Verdict {
    let mut child = Command::new(z3)
        .args(["-smt2", "-in", "-T:20"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn z3");
    child
        .stdin
        .take()
        .expect("z3 stdin")
        .write_all(script.as_bytes())
        .expect("write script");
    let output = child.wait_with_output().expect("z3 output");
    match String::from_utf8_lossy(&output.stdout).trim() {
        "sat" => Verdict::Sat,
        "unsat" => Verdict::Unsat,
        _ => Verdict::Unknown,
    }
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn the_solver_agrees_with_z3_on_random_bitvector_formulas() {
    let Some(z3) = z3_path() else {
        eprintln!("skipping: no z3 found (set FISSION_Z3)");
        return;
    };
    let cases: u64 = std::env::var("FISSION_DIFF_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);
    let seed: u64 = std::env::var("FISSION_DIFF_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0x5EED_F155_1017);
    let depth: u32 = std::env::var("FISSION_DIFF_DEPTH")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let mut rng = Rng(seed | 1);

    let (mut agree, mut ours_unknown, mut z3_unknown) = (0u64, 0u64, 0u64);
    let mut by_verdict = [0u64; 2];
    let mut disagreements = Vec::new();

    for case in 0..cases {
        let width = 4 + rng.below(5) as u32;
        let vars: Vec<SymExpr> = (0..1 + rng.below(3))
            .map(|i| SymExpr::new_var(&format!("x{i}"), width))
            .collect();
        let assertion_count = 1 + rng.below(2);
        let assertions: Vec<SymExpr> = (0..assertion_count)
            .map(|_| boolean(&mut rng, &vars, width, depth))
            .collect();

        let script = match smtlib::script(&assertions) {
            Ok(script) => script,
            Err(error) => panic!("case {case}: the generator built something unprintable: {error}"),
        };

        let mut solver = Solver::new();
        for assertion in &assertions {
            solver.assert(assertion.clone());
        }
        let ours = match solver.check_sat().expect("check_sat") {
            SatResult::Sat => Verdict::Sat,
            SatResult::Unsat => Verdict::Unsat,
            SatResult::Unknown => Verdict::Unknown,
        };
        let theirs = ask_z3(&z3, &script);

        match (ours, theirs) {
            (_, Verdict::Unknown) => z3_unknown += 1,
            (Verdict::Unknown, _) => ours_unknown += 1,
            (a, b) if a == b => {
                agree += 1;
                by_verdict[usize::from(a == Verdict::Unsat)] += 1;
            }
            (a, b) => disagreements.push((case, a, b, script)),
        }
    }

    eprintln!(
        "{cases} formulas (seed {seed}): {agree} agree ({} sat, {} unsat), \
         {} disagree, {ours_unknown} unknown here, {z3_unknown} unknown in z3",
        by_verdict[0],
        by_verdict[1],
        disagreements.len()
    );
    // Which operators the disagreeing formulas contain, against how often
    // each appears overall -- the operator present in every disagreement is
    // the first suspect.
    if !disagreements.is_empty() {
        let mut tally: std::collections::BTreeMap<&str, usize> = Default::default();
        for (_, _, _, script) in &disagreements {
            for op in [
                "bvadd", "bvsub", "bvmul", "bvudiv", "bvurem", "bvsdiv", "bvsrem", "bvsmod",
                "bvand", "bvor", "bvxor", "bvshl", "bvlshr", "bvashr", "bvult", "bvule", "bvslt",
                "bvsle", "bvsgt", "bvnot", "(= ", "ite",
            ] {
                if script.contains(op) {
                    *tally.entry(op).or_default() += 1;
                }
            }
        }
        eprintln!(
            "operators in the {} disagreeing formulas:",
            disagreements.len()
        );
        for (op, count) in &tally {
            eprintln!("  {op:<8} {count}");
        }
        let smallest = disagreements
            .iter()
            .min_by_key(|(_, _, _, script)| script.len())
            .expect("non-empty");
        eprintln!(
            "smallest: case {} ({:?} here, {:?} in z3)\n{}",
            smallest.0, smallest.1, smallest.2, smallest.3
        );
    }
    if let Some((case, ours, theirs, script)) = disagreements.first() {
        panic!("case {case}: fission-solver said {ours:?}, z3 said {theirs:?}\n{script}");
    }
    assert_eq!(
        ours_unknown, 0,
        "every operation in these formulas has a circuit; Unknown means one is missing"
    );
}
