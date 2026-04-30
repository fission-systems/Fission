impl Default for CompiledSlaDecodeStatus {
    fn default() -> Self {
        Self::Decoded
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDisplayOperand {
    pub operand_index: usize,
    pub kind: CompiledDisplayOperandKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledDisplayOperandKind {
    Generic,
    Subtable,
    ValueHex,
    NameTable(Vec<String>),
    ValueMap(Vec<i64>),
    VarnodeList(Vec<String>),
}

impl CompiledDisplayTemplate {
    pub fn empty() -> Self {
        Self {
            constructor_hash: 0,
            pieces: Vec::new(),
            first_whitespace: None,
            flowthru_operand_index: None,
            display: String::new(),
        }
    }

    pub fn fallback(display: String) -> Self {
        Self {
            display,
            ..Self::empty()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDecisionTree {
    pub root_node_index: usize,
    pub root_buckets: Vec<CompiledDecisionBucket>,
    pub nodes: Vec<CompiledDecisionNode>,
    pub decision_node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDecisionBucket {
    pub key: String,
    pub node_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDecisionNode {
    pub probe: CompiledDecisionProbe,
    pub branches: Vec<CompiledDecisionEdge>,
    pub leaf_constructor_indexes: Vec<usize>,
    pub leaf_entries: Vec<CompiledDecisionLeafEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDecisionEdge {
    pub value: u8,
    pub next_node_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDecisionLeafEntry {
    #[serde(default)]
    pub subtable_id: u32,
    #[serde(default)]
    pub constructor_id: u32,
    pub constructor_index: usize,
    pub pattern: CompiledDisjointPattern,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledDisjointPattern {
    Instruction(CompiledPatternBlock),
    Context(CompiledPatternBlock),
    Combine {
        context: CompiledPatternBlock,
        instruction: CompiledPatternBlock,
    },
    Or(Vec<CompiledDisjointPattern>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPatternBlock {
    pub offset: i32,
    pub nonzero_size: i32,
    pub mask_words: Vec<u32>,
    pub value_words: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledDecisionProbe {
    Terminal,
    InstructionBitSlice { offset: u8, mask: u8, shift: u8 },
    ContextBitSlice { offset: u8, mask: u8, shift: u8 },
    SlaInstructionBits { start_bit: u32, bit_size: u32 },
    SlaContextBits { start_bit: u32, bit_size: u32 },
    TokenFieldRef(CompiledTokenFieldRef),
    ContextFieldRef(CompiledContextFieldRef),
    TerminalPatternCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternConstraint {
    Instruction { offset: u32, mask: u64, value: u64 },
    Context { offset: u32, mask: u64, value: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledPatternMatcher {
    ExactBytes(Vec<u8>),
    RowCc { prefix: Vec<u8>, row: u8 },
    RowPage { row: u8, page: u8 },
    BitConstraints(Vec<PatternConstraint>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledTokenFieldRef {
    InstructionWidthProfile,
    AddressingForm,
    RegisterSelector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledContextFieldRef {
    DefaultContext,
}

impl CompiledPatternMatcher {
    pub fn key(&self) -> String {
        match self {
            Self::ExactBytes(bytes) => bytes
                .first()
                .map(|byte| format!("byte_{byte:02x}"))
                .unwrap_or_else(|| "empty".to_string()),
            Self::RowCc { prefix, row } => {
                if prefix.is_empty() {
                    format!("row_{row}")
                } else {
                    format!("row_{row}_after_{:02x}", prefix[prefix.len() - 1])
                }
            }
            Self::RowPage { row, page } => format!("row_{row}_page_{page}"),
            Self::BitConstraints(constraints) => {
                let mut hash = 0u64;
                for constraint in constraints {
                    match constraint {
                        PatternConstraint::Instruction {
                            offset,
                            mask,
                            value,
                        } => {
                            hash ^= (*offset as u64) ^ *mask ^ *value;
                        }
                        PatternConstraint::Context {
                            offset,
                            mask,
                            value,
                        } => {
                            hash ^= (*offset as u64) ^ *mask ^ *value ^ 0x12345678;
                        }
                    }
                }
                format!("bits_{hash:016x}")
            }
        }
    }
}

fn default_offsetbase() -> i32 {
    -1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledOperandSpec {
    TokenFieldExtraction {
        bit_offset: u32,
        bit_width: u32,
        sign_extend: bool,
    },
    SlaTokenField {
        big_endian: bool,
        sign_bit: bool,
        bit_start: u32,
        bit_end: u32,
        byte_start: u32,
        byte_end: u32,
        shift: i32,
        /// Byte offset of this token field's token from the start of the parent constructor.
        /// Mirrors OperandSymbol.reloffset in Ghidra (ATTRIB_OFF). Used in non-shared-cursor
        /// architectures so that `ctx.cursor + reloffset + byte_start` gives the correct
        /// absolute instruction-stream byte position (matches Ghidra's `point.getOffset() + bytestart`).
        #[serde(default)]
        reloffset: i32,
    },
    SlaVarnodeList {
        big_endian: bool,
        sign_bit: bool,
        bit_start: u32,
        bit_end: u32,
        byte_start: u32,
        byte_end: u32,
        shift: i32,
        entries: Vec<CompiledResolvedVarnode>,
        /// See `SlaTokenField::reloffset`.
        #[serde(default)]
        reloffset: i32,
    },
    SlaValueMap {
        big_endian: bool,
        sign_bit: bool,
        bit_start: u32,
        bit_end: u32,
        byte_start: u32,
        byte_end: u32,
        shift: i32,
        values: Vec<i64>,
        /// See `SlaTokenField::reloffset`.
        #[serde(default)]
        reloffset: i32,
    },
    SlaFixedVarnode {
        varnode: CompiledResolvedVarnode,
    },
    ContextFieldExtraction {
        bit_offset: u32,
        bit_width: u32,
        sign_extend: bool,
    },
    SubtableEvaluation {
        table_name: String,
        /// Byte offset of this operand's token from the start of the parent constructor.
        /// Derived from `ATTRIB_OFF` in Ghidra's SLA (OperandSymbol.reloffset).
        /// Used to position the sub-walker's cursor for non-shared-token-cursor architectures.
        #[serde(default)]
        reloffset: i32,
        /// Base operand index for the offset, or -1 if relative to constructor start.
        /// Derived from `ATTRIB_BASE` in Ghidra's SLA (OperandSymbol.offsetbase).
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
    },
    Immediate {
        size: u32,
        signed: bool,
    },
    Relative {
        size: u32,
    },
    FixedRegister {
        reg: CompiledFixedRegister,
        size: u32,
    },
    SlaPatternExpression {
        expr: CompiledPatternExpression,
        /// See `SlaTokenField::reloffset`. Used for non-shared-cursor architectures when
        /// the expression is a direct TokenField (e.g. `imm32` as a sequential operand).
        #[serde(default)]
        reloffset: i32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledResolvedVarnode {
    pub name: String,
    pub space: CompiledSpaceRef,
    pub offset: u64,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledConstructorTemplate {
    pub handles: Vec<CompiledHandleTemplate>,
    pub decode_steps: Vec<CompiledOperandDecodeStep>,
    pub num_labels: u32,
    #[serde(default)]
    pub result: Option<CompiledHandleTpl>,
    pub ops: Vec<CompiledOpTpl>,
    pub template_source: CompiledTemplateSource,
}

/// SLA-backed operand symbol metadata from Ghidra `OperandSymbol.encode` (`.sla` only carries a subset).
///
/// Reference: `OperandSymbol.encode` in Ghidra 12.0.4 — always writes `ATTRIB_MINLEN`; writes
/// `ATTRIB_CODE` when `isCodeAddress()`. JVM-only flags such as `variable_len` / `offset_irrel`
/// are not serialized on `ELEM_OPERAND_SYM` and cannot be recovered from `.sla` alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SlaOperandSymbolMeta {
    /// Ghidra `OperandSymbol.minimumlength` when `ATTRIB_MINLEN` is present in the packed operand.
    #[serde(default)]
    pub min_length: Option<i32>,
    /// Ghidra `OperandSymbol.isCodeAddress()` from `ATTRIB_CODE` when true.
    #[serde(default)]
    pub code_address: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledHandleTemplate {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
    #[serde(default)]
    pub sla_operand_symbol_meta: SlaOperandSymbolMeta,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledOperandDecodeStep {
    ConsumeTokenFields,
    DecodeOperand {
        operand_index: usize,
    },
    DescendSubtable {
        table_name: String,
        replace_current: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledPatternExpression {
    Constant(i64),
    TokenField {
        big_endian: bool,
        sign_bit: bool,
        bit_start: u32,
        bit_end: u32,
        byte_start: u32,
        byte_end: u32,
        shift: i32,
    },
    ContextField {
        sign_bit: bool,
        bit_start: u32,
        bit_end: u32,
        byte_start: u32,
        byte_end: u32,
        shift: i32,
    },
    OperandValue {
        index: usize,
    },
    Add(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Sub(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Mul(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Div(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    LeftShift(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    RightShift(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    And(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Or(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Xor(
        Box<CompiledPatternExpression>,
        Box<CompiledPatternExpression>,
    ),
    Negate(Box<CompiledPatternExpression>),
    Not(Box<CompiledPatternExpression>),
}
