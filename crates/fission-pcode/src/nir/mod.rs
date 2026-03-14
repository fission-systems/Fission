use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use std::collections::{BTreeMap, HashMap, HashSet};

mod cfg;
mod builder;
mod normalize;
mod piece;
mod printer;
mod structuring;
mod types;
#[cfg(test)]
mod tests;

pub use self::types::*;
use self::{builder::*, cfg::*, normalize::*, printer::*};

const UNIQUE_SPACE_ID: u64 = 3;
const REGISTER_SPACE_ID: u64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackBase {
    Rsp,
    Rbp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StackSlot {
    id: StackSlotId,
    name: String,
    ty: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VarnodeKey {
    space_id: u64,
    offset: u64,
    size: u32,
    is_constant: bool,
    constant_val: i64,
}

impl From<&Varnode> for VarnodeKey {
    fn from(value: &Varnode) -> Self {
        Self {
            space_id: value.space_id,
            offset: value.offset,
            size: value.size,
            is_constant: value.is_constant,
            constant_val: value.constant_val,
        }
    }
}

#[derive(Debug)]
struct PreviewBuilder<'a> {
    pcode: &'a PcodeFunction,
    options: &'a MlilPreviewOptions,
    type_context: Option<&'a PreviewTypeContext>,
    defs: HashMap<VarnodeKey, &'a PcodeOp>,
    address_to_index: HashMap<u64, usize>,
    layout_fallthrough: Vec<Option<usize>>,
    successors: Vec<Vec<usize>>,
    predecessors: Vec<Vec<usize>>,
    params: BTreeMap<usize, NirBinding>,
    locals: BTreeMap<i64, StackSlot>,
    locals_next_id: StackSlotId,
    temps: BTreeMap<String, NirBinding>,
    temp_next_id: u32,
    materialized_vns: HashMap<VarnodeKey, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinearExit {
    Join(usize),
    Return,
    End,
}

#[derive(Debug, Clone)]
struct SubpieceOrigin {
    base: VarnodeKey,
    base_vn: Varnode,
    base_size: u32,
    byte_offset: i64,
    piece_size: u32,
}

pub fn render_mlil_preview(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
) -> Result<String, MlilPreviewError> {
    render_mlil_preview_with_context(pcode, name, address, options, None)
}

pub fn render_mlil_preview_with_context(
    pcode: &PcodeFunction,
    name: &str,
    address: u64,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Result<String, MlilPreviewError> {
    if options.pe_x64_only && !options.is_supported_pe() {
        return Err(MlilPreviewError::UnsupportedArchitectureDetailed);
    }

    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=build_hir start fn=0x{address:x}");
    }
    let mut builder = PreviewBuilder::new(pcode, options, type_context);
    let mut hir = builder.build_hir(name, address).map_err(|err| {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!(
                "[mlil-preview] stage=build_hir error fn=0x{address:x} err={err}"
            );
        }
        err
    })?;
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=normalize start fn=0x{address:x}");
    }
    normalize_hir_function(&mut hir);
    if let Some(context) = type_context {
        if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
            eprintln!("[mlil-preview] stage=type_hints start fn=0x{address:x}");
        }
        apply_preview_type_hints(&mut hir, context);
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print start fn=0x{address:x}");
    }
    let rendered = print_hir_function(&hir);
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        eprintln!("[mlil-preview] stage=print done fn=0x{address:x}");
    }
    Ok(rendered)
}

fn is_comparison(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
    )
}

fn map_binary_op(opcode: PcodeOpcode) -> Result<HirBinaryOp, MlilPreviewError> {
    match opcode {
        PcodeOpcode::IntAdd => Ok(HirBinaryOp::Add),
        PcodeOpcode::IntSub => Ok(HirBinaryOp::Sub),
        PcodeOpcode::IntMult => Ok(HirBinaryOp::Mul),
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv => Ok(HirBinaryOp::Div),
        PcodeOpcode::IntRem | PcodeOpcode::IntSRem => Ok(HirBinaryOp::Mod),
        PcodeOpcode::IntAnd => Ok(HirBinaryOp::And),
        PcodeOpcode::BoolAnd => Ok(HirBinaryOp::LogicalAnd),
        PcodeOpcode::IntOr => Ok(HirBinaryOp::Or),
        PcodeOpcode::BoolOr => Ok(HirBinaryOp::LogicalOr),
        PcodeOpcode::IntXor | PcodeOpcode::BoolXor => Ok(HirBinaryOp::Xor),
        PcodeOpcode::IntLeft => Ok(HirBinaryOp::Shl),
        PcodeOpcode::IntRight => Ok(HirBinaryOp::Shr),
        PcodeOpcode::IntSRight => Ok(HirBinaryOp::Sar),
        PcodeOpcode::IntEqual => Ok(HirBinaryOp::Eq),
        PcodeOpcode::IntNotEqual => Ok(HirBinaryOp::Ne),
        PcodeOpcode::IntLess => Ok(HirBinaryOp::Lt),
        PcodeOpcode::IntLessEqual => Ok(HirBinaryOp::Le),
        PcodeOpcode::IntSLess => Ok(HirBinaryOp::SLt),
        PcodeOpcode::IntSLessEqual => Ok(HirBinaryOp::SLe),
        _ => Err(MlilPreviewError::UnsupportedPattern("binary op")),
    }
}

fn type_from_size(size: u32, signed: bool) -> NirType {
    match size {
        1 => NirType::Int { bits: 8, signed },
        2 => NirType::Int { bits: 16, signed },
        4 => NirType::Int { bits: 32, signed },
        8 => NirType::Int { bits: 64, signed },
        16 | 24 | 32 => NirType::Aggregate { size },
        _ => NirType::Unknown,
    }
}

fn is_materializable_output_opcode(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::Copy
            | PcodeOpcode::Cast
            | PcodeOpcode::IntZExt
            | PcodeOpcode::IntSExt
            | PcodeOpcode::Load
            | PcodeOpcode::PtrAdd
            | PcodeOpcode::PtrSub
            | PcodeOpcode::IntAdd
            | PcodeOpcode::IntSub
            | PcodeOpcode::IntMult
            | PcodeOpcode::IntDiv
            | PcodeOpcode::IntSDiv
            | PcodeOpcode::IntRem
            | PcodeOpcode::IntSRem
            | PcodeOpcode::IntAnd
            | PcodeOpcode::IntOr
            | PcodeOpcode::IntXor
            | PcodeOpcode::IntLeft
            | PcodeOpcode::IntRight
            | PcodeOpcode::IntSRight
            | PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::BoolAnd
            | PcodeOpcode::BoolOr
            | PcodeOpcode::BoolXor
            | PcodeOpcode::IntNegate
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow
            | PcodeOpcode::PopCount
            | PcodeOpcode::Call
            | PcodeOpcode::CallInd
            | PcodeOpcode::CallOther
            | PcodeOpcode::Piece
            | PcodeOpcode::SubPiece
            | PcodeOpcode::MultiEqual
            | PcodeOpcode::Indirect
    )
}

fn next_temp_name(ty: &NirType, next_id: &mut u32) -> String {
    let prefix = match ty {
        NirType::Bool => "bVar",
        NirType::Int { bits: 32, signed: true } => "iVar",
        NirType::Int { bits: 32, signed: false } => "uVar",
        _ => "xVar",
    };
    let name = format!("{prefix}{}", *next_id);
    *next_id += 1;
    name
}

fn register_name_with_param(offset: u64, _size: u32) -> Option<(&'static str, Option<usize>)> {
    match offset {
        0x08 => Some(("param_1", Some(0))),
        0x10 => Some(("param_2", Some(1))),
        0x80 => Some(("param_3", Some(2))),
        0x88 => Some(("param_4", Some(3))),
        0x00 => Some(("rax", None)),
        0x18 => Some(("rbx", None)),
        0x20 => Some(("rsp", None)),
        0x28 => Some(("rbp", None)),
        0x30 => Some(("rsi", None)),
        0x38 => Some(("rdi", None)),
        0x90 => Some(("r10", None)),
        0x98 => Some(("r11", None)),
        0xa0 => Some(("r12", None)),
        0xa8 => Some(("r13", None)),
        0xb0 => Some(("r14", None)),
        0xb8 => Some(("r15", None)),
        _ => None,
    }
}

fn register_name(offset: u64, size: u32) -> &'static str {
    register_name_with_param(offset, size)
        .map(|(name, _)| name)
        .unwrap_or("reg")
}

fn expr_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::Var(_) => NirType::Unknown,
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate { size: *size },
    }
}
