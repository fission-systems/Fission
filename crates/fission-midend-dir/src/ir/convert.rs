use crate::ir::{DirBinaryOp, DirExpr, DirLValue, DirStmt, DirSwitchCase, DirUnaryOp};
use fission_midend_core::ir::{
    HirBinaryOp, HirExpr, HirLValue, HirStmt, HirSwitchCase, HirUnaryOp,
};

/// Structural 1:1 conversion from the pre-structuring `DirStmt`/`DirExpr`
/// grammar to the final `HirStmt`/`HirExpr` grammar -- every `Dir*`
/// variant maps to the identically-shaped `Hir*` variant. Called exactly
/// once per decompile, in `fission_pcode::midend::orchestrate`'s
/// `render_mlil_preview_with_binary_and_context`, immediately after
/// structuring's CFG-to-AST rewrite (`run_structuring_pipeline`) and the
/// post-structuring `eliminate_redundant_var_assigns` cleanup finish. This
/// is the real `DirFunction -> HirFunction` boundary: not a redesign of
/// what structuring computes, just an explicit type change at the point the
/// boundary already exists in the pipeline.
pub fn dir_stmts_to_hir_stmts(stmts: Vec<DirStmt>) -> Vec<HirStmt> {
    stmts.into_iter().map(dir_stmt_to_hir_stmt).collect()
}

pub fn dir_stmt_to_hir_stmt(stmt: DirStmt) -> HirStmt {
    match stmt {
        DirStmt::Assign { lhs, rhs } => HirStmt::Assign {
            lhs: dir_lvalue_to_hir_lvalue(lhs),
            rhs: dir_expr_to_hir_expr(rhs),
        },
        DirStmt::Expr(e) => HirStmt::Expr(dir_expr_to_hir_expr(e)),
        DirStmt::VaStart {
            va_list,
            last_named_param,
        } => HirStmt::VaStart {
            va_list: dir_expr_to_hir_expr(va_list),
            last_named_param,
        },
        DirStmt::Block(stmts) => HirStmt::Block(dir_stmts_to_hir_stmts(stmts)),
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => HirStmt::Switch {
            expr: dir_expr_to_hir_expr(expr),
            cases: cases
                .into_iter()
                .map(|c| HirSwitchCase {
                    values: c.values,
                    body: dir_stmts_to_hir_stmts(c.body),
                })
                .collect(),
            default: dir_stmts_to_hir_stmts(default),
        },
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => HirStmt::If {
            cond: dir_expr_to_hir_expr(cond),
            then_body: dir_stmts_to_hir_stmts(then_body),
            else_body: dir_stmts_to_hir_stmts(else_body),
        },
        DirStmt::While { cond, body } => HirStmt::While {
            cond: dir_expr_to_hir_expr(cond),
            body: dir_stmts_to_hir_stmts(body),
        },
        DirStmt::DoWhile { body, cond } => HirStmt::DoWhile {
            body: dir_stmts_to_hir_stmts(body),
            cond: dir_expr_to_hir_expr(cond),
        },
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => HirStmt::For {
            init: init.map(|s| Box::new(dir_stmt_to_hir_stmt(*s))),
            cond: cond.map(dir_expr_to_hir_expr),
            update: update.map(|s| Box::new(dir_stmt_to_hir_stmt(*s))),
            body: dir_stmts_to_hir_stmts(body),
        },
        DirStmt::Label(l) => HirStmt::Label(l),
        DirStmt::Goto(l) => HirStmt::Goto(l),
        DirStmt::Return(e) => HirStmt::Return(e.map(dir_expr_to_hir_expr)),
        DirStmt::Break => HirStmt::Break,
        DirStmt::Continue => HirStmt::Continue,
    }
}

pub fn dir_lvalue_to_hir_lvalue(lv: DirLValue) -> HirLValue {
    match lv {
        DirLValue::Var(name) => HirLValue::Var(name),
        DirLValue::Deref { ptr, ty } => HirLValue::Deref {
            ptr: Box::new(dir_expr_to_hir_expr(*ptr)),
            ty,
        },
        DirLValue::Index {
            base,
            index,
            elem_ty,
        } => HirLValue::Index {
            base: Box::new(dir_expr_to_hir_expr(*base)),
            index: Box::new(dir_expr_to_hir_expr(*index)),
            elem_ty,
        },
        DirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => HirLValue::FieldAccess {
            base: Box::new(dir_expr_to_hir_expr(*base)),
            field_name,
            offset,
            ty,
        },
    }
}

pub fn dir_expr_to_hir_expr(expr: DirExpr) -> HirExpr {
    match expr {
        DirExpr::Var(name) => HirExpr::Var(name),
        DirExpr::AddressOfGlobal(name) => HirExpr::AddressOfGlobal(name),
        DirExpr::Const(v, ty) => HirExpr::Const(v, ty),
        DirExpr::Cast { ty, expr } => HirExpr::Cast {
            ty,
            expr: Box::new(dir_expr_to_hir_expr(*expr)),
        },
        DirExpr::Unary { op, expr, ty } => HirExpr::Unary {
            op: dir_unary_op_to_hir_unary_op(op),
            expr: Box::new(dir_expr_to_hir_expr(*expr)),
            ty,
        },
        DirExpr::Binary { op, lhs, rhs, ty } => HirExpr::Binary {
            op: dir_binary_op_to_hir_binary_op(op),
            lhs: Box::new(dir_expr_to_hir_expr(*lhs)),
            rhs: Box::new(dir_expr_to_hir_expr(*rhs)),
            ty,
        },
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => HirExpr::Select {
            cond: Box::new(dir_expr_to_hir_expr(*cond)),
            then_expr: Box::new(dir_expr_to_hir_expr(*then_expr)),
            else_expr: Box::new(dir_expr_to_hir_expr(*else_expr)),
            ty,
        },
        DirExpr::Call { target, args, ty } => HirExpr::Call {
            target,
            args: args.into_iter().map(dir_expr_to_hir_expr).collect(),
            ty,
        },
        DirExpr::Load { ptr, ty } => HirExpr::Load {
            ptr: Box::new(dir_expr_to_hir_expr(*ptr)),
            ty,
        },
        DirExpr::PtrOffset { base, offset } => HirExpr::PtrOffset {
            base: Box::new(dir_expr_to_hir_expr(*base)),
            offset,
        },
        DirExpr::Index {
            base,
            index,
            elem_ty,
        } => HirExpr::Index {
            base: Box::new(dir_expr_to_hir_expr(*base)),
            index: Box::new(dir_expr_to_hir_expr(*index)),
            elem_ty,
        },
        DirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => HirExpr::FieldAccess {
            base: Box::new(dir_expr_to_hir_expr(*base)),
            field_name,
            offset,
            ty,
        },
        DirExpr::AggregateCopy { src, size } => HirExpr::AggregateCopy {
            src: Box::new(dir_expr_to_hir_expr(*src)),
            size,
        },
    }
}

fn dir_unary_op_to_hir_unary_op(op: DirUnaryOp) -> HirUnaryOp {
    match op {
        DirUnaryOp::Neg => HirUnaryOp::Neg,
        DirUnaryOp::Not => HirUnaryOp::Not,
        DirUnaryOp::BitNot => HirUnaryOp::BitNot,
    }
}

fn dir_binary_op_to_hir_binary_op(op: DirBinaryOp) -> HirBinaryOp {
    match op {
        DirBinaryOp::Add => HirBinaryOp::Add,
        DirBinaryOp::Sub => HirBinaryOp::Sub,
        DirBinaryOp::Mul => HirBinaryOp::Mul,
        DirBinaryOp::Div => HirBinaryOp::Div,
        DirBinaryOp::Mod => HirBinaryOp::Mod,
        DirBinaryOp::LogicalAnd => HirBinaryOp::LogicalAnd,
        DirBinaryOp::LogicalOr => HirBinaryOp::LogicalOr,
        DirBinaryOp::And => HirBinaryOp::And,
        DirBinaryOp::Or => HirBinaryOp::Or,
        DirBinaryOp::Xor => HirBinaryOp::Xor,
        DirBinaryOp::Shl => HirBinaryOp::Shl,
        DirBinaryOp::Shr => HirBinaryOp::Shr,
        DirBinaryOp::Sar => HirBinaryOp::Sar,
        DirBinaryOp::Eq => HirBinaryOp::Eq,
        DirBinaryOp::Ne => HirBinaryOp::Ne,
        DirBinaryOp::Lt => HirBinaryOp::Lt,
        DirBinaryOp::Le => HirBinaryOp::Le,
        DirBinaryOp::Gt => HirBinaryOp::Gt,
        DirBinaryOp::Ge => HirBinaryOp::Ge,
        DirBinaryOp::SLt => HirBinaryOp::SLt,
        DirBinaryOp::SLe => HirBinaryOp::SLe,
        DirBinaryOp::SGt => HirBinaryOp::SGt,
        DirBinaryOp::SGe => HirBinaryOp::SGe,
    }
}

pub fn hir_stmts_to_dir_stmts(stmts: Vec<HirStmt>) -> Vec<DirStmt> {
    stmts.into_iter().map(hir_stmt_to_dir_stmt).collect()
}

pub fn hir_stmt_to_dir_stmt(stmt: HirStmt) -> DirStmt {
    match stmt {
        HirStmt::Assign { lhs, rhs } => DirStmt::Assign {
            lhs: hir_lvalue_to_dir_lvalue(lhs),
            rhs: hir_expr_to_dir_expr(rhs),
        },
        HirStmt::Expr(e) => DirStmt::Expr(hir_expr_to_dir_expr(e)),
        HirStmt::VaStart {
            va_list,
            last_named_param,
        } => DirStmt::VaStart {
            va_list: hir_expr_to_dir_expr(va_list),
            last_named_param,
        },
        HirStmt::Block(stmts) => DirStmt::Block(hir_stmts_to_dir_stmts(stmts)),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => DirStmt::Switch {
            expr: hir_expr_to_dir_expr(expr),
            cases: cases
                .into_iter()
                .map(|c| DirSwitchCase {
                    values: c.values,
                    body: hir_stmts_to_dir_stmts(c.body),
                })
                .collect(),
            default: hir_stmts_to_dir_stmts(default),
        },
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => DirStmt::If {
            cond: hir_expr_to_dir_expr(cond),
            then_body: hir_stmts_to_dir_stmts(then_body),
            else_body: hir_stmts_to_dir_stmts(else_body),
        },
        HirStmt::While { cond, body } => DirStmt::While {
            cond: hir_expr_to_dir_expr(cond),
            body: hir_stmts_to_dir_stmts(body),
        },
        HirStmt::DoWhile { body, cond } => DirStmt::DoWhile {
            body: hir_stmts_to_dir_stmts(body),
            cond: hir_expr_to_dir_expr(cond),
        },
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => DirStmt::For {
            init: init.map(|s| Box::new(hir_stmt_to_dir_stmt(*s))),
            cond: cond.map(hir_expr_to_dir_expr),
            update: update.map(|s| Box::new(hir_stmt_to_dir_stmt(*s))),
            body: hir_stmts_to_dir_stmts(body),
        },
        HirStmt::Label(l) => DirStmt::Label(l),
        HirStmt::Goto(l) => DirStmt::Goto(l),
        HirStmt::Return(e) => DirStmt::Return(e.map(hir_expr_to_dir_expr)),
        HirStmt::Break => DirStmt::Break,
        HirStmt::Continue => DirStmt::Continue,
    }
}

pub fn hir_lvalue_to_dir_lvalue(lv: HirLValue) -> DirLValue {
    match lv {
        HirLValue::Var(name) => DirLValue::Var(name),
        HirLValue::Deref { ptr, ty } => DirLValue::Deref {
            ptr: Box::new(hir_expr_to_dir_expr(*ptr)),
            ty,
        },
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => DirLValue::Index {
            base: Box::new(hir_expr_to_dir_expr(*base)),
            index: Box::new(hir_expr_to_dir_expr(*index)),
            elem_ty,
        },
        HirLValue::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => DirLValue::FieldAccess {
            base: Box::new(hir_expr_to_dir_expr(*base)),
            field_name,
            offset,
            ty,
        },
    }
}

pub fn hir_expr_to_dir_expr(expr: HirExpr) -> DirExpr {
    match expr {
        HirExpr::Var(name) => DirExpr::Var(name),
        HirExpr::AddressOfGlobal(name) => DirExpr::AddressOfGlobal(name),
        HirExpr::Const(v, ty) => DirExpr::Const(v, ty),
        HirExpr::Cast { ty, expr } => DirExpr::Cast {
            ty,
            expr: Box::new(hir_expr_to_dir_expr(*expr)),
        },
        HirExpr::Unary { op, expr, ty } => DirExpr::Unary {
            op: hir_unary_op_to_dir_unary_op(op),
            expr: Box::new(hir_expr_to_dir_expr(*expr)),
            ty,
        },
        HirExpr::Binary { op, lhs, rhs, ty } => DirExpr::Binary {
            op: hir_binary_op_to_dir_binary_op(op),
            lhs: Box::new(hir_expr_to_dir_expr(*lhs)),
            rhs: Box::new(hir_expr_to_dir_expr(*rhs)),
            ty,
        },
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => DirExpr::Select {
            cond: Box::new(hir_expr_to_dir_expr(*cond)),
            then_expr: Box::new(hir_expr_to_dir_expr(*then_expr)),
            else_expr: Box::new(hir_expr_to_dir_expr(*else_expr)),
            ty,
        },
        HirExpr::Call { target, args, ty } => DirExpr::Call {
            target,
            args: args.into_iter().map(hir_expr_to_dir_expr).collect(),
            ty,
        },
        HirExpr::Load { ptr, ty } => DirExpr::Load {
            ptr: Box::new(hir_expr_to_dir_expr(*ptr)),
            ty,
        },
        HirExpr::PtrOffset { base, offset } => DirExpr::PtrOffset {
            base: Box::new(hir_expr_to_dir_expr(*base)),
            offset,
        },
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => DirExpr::Index {
            base: Box::new(hir_expr_to_dir_expr(*base)),
            index: Box::new(hir_expr_to_dir_expr(*index)),
            elem_ty,
        },
        HirExpr::FieldAccess {
            base,
            field_name,
            offset,
            ty,
        } => DirExpr::FieldAccess {
            base: Box::new(hir_expr_to_dir_expr(*base)),
            field_name,
            offset,
            ty,
        },
        HirExpr::AggregateCopy { src, size } => DirExpr::AggregateCopy {
            src: Box::new(hir_expr_to_dir_expr(*src)),
            size,
        },
    }
}

fn hir_unary_op_to_dir_unary_op(op: HirUnaryOp) -> DirUnaryOp {
    match op {
        HirUnaryOp::Neg => DirUnaryOp::Neg,
        HirUnaryOp::Not => DirUnaryOp::Not,
        HirUnaryOp::BitNot => DirUnaryOp::BitNot,
    }
}

fn hir_binary_op_to_dir_binary_op(op: HirBinaryOp) -> DirBinaryOp {
    match op {
        HirBinaryOp::Add => DirBinaryOp::Add,
        HirBinaryOp::Sub => DirBinaryOp::Sub,
        HirBinaryOp::Mul => DirBinaryOp::Mul,
        HirBinaryOp::Div => DirBinaryOp::Div,
        HirBinaryOp::Mod => DirBinaryOp::Mod,
        HirBinaryOp::LogicalAnd => DirBinaryOp::LogicalAnd,
        HirBinaryOp::LogicalOr => DirBinaryOp::LogicalOr,
        HirBinaryOp::And => DirBinaryOp::And,
        HirBinaryOp::Or => DirBinaryOp::Or,
        HirBinaryOp::Xor => DirBinaryOp::Xor,
        HirBinaryOp::Shl => DirBinaryOp::Shl,
        HirBinaryOp::Shr => DirBinaryOp::Shr,
        HirBinaryOp::Sar => DirBinaryOp::Sar,
        HirBinaryOp::Eq => DirBinaryOp::Eq,
        HirBinaryOp::Ne => DirBinaryOp::Ne,
        HirBinaryOp::Lt => DirBinaryOp::Lt,
        HirBinaryOp::Le => DirBinaryOp::Le,
        HirBinaryOp::Gt => DirBinaryOp::Gt,
        HirBinaryOp::Ge => DirBinaryOp::Ge,
        HirBinaryOp::SLt => DirBinaryOp::SLt,
        HirBinaryOp::SLe => DirBinaryOp::SLe,
        HirBinaryOp::SGt => DirBinaryOp::SGt,
        HirBinaryOp::SGe => DirBinaryOp::SGe,
    }
}
