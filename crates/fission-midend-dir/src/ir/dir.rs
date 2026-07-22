use fission_midend_core::ir::NirType;

/// The flattened, goto/label-based statement AST that `fission-pcode`'s
/// builder emits directly from p-code, and that normalize/structuring's own
/// internal passes read and incrementally rewrite -- this is DIR (the
/// pre-structuring pipeline stage). Structuring performs a genuine
/// `DirFunction -> HirFunction` conversion once its CFG-to-AST rewrite is
/// done (see `fission_pcode::midend::orchestrate`'s call site and
/// [`crate::ir::dir_stmts_to_hir_stmts`]); nothing upstream of that
/// conversion ever produces or touches `HirStmt`
/// (`fission_midend_core::ir::HirStmt`).
///
/// `DirStmt` and `HirStmt` are independently defined (not a shared type
/// wearing two names, and not one generated from the other via a macro) even
/// though their shapes start out identical -- the point of separating them
/// is that they are allowed to diverge as DIR's and HIR's respective needs
/// diverge (e.g. HIR-only surface-presentation concerns, or DIR-only
/// pre-structuring bookkeeping), and a real independent definition makes an
/// accidental DIR/HIR mix-up a compile error rather than two same-shaped
/// types silently type-checking while being logically backwards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirStmt {
    Assign {
        lhs: DirLValue,
        rhs: DirExpr,
    },
    Expr(DirExpr),
    VaStart {
        va_list: DirExpr,
        last_named_param: String,
    },
    Block(Vec<DirStmt>),
    Switch {
        expr: DirExpr,
        cases: Vec<DirSwitchCase>,
        default: Vec<DirStmt>,
    },
    If {
        cond: DirExpr,
        then_body: Vec<DirStmt>,
        else_body: Vec<DirStmt>,
    },
    While {
        cond: DirExpr,
        body: Vec<DirStmt>,
    },
    DoWhile {
        body: Vec<DirStmt>,
        cond: DirExpr,
    },
    For {
        init: Option<Box<DirStmt>>,
        cond: Option<DirExpr>,
        update: Option<Box<DirStmt>>,
        body: Vec<DirStmt>,
    },
    Label(String),
    Goto(String),
    Return(Option<DirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirSwitchCase {
    pub values: Vec<i64>,
    pub body: Vec<DirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirLValue {
    Var(String),
    Deref {
        ptr: Box<DirExpr>,
        ty: NirType,
    },
    Index {
        base: Box<DirExpr>,
        index: Box<DirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<DirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirExpr {
    Var(String),
    AddressOfGlobal(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<DirExpr>,
    },
    Unary {
        op: DirUnaryOp,
        expr: Box<DirExpr>,
        ty: NirType,
    },
    Binary {
        op: DirBinaryOp,
        lhs: Box<DirExpr>,
        rhs: Box<DirExpr>,
        ty: NirType,
    },
    Select {
        cond: Box<DirExpr>,
        then_expr: Box<DirExpr>,
        else_expr: Box<DirExpr>,
        ty: NirType,
    },
    Call {
        target: String,
        args: Vec<DirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<DirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<DirExpr>,
        offset: i64,
    },
    Index {
        base: Box<DirExpr>,
        index: Box<DirExpr>,
        elem_ty: NirType,
    },
    FieldAccess {
        base: Box<DirExpr>,
        field_name: String,
        offset: u32,
        ty: NirType,
    },
    AggregateCopy {
        src: Box<DirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirBinaryOp {
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

