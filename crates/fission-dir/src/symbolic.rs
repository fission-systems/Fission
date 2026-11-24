//! Solver-backed PreHIR/HIR symbolic equivalence (the symbolic tier).
//!
//! Orchestrates [`crate::lower_sym`] (AST -> `SymExpr`) and
//! `fission_solver::Solver` (the real SAT/SMT-style solver): lower PreHIR's and
//! HIR's return value to one closed-form `SymExpr` each over a *shared* set
//! of parameter `Var`s, assert they're unequal, and ask the solver to
//! decide. `Unsat` is a genuine proof of equivalence (for the modeled
//! subset -- see [`crate::lower_sym`]'s scope notes), not "no counterexample
//! found among a few samples" the way [`crate::diff`]'s boundary-value
//! sweep is. `Sat` gives a real counterexample -- the exact input the
//! solver found the two sides diverge on, immediately replayable through
//! [`crate::ground_truth`] to see what the real machine actually computes
//! there.

use std::collections::{HashMap, HashSet};

use fission_midend_core::ir::{HirFunction, NirType};
use fission_midend_prehir::PreHirFunction;
use fission_solver::Solver;
use fission_solver::ast::{Sort, SymExpr, SymNodeId};
use fission_solver::solver::SatResult;

use crate::eval::{interpret_hir, interpret_prehir, is_signed, width_of};
use crate::lower_sym;
use crate::report::UnsupportedReason;

/// A validated solver-found counterexample: PreHIR and HIR provably diverge on
/// `args` (each carrying its exact declared bit width in `arg_widths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterexample {
    pub args: Vec<u64>,
    pub arg_widths: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolicOutcome {
    /// `Unsat` -- proved equivalent for the modeled subset.
    Equivalent,
    /// `Sat` -- a genuine, solver-found counterexample confirmed by symbolic
    /// evaluation and independent concrete PreHIR/HIR replay.
    Diverged(Counterexample),
    /// Not checkable at this tier (contains a loop/unsupported construct),
    /// or the solver returned `Unknown`.
    Unsupported(UnsupportedReason),
    /// Solver returned `Sat`, but witness extraction or independent replay
    /// could not confirm a valid counterexample (missing required model value,
    /// out-of-width model entry, or non-replaying witness).
    Inconclusive { reason: String },
}

/// The only solver verdict that proves equivalence is `Unsat`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SolverVerdict {
    Equivalent,
    Counterexample,
    Unknown,
}

pub(crate) fn classify_solver_result(result: SatResult) -> SolverVerdict {
    match result {
        SatResult::Unsat => SolverVerdict::Equivalent,
        SatResult::Sat => SolverVerdict::Counterexample,
        SatResult::Unknown => SolverVerdict::Unknown,
    }
}

fn validate_int_width(ty: &NirType) -> Result<(), &'static str> {
    if let NirType::Int { bits, .. } = ty
        && (*bits == 0 || *bits > 64)
    {
        return Err("symbolic tier supports 1..=64-bit integers only");
    }
    Ok(())
}

struct PreparedSymbolicQuery {
    prehir_cmp: SymExpr,
    hir_cmp: SymExpr,
    prehir_bits: u32,
    hir_bits: u32,
    cmp_bits: u32,
    param_specs: Vec<ParamSpec>,
}

struct ParamSpec {
    name: String,
    var: SymExpr,
    var_id: SymNodeId,
    width_bits: u32,
}

fn prepare_symbolic_query(
    prehir: &PreHirFunction,
    hir: &HirFunction,
) -> std::result::Result<PreparedSymbolicQuery, SymbolicOutcome> {
    if lower_sym::prehir::contains_loop(&prehir.body) || lower_sym::hir::contains_loop(&hir.body) {
        return Err(SymbolicOutcome::Unsupported(
            UnsupportedReason::ContainsLoop,
        ));
    }
    if prehir.params.len() != hir.params.len() {
        return Err(SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
            "PreHIR and HIR have a different parameter count -- can't share one set of symbolic \
             argument variables between them",
        )));
    }
    if let Err(msg) = validate_int_width(&prehir.return_type)
        .and_then(|()| validate_int_width(&hir.return_type))
        .and_then(|()| {
            prehir
                .params
                .iter()
                .try_for_each(|p| validate_int_width(&p.ty))
        })
        .and_then(|()| {
            hir.params
                .iter()
                .try_for_each(|p| validate_int_width(&p.ty))
        })
    {
        return Err(SymbolicOutcome::Unsupported(
            UnsupportedReason::UnsupportedType(msg),
        ));
    }

    // One shared `SymExpr::Var` per parameter index -- fed into *both*
    // PreHIR-side and HIR-side lowering, sized to the wider parameter width so
    // narrowing on one side cannot silently restrict inputs on the other.
    let mut param_specs = Vec::with_capacity(hir.params.len());
    let mut param_vars = Vec::with_capacity(hir.params.len());
    for (pre_p, hir_p) in prehir.params.iter().zip(&hir.params) {
        let width_bits = width_of(&pre_p.ty).max(width_of(&hir_p.ty)).clamp(1, 64);
        let var = SymExpr::new_var(&hir_p.name, width_bits);
        let SymExpr::Var { id: var_id, .. } = var else {
            unreachable!("SymExpr::new_var always returns Var");
        };
        param_vars.push(var.clone());
        param_specs.push(ParamSpec {
            name: hir_p.name.clone(),
            var,
            var_id,
            width_bits,
        });
    }

    let prehir_result =
        match lower_sym::prehir::lower(&prehir.body, &prehir.params, &prehir.locals, &param_vars) {
            Ok(e) => e,
            Err(err) => {
                tracing::debug!("lower_sym: PreHIR side unsupported: {err}");
                return Err(SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
                    "PreHIR side uses a construct the symbolic tier doesn't model (see debug logs)",
                )));
            }
        };
    let hir_result = match lower_sym::hir::lower(&hir.body, &hir.params, &hir.locals, &param_vars) {
        Ok(e) => e,
        Err(err) => {
            tracing::debug!("lower_sym: HIR side unsupported: {err}");
            return Err(SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
                "HIR side uses a construct the symbolic tier doesn't model (see debug logs)",
            )));
        }
    };

    // Both sides' results may have different bit widths if PreHIR/HIR declare
    // different return types for the same logical value -- compare over
    // the wider of the two using each side's own signedness.
    let prehir_bits = sym_bits(&prehir_result);
    let hir_bits = sym_bits(&hir_result);
    let cmp_bits = prehir_bits.max(hir_bits);
    let prehir_cmp = lower_sym::resize(
        prehir_result,
        prehir_bits,
        cmp_bits,
        is_signed(&prehir.return_type),
    );
    let hir_cmp = lower_sym::resize(hir_result, hir_bits, cmp_bits, is_signed(&hir.return_type));

    Ok(PreparedSymbolicQuery {
        prehir_cmp,
        hir_cmp,
        prehir_bits,
        hir_bits,
        cmp_bits,
        param_specs,
    })
}

/// Check whether `prehir` and `hir` compute the same return value for every
/// input, for the symbolic tier's modeled subset (see [`crate::lower_sym`]'s
/// scope notes -- acyclic, `Bool`/`Int` only, no `Div`/`Mod`/`Sar`).
pub fn check_symbolic_equivalence(prehir: &PreHirFunction, hir: &HirFunction) -> SymbolicOutcome {
    let query = match prepare_symbolic_query(prehir, hir) {
        Ok(query) => query,
        Err(outcome) => return outcome,
    };

    let mut solver = Solver::new();
    let param_node_ids: Vec<_> = query
        .param_specs
        .iter()
        .map(|spec| {
            // Registers the Var's node so `get_value` can read a counterexample
            // model back for it after a `Sat` result.
            solver.register_node(spec.var.clone())
        })
        .collect();
    solver.assert(SymExpr::new_neq(
        query.prehir_cmp.clone(),
        query.hir_cmp.clone(),
    ));

    let result = match solver.check_sat() {
        Ok(result) => result,
        Err(err) => {
            tracing::debug!("lower_sym: solver error: {err}");
            return SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
                "solver query failed",
            ));
        }
    };
    match classify_solver_result(result) {
        SolverVerdict::Equivalent => SymbolicOutcome::Equivalent,
        SolverVerdict::Counterexample => {
            validate_solver_witness(prehir, hir, &query, &param_node_ids, |id| {
                solver.get_value(id)
            })
        }
        SolverVerdict::Unknown => SymbolicOutcome::Unsupported(UnsupportedReason::Construct(
            "solver returned Unknown -- should be rare/never with no memory theory engaged",
        )),
    }
}

fn validate_solver_witness<F>(
    prehir: &PreHirFunction,
    hir: &HirFunction,
    query: &PreparedSymbolicQuery,
    param_node_ids: &[SymNodeId],
    mut model_value: F,
) -> SymbolicOutcome
where
    F: FnMut(SymNodeId) -> Option<u64>,
{
    let mut referenced_vars = HashSet::new();
    let mut collect = |expr: &SymExpr| {
        if let SymExpr::Var { id, .. } = expr {
            referenced_vars.insert(*id);
        }
    };
    query.prehir_cmp.visit_preorder(&mut collect);
    query.hir_cmp.visit_preorder(&mut collect);

    let mut args = Vec::with_capacity(query.param_specs.len());
    let mut arg_widths = Vec::with_capacity(query.param_specs.len());
    let mut env = HashMap::with_capacity(query.param_specs.len());

    for (spec, &node_id) in query.param_specs.iter().zip(param_node_ids) {
        let val = match model_value(node_id) {
            Some(raw) => {
                if !fits_bits(raw, spec.width_bits) {
                    return SymbolicOutcome::Inconclusive {
                        reason: format!(
                            "solver returned out-of-width model value 0x{raw:x} for {}-bit \
                             parameter '{}'",
                            spec.width_bits, spec.name
                        ),
                    };
                }
                raw
            }
            None => {
                if referenced_vars.contains(&spec.var_id) {
                    return SymbolicOutcome::Inconclusive {
                        reason: format!(
                            "solver returned incomplete model: missing value for constrained \
                             parameter '{}'",
                            spec.name
                        ),
                    };
                }
                // Parameter does not appear in either lowered return expression:
                // explicitly complete the free input with a width-valid zero.
                0
            }
        };
        args.push(val);
        arg_widths.push(spec.width_bits);
        env.insert(spec.var_id, val);
    }

    // 1. Confirm the candidate tuple actually satisfies `prehir_cmp != hir_cmp`
    //    on the lowered bitvector formulas.
    let pre_sym = match eval_sym_bv(&query.prehir_cmp, &env) {
        Ok(v) => v,
        Err(reason) => return SymbolicOutcome::Inconclusive { reason },
    };
    let hir_sym = match eval_sym_bv(&query.hir_cmp, &env) {
        Ok(v) => v,
        Err(reason) => return SymbolicOutcome::Inconclusive { reason },
    };
    if pre_sym == hir_sym {
        return SymbolicOutcome::Inconclusive {
            reason: format!(
                "solver counterexample {:?} does not satisfy PreHIR != HIR (both evaluated to \
                 0x{pre_sym:x})",
                args
            ),
        };
    }

    // 2. Independently replay the concrete tuple against the PreHIR/HIR
    //    interpreter under the same parameter and return comparison relation.
    let eval_args: Vec<i64> = args.iter().map(|&v| v as i64).collect();
    let pre_replay = interpret_prehir(&prehir.body, &prehir.params, &prehir.locals, &eval_args);
    let hir_replay = interpret_hir(&hir.body, &hir.params, &hir.locals, &eval_args);

    match (pre_replay, hir_replay) {
        (Ok(Some(pre_ret)), Ok(Some(hir_ret))) => {
            let pre_cmp = widen_bv(
                pre_ret as u64,
                query.prehir_bits,
                query.cmp_bits,
                is_signed(&prehir.return_type),
            );
            let hir_cmp = widen_bv(
                hir_ret as u64,
                query.hir_bits,
                query.cmp_bits,
                is_signed(&hir.return_type),
            );
            if pre_cmp == hir_cmp {
                return SymbolicOutcome::Inconclusive {
                    reason: format!(
                        "concrete PreHIR/HIR replay agreed on candidate {:?} \
                         (both returned 0x{pre_cmp:x})",
                        args
                    ),
                };
            }
            SymbolicOutcome::Diverged(Counterexample { args, arg_widths })
        }
        (Ok(None), _) | (_, Ok(None)) => SymbolicOutcome::Inconclusive {
            reason: "concrete replay returned without a value on solver candidate".to_string(),
        },
        (Err(err), _) | (_, Err(err)) => SymbolicOutcome::Inconclusive {
            reason: format!("concrete replay failed on solver candidate: {err}"),
        },
    }
}

fn mask_bv(bits: u32) -> u64 {
    match bits {
        0 => 0,
        1..=63 => (1u64 << bits) - 1,
        _ => u64::MAX,
    }
}

fn fits_bits(val: u64, width_bits: u32) -> bool {
    width_bits >= 64 || (val >> width_bits) == 0
}

fn sext_i128(val: u64, bits: u32) -> i128 {
    match bits {
        0 => 0,
        1..=63 => {
            let masked = val & mask_bv(bits);
            let sign_bit = 1u64 << (bits - 1);
            if masked & sign_bit == 0 {
                i128::from(masked)
            } else {
                i128::from(masked) - (1i128 << bits)
            }
        }
        _ => i128::from(val as i64),
    }
}

fn widen_bv(raw: u64, from_bits: u32, to_bits: u32, signed: bool) -> u64 {
    let narrow = raw & mask_bv(from_bits);
    let widened = if signed && from_bits > 0 && from_bits < to_bits {
        sext_i128(narrow, from_bits) as u64
    } else {
        narrow
    };
    widened & mask_bv(to_bits)
}

fn eval_sym_bv(expr: &SymExpr, env: &HashMap<SymNodeId, u64>) -> Result<u64, String> {
    let width = expr.get_bit_width();
    if !(1..=64).contains(&width) {
        return Err(format!(
            "unsupported {width}-bit expression during witness evaluation"
        ));
    }
    let mask = mask_bv(width);
    let value = match expr {
        SymExpr::Const { val, size } => val & mask_bv(*size),
        SymExpr::Var {
            id,
            name,
            sort: Sort::BitVector(bits),
        } => {
            let val = env.get(id).copied().ok_or_else(|| {
                format!("witness evaluation missing parameter '{name}' (id={id})")
            })?;
            val & mask_bv(*bits)
        }
        SymExpr::Add(a, b) => eval_sym_bv(a, env)?.wrapping_add(eval_sym_bv(b, env)?),
        SymExpr::Sub(a, b) => eval_sym_bv(a, env)?.wrapping_sub(eval_sym_bv(b, env)?),
        SymExpr::Mul(a, b) => eval_sym_bv(a, env)?.wrapping_mul(eval_sym_bv(b, env)?),
        SymExpr::Udiv(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r == 0 { mask } else { l / r }
        }
        SymExpr::Urem(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r == 0 { l } else { l % r }
        }
        SymExpr::And(a, b) => eval_sym_bv(a, env)? & eval_sym_bv(b, env)?,
        SymExpr::Or(a, b) => eval_sym_bv(a, env)? | eval_sym_bv(b, env)?,
        SymExpr::Xor(a, b) => eval_sym_bv(a, env)? ^ eval_sym_bv(b, env)?,
        SymExpr::Shl(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                0
            } else {
                l.wrapping_shl(r as u32)
            }
        }
        SymExpr::Lshr(a, b) => {
            let l = eval_sym_bv(a, env)?;
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                0
            } else {
                l >> (r as u32)
            }
        }
        SymExpr::Ashr(a, b) => {
            let l = sext_i128(eval_sym_bv(a, env)?, width);
            let r = eval_sym_bv(b, env)?;
            if r >= u64::from(width) {
                if l < 0 { mask } else { 0 }
            } else {
                (l >> (r as u32)) as u64
            }
        }
        SymExpr::Eq(a, b) => u64::from(eval_sym_bv(a, env)? == eval_sym_bv(b, env)?),
        SymExpr::Neq(a, b) => u64::from(eval_sym_bv(a, env)? != eval_sym_bv(b, env)?),
        SymExpr::Ult(a, b) => u64::from(eval_sym_bv(a, env)? < eval_sym_bv(b, env)?),
        SymExpr::Ule(a, b) => u64::from(eval_sym_bv(a, env)? <= eval_sym_bv(b, env)?),
        SymExpr::Slt(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) < sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Sle(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) <= sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Sgt(a, b) => {
            let w = a.get_bit_width().max(b.get_bit_width());
            u64::from(sext_i128(eval_sym_bv(a, env)?, w) > sext_i128(eval_sym_bv(b, env)?, w))
        }
        SymExpr::Ite { cond, t, f } => {
            if eval_sym_bv(cond, env)? != 0 {
                eval_sym_bv(t, env)?
            } else {
                eval_sym_bv(f, env)?
            }
        }
        SymExpr::Extract { expr, lsb, size } => {
            let inner = eval_sym_bv(expr, env)?;
            if *lsb >= 64 {
                0
            } else {
                (inner >> *lsb) & mask_bv(*size)
            }
        }
        SymExpr::Concat(high, low) => {
            let low_w = low.get_bit_width();
            let hi_v = eval_sym_bv(high, env)?;
            let lo_v = eval_sym_bv(low, env)?;
            if low_w >= 64 {
                lo_v
            } else {
                (hi_v << low_w) | lo_v
            }
        }
        other => {
            return Err(format!(
                "unsupported symbolic operator during witness verification: {other:?}"
            ));
        }
    };
    Ok(value & mask)
}

fn sym_bits(e: &SymExpr) -> u32 {
    use fission_solver::ast::Sort;
    match e.get_sort() {
        Sort::BitVector(bits) => bits,
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::{SolverVerdict, classify_solver_result};

    fn int(bits: u32, signed: bool) -> fission_midend_core::ir::NirType {
        fission_midend_core::ir::NirType::Int { bits, signed }
    }

    fn prehir_returning(value: i64, bits: u32) -> fission_midend_prehir::PreHirFunction {
        use fission_midend_prehir::{PreHirExpr, PreHirStmt};
        fission_midend_prehir::PreHirFunction {
            return_type: int(bits, true),
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                value,
                int(bits, true),
            )))],
            ..Default::default()
        }
    }

    fn hir_returning(value: i64, bits: u32) -> fission_midend_core::ir::HirFunction {
        use fission_midend_core::ir::{HirExpr, HirStmt};
        fission_midend_core::ir::HirFunction {
            return_type: int(bits, true),
            body: vec![HirStmt::Return(Some(HirExpr::Const(
                value,
                int(bits, true),
            )))],
            ..Default::default()
        }
    }

    fn name(outcome: &super::SymbolicOutcome) -> &'static str {
        match outcome {
            super::SymbolicOutcome::Equivalent => "Equivalent",
            super::SymbolicOutcome::Diverged(_) => "Diverged",
            super::SymbolicOutcome::Unsupported(_) => "Unsupported",
            super::SymbolicOutcome::Inconclusive { .. } => "Inconclusive",
        }
    }

    /// A signed `-1` returned at eight bits and at thirty-two is the same
    /// value. Zero-extending the narrow side made it `255` and reported a
    /// divergence that is not one.
    #[test]
    fn a_signed_return_is_widened_by_sign_not_by_zero() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        let verdict = check_symbolic_equivalence(&prehir_returning(-1, 8), &hir_returning(-1, 32));
        assert!(
            matches!(verdict, SymbolicOutcome::Equivalent),
            "got {}",
            name(&verdict)
        );
        // The check still has teeth: a genuinely different value diverges.
        let different =
            check_symbolic_equivalence(&prehir_returning(-1, 8), &hir_returning(255, 32));
        assert!(
            matches!(different, SymbolicOutcome::Diverged(_)),
            "got {}",
            name(&different)
        );
    }
    use fission_solver::ast::SymExpr;
    use fission_solver::{SatResult, Solver};

    #[test]
    fn unknown_from_an_unencoded_operation_cannot_prove_dir_equivalence() {
        let a = SymExpr::new_float_var("a", 32);
        let b = SymExpr::new_float_var("b", 32);
        let unsupported = SymExpr::FAdd(Box::new(a), Box::new(b));
        let mut solver = Solver::new();
        solver.assert(SymExpr::Neq(
            Box::new(unsupported),
            Box::new(SymExpr::new_const(0, 32)),
        ));

        let result = solver.check_sat().expect("solver result");
        assert_eq!(result, SatResult::Unknown);
        assert_eq!(classify_solver_result(result), SolverVerdict::Unknown);
        assert_ne!(
            classify_solver_result(SatResult::Unknown),
            SolverVerdict::Equivalent
        );
    }

    #[test]
    fn lower_sym_rejects_incomplete_return_paths_and_uninitialized_local_reads() {
        use crate::lower_sym;
        use fission_midend_core::ir::NirType;
        use fission_midend_prehir::{PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirStmt};

        let i32_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let params = vec![PreHirBinding {
            name: "x".to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }];
        let param_vars = vec![SymExpr::new_var("x", 32)];

        // 1. Single guarded return with fallthrough must be rejected rather than
        // discarding `(x > 0)` and claiming constant return 42 on all inputs.
        let incomplete_body = vec![PreHirStmt::If {
            cond: PreHirExpr::Binary {
                op: PreHirBinaryOp::SGt,
                lhs: Box::new(PreHirExpr::Var("x".to_string())),
                rhs: Box::new(PreHirExpr::Const(0, i32_ty.clone())),
                ty: NirType::Bool,
            },
            then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                42,
                i32_ty.clone(),
            )))]
            .into(),
            else_body: Vec::new().into(),
        }];
        let err = lower_sym::prehir::lower(&incomplete_body, &params, &[], &param_vars)
            .expect_err("incomplete return path should be rejected");
        assert!(err.to_string().contains("incomplete return"));

        // 2. Reading an uninitialized local must fail rather than silently seeding to 0.
        let locals = vec![PreHirBinding {
            name: "uninit".to_string(),
            ty: i32_ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }];
        let uninit_body = vec![PreHirStmt::Return(Some(PreHirExpr::Var(
            "uninit".to_string(),
        )))];
        let err = lower_sym::prehir::lower(&uninit_body, &params, &locals, &param_vars)
            .expect_err("uninitialized local read should be rejected");
        assert!(err.to_string().contains("undeclared variable 'uninit'"));
    }

    #[test]
    fn signed_var_widening_cast_sign_extends_in_symbolic_equivalence() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt};

        let i32_ty = int(32, true);
        let i64_ty = int(64, true);

        let prehir = PreHirFunction {
            return_type: i64_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![PreHirStmt::Return(Some(PreHirExpr::Cast {
                ty: i64_ty.clone(),
                expr: Box::new(PreHirExpr::Var("x".to_string())),
            }))],
            ..Default::default()
        };
        let hir = HirFunction {
            return_type: i64_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::If {
                cond: HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::SLt,
                    lhs: Box::new(HirExpr::Var("x".to_string())),
                    rhs: Box::new(HirExpr::Const(0, i32_ty)),
                    ty: fission_midend_core::ir::NirType::Bool,
                },
                then_body: vec![HirStmt::Return(Some(HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::Or,
                    lhs: Box::new(HirExpr::Cast {
                        ty: i64_ty.clone(),
                        expr: Box::new(HirExpr::Var("x".to_string())),
                    }),
                    rhs: Box::new(HirExpr::Const(-0x1_0000_0000_i64, i64_ty.clone())),
                    ty: i64_ty.clone(),
                }))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Cast {
                    ty: i64_ty.clone(),
                    expr: Box::new(HirExpr::Var("x".to_string())),
                }))],
            }],
            ..Default::default()
        };

        let verdict = check_symbolic_equivalence(&prehir, &hir);
        assert!(
            matches!(verdict, SymbolicOutcome::Equivalent),
            "signed var widening cast should sign-extend, got {}",
            name(&verdict)
        );
    }

    #[test]
    fn symbolic_counterexample_returns_registered_model_values() {
        use super::{SymbolicOutcome, check_symbolic_equivalence};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i32_ty = int(32, true);
        let prehir = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(7, i32_ty.clone())),
                        ty: fission_midend_core::ir::NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty)))],
            ..Default::default()
        };

        let SymbolicOutcome::Diverged(ce) = check_symbolic_equivalence(&prehir, &hir) else {
            panic!("expected counterexample");
        };
        assert_eq!(ce.args, vec![7]);
        assert_eq!(ce.arg_widths, vec![32]);
    }

    #[test]
    fn constrained_missing_and_invalid_models_become_inconclusive_while_free_params_zero_complete()
    {
        use super::{SymbolicOutcome, prepare_symbolic_query, validate_solver_witness};
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i32_ty = int(32, true);
        let u8_ty = fission_midend_core::ir::NirType::Int {
            bits: 8,
            signed: false,
        };
        let pre_param = |name: &str, ty: &fission_midend_core::ir::NirType| PreHirBinding {
            name: name.to_string(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };
        let hir_param = |name: &str, ty: &fission_midend_core::ir::NirType| NirBinding {
            name: name.to_string(),
            ty: ty.clone(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        };

        // Two-parameter pair: `x` (u8, constrained at x == 7 or x == 0) and `unused` (i32, free).
        let eq_const_pair = |target: i64| {
            let prehir = PreHirFunction {
                return_type: i32_ty.clone(),
                params: vec![pre_param("x", &u8_ty), pre_param("unused", &i32_ty)],
                body: vec![
                    PreHirStmt::If {
                        cond: PreHirExpr::Binary {
                            op: PreHirBinaryOp::Eq,
                            lhs: Box::new(PreHirExpr::Var("x".to_string())),
                            rhs: Box::new(PreHirExpr::Const(target, u8_ty.clone())),
                            ty: fission_midend_core::ir::NirType::Bool,
                        },
                        then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                            1,
                            i32_ty.clone(),
                        )))]
                        .into(),
                        else_body: Vec::new().into(),
                    },
                    PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
                ],
                ..Default::default()
            };
            let hir = HirFunction {
                return_type: i32_ty.clone(),
                params: vec![hir_param("x", &u8_ty), hir_param("unused", &i32_ty)],
                body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
                ..Default::default()
            };
            (prehir, hir)
        };

        // 1 & 2: Missing constrained model value (`x` -> None) MUST yield Inconclusive,
        // even when `x == 0` would coincidentally satisfy the inequality.
        for target in [7_i64, 0_i64] {
            let (prehir, hir) = eq_const_pair(target);
            let query = prepare_symbolic_query(&prehir, &hir).expect("query lowers");
            let node_ids = [101_u32, 102_u32];
            let outcome =
                validate_solver_witness(&prehir, &hir, &query, &node_ids, |id| match id {
                    101 => None,
                    102 => Some(0),
                    _ => None,
                });
            assert!(
                matches!(outcome, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("missing value for constrained parameter 'x'")),
                "target={target}: expected Inconclusive for missing constrained param, got {outcome:?}"
            );
        }

        // 3: Missing value for genuinely free/unconstrained `unused` completes explicitly to 0,
        // while valid `x = 7` replays as Diverged with exact parameter widths `[8, 32]`.
        let (prehir7, hir7) = eq_const_pair(7);
        let query7 = prepare_symbolic_query(&prehir7, &hir7).expect("query lowers");
        let node_ids = [201_u32, 202_u32];
        let outcome = validate_solver_witness(&prehir7, &hir7, &query7, &node_ids, |id| match id {
            201 => Some(7),
            202 => None,
            _ => None,
        });
        assert_eq!(
            outcome,
            SymbolicOutcome::Diverged(super::Counterexample {
                args: vec![7, 0],
                arg_widths: vec![8, 32],
            })
        );

        // 4: Out-of-width model value for 8-bit `x` (`0x1ff`) yields Inconclusive.
        let out_of_width =
            validate_solver_witness(&prehir7, &hir7, &query7, &node_ids, |id| match id {
                201 => Some(0x1ff),
                202 => Some(0),
                _ => None,
            });
        assert!(
            matches!(out_of_width, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("out-of-width")),
            "expected out-of-width Inconclusive, got {out_of_width:?}"
        );

        // 5: Non-replaying model (`x = 0` when divergence is at `x == 7`) yields Inconclusive.
        let spurious =
            validate_solver_witness(&prehir7, &hir7, &query7, &node_ids, |id| match id {
                201 => Some(0),
                202 => Some(0),
                _ => None,
            });
        assert!(
            matches!(spurious, SymbolicOutcome::Inconclusive { ref reason } if reason.contains("does not satisfy")),
            "expected non-replaying Inconclusive, got {spurious:?}"
        );
    }

    #[test]
    fn witness_validation_preserves_width_sign_boundaries_and_rejects_unsupported_int_widths() {
        use super::{Counterexample, SymbolicOutcome, check_symbolic_equivalence};
        use crate::report::UnsupportedReason;
        use fission_midend_core::ir::{HirExpr, HirFunction, HirStmt, NirBinding, NirType};
        use fission_midend_prehir::{
            PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt,
        };

        let i8_ty = int(8, true);
        let i32_ty = int(32, true);
        let u32_ty = int(32, false);
        let u8_ty = int(8, false);
        let u64_ty = int(64, false);

        // 1. i8 signed boundary (-1 -> 0xff) preserves 8-bit width.
        let pre_i8 = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: i8_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1, i8_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_i8 = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: i8_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_i8, &hir_i8),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![0xff],
                arg_widths: vec![8],
            })
        );

        // 2. u64 full-width boundary (`u64::MAX`) preserves 64-bit width.
        let pre_u64 = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: u64_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(-1_i64, u64_ty.clone())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_u64 = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: u64_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty.clone())))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_u64, &hir_u64),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![u64::MAX],
                arg_widths: vec![64],
            })
        );

        // 3. Asymmetric parameter widths (PreHIR u32 vs HIR u8): divergence at x == 300
        //    must still be discovered and reported at width 32 rather than clamped to 8.
        let pre_wide = PreHirFunction {
            return_type: i32_ty.clone(),
            params: vec![PreHirBinding {
                name: "x".to_string(),
                ty: u32_ty.clone(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Eq,
                        lhs: Box::new(PreHirExpr::Var("x".to_string())),
                        rhs: Box::new(PreHirExpr::Const(300, u32_ty)),
                        ty: NirType::Bool,
                    },
                    then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                        1,
                        i32_ty.clone(),
                    )))]
                    .into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Const(0, i32_ty.clone()))),
            ],
            ..Default::default()
        };
        let hir_narrow = HirFunction {
            return_type: i32_ty.clone(),
            params: vec![NirBinding {
                name: "x".to_string(),
                ty: u8_ty,
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            body: vec![HirStmt::Return(Some(HirExpr::Const(0, i32_ty)))],
            ..Default::default()
        };
        assert_eq!(
            check_symbolic_equivalence(&pre_wide, &hir_narrow),
            SymbolicOutcome::Diverged(Counterexample {
                args: vec![300],
                arg_widths: vec![32],
            })
        );

        // 4. Unsupported >64-bit or 0-bit integer parameters are explicitly rejected.
        for bad_bits in [0_u32, 128_u32] {
            let bad_ty = int(bad_bits, false);
            let pre_bad = PreHirFunction {
                return_type: bad_ty.clone(),
                body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                    0,
                    bad_ty.clone(),
                )))],
                ..Default::default()
            };
            let hir_bad = HirFunction {
                return_type: bad_ty.clone(),
                body: vec![HirStmt::Return(Some(HirExpr::Const(0, bad_ty)))],
                ..Default::default()
            };
            assert!(
                matches!(
                    check_symbolic_equivalence(&pre_bad, &hir_bad),
                    SymbolicOutcome::Unsupported(UnsupportedReason::UnsupportedType(_))
                ),
                "expected UnsupportedType for {bad_bits}-bit integer"
            );
        }
    }
}
