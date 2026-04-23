//! Pcode (P-Code) intermediate representation from Ghidra
//!
//! This module provides Rust structures for Ghidra's Pcode IR,
//! enabling direct optimization at the Pcode level before C generation.

use serde::{de::Error as _, Deserialize, Serialize};

/// Pcode operation code (opcode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PcodeOpcode {
    // Data movement
    Copy,
    Load,
    Store,

    // Control flow
    Branch,
    CBranch,
    BranchInd,
    Call,
    CallInd,
    CallOther,
    Return,

    // Integer arithmetic
    IntEqual,
    IntNotEqual,
    IntSLess,
    IntSLessEqual,
    IntLess,
    IntLessEqual,
    IntZExt,
    IntSExt,
    IntAdd,
    IntSub,
    IntCarry,
    IntSCarry,
    IntSBorrow,
    Int2Comp,
    IntNegate,
    IntXor,
    IntAnd,
    IntOr,
    IntLeft,
    IntRight,
    IntSRight,
    IntMult,
    IntDiv,
    IntSDiv,
    IntRem,
    IntSRem,

    // Boolean
    BoolNegate,
    BoolXor,
    BoolAnd,
    BoolOr,

    // Floating point
    FloatEqual,
    FloatNotEqual,
    FloatLess,
    FloatLessEqual,
    FloatNan,
    FloatAdd,
    FloatDiv,
    FloatMult,
    FloatSub,
    FloatNeg,
    FloatAbs,
    FloatSqrt,
    FloatInt2Float,
    FloatFloat2Float,
    FloatTrunc,
    FloatCeil,
    FloatFloor,
    FloatRound,

    // Special
    MultiEqual,
    Indirect,
    Piece,
    SubPiece,
    Cast,
    PtrAdd,
    PtrSub,
    SegmentOp,
    CPoolRef,
    New,
    Insert,
    Extract,
    PopCount,

    Unknown,
}

/// Ghidra OpCode (1-based) to PcodeOpcode. Indices 0..=73; CPUI_COPY=1..CPUI_MAX=74.
/// Slot 45 unused in Ghidra; CPUI_LZCOUNT=73 maps to Unknown.
#[rustfmt::skip]
fn ghidra_opcode_to_rust(n: u32) -> PcodeOpcode {
    match n {
        1 => PcodeOpcode::Copy, 2 => PcodeOpcode::Load, 3 => PcodeOpcode::Store,
        4 => PcodeOpcode::Branch, 5 => PcodeOpcode::CBranch, 6 => PcodeOpcode::BranchInd,
        7 => PcodeOpcode::Call, 8 => PcodeOpcode::CallInd, 9 => PcodeOpcode::CallOther,
        10 => PcodeOpcode::Return,
        11 => PcodeOpcode::IntEqual, 12 => PcodeOpcode::IntNotEqual,
        13 => PcodeOpcode::IntSLess, 14 => PcodeOpcode::IntSLessEqual,
        15 => PcodeOpcode::IntLess, 16 => PcodeOpcode::IntLessEqual,
        17 => PcodeOpcode::IntZExt, 18 => PcodeOpcode::IntSExt,
        19 => PcodeOpcode::IntAdd, 20 => PcodeOpcode::IntSub,
        21 => PcodeOpcode::IntCarry, 22 => PcodeOpcode::IntSCarry,
        23 => PcodeOpcode::IntSBorrow, 24 => PcodeOpcode::Int2Comp,
        25 => PcodeOpcode::IntNegate, 26 => PcodeOpcode::IntXor,
        27 => PcodeOpcode::IntAnd, 28 => PcodeOpcode::IntOr,
        29 => PcodeOpcode::IntLeft, 30 => PcodeOpcode::IntRight,
        31 => PcodeOpcode::IntSRight, 32 => PcodeOpcode::IntMult,
        33 => PcodeOpcode::IntDiv, 34 => PcodeOpcode::IntSDiv,
        35 => PcodeOpcode::IntRem, 36 => PcodeOpcode::IntSRem,
        37 => PcodeOpcode::BoolNegate, 38 => PcodeOpcode::BoolXor,
        39 => PcodeOpcode::BoolAnd, 40 => PcodeOpcode::BoolOr,
        41 => PcodeOpcode::FloatEqual, 42 => PcodeOpcode::FloatNotEqual,
        43 => PcodeOpcode::FloatLess, 44 => PcodeOpcode::FloatLessEqual,
        46 => PcodeOpcode::FloatNan,
        47 => PcodeOpcode::FloatAdd, 48 => PcodeOpcode::FloatDiv,
        49 => PcodeOpcode::FloatMult, 50 => PcodeOpcode::FloatSub,
        51 => PcodeOpcode::FloatNeg, 52 => PcodeOpcode::FloatAbs,
        53 => PcodeOpcode::FloatSqrt, 54 => PcodeOpcode::FloatInt2Float,
        55 => PcodeOpcode::FloatFloat2Float, 56 => PcodeOpcode::FloatTrunc,
        57 => PcodeOpcode::FloatCeil, 58 => PcodeOpcode::FloatFloor,
        59 => PcodeOpcode::FloatRound,
        60 => PcodeOpcode::MultiEqual, 61 => PcodeOpcode::Indirect,
        62 => PcodeOpcode::Piece, 63 => PcodeOpcode::SubPiece,
        64 => PcodeOpcode::Cast, 65 => PcodeOpcode::PtrAdd,
        66 => PcodeOpcode::PtrSub, 67 => PcodeOpcode::SegmentOp,
        68 => PcodeOpcode::CPoolRef, 69 => PcodeOpcode::New,
        70 => PcodeOpcode::Insert, 71 => PcodeOpcode::Extract,
        72 => PcodeOpcode::PopCount,
        _ => PcodeOpcode::Unknown,
    }
}

/// PcodeOpcode to Ghidra OpCode u32 for flat serialization
#[rustfmt::skip]
fn rust_opcode_to_ghidra(o: PcodeOpcode) -> u32 {
    match o {
        PcodeOpcode::Copy => 1, PcodeOpcode::Load => 2, PcodeOpcode::Store => 3,
        PcodeOpcode::Branch => 4, PcodeOpcode::CBranch => 5, PcodeOpcode::BranchInd => 6,
        PcodeOpcode::Call => 7, PcodeOpcode::CallInd => 8, PcodeOpcode::CallOther => 9,
        PcodeOpcode::Return => 10,
        PcodeOpcode::IntEqual => 11, PcodeOpcode::IntNotEqual => 12,
        PcodeOpcode::IntSLess => 13, PcodeOpcode::IntSLessEqual => 14,
        PcodeOpcode::IntLess => 15, PcodeOpcode::IntLessEqual => 16,
        PcodeOpcode::IntZExt => 17, PcodeOpcode::IntSExt => 18,
        PcodeOpcode::IntAdd => 19, PcodeOpcode::IntSub => 20,
        PcodeOpcode::IntCarry => 21, PcodeOpcode::IntSCarry => 22,
        PcodeOpcode::IntSBorrow => 23, PcodeOpcode::Int2Comp => 24,
        PcodeOpcode::IntNegate => 25, PcodeOpcode::IntXor => 26,
        PcodeOpcode::IntAnd => 27, PcodeOpcode::IntOr => 28,
        PcodeOpcode::IntLeft => 29, PcodeOpcode::IntRight => 30,
        PcodeOpcode::IntSRight => 31, PcodeOpcode::IntMult => 32,
        PcodeOpcode::IntDiv => 33, PcodeOpcode::IntSDiv => 34,
        PcodeOpcode::IntRem => 35, PcodeOpcode::IntSRem => 36,
        PcodeOpcode::BoolNegate => 37, PcodeOpcode::BoolXor => 38,
        PcodeOpcode::BoolAnd => 39, PcodeOpcode::BoolOr => 40,
        PcodeOpcode::FloatEqual => 41, PcodeOpcode::FloatNotEqual => 42,
        PcodeOpcode::FloatLess => 43, PcodeOpcode::FloatLessEqual => 44,
        PcodeOpcode::FloatNan => 46,
        PcodeOpcode::FloatAdd => 47, PcodeOpcode::FloatDiv => 48,
        PcodeOpcode::FloatMult => 49, PcodeOpcode::FloatSub => 50,
        PcodeOpcode::FloatNeg => 51, PcodeOpcode::FloatAbs => 52,
        PcodeOpcode::FloatSqrt => 53, PcodeOpcode::FloatInt2Float => 54,
        PcodeOpcode::FloatFloat2Float => 55, PcodeOpcode::FloatTrunc => 56,
        PcodeOpcode::FloatCeil => 57, PcodeOpcode::FloatFloor => 58,
        PcodeOpcode::FloatRound => 59,
        PcodeOpcode::MultiEqual => 60, PcodeOpcode::Indirect => 61,
        PcodeOpcode::Piece => 62, PcodeOpcode::SubPiece => 63,
        PcodeOpcode::Cast => 64, PcodeOpcode::PtrAdd => 65,
        PcodeOpcode::PtrSub => 66, PcodeOpcode::SegmentOp => 67,
        PcodeOpcode::CPoolRef => 68, PcodeOpcode::New => 69,
        PcodeOpcode::Insert => 70, PcodeOpcode::Extract => 71,
        PcodeOpcode::PopCount => 72,
        PcodeOpcode::Unknown => 0,
    }
}

impl PcodeOpcode {
    pub fn from_flat_u32(n: u32) -> Self {
        ghidra_opcode_to_rust(n)
    }

    pub fn to_flat_u32(self) -> u32 {
        rust_opcode_to_ghidra(self)
    }
    /// Parse opcode from string (from JSON)
    pub fn parse(s: &str) -> Self {
        match s {
            "copy" => Self::Copy,
            "COPY" => Self::Copy,
            "load" => Self::Load,
            "LOAD" => Self::Load,
            "store" => Self::Store,
            "STORE" => Self::Store,
            "goto" => Self::Branch,
            "BRANCH" => Self::Branch,
            "if" => Self::CBranch,
            "CBRANCH" => Self::CBranch,
            "BRANCHIND" => Self::BranchInd,
            "call" => Self::Call,
            "CALL" => Self::Call,
            "callind" => Self::CallInd,
            "CALLIND" => Self::CallInd,
            "callother" => Self::CallOther,
            "CALLOTHER" => Self::CallOther,
            "return" => Self::Return,
            "RETURN" => Self::Return,
            "==" => Self::IntEqual,
            "INT_EQUAL" => Self::IntEqual,
            "!=" => Self::IntNotEqual,
            "INT_NOTEQUAL" => Self::IntNotEqual,
            "INT_SLESS" => Self::IntSLess,
            "INT_SLESSEQUAL" => Self::IntSLessEqual,
            "<" => Self::IntLess,
            "INT_LESS" => Self::IntLess,
            "INT_LESSEQUAL" => Self::IntLessEqual,
            "ZEXT" => Self::IntZExt,
            "INT_ZEXT" => Self::IntZExt,
            "SEXT" => Self::IntSExt,
            "INT_SEXT" => Self::IntSExt,
            "+" => Self::IntAdd,
            "INT_ADD" => Self::IntAdd,
            "-" => Self::IntSub,
            "INT_SUB" => Self::IntSub,
            "*" => Self::IntMult,
            "/" => Self::IntDiv,
            "%" => Self::IntRem,
            "CARRY" => Self::IntCarry,
            "INT_CARRY" => Self::IntCarry,
            "SCARRY" => Self::IntSCarry,
            "INT_SCARRY" => Self::IntSCarry,
            "SBORROW" => Self::IntSBorrow,
            "INT_SBORROW" => Self::IntSBorrow,
            "INT_2COMP" => Self::Int2Comp,
            "INT_NEGATE" => Self::IntNegate,
            "^" => Self::IntXor,
            "INT_XOR" => Self::IntXor,
            "&" => Self::IntAnd,
            "INT_AND" => Self::IntAnd,
            "|" => Self::IntOr,
            "INT_OR" => Self::IntOr,
            "<<" => Self::IntLeft,
            ">>" => Self::IntRight,
            "INT_LEFT" => Self::IntLeft,
            "INT_RIGHT" => Self::IntRight,
            "INT_SRIGHT" => Self::IntSRight,
            "INT_MULT" => Self::IntMult,
            "INT_DIV" => Self::IntDiv,
            "INT_SDIV" => Self::IntSDiv,
            "INT_REM" => Self::IntRem,
            "INT_SREM" => Self::IntSRem,
            "!" => Self::BoolNegate,
            "BOOL_NEGATE" => Self::BoolNegate,
            "&&" => Self::BoolAnd,
            "||" => Self::BoolOr,
            "~" => Self::IntNegate,
            "BOOL_XOR" => Self::BoolXor,
            "BOOL_AND" => Self::BoolAnd,
            "BOOL_OR" => Self::BoolOr,
            "FLOAT_EQUAL" => Self::FloatEqual,
            "FLOAT_NOTEQUAL" => Self::FloatNotEqual,
            "FLOAT_LESS" => Self::FloatLess,
            "FLOAT_LESSEQUAL" => Self::FloatLessEqual,
            "FLOAT_NAN" => Self::FloatNan,
            "FLOAT_ADD" => Self::FloatAdd,
            "FLOAT_DIV" => Self::FloatDiv,
            "FLOAT_MULT" => Self::FloatMult,
            "FLOAT_SUB" => Self::FloatSub,
            "FLOAT_NEG" => Self::FloatNeg,
            "FLOAT_ABS" => Self::FloatAbs,
            "FLOAT_SQRT" => Self::FloatSqrt,
            "FLOAT_INT2FLOAT" => Self::FloatInt2Float,
            "FLOAT_FLOAT2FLOAT" => Self::FloatFloat2Float,
            "FLOAT_TRUNC" => Self::FloatTrunc,
            "FLOAT_CEIL" => Self::FloatCeil,
            "FLOAT_FLOOR" => Self::FloatFloor,
            "FLOAT_ROUND" => Self::FloatRound,
            "MULTIEQUAL" => Self::MultiEqual,
            "INDIRECT" => Self::Indirect,
            "PIECE" => Self::Piece,
            "SUB" => Self::SubPiece,
            "SUBPIECE" => Self::SubPiece,
            "CAST" => Self::Cast,
            "PTRADD" => Self::PtrAdd,
            "PTRSUB" => Self::PtrSub,
            "SEGMENTOP" => Self::SegmentOp,
            "CPOOLREF" => Self::CPoolRef,
            "NEW" => Self::New,
            "INSERT" => Self::Insert,
            "EXTRACT" => Self::Extract,
            "syscall" => Self::CallOther,
            "SYSCALL" => Self::CallOther,
            "POPCOUNT" => Self::PopCount,
            _ => Self::Unknown,
        }
    }

    /// Check if this is a commutative operation (order of operands doesn't matter)
    pub fn is_commutative(&self) -> bool {
        matches!(
            self,
            Self::IntAdd
                | Self::IntMult
                | Self::IntAnd
                | Self::IntOr
                | Self::IntXor
                | Self::IntEqual
                | Self::IntNotEqual
                | Self::BoolAnd
                | Self::BoolOr
                | Self::BoolXor
                | Self::FloatAdd
                | Self::FloatMult
        )
    }

    /// Check if this is a comparison operation
    pub fn is_comparison(&self) -> bool {
        matches!(
            self,
            Self::IntEqual
                | Self::IntNotEqual
                | Self::IntLess
                | Self::IntLessEqual
                | Self::IntSLess
                | Self::IntSLessEqual
                | Self::FloatEqual
                | Self::FloatNotEqual
                | Self::FloatLess
                | Self::FloatLessEqual
        )
    }

    /// Get the inverse comparison (for optimization)
    pub fn inverse_comparison(&self) -> Option<Self> {
        match self {
            Self::IntEqual => Some(Self::IntNotEqual),
            Self::IntNotEqual => Some(Self::IntEqual),
            Self::IntLess => Some(Self::IntLessEqual), // !(a < b) => a >= b
            Self::IntLessEqual => Some(Self::IntLess), // !(a <= b) => a > b
            Self::IntSLess => Some(Self::IntSLessEqual),
            Self::IntSLessEqual => Some(Self::IntSLess),
            _ => None,
        }
    }

    /// Check if this is a control flow operation
    pub fn is_control_flow(&self) -> bool {
        matches!(
            self,
            Self::Branch
                | Self::CBranch
                | Self::BranchInd
                | Self::Call
                | Self::CallInd
                | Self::CallOther
                | Self::Return
        )
    }

    /// Check if this is a branch operation (not including calls)
    pub fn is_branch(&self) -> bool {
        matches!(self, Self::Branch | Self::CBranch | Self::BranchInd)
    }

    /// Check if this is a call operation
    pub fn is_call(&self) -> bool {
        matches!(self, Self::Call | Self::CallInd | Self::CallOther)
    }

    /// Check if this is a return operation
    pub fn is_return(&self) -> bool {
        matches!(self, Self::Return)
    }
}

impl std::str::FromStr for PcodeOpcode {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

/// Varnode - represents a value in Pcode (register, memory, constant, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Varnode {
    pub space_id: u64,     // Address space (0=const, 1=unique, 2=register, etc.)
    pub offset: u64,       // Offset within space
    pub size: u32,         // Size in bytes
    pub is_constant: bool, // Is this a constant value?
    pub constant_val: i64, // Constant value if is_constant
}

impl Varnode {
    /// Create a constant varnode
    pub fn constant(val: i64, size: u32) -> Self {
        Self {
            space_id: 0,
            offset: val as u64,
            size,
            is_constant: true,
            constant_val: val,
        }
    }

    /// Check if this is zero
    pub fn is_zero(&self) -> bool {
        self.is_constant && self.constant_val == 0
    }

    /// Check if this is one
    pub fn is_one(&self) -> bool {
        self.is_constant && self.constant_val == 1
    }

    /// Check if this is all bits set (e.g., 0xFF for 1 byte, 0xFFFFFFFF for 4 bytes)
    pub fn is_all_ones(&self) -> bool {
        if !self.is_constant {
            return false;
        }
        let mask = match self.size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFF_FFFF,
            8 => -1i64,
            _ => return false,
        };
        self.constant_val == mask
    }
}

/// Pcode operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeOp {
    pub seq_num: u32, // Sequential number in basic block
    pub opcode: PcodeOpcode,
    pub address: u64, // Original instruction address
    pub output: Option<Varnode>,
    pub inputs: Vec<Varnode>,
    #[serde(default)]
    pub asm_mnemonic: Option<String>, // Assembly instruction mnemonic
}

/// Architecture-independent structural contract for a P-code opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PcodeShapeContract {
    pub output: PcodeOutputContract,
    pub inputs: PcodeInputContract,
    pub category: PcodeOpCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcodeOutputContract {
    Required,
    Forbidden,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcodeInputContract {
    Exact(usize),
    Range { min: usize, max: Option<usize> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcodeOpCategory {
    DataFlow,
    ControlFlow,
    SideEffect,
    Phi,
    Metadata,
}

/// Structural P-code validation failure. Semantic parity mistakes are tracked
/// separately; these errors mean the operation shape itself is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PcodeValidationError {
    UnknownOpcode {
        block_index: Option<u32>,
        op_index: Option<usize>,
        seq_num: u32,
    },
    MissingOutput {
        opcode: PcodeOpcode,
        block_index: Option<u32>,
        op_index: Option<usize>,
        seq_num: u32,
    },
    UnexpectedOutput {
        opcode: PcodeOpcode,
        block_index: Option<u32>,
        op_index: Option<usize>,
        seq_num: u32,
    },
    WrongInputCount {
        opcode: PcodeOpcode,
        expected: PcodeInputContract,
        actual: usize,
        block_index: Option<u32>,
        op_index: Option<usize>,
        seq_num: u32,
    },
    InvalidVarnodeSize {
        opcode: PcodeOpcode,
        role: &'static str,
        size: u32,
        block_index: Option<u32>,
        op_index: Option<usize>,
        seq_num: u32,
    },
}

impl std::fmt::Display for PcodeValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOpcode { seq_num, .. } => {
                write!(f, "InvalidPcodeShape: unknown opcode at seq {seq_num}")
            }
            Self::MissingOutput {
                opcode, seq_num, ..
            } => write!(
                f,
                "InvalidPcodeShape: {opcode:?} at seq {seq_num} requires an output"
            ),
            Self::UnexpectedOutput {
                opcode, seq_num, ..
            } => write!(
                f,
                "InvalidPcodeShape: {opcode:?} at seq {seq_num} forbids an output"
            ),
            Self::WrongInputCount {
                opcode,
                expected,
                actual,
                seq_num,
                ..
            } => write!(
                f,
                "InvalidPcodeShape: {opcode:?} at seq {seq_num} has {actual} inputs, expected {expected:?}"
            ),
            Self::InvalidVarnodeSize {
                opcode,
                role,
                size,
                seq_num,
                ..
            } => write!(
                f,
                "InvalidPcodeShape: {opcode:?} at seq {seq_num} has invalid {role} varnode size {size}"
            ),
        }
    }
}

impl std::error::Error for PcodeValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPcodeFunction {
    inner: PcodeFunction,
}

impl ValidatedPcodeFunction {
    pub fn new(function: PcodeFunction) -> Result<Self, PcodeValidationError> {
        function.validate()?;
        Ok(Self { inner: function })
    }

    #[must_use]
    pub fn as_raw(&self) -> &PcodeFunction {
        &self.inner
    }

    #[must_use]
    pub fn into_raw(self) -> PcodeFunction {
        self.inner
    }
}

impl PcodeInputContract {
    #[must_use]
    pub fn accepts(self, actual: usize) -> bool {
        match self {
            Self::Exact(expected) => actual == expected,
            Self::Range { min, max } => actual >= min && max.is_none_or(|max| actual <= max),
        }
    }
}

impl PcodeOpcode {
    #[must_use]
    pub const fn shape_contract(self) -> Option<PcodeShapeContract> {
        use PcodeInputContract::{Exact, Range};
        use PcodeOpCategory::{ControlFlow, DataFlow, Metadata, Phi, SideEffect};
        use PcodeOutputContract::{Forbidden, Optional, Required};

        let binary_data = PcodeShapeContract {
            output: Required,
            inputs: Exact(2),
            category: DataFlow,
        };
        let unary_data = PcodeShapeContract {
            output: Required,
            inputs: Exact(1),
            category: DataFlow,
        };

        Some(match self {
            Self::Unknown => return None,
            Self::Copy | Self::Cast | Self::IntZExt | Self::IntSExt | Self::Int2Comp
            | Self::IntNegate | Self::BoolNegate | Self::FloatNeg | Self::FloatAbs
            | Self::FloatSqrt | Self::FloatInt2Float | Self::FloatFloat2Float
            | Self::FloatTrunc | Self::FloatCeil | Self::FloatFloor | Self::FloatRound
            | Self::PopCount => unary_data,
            Self::IntEqual | Self::IntNotEqual | Self::IntSLess | Self::IntSLessEqual
            | Self::IntLess | Self::IntLessEqual | Self::IntAdd | Self::IntSub
            | Self::IntCarry | Self::IntSCarry | Self::IntSBorrow | Self::IntXor
            | Self::IntAnd | Self::IntOr | Self::IntLeft | Self::IntRight
            | Self::IntSRight | Self::IntMult | Self::IntDiv | Self::IntSDiv
            | Self::IntRem | Self::IntSRem | Self::BoolXor | Self::BoolAnd
            | Self::BoolOr | Self::FloatEqual | Self::FloatNotEqual | Self::FloatLess
            | Self::FloatLessEqual | Self::FloatAdd | Self::FloatDiv | Self::FloatMult
            | Self::FloatSub | Self::Piece | Self::SubPiece | Self::PtrSub => binary_data,
            Self::Load => PcodeShapeContract {
                output: Required,
                inputs: Exact(2),
                category: SideEffect,
            },
            Self::Store => PcodeShapeContract {
                output: Forbidden,
                // Fission's raw DTO historically stores the address-space either
                // implicitly (addr, value) or explicitly (space, addr, value).
                inputs: Range {
                    min: 2,
                    max: Some(3),
                },
                category: SideEffect,
            },
            Self::Branch | Self::BranchInd => PcodeShapeContract {
                output: Forbidden,
                inputs: Range {
                    min: 1,
                    max: Some(2),
                },
                category: ControlFlow,
            },
            Self::Call | Self::CallInd => PcodeShapeContract {
                output: Forbidden,
                // Fission carries recovered call arguments after the target in
                // the compatibility DTO. The shape invariant is target-present
                // and no output; arity strictness for arguments belongs above.
                inputs: Range { min: 1, max: None },
                category: ControlFlow,
            },
            // Fission historically permits RETURN without an explicit destination.
            // The important structural invariant here is that RETURN never writes.
            Self::Return => PcodeShapeContract {
                output: Forbidden,
                inputs: Range {
                    min: 0,
                    max: Some(2),
                },
                category: ControlFlow,
            },
            Self::CBranch => PcodeShapeContract {
                output: Forbidden,
                inputs: Range {
                    min: 2,
                    max: Some(3),
                },
                category: ControlFlow,
            },
            Self::CallOther => PcodeShapeContract {
                output: Optional,
                inputs: Range { min: 1, max: None },
                category: ControlFlow,
            },
            Self::MultiEqual => PcodeShapeContract {
                output: Required,
                inputs: Range { min: 1, max: None },
                category: Phi,
            },
            Self::Indirect => PcodeShapeContract {
                output: Required,
                inputs: Exact(2),
                category: Metadata,
            },
            Self::PtrAdd => PcodeShapeContract {
                output: Required,
                inputs: Exact(3),
                category: DataFlow,
            },
            Self::SegmentOp => PcodeShapeContract {
                output: Required,
                inputs: Range { min: 2, max: None },
                category: DataFlow,
            },
            Self::CPoolRef | Self::New => PcodeShapeContract {
                output: Required,
                inputs: Range { min: 1, max: None },
                category: DataFlow,
            },
            Self::Insert => PcodeShapeContract {
                output: Required,
                inputs: Exact(4),
                category: DataFlow,
            },
            Self::Extract => PcodeShapeContract {
                output: Required,
                inputs: Exact(3),
                category: DataFlow,
            },
            Self::FloatNan => unary_data,
        })
    }
}

impl PcodeOp {
    pub fn validate_shape_at(
        &self,
        block_index: Option<u32>,
        op_index: Option<usize>,
    ) -> Result<(), PcodeValidationError> {
        let Some(contract) = self.opcode.shape_contract() else {
            return Err(PcodeValidationError::UnknownOpcode {
                block_index,
                op_index,
                seq_num: self.seq_num,
            });
        };

        match (contract.output, self.output.as_ref()) {
            (PcodeOutputContract::Required, None) => {
                return Err(PcodeValidationError::MissingOutput {
                    opcode: self.opcode,
                    block_index,
                    op_index,
                    seq_num: self.seq_num,
                });
            }
            (PcodeOutputContract::Forbidden, Some(_)) => {
                return Err(PcodeValidationError::UnexpectedOutput {
                    opcode: self.opcode,
                    block_index,
                    op_index,
                    seq_num: self.seq_num,
                });
            }
            _ => {}
        }

        if !contract.inputs.accepts(self.inputs.len()) {
            return Err(PcodeValidationError::WrongInputCount {
                opcode: self.opcode,
                expected: contract.inputs,
                actual: self.inputs.len(),
                block_index,
                op_index,
                seq_num: self.seq_num,
            });
        }

        if let Some(output) = &self.output {
            if output.size == 0 {
                return Err(PcodeValidationError::InvalidVarnodeSize {
                    opcode: self.opcode,
                    role: "output",
                    size: output.size,
                    block_index,
                    op_index,
                    seq_num: self.seq_num,
                });
            }
        }
        for input in &self.inputs {
            if input.size == 0 {
                return Err(PcodeValidationError::InvalidVarnodeSize {
                    opcode: self.opcode,
                    role: "input",
                    size: input.size,
                    block_index,
                    op_index,
                    seq_num: self.seq_num,
                });
            }
        }

        Ok(())
    }

    pub fn validate_shape(&self) -> Result<(), PcodeValidationError> {
        self.validate_shape_at(None, None)
    }
}

/// Basic block of Pcode operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeBasicBlock {
    pub index: u32,
    pub start_address: u64,
    #[serde(default)]
    pub successors: Vec<u32>,
    pub ops: Vec<PcodeOp>,
}

/// Complete Pcode representation of a function
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcodeFunction {
    pub blocks: Vec<PcodeBasicBlock>,
}

impl PcodeFunction {
    pub fn validate(&self) -> Result<(), PcodeValidationError> {
        for block in &self.blocks {
            for (op_index, op) in block.ops.iter().enumerate() {
                op.validate_shape_at(Some(block.index), Some(op_index))?;
            }
        }
        Ok(())
    }

    pub fn into_validated(self) -> Result<ValidatedPcodeFunction, PcodeValidationError> {
        ValidatedPcodeFunction::new(self)
    }

    #[must_use]
    pub fn has_indirect_control_flow(&self) -> bool {
        self.blocks
            .iter()
            .flat_map(|block| block.ops.iter())
            .any(|op| matches!(op.opcode, PcodeOpcode::CallInd | PcodeOpcode::BranchInd))
    }

    /// Parse Pcode from JSON (returned by C++ FFI)
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct JsonRoot {
            blocks: Vec<JsonBlock>,
        }

        #[derive(Deserialize)]
        struct JsonBlock {
            index: u32,
            start_addr: String,
            #[serde(default)]
            successors: Vec<u32>,
            ops: Vec<JsonOp>,
        }

        #[derive(Deserialize)]
        struct JsonOp {
            seq: u32,
            opcode: String,
            addr: String,
            output: Option<JsonVarnode>,
            inputs: Vec<JsonVarnode>,
            #[serde(default)]
            asm: Option<String>,
        }

        #[derive(Deserialize)]
        struct JsonVarnode {
            space: u64,
            offset: String,
            size: u32,
            const_val: Option<serde_json::Value>,
        }

        let root: JsonRoot = serde_json::from_str(json)?;

        let blocks = root
            .blocks
            .into_iter()
            .map(|jb| {
                let start_address = parse_hex_addr(&jb.start_addr);
                let ops = jb
                    .ops
                    .into_iter()
                    .map(|jo| {
                        let address = parse_hex_addr(&jo.addr);
                        let opcode = PcodeOpcode::parse(&jo.opcode);
                        let output = jo.output.map(|jv| Varnode {
                            space_id: jv.space,
                            offset: parse_hex_addr(&jv.offset),
                            size: jv.size,
                            is_constant: jv.const_val.is_some(),
                            constant_val: parse_json_const_val(jv.const_val.as_ref()).unwrap_or(0),
                        });
                        let inputs = jo
                            .inputs
                            .into_iter()
                            .map(|jv| Varnode {
                                space_id: jv.space,
                                offset: parse_hex_addr(&jv.offset),
                                size: jv.size,
                                is_constant: jv.const_val.is_some(),
                                constant_val: parse_json_const_val(jv.const_val.as_ref())
                                    .unwrap_or(0),
                            })
                            .collect();

                        PcodeOp {
                            seq_num: jo.seq,
                            opcode,
                            address,
                            output,
                            inputs,
                            asm_mnemonic: jo.asm,
                        }
                    })
                    .collect();

                PcodeBasicBlock {
                    index: jb.index,
                    start_address,
                    successors: jb.successors,
                    ops,
                }
            })
            .collect();

        let function = PcodeFunction { blocks };
        function
            .validate()
            .map_err(|err| serde_json::Error::custom(err.to_string()))?;
        Ok(function)
    }

    /// Parse Pcode from flat binary format (zero-copy friendly).
    /// Format: FPCD magic(4) version(1) num_blocks(4) [block...]
    /// Block: index(4) start_addr(8) num_ops(4) [op...]
    /// Op: seq(4) opcode(4) addr(8) has_out(1) out_vn(32 if has_out) num_in(4) in_vn(32 each)
    /// Varnode: space_id(8) offset(8) size(4) is_const(1) _pad(3) const_val(8) = 32 bytes
    pub fn from_flat_bytes(bytes: &[u8]) -> Result<Self, FlatFormatError> {
        const MAGIC: &[u8; 4] = b"FPCD";
        const VARNODE_SIZE: usize = 32;

        if bytes.len() < 4 + 1 + 4 {
            return Err(FlatFormatError::TooShort);
        }
        if &bytes[0..4] != MAGIC {
            return Err(FlatFormatError::BadMagic);
        }
        let _version = bytes[4];
        let num_blocks = u32::from_le_bytes(bytes[5..9].try_into().unwrap()) as usize;
        let mut pos = 9;
        let mut blocks = Vec::with_capacity(num_blocks);

        for _ in 0..num_blocks {
            if pos + 4 + 8 + 4 > bytes.len() {
                return Err(FlatFormatError::Truncated);
            }
            let index = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
            pos += 4;
            let start_address = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
            pos += 8;
            let num_ops = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;

            let mut ops = Vec::with_capacity(num_ops);
            for _ in 0..num_ops {
                if pos + 4 + 4 + 8 + 1 > bytes.len() {
                    return Err(FlatFormatError::Truncated);
                }
                let seq_num = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
                pos += 4;
                let opcode = PcodeOpcode::from_flat_u32(u32::from_le_bytes(
                    bytes[pos..pos + 4].try_into().unwrap(),
                ));
                pos += 4;
                let address = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
                pos += 8;
                let has_output = bytes[pos] != 0;
                pos += 1;

                let output = if has_output {
                    if pos + VARNODE_SIZE > bytes.len() {
                        return Err(FlatFormatError::Truncated);
                    }
                    let vn = read_varnode(&bytes[pos..pos + VARNODE_SIZE]);
                    pos += VARNODE_SIZE;
                    Some(vn)
                } else {
                    None
                };

                if pos + 4 > bytes.len() {
                    return Err(FlatFormatError::Truncated);
                }
                let num_inputs =
                    u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
                pos += 4;

                let mut inputs = Vec::with_capacity(num_inputs);
                for _ in 0..num_inputs {
                    if pos + VARNODE_SIZE > bytes.len() {
                        return Err(FlatFormatError::Truncated);
                    }
                    inputs.push(read_varnode(&bytes[pos..pos + VARNODE_SIZE]));
                    pos += VARNODE_SIZE;
                }

                let op = PcodeOp {
                    seq_num,
                    opcode,
                    address,
                    output,
                    inputs,
                    asm_mnemonic: None,
                };
                op.validate_shape_at(Some(index), Some(ops.len()))
                    .map_err(FlatFormatError::InvalidPcodeShape)?;
                ops.push(op);
            }
            blocks.push(PcodeBasicBlock {
                index,
                start_address,
                successors: vec![],
                ops,
            });
        }

        let function = PcodeFunction { blocks };
        function
            .validate()
            .map_err(FlatFormatError::InvalidPcodeShape)?;
        Ok(function)
    }

    /// Serialize to flat binary format.
    pub fn to_flat_bytes(&self) -> Vec<u8> {
        const MAGIC: &[u8; 4] = b"FPCD";

        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.push(1u8); // version
        out.extend_from_slice(&(self.blocks.len() as u32).to_le_bytes());

        for block in &self.blocks {
            out.extend_from_slice(&block.index.to_le_bytes());
            out.extend_from_slice(&block.start_address.to_le_bytes());
            out.extend_from_slice(&(block.ops.len() as u32).to_le_bytes());

            for op in &block.ops {
                out.extend_from_slice(&op.seq_num.to_le_bytes());
                out.extend_from_slice(&op.opcode.to_flat_u32().to_le_bytes());
                out.extend_from_slice(&op.address.to_le_bytes());
                if let Some(ref vn) = op.output {
                    out.push(1);
                    write_varnode(&mut out, vn);
                } else {
                    out.push(0);
                }
                out.extend_from_slice(&(op.inputs.len() as u32).to_le_bytes());
                for vn in &op.inputs {
                    write_varnode(&mut out, vn);
                }
            }
        }
        out
    }

    /// Get all operations across all blocks
    pub fn all_ops(&self) -> impl Iterator<Item = &PcodeOp> {
        self.blocks.iter().flat_map(|b| b.ops.iter())
    }

    /// Get mutable access to all operations
    pub fn all_ops_mut(&mut self) -> impl Iterator<Item = &mut PcodeOp> {
        self.blocks.iter_mut().flat_map(|b| b.ops.iter_mut())
    }
}

#[must_use]
pub fn pcode_has_indirect_control_flow(pcode: &PcodeFunction) -> bool {
    pcode.has_indirect_control_flow()
}

fn parse_hex_addr(s: &str) -> u64 {
    let s = s.trim_start_matches("0x");
    u64::from_str_radix(s, 16).unwrap_or(0)
}

fn parse_json_const_val(value: Option<&serde_json::Value>) -> Option<i64> {
    match value? {
        serde_json::Value::Number(num) => {
            if let Some(v) = num.as_i64() {
                Some(v)
            } else {
                num.as_u64().map(|v| v as i64)
            }
        }
        _ => None,
    }
}

/// Error from flat format parsing
#[derive(Debug, Clone)]
pub enum FlatFormatError {
    TooShort,
    BadMagic,
    Truncated,
    InvalidPcodeShape(PcodeValidationError),
}

impl std::fmt::Display for FlatFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => write!(f, "Flat buffer too short"),
            Self::BadMagic => write!(f, "Invalid flat format magic"),
            Self::Truncated => write!(f, "Truncated flat buffer"),
            Self::InvalidPcodeShape(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for FlatFormatError {}

const VARNODE_FLAT_SIZE: usize = 32;

fn read_varnode(bytes: &[u8]) -> Varnode {
    debug_assert!(bytes.len() >= VARNODE_FLAT_SIZE);
    let space_id = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let offset = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let size = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let is_constant = bytes[20] != 0;
    let constant_val = i64::from_le_bytes(bytes[24..32].try_into().unwrap());
    Varnode {
        space_id,
        offset,
        size,
        is_constant,
        constant_val,
    }
}

fn write_varnode(out: &mut Vec<u8>, vn: &Varnode) {
    out.extend_from_slice(&vn.space_id.to_le_bytes());
    out.extend_from_slice(&vn.offset.to_le_bytes());
    out.extend_from_slice(&vn.size.to_le_bytes());
    out.push(if vn.is_constant { 1 } else { 0 });
    out.extend_from_slice(&[0u8; 3]); // padding
    out.extend_from_slice(&vn.constant_val.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_parse() {
        assert_eq!(PcodeOpcode::parse("INT_ADD"), PcodeOpcode::IntAdd);
        assert_eq!(PcodeOpcode::parse("+"), PcodeOpcode::IntAdd);
        assert_eq!(PcodeOpcode::parse("INT_XOR"), PcodeOpcode::IntXor);
        assert_eq!(PcodeOpcode::parse("COPY"), PcodeOpcode::Copy);
        assert_eq!(PcodeOpcode::parse("goto"), PcodeOpcode::Branch);
        assert_eq!(PcodeOpcode::parse("return"), PcodeOpcode::Return);
        assert_eq!(PcodeOpcode::parse("callind"), PcodeOpcode::CallInd);
        assert_eq!(PcodeOpcode::parse("=="), PcodeOpcode::IntEqual);
        assert_eq!(PcodeOpcode::parse("&"), PcodeOpcode::IntAnd);
        assert_eq!(PcodeOpcode::parse("*"), PcodeOpcode::IntMult);
        assert_eq!(PcodeOpcode::parse("/"), PcodeOpcode::IntDiv);
        assert_eq!(PcodeOpcode::parse("%"), PcodeOpcode::IntRem);
        assert_eq!(PcodeOpcode::parse("<<"), PcodeOpcode::IntLeft);
        assert_eq!(PcodeOpcode::parse(">>"), PcodeOpcode::IntRight);
        assert_eq!(PcodeOpcode::parse("!"), PcodeOpcode::BoolNegate);
        assert_eq!(PcodeOpcode::parse("&&"), PcodeOpcode::BoolAnd);
        assert_eq!(PcodeOpcode::parse("||"), PcodeOpcode::BoolOr);
        assert_eq!(PcodeOpcode::parse("~"), PcodeOpcode::IntNegate);
        assert_eq!(PcodeOpcode::parse("SUB"), PcodeOpcode::SubPiece);
        assert_eq!(PcodeOpcode::parse("syscall"), PcodeOpcode::CallOther);
        assert_eq!(PcodeOpcode::parse("ZEXT"), PcodeOpcode::IntZExt);
    }

    #[test]
    fn test_opcode_is_commutative() {
        assert!(PcodeOpcode::IntAdd.is_commutative());
        assert!(PcodeOpcode::IntXor.is_commutative());
        assert!(!PcodeOpcode::IntSub.is_commutative());
    }

    #[test]
    fn test_varnode_constants() {
        let zero = Varnode::constant(0, 4);
        assert!(zero.is_zero());
        assert!(!zero.is_one());

        let one = Varnode::constant(1, 4);
        assert!(one.is_one());
        assert!(!one.is_zero());
    }

    fn op(
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address: 0x1000,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn pcode_shape_contract_rejects_invalid_structures() {
        let out = Varnode {
            space_id: 1,
            offset: 0x100,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let lhs = Varnode {
            space_id: 1,
            offset: 0x108,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let rhs = Varnode::constant(1, 8);

        assert!(matches!(
            op(PcodeOpcode::IntAdd, Some(out.clone()), vec![lhs.clone()])
                .validate_shape()
                .unwrap_err(),
            PcodeValidationError::WrongInputCount { .. }
        ));
        assert!(matches!(
            op(PcodeOpcode::Copy, None, vec![lhs.clone()])
                .validate_shape()
                .unwrap_err(),
            PcodeValidationError::MissingOutput { .. }
        ));
        assert!(matches!(
            op(
                PcodeOpcode::Store,
                Some(out.clone()),
                vec![Varnode::constant(0, 8), lhs.clone(), rhs.clone()]
            )
            .validate_shape()
            .unwrap_err(),
            PcodeValidationError::UnexpectedOutput { .. }
        ));
        assert!(matches!(
            op(PcodeOpcode::CBranch, None, vec![rhs.clone()])
                .validate_shape()
                .unwrap_err(),
            PcodeValidationError::WrongInputCount { .. }
        ));
        assert!(matches!(
            op(PcodeOpcode::Return, Some(out), Vec::new())
                .validate_shape()
                .unwrap_err(),
            PcodeValidationError::UnexpectedOutput { .. }
        ));
        assert!(matches!(
            op(PcodeOpcode::Unknown, None, vec![lhs])
                .validate_shape()
                .unwrap_err(),
            PcodeValidationError::UnknownOpcode { .. }
        ));
    }

    #[test]
    fn pcode_shape_contract_accepts_core_valid_shapes() {
        let out = Varnode {
            space_id: 1,
            offset: 0x100,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let lhs = Varnode {
            space_id: 1,
            offset: 0x108,
            size: 8,
            is_constant: false,
            constant_val: 0,
        };
        let rhs = Varnode::constant(1, 8);

        op(PcodeOpcode::IntAdd, Some(out.clone()), vec![lhs.clone(), rhs.clone()])
            .validate_shape()
            .expect("valid int add shape");
        op(PcodeOpcode::Copy, Some(out.clone()), vec![lhs.clone()])
            .validate_shape()
            .expect("valid copy shape");
        op(
            PcodeOpcode::Store,
            None,
            vec![Varnode::constant(0, 8), lhs.clone(), rhs.clone()],
        )
        .validate_shape()
        .expect("valid store shape");
        op(PcodeOpcode::CBranch, None, vec![rhs.clone(), lhs])
            .validate_shape()
            .expect("valid cbranch shape");
        op(PcodeOpcode::Return, None, Vec::new())
            .validate_shape()
            .expect("valid transitional return shape");
        op(PcodeOpcode::Return, None, vec![rhs])
            .validate_shape()
            .expect("valid explicit return target shape");
    }

    /// Phase C regression: Flat format round-trip must produce identical PcodeFunction.
    #[test]
    fn test_flat_roundtrip_equivalence() {
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::IntXor,
                    address: 0x1000,
                    output: Some(Varnode {
                        space_id: 1,
                        offset: 0x100,
                        size: 4,
                        is_constant: false,
                        constant_val: 0,
                    }),
                    inputs: vec![
                        Varnode {
                            space_id: 2,
                            offset: 0x10,
                            size: 4,
                            is_constant: false,
                            constant_val: 0,
                        },
                        Varnode::constant(0, 4),
                    ],
                    asm_mnemonic: None,
                }],
            }],
        };
        let flat = func.to_flat_bytes();
        let restored = PcodeFunction::from_flat_bytes(&flat).expect("flat parse");
        assert_eq!(
            func, restored,
            "Flat round-trip must preserve PcodeFunction"
        );
    }

    /// Phase C regression: JSON and Flat paths must produce equivalent PcodeFunction.
    #[test]
    fn test_flat_vs_json_optimization_equivalence() {
        let json = r#"{"blocks":[{"index":0,"start_addr":"0x1000","ops":[{"seq":0,"opcode":"INT_XOR","addr":"0x1000","output":{"space":1,"offset":"0x100","size":4},"inputs":[{"space":2,"offset":"0x10","size":4},{"space":0,"offset":"0x0","size":4,"const_val":0}]}]}]}"#;
        let from_json = PcodeFunction::from_json(json).expect("json parse");
        let flat = from_json.to_flat_bytes();
        let from_flat = PcodeFunction::from_flat_bytes(&flat).expect("flat parse");
        assert_eq!(from_json.blocks.len(), from_flat.blocks.len());
        for (j, f) in from_json.blocks.iter().zip(from_flat.blocks.iter()) {
            assert_eq!(j.index, f.index);
            assert_eq!(j.start_address, f.start_address);
            assert_eq!(j.ops.len(), f.ops.len());
            for (jo, fo) in j.ops.iter().zip(f.ops.iter()) {
                assert_eq!(jo.seq_num, fo.seq_num);
                assert_eq!(jo.opcode, fo.opcode);
                assert_eq!(jo.address, fo.address);
                assert_eq!(jo.output, fo.output);
                assert_eq!(jo.inputs, fo.inputs);
            }
        }
    }

    #[test]
    fn test_json_parses_wrapped_negative_const_values() {
        let json = r#"{"blocks":[{"index":0,"start_addr":"0x1000","ops":[{"seq":0,"opcode":"INT_ADD","addr":"0x1000","output":{"space":1,"offset":"0x100","size":8},"inputs":[{"space":2,"offset":"0x20","size":8},{"space":0,"offset":"0xfffffffffffffffc","size":8,"const_val":18446744073709551612}]}]}]}"#;
        let parsed = PcodeFunction::from_json(json).expect("json parse");
        assert_eq!(parsed.blocks[0].ops[0].inputs[1].constant_val, -4);
    }

    #[test]
    fn json_import_rejects_invalid_pcode_shape() {
        let json = r#"{"blocks":[{"index":0,"start_addr":"0x1000","ops":[{"seq":0,"opcode":"INT_ADD","addr":"0x1000","output":{"space":1,"offset":"0x100","size":8},"inputs":[{"space":2,"offset":"0x20","size":8}]}]}]}"#;
        let err = PcodeFunction::from_json(json).expect_err("invalid pcode must be rejected");
        assert!(err.to_string().contains("InvalidPcodeShape"));
    }
}
