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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreHirExpr {
    Var(String),
    AddressOfGlobal(String),
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
