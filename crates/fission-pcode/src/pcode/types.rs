//! Pcode (P-Code) intermediate representation from Ghidra
//!
//! This module provides Rust structures for Ghidra's Pcode IR,
//! enabling direct optimization at the Pcode level before C generation.

use serde::{Deserialize, Serialize};

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

impl PcodeOpcode {
    /// Parse opcode from string (from JSON)
    pub fn parse(s: &str) -> Self {
        match s {
            "COPY" => Self::Copy,
            "LOAD" => Self::Load,
            "STORE" => Self::Store,
            "BRANCH" => Self::Branch,
            "CBRANCH" => Self::CBranch,
            "BRANCHIND" => Self::BranchInd,
            "CALL" => Self::Call,
            "CALLIND" => Self::CallInd,
            "CALLOTHER" => Self::CallOther,
            "RETURN" => Self::Return,
            "INT_EQUAL" => Self::IntEqual,
            "INT_NOTEQUAL" => Self::IntNotEqual,
            "INT_SLESS" => Self::IntSLess,
            "INT_SLESSEQUAL" => Self::IntSLessEqual,
            "INT_LESS" => Self::IntLess,
            "INT_LESSEQUAL" => Self::IntLessEqual,
            "INT_ZEXT" => Self::IntZExt,
            "INT_SEXT" => Self::IntSExt,
            "INT_ADD" => Self::IntAdd,
            "INT_SUB" => Self::IntSub,
            "INT_CARRY" => Self::IntCarry,
            "INT_SCARRY" => Self::IntSCarry,
            "INT_SBORROW" => Self::IntSBorrow,
            "INT_2COMP" => Self::Int2Comp,
            "INT_NEGATE" => Self::IntNegate,
            "INT_XOR" => Self::IntXor,
            "INT_AND" => Self::IntAnd,
            "INT_OR" => Self::IntOr,
            "INT_LEFT" => Self::IntLeft,
            "INT_RIGHT" => Self::IntRight,
            "INT_SRIGHT" => Self::IntSRight,
            "INT_MULT" => Self::IntMult,
            "INT_DIV" => Self::IntDiv,
            "INT_SDIV" => Self::IntSDiv,
            "INT_REM" => Self::IntRem,
            "INT_SREM" => Self::IntSRem,
            "BOOL_NEGATE" => Self::BoolNegate,
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
            "SUBPIECE" => Self::SubPiece,
            "CAST" => Self::Cast,
            "PTRADD" => Self::PtrAdd,
            "PTRSUB" => Self::PtrSub,
            "SEGMENTOP" => Self::SegmentOp,
            "CPOOLREF" => Self::CPoolRef,
            "NEW" => Self::New,
            "INSERT" => Self::Insert,
            "EXTRACT" => Self::Extract,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcodeOp {
    pub seq_num: u32, // Sequential number in basic block
    pub opcode: PcodeOpcode,
    pub address: u64, // Original instruction address
    pub output: Option<Varnode>,
    pub inputs: Vec<Varnode>,
    #[serde(default)]
    pub asm_mnemonic: Option<String>, // Assembly instruction mnemonic
}

/// Basic block of Pcode operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcodeBasicBlock {
    pub index: u32,
    pub start_address: u64,
    pub ops: Vec<PcodeOp>,
}

/// Complete Pcode representation of a function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcodeFunction {
    pub blocks: Vec<PcodeBasicBlock>,
}

impl PcodeFunction {
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
            const_val: Option<i64>,
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
                            constant_val: jv.const_val.unwrap_or(0),
                        });
                        let inputs = jo
                            .inputs
                            .into_iter()
                            .map(|jv| Varnode {
                                space_id: jv.space,
                                offset: parse_hex_addr(&jv.offset),
                                size: jv.size,
                                is_constant: jv.const_val.is_some(),
                                constant_val: jv.const_val.unwrap_or(0),
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
                    ops,
                }
            })
            .collect();

        Ok(PcodeFunction { blocks })
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

fn parse_hex_addr(s: &str) -> u64 {
    let s = s.trim_start_matches("0x");
    u64::from_str_radix(s, 16).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_parse() {
        assert_eq!(PcodeOpcode::parse("INT_ADD"), PcodeOpcode::IntAdd);
        assert_eq!(PcodeOpcode::parse("INT_XOR"), PcodeOpcode::IntXor);
        assert_eq!(PcodeOpcode::parse("COPY"), PcodeOpcode::Copy);
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
}
