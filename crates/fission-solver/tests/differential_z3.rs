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

// ---------------------------------------------------------------------------
// The widened differential.
//
// The test above draws 4-to-8-bit formulas from one width and one variable
// pool. Everything below widens that along the axes it left out: widths up to
// 64 bits, operands of different widths, concat and extract, constants that
// sit on a boundary (zero divisors, shift amounts at and past the width, the
// sign bit), arrays, the constant-folding constructors, the model behind a
// `Sat`, and a solver that is asked more than one question.
//
// Each case is seeded from `(seed, case)` alone, so a case reproduces on its
// own (`FISSION_DIFF_ONLY=<case>`) whatever the worker count, and a
// disagreement is shrunk before it is reported.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use fission_solver::{Sort, UnknownReason};

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Conflicts one query may spend here before it gives up. A random formula
/// with a 64-bit multiplier in it can cost this solver minutes where z3
/// takes milliseconds; that is a cost, not a wrong answer, and it is counted
/// apart from both.
fn conflict_budget() -> Option<u64> {
    match env_u64("FISSION_DIFF_CONFLICTS", 20_000) {
        0 => None,
        budget => Some(budget),
    }
}

/// The budget, and -- for measuring the search itself --
/// `FISSION_DIFF_RESTARTS=0` to turn restarts off.
fn configure(solver: &mut Solver) {
    solver.set_conflict_budget(conflict_budget());
    if env_u64("FISSION_DIFF_RESTARTS", 1) == 0 {
        solver.sat.set_restart_unit(None);
    }
}

const GAVE_UP: &str = "conflict budget";
const REFINED_OUT: &str = "oracle refinement rounds";

fn why_unknown(solver: &Solver) -> Vec<String> {
    match solver.last_unknown() {
        Some(UnknownReason::Unsupported(what)) => what.clone(),
        Some(UnknownReason::ConflictBudget) => vec![GAVE_UP.to_string()],
        Some(UnknownReason::OracleRefinement) => vec![REFINED_OUT.to_string()],
        None => vec!["no reason recorded".to_string()],
    }
}

/// A generator for one case, independent of every other case.
fn case_rng(seed: u64, case: u64) -> Rng {
    // splitmix64 over the pair, so neighbouring cases do not share a stream.
    let mut z = seed ^ case.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    Rng((z ^ (z >> 31)) | 1)
}

/// What a formula is allowed to contain.
#[derive(Clone, Copy)]
struct Shape {
    /// Widths a comparison's operands are drawn from.
    widths: &'static [u32],
    depth: u32,
    /// Give unsigned operations and `ite` operands of different widths; the
    /// solver zero-extends the narrower one.
    mixed_widths: bool,
    arrays: bool,
    /// How many multiply and divide nodes wider than eight bits one formula
    /// may hold. Each is a quadratic circuit, and nesting them is how a
    /// random formula turns into an afternoon.
    heavy: u32,
    /// Lean towards what the folding constructors look for: zero operands,
    /// constant operands, and the same term on both sides.
    folds: bool,
    /// Arrays are address spaces of bytes with concrete contents behind
    /// them, the way the emulator uses the solver: named `space_<n>` and
    /// answered through a [`MemoryOracle`].
    oracle: bool,
}

struct Gen {
    rng: Rng,
    shape: Shape,
    vars: Vec<SymExpr>,
    arrays: Vec<SymExpr>,
    heavy_left: u32,
}

impl Gen {
    fn new(rng: Rng, shape: Shape) -> Self {
        Self {
            rng,
            shape,
            vars: Vec::new(),
            arrays: Vec::new(),
            heavy_left: shape.heavy,
        }
    }

    fn below(&mut self, n: u64) -> u64 {
        self.rng.below(n)
    }

    fn pick_width(&mut self) -> u32 {
        let widths = self.shape.widths;
        widths[self.below(widths.len() as u64) as usize]
    }

    /// A variable `width` bits wide: an existing one more often than not, so
    /// the same variable turns up on both sides of an operation.
    fn var(&mut self, width: u32) -> SymExpr {
        let existing: Vec<SymExpr> = self
            .vars
            .iter()
            .filter(|v| v.get_bit_width() == width)
            .cloned()
            .collect();
        if !existing.is_empty() && (existing.len() >= 2 || self.below(3) != 0) {
            return existing[self.below(existing.len() as u64) as usize].clone();
        }
        let var = SymExpr::new_var(&format!("x{}", self.vars.len()), width);
        self.vars.push(var.clone());
        var
    }

    /// A constant, most often one that sits on an edge: zero (a divisor),
    /// the width and its neighbours (a shift amount), the sign bit.
    fn constant(&mut self, width: u32) -> SymExpr {
        let mask = if width >= 64 {
            u64::MAX
        } else {
            (1u64 << width) - 1
        };
        let sign = 1u64 << (width - 1);
        if self.shape.folds && self.below(3) == 0 {
            return SymExpr::new_const(0, width);
        }
        let value = match self.below(12) {
            0 => 0,
            1 => 1,
            2 => mask,
            3 => sign,
            4 => sign.wrapping_sub(1),
            5 => u64::from(width),
            6 => u64::from(width - 1),
            7 => u64::from(width + 1),
            8 => 1u64 << self.below(u64::from(width)),
            9 => mask.wrapping_sub(1),
            _ => self.rng.next(),
        };
        SymExpr::new_const(value, width)
    }

    fn leaf(&mut self, width: u32) -> SymExpr {
        if self.below(3) == 0 || (self.shape.folds && self.below(3) == 0) {
            self.constant(width)
        } else {
            self.var(width)
        }
    }

    /// Operand widths for an operation that zero-extends: both `width`, or
    /// one of them narrower.
    fn operand_widths(&mut self, width: u32) -> (u32, u32) {
        if !self.shape.mixed_widths || width == 1 || self.below(4) != 0 {
            return (width, width);
        }
        let narrow = 1 + self.below(u64::from(width - 1)) as u32;
        if self.below(2) == 0 {
            (narrow, width)
        } else {
            (width, narrow)
        }
    }

    fn take_heavy(&mut self, width: u32) -> bool {
        if width <= 8 {
            return true;
        }
        if self.heavy_left == 0 {
            return false;
        }
        self.heavy_left -= 1;
        true
    }

    /// A term exactly `width` bits wide.
    fn bv(&mut self, width: u32, depth: u32) -> SymExpr {
        if depth == 0 || self.below(5) == 0 {
            return self.leaf(width);
        }
        if width == 1 && self.below(2) == 0 {
            return self.boolean(depth - 1);
        }
        let d = depth - 1;
        loop {
            let pick = self.below(22);
            let term = match pick {
                // Unsigned and bitwise: operands may differ in width.
                0..=7 => {
                    if matches!(pick, 2..=4) && !self.take_heavy(width) {
                        continue;
                    }
                    let (wa, wb) = self.operand_widths(width);
                    let mut a = Box::new(self.bv(wa, d));
                    let mut b = if self.shape.folds && wa == wb && self.below(5) == 0 {
                        a.clone()
                    } else {
                        Box::new(self.bv(wb, d))
                    };
                    // A zero as wide as the result beside a narrower term:
                    // dropping it would narrow the result.
                    if self.shape.folds && wa != wb && self.below(3) == 0 {
                        let wide = if wa > wb { &mut a } else { &mut b };
                        **wide = SymExpr::new_const(0, width);
                    }
                    match pick {
                        0 => SymExpr::Add(a, b),
                        1 => SymExpr::Sub(a, b),
                        2 => SymExpr::Mul(a, b),
                        3 => SymExpr::Udiv(a, b),
                        4 => SymExpr::Urem(a, b),
                        5 => SymExpr::And(a, b),
                        6 => SymExpr::Or(a, b),
                        _ => SymExpr::Xor(a, b),
                    }
                }
                // Signed division and shifts: equal widths only.
                8..=13 => {
                    if matches!(pick, 8..=10) && !self.take_heavy(width) {
                        continue;
                    }
                    let (a, b) = (Box::new(self.bv(width, d)), Box::new(self.bv(width, d)));
                    match pick {
                        8 => SymExpr::Sdiv(a, b),
                        9 => SymExpr::Srem(a, b),
                        10 => SymExpr::Smod(a, b),
                        11 => SymExpr::Shl(a, b),
                        12 => SymExpr::Lshr(a, b),
                        _ => SymExpr::Ashr(a, b),
                    }
                }
                14 | 15 => {
                    let (wt, wf) = self.operand_widths(width);
                    SymExpr::Ite {
                        cond: Box::new(self.boolean(d)),
                        t: Box::new(self.bv(wt, d)),
                        f: Box::new(self.bv(wf, d)),
                    }
                }
                // A slice of something wider.
                16 | 17 => {
                    if width == 64 {
                        continue;
                    }
                    let room = u64::from(64 - width);
                    let source = width + 1 + self.below(room.min(24)) as u32;
                    let lsb = self.below(u64::from(source - width) + 1) as u32;
                    SymExpr::Extract {
                        expr: Box::new(self.bv(source, d)),
                        lsb,
                        size: width,
                    }
                }
                // Two narrower pieces, high then low.
                18 | 19 => {
                    if width < 2 {
                        continue;
                    }
                    let high = 1 + self.below(u64::from(width - 1)) as u32;
                    SymExpr::Concat(
                        Box::new(self.bv(high, d)),
                        Box::new(self.bv(width - high, d)),
                    )
                }
                // A read from memory. Small index sorts, so that two indices
                // are often the same slot.
                _ => {
                    if !self.shape.arrays || (self.shape.oracle && width != 8) {
                        continue;
                    }
                    const DOMAINS: [u32; 11] = [1, 2, 2, 3, 3, 4, 4, 8, 16, 32, 64];
                    let domain = DOMAINS[self.below(DOMAINS.len() as u64) as usize];
                    SymExpr::ArraySelect {
                        array: Box::new(self.array(domain, width, d.min(3))),
                        index: Box::new(self.bv(domain, d.min(1))),
                    }
                }
            };
            return term;
        }
    }

    /// An array from `domain`-bit indices to `range`-bit elements: a
    /// variable, a write on top of one, or a choice between two.
    fn array(&mut self, domain: u32, range: u32, depth: u32) -> SymExpr {
        if depth == 0 || self.below(3) == 0 {
            let sort = Sort::Array {
                domain: Box::new(Sort::BitVector(domain)),
                range: Box::new(Sort::BitVector(range)),
            };
            let existing: Vec<SymExpr> = self
                .arrays
                .iter()
                .filter(|a| a.get_sort() == sort)
                .cloned()
                .collect();
            if !existing.is_empty() && (existing.len() >= 2 || self.below(3) != 0) {
                return existing[self.below(existing.len() as u64) as usize].clone();
            }
            let name = if self.shape.oracle {
                format!("space_{}", self.arrays.len())
            } else {
                format!("m{}", self.arrays.len())
            };
            let array = SymExpr::new_array_var(&name, domain, range);
            self.arrays.push(array.clone());
            return array;
        }
        let d = depth - 1;
        if self.below(4) != 0 {
            SymExpr::ArrayStore {
                array: Box::new(self.array(domain, range, d)),
                index: Box::new(self.bv(domain, d.min(1))),
                value: Box::new(self.bv(range, d.min(1))),
            }
        } else {
            SymExpr::Ite {
                cond: Box::new(self.boolean(d.min(1))),
                t: Box::new(self.array(domain, range, d)),
                f: Box::new(self.array(domain, range, d)),
            }
        }
    }

    /// A one-bit condition.
    fn boolean(&mut self, depth: u32) -> SymExpr {
        let pick = self.below(9);
        if pick >= 7 && depth > 0 {
            // A connective: one-bit and/or/xor/ite over further conditions.
            return self.bv(1, depth);
        }
        let width = self.pick_width();
        if pick <= 3 {
            let (wa, wb) = self.operand_widths(width);
            let a = Box::new(self.bv(wa, depth));
            let b = if self.shape.folds && wa == wb && self.below(5) == 0 {
                a.clone()
            } else {
                Box::new(self.bv(wb, depth))
            };
            match pick {
                0 => SymExpr::Eq(a, b),
                1 => SymExpr::Neq(a, b),
                2 => SymExpr::Ult(a, b),
                _ => SymExpr::Ule(a, b),
            }
        } else {
            let (a, b) = (
                Box::new(self.bv(width, depth)),
                Box::new(self.bv(width, depth)),
            );
            match pick {
                4 => SymExpr::Slt(a, b),
                5 => SymExpr::Sle(a, b),
                _ => SymExpr::Sgt(a, b),
            }
        }
    }

    fn formula(&mut self) -> Vec<SymExpr> {
        let count = 1 + self.below(3);
        let depth = self.shape.depth;
        (0..count).map(|_| self.boolean(depth)).collect()
    }
}

// ----- structure: rebuilding and shrinking ---------------------------------

/// `expr` with each child replaced by `f(child)`.
fn map_children(expr: &SymExpr, f: &mut dyn FnMut(&SymExpr) -> SymExpr) -> SymExpr {
    use SymExpr::*;
    let mut b = |x: &SymExpr| Box::new(f(x));
    match expr {
        Add(x, y) => Add(b(x), b(y)),
        Sub(x, y) => Sub(b(x), b(y)),
        Mul(x, y) => Mul(b(x), b(y)),
        Udiv(x, y) => Udiv(b(x), b(y)),
        Urem(x, y) => Urem(b(x), b(y)),
        Sdiv(x, y) => Sdiv(b(x), b(y)),
        Srem(x, y) => Srem(b(x), b(y)),
        Smod(x, y) => Smod(b(x), b(y)),
        And(x, y) => And(b(x), b(y)),
        Or(x, y) => Or(b(x), b(y)),
        Xor(x, y) => Xor(b(x), b(y)),
        Shl(x, y) => Shl(b(x), b(y)),
        Lshr(x, y) => Lshr(b(x), b(y)),
        Ashr(x, y) => Ashr(b(x), b(y)),
        Eq(x, y) => Eq(b(x), b(y)),
        Neq(x, y) => Neq(b(x), b(y)),
        Ult(x, y) => Ult(b(x), b(y)),
        Ule(x, y) => Ule(b(x), b(y)),
        Slt(x, y) => Slt(b(x), b(y)),
        Sle(x, y) => Sle(b(x), b(y)),
        Sgt(x, y) => Sgt(b(x), b(y)),
        Concat(x, y) => Concat(b(x), b(y)),
        Ite { cond, t, f: e } => Ite {
            cond: b(cond),
            t: b(t),
            f: b(e),
        },
        Extract {
            expr: inner,
            lsb,
            size,
        } => Extract {
            expr: b(inner),
            lsb: *lsb,
            size: *size,
        },
        ArraySelect { array, index } => ArraySelect {
            array: b(array),
            index: b(index),
        },
        ArrayStore {
            array,
            index,
            value,
        } => ArrayStore {
            array: b(array),
            index: b(index),
            value: b(value),
        },
        other => other.clone(),
    }
}

fn children(expr: &SymExpr) -> Vec<SymExpr> {
    let mut out = Vec::new();
    map_children(expr, &mut |child| {
        out.push(child.clone());
        child.clone()
    });
    out
}

/// The same formula built through the folding constructors where one exists,
/// bottom-up -- the way `fission-dir` builds what it asks.
fn through_constructors(expr: &SymExpr) -> SymExpr {
    let rebuilt = map_children(expr, &mut |child| through_constructors(child));
    use SymExpr::*;
    match rebuilt {
        Add(a, b) => SymExpr::new_add(*a, *b),
        Sub(a, b) => SymExpr::new_sub(*a, *b),
        And(a, b) => SymExpr::new_and(*a, *b),
        Xor(a, b) => {
            let all_ones = |e: &SymExpr, other: &SymExpr| {
                matches!(e, Const { val, size }
                    if *size == other.get_bit_width()
                        && *val == if *size >= 64 { u64::MAX } else { (1u64 << size) - 1 })
            };
            if all_ones(&b, &a) {
                SymExpr::new_not(*a)
            } else {
                SymExpr::new_xor(*a, *b)
            }
        }
        Eq(a, b) => SymExpr::new_eq(*a, *b),
        Neq(a, b) => SymExpr::new_neq(*a, *b),
        Ult(a, b) => SymExpr::new_ult(*a, *b),
        Slt(a, b) => SymExpr::new_slt(*a, *b),
        Sle(a, b) => SymExpr::new_sle(*a, *b),
        Sgt(a, b) => SymExpr::new_sgt(*a, *b),
        other => other,
    }
}

/// How much there is left to remove: operations weigh more than variables,
/// variables more than constants, zero least.
fn weight(expr: &SymExpr) -> u64 {
    let own = match expr {
        SymExpr::Const { val: 0, .. } => 1,
        SymExpr::Const { .. } => 2,
        SymExpr::Var { .. } => 3,
        _ => 4,
    };
    own + children(expr).iter().map(weight).sum::<u64>()
}

/// Every formula one simplification away from `expr`: a node replaced by a
/// child of its own sort, by a constant, or by a variable that stands for it.
fn reductions(expr: &SymExpr, stand_ins: &mut BTreeMap<u32, SymExpr>) -> Vec<SymExpr> {
    let mut out = Vec::new();
    let sort = expr.get_sort();
    let kids = children(expr);
    for kid in &kids {
        if kid.get_sort() == sort {
            out.push(kid.clone());
        }
    }
    if let Sort::BitVector(width) = sort {
        if (1..=64).contains(&width) && !matches!(expr, SymExpr::Const { .. }) {
            out.push(SymExpr::new_const(0, width));
            out.push(SymExpr::new_const(1, width));
            out.push(SymExpr::new_const(u64::MAX, width));
            if !matches!(expr, SymExpr::Var { .. }) {
                out.push(
                    stand_ins
                        .entry(width)
                        .or_insert_with(|| SymExpr::new_var(&format!("s{width}"), width))
                        .clone(),
                );
            }
        }
    }
    for (i, kid) in kids.iter().enumerate() {
        for reduced in reductions(kid, stand_ins) {
            let mut n = 0;
            out.push(map_children(expr, &mut |child| {
                let this = n == i;
                n += 1;
                if this {
                    reduced.clone()
                } else {
                    child.clone()
                }
            }));
        }
    }
    out
}

/// Greedy delta debugging: keep taking the first smaller formula on which
/// `fails` still holds.
fn shrink(mut current: Vec<SymExpr>, fails: &mut dyn FnMut(&[SymExpr]) -> bool) -> Vec<SymExpr> {
    let mut stand_ins = BTreeMap::new();
    let mut budget = env_u64("FISSION_DIFF_SHRINK_STEPS", 4000);
    'again: loop {
        for i in 0..current.len() {
            if current.len() > 1 && budget > 0 {
                budget -= 1;
                let mut candidate = current.clone();
                candidate.remove(i);
                if fails(&candidate) {
                    current = candidate;
                    continue 'again;
                }
            }
        }
        for i in 0..current.len() {
            let before = weight(&current[i]);
            for reduced in reductions(&current[i], &mut stand_ins) {
                if weight(&reduced) >= before || budget == 0 {
                    continue;
                }
                budget -= 1;
                let mut candidate = current.clone();
                candidate[i] = reduced;
                if fails(&candidate) {
                    current = candidate;
                    continue 'again;
                }
            }
        }
        return current;
    }
}

// ----- concrete memory behind the arrays ------------------------------------

/// How many addresses of each space have concrete contents. The rest read
/// as unknown, which leaves the array free there.
const MEMORY_SLOTS: u64 = 16;

/// The byte at `addr` of space `space`: a fixed function of the two, so the
/// oracle the solver consults and the facts z3 is told cannot drift apart.
/// Small values, so that a read is often equal to a constant or another read.
fn memory_byte(space: u64, addr: u64) -> Option<u8> {
    if addr >= MEMORY_SLOTS {
        return None;
    }
    let mut z = (space << 8 | addr).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    match z % 8 {
        0 => None,
        1 => Some((z >> 8) as u8),
        _ => Some((z >> 8) as u8 % 4),
    }
}

struct Memory;

impl fission_solver::MemoryOracle for Memory {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8> {
        memory_byte(space_id, addr)
    }
}

/// What the oracle knows, as assertions: `select(space, addr) == byte` for
/// every address it answers for that the array's index sort can name.
fn memory_facts(assertions: &[SymExpr]) -> Vec<SymExpr> {
    let mut spaces: Vec<SymExpr> = Vec::new();
    for assertion in assertions {
        assertion.visit_preorder(&mut |node| {
            if matches!(
                node,
                SymExpr::Var {
                    sort: Sort::Array { .. },
                    ..
                }
            ) && !spaces.contains(node)
            {
                spaces.push(node.clone());
            }
        });
    }
    let mut facts = Vec::new();
    for space in spaces {
        let SymExpr::Var { name, sort, .. } = &space else {
            unreachable!()
        };
        let Sort::Array { domain, .. } = sort else {
            unreachable!()
        };
        let domain = domain.expect_bv();
        let id: u64 = name
            .strip_prefix("space_")
            .and_then(|n| n.parse().ok())
            .expect("an oracle-backed array is named space_<n>");
        for addr in 0..MEMORY_SLOTS {
            if domain < 64 && addr >> domain != 0 {
                break;
            }
            if let Some(byte) = memory_byte(id, addr) {
                facts.push(SymExpr::Eq(
                    Box::new(SymExpr::ArraySelect {
                        array: Box::new(space.clone()),
                        index: Box::new(SymExpr::new_const(addr, domain)),
                    }),
                    Box::new(SymExpr::new_const(u64::from(byte), 8)),
                ));
            }
        }
    }
    facts
}

// ----- one formula against z3 ----------------------------------------------

fn run_z3(z3: &str, script: &str) -> Verdict {
    let mut child = Command::new(z3)
        .args(["-smt2", "-in", "-T:60"])
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
    let text = String::from_utf8_lossy(&output.stdout);
    match text.trim() {
        "sat" => Verdict::Sat,
        "unsat" => Verdict::Unsat,
        other => {
            // A script z3 rejects is a printer bug, not an unknown.
            assert!(
                !other.contains("error"),
                "z3 rejected the script: {other}\n{script}"
            );
            Verdict::Unknown
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Agree(Verdict),
    /// The solver declined, and said why.
    Declined(Vec<String>),
    /// The solver ran out of conflicts; z3's answer says what it missed.
    GaveUp(Verdict),
    Z3Unknown,
    /// A different verdict: one of the two is wrong.
    Verdict {
        ours: Verdict,
        theirs: Verdict,
    },
    /// Both said SAT, and the model this solver produced is not one.
    Model(String),
}

impl Outcome {
    fn is_bug(&self) -> bool {
        matches!(self, Outcome::Verdict { .. } | Outcome::Model(_))
    }
}

fn selects_of(assertions: &[SymExpr]) -> Vec<SymExpr> {
    let mut out: Vec<SymExpr> = Vec::new();
    for assertion in assertions {
        assertion.visit_preorder(&mut |node| {
            if matches!(node, SymExpr::ArraySelect { .. }) && !out.contains(node) {
                out.push(node.clone());
            }
        });
    }
    out
}

fn bitvector_vars_of(assertions: &[SymExpr]) -> Vec<SymExpr> {
    let mut out: Vec<SymExpr> = Vec::new();
    for assertion in assertions {
        assertion.visit_preorder(&mut |node| {
            if matches!(
                node,
                SymExpr::Var {
                    sort: Sort::BitVector(_),
                    ..
                }
            ) && !out.contains(node)
            {
                out.push(node.clone());
            }
        });
    }
    out
}

/// Solve `ours` here and `reference` in z3 -- the same formula unless the
/// case is about the constructors -- and, when both say SAT, hand this
/// solver's model to z3: every variable and every array read pinned to the
/// value read back. A verdict can be right by accident; a model cannot.
fn check_formula(
    z3: &str,
    ours: &[SymExpr],
    reference: &[SymExpr],
    oracle: bool,
) -> (Outcome, String) {
    let (outcome, script, _) = check_formula_timed(z3, ours, reference, oracle);
    (outcome, script)
}

/// [`check_formula`], and how long this solver took over its verdict.
///
/// With `oracle`, the arrays have concrete contents: this solver learns them
/// by asking [`Memory`], z3 is told them up front.
fn check_formula_timed(
    z3: &str,
    ours: &[SymExpr],
    reference: &[SymExpr],
    oracle: bool,
) -> (Outcome, String, Duration) {
    let mut reference = reference.to_vec();
    if oracle {
        reference.extend(memory_facts(ours));
    }
    let reference = &reference[..];
    let script = smtlib::script(reference)
        .unwrap_or_else(|error| panic!("the generator built something unprintable: {error}"));

    let started = Instant::now();
    let mut solver = Solver::new();
    configure(&mut solver);
    let vars = bitvector_vars_of(ours);
    let ids: Vec<_> = vars
        .iter()
        .map(|var| solver.register_node(var.clone()))
        .collect();
    for assertion in ours {
        solver.assert(assertion.clone());
    }
    let result = if oracle {
        solver.check_sat_with_oracle(Some(&Memory), &[])
    } else {
        solver.check_sat()
    }
    .expect("check_sat");
    let elapsed = started.elapsed();
    let verdict = match result {
        SatResult::Sat => Verdict::Sat,
        SatResult::Unsat => Verdict::Unsat,
        SatResult::Unknown => {
            let why = why_unknown(&solver);
            // What it gave up on: a model it did not find, or a refutation.
            let outcome = if why.iter().any(|w| w == GAVE_UP) {
                Outcome::GaveUp(run_z3(z3, &script))
            } else {
                Outcome::Declined(why)
            };
            return (outcome, script, elapsed);
        }
    };
    let theirs = run_z3(z3, &script);
    if theirs == Verdict::Unknown {
        return (Outcome::Z3Unknown, script, elapsed);
    }
    if verdict != theirs {
        let outcome = Outcome::Verdict {
            ours: verdict,
            theirs,
        };
        return (outcome, script, elapsed);
    }
    if verdict == Verdict::Unsat {
        return (Outcome::Agree(verdict), script, elapsed);
    }

    let mut pinned: Vec<SymExpr> = reference.to_vec();
    let mut described = String::new();
    for (var, id) in vars.iter().zip(&ids) {
        let value = solver
            .get_value(*id)
            .expect("a registered variable has a model value");
        described.push_str(&format!(" {}={value:#x}", smtlib::symbol_for(var).unwrap()));
        pinned.push(pin(var, value));
    }
    for select in selects_of(ours) {
        let value = solver
            .bv_theory
            .evaluate_expr_in_model(&solver.sat, &select);
        pinned.push(pin(&select, value));
    }
    let pinned_script = smtlib::script(&pinned).expect("a pinned model prints");
    let outcome = match run_z3(z3, &pinned_script) {
        Verdict::Sat => Outcome::Agree(verdict),
        Verdict::Unknown => Outcome::Z3Unknown,
        Verdict::Unsat => Outcome::Model(format!("model:{described}\n{pinned_script}")),
    };
    (outcome, script, elapsed)
}

/// `term == value`, as a condition.
fn pin(term: &SymExpr, value: u64) -> SymExpr {
    SymExpr::Eq(
        Box::new(term.clone()),
        Box::new(SymExpr::new_const(value, term.get_bit_width())),
    )
}

// ----- running a batch -----------------------------------------------------

struct Finding {
    case: u64,
    outcome: Outcome,
    script: String,
    assertions: Vec<SymExpr>,
}

/// Run `cases` cases of `shape` across a few workers and report. With
/// `constructors`, this solver is given the formula rebuilt through the
/// folding constructors while z3 keeps the one the generator wrote.
fn run_batch(name: &str, shape: Shape, default_cases: u64, constructors: bool) {
    let Some(z3) = z3_path() else {
        eprintln!("skipping: no z3 found (set FISSION_Z3)");
        return;
    };
    let cases = env_u64("FISSION_DIFF_CASES", default_cases);
    let seed = env_u64("FISSION_DIFF_SEED", 0x5EED_F155_1017);
    let only = std::env::var("FISSION_DIFF_ONLY")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let jobs = env_u64("FISSION_DIFF_JOBS", 4).max(1);
    let hang = Duration::from_secs(env_u64("FISSION_DIFF_HANG_SECS", 300));

    let ours_for = |assertions: &[SymExpr]| -> Vec<SymExpr> {
        if constructors {
            assertions.iter().map(through_constructors).collect()
        } else {
            assertions.to_vec()
        }
    };

    let next = AtomicU64::new(0);
    let findings: Mutex<Vec<Finding>> = Mutex::new(Vec::new());
    let tally: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
    let slowest: Mutex<Vec<(Duration, u64)>> = Mutex::new(Vec::new());
    let gave_up: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    // What each worker is on, so a case that never ends can be named.
    let running: Mutex<BTreeMap<u64, (Instant, String)>> = Mutex::new(BTreeMap::new());
    let done = std::sync::atomic::AtomicBool::new(false);

    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                for (case, (started, script)) in running.lock().unwrap().iter() {
                    if started.elapsed() > hang {
                        eprintln!(
                            "{name}: case {case} (seed {seed}) has run for {:?}:\n{script}",
                            started.elapsed()
                        );
                        std::process::abort();
                    }
                }
            }
        });
        let workers: Vec<_> = (0..jobs)
            .map(|_| {
                scope.spawn(|| loop {
                    let case = next.fetch_add(1, Ordering::Relaxed);
                    if case >= cases {
                        break;
                    }
                    if only.is_some_and(|only| only != case) {
                        continue;
                    }
                    let mut gen = Gen::new(case_rng(seed, case), shape);
                    let assertions = gen.formula();
                    let script = smtlib::script(&assertions).unwrap_or_else(|error| {
                        panic!("case {case}: unprintable: {error}\n{assertions:?}")
                    });
                    running
                        .lock()
                        .unwrap()
                        .insert(case, (Instant::now(), script));
                    let (outcome, script, elapsed) =
                        check_formula_timed(&z3, &ours_for(&assertions), &assertions, shape.oracle);
                    running.lock().unwrap().remove(&case);

                    let key = match &outcome {
                        Outcome::Agree(Verdict::Sat) => "agree sat".to_string(),
                        Outcome::Agree(_) => "agree unsat".to_string(),
                        Outcome::Declined(why) => format!("declined: {}", why.join("; ")),
                        Outcome::GaveUp(theirs) => {
                            format!("gave up: {GAVE_UP} ({theirs:?} in z3)")
                        }
                        Outcome::Z3Unknown => "unknown in z3".to_string(),
                        Outcome::Verdict { ours, theirs } => {
                            format!("WRONG VERDICT {ours:?} here, {theirs:?} in z3")
                        }
                        Outcome::Model(_) => "WRONG MODEL".to_string(),
                    };
                    if matches!(&outcome, Outcome::GaveUp(_)) {
                        gave_up.lock().unwrap().push(case);
                        // Kept for whoever works on what this solver gives
                        // up on: the formulas themselves are the corpus.
                        if let Ok(dir) = std::env::var("FISSION_DIFF_DUMP") {
                            let path = format!("{dir}/{name}-{case}.smt2");
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::fs::write(path, &script);
                        }
                    }
                    *tally.lock().unwrap().entry(key).or_default() += 1;
                    {
                        let mut slowest = slowest.lock().unwrap();
                        slowest.push((elapsed, case));
                        slowest.sort_by(|a, b| b.cmp(a));
                        slowest.truncate(3);
                    }
                    if outcome.is_bug() {
                        findings.lock().unwrap().push(Finding {
                            case,
                            outcome,
                            script,
                            assertions,
                        });
                    }
                })
            })
            .collect();
        for worker in workers {
            if worker.join().is_err() {
                done.store(true, Ordering::Relaxed);
                panic!("{name}: a worker panicked");
            }
        }
        done.store(true, Ordering::Relaxed);
    });

    let tally = tally.into_inner().unwrap();
    eprintln!("{name}: {cases} cases, seed {seed}");
    for (key, count) in &tally {
        eprintln!("  {count:>6}  {key}");
    }
    let mut gave_up = gave_up.into_inner().unwrap();
    if !gave_up.is_empty() {
        gave_up.sort_unstable();
        eprintln!("  gave up on cases {gave_up:?}");
    }
    for (elapsed, case) in slowest.into_inner().unwrap() {
        eprintln!("  slowest here: case {case}, {elapsed:?}");
    }

    let mut findings = findings.into_inner().unwrap();
    findings.sort_by_key(|finding| finding.script.len());
    if let Some(first) = findings.first() {
        eprintln!(
            "{name}: {} findings; cases: {:?}",
            findings.len(),
            findings.iter().map(|f| f.case).collect::<Vec<_>>()
        );
        let kind = std::mem::discriminant(&first.outcome);
        let minimised = shrink(first.assertions.clone(), &mut |candidate| {
            if smtlib::script(candidate).is_err() {
                return false;
            }
            let (outcome, _) = check_formula(&z3, &ours_for(candidate), candidate, shape.oracle);
            std::mem::discriminant(&outcome) == kind
        });
        let (outcome, script) = check_formula(&z3, &ours_for(&minimised), &minimised, shape.oracle);
        panic!(
            "{name}: case {} (seed {seed}): {:?}\n{}\nminimised to {outcome:?}:\n{script}\n{minimised:#?}",
            first.case, first.outcome, first.script
        );
    }
    let declined: u64 = tally
        .iter()
        .filter(|(key, _)| key.starts_with("declined") && !key.ends_with(REFINED_OUT))
        .map(|(_, count)| count)
        .sum();
    assert_eq!(
        declined, 0,
        "{name}: every operation in these formulas has a circuit; a declined case means one is missing"
    );
}

const NARROW: &[u32] = &[1, 2, 3, 4, 5, 6, 7, 8];
const WIDE: &[u32] = &[1, 8, 13, 16, 24, 31, 32, 33, 48, 63, 64];

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn wide_bitvector_formulas_agree_with_z3() {
    run_batch(
        "wide",
        Shape {
            widths: WIDE,
            depth: env_u64("FISSION_DIFF_DEPTH", 3) as u32,
            mixed_widths: false,
            arrays: false,
            heavy: 2,
            folds: false,
            oracle: false,
        },
        600,
        false,
    );
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn mixed_width_formulas_agree_with_z3() {
    run_batch(
        "mixed",
        Shape {
            widths: &[1, 2, 3, 4, 5, 7, 8, 9, 12, 16, 17, 24, 32, 64],
            depth: env_u64("FISSION_DIFF_DEPTH", 3) as u32,
            mixed_widths: true,
            arrays: false,
            heavy: 2,
            folds: false,
            oracle: false,
        },
        1000,
        false,
    );
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn array_formulas_agree_with_z3() {
    run_batch(
        "arrays",
        Shape {
            widths: &[1, 2, 3, 4, 8, 16, 32],
            depth: env_u64("FISSION_DIFF_DEPTH", 3) as u32,
            mixed_widths: false,
            arrays: true,
            heavy: 1,
            folds: false,
            oracle: false,
        },
        1000,
        false,
    );
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn formulas_built_through_the_constructors_agree_with_z3() {
    run_batch(
        "constructors",
        Shape {
            widths: NARROW,
            depth: env_u64("FISSION_DIFF_DEPTH", 3) as u32,
            mixed_widths: true,
            arrays: true,
            heavy: 2,
            folds: true,
            oracle: false,
        },
        1000,
        true,
    );
}

// ----- one solver, many questions ------------------------------------------

/// A solver that is asked more than once: assertions arrive between queries,
/// queries carry extra conditions that must not outlive them, and the
/// derived questions -- always true, always false, some values, the least and
/// the greatest -- each have an answer z3 can confirm with satisfiability
/// alone. `Err` is the first answer that z3 contradicts.
fn incremental_case(
    z3: &str,
    gen: &mut Gen,
    log: &Mutex<Vec<String>>,
    clock: &Clock,
) -> Result<(), String> {
    let ask = |assertions: &[SymExpr]| {
        let script = smtlib::script(assertions).expect("printable");
        clock.time("z3", || run_z3(z3, &script))
    };
    let with = |asserted: &[SymExpr], extra: &[SymExpr]| -> Vec<SymExpr> {
        asserted.iter().chain(extra).cloned().collect()
    };
    let mut solver = Solver::new();
    configure(&mut solver);
    let gave_up = |solver: &Solver| {
        let gave_up = solver.last_unknown() == Some(&UnknownReason::ConflictBudget);
        if gave_up {
            clock.count("gave up");
        }
        gave_up
    };
    let mut asserted: Vec<SymExpr> = Vec::new();
    let mut registered: Vec<(SymExpr, u32)> = Vec::new();
    let depth = gen.shape.depth;

    for step in 0..3 + gen.below(5) {
        let kind = if step == 0 { 0 } else { gen.below(8) };
        // Everything this step mentions, built before the solver sees any of it.
        let condition = gen.boolean(depth);
        let extra: Vec<SymExpr> = (0..gen.below(2)).map(|_| gen.boolean(depth)).collect();
        let term = {
            let width = gen.pick_width();
            gen.bv(width, depth)
        };
        for var in &gen.vars {
            if !registered.iter().any(|(known, _)| known == var) {
                registered.push((var.clone(), solver.register_node(var.clone())));
            }
        }
        let width = term.get_bit_width();

        match kind {
            // Assert, or query under extra conditions: a verdict and a model.
            0..=2 => {
                let extras: Vec<SymExpr> = if kind == 0 {
                    log.lock().unwrap().push(format!("assert {condition:?}"));
                    solver.assert(condition.clone());
                    asserted.push(condition);
                    Vec::new()
                } else {
                    let mut extras = extra;
                    extras.push(condition);
                    log.lock().unwrap().push(format!("check under {extras:?}"));
                    extras
                };
                let ours = clock.time(if kind == 0 { "assert" } else { "check" }, || {
                    solver.check_sat_with_oracle(None, &extras).expect("check")
                });
                let all = with(&asserted, &extras);
                match (ours, ask(&all)) {
                    (SatResult::Unknown, _) if gave_up(&solver) => {}
                    (SatResult::Unknown, _) => {
                        return Err(format!("declined: {:?}", why_unknown(&solver)))
                    }
                    (_, Verdict::Unknown) => {}
                    (SatResult::Unsat, Verdict::Unsat) => {}
                    (SatResult::Sat, Verdict::Sat) => {
                        let mut pinned = all.clone();
                        for (var, id) in &registered {
                            let value = solver.get_value(*id).expect("model value");
                            pinned.push(pin(var, value));
                        }
                        for select in selects_of(&all) {
                            let value = solver
                                .bv_theory
                                .evaluate_expr_in_model(&solver.sat, &select);
                            pinned.push(pin(&select, value));
                        }
                        if ask(&pinned) == Verdict::Unsat {
                            return Err(format!(
                                "the model is not one:\n{}",
                                smtlib::script(&pinned).unwrap()
                            ));
                        }
                    }
                    (ours, theirs) => {
                        return Err(format!(
                            "{ours:?} here, {theirs:?} in z3:\n{}",
                            smtlib::script(&all).unwrap()
                        ))
                    }
                }
            }
            // Always true / always false.
            3 | 4 => {
                let constant = matches!(condition, SymExpr::Const { .. });
                let (ours, refutation) = if kind == 3 {
                    log.lock().unwrap().push(format!("is_true {condition:?}"));
                    (
                        clock.time("is_true", || solver.is_true(&condition)),
                        SymExpr::Eq(Box::new(condition), Box::new(SymExpr::new_const(0, 1))),
                    )
                } else {
                    log.lock().unwrap().push(format!("is_false {condition:?}"));
                    (
                        clock.time("is_false", || solver.is_false(&condition)),
                        condition,
                    )
                };
                let all = with(&asserted, &[refutation]);
                let expected = match ask(&all) {
                    Verdict::Unsat => true,
                    Verdict::Sat => false,
                    Verdict::Unknown => continue,
                };
                // "Not proven" is the honest answer when the search gave up.
                // And a constant is answered without consulting the context
                // at all, so in a contradictory context -- where everything
                // holds -- `is_true(0)` says no. That is the cautious side.
                let cautious = expected && (gave_up(&solver) || constant);
                if ours != expected && cautious && constant {
                    clock.count("constant");
                }
                if ours != expected && !cautious {
                    return Err(format!(
                        "answered {ours}, and z3 says the refutation is {}:\n{}",
                        if expected { "unsat" } else { "sat" },
                        smtlib::script(&all).unwrap()
                    ));
                }
            }
            // Some values of a term.
            5 => {
                let wanted = 1 + gen.below(3) as usize;
                log.lock()
                    .unwrap()
                    .push(format!("eval {wanted} of {term:?}"));
                let values = clock.time("eval", || solver.eval(&term, wanted));
                // A search that gave up found fewer values than there are;
                // the ones it did find still have to be real.
                let complete = !gave_up(&solver);
                let context = ask(&asserted);
                if context == Verdict::Unknown {
                    continue;
                }
                if (complete || !values.is_empty())
                    && values.is_empty() != (context == Verdict::Unsat)
                {
                    return Err(format!(
                        "eval gave {values:?} in a context z3 calls {context:?}"
                    ));
                }
                for (i, value) in values.iter().enumerate() {
                    if values[..i].contains(value) {
                        return Err(format!("eval repeated a value: {values:?}"));
                    }
                    let all = with(&asserted, &[pin(&term, *value)]);
                    if ask(&all) == Verdict::Unsat {
                        return Err(format!(
                            "eval gave {value:#x}, which the term cannot be:\n{}",
                            smtlib::script(&all).unwrap()
                        ));
                    }
                }
                if complete && !values.is_empty() && values.len() < wanted {
                    let others: Vec<SymExpr> = values
                        .iter()
                        .map(|value| {
                            SymExpr::Neq(
                                Box::new(term.clone()),
                                Box::new(SymExpr::new_const(*value, width)),
                            )
                        })
                        .collect();
                    let all = with(&asserted, &others);
                    if ask(&all) == Verdict::Sat {
                        return Err(format!(
                            "eval stopped at {values:?} with more to find:\n{}",
                            smtlib::script(&all).unwrap()
                        ));
                    }
                }
            }
            // The least and the greatest value of a term.
            _ => {
                let least = kind == 6;
                log.lock()
                    .unwrap()
                    .push(format!("{} of {term:?}", if least { "min" } else { "max" }));
                let ours = if least {
                    clock.time("min", || solver.min(&term))
                } else {
                    clock.time("max", || solver.max(&term))
                };
                let context = ask(&asserted);
                if context == Verdict::Unknown {
                    continue;
                }
                let Some(bound) = ours else {
                    if context == Verdict::Sat && !gave_up(&solver) {
                        return Err("no extremum in a satisfiable context".to_string());
                    }
                    continue;
                };
                let reached = with(&asserted, &[pin(&term, bound)]);
                if ask(&reached) == Verdict::Unsat {
                    return Err(format!(
                        "extremum {bound:#x} is not a value of the term:\n{}",
                        smtlib::script(&reached).unwrap()
                    ));
                }
                let constant = Box::new(SymExpr::new_const(bound, width));
                let beyond = if least {
                    SymExpr::Ult(Box::new(term.clone()), constant)
                } else {
                    SymExpr::Ult(constant, Box::new(term.clone()))
                };
                let beyond = with(&asserted, &[beyond]);
                if ask(&beyond) == Verdict::Sat {
                    return Err(format!(
                        "extremum {bound:#x} is not extreme:\n{}",
                        smtlib::script(&beyond).unwrap()
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Where a batch's time went, by kind of question.
#[derive(Default)]
struct Clock(Mutex<BTreeMap<&'static str, (u64, Duration)>>);

impl Clock {
    fn time<T>(&self, what: &'static str, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let out = f();
        let elapsed = started.elapsed();
        let mut spent = self.0.lock().unwrap();
        let entry = spent.entry(what).or_default();
        entry.0 += 1;
        entry.1 += elapsed;
        out
    }

    fn count(&self, what: &'static str) {
        self.0.lock().unwrap().entry(what).or_default().0 += 1;
    }

    fn report(&self) {
        for (what, (count, total)) in self.0.lock().unwrap().iter() {
            eprintln!("  {what:<9} {count:>6} calls, {total:?}");
        }
    }
}

fn run_incremental(name: &str, shape: Shape, default_cases: u64) {
    let Some(z3) = z3_path() else {
        eprintln!("skipping: no z3 found (set FISSION_Z3)");
        return;
    };
    let cases = env_u64("FISSION_DIFF_CASES", default_cases);
    let seed = env_u64("FISSION_DIFF_SEED", 0x5EED_F155_1017);
    let only = std::env::var("FISSION_DIFF_ONLY")
        .ok()
        .and_then(|v| v.parse::<u64>().ok());
    let jobs = env_u64("FISSION_DIFF_JOBS", 4).max(1);

    let hang = Duration::from_secs(env_u64("FISSION_DIFF_HANG_SECS", 300));

    let next = AtomicU64::new(0);
    let failures: Mutex<Vec<(u64, String, Vec<String>)>> = Mutex::new(Vec::new());
    let steps = AtomicU64::new(0);
    let clock = Clock::default();
    type Session = (Instant, std::sync::Arc<Mutex<Vec<String>>>);
    let running: Mutex<BTreeMap<u64, Session>> = Mutex::new(BTreeMap::new());
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(500));
                for (case, (started, log)) in running.lock().unwrap().iter() {
                    if started.elapsed() > hang {
                        eprintln!(
                            "{name}: session {case} (seed {seed}) has run for {:?}:\n{}",
                            started.elapsed(),
                            log.lock().unwrap().join("\n")
                        );
                        std::process::abort();
                    }
                }
            }
        });
        let workers: Vec<_> = (0..jobs)
            .map(|_| {
                scope.spawn(|| loop {
                    let case = next.fetch_add(1, Ordering::Relaxed);
                    if case >= cases {
                        break;
                    }
                    if only.is_some_and(|only| only != case) {
                        continue;
                    }
                    let mut gen = Gen::new(case_rng(seed, case), shape);
                    let log = std::sync::Arc::new(Mutex::new(Vec::new()));
                    running
                        .lock()
                        .unwrap()
                        .insert(case, (Instant::now(), log.clone()));
                    let result = incremental_case(&z3, &mut gen, &log, &clock);
                    running.lock().unwrap().remove(&case);
                    let log = log.lock().unwrap().clone();
                    steps.fetch_add(log.len() as u64, Ordering::Relaxed);
                    if let Err(why) = result {
                        failures.lock().unwrap().push((case, why, log));
                    }
                })
            })
            .collect();
        for worker in workers {
            if worker.join().is_err() {
                done.store(true, Ordering::Relaxed);
                panic!("{name}: a worker panicked");
            }
        }
        done.store(true, Ordering::Relaxed);
    });

    let mut failures = failures.into_inner().unwrap();
    failures.sort_by_key(|(case, _, log)| (log.len(), *case));
    eprintln!(
        "{name}: {cases} sessions, {} queries, seed {seed}, {} failed",
        steps.load(Ordering::Relaxed),
        failures.len()
    );
    clock.report();
    if let Some((case, why, log)) = failures.first() {
        panic!(
            "{name}: session {case} (seed {seed}), failing cases {:?}\n{}\n=> {why}",
            failures
                .iter()
                .map(|(case, _, _)| *case)
                .collect::<Vec<_>>(),
            log.join("\n")
        );
    }
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn a_solver_asked_many_questions_agrees_with_z3() {
    run_incremental(
        "incremental",
        Shape {
            widths: NARROW,
            depth: env_u64("FISSION_DIFF_DEPTH", 2) as u32,
            mixed_widths: true,
            arrays: true,
            heavy: 2,
            folds: false,
            oracle: false,
        },
        400,
    );
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn a_solver_asked_many_wide_questions_agrees_with_z3() {
    run_incremental(
        "incremental-wide",
        Shape {
            widths: &[8, 16, 32, 64],
            depth: env_u64("FISSION_DIFF_DEPTH", 2) as u32,
            mixed_widths: false,
            arrays: true,
            heavy: 1,
            folds: false,
            oracle: false,
        },
        200,
    );
}

#[test]
#[ignore = "needs a z3 binary; run with --ignored"]
fn memory_backed_formulas_agree_with_z3() {
    run_batch(
        "memory",
        Shape {
            widths: &[1, 2, 3, 4, 8, 8, 8, 16],
            depth: env_u64("FISSION_DIFF_DEPTH", 3) as u32,
            mixed_widths: false,
            arrays: true,
            heavy: 1,
            folds: false,
            oracle: true,
        },
        1000,
        false,
    );
}
