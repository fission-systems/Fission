//! Concrete-vs-symbolic agreement on the shared scalar contract
//! ([`crate::scalar_semantics`]).
//!
//! The symbolic tier's `Unsat` is a proof about the *lowered* bit-vector
//! formula; it certifies the IR only if the lowering computes exactly what
//! the concrete evaluator ([`crate::eval`]) computes. These tests hold the two
//! to each other three ways:
//!
//! 1. **Value agreement**: random typed expressions (every supported operator,
//!    every width in [`WIDTHS`], signed and unsigned, mixed operand/result/
//!    count widths, `Bool`) are evaluated concretely and through the lowered
//!    `SymExpr` -- exhaustively over every input when the parameters are at
//!    most 8 bits in total, over boundary and random inputs above that.
//! 2. **Solver agreement**: for a sample, the real solver must find
//!    `lowered != concrete` UNSAT with the inputs pinned, so the bit-blasted
//!    circuit -- not only this crate's witness evaluator -- agrees.
//! 3. **Verdict agreement**: pairs of functions (semantics-preserving
//!    rewrites and mutations) go through `check_symbolic_equivalence_report`;
//!    `Equivalent` must mean every input agrees concretely and `Diverged`
//!    must mean one does not.
//!
//! Randomness is a fixed-seed xorshift, so a failure reproduces exactly.

use std::collections::HashMap;

use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirFunction, HirStmt, HirUnaryOp, NirBinding, NirBindingOrigin, NirType,
};
use fission_midend_prehir::{
    PreHirBinaryOp, PreHirBinding, PreHirExpr, PreHirFunction, PreHirStmt, PreHirUnaryOp,
};
use fission_solver::Solver;
use fission_solver::ast::{SymExpr, SymNodeId};
use fission_solver::solver::SatResult;

use crate::eval::{interpret_hir, interpret_prehir};
use crate::lower_sym::{self, LowerSymError};
use crate::symbolic::{
    SymbolicOutcome, check_symbolic_equivalence, check_symbolic_equivalence_report, eval_sym_bv,
};

/// The widths the brief asks for: tiny (exhaustive), byte, odd, word
/// boundaries and the 64-bit edge.
const WIDTHS: [u32; 10] = [1, 2, 4, 7, 8, 16, 31, 32, 63, 64];

fn int(bits: u32, signed: bool) -> NirType {
    NirType::Int { bits, signed }
}

fn mask(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn bits_of(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        other => panic!("generator produced non-scalar type {other:?}"),
    }
}

fn signed_of(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { signed: true, .. })
}

// ---------------------------------------------------------------------------
// Deterministic generator.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn chance(&mut self, num: u64, den: u64) -> bool {
        self.next() % den < num
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn any_scalar_type(rng: &mut Rng) -> NirType {
    if rng.chance(1, 12) {
        NirType::Bool
    } else {
        int(*rng.pick(&WIDTHS), rng.chance(1, 2))
    }
}

/// Values that sit on the edges the contract has to get right: zero, one,
/// all-ones, the sign bit, the largest positive, and the shift-count edges
/// `w-1`, `w`, `w+1`, 63, 64, 65.
fn boundary_values(bits: u32) -> Vec<u64> {
    let m = mask(bits);
    let mut v = vec![0, 1, m, m >> 1, (m >> 1) + 1];
    for k in [
        u64::from(bits).saturating_sub(1),
        u64::from(bits),
        u64::from(bits) + 1,
        63,
        64,
        65,
    ] {
        v.push(k);
    }
    let mut out: Vec<u64> = v.into_iter().map(|x| x & m).collect();
    out.sort_unstable();
    out.dedup();
    out
}

const BINARY_OPS: [HirBinaryOp; 21] = [
    HirBinaryOp::Add,
    HirBinaryOp::Sub,
    HirBinaryOp::Mul,
    HirBinaryOp::And,
    HirBinaryOp::Or,
    HirBinaryOp::Xor,
    HirBinaryOp::Shl,
    HirBinaryOp::Shr,
    HirBinaryOp::Sar,
    HirBinaryOp::LogicalAnd,
    HirBinaryOp::LogicalOr,
    HirBinaryOp::Eq,
    HirBinaryOp::Ne,
    HirBinaryOp::Lt,
    HirBinaryOp::Le,
    HirBinaryOp::Gt,
    HirBinaryOp::Ge,
    HirBinaryOp::SLt,
    HirBinaryOp::SLe,
    HirBinaryOp::SGt,
    HirBinaryOp::SGe,
];

fn is_shift(op: HirBinaryOp) -> bool {
    matches!(op, HirBinaryOp::Shl | HirBinaryOp::Shr | HirBinaryOp::Sar)
}

fn is_comparison(op: HirBinaryOp) -> bool {
    matches!(
        op,
        HirBinaryOp::Eq
            | HirBinaryOp::Ne
            | HirBinaryOp::Lt
            | HirBinaryOp::Le
            | HirBinaryOp::Gt
            | HirBinaryOp::Ge
            | HirBinaryOp::SLt
            | HirBinaryOp::SLe
            | HirBinaryOp::SGt
            | HirBinaryOp::SGe
            | HirBinaryOp::LogicalAnd
            | HirBinaryOp::LogicalOr
    )
}

fn type_of(expr: &HirExpr, params: &[NirBinding]) -> NirType {
    match expr {
        HirExpr::Var(name) => params
            .iter()
            .find(|p| &p.name == name)
            .map(|p| p.ty.clone())
            .expect("generated var is a parameter"),
        HirExpr::Const(_, ty)
        | HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. } => ty.clone(),
        other => panic!("generator produced unsupported expr {other:?}"),
    }
}

fn leaf(rng: &mut Rng, params: &[NirBinding]) -> HirExpr {
    if !params.is_empty() && rng.chance(2, 3) {
        HirExpr::Var(rng.pick(params).name.clone())
    } else {
        let ty = any_scalar_type(rng);
        let bits = bits_of(&ty);
        let raw = if rng.chance(3, 4) {
            *rng.pick(&boundary_values(bits))
        } else {
            rng.next() & mask(bits)
        };
        HirExpr::Const(raw as i64, ty)
    }
}

fn gen_expr(rng: &mut Rng, params: &[NirBinding], depth: u32) -> HirExpr {
    if depth == 0 || rng.chance(1, 4) {
        return leaf(rng, params);
    }
    match rng.below(10) {
        0 | 1 => HirExpr::Cast {
            ty: any_scalar_type(rng),
            expr: Box::new(gen_expr(rng, params, depth - 1)),
        },
        2 => HirExpr::Unary {
            op: *rng.pick(&[HirUnaryOp::Neg, HirUnaryOp::Not, HirUnaryOp::BitNot]),
            expr: Box::new(gen_expr(rng, params, depth - 1)),
            ty: any_scalar_type(rng),
        },
        3 => HirExpr::Select {
            cond: Box::new(gen_expr(rng, params, depth - 1)),
            then_expr: Box::new(gen_expr(rng, params, depth - 1)),
            else_expr: Box::new(gen_expr(rng, params, depth - 1)),
            ty: any_scalar_type(rng),
        },
        4..=6 => {
            // Shifts get extra weight: they are what this module exists for.
            let op = *rng.pick(&[HirBinaryOp::Shl, HirBinaryOp::Shr, HirBinaryOp::Sar]);
            binary_with(rng, params, depth, op)
        }
        _ => {
            let op = *rng.pick(&BINARY_OPS);
            binary_with(rng, params, depth, op)
        }
    }
}

fn binary_with(rng: &mut Rng, params: &[NirBinding], depth: u32, op: HirBinaryOp) -> HirExpr {
    let lhs = gen_expr(rng, params, depth - 1);
    let rhs = gen_expr(rng, params, depth - 1);
    let ty = if is_comparison(op) && rng.chance(2, 3) {
        NirType::Bool
    } else if is_shift(op) && rng.chance(1, 2) {
        // Mostly keep the operand's type, but often not: narrowing and
        // widening shift results are exactly the reported counterexamples.
        type_of(&lhs, params)
    } else {
        any_scalar_type(rng)
    };
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty,
    }
}

fn gen_params(rng: &mut Rng, widths: &[u32], count: usize) -> Vec<NirBinding> {
    (0..count)
        .map(|i| {
            let bits = *rng.pick(widths);
            let ty = if bits == 1 && rng.chance(1, 3) {
                NirType::Bool
            } else {
                int(bits, rng.chance(1, 2))
            };
            NirBinding {
                name: format!("p{i}"),
                ty,
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(i)),
                initializer: None,
            }
        })
        .collect()
}

/// Every input tuple when the parameters total at most `exhaustive_bits`
/// bits; otherwise the boundary cross product (capped) plus random tuples.
fn input_tuples(rng: &mut Rng, params: &[NirBinding], exhaustive_bits: u32) -> Vec<Vec<u64>> {
    let widths: Vec<u32> = params.iter().map(|p| bits_of(&p.ty)).collect();
    let total: u32 = widths.iter().sum();
    if total <= exhaustive_bits {
        let mut out = Vec::with_capacity(1usize << total);
        for packed in 0u64..(1u64 << total) {
            let mut shift = 0;
            out.push(
                widths
                    .iter()
                    .map(|&w| {
                        let v = (packed >> shift) & mask(w);
                        shift += w;
                        v
                    })
                    .collect(),
            );
        }
        return out;
    }
    let mut out: Vec<Vec<u64>> = vec![Vec::new()];
    for &w in &widths {
        let values = boundary_values(w);
        out = out
            .into_iter()
            .flat_map(|prefix| {
                values.iter().map(move |&v| {
                    let mut t = prefix.clone();
                    t.push(v);
                    t
                })
            })
            .take(256)
            .collect();
    }
    for _ in 0..32 {
        out.push(widths.iter().map(|&w| rng.next() & mask(w)).collect());
    }
    out
}

// ---------------------------------------------------------------------------
// HIR -> PreHIR, so both macro instantiations are exercised on the same trees.
// ---------------------------------------------------------------------------

fn pre_unary(op: HirUnaryOp) -> PreHirUnaryOp {
    match op {
        HirUnaryOp::Neg => PreHirUnaryOp::Neg,
        HirUnaryOp::Not => PreHirUnaryOp::Not,
        HirUnaryOp::BitNot => PreHirUnaryOp::BitNot,
    }
}

fn pre_binary(op: HirBinaryOp) -> PreHirBinaryOp {
    match op {
        HirBinaryOp::Add => PreHirBinaryOp::Add,
        HirBinaryOp::Sub => PreHirBinaryOp::Sub,
        HirBinaryOp::Mul => PreHirBinaryOp::Mul,
        HirBinaryOp::Div => PreHirBinaryOp::Div,
        HirBinaryOp::Mod => PreHirBinaryOp::Mod,
        HirBinaryOp::LogicalAnd => PreHirBinaryOp::LogicalAnd,
        HirBinaryOp::LogicalOr => PreHirBinaryOp::LogicalOr,
        HirBinaryOp::And => PreHirBinaryOp::And,
        HirBinaryOp::Or => PreHirBinaryOp::Or,
        HirBinaryOp::Xor => PreHirBinaryOp::Xor,
        HirBinaryOp::Shl => PreHirBinaryOp::Shl,
        HirBinaryOp::Shr => PreHirBinaryOp::Shr,
        HirBinaryOp::Sar => PreHirBinaryOp::Sar,
        HirBinaryOp::Eq => PreHirBinaryOp::Eq,
        HirBinaryOp::Ne => PreHirBinaryOp::Ne,
        HirBinaryOp::Lt => PreHirBinaryOp::Lt,
        HirBinaryOp::Le => PreHirBinaryOp::Le,
        HirBinaryOp::Gt => PreHirBinaryOp::Gt,
        HirBinaryOp::Ge => PreHirBinaryOp::Ge,
        HirBinaryOp::SLt => PreHirBinaryOp::SLt,
        HirBinaryOp::SLe => PreHirBinaryOp::SLe,
        HirBinaryOp::SGt => PreHirBinaryOp::SGt,
        HirBinaryOp::SGe => PreHirBinaryOp::SGe,
    }
}

fn to_prehir(expr: &HirExpr) -> PreHirExpr {
    match expr {
        HirExpr::Var(n) => PreHirExpr::Var(n.clone()),
        HirExpr::Const(v, ty) => PreHirExpr::Const(*v, ty.clone()),
        HirExpr::Cast { ty, expr } => PreHirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(to_prehir(expr)),
        },
        HirExpr::Unary { op, expr, ty } => PreHirExpr::Unary {
            op: pre_unary(*op),
            expr: Box::new(to_prehir(expr)),
            ty: ty.clone(),
        },
        HirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: pre_binary(*op),
            lhs: Box::new(to_prehir(lhs)),
            rhs: Box::new(to_prehir(rhs)),
            ty: ty.clone(),
        },
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => PreHirExpr::Select {
            cond: Box::new(to_prehir(cond)),
            then_expr: Box::new(to_prehir(then_expr)),
            else_expr: Box::new(to_prehir(else_expr)),
            ty: ty.clone(),
        },
        other => panic!("to_prehir: unsupported {other:?}"),
    }
}

fn pre_params(params: &[NirBinding]) -> Vec<PreHirBinding> {
    params
        .iter()
        .map(|p| PreHirBinding {
            name: p.name.clone(),
            ty: p.ty.clone(),
            surface_type_name: None,
            origin: p.origin,
            initializer: None,
        })
        .collect()
}

fn hir_fn(params: &[NirBinding], expr: HirExpr) -> HirFunction {
    HirFunction {
        return_type: type_of(&expr, params),
        params: params.to_vec(),
        body: vec![HirStmt::Return(Some(expr))],
        ..Default::default()
    }
}

fn prehir_fn(params: &[NirBinding], expr: &HirExpr) -> PreHirFunction {
    PreHirFunction {
        return_type: type_of(expr, params),
        params: pre_params(params),
        body: vec![PreHirStmt::Return(Some(to_prehir(expr)))],
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// The two evaluations.
// ---------------------------------------------------------------------------

/// The concrete evaluator's value for `expr`, as a bit pattern of its
/// declared width.
fn concrete_hir(expr: &HirExpr, params: &[NirBinding], args: &[u64]) -> u64 {
    let bits = bits_of(&type_of(expr, params));
    let eval_args: Vec<i64> = args.iter().map(|&a| a as i64).collect();
    let v = interpret_hir(
        &[HirStmt::Return(Some(expr.clone()))],
        params,
        &[],
        &eval_args,
    )
    .unwrap_or_else(|e| panic!("concrete HIR refused {expr:?} on {args:?}: {e}"))
    .expect("value");
    (v as u64) & mask(bits)
}

fn concrete_prehir(expr: &PreHirExpr, params: &[PreHirBinding], args: &[u64], bits: u32) -> u64 {
    let eval_args: Vec<i64> = args.iter().map(|&a| a as i64).collect();
    let v = interpret_prehir(
        &[PreHirStmt::Return(Some(expr.clone()))],
        params,
        &[],
        &eval_args,
    )
    .unwrap_or_else(|e| panic!("concrete PreHIR refused {expr:?} on {args:?}: {e}"))
    .expect("value");
    (v as u64) & mask(bits)
}

struct Lowered {
    hir: SymExpr,
    prehir: SymExpr,
    vars: Vec<SymExpr>,
    ids: Vec<SymNodeId>,
}

/// The one refusal the contract documents for these generated trees: a
/// narrowing conversion of a multi-bit value into `Bool`.
fn is_documented_refusal(err: &LowerSymError) -> bool {
    matches!(err, LowerSymError::UnsupportedConstruct(msg) if msg.contains("into Bool"))
}

fn lower_both(expr: &HirExpr, params: &[NirBinding]) -> Result<Lowered, LowerSymError> {
    let vars: Vec<SymExpr> = params
        .iter()
        .map(|p| SymExpr::new_var(&p.name, bits_of(&p.ty)))
        .collect();
    let ids = vars
        .iter()
        .map(|v| match v {
            SymExpr::Var { id, .. } => *id,
            _ => unreachable!(),
        })
        .collect();
    let hir =
        lower_sym::hir::lower_checked(&[HirStmt::Return(Some(expr.clone()))], params, &[], &vars)?;
    let prehir = lower_sym::prehir::lower_checked(
        &[PreHirStmt::Return(Some(to_prehir(expr)))],
        &pre_params(params),
        &[],
        &vars,
    )?;
    Ok(Lowered {
        hir,
        prehir,
        vars,
        ids,
    })
}

fn sym_value(expr: &SymExpr, ids: &[SymNodeId], args: &[u64]) -> u64 {
    let env: HashMap<SymNodeId, u64> = ids.iter().copied().zip(args.iter().copied()).collect();
    eval_sym_bv(expr, &env).unwrap_or_else(|e| panic!("witness evaluator refused: {e}"))
}

#[derive(Default, Debug)]
struct Tally {
    expressions: usize,
    refused: usize,
    evaluations: usize,
    solver_checks: usize,
}

/// One random expression: concrete PreHIR/HIR and symbolic PreHIR/HIR must
/// give the same bits on every tuple; every `solver_every`-th expression is
/// also checked through the real solver.
fn check_expression(
    rng: &mut Rng,
    params: &[NirBinding],
    expr: &HirExpr,
    exhaustive_bits: u32,
    solver_check: bool,
    tally: &mut Tally,
) {
    tally.expressions += 1;
    let ty = type_of(expr, params);
    let bits = bits_of(&ty);
    let lowered = match lower_both(expr, params) {
        Ok(l) => l,
        Err(err) => {
            assert!(
                is_documented_refusal(&err),
                "undocumented lowering refusal {err} for {expr:?}"
            );
            tally.refused += 1;
            return;
        }
    };
    assert_eq!(
        lowered.hir.get_bit_width(),
        bits,
        "lowered width differs from the declared type of {expr:?}"
    );
    let pre_expr = to_prehir(expr);
    let pre_params = pre_params(params);
    let tuples = input_tuples(rng, params, exhaustive_bits);
    for args in &tuples {
        tally.evaluations += 1;
        let concrete = concrete_hir(expr, params, args);
        let concrete_pre = concrete_prehir(&pre_expr, &pre_params, args, bits);
        let symbolic = sym_value(&lowered.hir, &lowered.ids, args);
        let symbolic_pre = sym_value(&lowered.prehir, &lowered.ids, args);
        assert!(
            concrete == symbolic && concrete == concrete_pre && concrete == symbolic_pre,
            "concrete/symbolic disagree on {args:?} (params {:?}):\n  expr {expr:?}\n  \
             concrete HIR 0x{concrete:x}, PreHIR 0x{concrete_pre:x}; \
             symbolic HIR 0x{symbolic:x}, PreHIR 0x{symbolic_pre:x}",
            params.iter().map(|p| &p.ty).collect::<Vec<_>>()
        );
    }
    if solver_check {
        // Pin the inputs and ask the solver whether the lowered formula can
        // be anything but the concrete value: it must be UNSAT.
        for args in tuples.iter().take(3) {
            tally.solver_checks += 1;
            let expected = concrete_hir(expr, params, args);
            let mut solver = Solver::new();
            for (var, &val) in lowered.vars.iter().zip(args) {
                let w = var.get_bit_width();
                solver.assert(SymExpr::new_eq(
                    var.clone(),
                    SymExpr::Const {
                        val: val & mask(w),
                        size: w,
                    },
                ));
            }
            solver.assert(SymExpr::new_neq(
                lowered.hir.clone(),
                SymExpr::Const {
                    val: expected,
                    size: bits,
                },
            ));
            let result = solver.check_sat().expect("solver runs");
            assert_eq!(
                result,
                SatResult::Unsat,
                "solver disagrees with concrete 0x{expected:x} on {args:?} for {expr:?}"
            );
        }
    }
}

/// Exhaustive at small widths: parameters of 1, 2 and 4 bits (two of them
/// at most 8 bits in total), every input tuple, intermediate types of every
/// width in [`WIDTHS`].
#[test]
fn concrete_and_symbolic_agree_exhaustively_at_small_parameter_widths() {
    let mut rng = Rng(0x5EED_1330_0000_0001);
    let mut tally = Tally::default();
    for i in 0..1500 {
        let count = rng.below(3);
        let params = gen_params(&mut rng, &[1, 2, 4], count);
        let expr = gen_expr(&mut rng, &params, 3);
        check_expression(&mut rng, &params, &expr, 8, i % 10 == 0, &mut tally);
    }
    // Non-vacuity: the run must actually have compared something.
    assert!(tally.expressions - tally.refused >= 1300, "{tally:?}");
    assert!(tally.evaluations >= 10_000, "{tally:?}");
    assert!(tally.solver_checks >= 100, "{tally:?}");
}

/// Property run at every width in [`WIDTHS`] (parameters up to 64 bits):
/// boundary cross product plus random tuples per expression.
#[test]
fn concrete_and_symbolic_agree_on_boundary_and_random_inputs_at_all_widths() {
    let mut rng = Rng(0x5EED_1330_0000_0002);
    let mut tally = Tally::default();
    for i in 0..1500 {
        let count = 1 + rng.below(2);
        let params = gen_params(&mut rng, &WIDTHS, count);
        let expr = gen_expr(&mut rng, &params, 3);
        check_expression(&mut rng, &params, &expr, 8, i % 25 == 0, &mut tally);
    }
    assert!(tally.expressions - tally.refused >= 1300, "{tally:?}");
    assert!(tally.evaluations >= 20_000, "{tally:?}");
    assert!(tally.solver_checks >= 50, "{tally:?}");
}

/// Every shift kind, every operand/result/count width pairing from
/// [`WIDTHS`], signed and unsigned, every boundary count: the focused form
/// of the reported counterexamples.
#[test]
fn every_shift_width_pairing_agrees_on_every_boundary_count() {
    let mut rng = Rng(0x5EED_1330_0000_0003);
    let mut tally = Tally::default();
    for op in [HirBinaryOp::Shl, HirBinaryOp::Shr, HirBinaryOp::Sar] {
        for &vbits in &WIDTHS {
            for &rbits in &WIDTHS {
                for &cbits in &[1u32, 4, 7, 8, 16, 64] {
                    for (vs, cs) in [(false, false), (true, true), (true, false)] {
                        let params = vec![
                            NirBinding {
                                name: "v".to_string(),
                                ty: int(vbits, vs),
                                surface_type_name: None,
                                origin: Some(NirBindingOrigin::ParamIndex(0)),
                                initializer: None,
                            },
                            NirBinding {
                                name: "c".to_string(),
                                ty: int(cbits, cs),
                                surface_type_name: None,
                                origin: Some(NirBindingOrigin::ParamIndex(1)),
                                initializer: None,
                            },
                        ];
                        let expr = HirExpr::Binary {
                            op,
                            lhs: Box::new(HirExpr::Var("v".to_string())),
                            rhs: Box::new(HirExpr::Var("c".to_string())),
                            ty: int(rbits, vs),
                        };
                        check_expression(&mut rng, &params, &expr, 8, false, &mut tally);
                    }
                }
            }
        }
    }
    assert_eq!(tally.refused, 0, "{tally:?}");
    assert!(tally.evaluations >= 50_000, "{tally:?}");
}

// ---------------------------------------------------------------------------
// Verdict agreement: Equivalent / Diverged must match the concrete truth.
// ---------------------------------------------------------------------------

/// A rewrite that the shared contract says preserves the value.
fn preserving_rewrite(rng: &mut Rng, expr: &HirExpr, params: &[NirBinding]) -> HirExpr {
    let recurse = |rng: &mut Rng, e: &HirExpr| preserving_rewrite(rng, e, params);
    match expr {
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let (l, r) = (recurse(rng, lhs), recurse(rng, rhs));
            let commuted = |op| HirExpr::Binary {
                op,
                lhs: Box::new(r.clone()),
                rhs: Box::new(l.clone()),
                ty: ty.clone(),
            };
            match op {
                HirBinaryOp::Add
                | HirBinaryOp::Mul
                | HirBinaryOp::And
                | HirBinaryOp::Or
                | HirBinaryOp::Xor
                | HirBinaryOp::Eq
                | HirBinaryOp::Ne
                    if rng.chance(1, 2) =>
                {
                    commuted(*op)
                }
                HirBinaryOp::Lt if rng.chance(1, 2) => commuted(HirBinaryOp::Gt),
                HirBinaryOp::SLe if rng.chance(1, 2) => commuted(HirBinaryOp::SGe),
                // Shift at the operand width, then convert: what a narrowing
                // or widening shift result means under the contract.
                HirBinaryOp::Shr | HirBinaryOp::Sar | HirBinaryOp::Shl if rng.chance(1, 2) => {
                    let lty = type_of(lhs, params);
                    let lbits = bits_of(&lty);
                    let rbits = bits_of(ty);
                    let wide_bits = lbits.max(rbits);
                    let wide_signed = match op {
                        HirBinaryOp::Shr => false,
                        HirBinaryOp::Sar => true,
                        _ => signed_of(&lty),
                    };
                    // Reinterpret the operand at its own width with the
                    // signedness the operator implies (`Shl` keeps the
                    // operand's), extend it to the wide width, shift there,
                    // then convert to the declared result.
                    let own = if matches!(op, HirBinaryOp::Shl) {
                        l
                    } else {
                        cast(int(lbits, wide_signed), l)
                    };
                    let wide_ty = int(wide_bits, wide_signed);
                    cast(ty.clone(), bin(*op, cast(wide_ty.clone(), own), r, wide_ty))
                }
                _ => HirExpr::Binary {
                    op: *op,
                    lhs: Box::new(l),
                    rhs: Box::new(r),
                    ty: ty.clone(),
                },
            }
        }
        HirExpr::Cast { ty, expr } => HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(recurse(rng, expr)),
        },
        HirExpr::Unary { op, expr, ty } => HirExpr::Unary {
            op: *op,
            expr: Box::new(recurse(rng, expr)),
            ty: ty.clone(),
        },
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => HirExpr::Select {
            cond: Box::new(recurse(rng, cond)),
            then_expr: Box::new(recurse(rng, then_expr)),
            else_expr: Box::new(recurse(rng, else_expr)),
            ty: ty.clone(),
        },
        other => other.clone(),
    }
}

/// A mutation that usually (not always) changes the value.
fn mutate(rng: &mut Rng, expr: &HirExpr, params: &[NirBinding]) -> HirExpr {
    match expr {
        HirExpr::Binary { op, lhs, rhs, ty } if rng.chance(1, 2) => {
            let swapped = match op {
                HirBinaryOp::Shl => HirBinaryOp::Shr,
                HirBinaryOp::Shr => HirBinaryOp::Sar,
                HirBinaryOp::Sar => HirBinaryOp::Shl,
                HirBinaryOp::Add => HirBinaryOp::Sub,
                HirBinaryOp::Lt => HirBinaryOp::SLt,
                other => *rng.pick(
                    &BINARY_OPS
                        .iter()
                        .copied()
                        .filter(|o| o != other)
                        .collect::<Vec<_>>(),
                ),
            };
            HirExpr::Binary {
                op: swapped,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                ty: ty.clone(),
            }
        }
        HirExpr::Binary { op, lhs, rhs, ty } => HirExpr::Binary {
            op: *op,
            lhs: Box::new(mutate(rng, lhs, params)),
            rhs: rhs.clone(),
            ty: ty.clone(),
        },
        HirExpr::Const(v, ty) => HirExpr::Const(v.wrapping_add(1), ty.clone()),
        HirExpr::Cast { ty, expr } => HirExpr::Cast {
            ty: ty.clone(),
            expr: Box::new(mutate(rng, expr, params)),
        },
        other => HirExpr::Unary {
            op: HirUnaryOp::BitNot,
            expr: Box::new(other.clone()),
            ty: type_of(other, params),
        },
    }
}

#[test]
fn symbolic_verdicts_match_exhaustive_concrete_truth() {
    let mut rng = Rng(0x5EED_1330_0000_0004);
    let (mut equivalent, mut diverged, mut undecided) = (0usize, 0usize, 0usize);
    for i in 0..400 {
        let count = 1 + rng.below(2);
        let params = gen_params(&mut rng, &[1, 2, 4], count);
        let e1 = gen_expr(&mut rng, &params, 3);
        let e2 = if i % 2 == 0 {
            preserving_rewrite(&mut rng, &e1, &params)
        } else {
            mutate(&mut rng, &e1, &params)
        };
        // Same declared return type on both sides, so the observation is
        // the exact scalar and the truth is plain bit equality.
        let ty1 = type_of(&e1, &params);
        let e2 = if type_of(&e2, &params) == ty1 {
            e2
        } else {
            HirExpr::Cast {
                ty: ty1.clone(),
                expr: Box::new(e2),
            }
        };
        let tuples = input_tuples(&mut rng, &params, 8);
        let disagreeing: Vec<&Vec<u64>> = tuples
            .iter()
            .filter(|args| concrete_hir(&e1, &params, args) != concrete_hir(&e2, &params, args))
            .collect();
        let pre = prehir_fn(&params, &e1);
        let hir = hir_fn(&params, e2.clone());
        let report = check_symbolic_equivalence_report(&pre, &hir);
        match &report.outcome {
            SymbolicOutcome::Equivalent => {
                assert!(
                    disagreeing.is_empty(),
                    "UNSAT certified equivalence but concrete disagrees on {:?}:\n  pre {e1:?}\n  hir {e2:?}",
                    disagreeing[0]
                );
                equivalent += 1;
            }
            SymbolicOutcome::Diverged(cex) => {
                assert!(
                    !disagreeing.is_empty(),
                    "solver reported divergence {cex:?} but every input agrees:\n  pre {e1:?}\n  hir {e2:?}"
                );
                assert!(
                    disagreeing.iter().any(|t| **t == cex.args),
                    "counterexample {cex:?} is not a disagreeing input:\n  pre {e1:?}\n  hir {e2:?}"
                );
                diverged += 1;
            }
            SymbolicOutcome::Unsupported(_) | SymbolicOutcome::Inconclusive { .. } => {
                undecided += 1;
            }
        }
    }
    assert!(
        equivalent >= 120,
        "equivalent={equivalent} diverged={diverged} undecided={undecided}"
    );
    assert!(
        diverged >= 120,
        "equivalent={equivalent} diverged={diverged} undecided={undecided}"
    );
    assert!(
        undecided <= 80,
        "equivalent={equivalent} diverged={diverged} undecided={undecided}"
    );
}

// ---------------------------------------------------------------------------
// The reported counterexamples, end to end.
// ---------------------------------------------------------------------------

fn param(name: &str, ty: NirType, index: usize) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty,
        surface_type_name: None,
        origin: Some(NirBindingOrigin::ParamIndex(index)),
        initializer: None,
    }
}

fn var(name: &str) -> HirExpr {
    HirExpr::Var(name.to_string())
}

fn bin(op: HirBinaryOp, lhs: HirExpr, rhs: HirExpr, ty: NirType) -> HirExpr {
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty,
    }
}

fn cast(ty: NirType, expr: HirExpr) -> HirExpr {
    HirExpr::Cast {
        ty,
        expr: Box::new(expr),
    }
}

fn outcome(params: &[NirBinding], pre: &HirExpr, hir: HirExpr) -> SymbolicOutcome {
    check_symbolic_equivalence(&prehir_fn(params, pre), &hir_fn(params, hir))
}

/// 64-bit `x << 64` and `x << 65` are zero under the contract: both tiers
/// must say so, so that "equivalent to `0`" is a statement both tiers make.
#[test]
fn shift_by_64_and_65_is_zero_in_both_tiers() {
    let u64_ty = int(64, false);
    let params = [param("x", u64_ty.clone(), 0)];
    for count in [64, 65] {
        let shifted = bin(
            HirBinaryOp::Shl,
            var("x"),
            HirExpr::Const(count, u64_ty.clone()),
            u64_ty.clone(),
        );
        for x in [1u64, 3, u64::MAX] {
            assert_eq!(
                concrete_hir(&shifted, &params, &[x]),
                0,
                "x={x} count={count}"
            );
        }
        assert_eq!(
            outcome(&params, &shifted, HirExpr::Const(0, u64_ty.clone())),
            SymbolicOutcome::Equivalent
        );
        // And not equivalent to `x`, which is what `count & 63` computed.
        assert!(matches!(
            outcome(&params, &shifted, var("x")),
            SymbolicOutcome::Diverged(_)
        ));
    }
}

/// 16-bit `x >> 1` with an 8-bit result keeps bit 8 (`0x0100 >> 1 == 0x80`).
/// Truncating `x` to 8 bits before the shift proved it "equivalent" to
/// `(u8)x >> 1`, which differs at `x == 0x100`.
#[test]
fn a_narrowing_shift_is_not_equivalent_to_shifting_the_truncated_operand() {
    let (u16_ty, u8_ty) = (int(16, false), int(8, false));
    let params = [param("x", u16_ty.clone(), 0)];
    let narrowing = bin(
        HirBinaryOp::Shr,
        var("x"),
        HirExpr::Const(1, u16_ty.clone()),
        u8_ty.clone(),
    );
    let truncate_first = bin(
        HirBinaryOp::Shr,
        cast(u8_ty.clone(), var("x")),
        HirExpr::Const(1, u8_ty.clone()),
        u8_ty.clone(),
    );
    assert_eq!(concrete_hir(&narrowing, &params, &[0x0100]), 0x80);
    match outcome(&params, &narrowing, truncate_first) {
        SymbolicOutcome::Diverged(cex) => {
            assert_eq!(cex.args[0] & 0x100, 0x100, "{cex:?}")
        }
        other => panic!("expected a confirmed divergence, got {other:?}"),
    }
    // Shifting at the operand width, then narrowing, is the same function.
    let shift_then_narrow = cast(
        u8_ty,
        bin(
            HirBinaryOp::Shr,
            var("x"),
            HirExpr::Const(1, u16_ty.clone()),
            u16_ty,
        ),
    );
    assert_eq!(
        outcome(&params, &narrowing, shift_then_narrow),
        SymbolicOutcome::Equivalent
    );
}

/// A 16-bit count of 256 shifts an 8-bit value to zero; truncating the
/// count to the result width made it a shift by 0.
#[test]
fn a_count_wider_than_the_operand_is_not_truncated() {
    let (u8_ty, u16_ty) = (int(8, false), int(16, false));
    let params = [param("x", u8_ty.clone(), 0), param("c", u16_ty, 1)];
    let wide_count = bin(HirBinaryOp::Shl, var("x"), var("c"), u8_ty.clone());
    let truncated_count = bin(
        HirBinaryOp::Shl,
        var("x"),
        cast(u8_ty.clone(), var("c")),
        u8_ty,
    );
    assert_eq!(concrete_hir(&wide_count, &params, &[1, 256]), 0);
    assert_eq!(concrete_hir(&truncated_count, &params, &[1, 256]), 1);
    assert!(matches!(
        outcome(&params, &wide_count, truncated_count),
        SymbolicOutcome::Diverged(_)
    ));
}

/// A negative signed count is a large unsigned count: 64-bit `x << (i8)-1`
/// is zero, not `x << 63`.
#[test]
fn a_negative_signed_count_saturates() {
    let (u64_ty, i8_ty) = (int(64, false), int(8, true));
    let params = [param("x", u64_ty.clone(), 0), param("c", i8_ty, 1)];
    let shifted = bin(HirBinaryOp::Shl, var("x"), var("c"), u64_ty.clone());
    assert_eq!(concrete_hir(&shifted, &params, &[1, 0xff]), 0);
    let guarded = HirExpr::Select {
        cond: Box::new(bin(
            HirBinaryOp::SLt,
            var("c"),
            HirExpr::Const(0, int(8, true)),
            NirType::Bool,
        )),
        then_expr: Box::new(HirExpr::Const(0, u64_ty.clone())),
        else_expr: Box::new(shifted.clone()),
        ty: u64_ty,
    };
    assert_eq!(
        outcome(&params, &shifted, guarded),
        SymbolicOutcome::Equivalent
    );
}

/// Signed operands with widening and narrowing results: `Shl` extends the
/// operand by its own signedness, `Shr` zero-extends it and `Sar`
/// sign-extends it, each from the operand width.
#[test]
fn signed_operands_with_widening_and_narrowing_results_follow_the_contract() {
    let (i8_ty, i16_ty, u16_ty, u8_ty) =
        (int(8, true), int(16, true), int(16, false), int(8, false));
    let params = [param("x", i8_ty.clone(), 0)];
    let one16 = HirExpr::Const(1, i16_ty.clone());
    // Shl i8 -> i16 sign-extends first: (-128) << 1 == -256.
    let shl_widen = bin(HirBinaryOp::Shl, var("x"), one16.clone(), i16_ty.clone());
    assert_eq!(concrete_hir(&shl_widen, &params, &[0x80]), 0xff00);
    assert_eq!(
        outcome(
            &params,
            &shl_widen,
            bin(
                HirBinaryOp::Shl,
                cast(i16_ty.clone(), var("x")),
                one16.clone(),
                i16_ty.clone()
            )
        ),
        SymbolicOutcome::Equivalent
    );
    // Shr i8 -> i16 shifts the operand's own 8 bits logically: 0x80 >> 1 == 0x40.
    let shr_widen = bin(HirBinaryOp::Shr, var("x"), one16.clone(), i16_ty.clone());
    assert_eq!(concrete_hir(&shr_widen, &params, &[0x80]), 0x40);
    assert_eq!(
        outcome(
            &params,
            &shr_widen,
            bin(
                HirBinaryOp::Shr,
                cast(u16_ty, cast(u8_ty.clone(), var("x"))),
                one16.clone(),
                i16_ty.clone()
            )
        ),
        SymbolicOutcome::Equivalent
    );
    // Sar i8 -> i16 sign-extends: 0x80 >> 1 == 0xffc0.
    let sar_widen = bin(HirBinaryOp::Sar, var("x"), one16.clone(), i16_ty.clone());
    assert_eq!(concrete_hir(&sar_widen, &params, &[0x80]), 0xffc0);
    assert_eq!(
        outcome(
            &params,
            &sar_widen,
            bin(
                HirBinaryOp::Sar,
                cast(i16_ty.clone(), var("x")),
                one16,
                i16_ty
            )
        ),
        SymbolicOutcome::Equivalent
    );
    // Sar with a narrower result and a count past the result width but
    // inside the operand width: the sign still comes from the operand.
    let params16 = [param("y", int(16, true), 0)];
    let sar_narrow = bin(
        HirBinaryOp::Sar,
        var("y"),
        HirExpr::Const(12, int(16, false)),
        u8_ty,
    );
    assert_eq!(concrete_hir(&sar_narrow, &params16, &[0x8000]), 0xf8);
    assert_eq!(concrete_hir(&sar_narrow, &params16, &[0x7000]), 0x07);
}

/// A narrowing conversion into `Bool` is bit truncation in p-code and
/// `!= 0` in the C the printer writes; the symbolic tier refuses to pick
/// one and certify (or refute) on it.
#[test]
fn a_narrowing_conversion_into_bool_is_refused_not_decided() {
    let u8_ty = int(8, false);
    let params = [param("x", u8_ty.clone(), 0)];
    let as_bool = cast(NirType::Bool, var("x"));
    let nonzero = bin(
        HirBinaryOp::Ne,
        var("x"),
        HirExpr::Const(0, u8_ty),
        NirType::Bool,
    );
    assert!(matches!(
        outcome(&params, &as_bool, nonzero),
        SymbolicOutcome::Unsupported(_)
    ));
    // Widening from Bool is unambiguous and still decided.
    let bparams = [param("b", NirType::Bool, 0)];
    let widened = cast(int(8, false), var("b"));
    let selected = HirExpr::Select {
        cond: Box::new(var("b")),
        then_expr: Box::new(HirExpr::Const(1, int(8, false))),
        else_expr: Box::new(HirExpr::Const(0, int(8, false))),
        ty: int(8, false),
    };
    assert_eq!(
        outcome(&bparams, &widened, selected),
        SymbolicOutcome::Equivalent
    );
}
