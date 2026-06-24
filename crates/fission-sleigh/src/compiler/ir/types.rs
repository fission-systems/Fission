use std::collections::{BTreeMap, BTreeSet};

use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledSubtableDefinition {
    pub name: String,
    #[serde(default)]
    pub sla_subtable_id: u32,
    #[serde(default)]
    pub constructors_by_sla_id: BTreeMap<u32, usize>,
    pub constructors: Vec<CompiledExecutableConstructor>,
    pub decision_tree: CompiledDecisionTree,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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
    /// User-defined p-code operations (`<userop_head>` index -> name)
    #[serde(default)]
    pub userops: BTreeMap<u32, String>,
}

fn default_sla_uniqmask() -> u64 {
    0
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
    pub fn sla_ram_address_size(&self) -> anyhow::Result<u32> {
        self.sla_spaces
            .values()
            .find(|s| {
                s.name == "ram" || (s.name != "const" && s.name != "unique" && s.name != "register")
            })
            .map(|s| s.addr_size)
            .filter(|&sz| sz > 0)
            .ok_or_else(|| anyhow::anyhow!("SLA RAM/default address space size is missing"))
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledLanguageLayout {
    pub address_spaces: Vec<CompiledAddressSpace>,
    pub registers: Vec<CompiledRegister>,
    pub token_fields: Vec<CompiledTokenField>,
    pub context_fields: Vec<CompiledContextField>,
    pub subtables: Vec<CompiledSubtable>,
    pub display_templates: Vec<CompiledDisplayTemplate>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledAddressSpace {
    pub name: String,
    pub source: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledRegister {
    pub name: String,
    pub source: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledTokenField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledContextField {
    pub name: String,
    pub bit_offset: u32,
    pub bit_width: u32,
    pub source: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledSubtable {
    pub name: String,
    pub source: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledDisplayPiece {
    Literal(String),
    OperandRef(usize),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledSpecDefinition {
    pub kind: String,
    pub source: String,
    pub statement: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledMacro {
    pub name: String,
    pub source: String,
    pub body_line_count: usize,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledContextCommitTarget {
    OperandHandle { hand_index: u32 },
    InstStart,
    InstNext,
}

/// Deferred global context change (Ghidra `ContextCommit` / `globalset` statement).
///
/// When a constructor fires, its `context_commits` are queued. After the instruction
/// is decoded, `apply_context_commits()` resolves each commit's target address from the
/// fixed handle of the referenced symbol and writes the context bits to the context
/// cache for future instructions at that address.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledContextCommit {
    /// Symbol table ID of the target TripleSymbol (raw SLA `ATTR_ID`). Used for tracing.
    pub symbol_id: u32,
    pub target: CompiledContextCommitTarget,
    /// Word index within the context register (Ghidra `ATTR_NUMBER`).
    pub word_index: u32,
    /// Bit mask of the context bits to commit (Ghidra `ATTR_MASK`).
    pub mask: u32,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledContextOp {
    pub bit_offset: u32,
    pub bit_width: u32,
    pub value: u64,
    pub word_index: u32,
    pub mask: u64,
    pub shift: i32,
    pub expr: Option<CompiledPatternExpression>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum CompiledSlaDecodeStatus {
    Decoded,
    Unsupported,
}

impl Default for CompiledSlaDecodeStatus {
    fn default() -> Self {
        Self::Decoded
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDisplayOperand {
    pub operand_index: usize,
    pub kind: CompiledDisplayOperandKind,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

    pub fn from_literal_display(display: String) -> Self {
        Self {
            display,
            ..Self::empty()
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDecisionTree {
    pub root_node_index: usize,
    pub root_buckets: Vec<CompiledDecisionBucket>,
    pub nodes: Vec<CompiledDecisionNode>,
    pub decision_node_count: usize,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDecisionBucket {
    pub key: String,
    pub node_index: usize,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDecisionNode {
    pub probe: CompiledDecisionProbe,
    pub branches: Vec<CompiledDecisionEdge>,
    pub leaf_constructor_indexes: Vec<usize>,
    pub leaf_entries: Vec<CompiledDecisionLeafEntry>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDecisionEdge {
    pub value: u8,
    pub next_node_index: usize,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledDecisionLeafEntry {
    #[serde(default)]
    pub subtable_id: u32,
    #[serde(default)]
    pub constructor_id: u32,
    pub constructor_index: usize,
    pub pattern: CompiledDisjointPattern,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(bound(serialize = "__S: rkyv::ser::Serializer + rkyv::ser::ScratchSpace"))]
pub enum CompiledDisjointPattern {
    Instruction(CompiledPatternBlock),
    Context(CompiledPatternBlock),
    Combine {
        context: CompiledPatternBlock,
        instruction: CompiledPatternBlock,
    },
    Or(#[omit_bounds] Vec<CompiledDisjointPattern>),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledPatternBlock {
    pub offset: i32,
    pub nonzero_size: i32,
    pub mask_words: Vec<u32>,
    pub value_words: Vec<u32>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum CompiledDecisionProbe {
    Terminal,
    InstructionBitSlice { offset: u8, mask: u8, shift: u8 },
    ContextBitSlice { offset: u8, mask: u8, shift: u8 },
    SlaInstructionBits { start_bit: u32, bit_size: u32 },
    SlaContextBits { start_bit: u32, bit_size: u32 },
    TerminalPatternCheck,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum PatternConstraint {
    Instruction { offset: u32, mask: u64, value: u64 },
    Context { offset: u32, mask: u64, value: u64 },
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledPatternMatcher {
    ExactBytes(Vec<u8>),
    RowCc { prefix: Vec<u8>, row: u8 },
    RowPage { row: u8, page: u8 },
    BitConstraints(Vec<PatternConstraint>),
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledOperandSpec {
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
        /// Base operand index for the offset, or -1 if relative to constructor start.
        /// Derived from `ATTRIB_BASE` in Ghidra's SLA (OperandSymbol.offsetbase).
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
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
        /// See `SlaTokenField::offsetbase`.
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
    },
    SlaVarnodeListExpression {
        expr: CompiledPatternExpression,
        entries: Vec<CompiledResolvedVarnode>,
        /// See `SlaTokenField::reloffset`.
        #[serde(default)]
        reloffset: i32,
        /// See `SlaTokenField::offsetbase`.
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
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
        /// See `SlaTokenField::offsetbase`.
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
    },
    SlaValueMapExpression {
        expr: CompiledPatternExpression,
        values: Vec<i64>,
        /// See `SlaTokenField::reloffset`.
        #[serde(default)]
        reloffset: i32,
        /// See `SlaTokenField::offsetbase`.
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
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
    SlaPatternExpression {
        expr: CompiledPatternExpression,
        /// See `SlaTokenField::reloffset`. Used for non-shared-cursor architectures when
        /// the expression is a direct TokenField (e.g. `imm32` as a sequential operand).
        #[serde(default)]
        reloffset: i32,
        /// See `SlaTokenField::offsetbase`.
        #[serde(default = "default_offsetbase")]
        offsetbase: i32,
    },
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledResolvedVarnode {
    pub name: String,
    pub space: CompiledSpaceRef,
    pub offset: u64,
    pub size: u32,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledConstructorTemplate {
    pub handles: Vec<CompiledHandleTemplate>,
    pub decode_steps: Vec<CompiledOperandDecodeStep>,
    pub num_labels: u32,
    #[serde(default)]
    pub result: Option<CompiledHandleTpl>,
    pub ops: Vec<CompiledOpTpl>,
    pub template_source: CompiledTemplateSource,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledHandleTemplate {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
    /// Ghidra OperandSymbol.minimumlength, in bytes. Used when building
    /// ConstructState-compatible operand lengths before parent length calculation.
    #[serde(default)]
    pub minimum_length: u32,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledOperandDecodeStep {
    DecodeOperand {
        operand_index: usize,
    },
    DescendSubtable {
        table_name: String,
        replace_current: bool,
    },
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(bound(serialize = "__S: rkyv::ser::Serializer"))]
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
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Sub(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Mul(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Div(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    LeftShift(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    RightShift(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    And(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Or(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Xor(
        #[omit_bounds] Box<CompiledPatternExpression>,
        #[omit_bounds] Box<CompiledPatternExpression>,
    ),
    Negate(#[omit_bounds] Box<CompiledPatternExpression>),
    Not(#[omit_bounds] Box<CompiledPatternExpression>),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum CompiledTemplateSource {
    SpecDerived,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledOpTpl {
    /// Flat pcode opcode integer from SLA `ATTRIB_CODE` (Ghidra `PcodeOp` encoding).
    #[serde(default)]
    pub sla_raw_pcode_opcode: u32,
    pub opcode: CompiledOpTplOpcode,
    pub output: Option<CompiledVarnodeTpl>,
    pub inputs: Vec<CompiledVarnodeTpl>,
    pub label: Option<CompiledLabelRef>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum CompiledOpTplOpcode {
    Copy,
    Load,
    Store,
    IntAdd,
    IntSub,
    IntCarry,
    IntSCarry,
    IntSBorrow,
    Int2Comp,
    IntNegate,
    IntAnd,
    IntOr,
    IntXor,
    IntMult,
    IntDiv,
    IntSDiv,
    IntRem,
    IntSRem,
    IntLeft,
    IntRight,
    IntSRight,
    IntEqual,
    IntNotEqual,
    IntLess,
    IntLessEqual,
    IntSLess,
    IntSLessEqual,
    BoolNegate,
    BoolXor,
    BoolAnd,
    BoolOr,
    PopCount,
    LzCount,
    Cast,
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
    IntZExt,
    IntSExt,
    Subpiece,
    Piece,
    SegmentOp,
    CPoolRef,
    New,
    Insert,
    Extract,
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledVarnodeTpl {
    Varnode {
        space: CompiledSpaceTpl,
        offset: Box<CompiledConstTpl>,
        size: Box<CompiledConstTpl>,
    },
    HandleTpl(Box<CompiledHandleTpl>),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledHandleTpl {
    pub space: Option<CompiledSpaceTpl>,
    pub size: Option<CompiledConstTpl>,
    pub ptr_space: Option<CompiledSpaceTpl>,
    pub ptr_offset: Option<CompiledConstTpl>,
    pub ptr_size: Option<CompiledConstTpl>,
    pub temp_space: Option<CompiledSpaceTpl>,
    pub temp_offset: Option<CompiledConstTpl>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub enum CompiledSpaceTpl {
    SpaceRef(CompiledSpaceRef),
    Const(Box<CompiledConstTpl>),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_frontend_with_spaces(spaces: BTreeMap<u64, CompiledSpaceRef>) -> CompiledFrontend {
        CompiledFrontend {
            arch: "test".to_string(),
            default_context: 0,
            default_context_known_mask: 0,
            entry_spec: "test.slaspec".to_string(),
            entry_id: "test".to_string(),
            include_manifest: Vec::new(),
            defines: Vec::new(),
            definitions: Vec::new(),
            macros: Vec::new(),
            constructors: Vec::new(),
            subtables: BTreeMap::new(),
            language_layout: CompiledLanguageLayout {
                address_spaces: Vec::new(),
                registers: Vec::new(),
                token_fields: Vec::new(),
                context_fields: Vec::new(),
                subtables: Vec::new(),
                display_templates: Vec::new(),
            },
            construct_templates: Vec::new(),
            pcode_ops: Vec::new(),
            pattern_nodes: Vec::new(),
            sla_spaces: spaces,
            sla_unique_space_index: 0,
            sla_register_space_index: 0,
            sla_uniqbase: 0,
            sla_uniqmask: default_sla_uniqmask(),
            userops: BTreeMap::new(),
        }
    }

    #[test]
    fn default_sla_uniqmask_matches_ghidra_absent_attribute() {
        let frontend = minimal_frontend_with_spaces(BTreeMap::new());

        assert_eq!(default_sla_uniqmask(), 0);
        assert_eq!(frontend.sla_uniqmask, 0);
    }

    #[test]
    fn sla_ram_address_size_fails_closed_when_space_metadata_is_missing() {
        let frontend = minimal_frontend_with_spaces(BTreeMap::new());

        let error = frontend
            .sla_ram_address_size()
            .expect_err("missing SLA RAM metadata must fail closed");

        assert!(
            error
                .to_string()
                .contains("SLA RAM/default address space size is missing"),
            "{error:#}"
        );
    }

    #[test]
    fn sla_ram_address_size_rejects_zero_sized_default_space() {
        let mut spaces = BTreeMap::new();
        spaces.insert(
            1,
            CompiledSpaceRef {
                name: "ram".to_string(),
                index: 1,
                word_size: 1,
                addr_size: 0,
            },
        );
        let frontend = minimal_frontend_with_spaces(spaces);

        assert!(frontend.sla_ram_address_size().is_err());
    }

    #[test]
    fn sla_ram_address_size_uses_decoded_sla_space_size() {
        let mut spaces = BTreeMap::new();
        spaces.insert(
            1,
            CompiledSpaceRef {
                name: "ram".to_string(),
                index: 1,
                word_size: 1,
                addr_size: 4,
            },
        );
        let frontend = minimal_frontend_with_spaces(spaces);

        assert_eq!(frontend.sla_ram_address_size().expect("ram size"), 4);
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledLabelRef {
    pub name: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
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

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
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

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
pub enum CompiledConstructTplKind {
    Generic,
}

impl CompiledConstructTplKind {
    pub fn as_str(self) -> &'static str {
        match self {
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
            Self::Int2Comp => "INT_2COMP",
            Self::IntNegate => "INT_NEGATE",
            Self::IntAnd => "INT_AND",
            Self::IntOr => "INT_OR",
            Self::IntXor => "INT_XOR",
            Self::IntMult => "INT_MULT",
            Self::IntDiv => "INT_DIV",
            Self::IntSDiv => "INT_SDIV",
            Self::IntRem => "INT_REM",
            Self::IntSRem => "INT_SREM",
            Self::IntLeft => "INT_LEFT",
            Self::IntRight => "INT_RIGHT",
            Self::IntSRight => "INT_SRIGHT",
            Self::IntEqual => "INT_EQUAL",
            Self::IntNotEqual => "INT_NOTEQUAL",
            Self::IntLess => "INT_LESS",
            Self::IntLessEqual => "INT_LESSEQUAL",
            Self::IntSLess => "INT_SLESS",
            Self::IntSLessEqual => "INT_SLESSEQUAL",
            Self::BoolNegate => "BOOL_NEGATE",
            Self::BoolXor => "BOOL_XOR",
            Self::BoolAnd => "BOOL_AND",
            Self::BoolOr => "BOOL_OR",
            Self::PopCount => "POPCOUNT",
            Self::LzCount => "LZCOUNT",
            Self::Cast => "CAST",
            Self::FloatEqual => "FLOAT_EQUAL",
            Self::FloatNotEqual => "FLOAT_NOTEQUAL",
            Self::FloatLess => "FLOAT_LESS",
            Self::FloatLessEqual => "FLOAT_LESSEQUAL",
            Self::FloatNan => "FLOAT_NAN",
            Self::FloatAdd => "FLOAT_ADD",
            Self::FloatDiv => "FLOAT_DIV",
            Self::FloatMult => "FLOAT_MULT",
            Self::FloatSub => "FLOAT_SUB",
            Self::FloatNeg => "FLOAT_NEG",
            Self::FloatAbs => "FLOAT_ABS",
            Self::FloatSqrt => "FLOAT_SQRT",
            Self::FloatInt2Float => "FLOAT_INT2FLOAT",
            Self::FloatFloat2Float => "FLOAT_FLOAT2FLOAT",
            Self::FloatTrunc => "FLOAT_TRUNC",
            Self::FloatCeil => "FLOAT_CEIL",
            Self::FloatFloor => "FLOAT_FLOOR",
            Self::FloatRound => "FLOAT_ROUND",
            Self::IntZExt => "INT_ZEXT",
            Self::IntSExt => "INT_SEXT",
            Self::Subpiece => "SUBPIECE",
            Self::Piece => "PIECE",
            Self::SegmentOp => "SEGMENTOP",
            Self::CPoolRef => "CPOOLREF",
            Self::New => "NEW",
            Self::Insert => "INSERT",
            Self::Extract => "EXTRACT",
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

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledSemanticTemplate {
    pub status: String,
    pub action_hash: u64,
    pub op_count: usize,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledPcodeOp {
    pub name: String,
    pub defined_in: String,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
pub struct CompiledPatternNode {
    pub node_id: String,
    pub source: String,
    pub mnemonic: String,
    pub with_depth: usize,
    pub control_flow: ControlFlowClass,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
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
