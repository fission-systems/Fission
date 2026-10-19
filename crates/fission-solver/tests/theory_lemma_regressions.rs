use fission_solver::cnf::Lit;
use fission_solver::sat::{LBool, SatSolver};
use fission_solver::theory::{Theory, TheoryStatus};

struct Once(Option<Vec<Vec<Lit>>>);
impl Theory for Once {
    fn check(&mut self, _: &[Lit]) -> TheoryStatus {
        self.0
            .take()
            .map(TheoryStatus::Lemmas)
            .unwrap_or(TheoryStatus::Satisfied)
    }
}
fn lit(var: u32, negated: bool) -> Lit {
    Lit::new(var, negated)
}

#[test]
fn theory_units_do_not_drop_or_override_assumptions() {
    let mut sat = SatSolver::new();
    sat.add_clause(vec![lit(1, false), lit(1, true)]);
    let mut theory = Once(Some(vec![vec![lit(1, true)]]));
    assert!(!sat.solve_with_assumptions(Some(&mut theory), &[lit(1, false)]));
    assert!(sat.solve_with_assumptions(None, &[lit(1, true)]));
    assert!(!sat.solve_with_assumptions(None, &[lit(1, false)]));
}
#[test]
fn conflicting_theory_units_remain_unsatisfiable() {
    let mut sat = SatSolver::new();
    sat.add_clause(vec![lit(1, false)]);
    let mut theory = Once(Some(vec![vec![lit(1, true)]]));
    assert!(!sat.solve_with_theory(Some(&mut theory)));
    assert!(!sat.solve());
}
#[test]
fn theory_clauses_propagate_past_already_false_watches() {
    let mut sat = SatSolver::new();
    for var in 1..=3 {
        sat.add_clause(vec![lit(var, false), lit(var, true)]);
    }
    let mut theory = Once(Some(vec![vec![lit(1, true), lit(2, true), lit(3, false)]]));
    assert!(sat.solve_with_assumptions(Some(&mut theory), &[lit(1, false), lit(2, false)]));
    assert_eq!(sat.get_var_value(3), LBool::True);
}
#[test]
fn theory_can_introduce_new_boolean_variables() {
    let mut sat = SatSolver::new();
    let mut theory = Once(Some(vec![vec![lit(5, false)]]));
    assert!(sat.solve_with_theory(Some(&mut theory)));
    assert_eq!(sat.get_var_value(5), LBool::True);
    let mut sat = SatSolver::new();
    let mut theory = Once(Some(vec![vec![lit(5, false), lit(6, false)]]));
    assert!(sat.solve_with_theory(Some(&mut theory)));
    assert!(sat.get_var_value(5) == LBool::True || sat.get_var_value(6) == LBool::True);
}
#[test]
fn empty_theory_clause_is_a_permanent_conflict() {
    let mut sat = SatSolver::new();
    let mut theory = Once(Some(vec![vec![]]));
    assert!(!sat.solve_with_theory(Some(&mut theory)));
    assert!(!sat.solve());
}

#[test]
fn delayed_theory_lemmas_match_brute_force_with_assumptions() {
    struct Delayed(Once);
    impl Theory for Delayed {
        fn check(&mut self, assignments: &[Lit]) -> TheoryStatus {
            if assignments.len() < 5 {
                TheoryStatus::Satisfied
            } else {
                self.0.check(assignments)
            }
        }
    }
    let mut seed = 0xabcde123456789u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..2000 {
        let clauses: Vec<Vec<Lit>> = (0..8)
            .map(|_| {
                (0..1 + next() % 3)
                    .map(|_| {
                        let x = next();
                        lit(1 + (x % 5) as u32, x & 32 != 0)
                    })
                    .collect()
            })
            .collect();
        let assumptions = [lit(1, next() & 1 != 0), lit(2, next() & 1 != 0)];
        let expected = (0..32u32).any(|bits| {
            let holds = |l: Lit| ((bits >> (l.var() - 1)) & 1 != 0) != (l.0 < 0);
            assumptions.iter().all(|&l| holds(l))
                && clauses.iter().all(|c| c.iter().any(|&l| holds(l)))
        });
        let mut sat = SatSolver::new();
        for v in 1..=5 {
            sat.add_clause(vec![lit(v, false), lit(v, true)]);
        }
        for c in &clauses[..4] {
            sat.add_clause(c.clone());
        }
        let mut theory = Delayed(Once(Some(clauses[4..].to_vec())));
        let got = sat.solve_with_assumptions(Some(&mut theory), &assumptions);
        assert_eq!(
            got, expected,
            "clauses={clauses:?} assumptions={assumptions:?}"
        );
        if got {
            let holds = |l: Lit| (sat.get_var_value(l.var()) == LBool::True) != (l.0 < 0);
            assert!(assumptions.iter().all(|&l| holds(l)));
            assert!(clauses.iter().all(|c| c.iter().any(|&l| holds(l))));
        }
    }
}
