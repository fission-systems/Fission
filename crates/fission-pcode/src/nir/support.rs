use super::*;

pub(crate) const UNIQUE_SPACE_ID: u64 = 3;
pub(crate) const REGISTER_SPACE_ID: u64 = 1;
pub(crate) const RUST_SLEIGH_REGISTER_SPACE_ID: u64 = 4;

pub(crate) fn is_register_space_id(space_id: u64) -> bool {
    space_id == REGISTER_SPACE_ID || space_id == RUST_SLEIGH_REGISTER_SPACE_ID
}

pub(crate) fn is_register_varnode(vn: &Varnode) -> bool {
    is_register_space_id(vn.space_id)
}

pub(crate) const CONDITION_RECOVERY_BUDGET_MS: f64 = 10.0;
pub(crate) const CONDITION_RECOVERY_SUBCALL_LIMIT: usize = 512;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MIN: usize = 48;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_PER_BLOCK: usize = 4;
pub(crate) const BRANCH_CONDITION_RECOVERY_BUDGET_MAX: usize = 1024;
pub(crate) const PASSTHROUGH_PEEL_MAX_STEPS: usize = 48;
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
        proof: Option<DispatcherProofUnit>,
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

pub(crate) fn recovered_switch_case_values(
    targets: &[u64],
    default_target: Option<u64>,
    min_val: i64,
    proof: Option<&DispatcherProofUnit>,
) -> (Vec<(i64, u64)>, bool) {
    if let Some(proof) = proof
        && proof_supports_direct_emit(proof)
    {
        let recovered = proof
            .recovered_cases
            .iter()
            .copied()
            .filter(|(_, target)| Some(*target) != default_target)
            .collect::<Vec<_>>();
        if !recovered.is_empty() {
            return (recovered, true);
        }
    }

    (
        targets
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(ordinal, target)| {
                (Some(target) != default_target).then_some((min_val + ordinal as i64, target))
            })
            .collect(),
        false,
    )
}

pub(crate) fn proof_supports_direct_emit(proof: &DispatcherProofUnit) -> bool {
    crate::nir::structuring::EmitReadyDecision::from_dispatcher_proof(Some(proof)).emit_ready
        && proof.recovered_cases.len() >= proof.selector_cardinality
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
    /// AArch64 Procedure Call Standard: first eight integer args in X0-X7/W0-W7.
    AArch64,
    /// ARM Procedure Call Standard: first four integer args in R0-R3.
    Arm32,
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
            Self::AArch64 => &[
                0x4000, // x0/w0 → param_1
                0x4008, // x1/w1 → param_2
                0x4010, // x2/w2 → param_3
                0x4018, // x3/w3 → param_4
                0x4020, // x4/w4 → param_5
                0x4028, // x5/w5 → param_6
                0x4030, // x6/w6 → param_7
                0x4038, // x7/w7 → param_8
            ],
            Self::Arm32 => &[
                0x20, // r0 → param_1
                0x24, // r1 → param_2
                0x28, // r2 → param_3
                0x2c, // r3 → param_4
            ],
        }
    }

    /// Returns the (REGISTER-space offset, varnode size) pairs for all integer
    /// parameter registers used by call argument recovery.
    pub(crate) fn param_reg_slots(self) -> &'static [(u64, u32)] {
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
            Self::AArch64 => &[
                (0x4000, 8), // x0  → param_1
                (0x4008, 8), // x1  → param_2
                (0x4010, 8), // x2  → param_3
                (0x4018, 8), // x3  → param_4
                (0x4020, 8), // x4  → param_5
                (0x4028, 8), // x5  → param_6
                (0x4030, 8), // x6  → param_7
                (0x4038, 8), // x7  → param_8
            ],
            Self::Arm32 => &[
                (0x20, 4), // r0  → param_1
                (0x24, 4), // r1  → param_2
                (0x28, 4), // r2  → param_3
                (0x2c, 4), // r3  → param_4
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

pub(crate) fn aarch64_ghidra_reg_name(offset: u64, size: u32) -> Option<&'static str> {
    const X_REGS: [&str; 32] = [
        "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13",
        "x14", "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26",
        "x27", "x28", "x29", "x30", "xzr",
    ];
    const W_REGS: [&str; 32] = [
        "w0", "w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10", "w11", "w12", "w13",
        "w14", "w15", "w16", "w17", "w18", "w19", "w20", "w21", "w22", "w23", "w24", "w25", "w26",
        "w27", "w28", "w29", "w30", "wzr",
    ];
    if offset == 0x08 && size == 8 {
        return Some("sp");
    }
    match size {
        4 if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            W_REGS.get(idx).copied()
        }
        4 if (0x4004..=0x40fc).contains(&offset) && (offset - 0x4004) % 8 == 0 => {
            let idx = ((offset - 0x4004) / 8) as usize;
            W_REGS.get(idx).copied()
        }
        8 if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            X_REGS.get(idx).copied()
        }
        _ if (0x4000..=0x40f8).contains(&offset) && (offset - 0x4000) % 8 == 0 => {
            let idx = ((offset - 0x4000) / 8) as usize;
            X_REGS.get(idx).copied()
        }
        _ => None,
    }
}

pub(crate) fn aarch64_gpr_family_index(name: &str) -> Option<usize> {
    let rest = name.strip_prefix('x').or_else(|| name.strip_prefix('w'))?;
    let idx = rest.parse::<usize>().ok()?;
    (idx < 31).then_some(idx)
}

pub(crate) fn arm32_ghidra_reg_name(offset: u64, size: u32) -> Option<&'static str> {
    if size != 4 {
        return None;
    }
    match offset {
        0x20 => Some("r0"),
        0x24 => Some("r1"),
        0x28 => Some("r2"),
        0x2c => Some("r3"),
        0x30 => Some("r4"),
        0x34 => Some("r5"),
        0x38 => Some("r6"),
        0x3c => Some("r7"),
        0x40 => Some("r8"),
        0x44 => Some("r9"),
        0x48 => Some("r10"),
        0x4c => Some("r11"),
        0x50 => Some("r12"),
        0x54 => Some("sp"),
        0x58 => Some("lr"),
        0x5c => Some("pc"),
        _ => None,
    }
}

pub(crate) fn arm32_gpr_family_index(name: &str) -> Option<usize> {
    match name {
        "sp" => Some(13),
        "lr" => Some(14),
        "pc" => Some(15),
        _ => {
            let idx = name.strip_prefix('r')?.parse::<usize>().ok()?;
            (idx < 13).then_some(idx)
        }
    }
}

/// Static `param_N` names for up to 8 parameters (enough for AArch64 PCS).
const PARAM_NAMES: [&str; 8] = [
    "param_1", "param_2", "param_3", "param_4", "param_5", "param_6", "param_7", "param_8",
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
    let hw_name = match abi {
        CallingConvention::AArch64 if offset == 0x00 => match _size {
            4 => "w0",
            _ => "x0",
        },
        CallingConvention::AArch64 => aarch64_ghidra_reg_name(offset, _size)?,
        CallingConvention::Arm32 => arm32_ghidra_reg_name(offset, _size)?,
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
            x64_ghidra_reg_name(offset)?
        }
    };
    let param_idx = match abi {
        CallingConvention::AArch64 => aarch64_gpr_family_index(hw_name).and_then(|name_family| {
            abi.param_offsets().iter().position(|&param_offset| {
                aarch64_ghidra_reg_name(param_offset, 8)
                    .and_then(aarch64_gpr_family_index)
                    .is_some_and(|family| family == name_family)
            })
        }),
        CallingConvention::Arm32 => arm32_gpr_family_index(hw_name).and_then(|name_family| {
            abi.param_offsets().iter().position(|&param_offset| {
                arm32_ghidra_reg_name(param_offset, 4)
                    .and_then(arm32_gpr_family_index)
                    .is_some_and(|family| family == name_family)
            })
        }),
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => abi
            .param_offsets()
            .iter()
            .position(|&param_offset| param_offset == offset),
    };
    match param_idx {
        Some(idx) => Some((PARAM_NAMES[idx], Some(idx))),
        None => Some((hw_name, None)),
    }
}

/// Returns the hardware register name for a Ghidra REGISTER-space offset, ABI-independently.
pub(crate) fn register_name(offset: u64, size: u32) -> &'static str {
    x64_ghidra_reg_name(offset)
        .or_else(|| aarch64_ghidra_reg_name(offset, size))
        .or_else(|| arm32_ghidra_reg_name(offset, size))
        .unwrap_or("reg")
}

pub(crate) fn unique_register_name(offset: u64, size: u32) -> Option<&'static str> {
    crate::arch::x86::unique_x86_register_name(offset, size)
}

pub(crate) fn is_primary_return_register(vn: &Varnode) -> bool {
    ((vn.space_id == REGISTER_SPACE_ID || vn.space_id == RUST_SLEIGH_REGISTER_SPACE_ID)
        && vn.offset == 0x00)
        || (vn.space_id == UNIQUE_SPACE_ID
            && unique_register_name(vn.offset, vn.size) == Some("rax"))
}

pub(crate) fn is_primary_return_register_for_abi(vn: &Varnode, abi: CallingConvention) -> bool {
    match abi {
        CallingConvention::AArch64 => {
            (vn.space_id == REGISTER_SPACE_ID || vn.space_id == RUST_SLEIGH_REGISTER_SPACE_ID)
                && aarch64_ghidra_reg_name(vn.offset, vn.size)
                    .and_then(aarch64_gpr_family_index)
                    == Some(0)
        }
        CallingConvention::Arm32 => {
            (vn.space_id == REGISTER_SPACE_ID || vn.space_id == RUST_SLEIGH_REGISTER_SPACE_ID)
                && vn.offset == 0x20
        }
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => {
            is_primary_return_register(vn)
        }
    }
}

pub(crate) fn is_return_target_register_for_abi(vn: &Varnode, abi: CallingConvention) -> bool {
    if !is_register_space_id(vn.space_id) {
        return false;
    }
    match abi {
        CallingConvention::AArch64 => {
            aarch64_ghidra_reg_name(vn.offset, vn.size)
                .and_then(aarch64_gpr_family_index)
                == Some(30)
        }
        CallingConvention::Arm32 => vn.offset == 0x58,
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => false,
    }
}

pub(crate) fn primary_return_registers(pointer_size: u32, abi: CallingConvention) -> Vec<Varnode> {
    match abi {
        CallingConvention::AArch64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x4000,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x4000,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::Arm32 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x20,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: RUST_SLEIGH_REGISTER_SPACE_ID,
                offset: 0x20,
                size: 4,
                is_constant: false,
                constant_val: 0,
            },
        ],
        CallingConvention::WindowsX64 | CallingConvention::SystemVAmd64 => vec![
            Varnode {
                space_id: REGISTER_SPACE_ID,
                offset: 0x00,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
            Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: crate::arch::x86::X86_REG_BASE,
                size: pointer_size,
                is_constant: false,
                constant_val: 0,
            },
        ],
    }
}

pub(crate) fn register_name_32(offset: u64, size: u32) -> Option<&'static str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_legality() -> DispatcherLegality {
        DispatcherLegality {
            follow_block: Some(0x1300),
            postdom_ok: true,
            side_effect_free_selector: true,
            ordinal_domain_complete: true,
            shared_tail_conflict: false,
            valid: true,
        }
    }

    fn proof_with_cases(
        recovered_cases: Vec<(i64, u64)>,
        selector_cardinality: usize,
        proof_complete: bool,
        failure_family: Option<ProofFailureFamily>,
    ) -> DispatcherProofUnit {
        DispatcherProofUnit {
            selector_expr: "selector".to_string(),
            rendered_selector_expr: Some("selector".to_string()),
            candidate_targets: recovered_cases.iter().map(|(_, target)| *target).collect(),
            recovered_cases,
            selector_cardinality,
            target_cardinality: 2,
            case_map_source: DispatcherCaseMapSource::Merged,
            default_target: Some(0x1300),
            guard_set: vec!["ordinal_domain_complete".to_string()],
            follow_block: Some(0x1300),
            normalization: None,
            legality_witness: Some(complete_legality()),
            proof_scope: DispatcherProofScope::OuterDispatch,
            proof_complete,
            failure_family,
        }
    }

    #[test]
    fn proof_supports_direct_emit_allows_many_to_one_case_map() {
        let proof = proof_with_cases(vec![(0, 0x1100), (1, 0x1100), (2, 0x1200)], 3, true, None);
        assert!(proof_supports_direct_emit(&proof));
    }

    #[test]
    fn recovered_switch_case_values_ignore_incomplete_proof_payload() {
        let proof = proof_with_cases(
            vec![(0, 0x1100), (1, 0x1200)],
            2,
            false,
            Some(ProofFailureFamily::MissingOrdinalCoverage),
        );
        let (cases, used_proof_payload) =
            recovered_switch_case_values(&[0x1100, 0x1200], Some(0x1300), 7, Some(&proof));
        assert!(!used_proof_payload);
        assert_eq!(cases, vec![(7, 0x1100), (8, 0x1200)]);
    }

    #[test]
    fn emit_ready_decision_requires_complete_proof() {
        let proof = proof_with_cases(
            vec![(0, 0x1100), (1, 0x1200)],
            2,
            false,
            Some(ProofFailureFamily::MissingOrdinalCoverage),
        );
        let decision =
            crate::nir::structuring::EmitReadyDecision::from_dispatcher_proof(Some(&proof));
        assert!(decision.proof_present);
        assert!(!decision.proof_complete);
        assert!(!decision.emit_ready);
        assert_eq!(
            decision.failure,
            Some(crate::nir::structuring::EmitReadyFailureFamily::ProofIncomplete)
        );
    }
}
