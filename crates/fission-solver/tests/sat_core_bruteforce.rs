//! The SAT core against brute force, clause by clause.
//!
//! Random CNFs small enough to enumerate (at most 8 variables), fed to
//! `add_clause` in random order with unit clauses mixed in -- the order the
//! bit-vector layer uses, where an assertion's unit arrives before the gate
//! clauses that give it meaning. Every verdict is compared with enumeration,
//! and every SAT verdict's model is checked against every clause.

use fission_solver::cnf::Lit;
use fission_solver::sat::{LBool, SatSolver};

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

fn satisfiable_by_enumeration(vars: u32, clauses: &[Vec<Lit>]) -> bool {
    (0u32..1 << vars).any(|assignment| {
        clauses.iter().all(|clause| {
            clause.iter().any(|lit| {
                let value = assignment & (1 << (lit.var() - 1)) != 0;
                if lit.0 > 0 {
                    value
                } else {
                    !value
                }
            })
        })
    })
}

#[test]
fn the_sat_core_agrees_with_enumeration() {
    let mut rng = Rng(0xC0FF_EE12_3457);
    let mut failures: Vec<(u32, Vec<Vec<Lit>>, bool, bool)> = Vec::new();
    let mut model_failures = 0usize;
    let cases = 20_000;

    for _ in 0..cases {
        let vars = 3 + rng.below(6) as u32;
        let count = 1 + rng.below(20) as usize;
        let clauses: Vec<Vec<Lit>> = (0..count)
            .map(|_| {
                let len = 1 + rng.below(3) as usize;
                (0..len)
                    .map(|_| Lit::new(1 + rng.below(vars as u64) as u32, rng.below(2) == 0))
                    .collect()
            })
            .collect();

        let want = satisfiable_by_enumeration(vars, &clauses);
        let mut sat = SatSolver::new();
        let mut accepted = true;
        for clause in &clauses {
            if !sat.add_clause(clause.clone()) {
                accepted = false;
                break;
            }
        }
        let got = accepted && sat.solve();

        if got != want {
            failures.push((vars, clauses, want, got));
            continue;
        }
        if got {
            let model_ok = clauses.iter().all(|clause| {
                clause.iter().any(|lit| {
                    let value = matches!(sat.get_var_value(lit.var()), LBool::True);
                    if lit.0 > 0 {
                        value
                    } else {
                        !value
                    }
                })
            });
            if !model_ok {
                model_failures += 1;
            }
        }
    }

    failures.sort_by_key(|(_, clauses, _, _)| clauses.len());
    eprintln!(
        "{cases} CNFs: {} wrong verdicts, {model_failures} SAT verdicts with a model that violates a clause",
        failures.len()
    );
    if let Some((vars, clauses, want, got)) = failures.first() {
        let text: Vec<String> = clauses
            .iter()
            .map(|c| {
                format!(
                    "({})",
                    c.iter()
                        .map(|l| l.0.to_string())
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect();
        panic!(
            "smallest: {vars} vars, expected {} got {}: {}",
            if *want { "SAT" } else { "UNSAT" },
            if *got { "SAT" } else { "UNSAT" },
            text.join(" ")
        );
    }
    assert_eq!(model_failures, 0, "a SAT model violated a clause");
}

/// Once `add_clause` has said "unsatisfiable", that is a fact about every later
/// question. Callers ignored the `false` -- the bit-vector loader stopped at it
/// and dropped the remaining clauses, `Solver::assert` discarded it -- and the
/// next `solve` ran on a partial clause set and answered SAT.
#[test]
fn unsatisfiability_is_not_forgotten_by_the_next_question() {
    let mut rng = Rng(0x5117_C4E1);
    let mut checked = 0;
    for _ in 0..20_000 {
        let vars = 3 + rng.below(6) as u32;
        let count = 2 + rng.below(20) as usize;
        let clauses: Vec<Vec<Lit>> = (0..count)
            .map(|_| {
                let len = 1 + rng.below(3) as usize;
                (0..len)
                    .map(|_| Lit::new(1 + rng.below(vars as u64) as u32, rng.below(2) == 0))
                    .collect()
            })
            .collect();
        if satisfiable_by_enumeration(vars, &clauses) {
            continue;
        }

        // Add everything regardless of what `add_clause` says, the way a
        // careless caller would, then ask.
        let mut sat = SatSolver::new();
        let mut refused_early = false;
        for clause in &clauses {
            if !sat.add_clause(clause.clone()) {
                refused_early = true;
            }
        }
        if refused_early {
            checked += 1;
            assert!(
                !sat.solve(),
                "add_clause reported UNSAT, and solve then found a model: {clauses:?}"
            );
            assert!(
                !sat.add_clause(vec![Lit::new(1, false)]),
                "an inconsistent solver accepted a new clause"
            );
        }
    }
    assert!(
        checked > 100,
        "too few early refutations exercised: {checked}"
    );
}

/// Problems big enough to make the solver collect learned clauses.
///
/// The small cases above rarely reach a hundred conflicts, so the learned-
/// clause collector never ran in them. It decides what is "learned" by index
/// -- everything at or after `learned_start` -- and nothing in the solver ever
/// set that boundary, so every clause, input included, was a candidate for
/// deletion. Bit-vector problems from five bits up hit it: a dropped input
/// clause is a lost constraint, and the answer is a false SAT.
#[test]
fn the_sat_core_agrees_with_enumeration_on_problems_that_trigger_clause_collection() {
    let mut rng = Rng(0x6C_0DD_BA11);
    let mut wrong = 0usize;
    let mut first: Option<String> = None;
    let cases = 400;
    for case in 0..cases {
        let vars = 14 + rng.below(3) as u32; // 14..16: enumerable, and hard
                                             // Near the 3-SAT phase transition (clauses ~ 4.26 * vars), where
                                             // search needs many conflicts.
        let count = (vars as u64 * 4 + rng.below(vars as u64 / 2 + 1)) as usize;
        let clauses: Vec<Vec<Lit>> = (0..count)
            .map(|_| {
                (0..3)
                    .map(|_| Lit::new(1 + rng.below(vars as u64) as u32, rng.below(2) == 0))
                    .collect()
            })
            .collect();
        let want = satisfiable_by_enumeration(vars, &clauses);
        let mut sat = SatSolver::new();
        let mut accepted = true;
        for clause in &clauses {
            accepted &= sat.add_clause(clause.clone());
        }
        let got = accepted && sat.solve();
        let model_ok = !got
            || clauses.iter().all(|clause| {
                clause.iter().any(|lit| {
                    let value = matches!(sat.get_var_value(lit.var()), LBool::True);
                    if lit.0 > 0 {
                        value
                    } else {
                        !value
                    }
                })
            });
        if got != want || !model_ok {
            wrong += 1;
            first.get_or_insert_with(|| {
                format!("case {case}: {vars} vars, {count} clauses, expected sat={want}, got sat={got}, model ok={model_ok}")
            });
        }
    }
    eprintln!("{cases} hard CNFs: {wrong} wrong");
    assert_eq!(wrong, 0, "{}", first.unwrap_or_default());
}

/// Pigeonhole: `pigeons` pigeons into `pigeons - 1` holes, one pigeon per
/// hole. Unsatisfiable, and famously expensive for resolution -- so a CDCL
/// solver has to learn and collect many clauses to answer it. Too large to
/// enumerate, but the answer is known.
///
/// Collection used to delete *input* clauses: it paired learned-clause
/// metadata with clause indices as if every clause were learned, because the
/// boundary between input and learned clauses was never set. A deleted input
/// clause is a lost constraint, and this came back SAT.
#[test]
fn pigeonhole_is_unsatisfiable_even_after_clause_collection() {
    for pigeons in 5..=8u32 {
        let holes = pigeons - 1;
        let var = |p: u32, h: u32| Lit::new(p * holes + h + 1, false);
        let mut sat = SatSolver::new();
        let mut accepted = true;
        for p in 0..pigeons {
            accepted &= sat.add_clause((0..holes).map(|h| var(p, h)).collect());
        }
        for h in 0..holes {
            for p in 0..pigeons {
                for q in p + 1..pigeons {
                    accepted &= sat.add_clause(vec![var(p, h).not(), var(q, h).not()]);
                }
            }
        }
        assert!(
            !(accepted && sat.solve()),
            "{pigeons} pigeons fit into {holes} holes, says the solver"
        );
    }
}
