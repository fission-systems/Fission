use crate::ir::{
    PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt, PreHirSwitchCase, PreHirUnaryOp,
};
use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirLValue, HirStmt, HirSwitchCase, HirUnaryOp,
};

/// Structural 1:1 conversion from the pre-structuring `PreHirStmt`/`PreHirExpr`
/// grammar to the final `HirStmt`/`HirExpr` grammar -- every `PreHir*`
/// variant maps to the identically-shaped `Hir*` variant. Called exactly
/// once per decompile, in `fission_pcode::midend::orchestrate`'s
/// `render_mlil_preview_with_binary_and_context`, immediately after
/// structuring's CFG-to-AST rewrite (`run_structuring_pipeline`) and the
/// post-structuring `eliminate_redundant_var_assigns` cleanup finish. This
/// is the real `PreHirFunction -> HirFunction` boundary: not a redesign of
/// what structuring computes, just an explicit type change at the point the
/// boundary already exists in the pipeline.
pub fn prehir_stmts_to_hir_stmts(stmts: Vec<PreHirStmt>) -> Vec<HirStmt> {
    stmts.into_iter().map(prehir_stmt_to_hir_stmt).collect()
}

pub fn prehir_stmt_to_hir_stmt(stmt: PreHirStmt) -> HirStmt {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => HirStmt::Assign {
            lhs: prehir_lvalue_to_hir_lvalue(lhs),
            rhs: prehir_expr_to_hir_expr(rhs),
        },
        PreHirStmt::Expr(e) => HirStmt::Expr(prehir_expr_to_hir_expr(e)),
        PreHirStmt::VaStart {
            va_list,
            last_named_param,
        } => HirStmt::VaStart {
            va_list: prehir_expr_to_hir_expr(va_list),
            last_named_param,
        },
        PreHirStmt::Block(stmts) => HirStmt::Block(prehir_stmts_to_hir_stmts(
            std::rc::Rc::unwrap_or_clone(stmts),
        )),
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => HirStmt::Switch {
            expr: prehir_expr_to_hir_expr(expr),
            cases: cases
                .into_iter()
                .map(|c| HirSwitchCase {
                    values: c.values,
                    body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(c.body)),
                })
                .collect(),
            default: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(default)),
        },
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => HirStmt::If {
            cond: prehir_expr_to_hir_expr(cond),
            then_body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(then_body)),
            else_body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(else_body)),
        },
        PreHirStmt::While { cond, body } => HirStmt::While {
            cond: prehir_expr_to_hir_expr(cond),
            body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(body)),
        },
        PreHirStmt::DoWhile { body, cond } => HirStmt::DoWhile {
            body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(body)),
            cond: prehir_expr_to_hir_expr(cond),
        },
        PreHirStmt::For {
            init,
            cond,
            update,
            body,
        } => HirStmt::For {
            init: init.map(|s| Box::new(prehir_stmt_to_hir_stmt(*s))),
            cond: cond.map(prehir_expr_to_hir_expr),
            update: update.map(|s| Box::new(prehir_stmt_to_hir_stmt(*s))),
            body: prehir_stmts_to_hir_stmts(std::rc::Rc::unwrap_or_clone(body)),
        },
        PreHirStmt::Label(l) => HirStmt::Label(l),
        PreHirStmt::Goto(l) => HirStmt::Goto(l),
        PreHirStmt::Return(e) => HirStmt::Return(e.map(prehir_expr_to_hir_expr)),
        PreHirStmt::Break => HirStmt::Break,
        PreHirStmt::Continue => HirStmt::Continue,
    }
}

pub fn prehir_lvalue_to_hir_lvalue(lv: PreHirLValue) -> HirLValue {
    match lv {
        PreHirLValue::Var(name) => HirLValue::Var(name),
        PreHirLValue::Deref { ptr, ty } => HirLValue::Deref {
            ptr: Box::new(prehir_expr_to_hir_expr(*ptr)),
            ty,
        },
        PreHirLValue::Index {
            base,
            index,
            elem_ty,
        } => HirLValue::Index {
            base: Box::new(prehir_expr_to_hir_expr(*base)),
            index: Box::new(prehir_expr_to_hir_expr(*index)),
            elem_ty,
        },
        PreHirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => HirLValue::FieldAccess {
            base: Box::new(prehir_expr_to_hir_expr(*base)),
            field_name,
            offset,
            ty,
        },
    }
}

pub fn prehir_expr_to_hir_expr(expr: PreHirExpr) -> HirExpr {
    match expr {
        PreHirExpr::Var(name) => HirExpr::Var(name),
        PreHirExpr::AddressOfGlobal(name) => HirExpr::AddressOfGlobal(name),
        PreHirExpr::Const(v, ty) => HirExpr::Const(v, ty),
        PreHirExpr::Cast { ty, expr } => HirExpr::Cast {
            ty,
            expr: Box::new(prehir_expr_to_hir_expr(*expr)),
        },
        PreHirExpr::Unary { op, expr, ty } => HirExpr::Unary {
            op: prehir_unary_op_to_hir_unary_op(op),
            expr: Box::new(prehir_expr_to_hir_expr(*expr)),
            ty,
        },
        PreHirExpr::Binary { op, lhs, rhs, ty } => HirExpr::Binary {
            op: prehir_binary_op_to_hir_binary_op(op),
            lhs: Box::new(prehir_expr_to_hir_expr(*lhs)),
            rhs: Box::new(prehir_expr_to_hir_expr(*rhs)),
            ty,
        },
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => HirExpr::Select {
            cond: Box::new(prehir_expr_to_hir_expr(*cond)),
            then_expr: Box::new(prehir_expr_to_hir_expr(*then_expr)),
            else_expr: Box::new(prehir_expr_to_hir_expr(*else_expr)),
            ty,
        },
        PreHirExpr::Call { target, args, ty } => HirExpr::Call {
            target,
            args: args.into_iter().map(prehir_expr_to_hir_expr).collect(),
            ty,
        },
        PreHirExpr::Load { ptr, ty } => HirExpr::Load {
            ptr: Box::new(prehir_expr_to_hir_expr(*ptr)),
            ty,
        },
        PreHirExpr::PtrOffset { base, offset } => HirExpr::PtrOffset {
            base: Box::new(prehir_expr_to_hir_expr(*base)),
            offset,
        },
        PreHirExpr::Index {
            base,
            index,
            elem_ty,
        } => HirExpr::Index {
            base: Box::new(prehir_expr_to_hir_expr(*base)),
            index: Box::new(prehir_expr_to_hir_expr(*index)),
            elem_ty,
        },
        PreHirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => HirExpr::FieldAccess {
            base: Box::new(prehir_expr_to_hir_expr(*base)),
            field_name,
            offset,
            ty,
        },
        PreHirExpr::AggregateCopy { src, size } => HirExpr::AggregateCopy {
            src: Box::new(prehir_expr_to_hir_expr(*src)),
            size,
        },
    }
}

fn prehir_unary_op_to_hir_unary_op(op: PreHirUnaryOp) -> HirUnaryOp {
    match op {
        PreHirUnaryOp::Neg => HirUnaryOp::Neg,
        PreHirUnaryOp::Not => HirUnaryOp::Not,
        PreHirUnaryOp::BitNot => HirUnaryOp::BitNot,
    }
}

fn prehir_binary_op_to_hir_binary_op(op: PreHirBinaryOp) -> HirBinaryOp {
    match op {
        PreHirBinaryOp::Add => HirBinaryOp::Add,
        PreHirBinaryOp::Sub => HirBinaryOp::Sub,
        PreHirBinaryOp::Mul => HirBinaryOp::Mul,
        PreHirBinaryOp::Div => HirBinaryOp::Div,
        PreHirBinaryOp::Mod => HirBinaryOp::Mod,
        PreHirBinaryOp::LogicalAnd => HirBinaryOp::LogicalAnd,
        PreHirBinaryOp::LogicalOr => HirBinaryOp::LogicalOr,
        PreHirBinaryOp::And => HirBinaryOp::And,
        PreHirBinaryOp::Or => HirBinaryOp::Or,
        PreHirBinaryOp::Xor => HirBinaryOp::Xor,
        PreHirBinaryOp::Shl => HirBinaryOp::Shl,
        PreHirBinaryOp::Shr => HirBinaryOp::Shr,
        PreHirBinaryOp::Sar => HirBinaryOp::Sar,
        PreHirBinaryOp::Eq => HirBinaryOp::Eq,
        PreHirBinaryOp::Ne => HirBinaryOp::Ne,
        PreHirBinaryOp::Lt => HirBinaryOp::Lt,
        PreHirBinaryOp::Le => HirBinaryOp::Le,
        PreHirBinaryOp::Gt => HirBinaryOp::Gt,
        PreHirBinaryOp::Ge => HirBinaryOp::Ge,
        PreHirBinaryOp::SLt => HirBinaryOp::SLt,
        PreHirBinaryOp::SLe => HirBinaryOp::SLe,
        PreHirBinaryOp::SGt => HirBinaryOp::SGt,
        PreHirBinaryOp::SGe => HirBinaryOp::SGe,
    }
}

pub fn hir_stmts_to_prehir_stmts(stmts: Vec<HirStmt>) -> Vec<PreHirStmt> {
    stmts.into_iter().map(hir_stmt_to_prehir_stmt).collect()
}

pub fn hir_stmt_to_prehir_stmt(stmt: HirStmt) -> PreHirStmt {
    match stmt {
        HirStmt::Assign { lhs, rhs } => PreHirStmt::Assign {
            lhs: hir_lvalue_to_prehir_lvalue(lhs),
            rhs: hir_expr_to_prehir_expr(rhs),
        },
        HirStmt::Expr(e) => PreHirStmt::Expr(hir_expr_to_prehir_expr(e)),
        HirStmt::VaStart {
            va_list,
            last_named_param,
        } => PreHirStmt::VaStart {
            va_list: hir_expr_to_prehir_expr(va_list),
            last_named_param,
        },
        HirStmt::Block(stmts) => {
            PreHirStmt::Block(std::rc::Rc::new(hir_stmts_to_prehir_stmts(stmts)))
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => PreHirStmt::Switch {
            expr: hir_expr_to_prehir_expr(expr),
            cases: cases
                .into_iter()
                .map(|c| PreHirSwitchCase {
                    values: c.values,
                    body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(c.body)),
                })
                .collect(),
            default: std::rc::Rc::new(hir_stmts_to_prehir_stmts(default)),
        },
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => PreHirStmt::If {
            cond: hir_expr_to_prehir_expr(cond),
            then_body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(then_body)),
            else_body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(else_body)),
        },
        HirStmt::While { cond, body } => PreHirStmt::While {
            cond: hir_expr_to_prehir_expr(cond),
            body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(body)),
        },
        HirStmt::DoWhile { body, cond } => PreHirStmt::DoWhile {
            body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(body)),
            cond: hir_expr_to_prehir_expr(cond),
        },
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => PreHirStmt::For {
            init: init.map(|s| Box::new(hir_stmt_to_prehir_stmt(*s))),
            cond: cond.map(hir_expr_to_prehir_expr),
            update: update.map(|s| Box::new(hir_stmt_to_prehir_stmt(*s))),
            body: std::rc::Rc::new(hir_stmts_to_prehir_stmts(body)),
        },
        HirStmt::Label(l) => PreHirStmt::Label(l),
        HirStmt::Goto(l) => PreHirStmt::Goto(l),
        HirStmt::Return(e) => PreHirStmt::Return(e.map(hir_expr_to_prehir_expr)),
        HirStmt::Break => PreHirStmt::Break,
        HirStmt::Continue => PreHirStmt::Continue,
    }
}

pub fn hir_lvalue_to_prehir_lvalue(lv: HirLValue) -> PreHirLValue {
    match lv {
        HirLValue::Var(name) => PreHirLValue::Var(name),
        HirLValue::Deref { ptr, ty } => PreHirLValue::Deref {
            ptr: Box::new(hir_expr_to_prehir_expr(*ptr)),
            ty,
        },
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => PreHirLValue::Index {
            base: Box::new(hir_expr_to_prehir_expr(*base)),
            index: Box::new(hir_expr_to_prehir_expr(*index)),
            elem_ty,
        },
        HirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => PreHirLValue::FieldAccess {
            base: Box::new(hir_expr_to_prehir_expr(*base)),
            field_name,
            offset,
            ty,
        },
    }
}

pub fn hir_expr_to_prehir_expr(expr: HirExpr) -> PreHirExpr {
    match expr {
        HirExpr::Var(name) => PreHirExpr::Var(name),
        HirExpr::AddressOfGlobal(name) => PreHirExpr::AddressOfGlobal(name),
        HirExpr::Const(v, ty) => PreHirExpr::Const(v, ty),
        HirExpr::Cast { ty, expr } => PreHirExpr::Cast {
            ty,
            expr: Box::new(hir_expr_to_prehir_expr(*expr)),
        },
        HirExpr::Unary { op, expr, ty } => PreHirExpr::Unary {
            op: hir_unary_op_to_prehir_unary_op(op),
            expr: Box::new(hir_expr_to_prehir_expr(*expr)),
            ty,
        },
        HirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: hir_binary_op_to_prehir_binary_op(op),
            lhs: Box::new(hir_expr_to_prehir_expr(*lhs)),
            rhs: Box::new(hir_expr_to_prehir_expr(*rhs)),
            ty,
        },
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => PreHirExpr::Select {
            cond: Box::new(hir_expr_to_prehir_expr(*cond)),
            then_expr: Box::new(hir_expr_to_prehir_expr(*then_expr)),
            else_expr: Box::new(hir_expr_to_prehir_expr(*else_expr)),
            ty,
        },
        HirExpr::Call { target, args, ty } => PreHirExpr::Call {
            target,
            args: args.into_iter().map(hir_expr_to_prehir_expr).collect(),
            ty,
        },
        HirExpr::Load { ptr, ty } => PreHirExpr::Load {
            ptr: Box::new(hir_expr_to_prehir_expr(*ptr)),
            ty,
        },
        HirExpr::PtrOffset { base, offset } => PreHirExpr::PtrOffset {
            base: Box::new(hir_expr_to_prehir_expr(*base)),
            offset,
        },
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => PreHirExpr::Index {
            base: Box::new(hir_expr_to_prehir_expr(*base)),
            index: Box::new(hir_expr_to_prehir_expr(*index)),
            elem_ty,
        },
        HirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => PreHirExpr::FieldAccess {
            base: Box::new(hir_expr_to_prehir_expr(*base)),
            field_name,
            offset,
            ty,
        },
        HirExpr::AggregateCopy { src, size } => PreHirExpr::AggregateCopy {
            src: Box::new(hir_expr_to_prehir_expr(*src)),
            size,
        },
    }
}

fn hir_unary_op_to_prehir_unary_op(op: HirUnaryOp) -> PreHirUnaryOp {
    match op {
        HirUnaryOp::Neg => PreHirUnaryOp::Neg,
        HirUnaryOp::Not => PreHirUnaryOp::Not,
        HirUnaryOp::BitNot => PreHirUnaryOp::BitNot,
    }
}

fn hir_binary_op_to_prehir_binary_op(op: HirBinaryOp) -> PreHirBinaryOp {
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
