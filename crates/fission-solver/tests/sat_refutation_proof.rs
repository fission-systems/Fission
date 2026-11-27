// SPDX-License-Identifier: Apache-2.0
//! Issue #1232: Independently checkable DRAT/RUP UNSAT refutations for bounded
//! pure bitvector queries, negative controls, budgets, and exhaustive small-width checks.

use fission_solver::ast::{Sort, SymExpr};
use fission_solver::cnf::{sha256_hex, to_dimacs_cnf, Clause, Lit};
use fission_solver::sat::{
    solve_pure_bv_certified, verify_drat_rup_proof, CertifiedQueryConfig, CertifiedVerdict,
    ProofCheckError, ProofCheckerLimits, SatSolver, CHECKER_VERSION, ENCODING_VERSION,
    SOLVER_VERSION, TCB_SCOPE_STATEMENT,
};
use fission_solver::theory::{Theory, TheoryStatus};

fn bv_or(a: SymExpr, b: SymExpr) -> SymExpr {
    SymExpr::Or(Box::new(a), Box::new(b))
}

fn build_pigeonhole_cnf(pigeons: usize, holes: usize) -> (u32, Vec<Clause>) {
    let var = |p: usize, h: usize| -> u32 { (p * holes + h + 1) as u32 };
    let num_vars = (pigeons * holes) as u32;
    let mut clauses = Vec::new();
    for p in 0..pigeons {
        let mut at_least_one = Vec::with_capacity(holes);
        for h in 0..holes {
            at_least_one.push(Lit::new(var(p, h), false));
        }
        clauses.push(Clause(at_least_one));
    }
    for h in 0..holes {
        for p1 in 0..pigeons {
            for p2 in (p1 + 1)..pigeons {
                clauses.push(Clause(vec![
                    Lit::new(var(p1, h), true),
                    Lit::new(var(p2, h), true),
                ]));
            }
        }
    }
    (num_vars, clauses)
}

#[test]
fn pure_bv_unsat_emits_independently_verified_refutation_certificate() {
    let x = SymExpr::new_var("x", 16);
    let y = SymExpr::new_var("y", 16);
    // Unsatisfiable 16-bit BV equivalence negation: ((x + y) - y) != x.
    let cancel = SymExpr::new_sub(SymExpr::new_add(x.clone(), y.clone()), y);
    let query = SymExpr::new_neq(cancel, x);

    let verdict = solve_pure_bv_certified(&[query], &[], &CertifiedQueryConfig::default());
    let CertifiedVerdict::UnsatCertified(cert) = verdict else {
        panic!("expected UnsatCertified, got {verdict:?}");
    };

    assert_eq!(cert.encoding_version, ENCODING_VERSION);
    assert_eq!(cert.solver_version, SOLVER_VERSION);
    assert_eq!(cert.checker_version, CHECKER_VERSION);
    assert_eq!(cert.tcb_scope, TCB_SCOPE_STATEMENT);
    assert_eq!(cert.cnf_sha256, sha256_hex(cert.dimacs_cnf.as_bytes()));
    assert_eq!(cert.drat_sha256, sha256_hex(cert.drat_proof.as_bytes()));
    assert_eq!(cert.check_report.cnf_sha256, cert.cnf_sha256);
    assert_eq!(cert.check_report.drat_sha256, cert.drat_sha256);

    // Replay independently from exported strings alone.
    let t_check = std::time::Instant::now();
    let replayed = verify_drat_rup_proof(
        &cert.dimacs_cnf,
        &cert.assumption_lits,
        &cert.drat_proof,
        ProofCheckerLimits::default(),
    )
    .expect("independent replay must verify");
    let check_elapsed = t_check.elapsed();
    assert_eq!(replayed, cert.check_report);
    assert!(replayed.verified_add_steps >= 1);
    eprintln!(
        "[BV-16bit] cnf_bytes={} drat_bytes={} vars={} input_clauses={} add_steps={} delete_steps={} propagations={} check_us={}",
        cert.dimacs_cnf.len(),
        cert.drat_proof.len(),
        replayed.num_vars,
        replayed.input_clauses,
        replayed.verified_add_steps,
        replayed.delete_steps,
        replayed.propagations,
        check_elapsed.as_micros(),
    );
}

#[test]
fn assumption_scoped_pure_bv_unsat_certifies_and_rejects_modified_assumptions() {
    let a = SymExpr::new_var("a", 8);
    let b = SymExpr::new_var("b", 8);
    let eq = SymExpr::new_eq(
        SymExpr::new_add(a.clone(), b.clone()),
        SymExpr::new_const(10, 8),
    );
    let asm1 = SymExpr::new_eq(a, SymExpr::new_const(4, 8));
    let asm2 = SymExpr::new_neq(b, SymExpr::new_const(6, 8));

    let verdict = solve_pure_bv_certified(&[eq], &[asm1, asm2], &CertifiedQueryConfig::default());
    let CertifiedVerdict::UnsatCertified(cert) = verdict else {
        panic!("expected UnsatCertified with assumptions, got {verdict:?}");
    };
    assert_eq!(cert.assumption_lits.len(), 2);

    // Dropping one assumption breaks the refutation proof (empty clause or step fails RUP).
    let dropped_err = verify_drat_rup_proof(
        &cert.dimacs_cnf,
        &cert.assumption_lits[..1],
        &cert.drat_proof,
        ProofCheckerLimits::default(),
    )
    .expect_err("dropping an assumption must invalidate the proof");
    assert!(matches!(
        dropped_err,
        ProofCheckError::RupCheckFailed { .. }
    ));

    // Out-of-range assumption variable is explicitly rejected.
    let bad_asm_err = verify_drat_rup_proof(
        &cert.dimacs_cnf,
        &[Lit::new(cert.num_vars + 1, false)],
        &cert.drat_proof,
        ProofCheckerLimits::default(),
    )
    .expect_err("out-of-range assumption var must fail");
    assert!(matches!(
        bad_asm_err,
        ProofCheckError::UnknownAssumptionVariable { .. }
    ));
}

#[test]
fn satisfiable_query_never_emits_refutation_certificate() {
    let x = SymExpr::new_var("x", 8);
    let sat_query = SymExpr::new_neq(SymExpr::new_add(x.clone(), SymExpr::new_const(1, 8)), x);
    let verdict = solve_pure_bv_certified(&[sat_query], &[], &CertifiedQueryConfig::default());
    assert_eq!(verdict, CertifiedVerdict::Sat);
}

#[test]
fn learned_clause_deletion_in_pigeonhole_refutation_verifies_independently() {
    // PHP(6,5) triggers >100 conflicts so gc_learned emits `d ... 0` DRAT deletion steps.
    let (num_vars, clauses) = build_pigeonhole_cnf(6, 5);
    let dimacs = to_dimacs_cnf(&clauses, num_vars);

    let mut sat = SatSolver::new();
    sat.enable_proof_logging(None);
    for c in &clauses {
        assert!(sat.add_clause(c.0.clone()));
    }
    assert!(!sat.solve(), "PHP(6,5) must be UNSAT");

    let proof = sat.proof().expect("proof recorded");
    assert!(!proof.truncated);
    let drat_text = proof.to_drat_text();
    let t_check = std::time::Instant::now();
    let report = verify_drat_rup_proof(&dimacs, &[], &drat_text, ProofCheckerLimits::default())
        .expect("PHP(6,5) proof with GC deletions must verify");
    let check_elapsed = t_check.elapsed();

    assert!(
        report.delete_steps > 0,
        "expected learned clause deletions during PHP(6,5) refutation, got {report:?}"
    );
    assert!(report.verified_add_steps >= 100);
    eprintln!(
        "[PHP(6,5)] cnf_bytes={} drat_bytes={} vars={} input_clauses={} add_steps={} delete_steps={} propagations={} check_ms={}",
        dimacs.len(),
        drat_text.len(),
        report.num_vars,
        report.input_clauses,
        report.verified_add_steps,
        report.delete_steps,
        report.propagations,
        check_elapsed.as_millis(),
    );
}

#[test]
fn corrupted_or_altered_proofs_and_cnfs_fail_closed() {
    // Use PHP(4,3): UNSAT after search, not by root unit propagation alone,
    // so prepending an un-implied unit clause `1 0` is genuinely non-RUP.
    let (num_vars, clauses) = build_pigeonhole_cnf(4, 3);
    let dimacs_cnf = to_dimacs_cnf(&clauses, num_vars);
    let mut sat = SatSolver::new();
    sat.enable_proof_logging(None);
    for c in &clauses {
        assert!(sat.add_clause(c.0.clone()));
    }
    assert!(!sat.solve());
    let drat_proof = sat.proof().expect("proof").to_drat_text();

    // 1. Missing terminating empty clause `0`.
    let no_empty_drat = drat_proof.strip_suffix("0\n").unwrap_or(&drat_proof);
    let missing_empty = verify_drat_rup_proof(
        &dimacs_cnf,
        &[],
        no_empty_drat,
        ProofCheckerLimits::default(),
    )
    .expect_err("proof without empty clause must fail");
    assert!(matches!(
        missing_empty,
        ProofCheckError::MissingEmptyClause | ProofCheckError::RupCheckFailed { .. }
    ));

    // 2. Corrupted DRAT addition step (bogus unary clause asserting out-of-range var).
    let bogus_var_drat = format!("{} 0\n0\n", num_vars + 5);
    let var_err = verify_drat_rup_proof(
        &dimacs_cnf,
        &[],
        &bogus_var_drat,
        ProofCheckerLimits::default(),
    )
    .expect_err("out-of-range literal in DRAT must fail");
    assert!(matches!(var_err, ProofCheckError::UnknownVariable { .. }));

    // 3. Valid variable, non-implied unit clause `1 0`.
    let non_rup_drat = format!("1 0\n{drat_proof}");
    let rup_err = verify_drat_rup_proof(
        &dimacs_cnf,
        &[],
        &non_rup_drat,
        ProofCheckerLimits::default(),
    )
    .expect_err("non-RUP unit clause must be rejected");
    assert!(matches!(
        rup_err,
        ProofCheckError::RupCheckFailed { step: 0, .. }
    ));

    // 4. Altered CNF (replace CNF with a SAT formula and replay the old DRAT proof).
    let sat_cnf = to_dimacs_cnf(
        &[Clause(vec![Lit::new(1, false), Lit::new(2, false)])],
        num_vars,
    );
    let altered_cnf_err =
        verify_drat_rup_proof(&sat_cnf, &[], &drat_proof, ProofCheckerLimits::default())
            .expect_err("altered SAT CNF must fail original DRAT proof");
    assert!(matches!(
        altered_cnf_err,
        ProofCheckError::RupCheckFailed { .. }
    ));

    // 5. Malformed DRAT syntax.
    let syntax_err =
        verify_drat_rup_proof(&dimacs_cnf, &[], "1 2\n0\n", ProofCheckerLimits::default())
            .expect_err("unterminated DRAT line must fail");
    assert!(matches!(syntax_err, ProofCheckError::InvalidDrat(_)));
}

#[test]
fn unsupported_array_and_float_and_theory_queries_do_not_forge_certificates() {
    // Array sort/select rejected.
    let arr = SymExpr::new_array_var("mem", 8, 8);
    let idx = SymExpr::new_var("idx", 8);
    let sel = SymExpr::ArraySelect {
        array: Box::new(arr),
        index: Box::new(idx),
    };
    let arr_q = SymExpr::new_eq(sel, SymExpr::new_const(0, 8));
    assert!(matches!(
        solve_pure_bv_certified(&[arr_q], &[], &CertifiedQueryConfig::default()),
        CertifiedVerdict::Unsupported { .. }
    ));

    // Float sort rejected.
    let f = SymExpr::new_float_var("f", 32);
    let f_q = SymExpr::new_feq(f.clone(), f);
    assert!(matches!(
        solve_pure_bv_certified(&[f_q], &[], &CertifiedQueryConfig::default()),
        CertifiedVerdict::Unsupported { .. }
    ));

    // Ill-sorted expression rejected.
    let conflicting_a = SymExpr::Var {
        id: 9999,
        name: "bad".to_string(),
        sort: Sort::BitVector(8),
    };
    let conflicting_b = SymExpr::Var {
        id: 9999,
        name: "bad".to_string(),
        sort: Sort::BitVector(16),
    };
    let ill = SymExpr::new_eq(conflicting_a, conflicting_b);
    assert!(matches!(
        solve_pure_bv_certified(&[ill], &[], &CertifiedQueryConfig::default()),
        CertifiedVerdict::Unsupported { .. }
    ));

    // SatSolver with a Theory taints pure-CNF proof logging.
    struct DummyTheory;
    impl Theory for DummyTheory {
        fn check(&mut self, _assignments: &[Lit]) -> TheoryStatus {
            TheoryStatus::Satisfied
        }
    }
    let mut sat = SatSolver::new();
    sat.enable_proof_logging(None);
    sat.add_clause(vec![Lit::new(1, false)]);
    let mut th = DummyTheory;
    assert!(sat.solve_with_theory(Some(&mut th)));
    assert!(sat.proof_tainted_reason().is_some());
}

#[test]
fn budget_exhaustion_fails_closed_when_certificate_required_and_reports_unchecked_otherwise() {
    let x = SymExpr::new_var("x", 16);
    let y = SymExpr::new_var("y", 16);
    // (x + y) - y != x needs multiple CDCL conflicts/DRAT steps.
    let query = SymExpr::new_neq(
        SymExpr::new_sub(SymExpr::new_add(x.clone(), y.clone()), y),
        x,
    );

    // 1. SAT conflict budget exhaustion.
    let sat_exhausted = solve_pure_bv_certified(
        std::slice::from_ref(&query),
        &[],
        &CertifiedQueryConfig {
            require_certificate: true,
            conflict_budget: Some(1),
            proof_step_budget: None,
            checker_limits: ProofCheckerLimits::default(),
        },
    );
    assert_eq!(
        sat_exhausted,
        CertifiedVerdict::BudgetExhausted {
            stage: "sat_search"
        }
    );

    // 2. Proof step budget exhaustion with require_certificate=true fails closed.
    let log_exhausted = solve_pure_bv_certified(
        std::slice::from_ref(&query),
        &[],
        &CertifiedQueryConfig {
            require_certificate: true,
            conflict_budget: None,
            proof_step_budget: Some(1),
            checker_limits: ProofCheckerLimits::default(),
        },
    );
    assert_eq!(
        log_exhausted,
        CertifiedVerdict::BudgetExhausted {
            stage: "proof_logging"
        }
    );

    // 3. Proof step budget exhaustion with require_certificate=false reports UnsatUnchecked, never UnsatCertified.
    let unchecked = solve_pure_bv_certified(
        &[query],
        &[],
        &CertifiedQueryConfig {
            require_certificate: false,
            conflict_budget: None,
            proof_step_budget: Some(1),
            checker_limits: ProofCheckerLimits::default(),
        },
    );
    assert!(matches!(unchecked, CertifiedVerdict::UnsatUnchecked { .. }));

    // 4. Independent checker propagation budget exceeded.
    let (num_vars, clauses) = build_pigeonhole_cnf(6, 5);
    let dimacs = to_dimacs_cnf(&clauses, num_vars);
    let mut sat = SatSolver::new();
    sat.enable_proof_logging(None);
    for c in &clauses {
        sat.add_clause(c.0.clone());
    }
    assert!(!sat.solve());
    let drat = sat.proof().unwrap().to_drat_text();
    let prop_limit_err = verify_drat_rup_proof(
        &dimacs,
        &[],
        &drat,
        ProofCheckerLimits {
            max_steps: 200_000,
            max_propagations: 5,
        },
    )
    .expect_err("tiny propagation limit must abort verification");
    assert!(matches!(
        prop_limit_err,
        ProofCheckError::ResourceLimitExceeded {
            kind: "propagations",
            ..
        }
    ));
}

#[test]
fn exhaustive_3bit_identities_and_counterexamples_agree_with_certified_verdicts() {
    let a = SymExpr::new_var("a", 3);
    let b = SymExpr::new_var("b", 3);

    // Property 1 (true identity): (a ^ b) == ((a | b) & ~(a & b))
    let xor_ab = SymExpr::new_xor(a.clone(), b.clone());
    let alt_xor = SymExpr::new_and(
        bv_or(a.clone(), b.clone()),
        SymExpr::new_not(SymExpr::new_and(a.clone(), b.clone())),
    );
    let prop1_exhaustive_holds =
        (0u8..8).all(|av| (0u8..8).all(|bv| (av ^ bv) & 7 == ((av | bv) & !(av & bv)) & 7));
    assert!(prop1_exhaustive_holds);
    assert!(matches!(
        solve_pure_bv_certified(
            &[SymExpr::new_neq(xor_ab, alt_xor)],
            &[],
            &CertifiedQueryConfig::default()
        ),
        CertifiedVerdict::UnsatCertified(_)
    ));

    // Property 2 (true identity): a + b == b + a
    let prop2_exhaustive_holds =
        (0u8..8).all(|av| (0u8..8).all(|bv| av.wrapping_add(bv) & 7 == bv.wrapping_add(av) & 7));
    assert!(prop2_exhaustive_holds);
    assert!(matches!(
        solve_pure_bv_certified(
            &[SymExpr::new_neq(
                SymExpr::new_add(a.clone(), b.clone()),
                SymExpr::new_add(b.clone(), a.clone()),
            )],
            &[],
            &CertifiedQueryConfig::default(),
        ),
        CertifiedVerdict::UnsatCertified(_)
    ));

    // Property 3 (false identity): (a + b) != (a ^ b) has counterexamples (e.g. a=1, b=1)
    let has_counterexample =
        (0u8..8).any(|av| (0u8..8).any(|bv| (av.wrapping_add(bv) & 7) != ((av ^ bv) & 7)));
    assert!(has_counterexample);
    assert_eq!(
        solve_pure_bv_certified(
            &[SymExpr::new_neq(
                SymExpr::new_add(a.clone(), b.clone()),
                SymExpr::new_xor(a, b),
            )],
            &[],
            &CertifiedQueryConfig::default(),
        ),
        CertifiedVerdict::Sat
    );
}
