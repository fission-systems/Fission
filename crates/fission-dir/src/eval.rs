//! A tree-walking interpreter for `PreHirStmt`/`PreHirExpr` (from
//! `fission-midend-prehir`) and `HirStmt`/`HirExpr` (from `fission-midend-core`).
//!
//! This is the concrete-tier backend used by both [`crate::diff`] (PreHIR-vs-HIR
//! diffing, no ground truth) and [`crate::ground_truth`] (PreHIR/HIR-vs-real-
//! machine-code comparison). Two interpreters, [`prehir::interpret`] and
//! [`hir::interpret`], generated from one [`define_interp!`] macro
//! invocation each, so the interpretation *logic* can never silently drift
//! between the two even though `PreHirStmt`/`HirStmt` are genuinely separate
//! types (see `fission_midend_core::ir::hir`'s module doc). Divergence
//! between PreHIR and HIR results is exactly what this crate exists to check
//! for -- if this file's own two copies disagreed on what "the same"
//! p-code-derived logic means, that would corrupt the very comparison this
//! crate makes.
//!
//! # Scope (concrete tier)
//!
//! Supported: `Var`/`Const`/`Cast`/`Unary`/`Binary`/`Select` expressions;
//! `Assign` (to a plain `Var` only), `Block`, `Switch`, `If`, `While`,
//! `DoWhile`, `For`, `Label`, `Goto`, `Return`, `Break`, `Continue`
//! statements.
//!
//! Not supported -- `eval_expr`/`exec_stmt` return `Err` rather than
//! silently producing a wrong or default value: `Load`/`PtrOffset`/`Index`/
//! `FieldAccess`/`AggregateCopy`/`AddressOfGlobal`/general `Call`, and
//! `Deref`/`Index`/`FieldAccess` on the assignment side. There is no memory
//! model at this tier -- verification is scoped to pure/arithmetic
//! functions. A function that touches any of these bails out of
//! verification with a clear reason rather than being silently skipped or
//! falsely reported equivalent.
//!
//! `LogicalAnd`/`LogicalOr` are evaluated eagerly (both operands always
//! evaluated), not short-circuited -- correct as long as neither operand can
//! have a side effect, which holds given general `Call` is unsupported (the
//! only source of a visible side effect in this AST).

use fission_midend_core::ir::NirType;

/// A single scalar value: the raw two's-complement bit pattern, meaningful
/// in its low declared-type-width bits -- every value entering or leaving
/// an interpreter's environment is normalized to its declared type
/// immediately, so callers never need to re-mask.
pub type Value = i64;

/// `ty`'s declared bit width, clamped to `[1, 64]` (this interpreter's
/// values are always plain `i64`s). Shared by both `prehir::`/`hir::` and by
/// [`crate::ground_truth`], which needs the same width to mask a real
/// emulator register value for a fair comparison against an interpreted
/// result.
pub fn width_of(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        NirType::Ptr(_) => 64,
        // Not reachable for any supported expression (Aggregate/Float
        // carriers are all rejected before evaluation reaches here); fall
        // back to full width rather than panicking if one slips through.
        NirType::Aggregate { .. } | NirType::Float { .. } | NirType::Unknown => 64,
    }
}

pub fn is_signed(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { signed: true, .. })
}

/// Mask `raw` to `ty`'s declared width, sign-extending if `ty` is signed.
pub fn normalize(raw: i64, ty: &NirType) -> i64 {
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

/// Generates one interpreter module for a `Stmt`/`Expr`/`LValue`/`BinaryOp`/
/// `UnaryOp`/`Binding`/`Function` family (either all `Dir*`, imported from
/// `fission_midend_prehir`, or all `Hir*` plus the shared `NirBinding`,
/// imported from `fission_midend_core::ir`). See this file's module doc for
/// why this is a macro instantiated twice rather than one generic/
/// trait-based implementation or two independently hand-written copies.
macro_rules! define_interp {
    (
        $modname:ident, $ir_crate:path,
        $Stmt:ident, $Expr:ident, $LValue:ident, $BinaryOp:ident, $UnaryOp:ident,
        $Binding:ident, $Function:ident
    ) => {
        pub mod $modname {
            use anyhow::{Result, bail};
            use $ir_crate::{$BinaryOp, $Binding, $Expr, $Function, $LValue, $Stmt, $UnaryOp};
            use fission_midend_core::ir::NirType;
            use std::collections::HashMap;

            use super::Value;

            #[derive(Debug, Clone)]
            enum Flow {
                Normal,
                Break,
                Continue,
                Return(Option<Value>),
                Goto(String),
            }

            /// An address's own type. Pointer arithmetic produces an address
            /// whatever it points at, so `infer_ty` needs one to hand back.
            const PTR_TY: NirType = NirType::Int {
                bits: 64,
                signed: false,
            };

            /// Sparse byte-addressable memory.
            ///
            /// Only bytes this body actually stored exist. A read that
            /// reaches an unwritten byte is an error, not a zero -- the same
            /// contract the variable environment keeps, and for the same
            /// reason: against a real emulator, an invented value is a
            /// confident wrong answer, and it can as easily make a wrong body
            /// agree as make a right one diverge.
            ///
            /// So this models memory a function *uses internally* -- stores to
            /// a stack slot or a buffer, then loads them back -- and still
            /// refuses globals and caller-owned pointers, whose contents this
            /// tier has no way to know.
            #[derive(Default)]
            struct Memory {
                bytes: HashMap<u64, u8>,
            }

            impl Memory {
                /// Little-endian, matching every target in the corpus this
                /// verifies against.
                fn read(&self, addr: u64, bits: u32) -> Result<Value> {
                    let n = bytes_for(bits);
                    let mut raw: u64 = 0;
                    for i in 0..n {
                        let at = addr.wrapping_add(i as u64);
                        let b = self.bytes.get(&at).copied().ok_or_else(|| {
                            anyhow::anyhow!(
                                "interp: load from never-written address {at:#x} -- this tier \
                                 models only memory the body itself wrote"
                            )
                        })?;
                        raw |= (b as u64) << (8 * i);
                    }
                    Ok(raw as i64)
                }

                fn write(&mut self, addr: u64, bits: u32, value: Value) {
                    for i in 0..bytes_for(bits) {
                        let b = ((value as u64) >> (8 * i)) as u8;
                        self.bytes.insert(addr.wrapping_add(i as u64), b);
                    }
                }
            }

            /// Bytes a value of `bits` occupies, rounded up and capped at 8.
            fn bytes_for(bits: u32) -> u32 {
                bits.div_ceil(8).clamp(1, 8)
            }

            struct Env {
                values: HashMap<String, Value>,
                types: HashMap<String, NirType>,
                mem: Memory,
            }

            impl Env {
                fn get(&self, name: &str) -> Result<Value> {
                    self.values.get(name).copied().ok_or_else(|| {
                        anyhow::anyhow!("interp: read of undeclared variable '{name}'")
                    })
                }

                fn ty(&self, name: &str) -> Result<&NirType> {
                    self.types.get(name).ok_or_else(|| {
                        anyhow::anyhow!("interp: no declared type for variable '{name}'")
                    })
                }

                fn set(&mut self, name: &str, v: Value) {
                    self.values.insert(name.to_string(), v);
                }
            }

            use super::{is_signed, normalize, width_of};

            /// `raw`'s low `bits` bits, as an unsigned value -- used for
            /// unsigned comparisons/shifts/division, where a signed `i64`
            /// reinterpretation would be wrong (e.g. comparing a `u32`
            /// `0xFFFFFFFF` should read as 4294967295, not -1).
            fn unsigned_masked(raw: i64, bits: u32) -> u64 {
                if bits >= 64 {
                    raw as u64
                } else {
                    (raw as u64) & ((1u64 << bits) - 1)
                }
            }

            /// Infer the declared `NirType` of `expr` without evaluating it
            /// -- every supported expr variant carries its own result type
            /// explicitly (`Const(_, ty)`, `Cast{ty,..}`, `Unary{ty,..}`,
            /// `Binary{ty,..}`, `Select{ty,..}`) except `Var`, which is
            /// looked up in the environment's declared param/local types.
            fn infer_ty<'a>(expr: &'a $Expr, env: &'a Env) -> Result<&'a NirType> {
                match expr {
                    $Expr::Var(name) => env.ty(name),
                    $Expr::Const(_, ty)
                    | $Expr::Cast { ty, .. }
                    | $Expr::Unary { ty, .. }
                    | $Expr::Binary { ty, .. }
                    | $Expr::Select { ty, .. }
                    | $Expr::Load { ty, .. }
                    | $Expr::FieldAccess { ty, .. } => Ok(ty),
                    // An address is an address, whatever it points at.
                    $Expr::PtrOffset { .. } | $Expr::Index { .. } => Ok(&PTR_TY),
                    other => bail!("interp: cannot infer type of unsupported expr {other:?}"),
                }
            }

            /// The address expression and width a store targets, or `None`
            /// for a plain variable.
            ///
            /// An `Index` or `FieldAccess` target is the same address
            /// arithmetic `eval_expr` already does for the reading side, so it
            /// is expressed by reusing those forms rather than repeating the
            /// stride and offset rules -- two copies of that arithmetic
            /// drifting apart would corrupt exactly the comparison this crate
            /// makes.
            fn store_target(lhs: &$LValue) -> Option<(&$Expr, u32)> {
                match lhs {
                    $LValue::Var(_) => None,
                    $LValue::Deref { ptr, ty } => Some((ptr, width_of(ty))),
                    // `base`/`index` and `base`/`offset` are carried by the
                    // lvalue itself, so the address cannot be handed back as a
                    // single borrowed expression; those stay unsupported until
                    // there is a reason to widen this.
                    _ => None,
                }
            }

            fn eval_expr(expr: &$Expr, env: &Env) -> Result<Value> {
                match expr {
                    $Expr::Var(name) => env.get(name),
                    $Expr::Const(v, ty) => Ok(normalize(*v, ty)),
                    $Expr::Cast { ty, expr } => Ok(normalize(eval_expr(expr, env)?, ty)),
                    $Expr::Unary { op, expr, ty } => {
                        let v = eval_expr(expr, env)?;
                        let raw = match op {
                            $UnaryOp::Neg => v.wrapping_neg(),
                            $UnaryOp::Not => {
                                if v == 0 {
                                    1
                                } else {
                                    0
                                }
                            }
                            $UnaryOp::BitNot => !v,
                        };
                        Ok(normalize(raw, ty))
                    }
                    $Expr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
                    $Expr::Select {
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
                    $Expr::Load { ptr, ty } => {
                        let addr = eval_expr(ptr, env)? as u64;
                        Ok(normalize(env.mem.read(addr, width_of(ty))?, ty))
                    }
                    $Expr::PtrOffset { base, offset } => {
                        Ok((eval_expr(base, env)?).wrapping_add(*offset))
                    }
                    $Expr::Index {
                        base,
                        index,
                        elem_ty,
                    } => {
                        let stride = bytes_for(width_of(elem_ty)) as i64;
                        let i = eval_expr(index, env)?;
                        Ok((eval_expr(base, env)?).wrapping_add(i.wrapping_mul(stride)))
                    }
                    $Expr::FieldAccess { base, offset, .. } => {
                        Ok((eval_expr(base, env)?).wrapping_add(*offset as i64))
                    }
                    $Expr::Call { target, args, ty } => eval_builtin_call(target, args, ty, env),
                    other => bail!(
                        "interp: unsupported expr {other:?} -- no memory/call model at the \
                         concrete tier, see module docs"
                    ),
                }
            }

            /// x86-flag-recovery pseudo-calls (`fission_pcode`'s own
            /// `is_pure_intrinsic_call` list: `__carry`/`__scarry`/
            /// `__sborrow`/`__popcount`) are the *only* `Call` targets
            /// evaluated -- real x86 comparisons almost never survive as a
            /// plain binary-op comparison; they go through this
            /// flag-decomposition machinery instead (`of = __sborrow(a,b);
            /// ... if (zf || xVar) ...`), so without recognizing these
            /// specific four, this tier couldn't verify almost any real
            /// x86 function with a comparison in it. Any other `Call`
            /// target still bails -- a small, fixed, well-known whitelist
            /// of *pure* intrinsics, not general interprocedural support.
            fn eval_builtin_call(
                target: &str,
                args: &[$Expr],
                ty: &NirType,
                env: &Env,
            ) -> Result<Value> {
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

            /// Unsigned carry-out of `l + r` at `bits` width (`INT_CARRY`'s
            /// definition: CF after an unsigned add) -- computed in `u128`
            /// to sidestep width-64 wraparound edge cases entirely rather
            /// than reasoning about it bitwise.
            fn unsigned_add_carries(l: i64, r: i64, bits: u32) -> bool {
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let sum = unsigned_masked(l, bits) as u128 + unsigned_masked(r, bits) as u128;
                sum > mask as u128
            }

            /// Signed overflow of `l + r` at `bits` width (`INT_SCARRY`'s
            /// definition: OF after a signed add), via `i128` arithmetic
            /// against the true signed range for `bits`.
            fn signed_add_overflows(l: i64, r: i64, bits: u32) -> bool {
                let (min, max) = signed_range(bits);
                let sum = sign_extend_i128(l, bits) + sign_extend_i128(r, bits);
                sum < min || sum > max
            }

            /// Signed overflow of `l - r` at `bits` width (`INT_SBORROW`'s
            /// definition: OF after a signed subtract / `cmp`).
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
                op: &$BinaryOp,
                lhs: &$Expr,
                rhs: &$Expr,
                ty: &NirType,
                env: &Env,
            ) -> Result<Value> {
                let l = eval_expr(lhs, env)?;
                let r = eval_expr(rhs, env)?;
                // Comparisons need the *operands'* width (to mask correctly
                // for unsigned comparisons); arithmetic/logical ops are
                // normalized to the *result* type `ty` after computing.
                let operand_bits = width_of(infer_ty(lhs, env)?).clamp(1, 64);

                let raw = match op {
                    $BinaryOp::Add => l.wrapping_add(r),
                    $BinaryOp::Sub => l.wrapping_sub(r),
                    $BinaryOp::Mul => l.wrapping_mul(r),
                    $BinaryOp::Div => {
                        if r == 0 {
                            bail!("interp: division by zero");
                        }
                        if is_signed(ty) {
                            l.wrapping_div(r)
                        } else {
                            (unsigned_masked(l, operand_bits) / unsigned_masked(r, operand_bits))
                                as i64
                        }
                    }
                    $BinaryOp::Mod => {
                        if r == 0 {
                            bail!("interp: modulo by zero");
                        }
                        if is_signed(ty) {
                            l.wrapping_rem(r)
                        } else {
                            (unsigned_masked(l, operand_bits) % unsigned_masked(r, operand_bits))
                                as i64
                        }
                    }
                    $BinaryOp::LogicalAnd => bool_to_raw(l != 0 && r != 0),
                    $BinaryOp::LogicalOr => bool_to_raw(l != 0 || r != 0),
                    $BinaryOp::And => l & r,
                    $BinaryOp::Or => l | r,
                    $BinaryOp::Xor => l ^ r,
                    $BinaryOp::Shl => l.wrapping_shl((r as u32) & 63),
                    $BinaryOp::Shr => {
                        (unsigned_masked(l, operand_bits) >> ((r as u32) & 63)) as i64
                    }
                    $BinaryOp::Sar => l.wrapping_shr((r as u32) & 63),
                    $BinaryOp::Eq => bool_to_raw(l == r),
                    $BinaryOp::Ne => bool_to_raw(l != r),
                    $BinaryOp::Lt => bool_to_raw(
                        unsigned_masked(l, operand_bits) < unsigned_masked(r, operand_bits),
                    ),
                    $BinaryOp::Le => bool_to_raw(
                        unsigned_masked(l, operand_bits) <= unsigned_masked(r, operand_bits),
                    ),
                    $BinaryOp::Gt => bool_to_raw(
                        unsigned_masked(l, operand_bits) > unsigned_masked(r, operand_bits),
                    ),
                    $BinaryOp::Ge => bool_to_raw(
                        unsigned_masked(l, operand_bits) >= unsigned_masked(r, operand_bits),
                    ),
                    $BinaryOp::SLt => bool_to_raw(l < r),
                    $BinaryOp::SLe => bool_to_raw(l <= r),
                    $BinaryOp::SGt => bool_to_raw(l > r),
                    $BinaryOp::SGe => bool_to_raw(l >= r),
                };
                Ok(normalize(raw, ty))
            }

            fn bool_to_raw(b: bool) -> i64 {
                if b { 1 } else { 0 }
            }

            fn exec_stmt(stmt: &$Stmt, env: &mut Env) -> Result<Flow> {
                match stmt {
                    $Stmt::Assign { lhs, rhs } => {
                        // Stores go to memory; only a plain variable falls
                        // through to the environment below.
                        if let Some((addr_expr, bits)) = store_target(lhs) {
                            let addr = eval_expr(addr_expr, env)? as u64;
                            let v = eval_expr(rhs, env)?;
                            env.mem.write(addr, bits, v);
                            return Ok(Flow::Normal);
                        }
                        let name = match lhs {
                            $LValue::Var(name) => name,
                            other => bail!(
                                "interp: unsupported assignment target {other:?}"
                            ),
                        };
                        // A real function's declared locals don't
                        // necessarily list every name its body assigns --
                        // e.g. a return-value scaffold slot can be
                        // assigned without a matching binding entry.
                        // Rather than bailing on every such case, infer
                        // the type from the RHS and register it as a
                        // fresh variable the first time it's assigned.
                        let ty = match env.types.get(name) {
                            Some(ty) => ty.clone(),
                            None => infer_ty(rhs, env)?.clone(),
                        };
                        let v = normalize(eval_expr(rhs, env)?, &ty);
                        env.types.insert(name.clone(), ty);
                        env.set(name, v);
                        Ok(Flow::Normal)
                    }
                    $Stmt::Expr(e) => {
                        eval_expr(e, env)?;
                        Ok(Flow::Normal)
                    }
                    $Stmt::VaStart { .. } => {
                        bail!("interp: variadic functions are not supported at the concrete tier")
                    }
                    $Stmt::Block(stmts) => exec_block(stmts, env),
                    $Stmt::Switch {
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
                            // `Break` exits the nearest enclosing loop
                            // *or* switch -- consumed here, same as a
                            // loop consumes its own `Break`.
                            Flow::Break => Ok(Flow::Normal),
                            other => Ok(other),
                        }
                    }
                    $Stmt::If {
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
                    $Stmt::While { cond, body } => loop {
                        if eval_expr(cond, env)? == 0 {
                            return Ok(Flow::Normal);
                        }
                        match exec_block(body, env)? {
                            Flow::Normal | Flow::Continue => continue,
                            Flow::Break => return Ok(Flow::Normal),
                            other => return Ok(other),
                        }
                    },
                    $Stmt::DoWhile { body, cond } => loop {
                        match exec_block(body, env)? {
                            Flow::Normal | Flow::Continue => {}
                            Flow::Break => return Ok(Flow::Normal),
                            other => return Ok(other),
                        }
                        if eval_expr(cond, env)? == 0 {
                            return Ok(Flow::Normal);
                        }
                    },
                    $Stmt::For {
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
                    $Stmt::Label(_) => Ok(Flow::Normal),
                    $Stmt::Goto(label) => Ok(Flow::Goto(label.clone())),
                    $Stmt::Return(expr) => match expr {
                        Some(e) => Ok(Flow::Return(Some(eval_expr(e, env)?))),
                        None => Ok(Flow::Return(None)),
                    },
                    $Stmt::Break => Ok(Flow::Break),
                    $Stmt::Continue => Ok(Flow::Continue),
                }
            }

            /// Execute a statement list, resolving any `Goto` whose target
            /// `Label` is in `stmts` itself (at any position -- forward or
            /// backward) by moving the cursor there; a `Goto` whose label
            /// isn't found here is propagated to the caller, which tries
            /// its own (enclosing) statement list next. This is what lets
            /// a label at function-body scope catch a `goto` issued from
            /// inside a deeply nested `If`/`Block`.
            fn exec_block(stmts: &[$Stmt], env: &mut Env) -> Result<Flow> {
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

            fn find_label(stmts: &[$Stmt], label: &str) -> Option<usize> {
                stmts
                    .iter()
                    .position(|s| matches!(s, $Stmt::Label(l) if l == label))
            }

            /// Interpret `body` with `args` bound to `params` in order and
            /// `locals` seeded from their initializer expression when they
            /// have one -- an uninitialized local stays unbound, so reading it
            /// before assignment is an error rather than a zero -- and return
            /// the function's return value (`None` for
            /// a bare `return;`, or if control fell off the end of the
            /// body without an explicit `Return`).
            pub fn interpret(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
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
                    mem: Memory::default(),
                };
                for (p, a) in params.iter().zip(args) {
                    env.types.insert(p.name.clone(), p.ty.clone());
                    env.values.insert(p.name.clone(), normalize(*a, &p.ty));
                }
                for l in locals {
                    env.types.insert(l.name.clone(), l.ty.clone());
                    // Only a local with an initializer gets a value. An
                    // uninitialized one stays unbound, so `Env::get` refuses a
                    // read that reaches it before any assignment -- which is
                    // this tier's whole contract: bail rather than invent.
                    //
                    // Seeding those to 0 made the interpreter answer for
                    // programs it cannot model. `mingw_get_invalid_parameter_handler`
                    // decompiles to `return tmp_140007190;` -- a global read
                    // recovered as an uninitialized temp -- and the zero made
                    // it look like a confident answer that disagreed with the
                    // machine, rather than the "cannot check this" it is. The
                    // same default could equally have made a wrong body agree.
                    if let Some(init) = &l.initializer {
                        let v = eval_expr(init, &env)?;
                        env.values.insert(l.name.clone(), v);
                    }
                }
                match exec_block(body, &mut env)? {
                    Flow::Return(v) => Ok(v),
                    Flow::Normal => Ok(None),
                    other => bail!("interp: body ended in unexpected control state {other:?}"),
                }
            }

            /// Interpret a whole `$Function`, using its own `params`/
            /// `locals`/`body` -- the usual entry point (see [`interpret`]
            /// for the lower-level, explicit-parts version
            /// `crate::diff::diff_prehir_hir` uses to let PreHIR and HIR share
            /// one differential check even though their `params`/`locals`
            /// come from independently-typed structs).
            #[allow(dead_code)]
            pub fn interpret_function(
                func: &$Function,
                args: &[i64],
            ) -> Result<Option<Value>> {
                interpret(&func.body, &func.params, &func.locals, args)
            }
        }
    };
}

define_interp!(
    prehir,
    fission_midend_prehir,
    PreHirStmt,
    PreHirExpr,
    PreHirLValue,
    PreHirBinaryOp,
    PreHirUnaryOp,
    PreHirBinding,
    PreHirFunction
);
define_interp!(
    hir,
    fission_midend_core::ir,
    HirStmt,
    HirExpr,
    HirLValue,
    HirBinaryOp,
    HirUnaryOp,
    NirBinding,
    HirFunction
);

pub use hir::interpret as interpret_hir;
pub use prehir::interpret as interpret_prehir;
