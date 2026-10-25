use fission_midend_core::ir::NirType;
use std::rc::Rc;

/// The flattened, goto/label-based statement AST that `fission-pcode`'s
/// builder emits directly from p-code, and that normalize/structuring's own
/// internal passes read and incrementally rewrite -- this is PreHIR (the
/// pre-structuring pipeline stage). Structuring performs a genuine
/// `PreHirFunction -> HirFunction` conversion once its CFG-to-AST rewrite is
/// done (see `fission_pcode::midend::orchestrate`'s call site and
/// [`crate::ir::prehir_stmts_to_hir_stmts`]); nothing upstream of that
/// conversion ever produces or touches `HirStmt`
/// (`fission_midend_core::ir::HirStmt`).
///
/// `PreHirStmt` and `HirStmt` are independently defined (not a shared type
/// wearing two names, and not one generated from the other via a macro) even
/// though their shapes start out identical -- the point of separating them
/// is that they are allowed to diverge as PreHIR's and HIR's respective needs
/// diverge (e.g. HIR-only surface-presentation concerns, or PreHIR-only
/// pre-structuring bookkeeping), and a real independent definition makes an
/// accidental PreHIR/HIR mix-up a compile error rather than two same-shaped
/// types silently type-checking while being logically backwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreHirStmt {
    Assign {
        lhs: PreHirLValue,
        rhs: PreHirExpr,
    },
    Expr(PreHirExpr),
    VaStart {
        va_list: PreHirExpr,
        last_named_param: String,
    },
    Block(Rc<Vec<PreHirStmt>>),
    Switch {
        expr: PreHirExpr,
        cases: Vec<PreHirSwitchCase>,
        default: Rc<Vec<PreHirStmt>>,
    },
    If {
        cond: PreHirExpr,
        then_body: Rc<Vec<PreHirStmt>>,
        else_body: Rc<Vec<PreHirStmt>>,
    },
    While {
        cond: PreHirExpr,
        body: Rc<Vec<PreHirStmt>>,
    },
    DoWhile {
        body: Rc<Vec<PreHirStmt>>,
        cond: PreHirExpr,
    },
    For {
        init: Option<Box<PreHirStmt>>,
        cond: Option<PreHirExpr>,
        update: Option<Box<PreHirStmt>>,
        body: Rc<Vec<PreHirStmt>>,
    },
    Label(String),
    Goto(String),
    Return(Option<PreHirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreHirSwitchCase {
    pub values: Vec<i64>,
    pub body: Rc<Vec<PreHirStmt>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreHirLValue {
    Var(String),
    Deref {
        ptr: Box<PreHirExpr>,
        ty: NirType,
    },
    Index {
        base: Box<PreHirExpr>,
        index: Box<PreHirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<PreHirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
}

#[derive(Clone, Eq)]
pub enum PreHirExpr {
    Var(String),
    AddressOfGlobal(String),
    AddressOfLocal(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<PreHirExpr>,
    },
    Unary {
        op: PreHirUnaryOp,
        expr: Box<PreHirExpr>,
        ty: NirType,
    },
    Binary {
        op: PreHirBinaryOp,
        lhs: Box<PreHirExpr>,
        rhs: Box<PreHirExpr>,
        ty: NirType,
    },
    Select {
        cond: Box<PreHirExpr>,
        then_expr: Box<PreHirExpr>,
        else_expr: Box<PreHirExpr>,
        ty: NirType,
    },
    Call {
        /// Source operation; excluded from semantic expression equality.
        origin: Option<fission_midend_core::ir::CallOrigin>,
        target: String,
        args: Vec<PreHirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<PreHirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<PreHirExpr>,
        offset: i64,
    },
    Index {
        base: Box<PreHirExpr>,
        index: Box<PreHirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<PreHirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
    AggregateCopy {
        src: Box<PreHirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreHirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreHirBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LogicalAnd,
    LogicalOr,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    SLt,
    SLe,
    SGt,
    SGe,
}

// Provenance must not change folding, CSE, diagnostic keys or expression identity.
impl PartialEq for PreHirExpr {
    fn eq(&self, other: &Self) -> bool {
        match self {
            Self::Var(a0) => matches!(other, Self::Var(b0) if a0 == b0),
            Self::AddressOfGlobal(a0) => matches!(other, Self::AddressOfGlobal(b0) if a0 == b0),
            Self::AddressOfLocal(a0) => matches!(other, Self::AddressOfLocal(b0) if a0 == b0),
            Self::Const(a0, a1) => matches!(other, Self::Const(b0, b1) if a0 == b0 && a1 == b1),
            Self::Cast { ty: a0, expr: a1 } => {
                matches!(other, Self::Cast { ty: b0, expr: b1 } if a0 == b0 && a1 == b1)
            }
            Self::Unary {
                op: a0,
                expr: a1,
                ty: a2,
            } => {
                matches!(other, Self::Unary { op: b0, expr: b1, ty: b2 } if a0 == b0 && a1 == b1 && a2 == b2)
            }
            Self::Binary {
                op: a0,
                lhs: a1,
                rhs: a2,
                ty: a3,
            } => {
                matches!(other, Self::Binary { op: b0, lhs: b1, rhs: b2, ty: b3 } if a0 == b0 && a1 == b1 && a2 == b2 && a3 == b3)
            }
            Self::Select {
                cond: a0,
                then_expr: a1,
                else_expr: a2,
                ty: a3,
            } => {
                matches!(other, Self::Select { cond: b0, then_expr: b1, else_expr: b2, ty: b3 } if a0 == b0 && a1 == b1 && a2 == b2 && a3 == b3)
            }
            Self::Call {
                target: a0,
                args: a1,
                ty: a2,
                ..
            } => {
                matches!(other, Self::Call { target: b0, args: b1, ty: b2, .. } if a0 == b0 && a1 == b1 && a2 == b2)
            }
            Self::Load { ptr: a0, ty: a1 } => {
                matches!(other, Self::Load { ptr: b0, ty: b1 } if a0 == b0 && a1 == b1)
            }
            Self::PtrOffset {
                base: a0,
                offset: a1,
            } => matches!(other, Self::PtrOffset { base: b0, offset: b1 } if a0 == b0 && a1 == b1),
            Self::Index {
                base: a0,
                index: a1,
                elem_ty: a2,
            } => {
                matches!(other, Self::Index { base: b0, index: b1, elem_ty: b2 } if a0 == b0 && a1 == b1 && a2 == b2)
            }
            Self::FieldAccess {
                base: a0,
                field_name: a1,
                offset: a2,
                ty: a3,
            } => {
                matches!(other, Self::FieldAccess { base: b0, field_name: b1, offset: b2, ty: b3 } if a0 == b0 && a1 == b1 && a2 == b2 && a3 == b3)
            }
            Self::AggregateCopy { src: a0, size: a1 } => {
                matches!(other, Self::AggregateCopy { src: b0, size: b1 } if a0 == b0 && a1 == b1)
            }
        }
    }
}
impl std::fmt::Debug for PreHirExpr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Var(a0) => f.debug_tuple("Var").field(a0).finish(),
            Self::AddressOfGlobal(a0) => f.debug_tuple("AddressOfGlobal").field(a0).finish(),
            Self::AddressOfLocal(a0) => f.debug_tuple("AddressOfLocal").field(a0).finish(),
            Self::Const(a0, a1) => f.debug_tuple("Const").field(a0).field(a1).finish(),
            Self::Cast { ty, expr } => f
                .debug_struct("Cast")
                .field("ty", ty)
                .field("expr", expr)
                .finish(),
            Self::Unary { op, expr, ty } => f
                .debug_struct("Unary")
                .field("op", op)
                .field("expr", expr)
                .field("ty", ty)
                .finish(),
            Self::Binary { op, lhs, rhs, ty } => f
                .debug_struct("Binary")
                .field("op", op)
                .field("lhs", lhs)
                .field("rhs", rhs)
                .field("ty", ty)
                .finish(),
            Self::Select {
                cond,
                then_expr,
                else_expr,
                ty,
            } => f
                .debug_struct("Select")
                .field("cond", cond)
                .field("then_expr", then_expr)
                .field("else_expr", else_expr)
                .field("ty", ty)
                .finish(),
            Self::Call {
                target, args, ty, ..
            } => f
                .debug_struct("Call")
                .field("target", target)
                .field("args", args)
                .field("ty", ty)
                .finish(),
            Self::Load { ptr, ty } => f
                .debug_struct("Load")
                .field("ptr", ptr)
                .field("ty", ty)
                .finish(),
            Self::PtrOffset { base, offset } => f
                .debug_struct("PtrOffset")
                .field("base", base)
                .field("offset", offset)
                .finish(),
            Self::Index {
                base,
                index,
                elem_ty,
            } => f
                .debug_struct("Index")
                .field("base", base)
                .field("index", index)
                .field("elem_ty", elem_ty)
                .finish(),
            Self::FieldAccess {
                base,
                field_name,
                offset,
                ty,
            } => f
                .debug_struct("FieldAccess")
                .field("base", base)
                .field("field_name", field_name)
                .field("offset", offset)
                .field("ty", ty)
                .finish(),
            Self::AggregateCopy { src, size } => f
                .debug_struct("AggregateCopy")
                .field("src", src)
                .field("size", size)
                .finish(),
        }
    }
}
