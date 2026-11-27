use fission_solver::aig::{AigManager, LoweringBudget, LoweringBudgetKind};
use fission_solver::cnf::CnfBuilder;
use fission_solver::sat::SatSolver;
use fission_solver::theory::array::ArrayTheory;
use fission_solver::theory::bitvector::BvTheorySolver;
use fission_solver::{
    validate_expr_with_budget, EvalTermination, SatResult, Solver, Sort, SymExpr, UnknownReason,
    ValidatedDag, ValidationBudget, ValidationError, VarSortEnv, MAX_EXPR_DEPTH,
};

fn bit_mask(width: u32) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

fn eval_aig_eq_sat(aig: &mut AigManager, expr: &SymExpr) -> bool {
    let bits = aig.lower_expr(expr);
    assert_eq!(bits.len(), 1, "expected 1-bit predicate");
    let mut cnf = CnfBuilder::new();
    aig.to_cnf(&mut cnf);
    cnf.assert_lit(bits[0]);
    let mut sat = SatSolver::new();
    for clause in &cnf.clauses {
        if !sat.add_clause(clause.0.clone()) {
            return false;
        }
    }
    sat.solve()
}

#[test]
fn shared_subexpression_dag_reuses_lowering_and_bounds_width_inference() {
    let x = SymExpr::new_var("x", 16);
    let mut cur = x.clone();
    for _ in 0..14 {
        let bumped = SymExpr::Add(Box::new(cur.clone()), Box::new(SymExpr::new_const(1, 16)));
        cur = SymExpr::Xor(Box::new(cur), Box::new(bumped));
    }

    let mut aig = AigManager::new();
    let bits = aig.lower_expr(&cur);
    assert_eq!(bits.len(), 16);
    assert!(aig.unsupported().is_empty());

    let stats = aig.lowering_stats();
    println!(
        "diamond_d14 stats: occurrences={}, unique={}, dag_hits={}, lowered_hits={}, width_visits={}, aig_nodes={}, strash_hits={}, time_ns={}",
        stats.tree_occurrences,
        stats.unique_subexpressions,
        stats.dag_cache_hits,
        stats.lowered_cache_hits,
        stats.width_inference_visits,
        stats.aig_nodes_created,
        stats.strash_hits,
        stats.lowering_time_ns,
    );

    assert!(
        stats.tree_occurrences > 10_000,
        "expected exponential tree occurrences, got {}",
        stats.tree_occurrences
    );
    assert!(
        stats.unique_subexpressions <= 40,
        "expected O(depth) unique DAG nodes, got {}",
        stats.unique_subexpressions
    );
    assert_eq!(
        stats.width_inference_visits, stats.unique_subexpressions,
        "width inference must be memoized per unique DAG node"
    );
    assert!(
        stats.dag_cache_hits > 10_000,
        "expected repeated subexpressions to hit DAG cache, got {}",
        stats.dag_cache_hits
    );

    let mut solver = Solver::new();
    let x_id = solver.register_var("sx".to_string(), 8);
    let sx = solver.node(x_id).unwrap().clone();
    let mut chain = sx.clone();
    for _ in 0..8 {
        let bumped = SymExpr::Add(Box::new(chain.clone()), Box::new(SymExpr::new_const(1, 8)));
        chain = SymExpr::Xor(Box::new(chain), Box::new(bumped));
    }
    let expected = {
        let mut v: u8 = 0x25;
        for _ in 0..8 {
            v ^= v.wrapping_add(1);
        }
        u64::from(v)
    };
    solver.assert(SymExpr::Eq(
        Box::new(sx.clone()),
        Box::new(SymExpr::new_const(0x25, 8)),
    ));
    solver.assert(SymExpr::Eq(
        Box::new(chain.clone()),
        Box::new(SymExpr::new_const(expected, 8)),
    ));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.get_value(x_id), Some(0x25));

    let before_hits = solver.bv_theory().aig.lowering_stats().lowered_cache_hits;
    assert_eq!(
        solver
            .check_sat_with_oracle(
                None,
                &[SymExpr::Neq(
                    Box::new(chain.clone()),
                    Box::new(SymExpr::new_const(expected, 8)),
                )],
            )
            .unwrap(),
        SatResult::Unsat
    );
    let after_hits = solver.bv_theory().aig.lowering_stats().lowered_cache_hits;
    assert!(
        after_hits > before_hits,
        "expected lowered cache reuse across queries on same Solver (before={before_hits}, after={after_hits})"
    );

    let fresh_solver = Solver::new();
    assert_eq!(fresh_solver.bv_theory().aig.dag_node_count(), 0);
    assert_eq!(
        fresh_solver.bv_theory().aig.lowering_stats().dag_cache_hits,
        0
    );
}

#[test]
fn dag_cache_hit_replays_unsupported_status_across_queries() {
    let mut solver = Solver::new();
    let x_id = solver.register_var("x".to_string(), 8);
    let x = solver.node(x_id).unwrap().clone();
    solver.assert(SymExpr::Eq(Box::new(x), Box::new(SymExpr::new_const(7, 8))));
    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);

    let fa = SymExpr::new_float_var("fa", 32);
    let fb = SymExpr::new_float_var("fb", 32);
    let fadd = SymExpr::FAdd(Box::new(fa), Box::new(fb));
    let unsupported_pred =
        SymExpr::Neq(Box::new(fadd.clone()), Box::new(SymExpr::new_const(0, 32)));

    assert_eq!(
        solver
            .check_sat_with_oracle(None, std::slice::from_ref(&unsupported_pred))
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::Unsupported(_))
    ));

    let hits_before = solver.bv_theory().aig.lowering_stats().lowered_cache_hits;
    assert_eq!(
        solver
            .check_sat_with_oracle(None, std::slice::from_ref(&unsupported_pred))
            .unwrap(),
        SatResult::Unknown
    );
    assert!(matches!(
        solver.last_unknown(),
        Some(UnknownReason::Unsupported(_))
    ));
    assert!(
        solver.bv_theory().aig.lowering_stats().lowered_cache_hits > hits_before,
        "second query should hit lowered DAG cache while preserving Unsupported status"
    );

    let report = solver.eval_detailed(&fadd, 2);
    assert!(matches!(
        report.termination,
        EvalTermination::Unknown(UnknownReason::Unsupported(_))
    ));

    assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
    assert_eq!(solver.get_value(x_id), Some(7));
}

#[test]
fn explicit_lowering_and_validation_budgets_prevent_unbounded_work_and_false_verdicts() {
    let x = SymExpr::new_var("x", 32);
    let y = SymExpr::new_var("y", 32);

    // Regression: symbolic Mul(x, y) must terminate immediately without affine self-loop
    // and compute exact SAT/UNSAT semantics.
    let mul = SymExpr::Mul(Box::new(x.clone()), Box::new(y.clone()));
    let sat_formula = SymExpr::Eq(Box::new(mul.clone()), Box::new(SymExpr::new_const(6, 32)));
    let unsat_formula = SymExpr::Neq(Box::new(mul.clone()), Box::new(mul.clone()));

    let mut mul_aig = AigManager::new();
    assert!(eval_aig_eq_sat(&mut mul_aig, &sat_formula));
    assert!(mul_aig.unsupported().is_empty());

    let mut mul_unsat_aig = AigManager::new();
    assert!(!eval_aig_eq_sat(&mut mul_unsat_aig, &unsat_formula));
    assert!(mul_unsat_aig.unsupported().is_empty());

    // 1. AIG gate budget on SAT and UNSAT formulas -> structured budget hit, never false verdict.
    for (label, formula) in [("sat", &sat_formula), ("unsat", &unsat_formula)] {
        let mut bv = BvTheorySolver::new();
        bv.aig.set_lowering_budget(LoweringBudget {
            max_depth: MAX_EXPR_DEPTH,
            max_expr_visits: 100_000,
            max_unique_nodes: 100_000,
            max_aig_nodes: 120,
        });
        bv.assert_expr(formula);
        assert!(
            bv.aig.node_count() <= 120,
            "{label}: AIG node count exceeded hard budget: {}",
            bv.aig.node_count()
        );
        assert_eq!(
            bv.aig.lowering_stats().budget_exceeded,
            Some(LoweringBudgetKind::AigNodes { limit: 120 }),
            "{label}: expected structured AigNodes budget status"
        );
        assert!(
            bv.unsupported()
                .iter()
                .any(|m| m.contains("lowering budget exceeded")),
            "{label}: expected lowering budget note in unsupported(): {:?}",
            bv.unsupported()
        );
    }

    // 2. Partial lowering must NOT be cached: after hitting budget on a small limit,
    // solving without a tight limit still succeeds.
    let mut aig = AigManager::new();
    aig.set_lowering_budget(LoweringBudget {
        max_depth: 64,
        max_expr_visits: 10_000,
        max_unique_nodes: 10_000,
        max_aig_nodes: 80,
    });
    let _ = aig.lower_expr(&sat_formula);
    assert_eq!(
        aig.lowering_stats().budget_exceeded,
        Some(LoweringBudgetKind::AigNodes { limit: 80 })
    );
    aig.set_lowering_budget(LoweringBudget::default());
    assert!(eval_aig_eq_sat(&mut aig, &sat_formula));

    // 3. Visit budget and unique-node budget on AigManager & ValidatedDag.
    let mut chain = x.clone();
    for _ in 0..8 {
        chain = SymExpr::Xor(
            Box::new(chain.clone()),
            Box::new(SymExpr::Add(
                Box::new(chain),
                Box::new(SymExpr::new_const(1, 32)),
            )),
        );
    }
    let mut aig_visits = AigManager::new();
    aig_visits.set_lowering_budget(LoweringBudget {
        max_depth: MAX_EXPR_DEPTH,
        max_expr_visits: 50,
        max_unique_nodes: 10_000,
        max_aig_nodes: 100_000,
    });
    let _ = aig_visits.lower_expr(&chain);
    assert_eq!(
        aig_visits.lowering_stats().budget_exceeded,
        Some(LoweringBudgetKind::ExprVisits { limit: 50 })
    );

    let mut aig_unique = AigManager::new();
    aig_unique.set_lowering_budget(LoweringBudget {
        max_depth: MAX_EXPR_DEPTH,
        max_expr_visits: 100_000,
        max_unique_nodes: 5,
        max_aig_nodes: 100_000,
    });
    let _ = aig_unique.lower_expr(&chain);
    assert_eq!(
        aig_unique.lowering_stats().budget_exceeded,
        Some(LoweringBudgetKind::UniqueNodes { limit: 5 })
    );

    let mut env = VarSortEnv::new();
    let err_visits = validate_expr_with_budget(
        &chain,
        &mut env,
        ValidationBudget {
            max_depth: MAX_EXPR_DEPTH,
            max_expr_visits: 50,
            max_unique_nodes: 10_000,
        },
    );
    assert_eq!(
        err_visits,
        Err(ValidationError::VisitBudgetExceeded { limit: 50 })
    );

    let mut dag = ValidatedDag::new();
    let err_unique = dag.intern_validated(
        &chain,
        &mut VarSortEnv::new(),
        ValidationBudget {
            max_depth: MAX_EXPR_DEPTH,
            max_expr_visits: 100_000,
            max_unique_nodes: 4,
        },
    );
    assert_eq!(
        err_unique,
        Err(ValidationError::UniqueNodeBudgetExceeded { limit: 4 })
    );
    assert!(
        dag.is_empty(),
        "failed DAG interning must roll back cleanly"
    );

    // 4. Deep chain near MAX_EXPR_DEPTH validates and lowers iteratively on a 512 KiB stack.
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            let v = SymExpr::new_var("deep_v", 8);
            let depth = MAX_EXPR_DEPTH - 4;
            let mut deep = v.clone();
            for _ in 0..depth {
                deep = SymExpr::Add(Box::new(deep), Box::new(SymExpr::new_const(1, 8)));
            }
            let eq = SymExpr::Eq(
                Box::new(deep),
                Box::new(SymExpr::new_const((depth as u64 + 7) & 0xff, 8)),
            );
            let mut solver = Solver::new();
            solver.assert(eq);
            assert_eq!(solver.check_sat().unwrap(), SatResult::Sat);
            assert!(solver.is_true(&SymExpr::Eq(
                Box::new(v),
                Box::new(SymExpr::new_const(7, 8)),
            )));
        })
        .expect("spawn small-stack thread")
        .join()
        .expect("iterative validation/lowering must not stack overflow");
}

#[test]
fn modular_affine_normalization_exhaustive_small_widths_and_boundaries() {
    for width in 1..=4u32 {
        let mask = bit_mask(width);
        let limit = 1u64 << width;
        let x = SymExpr::new_var("ax", width);
        let y = SymExpr::new_var("ay", width);
        let c1 = 3 & mask;
        let c2 = 5 & mask;

        let exprs: Vec<(&str, SymExpr, Box<dyn Fn(u64, u64) -> u64>)> = vec![
            (
                "x + x",
                SymExpr::Add(Box::new(x.clone()), Box::new(x.clone())),
                Box::new(move |xv, _| xv.wrapping_add(xv) & mask),
            ),
            (
                "(x + c1) + (y + c2)",
                SymExpr::Add(
                    Box::new(SymExpr::Add(
                        Box::new(x.clone()),
                        Box::new(SymExpr::new_const(c1, width)),
                    )),
                    Box::new(SymExpr::Add(
                        Box::new(y.clone()),
                        Box::new(SymExpr::new_const(c2, width)),
                    )),
                ),
                Box::new(move |xv, yv| {
                    xv.wrapping_add(c1).wrapping_add(yv).wrapping_add(c2) & mask
                }),
            ),
            (
                "(x + y) - x",
                SymExpr::Sub(
                    Box::new(SymExpr::Add(Box::new(x.clone()), Box::new(y.clone()))),
                    Box::new(x.clone()),
                ),
                Box::new(move |_, yv| yv & mask),
            ),
            (
                "3*x + 5*x",
                SymExpr::Add(
                    Box::new(SymExpr::Mul(
                        Box::new(SymExpr::new_const(3 & mask, width)),
                        Box::new(x.clone()),
                    )),
                    Box::new(SymExpr::Mul(
                        Box::new(SymExpr::new_const(5 & mask, width)),
                        Box::new(x.clone()),
                    )),
                ),
                Box::new(move |xv, _| {
                    let a = (3 & mask).wrapping_mul(xv) & mask;
                    let b = (5 & mask).wrapping_mul(xv) & mask;
                    a.wrapping_add(b) & mask
                }),
            ),
            (
                "(x - y) + (y - x)",
                SymExpr::Add(
                    Box::new(SymExpr::Sub(Box::new(x.clone()), Box::new(y.clone()))),
                    Box::new(SymExpr::Sub(Box::new(y.clone()), Box::new(x.clone()))),
                ),
                Box::new(move |_, _| 0),
            ),
            (
                "((x + c1) * 2) - (x + x)",
                SymExpr::Sub(
                    Box::new(SymExpr::Mul(
                        Box::new(SymExpr::Add(
                            Box::new(x.clone()),
                            Box::new(SymExpr::new_const(c1, width)),
                        )),
                        Box::new(SymExpr::new_const(2 & mask, width)),
                    )),
                    Box::new(SymExpr::Add(Box::new(x.clone()), Box::new(x.clone()))),
                ),
                Box::new(move |xv, _| {
                    let lhs = (xv.wrapping_add(c1) & mask).wrapping_mul(2 & mask) & mask;
                    let rhs = xv.wrapping_add(xv) & mask;
                    lhs.wrapping_sub(rhs) & mask
                }),
            ),
        ];

        for (label, expr, oracle) in &exprs {
            for xv in 0..limit {
                for yv in 0..limit {
                    let expected = oracle(xv, yv);
                    let assign = SymExpr::And(
                        Box::new(SymExpr::Eq(
                            Box::new(x.clone()),
                            Box::new(SymExpr::new_const(xv, width)),
                        )),
                        Box::new(SymExpr::Eq(
                            Box::new(y.clone()),
                            Box::new(SymExpr::new_const(yv, width)),
                        )),
                    );
                    let check_eq = SymExpr::And(
                        Box::new(assign.clone()),
                        Box::new(SymExpr::Eq(
                            Box::new(expr.clone()),
                            Box::new(SymExpr::new_const(expected, width)),
                        )),
                    );
                    let check_neq = SymExpr::And(
                        Box::new(assign),
                        Box::new(SymExpr::Neq(
                            Box::new(expr.clone()),
                            Box::new(SymExpr::new_const(expected, width)),
                        )),
                    );

                    let mut aig_on_eq = AigManager::new();
                    aig_on_eq.set_modular_affine_enabled(true);
                    let mut aig_off_eq = AigManager::new();
                    aig_off_eq.set_modular_affine_enabled(false);
                    let mut aig_on_neq = AigManager::new();
                    aig_on_neq.set_modular_affine_enabled(true);
                    let mut aig_off_neq = AigManager::new();
                    aig_off_neq.set_modular_affine_enabled(false);

                    assert!(
                        eval_aig_eq_sat(&mut aig_on_eq, &check_eq),
                        "w={width} {label} ({xv},{yv}) -> {expected}: affine ON failed SAT"
                    );
                    assert!(
                        eval_aig_eq_sat(&mut aig_off_eq, &check_eq),
                        "w={width} {label} ({xv},{yv}) -> {expected}: affine OFF failed SAT"
                    );
                    assert!(
                        !eval_aig_eq_sat(&mut aig_on_neq, &check_neq),
                        "w={width} {label} ({xv},{yv}) -> {expected}: affine ON must be UNSAT for neq"
                    );
                    assert!(
                        !eval_aig_eq_sat(&mut aig_off_neq, &check_neq),
                        "w={width} {label} ({xv},{yv}) -> {expected}: affine OFF must be UNSAT for neq"
                    );
                    assert!(aig_on_eq.lowering_stats().modular_affine_rewrites > 0);
                }
            }
        }
    }

    for width in [8u32, 16, 32, 64] {
        let mask = bit_mask(width);
        let sign = 1u64 << (width - 1);
        let x = SymExpr::new_var("bx", width);
        let y = SymExpr::new_var("by", width);
        let expr = SymExpr::Sub(
            Box::new(SymExpr::Add(
                Box::new(SymExpr::Add(
                    Box::new(SymExpr::Mul(
                        Box::new(SymExpr::new_const(3, width)),
                        Box::new(x.clone()),
                    )),
                    Box::new(SymExpr::Mul(
                        Box::new(SymExpr::new_const(5, width)),
                        Box::new(x.clone()),
                    )),
                )),
                Box::new(SymExpr::Add(
                    Box::new(y.clone()),
                    Box::new(SymExpr::new_const(17, width)),
                )),
            )),
            Box::new(y.clone()),
        );

        let mut aig_on = AigManager::new();
        aig_on.set_modular_affine_enabled(true);
        let _ = aig_on.lower_expr(&expr);
        let mut aig_off = AigManager::new();
        aig_off.set_modular_affine_enabled(false);
        let _ = aig_off.lower_expr(&expr);
        println!(
            "affine w={width}: nodes_on={} vs nodes_off={} (rewrites={})",
            aig_on.node_count(),
            aig_off.node_count(),
            aig_on.lowering_stats().modular_affine_rewrites
        );
        assert!(
            aig_on.node_count() < aig_off.node_count(),
            "w={width}: modular affine normalization should reduce AIG nodes (on={}, off={})",
            aig_on.node_count(),
            aig_off.node_count()
        );

        for xv in [0u64, 1, sign - 1, sign, mask - 1, mask] {
            let expected = xv.wrapping_mul(8).wrapping_add(17) & mask;
            let mut solver = Solver::new();
            solver.assert(SymExpr::Eq(
                Box::new(x.clone()),
                Box::new(SymExpr::new_const(xv, width)),
            ));
            solver.assert(SymExpr::Eq(
                Box::new(y.clone()),
                Box::new(SymExpr::new_const(sign ^ 3, width)),
            ));
            assert!(solver.is_true(&SymExpr::Eq(
                Box::new(expr.clone()),
                Box::new(SymExpr::new_const(expected, width)),
            )));
        }
    }

    // Negative control 1: Mixed-width zero-extension boundary must NEVER be flattened across widths.
    let x8 = SymExpr::new_var("mx8", 8);
    let y8 = SymExpr::new_var("my8", 8);
    let z16 = SymExpr::new_var("mz16", 16);
    let mixed = SymExpr::Add(
        Box::new(SymExpr::Add(Box::new(x8.clone()), Box::new(y8.clone()))),
        Box::new(z16.clone()),
    );
    let mut s_mixed = Solver::new();
    s_mixed.assert(SymExpr::Eq(
        Box::new(x8),
        Box::new(SymExpr::new_const(200, 8)),
    ));
    s_mixed.assert(SymExpr::Eq(
        Box::new(y8),
        Box::new(SymExpr::new_const(100, 8)),
    ));
    s_mixed.assert(SymExpr::Eq(
        Box::new(z16),
        Box::new(SymExpr::new_const(10, 16)),
    ));
    assert!(s_mixed.is_true(&SymExpr::Eq(
        Box::new(mixed.clone()),
        Box::new(SymExpr::new_const(54, 16)),
    )));
    assert!(s_mixed.is_false(&SymExpr::Eq(
        Box::new(mixed),
        Box::new(SymExpr::new_const(310, 16)),
    )));

    // Negative control 2: Variable sort aliasing must NOT be cancelled away by modular affine `v - v`.
    let v8 = SymExpr::Var {
        id: 777_001,
        name: "alias".to_string(),
        sort: Sort::BitVector(8),
    };
    let v16 = SymExpr::Var {
        id: 777_001,
        name: "alias".to_string(),
        sort: Sort::BitVector(16),
    };
    let mut aig_alias = AigManager::new();
    let _ = aig_alias.lower_expr(&v8);
    assert!(aig_alias.unsupported().is_empty());
    let _ = aig_alias.lower_expr(&SymExpr::Sub(Box::new(v16.clone()), Box::new(v16)));
    assert!(
        aig_alias
            .unsupported()
            .iter()
            .any(|m| m.contains("aliases conflicting sorts")),
        "affine cancellation must not erase variable sort conflicts: {:?}",
        aig_alias.unsupported()
    );
}

#[test]
fn array_theory_interning_deduplicates_shared_terms_and_bounds_lemmas() {
    let arr = SymExpr::new_array_var("mem", 8, 8);
    let i = SymExpr::new_var("i", 8);
    let j = SymExpr::new_var("j", 8);
    let v = SymExpr::new_var("v", 8);

    let stored = SymExpr::ArrayStore {
        array: Box::new(arr),
        index: Box::new(i.clone()),
        value: Box::new(v.clone()),
    };
    let sel_j = SymExpr::ArraySelect {
        array: Box::new(stored),
        index: Box::new(j.clone()),
    };

    let mut repeated = sel_j.clone();
    for _ in 0..6 {
        repeated = SymExpr::Add(Box::new(repeated.clone()), Box::new(repeated));
    }

    let mut theory = ArrayTheory::new();
    theory.add_expr(&repeated);
    let stats = theory.stats();
    assert!(
        stats.term_cache_hits > 50,
        "expected repeated ArraySelect/ArrayStore terms to hit intern cache, got {stats:?}"
    );
    assert!(!theory.budget_exceeded());
    assert!(!theory.take_lemmas().is_empty());

    let mut bounded = ArrayTheory::new();
    bounded.set_max_lemmas(2);
    bounded.add_expr(&repeated);
    assert!(bounded.budget_exceeded());
    assert!(bounded.take_lemmas().len() <= 2);

    let mut solver = Solver::new();
    solver.assert(SymExpr::Eq(Box::new(i), Box::new(j)));
    solver.assert(SymExpr::Eq(
        Box::new(v),
        Box::new(SymExpr::new_const(42, 8)),
    ));
    assert!(solver.is_true(&SymExpr::Eq(
        Box::new(sel_j),
        Box::new(SymExpr::new_const(42, 8)),
    )));
}
