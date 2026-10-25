use super::*;

/// The final, structured statement AST (`if`/`while`/`for`, no stray
/// `Goto`/`Label` left over from flattening) that structuring's
/// `PreHirFunction -> HirFunction` conversion produces, and that
/// `fission-pcode`'s `render`/printer layer (and anything downstream of a
/// finished decompile) consumes. See `fission_midend_prehir::ir::PreHirStmt`'s
/// doc comment for why this is an independently-defined type rather than
/// `PreHirStmt` reused under a different name (`fission-midend-prehir` is
/// downstream of this crate, so this crate can't link to it directly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Assign {
        lhs: HirLValue,
        rhs: HirExpr,
    },
    Expr(HirExpr),
    VaStart {
        va_list: HirExpr,
        last_named_param: String,
    },
    Block(Vec<HirStmt>),
    Switch {
        expr: HirExpr,
        cases: Vec<HirSwitchCase>,
        default: Vec<HirStmt>,
    },
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    For {
        init: Option<Box<HirStmt>>,
        cond: Option<HirExpr>,
        update: Option<Box<HirStmt>>,
        body: Vec<HirStmt>,
    },
    Label(String),
    Goto(String),
    Return(Option<HirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirSwitchCase {
    pub values: Vec<i64>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirLValue {
    Var(String),
    Deref {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<HirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
}

#[derive(Clone, Eq)]
pub enum HirExpr {
    Var(String),
    AddressOfGlobal(String),
    AddressOfLocal(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<HirExpr>,
    },
    Unary {
        op: HirUnaryOp,
        expr: Box<HirExpr>,
        ty: NirType,
    },
    Binary {
        op: HirBinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        ty: NirType,
    },
    Select {
        cond: Box<HirExpr>,
        then_expr: Box<HirExpr>,
        else_expr: Box<HirExpr>,
        ty: NirType,
    },
    Call {
        /// Source operation; excluded from semantic expression equality.
        origin: Option<CallOrigin>,
        target: String,
        args: Vec<HirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<HirExpr>,
        offset: i64,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<HirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
    AggregateCopy {
        src: Box<HirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOp {
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
impl PartialEq for HirExpr {
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
impl std::fmt::Debug for HirExpr {
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

/// A real p-code CALL/CALLIND operation. Synthetic helpers and tail-transfer
/// summaries have no origin. Cloned expressions retain the original identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallOrigin {
    pub instruction_address: u64,
    pub pcode_sequence: u32,
}
