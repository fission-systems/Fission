//! The persistent assertions and the backend they are lowered into (AIG, CNF,
//! SAT clauses including learned ones, array-theory terms, the variable
//! identity table, the published model) are one state.
//!
//! `Solver::assertions` and the lowering cursor used to be public fields. A
//! caller who edited the list in place left the clauses of the old assertion
//! in the SAT core and the cursor pointing past assertions it had never
//! lowered, so the solver answered for a problem nobody had asked:
//!
//! | edit                                   | incremental | fresh   |
//! |----------------------------------------|-------------|---------|
//! | replace `x == 0` by `x == 1`, ask `x == 1` | Unsat   | Sat     |
//! | remove `x == 0`, ask `x == 1`          | Unsat       | Sat     |
//! | clear, ask `x == 1`                    | Unsat       | Sat     |
//! | push `x == 9` unlowered, swap to front | Sat         | Unsat   |
//! | truncate to 0, push `x == 3`, `x == 4` | Sat         | Unsat   |
//!
//! Each test below is one of those edits made through the controlled API,
//! checked against a fresh solver and, where the domain is small, against
//! brute-force enumeration (no solver at all).

use fission_solver::{
    AbsentModelReason, ModelLookup, ModelValue, SatResult, Solver, Sort, SymExpr, UnknownReason,
    ValidationError, MAX_EXPR_DEPTH,
};

fn eq(a: &SymExpr, v: u64) -> SymExpr {
    let width = a.get_bit_width();
    SymExpr::Eq(Box::new(a.clone()), Box::new(SymExpr::new_const(v, width)))
}

fn ule(a: &SymExpr, v: u64) -> SymExpr {
    let width = a.get_bit_width();
    SymExpr::Ule(Box::new(a.clone()), Box::new(SymExpr::new_const(v, width)))
}

fn var(id: u32, bits: u32) -> SymExpr {
    SymExpr::Var {
        id,
        name: format!("v{id}"),
        sort: Sort::BitVector(bits),
    }
}

/// The verdict of a solver built from nothing but `assertions`.
fn fresh(assertions: &[SymExpr], extra: &[SymExpr]) -> SatResult {
    let mut solver = Solver::new();
    for assertion in assertions {
        solver.assert(assertion.clone());
    }
    solver.check_sat_with_oracle(None, extra).unwrap()
}

fn agrees_with_fresh(solver: &mut Solver, extra: &[SymExpr]) -> SatResult {
    let incremental = solver.check_sat_with_oracle(None, extra).unwrap();
    let expected = fresh(solver.assertions(), extra);
    assert_eq!(
        incremental,
        expected,
        "incremental and fresh solvers disagree on {:?} + {:?}",
        solver.assertions(),
        extra
    );
    incremental
}

#[test]
fn replacing_an_assertion_drops_the_clauses_of_the_old_one() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(eq(&x, 0));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    let old = solver.replace_assertion(0, eq(&x, 1));
    assert_eq!(old, Some(eq(&x, 0)));
    assert_eq!(agrees_with_fresh(&mut solver, &[eq(&x, 1)]), SatResult::Sat);
    assert_eq!(
        agrees_with_fresh(&mut solver, &[eq(&x, 0)]),
        SatResult::Unsat
    );
    assert_eq!(solver.replace_assertion(7, eq(&x, 2)), None);
    assert_eq!(solver.assertions(), &[eq(&x, 1)]);
}

#[test]
fn removing_an_assertion_drops_its_clauses() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(eq(&x, 0));
    solver.assert(ule(&x, 5));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.remove_assertion(0), Some(eq(&x, 0)));
    assert_eq!(agrees_with_fresh(&mut solver, &[eq(&x, 1)]), SatResult::Sat);
    assert_eq!(
        agrees_with_fresh(&mut solver, &[eq(&x, 6)]),
        SatResult::Unsat
    );
    assert_eq!(solver.remove_assertion(1), None);
}

#[test]
fn clearing_assertions_drops_every_clause_and_keeps_registered_nodes() {
    let mut solver = Solver::new();
    let id = solver.register_var("x".to_string(), 8);
    let x = solver.node(id).unwrap().clone();
    solver.assert(eq(&x, 0));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    solver.clear_assertions();
    assert!(solver.assertions().is_empty());
    assert_eq!(agrees_with_fresh(&mut solver, &[eq(&x, 1)]), SatResult::Sat);
    assert_eq!(solver.get_value(id), Some(1));
    // The registered declaration survives the rebuild.
    assert_eq!(solver.identity().sort_of(id), Some(&Sort::BitVector(8)));
}

#[test]
fn reordering_never_skips_an_assertion() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(ule(&x, 5));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    solver.assert(eq(&x, 9));
    assert!(solver.swap_assertions(0, 1));
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Unsat);
    assert!(!solver.swap_assertions(0, 2));
    assert_eq!(solver.lowered_assertion_count(), 2);
}

#[test]
fn truncation_then_append_lowers_the_new_assertions() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(eq(&x, 0));
    solver.assert(eq(&x, 0));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    solver.truncate_assertions(0);
    solver.assert(eq(&x, 3));
    solver.assert(eq(&x, 4));
    assert_eq!(solver.lowered_assertion_count(), 2);
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Unsat);
    solver.truncate_assertions(1);
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Sat);
    // Truncating to a longer length changes nothing.
    let revision = solver.assertion_revision();
    solver.truncate_assertions(5);
    assert_eq!(solver.assertion_revision(), revision);
}

#[test]
fn every_edit_withdraws_the_published_model() {
    type Edit = fn(&mut Solver, &SymExpr);
    let edits: [(&str, Edit); 6] = [
        ("assert", |s, x| s.assert(ule(x, 200))),
        ("replace", |s, x| {
            s.replace_assertion(0, ule(x, 200));
        }),
        ("remove", |s, _| {
            s.remove_assertion(0);
        }),
        ("truncate", |s, _| s.truncate_assertions(0)),
        ("clear", |s, _| s.clear_assertions()),
        ("swap", |s, x| {
            s.assert(ule(x, 250));
            s.check_sat().unwrap();
            s.swap_assertions(0, 1);
        }),
    ];
    for (name, edit) in edits {
        let mut solver = Solver::new();
        let id = solver.register_var("x".to_string(), 8);
        let x = solver.node(id).unwrap().clone();
        solver.assert(eq(&x, 7));
        assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
        assert_eq!(solver.get_value(id), Some(7), "{name}");
        let before = solver.assertion_revision();
        edit(&mut solver, &x);
        assert_ne!(solver.assertion_revision(), before, "{name}");
        assert_eq!(solver.get_value(id), None, "{name}");
        assert!(solver.published_model().is_none(), "{name}");
        assert!(solver.model().is_empty(), "{name}");
        assert_eq!(solver.eval_in_model(&x), None, "{name}");
        assert_eq!(
            solver.get_model_value(id),
            ModelLookup::Absent(AbsentModelReason::AssertionsModified),
            "{name}"
        );
    }
}

#[test]
fn an_invalid_assumption_leaves_no_binding_behind() {
    let mut solver = Solver::new();
    let id = 900_001;
    let wide = var(id, 16);
    // Well formed up to the last conjunct, which is not: the binding of `id`
    // as 16 bits made on the way must not survive the rejection.
    let rejected = SymExpr::And(
        Box::new(eq(&wide, 3)),
        Box::new(SymExpr::Extract {
            expr: Box::new(wide.clone()),
            lsb: 12,
            size: 8,
        }),
    );
    assert_eq!(
        solver.check_sat_with_oracle(None, &[rejected]).unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::InvalidExpression(_))
    ));
    assert!(solver.identity().sort_of(id).is_none());

    // The same ID as 8 bits is therefore a fresh, consistent declaration.
    let narrow = var(id, 8);
    assert_eq!(
        solver
            .check_sat_with_oracle(None, &[eq(&narrow, 200)])
            .unwrap(),
        SatResult::Sat
    );
    assert_eq!(solver.last_unknown(), None);
    assert_eq!(solver.identity().sort_of(id), Some(&Sort::BitVector(8)));
}

#[test]
fn a_variable_id_reused_with_another_sort_across_queries_is_rejected() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(ule(&x, 100));
    let id = 900_002;
    assert_eq!(
        solver
            .check_sat_with_oracle(None, &[eq(&var(id, 8), 1)])
            .unwrap(),
        SatResult::Sat
    );
    // Lowered once as 8 bits, the ID has AIG bits of that width; reading it
    // as 16 bits is an identity conflict, reported as such.
    assert_eq!(
        solver
            .check_sat_with_oracle(None, &[eq(&var(id, 16), 1)])
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::InvalidExpression(errs))
            if matches!(errs.as_slice(), [ValidationError::ConflictingVarSort { id: 900_002, .. }])
    ));
    // The rejected query does not affect the next valid one.
    assert_eq!(
        solver.check_sat_with_oracle(None, &[eq(&x, 100)]).unwrap(),
        SatResult::Sat
    );
    assert_eq!(
        solver.check_sat_with_oracle(None, &[eq(&x, 101)]).unwrap(),
        SatResult::Unsat
    );
}

#[test]
fn an_invalid_persistent_assertion_is_sticky_until_removed() {
    let mut solver = Solver::new();
    let x = SymExpr::new_var("x", 8);
    solver.assert(ule(&x, 3));
    let bad = SymExpr::Eq(Box::new(var(900_003, 8)), Box::new(var(900_003, 16)));
    solver.assert(bad.clone());
    solver.assert(eq(&x, 2));
    assert_eq!(solver.lowered_assertion_count(), 1);
    for _ in 0..2 {
        assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
        assert!(matches!(
            solver.last_unknown(),
            Some(UnknownReason::InvalidExpression(_))
        ));
    }
    assert!(!solver.is_true(&ule(&x, 3)));
    assert_eq!(solver.remove_assertion(1), Some(bad.clone()));
    assert_eq!(solver.lowered_assertion_count(), 2);
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Sat);
    assert!(solver.is_true(&eq(&x, 2)));

    // `try_assert` refuses it without changing anything.
    let revision = solver.assertion_revision();
    assert!(matches!(
        solver.try_assert(bad),
        Err(ValidationError::ConflictingVarSort { id: 900_003, .. })
    ));
    assert_eq!(solver.assertion_revision(), revision);
    assert_eq!(solver.assertions().len(), 2);
    assert!(solver.try_assert(ule(&x, 9)).is_ok());
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Sat);
}

#[test]
fn insert_node_rejects_a_conflicting_declaration() {
    let mut solver = Solver::new();
    let x = var(900_004, 8);
    solver.assert(eq(&x, 5));
    assert!(matches!(
        solver.insert_node(900_004, var(900_004, 32)),
        Err(ValidationError::ConflictingVarSort { id: 900_004, .. })
    ));
    assert!(solver.node(900_004).is_none());
    assert_eq!(solver.insert_node(900_004, x.clone()), Ok(None));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(
        solver.get_model_value(900_004),
        ModelLookup::Value(ModelValue::Assigned(5))
    );
}

#[test]
fn eval_in_model_failures_are_local_to_the_call() {
    let mut solver = Solver::new();
    let id = solver.register_var("x".to_string(), 8);
    let x = solver.node(id).unwrap().clone();
    assert_eq!(solver.eval_in_model(&x), None, "no model yet");
    solver.assert(eq(&x, 0x2a));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.eval_in_model(&x), Some(ModelValue::Assigned(0x2a)));
    // Ill-formed, too wide, and float circuits it has none for: no value,
    // and none of them marks the solver's problem unsupported.
    assert_eq!(solver.eval_in_model(&var(id, 16)), None);
    assert_eq!(
        solver.eval_in_model(&SymExpr::Concat(
            Box::new(SymExpr::new_var("w", 64)),
            Box::new(x.clone())
        )),
        None
    );
    assert!(solver.bv_theory().unsupported().is_empty());
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.get_value(id), Some(0x2a));
}

/// Arrays: the select/store terms and lemmas of a removed assertion must go
/// with it.
#[test]
fn array_terms_follow_the_assertion_list() {
    let memory = SymExpr::new_array_var("m", 4, 8);
    let index = SymExpr::new_var("i", 4);
    let stored = SymExpr::ArrayStore {
        array: Box::new(memory.clone()),
        index: Box::new(index.clone()),
        value: Box::new(SymExpr::new_const(0x11, 8)),
    };
    let read_back = SymExpr::ArraySelect {
        array: Box::new(stored),
        index: Box::new(index.clone()),
    };
    let mut solver = Solver::new();
    // Read-over-write: the read is 0x11, never 0x22.
    solver.assert(eq(&read_back, 0x22));
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Unsat);
    solver.replace_assertion(0, eq(&read_back, 0x11));
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Sat);
    let plain_read = SymExpr::ArraySelect {
        array: Box::new(memory),
        index: Box::new(index),
    };
    solver.assert(eq(&plain_read, 0x33));
    assert_eq!(agrees_with_fresh(&mut solver, &[]), SatResult::Sat);
    solver.remove_assertion(0);
    assert_eq!(
        agrees_with_fresh(&mut solver, &[eq(&plain_read, 0x33)]),
        SatResult::Sat
    );
    assert_eq!(
        agrees_with_fresh(&mut solver, &[eq(&plain_read, 0x34)]),
        SatResult::Unsat
    );
}

/// A chain `((x + 1) + 1) ... + 1` of the given depth.
fn chain(x: &SymExpr, depth: usize) -> SymExpr {
    let mut expr = x.clone();
    for _ in 0..depth {
        expr = SymExpr::Add(Box::new(expr), Box::new(SymExpr::new_const(1, 16)));
    }
    expr
}

#[test]
fn deep_assertions_at_the_limit_solve_and_past_it_are_removable() {
    // Run on a big stack: the debug-build lowering of a 2,000-deep term
    // recurses as deep, and the default test thread is 2 MiB.
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(|| {
            let x = SymExpr::new_var("x", 16);
            // `Ule(chain, c)` is one level deeper than the chain.
            let depth = MAX_EXPR_DEPTH - 2;
            let mut solver = Solver::new();
            solver.assert(SymExpr::Eq(
                Box::new(chain(&x, depth)),
                Box::new(SymExpr::new_const(depth as u64 + 5, 16)),
            ));
            assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
            assert!(solver.is_true(&eq(&x, 5)));

            solver.assert(eq(&chain(&x, MAX_EXPR_DEPTH + 1), 0));
            assert_eq!(solver.check_sat().unwrap(), SatResult::Unknown);
            assert!(matches!(
                solver.last_unknown(),
                Some(UnknownReason::InvalidExpression(errs))
                    if matches!(errs.as_slice(), [ValidationError::DepthExceeded { .. }])
            ));
            solver.remove_assertion(1);
            assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
            assert!(solver.is_true(&eq(&x, 5)));
        })
        .unwrap()
        .join()
        .unwrap();
}

// ── Randomized differential against brute-force enumeration ────────────────

const VARS: usize = 3;
const BITS: u32 = 4;

#[derive(Clone, Debug)]
enum Atom {
    Ule(usize, u64),
    SumEq(usize, usize, u64),
    Neq(usize, u64),
    XorUlt(usize, usize, u64),
}

impl Atom {
    fn holds(&self, values: &[u64; VARS]) -> bool {
        let mask = (1u64 << BITS) - 1;
        match *self {
            Self::Ule(a, c) => values[a] <= c,
            Self::SumEq(a, b, c) => (values[a] + values[b]) & mask == c,
            Self::Neq(a, c) => values[a] != c,
            Self::XorUlt(a, b, c) => (values[a] ^ values[b]) < c,
        }
    }

    fn expr(&self, vars: &[SymExpr]) -> SymExpr {
        let k = |c: u64| SymExpr::new_const(c, BITS);
        let bx = |e: &SymExpr| Box::new(e.clone());
        match *self {
            Self::Ule(a, c) => SymExpr::Ule(bx(&vars[a]), Box::new(k(c))),
            Self::SumEq(a, b, c) => SymExpr::Eq(
                Box::new(SymExpr::Add(bx(&vars[a]), bx(&vars[b]))),
                Box::new(k(c)),
            ),
            Self::Neq(a, c) => SymExpr::Neq(bx(&vars[a]), Box::new(k(c))),
            Self::XorUlt(a, b, c) => SymExpr::Ult(
                Box::new(SymExpr::Xor(bx(&vars[a]), bx(&vars[b]))),
                Box::new(k(c)),
            ),
        }
    }
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn atom(&mut self) -> Atom {
        let a = self.below(VARS as u64) as usize;
        let b = self.below(VARS as u64) as usize;
        let c = self.below(1 << BITS);
        match self.below(4) {
            0 => Atom::Ule(a, c),
            1 => Atom::SumEq(a, b, c),
            2 => Atom::Neq(a, c),
            _ => Atom::XorUlt(a, b, c),
        }
    }
}

/// Satisfiable by enumeration of all `2^(VARS*BITS)` assignments.
fn brute_force(atoms: &[Atom]) -> SatResult {
    let size = 1u64 << BITS;
    for packed in 0..size.pow(VARS as u32) {
        let mut values = [0u64; VARS];
        for (i, value) in values.iter_mut().enumerate() {
            *value = (packed >> (i as u32 * BITS)) & (size - 1);
        }
        if atoms.iter().all(|atom| atom.holds(&values)) {
            return SatResult::Sat;
        }
    }
    SatResult::Unsat
}

/// A random program of appends, replacements, removals, swaps, truncations,
/// clears, rejected (ill-formed) queries and valid queries. After each valid
/// query the incremental verdict must equal both brute force and a fresh
/// solver, and a `Sat` model must satisfy every assertion and assumption.
#[test]
fn random_edit_sequences_agree_with_brute_force_and_a_fresh_solver() {
    let seeds = std::env::var("FISSION_STORE_DIFF_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(40u64);
    let mut queries = 0;
    for seed in 0..seeds {
        let mut rng = Lcg(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x5151);
        let mut solver = Solver::new();
        let ids: Vec<u32> = (0..VARS)
            .map(|i| solver.register_var(format!("x{i}"), BITS))
            .collect();
        let vars: Vec<SymExpr> = ids
            .iter()
            .map(|&id| solver.node(id).unwrap().clone())
            .collect();
        let mut atoms: Vec<Atom> = Vec::new();
        for _ in 0..60 {
            match rng.below(10) {
                0..=2 => {
                    let atom = rng.atom();
                    solver.assert(atom.expr(&vars));
                    atoms.push(atom);
                }
                3 if !atoms.is_empty() => {
                    let i = rng.below(atoms.len() as u64) as usize;
                    let atom = rng.atom();
                    solver.replace_assertion(i, atom.expr(&vars));
                    atoms[i] = atom;
                }
                4 if !atoms.is_empty() => {
                    let i = rng.below(atoms.len() as u64) as usize;
                    solver.remove_assertion(i);
                    atoms.remove(i);
                }
                5 if atoms.len() > 1 => {
                    let i = rng.below(atoms.len() as u64) as usize;
                    let j = rng.below(atoms.len() as u64) as usize;
                    solver.swap_assertions(i, j);
                    atoms.swap(i, j);
                }
                6 => {
                    let len = rng.below(atoms.len() as u64 + 1) as usize;
                    solver.truncate_assertions(len);
                    atoms.truncate(len);
                }
                7 if rng.below(4) == 0 => {
                    solver.clear_assertions();
                    atoms.clear();
                }
                7 => {
                    // A rejected query: one of the registered IDs read at
                    // another width.
                    let id = ids[rng.below(VARS as u64) as usize];
                    let bad = eq(&var(id, BITS * 2), rng.below(1 << BITS));
                    assert_eq!(
                        solver
                            .check_sat_with_oracle(None, &[rng.atom().expr(&vars), bad])
                            .unwrap(),
                        SatResult::Unknown
                    );
                }
                _ => {
                    let extra: Vec<Atom> = (0..rng.below(3)).map(|_| rng.atom()).collect();
                    let extra_exprs: Vec<SymExpr> =
                        extra.iter().map(|atom| atom.expr(&vars)).collect();
                    let incremental = solver.check_sat_with_oracle(None, &extra_exprs).unwrap();
                    let all: Vec<Atom> = atoms.iter().chain(&extra).cloned().collect();
                    let expected = brute_force(&all);
                    assert_eq!(
                        incremental, expected,
                        "seed {seed}: assertions {atoms:?} extra {extra:?}"
                    );
                    assert_eq!(
                        fresh(solver.assertions(), &extra_exprs),
                        expected,
                        "seed {seed}: fresh solver disagrees"
                    );
                    if incremental == SatResult::Sat {
                        let mut values = [0u64; VARS];
                        for (value, &id) in values.iter_mut().zip(&ids) {
                            *value = solver.get_value(id).expect("model value");
                        }
                        assert!(
                            all.iter().all(|atom| atom.holds(&values)),
                            "seed {seed}: model {values:?} violates {all:?}"
                        );
                    }
                    queries += 1;
                }
            }
            assert_eq!(solver.lowered_assertion_count(), atoms.len());
        }
    }
    assert!(queries > seeds as usize, "only {queries} queries ran");
}
