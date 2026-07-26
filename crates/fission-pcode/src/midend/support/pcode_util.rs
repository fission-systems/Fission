use super::*;

pub(crate) fn is_comparison(opcode: PcodeOpcode) -> bool {
    matches!(
        opcode,
        PcodeOpcode::IntEqual
            | PcodeOpcode::IntNotEqual
            | PcodeOpcode::IntLess
            | PcodeOpcode::IntLessEqual
            | PcodeOpcode::IntSLess
            | PcodeOpcode::IntSLessEqual
            | PcodeOpcode::FloatEqual
            | PcodeOpcode::FloatNotEqual
            | PcodeOpcode::FloatLess
            | PcodeOpcode::FloatLessEqual
    )
}

pub(crate) fn map_binary_op(opcode: PcodeOpcode) -> Result<PreHirBinaryOp, MlilPreviewError> {
    match opcode {
        PcodeOpcode::IntAdd | PcodeOpcode::FloatAdd => Ok(PreHirBinaryOp::Add),
        PcodeOpcode::IntSub | PcodeOpcode::FloatSub => Ok(PreHirBinaryOp::Sub),
        PcodeOpcode::IntMult | PcodeOpcode::FloatMult => Ok(PreHirBinaryOp::Mul),
        PcodeOpcode::IntDiv | PcodeOpcode::IntSDiv | PcodeOpcode::FloatDiv => {
            Ok(PreHirBinaryOp::Div)
        }
        PcodeOpcode::IntRem | PcodeOpcode::IntSRem => Ok(PreHirBinaryOp::Mod),
        PcodeOpcode::IntAnd => Ok(PreHirBinaryOp::And),
        PcodeOpcode::BoolAnd => Ok(PreHirBinaryOp::LogicalAnd),
        PcodeOpcode::IntOr => Ok(PreHirBinaryOp::Or),
        PcodeOpcode::BoolOr => Ok(PreHirBinaryOp::LogicalOr),
        PcodeOpcode::IntXor | PcodeOpcode::BoolXor => Ok(PreHirBinaryOp::Xor),
        PcodeOpcode::IntLeft => Ok(PreHirBinaryOp::Shl),
        PcodeOpcode::IntRight => Ok(PreHirBinaryOp::Shr),
        PcodeOpcode::IntSRight => Ok(PreHirBinaryOp::Sar),
        PcodeOpcode::IntEqual | PcodeOpcode::FloatEqual => Ok(PreHirBinaryOp::Eq),
        PcodeOpcode::IntNotEqual | PcodeOpcode::FloatNotEqual => Ok(PreHirBinaryOp::Ne),
        PcodeOpcode::IntLess | PcodeOpcode::FloatLess => Ok(PreHirBinaryOp::Lt),
        PcodeOpcode::IntLessEqual | PcodeOpcode::FloatLessEqual => Ok(PreHirBinaryOp::Le),
        PcodeOpcode::IntSLess => Ok(PreHirBinaryOp::SLt),
        PcodeOpcode::IntSLessEqual => Ok(PreHirBinaryOp::SLe),
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

pub(crate) fn pcode_output_type_from_size(opcode: PcodeOpcode, size: u32) -> NirType {
    // Float arithmetic / conversion outputs are IEEE values of `size` bytes.
    // Without this arm, `ensure_temp_binding_for_output` types FLOAT_MULT /
    // FLOAT_ADD temps as `uint`/`int`, so matrix-class float kernels print as
    // integer `*=`/`+=` and poison downstream pointer elem types.
    if matches!(
        opcode,
        PcodeOpcode::FloatAdd
            | PcodeOpcode::FloatSub
            | PcodeOpcode::FloatMult
            | PcodeOpcode::FloatDiv
            | PcodeOpcode::FloatNeg
            | PcodeOpcode::FloatAbs
            | PcodeOpcode::FloatSqrt
            | PcodeOpcode::FloatCeil
            | PcodeOpcode::FloatFloor
            | PcodeOpcode::FloatRound
            | PcodeOpcode::FloatInt2Float
            | PcodeOpcode::FloatFloat2Float
    ) {
        return float_type_from_size(size);
    }
    // FLOAT_TRUNC is float→signed int (Ghidra TypeOpFloatTrunc: TYPE_INT).
    if matches!(opcode, PcodeOpcode::FloatTrunc) {
        return type_from_size(size, true);
    }
    // Float comparisons produce a boolean-sized flag, not a float.
    if matches!(
        opcode,
        PcodeOpcode::FloatEqual
            | PcodeOpcode::FloatNotEqual
            | PcodeOpcode::FloatLess
            | PcodeOpcode::FloatLessEqual
            | PcodeOpcode::FloatNan
    ) {
        return if size == 1 {
            NirType::Bool
        } else {
            type_from_size(size, false)
        };
    }
    type_from_size(
        size,
        matches!(
            opcode,
            PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntSDiv
                | PcodeOpcode::IntSRem
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
                | PcodeOpcode::IntSExt
                | PcodeOpcode::IntSRight
        ),
    )
}

pub(crate) fn float_type_from_size(size: u32) -> NirType {
    match size {
        4 => NirType::Float { bits: 32 },
        8 => NirType::Float { bits: 64 },
        10 => NirType::Float { bits: 80 },
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
            | PcodeOpcode::FloatAdd
            | PcodeOpcode::FloatDiv
            | PcodeOpcode::FloatMult
            | PcodeOpcode::FloatSub
            | PcodeOpcode::FloatInt2Float
            | PcodeOpcode::FloatFloat2Float
            | PcodeOpcode::FloatTrunc
            | PcodeOpcode::FloatNeg
            | PcodeOpcode::FloatAbs
            | PcodeOpcode::FloatSqrt
            | PcodeOpcode::FloatCeil
            | PcodeOpcode::FloatFloor
            | PcodeOpcode::FloatRound
            | PcodeOpcode::FloatEqual
            | PcodeOpcode::FloatNotEqual
            | PcodeOpcode::FloatLess
            | PcodeOpcode::FloatLessEqual
            | PcodeOpcode::FloatNan
            | PcodeOpcode::IntNegate
            | PcodeOpcode::BoolNegate
            | PcodeOpcode::Int2Comp
            | PcodeOpcode::IntCarry
            | PcodeOpcode::IntSCarry
            | PcodeOpcode::IntSBorrow
            | PcodeOpcode::PopCount
            | PcodeOpcode::LzCount
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
