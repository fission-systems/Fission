use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
use super::preprocessor::ExpandedSpec;
use super::sla::CompiledSlaTemplateLibrary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFrontend {
    pub arch: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub include_manifest: Vec<String>,
    pub defines: Vec<(String, String)>,
    pub definitions: Vec<CompiledSpecDefinition>,
    pub macros: Vec<CompiledMacro>,
    pub constructors: Vec<CompiledConstructor>,
    pub executable_constructors: Vec<CompiledExecutableConstructor>,
    pub decision_tree: CompiledDecisionTree,
    pub language_layout: CompiledLanguageLayout,
    pub construct_templates: Vec<CompiledConstructTpl>,
    pub pcode_ops: Vec<CompiledPcodeOp>,
    pub pattern_nodes: Vec<CompiledPatternNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledLanguageLayout {
    pub address_spaces: Vec<CompiledAddressSpace>,
    pub registers: Vec<CompiledRegister>,
    pub token_fields: Vec<CompiledTokenField>,
    pub context_fields: Vec<CompiledContextField>,
    pub subtables: Vec<CompiledSubtable>,
    pub display_templates: Vec<CompiledDisplayTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledAddressSpace {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRegister {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledTokenField {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledContextField {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSubtable {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDisplayTemplate {
    pub constructor_hash: u64,
    pub display: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSpecDefinition {
    pub kind: String,
    pub source: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledMacro {
    pub name: String,
    pub source: String,
    pub body_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledExecutableConstructor {
    pub mnemonic: String,
    pub source: String,
    pub display: String,
    pub signature_hash: u64,
    pub matcher: CompiledPatternMatcher,
    pub mod_constraint: Option<u8>,
    pub operand_reg_values: Vec<u8>,
    pub opsize_variants: Vec<u8>,
    pub operand_specs: Vec<CompiledOperandSpec>,
    pub construct_tpl_kind: CompiledConstructTplKind,
    pub constructor_template: CompiledConstructorTemplate,
    pub runtime_ready: bool,
    pub unsupported_template_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDecisionTree {
    pub root_node_index: usize,
    pub root_buckets: Vec<CompiledDecisionBucket>,
    pub nodes: Vec<CompiledDecisionNode>,
    pub decision_node_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDecisionBucket {
    pub key: String,
    pub node_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDecisionNode {
    pub probe: CompiledDecisionProbe,
    pub branches: Vec<CompiledDecisionEdge>,
    pub leaf_constructor_indexes: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledDecisionEdge {
    pub value: u8,
    pub next_node_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledDecisionProbe {
    Terminal,
    InstructionBitSlice { offset: u8, mask: u8, shift: u8 },
    ContextBitSlice { offset: u8, mask: u8, shift: u8 },
    TokenFieldRef(CompiledTokenFieldRef),
    ContextFieldRef(CompiledContextFieldRef),
    TerminalPatternCheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledPatternMatcher {
    ExactBytes(Vec<u8>),
    RowCc { prefix: Vec<u8>, row: u8 },
    RowPage { row: u8, page: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledTokenFieldRef {
    InstructionWidthProfile,
    AddressingForm,
    RegisterSelector,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledOperandSpec {
    TokenFieldRm {
        size: u32,
        memory_only: bool,
    },
    TokenFieldReg {
        size: u32,
    },
    OpcodeTokenReg {
        size: u32,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledConstructorTemplate {
    pub handles: Vec<CompiledHandleTemplate>,
    pub decode_steps: Vec<CompiledOperandDecodeStep>,
    pub semantic_ops: Vec<CompiledSemanticOp>,
    pub op_templates: Vec<CompiledOpTpl>,
    pub template_source: CompiledTemplateSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledHandleTemplate {
    pub operand_index: usize,
    pub spec: CompiledOperandSpec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledOperandDecodeStep {
    ConsumeTokenFields,
    DecodeOperand { operand_index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledSemanticOp {
    Nop,
    Return,
    Call,
    Jump,
    ConditionalJump,
    Copy,
    AddressOf,
    StackStore,
    StackLoad,
    FrameTeardown,
    Binary { opcode: CompiledArithmeticOpcode },
    Compare { bitwise: bool },
    Extend { signed: bool },
    SetCc,
    AccumulatorExtend { src_size: u32, dst_size: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledConstructTpl {
    pub constructor_hash: u64,
    pub ops: Vec<CompiledSemanticOp>,
    pub op_templates: Vec<CompiledOpTpl>,
    pub template_source: CompiledTemplateSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompiledTemplateSource {
    SpecDerived,
    /// Fission-native templates for constructors whose SLA templates reference
    /// unresolvable subconstructor handles (e.g., J^cc `cc` subtable).
    /// These use Fission varnode shapes (Handle, ConditionPredicate, etc.)
    /// and are evaluated by the native template executor.
    NativeFission,
    CompatibilityLowered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledOpTpl {
    pub opcode: CompiledOpTplOpcode,
    pub output: Option<CompiledVarnodeTpl>,
    pub inputs: Vec<CompiledVarnodeTpl>,
    pub label: Option<CompiledLabelRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    CBranch,
    Call,
    Return,
    CallOther,
    Build,
    Label,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledVarnodeTpl {
    /// Ghidra-shaped `VarnodeTpl`: `(space, offset, size)` are all `ConstTpl`
    /// descendants. This is the only canonical varnode shape for
    /// `SpecDerived` raw p-code emission.
    Varnode {
        space: CompiledSpaceTpl,
        offset: Box<CompiledConstTpl>,
        size: Box<CompiledConstTpl>,
    },
    /// Ghidra-shaped `HandleTpl`. This carries operand/exported-handle
    /// indirection without lowering it through a mnemonic-specific helper.
    HandleTpl(Box<CompiledHandleTpl>),
    // Compatibility-only conveniences below this line. These may remain in
    // generated inventory/debug output, but they are not valid for
    // `SpecDerived` raw p-code emission.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledHandleTpl {
    pub space: Option<CompiledSpaceTpl>,
    pub size: Option<CompiledConstTpl>,
    pub ptr_space: Option<CompiledSpaceTpl>,
    pub ptr_offset: Option<CompiledConstTpl>,
    pub ptr_size: Option<CompiledConstTpl>,
    pub temp_space: Option<CompiledSpaceTpl>,
    pub temp_offset: Option<CompiledConstTpl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledSpaceTpl {
    SpaceRef(CompiledSpaceRef),
    Const(Box<CompiledConstTpl>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompiledConstTpl {
    Real {
        value: u64,
    },
    Handle {
        handle_index: i64,
        selector: CompiledHandleSelector,
        plus: Option<u64>,
    },
    Integer { value: i64, size: u32 },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSpaceRef {
    pub name: String,
    pub index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledLabelRef {
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledHandleSelector {
    Space,
    Offset,
    Size,
    OffsetPlus,
}

impl CompiledOpTpl {
    pub fn uses_only_ghidra_template_shapes(&self) -> bool {
        self.output
            .as_ref()
            .map(CompiledVarnodeTpl::is_ghidra_template_shape)
            .unwrap_or(true)
            && self
                .inputs
                .iter()
                .all(CompiledVarnodeTpl::is_ghidra_template_shape)
    }
}

impl CompiledVarnodeTpl {
    pub fn is_ghidra_template_shape(&self) -> bool {
        matches!(self, Self::Varnode { .. } | Self::HandleTpl(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledFixedRegister {
    Accumulator,
    StackPointer,
    FramePointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledConstructTplKind {
    Unsupported,
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
        }
    }
}

impl CompiledDecisionProbe {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::InstructionBitSlice { .. } => "instruction_bit_slice",
            Self::ContextBitSlice { .. } => "context_bit_slice",
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

impl CompiledSemanticOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Nop => "nop",
            Self::Return => "return",
            Self::Call => "call",
            Self::Jump => "jump",
            Self::ConditionalJump => "conditional_jump",
            Self::Copy => "copy",
            Self::AddressOf => "address_of",
            Self::StackStore => "store_stack",
            Self::StackLoad => "load_stack",
            Self::FrameTeardown => "frame_teardown",
            Self::Binary { .. } => "binary",
            Self::Compare { bitwise: false } => "compare",
            Self::Compare { bitwise: true } => "test",
            Self::Extend { signed: false } => "zext",
            Self::Extend { signed: true } => "sext",
            Self::SetCc => "setcc",
            Self::AccumulatorExtend { .. } => "accumulator_extend",
        }
    }
}

impl CompiledTemplateSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpecDerived => "spec_derived",
            Self::NativeFission => "native_fission",
            Self::CompatibilityLowered => "compatibility_lowered",
        }
    }
}

impl CompiledOpTplOpcode {
    pub fn as_str(self) -> &'static str {
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
            Self::CBranch => "CBRANCH",
            Self::Call => "CALL",
            Self::Return => "RETURN",
            Self::CallOther => "CALLOTHER",
            Self::Build => "BUILD",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSemanticTemplate {
    pub status: String,
    pub action_hash: u64,
    pub op_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPcodeOp {
    pub name: String,
    pub defined_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPatternNode {
    pub node_id: String,
    pub source: String,
    pub mnemonic: String,
    pub with_depth: usize,
    pub control_flow: ControlFlowClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

pub fn compile_frontend(
    arch: &str,
    expanded: &ExpandedSpec,
    ast: &SpecAst,
) -> Result<CompiledFrontend> {
    let mut collector = Collector {
        definitions: Vec::new(),
        macros: Vec::new(),
        constructors: Vec::new(),
        executable_constructors: Vec::new(),
        pcode_ops: BTreeSet::new(),
        pcode_op_sources: BTreeMap::new(),
        pattern_nodes: Vec::new(),
    };
    collector.collect_items(&ast.items, &mut Vec::new());

    let language_layout = collector.language_layout();
    let construct_templates = collector.construct_templates();
    let mut pcode_ops = collector
        .pcode_ops
        .into_iter()
        .map(|name| CompiledPcodeOp {
            defined_in: collector
                .pcode_op_sources
                .get(&name)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string()),
            name,
        })
        .collect::<Vec<_>>();
    pcode_ops.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    let decision_tree = build_decision_tree(&collector.executable_constructors);
    Ok(CompiledFrontend {
        arch: arch.to_string(),
        entry_spec: expanded
            .entry_spec
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown.slaspec")
            .to_string(),
        entry_id: expanded
            .entry_spec
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
            .to_string(),
        include_manifest: expanded
            .include_manifest
            .iter()
            .map(|entry| format!("{}@{}", entry.relative_path, entry.depth))
            .collect(),
        defines: expanded
            .defines
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        definitions: collector.definitions,
        macros: collector.macros,
        constructors: collector.constructors,
        executable_constructors: collector.executable_constructors,
        decision_tree,
        language_layout,
        construct_templates,
        pcode_ops,
        pattern_nodes: collector.pattern_nodes,
    })
}

pub fn apply_sla_construct_templates(
    compiled: &mut CompiledFrontend,
    library: &CompiledSlaTemplateLibrary,
) -> usize {
    let mut updated = 0usize;
    for constructor in &mut compiled.executable_constructors {
        let Some(templates) = library.constructors_by_source.get(&constructor.source) else {
            continue;
        };
        if templates.len() != 1 {
            constructor.runtime_ready = false;
            constructor.unsupported_template_kind =
                Some("sla_constructor_mapping_mismatch".to_string());
            continue;
        }
        let decoded = &templates[0].constructor_template;
        // Only reject templates containing truly unsupported opcodes.
        // Build (subtable inlining) and CallOther (user-defined ops) are
        // now handled at emission time in the runtime emitter.
        let has_unsupported_opcode = decoded
            .op_templates
            .iter()
            .any(|op| matches!(op.opcode, CompiledOpTplOpcode::Unsupported));

        // Build handle index remapping from SLA ordering to our ordering.
        // Fission extracts `operand_specs` based on the display string.
        // Ghidra emits `ELEM_OPPRINT` indices in the exact order of the display string.
        // Therefore, `opprint_indices[fission_idx]` gives the SLA operand index.
        //
        // When opprint_indices is empty, this constructor has no visible operands
        // in its display string (sub-constructors, prefix-only, etc.), so we
        // must NOT remap — the SLA template indices are already correct.
        let opprint = &templates[0].opprint_indices;
        let mut remapped_templates = decoded.op_templates.clone();

        if !opprint.is_empty() {
            let mut handle_remap = vec![usize::MAX; 32];
            for (fission_idx, sla_idx) in opprint.iter().enumerate() {
                if *sla_idx < handle_remap.len() {
                    handle_remap[*sla_idx] = fission_idx;
                }
            }
            for op in &mut remapped_templates {
                remap_op_tpl_handles(op, &handle_remap);
            }
        }

        // Detect SLA templates that reference handle indices beyond what
        // Fission's runtime can resolve (our handles vec has exactly
        // operand_specs.len() entries). If any op_template references a
        // handle index >= operand_specs.len(), we must mark the constructor
        // unsupported rather than panicking at runtime.
        let num_handles = constructor.operand_specs.len();
        let has_unresolvable_handle = remapped_templates.iter().any(|op| {
            let mut max_idx: Option<i64> = None;
            if let Some(ref out) = op.output {
                collect_max_handle_index(out, &mut max_idx);
            }
            for inp in &op.inputs {
                collect_max_handle_index(inp, &mut max_idx);
            }
            max_idx.is_some_and(|idx| idx >= 0 && (idx as usize) >= num_handles)
        });

        if has_unresolvable_handle && !has_unsupported_opcode {
            // The SLA template references handles that Fission's runtime can't
            // resolve (e.g., the `cc` subconstructor in J^cc). If the constructor
            // already has Fission-native semantic ops (ConditionalJump, SetCc,
            // etc.), keep those instead of overwriting with broken SLA templates.
            // This allows Jcc/Setcc/etc. to remain runtime_ready using their
            // native Fission templates.
            let has_native_semantics = !constructor
                .constructor_template
                .semantic_ops
                .is_empty();
            if has_native_semantics {
                // Keep the original Fission-generated op_templates; don't
                // overwrite with the SLA templates that have unresolvable handles.
                // Tag as NativeFission so the evaluator uses the native executor.
                constructor.constructor_template.template_source =
                    CompiledTemplateSource::NativeFission;
                constructor.unsupported_template_kind = None;
                updated += 1;
                continue;
            }
            // No native semantics — mark as unsupported (fail-closed).
            constructor.constructor_template.op_templates = remapped_templates;
            constructor.constructor_template.template_source = CompiledTemplateSource::SpecDerived;
            constructor.runtime_ready = false;
            constructor.unsupported_template_kind =
                Some("sla_template_references_unresolvable_handle".to_string());
        } else {
            constructor.constructor_template.op_templates = remapped_templates;
            constructor.constructor_template.template_source = CompiledTemplateSource::SpecDerived;
            let is_unsupported = has_unsupported_opcode;
            constructor.runtime_ready = !is_unsupported;
            constructor.unsupported_template_kind = if has_unsupported_opcode {
                Some("unsupported_pcode_opcode_in_sla_construct_tpl".to_string())
            } else {
                None
            };
        }
        updated += 1;
    }
    compiled.construct_templates = compiled
        .executable_constructors
        .iter()
        .map(|constructor| CompiledConstructTpl {
            constructor_hash: constructor.signature_hash,
            ops: constructor.constructor_template.semantic_ops.clone(),
            op_templates: constructor.constructor_template.op_templates.clone(),
            template_source: constructor.constructor_template.template_source,
        })
        .collect();
    updated
}

/// Remap all handle index references within a single op template.
fn remap_op_tpl_handles(op: &mut CompiledOpTpl, remap: &[usize]) {
    if let Some(ref mut output) = op.output {
        remap_varnode_tpl_handles(output, remap);
    }
    for input in &mut op.inputs {
        remap_varnode_tpl_handles(input, remap);
    }
}

fn remap_varnode_tpl_handles(vn: &mut CompiledVarnodeTpl, remap: &[usize]) {
    match vn {
        CompiledVarnodeTpl::Varnode { space, offset, size } => {
            remap_space_tpl_handles(space, remap);
            remap_const_tpl_handles(offset, remap);
            remap_const_tpl_handles(size, remap);
        }
        CompiledVarnodeTpl::HandleTpl(ref mut handle) => {
            if let Some(ref mut s) = handle.space {
                remap_space_tpl_handles(s, remap);
            }
            if let Some(ref mut c) = handle.size {
                remap_const_tpl_handles(c, remap);
            }
            if let Some(ref mut s) = handle.ptr_space {
                remap_space_tpl_handles(s, remap);
            }
            if let Some(ref mut c) = handle.ptr_offset {
                remap_const_tpl_handles(c, remap);
            }
            if let Some(ref mut c) = handle.ptr_size {
                remap_const_tpl_handles(c, remap);
            }
            if let Some(ref mut s) = handle.temp_space {
                remap_space_tpl_handles(s, remap);
            }
            if let Some(ref mut c) = handle.temp_offset {
                remap_const_tpl_handles(c, remap);
            }
        }
        _ => {} // Other variants don't have handle references
    }
}

fn remap_space_tpl_handles(space: &mut CompiledSpaceTpl, remap: &[usize]) {
    match space {
        CompiledSpaceTpl::Const(ref mut c) => remap_const_tpl_handles(c, remap),
        _ => {}
    }
}

fn remap_const_tpl_handles(c: &mut CompiledConstTpl, remap: &[usize]) {
    if let CompiledConstTpl::Handle { handle_index, .. } = c {
        let idx = *handle_index as usize;
        if idx < remap.len() {
            let mapped = remap[idx];
            if mapped != usize::MAX {
                *handle_index = mapped as i64;
            } else {
                *handle_index = usize::MAX as i64;
            }
        }
    }
}

/// Walk a VarnodeTpl and record the maximum handle index referenced.
fn collect_max_handle_index(vn: &CompiledVarnodeTpl, max_idx: &mut Option<i64>) {
    match vn {
        CompiledVarnodeTpl::Varnode { space, offset, size } => {
            collect_max_handle_index_from_space(space, max_idx);
            collect_max_handle_index_from_const(offset, max_idx);
            collect_max_handle_index_from_const(size, max_idx);
        }
        CompiledVarnodeTpl::HandleTpl(handle) => {
            if let Some(ref s) = handle.space {
                collect_max_handle_index_from_space(s, max_idx);
            }
            if let Some(ref c) = handle.size {
                collect_max_handle_index_from_const(c, max_idx);
            }
            if let Some(ref s) = handle.ptr_space {
                collect_max_handle_index_from_space(s, max_idx);
            }
            if let Some(ref c) = handle.ptr_offset {
                collect_max_handle_index_from_const(c, max_idx);
            }
            if let Some(ref c) = handle.ptr_size {
                collect_max_handle_index_from_const(c, max_idx);
            }
            if let Some(ref s) = handle.temp_space {
                collect_max_handle_index_from_space(s, max_idx);
            }
            if let Some(ref c) = handle.temp_offset {
                collect_max_handle_index_from_const(c, max_idx);
            }
        }
        _ => {}
    }
}

fn collect_max_handle_index_from_const(c: &CompiledConstTpl, max_idx: &mut Option<i64>) {
    if let CompiledConstTpl::Handle { handle_index, .. } = c {
        *max_idx = Some(max_idx.map_or(*handle_index, |cur| cur.max(*handle_index)));
    }
}

fn collect_max_handle_index_from_space(s: &CompiledSpaceTpl, max_idx: &mut Option<i64>) {
    if let CompiledSpaceTpl::Const(c) = s {
        collect_max_handle_index_from_const(c, max_idx);
    }
}

struct Collector {
    definitions: Vec<CompiledSpecDefinition>,
    macros: Vec<CompiledMacro>,
    constructors: Vec<CompiledConstructor>,
    executable_constructors: Vec<CompiledExecutableConstructor>,
    pcode_ops: BTreeSet<String>,
    pcode_op_sources: BTreeMap<String, String>,
    pattern_nodes: Vec<CompiledPatternNode>,
}

impl Collector {
    fn language_layout(&self) -> CompiledLanguageLayout {
        let mut address_spaces = Vec::new();
        let mut registers = Vec::new();
        let mut token_fields = Vec::new();
        let mut context_fields = Vec::new();
        let mut subtables = Vec::new();
        for definition in &self.definitions {
            match definition.kind.as_str() {
                "space" => address_spaces.push(CompiledAddressSpace {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "register" => registers.push(CompiledRegister {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "token" => token_fields.push(CompiledTokenField {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "context" => context_fields.push(CompiledContextField {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                "table" => subtables.push(CompiledSubtable {
                    name: definition_name(&definition.statement),
                    source: definition.source.clone(),
                }),
                _ => {}
            }
        }
        let display_templates = self
            .constructors
            .iter()
            .map(|constructor| CompiledDisplayTemplate {
                constructor_hash: constructor.signature_hash,
                display: constructor.display.clone(),
            })
            .collect();
        CompiledLanguageLayout {
            address_spaces,
            registers,
            token_fields,
            context_fields,
            subtables,
            display_templates,
        }
    }

    fn construct_templates(&self) -> Vec<CompiledConstructTpl> {
        self.executable_constructors
            .iter()
            .map(|constructor| CompiledConstructTpl {
                constructor_hash: constructor.signature_hash,
                ops: constructor.constructor_template.semantic_ops.clone(),
                op_templates: constructor.constructor_template.op_templates.clone(),
                template_source: constructor.constructor_template.template_source,
            })
            .collect()
    }

    fn collect_items(&mut self, items: &[AstItem], with_stack: &mut Vec<WithContextFrame>) {
        for item in items {
            match item {
                AstItem::Define(definition) => {
                    let kind = definition
                        .statement
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("unknown")
                        .trim_end_matches(';')
                        .to_string();
                    let source = format!(
                        "{}:{}",
                        definition
                            .file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("<unknown>"),
                        definition.line_number
                    );
                    if kind == "pcodeop" {
                        if let Some(name) = definition
                            .statement
                            .split_whitespace()
                            .nth(2)
                            .map(|value| value.trim_end_matches(';').to_string())
                        {
                            self.pcode_ops.insert(name.clone());
                            self.pcode_op_sources.insert(name, source.clone());
                        }
                    }
                    self.definitions.push(CompiledSpecDefinition {
                        kind,
                        source,
                        statement: definition.statement.clone(),
                    });
                }
                AstItem::Macro(m) => {
                    self.macros.push(CompiledMacro {
                        name: macro_name(&m.signature),
                        source: format!(
                            "{}:{}",
                            m.file
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("<unknown>"),
                            m.line_number
                        ),
                        body_line_count: m.body.lines().count(),
                    });
                }
                AstItem::Constructor(c) => {
                    self.record_constructor(c, with_stack);
                }
                AstItem::WithBlock(block) => {
                    with_stack.push(WithContextFrame {
                        header: block.header.clone(),
                    });
                    self.collect_items(&block.items, with_stack);
                    with_stack.pop();
                }
                AstItem::Raw(_) => {}
            }
        }
    }

    fn record_constructor(
        &mut self,
        constructor: &AstConstructor,
        with_stack: &[WithContextFrame],
    ) {
        let mnemonic = constructor_mnemonic(&constructor.signature);
        let source = format!(
            "{}:{}",
            constructor
                .file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>"),
            constructor.line_number
        );
        let control_flow = classify_control_flow(&constructor.body);
        let semantic_ops = constructor_semantic_ops(&constructor.body, &self.pcode_ops);
        let signature_hash = stable_hash(&constructor.signature);
        let semantic_template = CompiledSemanticTemplate {
            status: if constructor.body.trim().is_empty() {
                "empty".to_string()
            } else {
                "unsupported_template".to_string()
            },
            action_hash: stable_hash(&constructor.body),
            op_count: semantic_ops.len(),
        };
        self.pattern_nodes.push(CompiledPatternNode {
            node_id: format!("{source}#{:016x}", signature_hash),
            source: source.clone(),
            mnemonic: mnemonic.clone(),
            with_depth: with_stack.len(),
            control_flow,
        });
        self.constructors.push(CompiledConstructor {
            mnemonic: mnemonic.clone(),
            display: constructor.signature.clone(),
            source: source.clone(),
            control_flow,
            pattern_signature: constructor.signature.clone(),
            semantic_template,
            with_stack: with_stack
                .iter()
                .map(|frame| frame.header.clone())
                .collect(),
            semantic_ops,
            signature_hash,
        });
        if let Some(executable) = compile_executable_constructor(
            &constructor.signature,
            &mnemonic,
            &source,
            signature_hash,
        ) {
            self.executable_constructors.push(executable);
        }
    }
}

fn constructor_mnemonic(signature: &str) -> String {
    signature
        .trim_start_matches(':')
        .split_whitespace()
        .next()
        .unwrap_or("<unknown>")
        .trim_end_matches(',')
        .to_string()
}

fn macro_name(signature: &str) -> String {
    signature
        .strip_prefix("macro ")
        .unwrap_or(signature)
        .split('(')
        .next()
        .unwrap_or("<unknown>")
        .trim()
        .to_string()
}

fn definition_name(statement: &str) -> String {
    statement
        .split_whitespace()
        .nth(2)
        .unwrap_or("<unknown>")
        .trim_matches(|ch| ch == ';' || ch == ':' || ch == '(' || ch == ')')
        .to_string()
}

fn classify_control_flow(body: &str) -> ControlFlowClass {
    let lower = body.to_ascii_lowercase();
    let has_call = lower.contains("call ");
    let has_return = lower.contains("return");
    let has_cbranch = lower.contains("cbranch") || lower.contains("if ");
    let has_branch = lower.contains("goto ") || lower.contains("branch");

    match (has_call, has_return, has_cbranch, has_branch) {
        (false, false, false, false) => ControlFlowClass::None,
        (true, false, false, false) => ControlFlowClass::Call,
        (false, true, false, false) => ControlFlowClass::Return,
        (false, false, true, _) => ControlFlowClass::ConditionalBranch,
        (false, false, false, true) => ControlFlowClass::Branch,
        _ => ControlFlowClass::Mixed,
    }
}

fn constructor_semantic_ops(body: &str, defined_pcode_ops: &BTreeSet<String>) -> Vec<String> {
    let mut found = BTreeSet::new();
    for candidate in defined_pcode_ops {
        let probe = format!("{candidate}(");
        if body.contains(&probe) {
            found.insert(candidate.clone());
        }
    }
    found.into_iter().collect()
}

fn stable_hash(text: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn build_decision_tree(constructors: &[CompiledExecutableConstructor]) -> CompiledDecisionTree {
    let constructor_indexes = (0..constructors.len()).collect::<Vec<_>>();
    let root_probes = decision_probes_for_constructors(constructors);
    let mut nodes = Vec::new();
    let root_node_index =
        build_bucket_node(constructors, &constructor_indexes, &root_probes, &mut nodes);
    let mut buckets = BTreeMap::<String, Vec<usize>>::new();
    for (index, constructor) in constructors.iter().enumerate() {
        buckets
            .entry(constructor.matcher.key())
            .or_default()
            .push(index);
    }
    let root_buckets = buckets
        .into_iter()
        .map(|(key, constructor_indexes)| {
            let node_index =
                build_bucket_node(constructors, &constructor_indexes, &root_probes, &mut nodes);
            CompiledDecisionBucket { key, node_index }
        })
        .collect::<Vec<_>>();
    CompiledDecisionTree {
        root_node_index,
        decision_node_count: nodes.len(),
        nodes,
        root_buckets,
    }
}

fn decision_probes_for_constructors(
    constructors: &[CompiledExecutableConstructor],
) -> Vec<CompiledDecisionProbe> {
    let max_opcode_len = constructors
        .iter()
        .map(|ctor| pattern_matcher_probe_len(&ctor.matcher))
        .max()
        .unwrap_or(1)
        .min(4);

    let mut probes = (0..max_opcode_len)
        .map(|offset| CompiledDecisionProbe::InstructionBitSlice {
            offset: offset as u8,
            mask: 0xff,
            shift: 0,
        })
        .collect::<Vec<_>>();
    probes.extend([
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::InstructionWidthProfile),
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::AddressingForm),
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector),
    ]);
    probes
}

fn pattern_matcher_probe_len(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
    }
}

fn build_bucket_node(
    constructors: &[CompiledExecutableConstructor],
    constructor_indexes: &[usize],
    probes: &[CompiledDecisionProbe],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    if constructor_indexes.len() <= 1 || probes.is_empty() {
        return push_leaf_node(constructors, constructor_indexes, nodes);
    }

    for (probe_pos, probe) in probes.iter().enumerate() {
        let mut wildcard = Vec::new();
        let mut groups = BTreeMap::<u8, Vec<usize>>::new();
        for index in constructor_indexes.iter().copied() {
            let ctor = &constructors[index];
            let values = decision_feature_values(ctor, *probe);
            if values.is_empty() {
                wildcard.push(index);
            } else {
                for value in values {
                    groups.entry(value).or_default().push(index);
                }
            }
        }
        if groups.len() <= 1 {
            continue;
        }

        let remaining = probes[probe_pos + 1..].to_vec();
        let node_index = nodes.len();
        nodes.push(CompiledDecisionNode {
            probe: *probe,
            branches: Vec::new(),
            leaf_constructor_indexes: Vec::new(),
        });
        let mut branches = Vec::new();
        for (value, mut specific) in groups {
            let mut branch_indexes = wildcard.clone();
            branch_indexes.append(&mut specific);
            branch_indexes.sort_unstable();
            branch_indexes.dedup();
            let child_index = build_bucket_node(constructors, &branch_indexes, &remaining, nodes);
            branches.push(CompiledDecisionEdge {
                value,
                next_node_index: child_index,
            });
        }
        branches.sort_by_key(|branch| branch.value);
        nodes[node_index].branches = branches;
        return node_index;
    }

    push_leaf_node(constructors, constructor_indexes, nodes)
}

fn push_leaf_node(
    constructors: &[CompiledExecutableConstructor],
    constructor_indexes: &[usize],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    let mut indexes = constructor_indexes.to_vec();
    indexes.sort_by(|lhs, rhs| {
        decision_specificity(&constructors[*rhs])
            .cmp(&decision_specificity(&constructors[*lhs]))
            .then_with(|| lhs.cmp(rhs))
    });
    indexes.dedup();
    let node_index = nodes.len();
    nodes.push(CompiledDecisionNode {
        probe: CompiledDecisionProbe::Terminal,
        branches: Vec::new(),
        leaf_constructor_indexes: indexes,
    });
    node_index
}

fn decision_feature_values(
    constructor: &CompiledExecutableConstructor,
    probe: CompiledDecisionProbe,
) -> Vec<u8> {
    match probe {
        CompiledDecisionProbe::Terminal => Vec::new(),
        CompiledDecisionProbe::InstructionBitSlice {
            offset,
            mask,
            shift,
        } => instruction_probe_values(&constructor.matcher, offset as usize)
            .into_iter()
            .map(|value| (value & mask) >> shift)
            .collect(),
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::InstructionWidthProfile) => {
            constructor.opsize_variants.clone()
        }
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::AddressingForm) => {
            if let Some(value) = constructor.mod_constraint {
                return vec![value];
            }
            let has_token_bundle = constructor.operand_specs.iter().any(|spec| {
                matches!(
                    spec,
                    CompiledOperandSpec::TokenFieldRm { .. }
                        | CompiledOperandSpec::TokenFieldReg { .. }
                )
            });
            let memory_only = constructor.operand_specs.iter().any(|spec| {
                matches!(
                    spec,
                    CompiledOperandSpec::TokenFieldRm {
                        memory_only: true,
                        ..
                    }
                )
            });
            if memory_only {
                vec![0, 1, 2]
            } else if has_token_bundle {
                vec![0, 1, 2, 3]
            } else {
                Vec::new()
            }
        }
        CompiledDecisionProbe::TokenFieldRef(CompiledTokenFieldRef::RegisterSelector) => {
            constructor.operand_reg_values.clone()
        }
        CompiledDecisionProbe::ContextBitSlice { .. }
        | CompiledDecisionProbe::ContextFieldRef(_)
        | CompiledDecisionProbe::TerminalPatternCheck => Vec::new(),
    }
}

fn instruction_probe_values(matcher: &CompiledPatternMatcher, offset: usize) -> Vec<u8> {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => {
            bytes.get(offset).copied().into_iter().collect()
        }
        CompiledPatternMatcher::RowCc { prefix, row } => {
            if let Some(byte) = prefix.get(offset) {
                return vec![*byte];
            }
            if offset == prefix.len() {
                return (0u8..=15).map(|cc| (row << 4) | cc).collect();
            }
            Vec::new()
        }
        CompiledPatternMatcher::RowPage { row, page } => {
            if offset == 0 {
                return (0u8..=7)
                    .map(|low| (row << 4) | (page << 3) | low)
                    .collect();
            }
            Vec::new()
        }
    }
}

fn decision_specificity(constructor: &CompiledExecutableConstructor) -> usize {
    let mut score = 0usize;
    score += constructor.opsize_variants.len().min(1) * 2;
    score += constructor.operand_reg_values.len().min(1) * 3;
    score += usize::from(constructor.mod_constraint.is_some()) * 2;
    score += constructor
        .operand_specs
        .iter()
        .filter(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::TokenFieldRm {
                    memory_only: true,
                    ..
                }
            )
        })
        .count()
        * 2;
    score += match &constructor.matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
    };
    score
}

fn compile_executable_constructor(
    signature: &str,
    mnemonic: &str,
    source: &str,
    signature_hash: u64,
) -> Option<CompiledExecutableConstructor> {
    if !runtime_signature_is_supported(signature) {
        return None;
    }
    let normalized_mnemonic = normalize_executable_mnemonic(mnemonic);
    let construct_tpl_kind = classify_construct_tpl_kind(&normalized_mnemonic);
    let matcher = parse_opcode_matcher(signature)?;
    let operand_specs = parse_operand_specs(signature, &matcher, construct_tpl_kind).ok()?;
    let mod_constraint = parse_single_value(signature, "mod=");
    let operand_selector_key = format!("{}{}=", "reg_", "opcode");
    let operand_reg_values = parse_value_list(signature, &operand_selector_key);
    let opsize_variants = parse_opsize_variants(signature);
    let unsupported_template_kind =
        unsupported_template_reason(signature, construct_tpl_kind, &operand_specs);
    let runtime_ready = unsupported_template_kind.is_none();
    let constructor_template = build_constructor_template(&operand_specs, construct_tpl_kind);

    Some(CompiledExecutableConstructor {
        mnemonic: normalized_mnemonic,
        source: source.to_string(),
        display: signature.to_string(),
        signature_hash,
        matcher,
        mod_constraint,
        operand_reg_values,
        opsize_variants,
        operand_specs,
        construct_tpl_kind,
        constructor_template,
        runtime_ready,
        unsupported_template_kind,
    })
}

fn normalize_executable_mnemonic(mnemonic: &str) -> String {
    let trimmed = mnemonic.trim();
    if trimmed.eq_ignore_ascii_case("J^cc") {
        return "J^CC".to_string();
    }
    if trimmed.eq_ignore_ascii_case("SET^cc") {
        return "SET^CC".to_string();
    }
    trimmed
        .split('^')
        .next()
        .unwrap_or(trimmed)
        .trim()
        .to_string()
}

fn runtime_signature_is_supported(signature: &str) -> bool {
    if signature.contains("$(LONGMODE_OFF)") {
        return false;
    }
    if signature.contains("$(VEX_") || signature.contains("$(EVEX_") || signature.contains("$(PRE_")
    {
        return false;
    }
    if !signature.contains("vexMode=0") && signature.contains("vexMode=") {
        return false;
    }
    true
}

fn classify_construct_tpl_kind(mnemonic: &str) -> CompiledConstructTplKind {
    match mnemonic.to_ascii_uppercase().as_str() {
        "FINIT" | "FNINIT" => CompiledConstructTplKind::Unsupported,
        "NOP" | "PAUSE" => CompiledConstructTplKind::Nop,
        "RET" => CompiledConstructTplKind::Ret,
        "CALL" => CompiledConstructTplKind::Call,
        "JMP" => CompiledConstructTplKind::Jmp,
        "J^CC" => CompiledConstructTplKind::Jcc,
        "MOV" => CompiledConstructTplKind::Mov,
        "LEA" => CompiledConstructTplKind::AddressOf,
        "PUSH" => CompiledConstructTplKind::StackStore,
        "POP" => CompiledConstructTplKind::StackLoad,
        "LEAVE" => CompiledConstructTplKind::FrameTeardown,
        "ADD" => CompiledConstructTplKind::Add,
        "SUB" => CompiledConstructTplKind::Sub,
        "AND" => CompiledConstructTplKind::And,
        "OR" => CompiledConstructTplKind::Or,
        "XOR" => CompiledConstructTplKind::Xor,
        "IMUL" => CompiledConstructTplKind::Imul,
        "MUL" => CompiledConstructTplKind::Mul,
        "SHL" | "SAL" => CompiledConstructTplKind::Shl,
        "SHR" => CompiledConstructTplKind::Shr,
        "SAR" => CompiledConstructTplKind::Sar,
        "INC" => CompiledConstructTplKind::Inc,
        "DEC" => CompiledConstructTplKind::Dec,
        "CMP" => CompiledConstructTplKind::Cmp,
        "TEST" => CompiledConstructTplKind::Test,
        "MOVZX" => CompiledConstructTplKind::Movzx,
        "MOVSX" => CompiledConstructTplKind::Movsx,
        "MOVSXD" => CompiledConstructTplKind::Movsxd,
        "SET^CC" => CompiledConstructTplKind::Setcc,
        "CBW" => CompiledConstructTplKind::Cbw,
        "CWDE" => CompiledConstructTplKind::Cwde,
        "CDQE" => CompiledConstructTplKind::Cdqe,
        _ => CompiledConstructTplKind::Unsupported,
    }
}

fn parse_opcode_matcher(signature: &str) -> Option<CompiledPatternMatcher> {
    let bytes = parse_byte_sequence(signature);
    if let Some(row) = parse_single_value(signature, "row=") {
        if signature.contains("& cc") {
            return Some(CompiledPatternMatcher::RowCc { prefix: bytes, row });
        }
        if let Some(page) = parse_single_value(signature, "page=") {
            return Some(CompiledPatternMatcher::RowPage { row, page });
        }
    }
    if bytes.is_empty() {
        None
    } else {
        Some(CompiledPatternMatcher::ExactBytes(bytes))
    }
}

fn parse_operand_specs(
    signature: &str,
    matcher: &CompiledPatternMatcher,
    construct_tpl_kind: CompiledConstructTplKind,
) -> Result<Vec<CompiledOperandSpec>> {
    let head = signature
        .trim_start_matches(':')
        .split(" is ")
        .next()
        .unwrap_or(signature);
    let operand_part = head
        .split_whitespace()
        .skip(1)
        .collect::<Vec<_>>()
        .join(" ");
    if operand_part.is_empty() {
        return Ok(Vec::new());
    }

    let mut specs = Vec::new();
    for raw_token in operand_part.split(',') {
        let token = raw_token.trim().trim_matches(|ch| ch == '(' || ch == ')');
        if token.is_empty() {
            continue;
        }
        if let Some(size) = relative_size(token) {
            specs.push(CompiledOperandSpec::Relative { size });
            continue;
        }
        if let Some((size, signed)) = immediate_size(token) {
            specs.push(CompiledOperandSpec::Immediate { size, signed });
            continue;
        }
        if token.eq_ignore_ascii_case("FS")
            || token.eq_ignore_ascii_case("GS")
            || token.eq_ignore_ascii_case("CS")
            || token.eq_ignore_ascii_case("SS")
            || token.eq_ignore_ascii_case("DS")
            || token.eq_ignore_ascii_case("ES")
        {
            return Err(anyhow::anyhow!(
                "segment operand is not executable in first runtime wave"
            ));
        }
        if let Some(size) = fixed_accumulator_size(token) {
            specs.push(CompiledOperandSpec::FixedRegister {
                reg: CompiledFixedRegister::Accumulator,
                size,
            });
            continue;
        }
        if let Some(size) = register_size_token(token) {
            let spec = match matcher {
                CompiledPatternMatcher::RowPage { .. }
                    if token.starts_with("Rmr") || token.starts_with("CRmr") =>
                {
                    CompiledOperandSpec::OpcodeTokenReg { size }
                }
                _ if token.starts_with("Reg")
                    || token == "Sreg"
                    || token == "creg"
                    || token == "creg_x"
                    || token == "debugreg"
                    || token == "debugreg_x" =>
                {
                    CompiledOperandSpec::TokenFieldReg { size }
                }
                _ => CompiledOperandSpec::TokenFieldRm {
                    size,
                    memory_only: token.starts_with('m'),
                },
            };
            specs.push(spec);
            continue;
        }
    }

    if specs.is_empty() {
        return Err(anyhow::anyhow!(
            "no executable operand specs parsed for {signature}"
        ));
    }

    if matches!(construct_tpl_kind, CompiledConstructTplKind::Setcc) && specs.len() != 1 {
        return Err(anyhow::anyhow!("setcc expects one operand"));
    }
    Ok(specs)
}

fn parse_byte_sequence(signature: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut start = 0usize;
    while let Some(pos) = signature[start..].find("byte=0x") {
        let begin = start + pos + "byte=0x".len();
        let hex = signature[begin..]
            .chars()
            .take_while(|ch| ch.is_ascii_hexdigit())
            .collect::<String>();
        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
            bytes.push(byte);
        }
        start = begin + hex.len();
    }
    bytes
}

fn parse_single_value(signature: &str, key: &str) -> Option<u8> {
    let mut search_start = 0usize;
    while let Some(pos) = signature[search_start..].find(key) {
        let absolute = search_start + pos;
        let has_token_boundary = absolute == 0
            || signature[..absolute]
                .chars()
                .next_back()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
        let value_start = absolute + key.len();
        if has_token_boundary {
            let digits = signature[value_start..]
                .chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>();
            if let Ok(value) = digits.parse() {
                return Some(value);
            }
        }
        search_start = value_start;
    }
    None
}

fn parse_value_list(signature: &str, key: &str) -> Vec<u8> {
    if let Some(single) = parse_single_value(signature, key) {
        return vec![single];
    }
    let Some(start) = signature.find(key) else {
        return Vec::new();
    };
    let rest = &signature[start + key.len()..];
    if !rest.starts_with('(') {
        return Vec::new();
    }
    let Some(end) = rest.find(')') else {
        return Vec::new();
    };
    rest[1..end]
        .split('|')
        .filter_map(|value| value.trim().parse().ok())
        .collect()
}

fn parse_opsize_variants(signature: &str) -> Vec<u8> {
    if signature.contains("(opsize=1 | opsize=2)") {
        return vec![1, 2];
    }
    if let Some(opsize) = parse_single_value(signature, "opsize=") {
        return vec![opsize];
    }
    Vec::new()
}

fn unsupported_template_reason(
    signature: &str,
    construct_tpl_kind: CompiledConstructTplKind,
    operand_specs: &[CompiledOperandSpec],
) -> Option<String> {
    if let Some(reason) = unsupported_check_constraint_reason(signature) {
        return Some(reason);
    }

    if signature.contains("currentCS")
        || signature.contains("rexRprefix=")
        || signature.contains("creg")
        || signature.contains("debugreg")
        || signature.contains("xmmmod=")
        || signature.contains("ymmmod=")
        || signature.contains("zmm")
        || signature.contains("bnd")
        || signature.contains("moffs")
    {
        return Some("unsupported_runtime_constraint".to_string());
    }

    match construct_tpl_kind {
        CompiledConstructTplKind::Unsupported => {
            return Some("unsupported_template_kind".to_string());
        }
        CompiledConstructTplKind::Nop
        | CompiledConstructTplKind::Ret
        | CompiledConstructTplKind::Call
        | CompiledConstructTplKind::Jmp
        | CompiledConstructTplKind::Jcc
        | CompiledConstructTplKind::Mov
        | CompiledConstructTplKind::AddressOf
        | CompiledConstructTplKind::StackStore
        | CompiledConstructTplKind::StackLoad
        | CompiledConstructTplKind::FrameTeardown
        | CompiledConstructTplKind::Add
        | CompiledConstructTplKind::Sub
        | CompiledConstructTplKind::And
        | CompiledConstructTplKind::Or
        | CompiledConstructTplKind::Xor
        | CompiledConstructTplKind::Imul
        | CompiledConstructTplKind::Mul
        | CompiledConstructTplKind::Shl
        | CompiledConstructTplKind::Shr
        | CompiledConstructTplKind::Sar
        | CompiledConstructTplKind::Inc
        | CompiledConstructTplKind::Dec
        | CompiledConstructTplKind::Cmp
        | CompiledConstructTplKind::Test
        | CompiledConstructTplKind::Movzx
        | CompiledConstructTplKind::Movsx
        | CompiledConstructTplKind::Movsxd
        | CompiledConstructTplKind::Setcc
        | CompiledConstructTplKind::Cbw
        | CompiledConstructTplKind::Cwde
        | CompiledConstructTplKind::Cdqe => {}
    }

    if operand_specs.len() > 2
        && !matches!(
            construct_tpl_kind,
            CompiledConstructTplKind::StackStore | CompiledConstructTplKind::StackLoad
        )
    {
        return Some("unsupported_operand_arity".to_string());
    }
    None
}

fn unsupported_check_constraint_reason(signature: &str) -> Option<String> {
    for token in signature.split(|ch: char| ch.is_whitespace() || ch == '&' || ch == ';') {
        let trimmed = token.trim_matches(|ch| ch == '(' || ch == ')' || ch == ',');
        if !trimmed.starts_with("check_") {
            continue;
        }
        if matches!(
            trimmed,
            "check_Reg32_dest" | "check_Rmr32_dest" | "check_rm32_dest" | "check_EAX_dest"
        ) {
            continue;
        }
        return Some("unsupported_runtime_constraint".to_string());
    }
    None
}

fn build_constructor_template(
    operand_specs: &[CompiledOperandSpec],
    construct_tpl_kind: CompiledConstructTplKind,
) -> CompiledConstructorTemplate {
    let handles = operand_specs
        .iter()
        .cloned()
        .enumerate()
        .map(|(operand_index, spec)| CompiledHandleTemplate {
            operand_index,
            spec,
        })
        .collect::<Vec<_>>();
    let mut decode_steps = Vec::new();
    if operand_specs.iter().any(|spec| {
        matches!(
            spec,
            CompiledOperandSpec::TokenFieldRm { .. } | CompiledOperandSpec::TokenFieldReg { .. }
        )
    }) {
        decode_steps.push(CompiledOperandDecodeStep::ConsumeTokenFields);
    }
    decode_steps.extend(
        (0..operand_specs.len())
            .map(|operand_index| CompiledOperandDecodeStep::DecodeOperand { operand_index }),
    );
    let semantic_ops = semantic_ops_for_kind(construct_tpl_kind);
    let op_templates = op_templates_for_constructor(operand_specs, construct_tpl_kind);
    CompiledConstructorTemplate {
        handles,
        decode_steps,
        semantic_ops,
        op_templates,
        template_source: CompiledTemplateSource::CompatibilityLowered,
    }
}

fn semantic_ops_for_kind(construct_tpl_kind: CompiledConstructTplKind) -> Vec<CompiledSemanticOp> {
    use CompiledArithmeticOpcode as Arith;
    use CompiledConstructTplKind as Kind;
    use CompiledSemanticOp as Op;

    vec![match construct_tpl_kind {
        Kind::Unsupported => Op::Nop,
        Kind::Nop => Op::Nop,
        Kind::Ret => Op::Return,
        Kind::Call => Op::Call,
        Kind::Jmp => Op::Jump,
        Kind::Jcc => Op::ConditionalJump,
        Kind::Mov => Op::Copy,
        Kind::AddressOf => Op::AddressOf,
        Kind::StackStore => Op::StackStore,
        Kind::StackLoad => Op::StackLoad,
        Kind::FrameTeardown => Op::FrameTeardown,
        Kind::Add => Op::Binary { opcode: Arith::Add },
        Kind::Sub => Op::Binary { opcode: Arith::Sub },
        Kind::And => Op::Binary { opcode: Arith::And },
        Kind::Or => Op::Binary { opcode: Arith::Or },
        Kind::Xor => Op::Binary { opcode: Arith::Xor },
        Kind::Imul | Kind::Mul => Op::Binary { opcode: Arith::Mul },
        Kind::Shl => Op::Binary { opcode: Arith::Shl },
        Kind::Shr => Op::Binary { opcode: Arith::Shr },
        Kind::Sar => Op::Binary { opcode: Arith::Sar },
        Kind::Inc => Op::Binary { opcode: Arith::Inc },
        Kind::Dec => Op::Binary { opcode: Arith::Dec },
        Kind::Cmp => Op::Compare { bitwise: false },
        Kind::Test => Op::Compare { bitwise: true },
        Kind::Movzx => Op::Extend { signed: false },
        Kind::Movsx | Kind::Movsxd => Op::Extend { signed: true },
        Kind::Setcc => Op::SetCc,
        Kind::Cbw => Op::AccumulatorExtend {
            src_size: 1,
            dst_size: 2,
        },
        Kind::Cwde => Op::AccumulatorExtend {
            src_size: 2,
            dst_size: 4,
        },
        Kind::Cdqe => Op::AccumulatorExtend {
            src_size: 4,
            dst_size: 8,
        },
    }]
}

fn op_templates_for_constructor(
    operand_specs: &[CompiledOperandSpec],
    construct_tpl_kind: CompiledConstructTplKind,
) -> Vec<CompiledOpTpl> {
    use CompiledConstTpl as ConstTpl;
    use CompiledConstructTplKind as Kind;
    use CompiledFixedRegister as FixedReg;
    use CompiledOpTplOpcode as Opcode;
    use CompiledVarnodeTpl as VnTpl;

    let handle = |operand_index| VnTpl::Handle { operand_index };
    let effective_address = |operand_index| VnTpl::EffectiveAddress { operand_index };
    let condition_predicate = || VnTpl::ConditionPredicate;
    let temp = |id, size| VnTpl::Temp { id, size };
    let fixed = |reg, size| VnTpl::FixedRegister { reg, size };
    let flag = |bit| VnTpl::Flag { bit };
    let sized_const = |value: i64, size: u32| VnTpl::Const(ConstTpl::Integer { value, size });
    let binary_tpl = |opcode| {
        vec![CompiledOpTpl {
            opcode,
            output: Some(handle(0)),
            inputs: vec![handle(0), handle(1)],
            label: None,
        }]
    };

    match construct_tpl_kind {
        Kind::Nop | Kind::Unsupported => Vec::new(),
        Kind::Ret => vec![
            CompiledOpTpl {
                opcode: Opcode::Load,
                output: Some(temp(0, 8)),
                inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::IntAdd,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Return,
                output: None,
                inputs: vec![temp(0, 8)],
                label: None,
            },
        ],
        Kind::Call => vec![
            CompiledOpTpl {
                opcode: Opcode::IntSub,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Store,
                output: None,
                inputs: vec![
                    sized_const(0, 8),
                    fixed(FixedReg::StackPointer, 8),
                    VnTpl::Const(ConstTpl::InstNext),
                ],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Call,
                output: None,
                inputs: vec![handle(0)],
                label: None,
            },
        ],
        Kind::Jmp => vec![CompiledOpTpl {
            opcode: Opcode::Branch,
            output: None,
            inputs: vec![handle(0)],
            label: None,
        }],
        Kind::Mov => {
            if operand_specs.len() < 2 {
                return Vec::new();
            }
            vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::Movzx => {
            if operand_specs.len() < 2 {
                return Vec::new();
            }
            vec![CompiledOpTpl {
                opcode: Opcode::IntZExt,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::Movsx | Kind::Movsxd => {
            if operand_specs.len() < 2 {
                return Vec::new();
            }
            vec![CompiledOpTpl {
                opcode: Opcode::IntSExt,
                output: Some(handle(0)),
                inputs: vec![handle(1)],
                label: None,
            }]
        }
        Kind::AddressOf => {
            if operand_specs.len() < 2 {
                return Vec::new();
            }
            let dst_size = operand_spec_size(&operand_specs[0]);
            let mut ops = Vec::new();
            if dst_size < 8 {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Subpiece,
                    output: Some(temp(0, dst_size)),
                    inputs: vec![effective_address(1), sized_const(0, 8)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![temp(0, dst_size)],
                    label: None,
                });
            } else {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![effective_address(1)],
                    label: None,
                });
            }
            ops
        }
        Kind::StackStore => {
            if operand_specs.is_empty() {
                return Vec::new();
            }
            let value_size = operand_spec_size(&operand_specs[0]);
            let stack_size = value_size.max(8);
            vec![
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(temp(0, 8)),
                    inputs: vec![fixed(FixedReg::StackPointer, 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntSub,
                    output: Some(fixed(FixedReg::StackPointer, 8)),
                    inputs: vec![temp(0, 8), sized_const(i64::from(stack_size), 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Store,
                    output: None,
                    inputs: vec![
                        sized_const(0, 8),
                        fixed(FixedReg::StackPointer, 8),
                        handle(0),
                    ],
                    label: None,
                },
            ]
        }
        Kind::StackLoad => {
            if operand_specs.is_empty() {
                return Vec::new();
            }
            let value_size = operand_spec_size(&operand_specs[0]);
            let stack_size = value_size.max(8);
            vec![
                CompiledOpTpl {
                    opcode: Opcode::Load,
                    output: Some(temp(0, stack_size)),
                    inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(handle(0)),
                    inputs: vec![temp(0, stack_size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAdd,
                    output: Some(fixed(FixedReg::StackPointer, 8)),
                    inputs: vec![
                        fixed(FixedReg::StackPointer, 8),
                        sized_const(i64::from(stack_size), 8),
                    ],
                    label: None,
                },
            ]
        }
        Kind::FrameTeardown => vec![
            CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::FramePointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Load,
                output: Some(temp(0, 8)),
                inputs: vec![sized_const(0, 8), fixed(FixedReg::StackPointer, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(fixed(FixedReg::FramePointer, 8)),
                inputs: vec![temp(0, 8)],
                label: None,
            },
            CompiledOpTpl {
                opcode: Opcode::IntAdd,
                output: Some(fixed(FixedReg::StackPointer, 8)),
                inputs: vec![fixed(FixedReg::StackPointer, 8), sized_const(8, 8)],
                label: None,
            },
        ],
        Kind::Cmp | Kind::Test => {
            if operand_specs.len() < 2 {
                return Vec::new();
            }
            let size = operand_specs
                .iter()
                .take(2)
                .map(operand_spec_size)
                .max()
                .unwrap_or(1)
                .max(1);
            let is_test = matches!(construct_tpl_kind, Kind::Test);
            let mut ops = vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(temp(0, size)),
                inputs: vec![handle(0)],
                label: None,
            }];
            if is_test {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(1, size)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(0)),
                    inputs: vec![sized_const(0, 1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(11)),
                    inputs: vec![sized_const(0, 1)],
                    label: None,
                });
            } else {
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntLess,
                    output: Some(temp(2, 1)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntSBorrow,
                    output: Some(temp(3, 1)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::IntSub,
                    output: Some(temp(1, size)),
                    inputs: vec![temp(0, size), handle(1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(0)),
                    inputs: vec![temp(2, 1)],
                    label: None,
                });
                ops.push(CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(11)),
                    inputs: vec![temp(3, 1)],
                    label: None,
                });
            }
            ops.extend([
                CompiledOpTpl {
                    opcode: Opcode::IntSLess,
                    output: Some(temp(4, 1)),
                    inputs: vec![temp(1, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntEqual,
                    output: Some(temp(5, 1)),
                    inputs: vec![temp(1, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(6, size)),
                    inputs: vec![temp(1, size), sized_const(0xff, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::PopCount,
                    output: Some(temp(7, size)),
                    inputs: vec![temp(6, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntAnd,
                    output: Some(temp(8, size)),
                    inputs: vec![temp(7, size), sized_const(1, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::IntEqual,
                    output: Some(temp(9, 1)),
                    inputs: vec![temp(8, size), sized_const(0, size)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(7)),
                    inputs: vec![temp(4, 1)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(6)),
                    inputs: vec![temp(5, 1)],
                    label: None,
                },
                CompiledOpTpl {
                    opcode: Opcode::Copy,
                    output: Some(flag(2)),
                    inputs: vec![temp(9, 1)],
                    label: None,
                },
            ]);
            ops
        }
        Kind::Add => binary_tpl(Opcode::IntAdd),
        Kind::Sub => binary_tpl(Opcode::IntSub),
        Kind::And => binary_tpl(Opcode::IntAnd),
        Kind::Or => binary_tpl(Opcode::IntOr),
        Kind::Xor => binary_tpl(Opcode::IntXor),
        Kind::Imul | Kind::Mul => binary_tpl(Opcode::IntMult),
        Kind::Shl => binary_tpl(Opcode::IntLeft),
        Kind::Shr => binary_tpl(Opcode::IntRight),
        Kind::Sar => binary_tpl(Opcode::IntSRight),
        Kind::Inc | Kind::Dec => {
            let Some(size) = operand_specs.first().map(operand_spec_size) else {
                return Vec::new();
            };
            vec![CompiledOpTpl {
                opcode: match construct_tpl_kind {
                    Kind::Inc => Opcode::IntAdd,
                    Kind::Dec => Opcode::IntSub,
                    _ => unreachable!(),
                },
                output: Some(handle(0)),
                inputs: vec![handle(0), sized_const(1, size)],
                label: None,
            }]
        }
        Kind::Cbw => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 2)),
            inputs: vec![fixed(FixedReg::Accumulator, 1)],
            label: None,
        }],
        Kind::Cwde => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 4)),
            inputs: vec![fixed(FixedReg::Accumulator, 2)],
            label: None,
        }],
        Kind::Cdqe => vec![CompiledOpTpl {
            opcode: Opcode::IntSExt,
            output: Some(fixed(FixedReg::Accumulator, 8)),
            inputs: vec![fixed(FixedReg::Accumulator, 4)],
            label: None,
        }],
        Kind::Jcc => {
            if operand_specs.is_empty() {
                return Vec::new();
            }
            vec![CompiledOpTpl {
                opcode: Opcode::CBranch,
                output: None,
                inputs: vec![handle(0), condition_predicate()],
                label: None,
            }]
        }
        Kind::Setcc => {
            if operand_specs.is_empty() {
                return Vec::new();
            }
            vec![CompiledOpTpl {
                opcode: Opcode::Copy,
                output: Some(handle(0)),
                inputs: vec![condition_predicate()],
                label: None,
            }]
        }
    }
}

fn operand_spec_size(spec: &CompiledOperandSpec) -> u32 {
    match spec {
        CompiledOperandSpec::TokenFieldRm { size, .. }
        | CompiledOperandSpec::TokenFieldReg { size }
        | CompiledOperandSpec::OpcodeTokenReg { size }
        | CompiledOperandSpec::Immediate { size, .. }
        | CompiledOperandSpec::Relative { size }
        | CompiledOperandSpec::FixedRegister { size, .. } => *size,
    }
}

fn register_size_token(token: &str) -> Option<u32> {
    let digits = token
        .chars()
        .rev()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    if digits.is_empty() {
        return match token {
            "AL" => Some(1),
            "AX" => Some(2),
            "EAX" => Some(4),
            "RAX" => Some(8),
            "FS" | "GS" | "CS" | "SS" | "DS" | "ES" => Some(2),
            _ => None,
        };
    }
    digits.parse::<u32>().ok().map(|bits| (bits / 8).max(1))
}

fn relative_size(token: &str) -> Option<u32> {
    if !token.starts_with("rel") {
        return None;
    }
    register_size_token(token)
}

fn immediate_size(token: &str) -> Option<(u32, bool)> {
    if !(token.starts_with("imm") || token.starts_with("simm")) {
        return None;
    }
    let signed = token.starts_with("simm");
    let digits = token
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let bits = digits.parse::<u32>().ok()?;
    Some(((bits / 8).max(1), signed))
}

fn fixed_accumulator_size(token: &str) -> Option<u32> {
    match token {
        "AL" => Some(1),
        "AX" => Some(2),
        "EAX" => Some(4),
        "RAX" => Some(8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{
        compile_frontend_for_entry_spec, expand_entry_spec, infer_arch_from_entry_spec,
        parse_expanded_spec, x86_64_entry_spec_path,
    };

    #[test]
    fn compile_frontend_collects_pcode_ops_and_patterns() {
        let entry_spec = x86_64_entry_spec_path();
        let expanded = expand_entry_spec(&entry_spec).expect("expand spec");
        let ast = parse_expanded_spec(&expanded).expect("parse spec");
        let arch = infer_arch_from_entry_spec(&entry_spec).expect("infer arch");
        let compiled = compile_frontend(&arch, &expanded, &ast).expect("compile frontend");
        assert!(!compiled.pcode_ops.is_empty());
        assert!(!compiled.pattern_nodes.is_empty());
        assert!(compiled
            .constructors
            .iter()
            .any(|item| item.mnemonic.eq_ignore_ascii_case("RET")
                || item.control_flow != ControlFlowClass::None));
        assert!(!compiled.language_layout.address_spaces.is_empty());
        assert!(!compiled.language_layout.registers.is_empty());
        assert!(!compiled.language_layout.display_templates.is_empty());
        assert!(!compiled.construct_templates.is_empty());
        assert!(compiled
            .decision_tree
            .nodes
            .iter()
            .any(|node| matches!(node.probe, CompiledDecisionProbe::TokenFieldRef(_))));
    }

    #[test]
    fn compile_frontend_for_entry_spec_sets_arch_from_path() {
        let compiled = compile_frontend_for_entry_spec(&x86_64_entry_spec_path())
            .expect("compile generic entry spec");
        assert_eq!(compiled.arch, "x86");
    }

    #[test]
    fn control_flow_classifier_separates_branch_from_none() {
        assert_eq!(
            classify_control_flow("tmp = x + y;"),
            ControlFlowClass::None
        );
        assert_eq!(
            classify_control_flow("goto inst_next;"),
            ControlFlowClass::Branch
        );
        assert_eq!(
            classify_control_flow("if cond goto inst_next;"),
            ControlFlowClass::ConditionalBranch
        );
    }
}
