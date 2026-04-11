use super::*;

pub(crate) const UNIQUE_SPACE_ID: u64 = 3;
pub(crate) const REGISTER_SPACE_ID: u64 = 1;

pub(crate) const X86_TRY_LOWER_IF_BUDGET_MS: f64 = 10.0;
pub(crate) const X86_TRY_LOWER_IF_SUBCALL_LIMIT: usize = 512;
pub(crate) const X86_BRANCH_RECOVERY_BUDGET_MIN: usize = 48;
pub(crate) const X86_BRANCH_RECOVERY_BUDGET_PER_BLOCK: usize = 4;
pub(crate) const X86_BRANCH_RECOVERY_BUDGET_MAX: usize = 1024;
pub(crate) const X86_PASSTHROUGH_PEEL_MAX_STEPS: usize = 48;
pub(crate) const SWITCH_CHAIN_PARSE_BUDGET_MAX: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackBase {
    Rsp,
    Rbp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackSlot {
    pub(crate) id: StackSlotId,
    pub(crate) name: String,
    pub(crate) ty: NirType,
    pub(crate) origin: NirBindingOrigin,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct VarnodeKey {
    pub(crate) space_id: u64,
    pub(crate) offset: u64,
    pub(crate) size: u32,
    pub(crate) is_constant: bool,
    pub(crate) constant_val: i64,
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MaterializedVarnodeKey {
    pub(crate) varnode: VarnodeKey,
    pub(crate) def_addr: u64,
    pub(crate) def_seq: u32,
}

impl MaterializedVarnodeKey {
    pub(crate) fn new(vn: &Varnode, op: &PcodeOp) -> Self {
        Self {
            varnode: VarnodeKey::from(vn),
            def_addr: op.address,
            def_seq: op.seq_num,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DefSite<'a> {
    pub(crate) block_idx: usize,
    pub(crate) op_idx: usize,
    pub(crate) _marker: std::marker::PhantomData<&'a PcodeOp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LoweringSite {
    pub(crate) block_idx: usize,
    pub(crate) op_idx: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LoweredTerminator {
    Fallthrough(Option<u64>),
    Goto(u64),
    Cond {
        cond: HirExpr,
        true_target: u64,
        false_target: Option<u64>,
    },
    Switch {
        expr: HirExpr,
        targets: Vec<u64>,
        default_target: Option<u64>, // Usually the last target or something specific
        /// Offset to add to ordinal case indices when the switch selector was
        /// adjusted by the compiler (e.g. `sel = orig - min_val`).
        /// case value = `min_val + ordinal_index`.  Zero when unknown/unrecovered.
        min_val: i64,
    },
    Return(Option<HirExpr>),
    Unsupported {
        evidence: UnsupportedControlEvidence,
        target_expr: Option<HirExpr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LinearExit {
    Join(usize),
    Return,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LinearBodyCacheKey {
    pub(crate) start_idx: usize,
    pub(crate) exit: LinearExit,
    pub(crate) region_recovery: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ConditionalTailKey {
    pub(crate) true_idx: usize,
    pub(crate) false_idx: usize,
    pub(crate) exit: LinearExit,
    pub(crate) region_recovery: bool,
}

#[derive(Debug)]
pub(crate) struct IfLoweringBudget {
    pub(crate) enabled: bool,
    pub(crate) start: Instant,
    pub(crate) subcalls: usize,
    pub(crate) tripped: bool,
    pub(crate) idx: usize,
    pub(crate) block_addr: u64,
    pub(crate) label: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct SubpieceOrigin {
    pub(crate) base: VarnodeKey,
    pub(crate) base_vn: Varnode,
    pub(crate) base_size: u32,
    pub(crate) byte_offset: i64,
    pub(crate) piece_size: u32,
}

pub(crate) fn is_comparison(opcode: PcodeOpcode) -> bool {
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

pub(crate) fn map_binary_op(opcode: PcodeOpcode) -> Result<HirBinaryOp, MlilPreviewError> {
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

pub(crate) fn type_from_size(size: u32, signed: bool) -> NirType {
    match size {
        1 => NirType::Int { bits: 8, signed },
        2 => NirType::Int { bits: 16, signed },
        4 => NirType::Int { bits: 32, signed },
        8 => NirType::Int { bits: 64, signed },
        16 | 24 | 32 => NirType::Aggregate {
            size,
            fields: vec![],
        },
        _ => NirType::Unknown,
    }
}

pub(crate) fn is_materializable_output_opcode(opcode: PcodeOpcode) -> bool {
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

pub(crate) fn next_temp_name(ty: &NirType, next_id: &mut u32) -> String {
    let prefix = match ty {
        NirType::Bool => "bVar",
        NirType::Int {
            bits: 32,
            signed: true,
        } => "iVar",
        NirType::Int {
            bits: 32,
            signed: false,
        } => "uVar",
        _ => "xVar",
    };
    let name = format!("{prefix}{}", *next_id);
    *next_id += 1;
    name
}

/// x64 calling convention used when identifying parameter registers.
///
/// This affects which REGISTER-space varnodes are labelled `param_1`, `param_2`, etc.
/// in decompiled output. It does **not** affect hardware register names (rax, rbx, …).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CallingConvention {
    /// Windows x64 fastcall: first four integer args in RCX, RDX, R8, R9.
    WindowsX64,
    /// System V AMD64 ABI (Linux / macOS): first six integer args in RDI, RSI, RDX, RCX, R8, R9.
    SystemVAmd64,
}

impl Default for CallingConvention {
    fn default() -> Self {
        Self::WindowsX64
    }
}

impl CallingConvention {
    /// Returns the ordered list of Ghidra REGISTER-space offsets for integer parameter registers.
    pub(crate) fn param_offsets(self) -> &'static [u64] {
        match self {
            Self::WindowsX64 => &[
                0x08, // rcx → param_1
                0x10, // rdx → param_2
                0x80, // r8  → param_3
                0x88, // r9  → param_4
            ],
            Self::SystemVAmd64 => &[
                0x38, // rdi → param_1
                0x30, // rsi → param_2
                0x10, // rdx → param_3
                0x08, // rcx → param_4
                0x80, // r8  → param_5
                0x88, // r9  → param_6
            ],
        }
    }

    /// Returns the (REGISTER-space offset, 64-bit size) pairs for all integer
    /// parameter registers in 64-bit mode — used for call argument recovery.
    pub(crate) fn param_reg_slots_64(self) -> &'static [(u64, u32)] {
        match self {
            Self::WindowsX64 => &[
                (0x08, 8), // rcx  → param_1
                (0x10, 8), // rdx  → param_2
                (0x80, 8), // r8   → param_3
                (0x88, 8), // r9   → param_4
            ],
            Self::SystemVAmd64 => &[
                (0x38, 8), // rdi  → param_1
                (0x30, 8), // rsi  → param_2
                (0x10, 8), // rdx  → param_3
                (0x08, 8), // rcx  → param_4
                (0x80, 8), // r8   → param_5
                (0x88, 8), // r9   → param_6
            ],
        }
    }
}

/// Maps a Ghidra REGISTER-space offset to the hardware register name for x86-64.
///
/// This is ABI-independent — offset 0x08 is always RCX regardless of calling convention.
pub(crate) fn x64_ghidra_reg_name(offset: u64) -> Option<&'static str> {
    match offset {
        0x00 => Some("rax"),
        0x08 => Some("rcx"),
        0x10 => Some("rdx"),
        0x18 => Some("rbx"),
        0x20 => Some("rsp"),
        0x28 => Some("rbp"),
        0x30 => Some("rsi"),
        0x38 => Some("rdi"),
        0x80 => Some("r8"),
        0x88 => Some("r9"),
        0x90 => Some("r10"),
        0x98 => Some("r11"),
        0xa0 => Some("r12"),
        0xa8 => Some("r13"),
        0xb0 => Some("r14"),
        0xb8 => Some("r15"),
        _ => None,
    }
}

/// Static `param_N` names for up to 6 parameters (enough for System V AMD64).
const PARAM_NAMES: [&str; 6] = [
    "param_1", "param_2", "param_3", "param_4", "param_5", "param_6",
];

/// Returns `(display_name, param_index)` for a Ghidra REGISTER-space varnode.
///
/// - If the offset is a parameter register for `abi`, returns `("param_N", Some(N-1))`.
/// - Otherwise returns the hardware register name with `None`.
pub(crate) fn register_name_with_param(
    offset: u64,
    _size: u32,
    abi: CallingConvention,
) -> Option<(&'static str, Option<usize>)> {
    let hw_name = x64_ghidra_reg_name(offset)?;
    let param_idx = abi
        .param_offsets()
        .iter()
        .position(|&param_offset| param_offset == offset);
    match param_idx {
        Some(idx) => Some((PARAM_NAMES[idx], Some(idx))),
        None => Some((hw_name, None)),
    }
}

/// Returns the hardware register name for a Ghidra REGISTER-space offset, ABI-independently.
pub(crate) fn register_name(offset: u64, _size: u32) -> &'static str {
    x64_ghidra_reg_name(offset).unwrap_or("reg")
}

pub(crate) fn x86_register_name(offset: u64, size: u32) -> Option<&'static str> {
    match (offset, size) {
        (0x00, 4) => Some("eax"),
        (0x04, 4) => Some("ecx"),
        (0x08, 4) => Some("edx"),
        (0x0c, 4) => Some("ebx"),
        (0x10, 4) => Some("esp"),
        (0x14, 4) => Some("ebp"),
        (0x18, 4) => Some("esi"),
        (0x1c, 4) => Some("edi"),
        _ => None,
    }
}

pub(crate) fn expr_type(expr: &HirExpr) -> NirType {
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
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
            size: *size,
            fields: vec![],
        },
    }
}
