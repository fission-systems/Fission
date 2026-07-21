use super::*;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExpr {
    Var(String),
    AddressOfGlobal(String),
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

/// The flattened, goto/label-based body that `fission-pcode`'s structuring
/// stage receives as input -- "동적으로 일치한 IR" (dynamically-verified IR):
/// the same `HirStmt`/`HirExpr` grammar as [`Hir`], captured immediately
/// before structuring's CFG-to-AST rewrite runs (see
/// `fission_pcode::take_last_dir_snapshot`). Deliberately a thin newtype
/// over `Vec<HirStmt>` rather than a parallel AST: DIR and HIR share an
/// identical grammar (structuring only ever rewrites control flow, never
/// invents new statement/expression shapes), so duplicating the enum would
/// just be the same variants twice with a conversion layer between them for
/// no benefit. What the newtype *does* buy: an accidental DIR/HIR argument
/// swap (e.g. in `fission_dir::diff::diff_dir_hir`) becomes a compile error
/// instead of a same-shaped `Vec<HirStmt>` silently passing type-checking
/// while being logically wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dir(pub Vec<HirStmt>);

/// The final, structured HIR body (if/while/for, no stray `Goto`/`Label`
/// left over from flattening) -- same wrapper rationale as [`Dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hir(pub Vec<HirStmt>);
