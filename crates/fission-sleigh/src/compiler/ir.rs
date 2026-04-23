use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
use super::preprocessor::ExpandedSpec;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompiledConstructTplKind {
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
    let construct_tpl_kind = classify_construct_tpl_kind(&normalized_mnemonic)?;
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

fn classify_construct_tpl_kind(mnemonic: &str) -> Option<CompiledConstructTplKind> {
    Some(match mnemonic.to_ascii_uppercase().as_str() {
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
        _ => return None,
    })
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
    let start = signature.find(key)? + key.len();
    let digits = signature[start..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
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
    if signature.contains("currentCS")
        || signature.contains("check_")
        || signature.contains("bit64=")
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
    CompiledConstructorTemplate {
        handles,
        decode_steps,
        semantic_ops,
    }
}

fn semantic_ops_for_kind(construct_tpl_kind: CompiledConstructTplKind) -> Vec<CompiledSemanticOp> {
    use CompiledArithmeticOpcode as Arith;
    use CompiledSemanticOp as Op;
    use CompiledConstructTplKind as Kind;

    vec![match construct_tpl_kind {
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
        assert!(compiled.decision_tree.nodes.iter().any(|node| matches!(
            node.probe,
            CompiledDecisionProbe::TokenFieldRef(_)
        )));
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
