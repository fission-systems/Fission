//! A tree-walking interpreter for `fission_midend_core::ir`'s `HirStmt`/
//! `HirExpr` AST.
//!
//! This is the load-bearing piece that makes DIR-vs-HIR differential
//! verification possible without any real machine execution: DIR (the
//! flattened, goto/label-based body structuring receives as input --
//! captured via `fission_pcode::take_last_dir_snapshot`) and HIR (the final
//! structured output) are the *same Rust type* (`Vec<HirStmt>`), so one
//! interpreter, fed the same concrete arguments, can evaluate both and diff
//! the results.
//!
//! # Scope (Phase 1)
//!
//! Supported: `Var`/`Const`/`Cast`/`Unary`/`Binary`/`Select` expressions;
//! `Assign` (to a plain `Var` only), `Block`, `Switch`, `If`, `While`,
//! `DoWhile`, `For`, `Label`, `Goto`, `Return`, `Break`, `Continue`
//! statements.
//!
//! Not supported -- `eval_expr`/`exec_stmt` return `Err` rather than
//! silently producing a wrong or default value, matching this codebase's
//! established discipline (see `crate::selfjit::compiler` in
//! `fission-emulator` for the same pattern): `HirExpr::Load`/`PtrOffset`/
//! `Index`/`FieldAccess`/`AggregateCopy`/`AddressOfGlobal`/`Call`, and
//! `HirLValue::Deref`/`Index`/`FieldAccess` on the assignment side. There is
//! no memory model in Phase 1 -- verification is scoped to pure/arithmetic
//! functions. A function that touches any of these bails out of
//! verification with a clear reason rather than being silently skipped or
//! falsely reported equivalent.
//!
//! `LogicalAnd`/`LogicalOr` are evaluated eagerly (both operands always
//! evaluated), not short-circuited -- correct as long as neither operand can
//! have a side effect, which holds given `Call` is unsupported (the only
//! source of a visible side effect in this AST).

use anyhow::{Result, bail};
use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirLValue, HirStmt, HirUnaryOp, NirBinding, NirType,
};
use std::collections::HashMap;

/// A single scalar value: the raw two's-complement bit pattern, meaningful
/// in its low `width(ty)` bits (see [`normalize`] -- every value entering or
/// leaving the environment is normalized to its declared type immediately,
/// so callers never need to re-mask).
pub type Value = i64;

#[derive(Debug, Clone)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return(Option<Value>),
    Goto(String),
}

struct Env {
    values: HashMap<String, Value>,
    types: HashMap<String, NirType>,
}

impl Env {
    fn get(&self, name: &str) -> Result<Value> {
        self.values
            .get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("interp: read of undeclared variable '{name}'"))
    }

    fn ty(&self, name: &str) -> Result<&NirType> {
        self.types
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("interp: no declared type for variable '{name}'"))
    }

    fn set(&mut self, name: &str, v: Value) {
        self.values.insert(name.to_string(), v);
    }
}

fn width_of(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        NirType::Ptr(_) => 64,
        // Not reachable for any Phase-1-supported expression (Aggregate/
        // Float carriers are all rejected before evaluation reaches here);
        // fall back to full width rather than panicking if one slips through.
        NirType::Aggregate { .. } | NirType::Float { .. } | NirType::Unknown => 64,
    }
}

fn is_signed(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { signed: true, .. })
}

/// Mask `raw` to `ty`'s declared width, sign-extending if `ty` is signed.
/// Every value stored in [`Env`] or returned from [`eval_expr`] has already
/// been normalized -- callers never need to re-mask.
fn normalize(raw: i64, ty: &NirType) -> i64 {
    let bits = width_of(ty).clamp(1, 64);
    if bits >= 64 {
        return raw;
    }
    let mask = (1i64 << bits) - 1;
    let v = raw & mask;
    if is_signed(ty) && (v & (1i64 << (bits - 1))) != 0 {
        v | !mask
    } else {
        v
    }
}

/// `raw`'s low `bits` bits, as an unsigned value -- used for unsigned
/// comparisons/shifts/division, where a signed `i64` reinterpretation would
/// be wrong (e.g. comparing a `u32` `0xFFFFFFFF` should read as
/// 4294967295, not -1).
fn unsigned_masked(raw: i64, bits: u32) -> u64 {
    if bits >= 64 {
        raw as u64
    } else {
        (raw as u64) & ((1u64 << bits) - 1)
    }
}

/// Infer the declared `NirType` of `expr` without evaluating it -- every
/// Phase-1-supported `HirExpr` variant carries its own result type
/// explicitly (`Const(_, ty)`, `Cast{ty,..}`, `Unary{ty,..}`,
/// `Binary{ty,..}`, `Select{ty,..}`) except `Var`, which is looked up in the
/// environment's declared param/local types.
fn infer_ty<'a>(expr: &'a HirExpr, env: &'a Env) -> Result<&'a NirType> {
    match expr {
        HirExpr::Var(name) => env.ty(name),
        HirExpr::Const(_, ty)
        | HirExpr::Cast { ty, .. }
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. } => Ok(ty),
        other => bail!("interp: cannot infer type of unsupported expr {other:?}"),
    }
}

fn eval_expr(expr: &HirExpr, env: &Env) -> Result<Value> {
    match expr {
        HirExpr::Var(name) => env.get(name),
        HirExpr::Const(v, ty) => Ok(normalize(*v, ty)),
        HirExpr::Cast { ty, expr } => Ok(normalize(eval_expr(expr, env)?, ty)),
        HirExpr::Unary { op, expr, ty } => {
            let v = eval_expr(expr, env)?;
            let raw = match op {
                HirUnaryOp::Neg => v.wrapping_neg(),
                HirUnaryOp::Not => {
                    if v == 0 {
                        1
                    } else {
                        0
                    }
                }
                HirUnaryOp::BitNot => !v,
            };
            Ok(normalize(raw, ty))
        }
        HirExpr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => {
            let c = eval_expr(cond, env)?;
            let v = if c != 0 {
                eval_expr(then_expr, env)?
            } else {
                eval_expr(else_expr, env)?
            };
            Ok(normalize(v, ty))
        }
        HirExpr::Call { target, args, ty } => eval_builtin_call(target, args, ty, env),
        other => bail!(
            "interp: unsupported expr {other:?} -- no memory/call model in Phase 1, see module docs"
        ),
    }
}

/// x86-flag-recovery pseudo-calls (`fission_pcode`'s own
/// `is_pure_intrinsic_call` list: `__carry`/`__scarry`/`__sborrow`/
/// `__popcount`) are the *only* `Call` targets Phase 1 evaluates -- real
/// x86 comparisons almost never survive to this HIR snapshot as a plain
/// `HirBinaryOp` comparison; they go through this flag-decomposition
/// machinery instead (`of = __sborrow(a,b); ... if (zf || xVar) ...`), so
/// without recognizing these specific four, Phase 1 couldn't verify almost
/// any real x86 function with a comparison in it -- confirmed by running
/// this crate's first real-binary test before this was added (see
/// `tests/real_binary_end_to_end.rs`; it hit exactly this gap). Any other
/// `Call` target still bails -- this is a small, fixed, well-known
/// whitelist of *pure* intrinsics, not general interprocedural support.
fn eval_builtin_call(target: &str, args: &[HirExpr], ty: &NirType, env: &Env) -> Result<Value> {
    match target {
        "__carry" | "__scarry" | "__sborrow" => {
            anyhow::ensure!(args.len() == 2, "interp: {target} needs 2 args");
            let l = eval_expr(&args[0], env)?;
            let r = eval_expr(&args[1], env)?;
            let bits = width_of(infer_ty(&args[0], env)?).clamp(1, 64);
            let flag = match target {
                "__carry" => unsigned_add_carries(l, r, bits),
                "__scarry" => signed_add_overflows(l, r, bits),
                "__sborrow" => signed_sub_overflows(l, r, bits),
                _ => unreachable!(),
            };
            Ok(normalize(bool_to_raw(flag), ty))
        }
        "__popcount" => {
            anyhow::ensure!(args.len() == 1, "interp: __popcount needs 1 arg");
            let v = eval_expr(&args[0], env)?;
            let bits = width_of(infer_ty(&args[0], env)?).clamp(1, 64);
            let count = unsigned_masked(v, bits).count_ones() as i64;
            Ok(normalize(count, ty))
        }
        other => bail!(
            "interp: unsupported call target '{other}' -- only the pure x86-flag \
             intrinsics (__carry/__scarry/__sborrow/__popcount) are modeled, see \
             module docs"
        ),
    }
}

/// Unsigned carry-out of `l + r` at `bits` width (`INT_CARRY`'s definition:
/// CF after an unsigned add) -- computed in `u128` to sidestep width-64
/// wraparound edge cases entirely rather than reasoning about it bitwise.
fn unsigned_add_carries(l: i64, r: i64, bits: u32) -> bool {
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let sum = unsigned_masked(l, bits) as u128 + unsigned_masked(r, bits) as u128;
    sum > mask as u128
}

/// Signed overflow of `l + r` at `bits` width (`INT_SCARRY`'s definition: OF
/// after a signed add), via `i128` arithmetic against the true signed range
/// for `bits`.
fn signed_add_overflows(l: i64, r: i64, bits: u32) -> bool {
    let (min, max) = signed_range(bits);
    let sum = sign_extend_i128(l, bits) + sign_extend_i128(r, bits);
    sum < min || sum > max
}

/// Signed overflow of `l - r` at `bits` width (`INT_SBORROW`'s definition:
/// OF after a signed subtract / `cmp`) -- this is the one this crate's real
/// end-to-end test actually hit (`__sborrow` guarding an `if (a > b)`-style
/// comparison's flag recovery).
fn signed_sub_overflows(l: i64, r: i64, bits: u32) -> bool {
    let (min, max) = signed_range(bits);
    let diff = sign_extend_i128(l, bits) - sign_extend_i128(r, bits);
    diff < min || diff > max
}

fn signed_range(bits: u32) -> (i128, i128) {
    if bits >= 64 {
        (i64::MIN as i128, i64::MAX as i128)
    } else {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    }
}

fn sign_extend_i128(raw: i64, bits: u32) -> i128 {
    if bits >= 64 {
        raw as i128
    } else {
        let masked = (raw as u64) & ((1u64 << bits) - 1);
        if masked & (1u64 << (bits - 1)) != 0 {
            (masked as i128) - (1i128 << bits)
        } else {
            masked as i128
        }
    }
}

fn eval_binary(
    op: &HirBinaryOp,
    lhs: &HirExpr,
    rhs: &HirExpr,
    ty: &NirType,
    env: &Env,
) -> Result<Value> {
    let l = eval_expr(lhs, env)?;
    let r = eval_expr(rhs, env)?;
    // Comparisons need the *operands'* width (to mask correctly for
    // unsigned comparisons); arithmetic/logical ops are normalized to the
    // *result* type `ty` after computing.
    let operand_bits = width_of(infer_ty(lhs, env)?).clamp(1, 64);

    let raw = match op {
        HirBinaryOp::Add => l.wrapping_add(r),
        HirBinaryOp::Sub => l.wrapping_sub(r),
        HirBinaryOp::Mul => l.wrapping_mul(r),
        HirBinaryOp::Div => {
            if r == 0 {
                bail!("interp: division by zero");
            }
            if is_signed(ty) {
                l.wrapping_div(r)
            } else {
                (unsigned_masked(l, operand_bits) / unsigned_masked(r, operand_bits)) as i64
            }
        }
        HirBinaryOp::Mod => {
            if r == 0 {
                bail!("interp: modulo by zero");
            }
            if is_signed(ty) {
                l.wrapping_rem(r)
            } else {
                (unsigned_masked(l, operand_bits) % unsigned_masked(r, operand_bits)) as i64
            }
        }
        HirBinaryOp::LogicalAnd => bool_to_raw(l != 0 && r != 0),
        HirBinaryOp::LogicalOr => bool_to_raw(l != 0 || r != 0),
        HirBinaryOp::And => l & r,
        HirBinaryOp::Or => l | r,
        HirBinaryOp::Xor => l ^ r,
        HirBinaryOp::Shl => l.wrapping_shl((r as u32) & 63),
        HirBinaryOp::Shr => (unsigned_masked(l, operand_bits) >> ((r as u32) & 63)) as i64,
        HirBinaryOp::Sar => l.wrapping_shr((r as u32) & 63),
        HirBinaryOp::Eq => bool_to_raw(l == r),
        HirBinaryOp::Ne => bool_to_raw(l != r),
        HirBinaryOp::Lt => bool_to_raw(unsigned_masked(l, operand_bits) < unsigned_masked(r, operand_bits)),
        HirBinaryOp::Le => bool_to_raw(unsigned_masked(l, operand_bits) <= unsigned_masked(r, operand_bits)),
        HirBinaryOp::Gt => bool_to_raw(unsigned_masked(l, operand_bits) > unsigned_masked(r, operand_bits)),
        HirBinaryOp::Ge => bool_to_raw(unsigned_masked(l, operand_bits) >= unsigned_masked(r, operand_bits)),
        HirBinaryOp::SLt => bool_to_raw(l < r),
        HirBinaryOp::SLe => bool_to_raw(l <= r),
        HirBinaryOp::SGt => bool_to_raw(l > r),
        HirBinaryOp::SGe => bool_to_raw(l >= r),
    };
    Ok(normalize(raw, ty))
}

fn bool_to_raw(b: bool) -> i64 {
    if b {
        1
    } else {
        0
    }
}

fn exec_stmt(stmt: &HirStmt, env: &mut Env) -> Result<Flow> {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            let name = match lhs {
                HirLValue::Var(name) => name,
                other => bail!(
                    "interp: unsupported assignment target {other:?} -- no memory model in Phase 1"
                ),
            };
            // A real function's `HirFunction::locals` doesn't necessarily
            // list every name its body assigns -- e.g. a return-value
            // scaffold slot (`NirBindingOrigin::ReturnScaffold`-style) can
            // be assigned without a matching `NirBinding` entry (confirmed
            // empirically: this crate's real end-to-end test fixture
            // assigns `local_4` in both `if` arms with no declaration for
            // it at all). Rather than bailing on every such case, infer the
            // type from the RHS and register it as a fresh variable the
            // first time it's assigned -- matches how a real dynamically-
            // typed evaluator would behave, and is strictly more permissive
            // than silently guessing a value.
            let ty = match env.types.get(name) {
                Some(ty) => ty.clone(),
                None => infer_ty(rhs, env)?.clone(),
            };
            let v = normalize(eval_expr(rhs, env)?, &ty);
            env.types.insert(name.clone(), ty);
            env.set(name, v);
            Ok(Flow::Normal)
        }
        HirStmt::Expr(e) => {
            eval_expr(e, env)?;
            Ok(Flow::Normal)
        }
        HirStmt::VaStart { .. } => bail!("interp: variadic functions are not supported in Phase 1"),
        HirStmt::Block(stmts) => exec_block(stmts, env),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            let v = eval_expr(expr, env)?;
            let body = cases
                .iter()
                .find(|c| c.values.contains(&v))
                .map(|c| c.body.as_slice())
                .unwrap_or(default.as_slice());
            match exec_block(body, env)? {
                // `Break` exits the nearest enclosing loop *or* switch --
                // consumed here, same as a loop consumes its own `Break`.
                Flow::Break => Ok(Flow::Normal),
                other => Ok(other),
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            if eval_expr(cond, env)? != 0 {
                exec_block(then_body, env)
            } else {
                exec_block(else_body, env)
            }
        }
        HirStmt::While { cond, body } => loop {
            if eval_expr(cond, env)? == 0 {
                return Ok(Flow::Normal);
            }
            match exec_block(body, env)? {
                Flow::Normal | Flow::Continue => continue,
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
        },
        HirStmt::DoWhile { body, cond } => loop {
            match exec_block(body, env)? {
                Flow::Normal | Flow::Continue => {}
                Flow::Break => return Ok(Flow::Normal),
                other => return Ok(other),
            }
            if eval_expr(cond, env)? == 0 {
                return Ok(Flow::Normal);
            }
        },
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(init) = init {
                match exec_stmt(init, env)? {
                    Flow::Normal => {}
                    other => return Ok(other),
                }
            }
            loop {
                if let Some(cond) = cond {
                    if eval_expr(cond, env)? == 0 {
                        return Ok(Flow::Normal);
                    }
                }
                match exec_block(body, env)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => return Ok(Flow::Normal),
                    other => return Ok(other),
                }
                if let Some(update) = update {
                    match exec_stmt(update, env)? {
                        Flow::Normal => {}
                        other => return Ok(other),
                    }
                }
            }
        }
        HirStmt::Label(_) => Ok(Flow::Normal),
        HirStmt::Goto(label) => Ok(Flow::Goto(label.clone())),
        HirStmt::Return(expr) => match expr {
            Some(e) => Ok(Flow::Return(Some(eval_expr(e, env)?))),
            None => Ok(Flow::Return(None)),
        },
        HirStmt::Break => Ok(Flow::Break),
        HirStmt::Continue => Ok(Flow::Continue),
    }
}

/// Execute a statement list, resolving any `Goto` whose target `Label` is
/// in `stmts` itself (at any position -- forward or backward) by moving the
/// cursor there; a `Goto` whose label isn't found here is propagated to the
/// caller, which tries its own (enclosing) statement list next. This is
/// what lets a label at function-body scope catch a `goto` issued from
/// inside a deeply nested `If`/`Block`.
fn exec_block(stmts: &[HirStmt], env: &mut Env) -> Result<Flow> {
    let mut idx = 0usize;
    while idx < stmts.len() {
        match exec_stmt(&stmts[idx], env)? {
            Flow::Normal => idx += 1,
            Flow::Goto(label) => match find_label(stmts, &label) {
                Some(target) => idx = target + 1,
                None => return Ok(Flow::Goto(label)),
            },
            other => return Ok(other),
        }
    }
    Ok(Flow::Normal)
}

fn find_label(stmts: &[HirStmt], label: &str) -> Option<usize> {
    stmts
        .iter()
        .position(|s| matches!(s, HirStmt::Label(l) if l == label))
}

/// Interpret `body` (either a DIR or a HIR snapshot -- same AST type) with
/// `args` bound to `params` in order, `locals` seeded from their
/// initializer expression (or 0 if none), and return the function's return
/// value (`None` for a bare `return;`, or if control fell off the end of
/// the body without an explicit `Return`).
///
/// Assumes `params`/`locals` are the same declaration lists for both the
/// DIR and HIR interpretation of a given function -- true as long as
/// structuring only rewrites control flow in `body` and doesn't rename or
/// introduce new bindings the DIR snapshot's variable references don't
/// know about. If that assumption is ever violated for a real function,
/// `Env::get`/`Env::ty` fail loudly (undeclared variable) rather than
/// silently reading a wrong value.
pub fn interpret(
    body: &[HirStmt],
    params: &[NirBinding],
    locals: &[NirBinding],
    args: &[i64],
) -> Result<Option<Value>> {
    anyhow::ensure!(
        args.len() == params.len(),
        "interp: {} args given, function has {} params",
        args.len(),
        params.len()
    );
    let mut env = Env {
        values: HashMap::new(),
        types: HashMap::new(),
    };
    for (p, a) in params.iter().zip(args) {
        env.types.insert(p.name.clone(), p.ty.clone());
        env.values.insert(p.name.clone(), normalize(*a, &p.ty));
    }
    for l in locals {
        env.types.insert(l.name.clone(), l.ty.clone());
        let v = match &l.initializer {
            Some(init) => eval_expr(init, &env)?,
            None => 0,
        };
        env.values.insert(l.name.clone(), v);
    }
    match exec_block(body, &mut env)? {
        Flow::Return(v) => Ok(v),
        Flow::Normal => Ok(None),
        other => bail!("interp: body ended in unexpected control state {other:?}"),
    }
}
