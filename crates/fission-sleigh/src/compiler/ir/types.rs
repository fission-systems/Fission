use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSubtableDefinition {
    pub name: String,
    #[serde(default)]
    pub sla_subtable_id: u32,
    #[serde(default)]
    pub constructors_by_sla_id: BTreeMap<u32, usize>,
    pub constructors: Vec<CompiledExecutableConstructor>,
    pub decision_tree: CompiledDecisionTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFrontend {
    pub arch: String,
    pub default_context: u64,
    pub default_context_known_mask: u64,
    pub entry_spec: String,
    pub entry_id: String,
    pub include_manifest: Vec<String>,
    pub defines: Vec<(String, String)>,
    pub definitions: Vec<CompiledSpecDefinition>,
    pub macros: Vec<CompiledMacro>,
    pub constructors: Vec<CompiledConstructor>,
    pub subtables: BTreeMap<String, CompiledSubtableDefinition>,
    pub language_layout: CompiledLanguageLayout,
    pub construct_templates: Vec<CompiledConstructTpl>,
    pub pcode_ops: Vec<CompiledPcodeOp>,
    pub pattern_nodes: Vec<CompiledPatternNode>,
    /// Address spaces decoded from the `.sla` file (index → space ref).
    #[serde(default)]
    pub sla_spaces: BTreeMap<u64, CompiledSpaceRef>,
    /// Index of the unique (temporary) address space derived from `.sla`.
    /// Replaces the hardcoded `UNIQUE_SPACE_ID = 3` constant.
    #[serde(default)]
    pub sla_unique_space_index: u64,
    /// Index of the register address space derived from `.sla`.
    #[serde(default)]
    pub sla_register_space_index: u64,
    /// Base offset for unique temporary varnode allocation (`uniqbase` from `.sla`).
    #[serde(default)]
    pub sla_uniqbase: u64,
    /// Ghidra unique allocation mask (`ATTR_UNIQMASK`); used with instruction PC for temp bases.
    #[serde(default = "default_sla_uniqmask")]
    pub sla_uniqmask: u64,
}

fn default_sla_uniqmask() -> u64 {
    u64::MAX
}

impl CompiledFrontend {
    /// Ghidra `ConstTpl.J_CURSPACE` / `ParserWalker.getCurSpace()`: the default
    /// non-const address space for pcode emission (typically `ram`).
    ///
    /// Algorithm: prefer the SLA space named `ram`; otherwise the first space
    /// that is not `const`, `unique`, or `register`. No numeric index guess.
    pub fn sla_default_cur_space_index(&self) -> anyhow::Result<u64> {
        if let Some((idx, _)) = self.sla_spaces.iter().find(|(_, s)| s.name == "ram") {
            return Ok(*idx);
        }
        self.sla_spaces
            .iter()
            .find(|(_, s)| s.name != "const" && s.name != "unique" && s.name != "register")
            .map(|(idx, _)| *idx)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "SLA space table has no ram or other default address space for CurSpace"
                )
            })
    }

    /// Pointer size in bytes for [`Self::sla_default_cur_space_index`]'s space.
    pub fn sla_default_cur_space_pointer_size(&self) -> anyhow::Result<u32> {
        let idx = self.sla_default_cur_space_index()?;
        let space = self
            .sla_spaces
            .get(&idx)
            .ok_or_else(|| anyhow::anyhow!("CurSpace index {idx} missing from sla_spaces"))?;
        if space.addr_size == 0 {
            anyhow::bail!(
                "SLA space {} has addr_size=0 (cannot resolve CurSpaceSize)",
                space.name
            );
        }
        Ok(space.addr_size)
    }

    /// Returns the pointer/address size in bytes for the RAM (default data) space.
    /// This is ATTRIB_SIZE in Ghidra (e.g. 4 for 32-bit, 8 for 64-bit).
    /// Falls back to 8 (64-bit) when the SLA did not encode an address size.
    pub fn sla_ram_address_size(&self) -> u32 {
        self.sla_spaces
            .values()
            .find(|s| {
                s.name == "ram" || (s.name != "const" && s.name != "unique" && s.name != "register")
            })
            .map(|s| s.addr_size)
            .filter(|&sz| sz > 0)
            .unwrap_or(8)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledLanguageLayout {
    pub address_spaces: Vec<CompiledAddressSpace>,
    pub registers: Vec<CompiledRegister>,
    pub token_fields: Vec<CompiledTokenField>,
    pub context_fields: Vec<CompiledContextField>,
    pub subtables: Vec<CompiledSubtable>,
    pub display_templates: Vec<CompiledDisplayTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledAddressSpace {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledRegister {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledTokenField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSubtable {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledDisplayTemplate {
    pub constructor_hash: u64,
    #[serde(default)]
    pub pieces: Vec<CompiledDisplayPiece>,
    #[serde(default)]
    pub first_whitespace: Option<usize>,
    #[serde(default)]
    pub flowthru_operand_index: Option<usize>,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledDisplayPiece {
    Literal(String),
    OperandRef(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSpecDefinition {
    pub kind: String,
    pub source: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledMacro {
    pub name: String,
    pub source: String,
    pub body_line_count: usize,
}

/// Deferred global context change (Ghidra `ContextCommit` / `globalset` statement).
///
/// When a constructor fires, its `context_commits` are queued. After the instruction
/// is decoded, `apply_context_commits()` resolves each commit's target address from the
/// fixed handle of the referenced symbol and writes the context bits to the context
/// cache for future instructions at that address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextCommit {
    /// Symbol table ID of the target operand (raw SLA `ATTR_ID`). Used for tracing.
    pub symbol_id: u32,
    /// Resolved operand handle index within the constructor's handle list.
    /// `u32::MAX` means the symbol is a built-in (e.g. `inst_next`): the target
    /// address is computed at runtime as `instruction_start + instruction_length`.
    pub hand_index: u32,
    /// Word index within the context register (Ghidra `ATTR_NUMBER`).
    pub word_index: u32,
    /// Bit mask of the context bits to commit (Ghidra `ATTR_MASK`).
    pub mask: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledContextOp {
    pub bit_offset: u32,
    pub bit_width: u32,
    pub value: u64,
    pub word_index: u32,
    pub mask: u64,
    pub shift: i32,
    pub expr: Option<CompiledPatternExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledConstructor {
    pub mnemonic: String,
    pub display: String,
    pub source: String,
    pub control_flow: ControlFlowClass,
    pub pattern_signature: String,
    pub semantic_template: CompiledSemanticTemplate,
    pub with_stack: Vec<String>,
    pub semantic_ops: Vec<String>,
    pub signature_hash: u64,
    pub context_changes: Vec<CompiledContextOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledExecutableConstructor {
    pub constructor_id: u32,
    #[serde(default)]
    pub sla_identity: Option<CompiledSlaConstructorIdentity>,
    #[serde(default)]
    pub sla_decode_status: CompiledSlaDecodeStatus,
    pub mnemonic: String,
    pub source: String,
    pub display: String,
    #[serde(default = "CompiledDisplayTemplate::empty")]
    pub display_template: CompiledDisplayTemplate,
    pub signature_hash: u64,
    pub minimum_length: u32,
    pub context_changes: Vec<CompiledContextOp>,
    /// Deferred global context commits (Ghidra `globalset` / `ContextCommit`).
    #[serde(default)]
    pub context_commits: Vec<CompiledContextCommit>,
    pub matcher: CompiledPatternMatcher,
    pub mod_constraint: Option<u8>,
    pub operand_reg_values: Vec<u8>,
    pub opsize_variants: Vec<u8>,
    pub operand_specs: Vec<CompiledOperandSpec>,
    #[serde(default)]
    pub display_operands: Vec<CompiledDisplayOperand>,
    pub construct_tpl_kind: CompiledConstructTplKind,
    pub constructor_template: CompiledConstructorTemplate,
    /// Named p-code sections from Ghidra's `namedtempl`.
    /// Index corresponds to the section number (ATTR_SECTION value).
    #[serde(default)]
    pub named_templates: Vec<Option<CompiledConstructTpl>>,
    pub runtime_ready: bool,
    pub unsupported_template_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSlaConstructorIdentity {
    #[serde(default)]
    pub subtable_id: u32,
    pub subtable_name: String,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    #[serde(default)]
    pub source_file: String,
    #[serde(default)]
    pub source_line: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledSlaDecodeStatus {
    Decoded,
    Unsupported,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledHandleTemplate {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
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
    InstStart,
    InstNext,
    InstNext2,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledConstructTpl {
    pub constructor_hash: u64,
    pub num_labels: u32,
    #[serde(default)]
    pub result: Option<CompiledHandleTpl>,
    pub ops: Vec<CompiledOpTpl>,
}

impl CompiledConstructTpl {
    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        if let Some(result) = &self.result {
            if let Some(reason) = result.ghidra_template_shape_error() {
                return Some(reason);
            }
        }
        self.ops
            .iter()
            .find_map(CompiledOpTpl::ghidra_template_shape_error)
    }

    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledTemplateSource {
    SpecDerived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledOpTpl {
    /// Flat pcode opcode integer from SLA `ATTRIB_CODE` (Ghidra `PcodeOp` encoding).
    #[serde(default)]
    pub sla_raw_pcode_opcode: u32,
    pub opcode: CompiledOpTplOpcode,
    pub output: Option<CompiledVarnodeTpl>,
    pub inputs: Vec<CompiledVarnodeTpl>,
    pub label: Option<CompiledLabelRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledOpTplOpcode {
    Copy,
    Load,
    Store,
    IntAdd,
    IntSub,
    IntCarry,
    IntSCarry,
    IntSBorrow,
    IntAnd,
    IntOr,
    IntXor,
    IntMult,
    IntLeft,
    IntRight,
    IntSRight,
    IntEqual,
    IntNotEqual,
    IntLess,
    IntSLess,
    BoolNegate,
    BoolAnd,
    BoolOr,
    PopCount,
    IntZExt,
    IntSExt,
    Subpiece,
    Piece,
    Branch,
    BranchInd,
    CBranch,
    Call,
    CallInd,
    Return,
    CallOther,
    Build,
    /// Ghidra `PcodeEmit.appendCrossBuild`: `PTRSUB` placeholder in ConstructTpl.
    CrossBuild,
    /// Ghidra `PcodeEmit.delaySlot`: `INDIRECT` placeholder in ConstructTpl.
    DelaySlotIndirect,
    Label,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledVarnodeTpl {
    Varnode {
        space: CompiledSpaceTpl,
        offset: Box<CompiledConstTpl>,
        size: Box<CompiledConstTpl>,
    },
    HandleTpl(Box<CompiledHandleTpl>),
    Handle {
        operand_index: usize,
    },
    EffectiveAddress {
        operand_index: usize,
    },
    ConditionPredicate,
    Const(CompiledConstTpl),
    Space(CompiledSpaceRef),
    Temp {
        id: u32,
        size: u32,
    },
    Register {
        name: String,
        size: u32,
    },
    FixedRegister {
        reg: CompiledFixedRegister,
        size: u32,
    },
    Flag {
        bit: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledHandleTpl {
    pub space: Option<CompiledSpaceTpl>,
    pub size: Option<CompiledConstTpl>,
    pub ptr_space: Option<CompiledSpaceTpl>,
    pub ptr_offset: Option<CompiledConstTpl>,
    pub ptr_size: Option<CompiledConstTpl>,
    pub temp_space: Option<CompiledSpaceTpl>,
    pub temp_offset: Option<CompiledConstTpl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledSpaceTpl {
    SpaceRef(CompiledSpaceRef),
    Const(Box<CompiledConstTpl>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledConstTpl {
    Real {
        value: u64,
    },
    Handle {
        handle_index: i64,
        selector: CompiledHandleSelector,
        plus: Option<u64>,
    },
    Integer {
        value: i64,
        size: u32,
    },
    RelativeAddress,
    Relative {
        value: u64,
    },
    InstStart,
    InstNext,
    InstNext2,
    CurSpace,
    CurSpaceSize,
    SpaceId(CompiledSpaceRef),
    FlowRef,
    FlowRefSize,
    FlowDest,
    FlowDestSize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSpaceRef {
    pub name: String,
    pub index: u64,
    /// Addressable unit size in bytes (ATTRIB_WORDSIZE in Ghidra).
    /// 1 for byte-addressed spaces (RAM, register). Defaults to 1.
    #[serde(default = "default_word_size")]
    pub word_size: u32,
    /// Pointer/address size in bytes (ATTRIB_SIZE in Ghidra).
    /// 4 for 32-bit address spaces, 8 for 64-bit. Defaults to 0 (unknown).
    #[serde(default)]
    pub addr_size: u32,
}

fn default_word_size() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledLabelRef {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledHandleSelector {
    Space,
    Offset,
    Size,
    OffsetPlus,
}

impl CompiledOpTpl {
    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }

    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        if let Some(output) = &self.output {
            if let Some(reason) = output.ghidra_template_shape_error() {
                return Some(reason);
            }
        }
        self.inputs
            .iter()
            .find_map(CompiledVarnodeTpl::ghidra_template_shape_error)
    }
}

impl CompiledVarnodeTpl {
    pub fn is_ghidra_template_shape(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }

    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        match self {
            Self::Varnode { .. } => None,
            Self::HandleTpl(handle) => handle.ghidra_template_shape_error(),
            Self::Handle { .. } => Some("compatibility_handle_varnode"),
            Self::EffectiveAddress { .. } => Some("compatibility_effective_address_varnode"),
            Self::ConditionPredicate => Some("compatibility_condition_predicate_varnode"),
            Self::Const(_) => Some("compatibility_const_varnode"),
            Self::Space(_) => Some("compatibility_space_varnode"),
            Self::Temp { .. } => Some("compatibility_temp_varnode"),
            Self::Register { .. } => Some("compatibility_register_varnode"),
            Self::FixedRegister { .. } => Some("compatibility_fixed_register_varnode"),
            Self::Flag { .. } => Some("compatibility_flag_varnode"),
        }
    }
}

impl CompiledConstructorTemplate {
    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        if let Some(result) = &self.result {
            if let Some(reason) = result.ghidra_template_shape_error() {
                return Some(reason);
            }
        }
        self.ops
            .iter()
            .find_map(CompiledOpTpl::ghidra_template_shape_error)
    }

    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.ghidra_template_shape_error().is_none()
    }
}

impl CompiledHandleTpl {
    pub fn ghidra_template_shape_error(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledArithmeticOpcode {
    Add,
    Sub,
    And,
    Or,
    Xor,
    Mul,
    Shl,
    Shr,
    Sar,
    Inc,
    Dec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledFixedRegister {
    Accumulator,
    StackPointer,
    FramePointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompiledConstructTplKind {
    Unsupported,
    Generic,
    Nop,
    Ret,
    Call,
    Jmp,
    Jcc,
    Mov,
    AddressOf,
    StackStore,
    StackLoad,
    FrameTeardown,
    Add,
    Sub,
    And,
    Or,
    Xor,
    Imul,
    Mul,
    Shl,
    Shr,
    Sar,
    Inc,
    Dec,
    Cmp,
    Test,
    Movzx,
    Movsx,
    Movsxd,
    Setcc,
    Cbw,
    Cwde,
    Cdqe,
}

impl CompiledConstructTplKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::Nop => "nop",
            Self::Ret => "ret",
            Self::Call => "call",
            Self::Jmp => "jmp",
            Self::Jcc => "jcc",
            Self::Mov => "mov",
            Self::AddressOf => "lea",
            Self::StackStore => "push",
            Self::StackLoad => "pop",
            Self::FrameTeardown => "leave",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Imul => "imul",
            Self::Mul => "mul",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Sar => "sar",
            Self::Inc => "inc",
            Self::Dec => "dec",
            Self::Cmp => "cmp",
            Self::Test => "test",
            Self::Movzx => "movzx",
            Self::Movsx => "movsx",
            Self::Movsxd => "movsxd",
            Self::Setcc => "setcc",
            Self::Cbw => "cbw",
            Self::Cwde => "cwde",
            Self::Cdqe => "cdqe",
            Self::Generic => "generic",
        }
    }
}

impl CompiledDecisionProbe {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::InstructionBitSlice { .. } => "instruction_bit_slice",
            Self::ContextBitSlice { .. } => "context_bit_slice",
            Self::SlaInstructionBits { .. } => "sla_instruction_bits",
            Self::SlaContextBits { .. } => "sla_context_bits",
            Self::TokenFieldRef(CompiledTokenFieldRef::InstructionWidthProfile) => {
                "token_field_instruction_width"
            }
            Self::TokenFieldRef(CompiledTokenFieldRef::AddressingForm) => {
                "token_field_addressing_form"
            }
            Self::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
                "token_field_register_selector"
            }
            Self::ContextFieldRef(CompiledContextFieldRef::DefaultContext) => {
                "context_field_default"
            }
            Self::TerminalPatternCheck => "terminal_pattern_check",
        }
    }
}

impl CompiledTemplateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpecDerived => "spec_derived",
        }
    }
}

impl CompiledOpTplOpcode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Copy => "COPY",
            Self::Load => "LOAD",
            Self::Store => "STORE",
            Self::IntAdd => "INT_ADD",
            Self::IntSub => "INT_SUB",
            Self::IntCarry => "INT_CARRY",
            Self::IntSCarry => "INT_SCARRY",
            Self::IntSBorrow => "INT_SBORROW",
            Self::IntAnd => "INT_AND",
            Self::IntOr => "INT_OR",
            Self::IntXor => "INT_XOR",
            Self::IntMult => "INT_MULT",
            Self::IntLeft => "INT_LEFT",
            Self::IntRight => "INT_RIGHT",
            Self::IntSRight => "INT_SRIGHT",
            Self::IntEqual => "INT_EQUAL",
            Self::IntNotEqual => "INT_NOTEQUAL",
            Self::IntLess => "INT_LESS",
            Self::IntSLess => "INT_SLESS",
            Self::BoolNegate => "BOOL_NEGATE",
            Self::BoolAnd => "BOOL_AND",
            Self::BoolOr => "BOOL_OR",
            Self::PopCount => "POPCOUNT",
            Self::IntZExt => "INT_ZEXT",
            Self::IntSExt => "INT_SEXT",
            Self::Subpiece => "SUBPIECE",
            Self::Piece => "PIECE",
            Self::Branch => "BRANCH",
            Self::BranchInd => "BRANCHIND",
            Self::CBranch => "CBRANCH",
            Self::Call => "CALL",
            Self::CallInd => "CALLIND",
            Self::Return => "RETURN",
            Self::CallOther => "CALLOTHER",
            Self::Build => "BUILD",
            Self::CrossBuild => "CROSSBUILD",
            Self::DelaySlotIndirect => "DELAYSLOT_INDIRECT",
            Self::Label => "LABEL",
            Self::Unsupported => "UNSUPPORTED",
        }
    }
}

impl CompiledArithmeticOpcode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Mul => "mul",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Sar => "sar",
            Self::Inc => "inc",
            Self::Dec => "dec",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledSemanticTemplate {
    pub status: String,
    pub action_hash: u64,
    pub op_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPcodeOp {
    pub name: String,
    pub defined_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledPatternNode {
    pub node_id: String,
    pub source: String,
    pub mnemonic: String,
    pub with_depth: usize,
    pub control_flow: ControlFlowClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ControlFlowClass {
    None,
    Branch,
    ConditionalBranch,
    Call,
    Return,
    Mixed,
}

impl ControlFlowClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Branch => "branch",
            Self::ConditionalBranch => "conditional_branch",
            Self::Call => "call",
            Self::Return => "return",
            Self::Mixed => "mixed",
        }
    }
}
