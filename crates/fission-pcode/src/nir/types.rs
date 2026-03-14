use fission_loader::loader::LoadedBinary;
use std::collections::HashMap;
use thiserror::Error;

pub type NirValueId = u32;
pub type StackSlotId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirType {
    Unknown,
    Bool,
    Int { bits: u32, signed: bool },
    Ptr(Box<NirType>),
    Aggregate { size: u32 },
    Float { bits: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBinding {
    pub name: String,
    pub ty: NirType,
    pub surface_type_name: Option<String>,
    pub initializer: Option<HirExpr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirFunction {
    pub name: String,
    pub address: u64,
    pub blocks: Vec<NirBlock>,
    pub locals: Vec<NirBinding>,
    pub params: Vec<NirBinding>,
    pub return_type: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBlock {
    pub id: u32,
    pub phis: Vec<String>,
    pub stmts: Vec<HirStmt>,
    pub terminator: NirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirTerminator {
    Fallthrough(Option<u32>),
    Goto(u32),
    Branch {
        cond: HirExpr,
        true_target: u32,
        false_target: Option<u32>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<NirBinding>,
    pub locals: Vec<NirBinding>,
    pub return_type: NirType,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Assign { lhs: HirLValue, rhs: HirExpr },
    Expr(HirExpr),
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
    Deref { ptr: Box<HirExpr>, ty: NirType },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExpr {
    Var(String),
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
    SLt,
    SLe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MlilPreviewOptions {
    pub pe_x64_only: bool,
    pub is_64bit: bool,
    pub pointer_size: u32,
    pub format: String,
    pub image_base: u64,
    pub sections: Vec<(u64, u64)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewTypeContext {
    pub call_targets: HashMap<u64, String>,
    pub call_param_rules: Vec<PreviewCallParamRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCallParamRule {
    pub callee_name: String,
    pub arg_index: usize,
    pub pointer_alias: String,
    pub pointee_alias: String,
    pub pointer_size: u32,
    pub pointee_sizes: Vec<u32>,
}

impl MlilPreviewOptions {
    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let sections = binary
            .inner()
            .sections
            .iter()
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size as u64,
                )
            })
            .collect();
        Self {
            pe_x64_only: true,
            is_64bit: binary.is_64bit,
            pointer_size: if binary.is_64bit { 8 } else { 4 },
            format: binary.format.clone(),
            image_base: binary.inner().image_base,
            sections,
        }
    }

    pub(super) fn is_pe_x64(&self) -> bool {
        self.is_64bit && self.format.to_ascii_uppercase().starts_with("PE")
    }

    pub(super) fn is_supported_pe(&self) -> bool {
        self.format.to_ascii_uppercase().starts_with("PE")
    }

    pub(super) fn is_mapped_global(&self, address: u64) -> bool {
        self.sections
            .iter()
            .any(|(start, end)| address >= *start && address < *end)
    }
}

#[derive(Debug, Error)]
pub enum MlilPreviewError {
    #[error("mlil-preview currently supports PE x64 only")]
    UnsupportedArchitecture,
    #[error("unsupported architecture in mlil-preview")]
    UnsupportedArchitectureDetailed,
    #[error("unsupported control flow in mlil-preview")]
    UnsupportedControlFlow,
    #[error("unsupported branch target in mlil-preview")]
    UnsupportedCfgBranchTarget,
    #[error("unsupported region shape in mlil-preview")]
    UnsupportedCfgRegionShape,
    #[error("unsupported phi join in mlil-preview")]
    UnsupportedCfgPhiJoin,
    #[error("unsupported indirect call region in mlil-preview")]
    UnsupportedCfgIndirectCallRegion,
    #[error("unsupported pcode pattern: {0}")]
    UnsupportedPattern(&'static str),
    #[error("value lowering failed")]
    LoweringFailed,
    #[error("value lowering failed on multiequal")]
    UnsupportedExprMultiequal,
    #[error("value lowering failed on varnode")]
    UnsupportedExprVarnodeLowering,
}
