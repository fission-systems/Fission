use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};

use super::*;
use crate::compiler::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
use crate::compiler::preprocessor::{ExpandedSpec, PreprocessedLine};
use crate::compiler::sla::CompiledSlaTemplateLibrary;

pub fn compile_frontend(
    arch: &str,
    expanded: &ExpandedSpec,
    ast_result: Result<SpecAst>,
    entry_spec: &Path,
) -> Result<CompiledFrontend> {
    let mut collector = Collector {
        definitions: Vec::new(),
        macros: Vec::new(),
        constructors: Vec::new(),
        subtable_executables: BTreeMap::new(),
        pcode_ops: BTreeSet::new(),
        pcode_op_sources: BTreeMap::new(),
        default_context: 0,
        pattern_nodes: Vec::new(),
        field_info: BTreeMap::new(),
    };

    if let Ok(ast) = ast_result {
        collector.collect_items(&ast.items, &mut Vec::new());
    }
    collector.collect_define_bits_from_expanded(&expanded.lines);

    // Infer default context from .pspec if available
    let (default_context, default_context_known_mask) =
        infer_default_context_from_pspec(entry_spec, &collector.field_info)?;
    collector.default_context = default_context;
    eprintln!(
        "Inferred Default Context for {}: 0x{:016x}",
        arch, collector.default_context
    );

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

    let mut subtables = BTreeMap::new();
    for (name, constructors) in &collector.subtable_executables {
        let mut sorted_constructors = constructors.clone();
        sorted_constructors.sort_by_key(|ctor| std::cmp::Reverse(decision_specificity(ctor)));
        let decision_tree = build_decision_tree(&sorted_constructors);
        subtables.insert(
            name.clone(),
            CompiledSubtableDefinition {
                name: name.clone(),
                sla_subtable_id: 0,
                constructors_by_sla_id: constructors_by_sla_id(&sorted_constructors),
                constructors: sorted_constructors,
                decision_tree,
            },
        );
    }

    // Ensure "instruction" subtable exists as it's the primary entry point
    if !subtables.contains_key("instruction") {
        subtables.insert(
            "instruction".to_string(),
            CompiledSubtableDefinition {
                name: "instruction".to_string(),
                sla_subtable_id: 0,
                constructors_by_sla_id: BTreeMap::new(),
                constructors: Vec::new(),
                decision_tree: CompiledDecisionTree {
                    root_node_index: 0,
                    nodes: Vec::new(),
                    decision_node_count: 0,
                    root_buckets: Vec::new(),
                },
            },
        );
    }

    Ok(CompiledFrontend {
        arch: arch.to_string(),
        default_context: collector.default_context,
        default_context_known_mask,
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
        subtables,
        language_layout,
        construct_templates,
        pcode_ops,
        pattern_nodes: collector.pattern_nodes,
        sla_spaces: BTreeMap::new(),
        sla_unique_space_index: u64::MAX,
        sla_register_space_index: u64::MAX,
        sla_uniqbase: 0,
        sla_uniqmask: u64::MAX,
    })
}

fn infer_default_context_from_pspec(
    entry_spec: &Path,
    field_info: &BTreeMap<String, FieldBitRange>,
) -> Result<(u64, u64)> {
    let pspec_path = entry_spec.with_extension("pspec");
    if !pspec_path.exists() {
        return Ok((0, 0));
    }

    let content = fs::read_to_string(&pspec_path)
        .with_context(|| format!("read pspec {}", pspec_path.display()))?;
    let mut default_context = 0u64;
    let mut default_context_known_mask = 0u64;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("<set ") {
            if let Some(name) = extract_xml_attribute(line, "name") {
                if let Some(val_str) = extract_xml_attribute(line, "val") {
                    let val = if val_str.starts_with("0x") {
                        u64::from_str_radix(&val_str[2..], 16).unwrap_or(0)
                    } else {
                        val_str.parse::<u64>().unwrap_or(0)
                    };

                    if let Some(info) = field_info.get(&name) {
                        set_packed_context_bits(
                            &mut default_context,
                            info.bit_offset,
                            info.bit_width,
                            val,
                        )?;
                        let known_value = if info.bit_width >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << info.bit_width) - 1
                        };
                        set_packed_context_bits(
                            &mut default_context_known_mask,
                            info.bit_offset,
                            info.bit_width,
                            known_value,
                        )?;
                    }
                }
            }
        }
    }
    Ok((default_context, default_context_known_mask))
}

fn set_packed_context_bits(
    context_register: &mut u64,
    startbit: u32,
    bitsize: u32,
    value: u64,
) -> Result<()> {
    if bitsize == 0 {
        return Ok(());
    }
    if bitsize > 64 {
        return Err(anyhow!(
            "packed context bit write must be 1..=64 bits, got {bitsize}"
        ));
    }

    let mut remaining = bitsize;
    let mut word_index = startbit / 32;
    let mut bit_offset = startbit % 32;
    while remaining > 0 {
        let chunk_bits = remaining.min(32 - bit_offset);
        let chunk_mask = if chunk_bits >= 32 {
            u32::MAX
        } else {
            (1u32 << chunk_bits) - 1
        };
        let word_shift = 32 - chunk_bits - bit_offset;
        let value_shift = remaining - chunk_bits;
        let chunk_value = ((value >> value_shift) as u32) & chunk_mask;
        set_packed_context_word(
            context_register,
            word_index,
            chunk_value << word_shift,
            chunk_mask << word_shift,
        )?;
        remaining -= chunk_bits;
        word_index += 1;
        bit_offset = 0;
    }
    Ok(())
}

fn set_packed_context_word(
    context_register: &mut u64,
    index: u32,
    value: u32,
    mask: u32,
) -> Result<()> {
    let shift = match index {
        0 => 0,
        1 => 32,
        _ => return Err(anyhow!("packed context word index {index} is out of range")),
    };
    let shifted_mask = u64::from(mask) << shift;
    let shifted_value = u64::from(value & mask) << shift;
    *context_register &= !shifted_mask;
    *context_register |= shifted_value;
    Ok(())
}

fn extract_xml_attribute(line: &str, attr: &str) -> Option<String> {
    let key = format!("{}=\"", attr);
    if let Some(start) = line.find(&key) {
        let after = &line[start + key.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    }
    None
}

pub fn build_frontend_from_sla_native_model(
    compiled: &mut CompiledFrontend,
    library: &CompiledSlaTemplateLibrary,
) -> usize {
    let mut updated = 0usize;

    // Propagate SLA-native space metadata so the runtime never uses hardcoded
    // space IDs. These replace UNIQUE_SPACE_ID=3, register space=4, and the
    // 0x9300 unique offset seed that were previously architecture-specific constants.
    compiled.sla_spaces = library.spaces.clone();
    compiled.sla_unique_space_index = library.unique_space_index;
    compiled.sla_register_space_index = library.register_space_index;
    compiled.sla_uniqbase = library.uniqbase;
    compiled.sla_uniqmask = library.uniqmask;
    // Once a packaged .sla library is available, runtime execution must be
    // driven by that constructor identity domain only.
    compiled.subtables.clear();

    // The compiled .sla artifact is the canonical executable identity. Ghidra
    // decision leaves resolve subtable-local constructor ids; Fission must
    // preserve and execute that same identity domain instead of remapping
    // through .slaspec source lines, display text, or local constructor order.
    for (name, sla_subtable) in &library.subtables {
        let mut executable_constructors = Vec::with_capacity(sla_subtable.constructors.len());
        for (idx, sla_template) in sla_subtable.constructors.iter().enumerate() {
            executable_constructors.push(executable_constructor_from_sla_template(
                name,
                idx,
                sla_template,
            ));
            updated += 1;
        }

        let decision_tree =
            sla_subtable
                .decision_tree
                .clone()
                .unwrap_or_else(|| CompiledDecisionTree {
                    root_node_index: 0,
                    nodes: Vec::new(),
                    decision_node_count: 0,
                    root_buckets: Vec::new(),
                });

        compiled.subtables.insert(
            name.clone(),
            CompiledSubtableDefinition {
                name: name.clone(),
                sla_subtable_id: sla_subtable
                    .constructors
                    .iter()
                    .map(|constructor| constructor.subtable_id)
                    .next()
                    .unwrap_or(0),
                constructors_by_sla_id: constructors_by_sla_id(&executable_constructors),
                constructors: executable_constructors,
                decision_tree,
            },
        );
    }

    // 3. Populate construct_templates list for the runtime emitter
    compiled.construct_templates = compiled
        .subtables
        .values()
        .flat_map(|subtable| &subtable.constructors)
        .map(|constructor| CompiledConstructTpl {
            constructor_hash: constructor.signature_hash,
            num_labels: constructor.constructor_template.num_labels,
            result: constructor.constructor_template.result.clone(),
            ops: constructor.constructor_template.ops.clone(),
        })
        .collect();

    updated
}

fn constructors_by_sla_id(constructors: &[CompiledExecutableConstructor]) -> BTreeMap<u32, usize> {
    constructors
        .iter()
        .enumerate()
        .filter_map(|(index, constructor)| {
            constructor
                .sla_identity
                .as_ref()
                .map(|identity| (identity.constructor_id, index))
        })
        .collect()
}

fn executable_constructor_from_sla_template(
    subtable_name: &str,
    local_index: usize,
    sla_template: &crate::compiler::sla::CompiledSlaConstructorTemplate,
) -> CompiledExecutableConstructor {
    let source = format!(
        "sla:{}:{}:{}",
        subtable_name, sla_template.id, sla_template.source_key
    );
    let signature_hash = stable_hash(&source) ^ (u64::from(sla_template.id) << 32);
    let mut display_template = sla_template.display_template.clone();
    display_template.constructor_hash = signature_hash;

    let mut decode_steps = Vec::new();
    if let Some(flowthru_operand_index) = sla_template.flowthru_operand_index {
        if let Some(CompiledOperandSpec::SubtableEvaluation { table_name, .. }) =
            sla_template.operand_specs.get(flowthru_operand_index)
        {
            decode_steps.push(CompiledOperandDecodeStep::DescendSubtable {
                table_name: table_name.clone(),
                replace_current: true,
            });
        }
    }
    if decode_steps.is_empty() {
        decode_steps.extend(
            (0..sla_template.operand_specs.len())
                .map(|operand_index| CompiledOperandDecodeStep::DecodeOperand { operand_index }),
        );
    }

    let unsupported_template_kind = sla_constructor_unsupported_reason(sla_template);
    let decode_failed = sla_template.decode_status != CompiledSlaDecodeStatus::Decoded;
    CompiledExecutableConstructor {
        constructor_id: sla_template.id,
        sla_identity: Some(CompiledSlaConstructorIdentity {
            subtable_id: sla_template.subtable_id,
            subtable_name: subtable_name.to_string(),
            constructor_id: sla_template.id,
            constructor_slot: sla_template.constructor_slot,
            source_file: sla_template.source_file.clone(),
            source_line: sla_template.line,
        }),
        sla_decode_status: if decode_failed {
            CompiledSlaDecodeStatus::Unsupported
        } else {
            CompiledSlaDecodeStatus::Decoded
        },
        mnemonic: constructor_mnemonic_from_display(&display_template)
            .unwrap_or_else(|| format!("ctor_{}", local_index)),
        source,
        display: display_template.display.clone(),
        display_template,
        signature_hash,
        minimum_length: sla_template.minimum_length,
        context_changes: sla_template.context_changes.clone(),
        context_commits: sla_template.context_commits.clone(),
        matcher: CompiledPatternMatcher::BitConstraints(vec![]),
        opsize_variants: Vec::new(),
        operand_specs: sla_template.operand_specs.clone(),
        display_operands: sla_template.display_operands.clone(),
        construct_tpl_kind: CompiledConstructTplKind::Generic,
        constructor_template: CompiledConstructorTemplate {
            handles: sla_template
                .operand_specs
                .iter()
                .cloned()
                .enumerate()
                .map(|(operand_index, spec)| {
                    let minimum_length = sla_template
                        .operand_minimum_lengths
                        .get(operand_index)
                        .copied()
                        .unwrap_or(0);
                    CompiledHandleTemplate {
                        operand_index,
                        spec,
                        minimum_length,
                    }
                })
                .collect(),
            decode_steps,
            num_labels: sla_template.constructor_template.num_labels,
            result: sla_template.constructor_template.result.clone(),
            ops: sla_template.constructor_template.ops.clone(),
            template_source: CompiledTemplateSource::SpecDerived,
        },
        named_templates: sla_template.named_templates.clone(),
        runtime_ready: unsupported_template_kind.is_none(),
        unsupported_template_kind,
    }
}

fn sla_constructor_unsupported_reason(
    sla_template: &crate::compiler::sla::CompiledSlaConstructorTemplate,
) -> Option<String> {
    if sla_template.decode_status != CompiledSlaDecodeStatus::Decoded {
        let reason = sla_template
            .decode_error
            .as_deref()
            .unwrap_or("unknown_decode_failure");
        return Some(format!("sla_constructor_decode_failed:{reason}"));
    }
    sla_template
        .constructor_template
        .ghidra_template_shape_error()
        .map(|reason| format!("sla_construct_tpl_contains_{reason}"))
}

fn constructor_mnemonic_from_display(template: &CompiledDisplayTemplate) -> Option<String> {
    template.pieces.iter().find_map(|piece| {
        let CompiledDisplayPiece::Literal(text) = piece else {
            return None;
        };
        let mnemonic = text.trim();
        (!mnemonic.is_empty()).then(|| mnemonic.to_ascii_lowercase())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Instruction,
    Context,
}

pub(super) struct FieldBitRange {
    pub(super) bit_offset: u32,
    pub(super) bit_width: u32,
    pub(super) kind: FieldKind,
}

pub(super) struct Collector {
    pub(super) definitions: Vec<CompiledSpecDefinition>,
    pub(super) macros: Vec<CompiledMacro>,
    pub(super) constructors: Vec<CompiledConstructor>,
    pub(super) subtable_executables: BTreeMap<String, Vec<CompiledExecutableConstructor>>,
    pub(super) pcode_ops: BTreeSet<String>,
    pub(super) pcode_op_sources: BTreeMap<String, String>,
    pub(super) default_context: u64,
    pub(super) pattern_nodes: Vec<CompiledPatternNode>,
    pub(super) field_info: BTreeMap<String, FieldBitRange>,
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
                "token" => {
                    let name = definition_name(&definition.statement);
                    let info = self.field_info.get(&name);
                    token_fields.push(CompiledTokenField {
                        name,
                        bit_offset: info.map(|i| i.bit_offset).unwrap_or(0),
                        bit_width: info.map(|i| i.bit_width).unwrap_or(0),
                        source: definition.source.clone(),
                    })
                }
                "context" => {
                    let name = definition_name(&definition.statement);
                    let info = self.field_info.get(&name);
                    context_fields.push(CompiledContextField {
                        name,
                        bit_offset: info.map(|i| i.bit_offset).unwrap_or(0),
                        bit_width: info.map(|i| i.bit_width).unwrap_or(0),
                        source: definition.source.clone(),
                    })
                }
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
                pieces: Vec::new(),
                first_whitespace: None,
                flowthru_operand_index: None,
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
        self.subtable_executables
            .values()
            .flatten()
            .map(|constructor| CompiledConstructTpl {
                constructor_hash: constructor.signature_hash,
                num_labels: constructor.constructor_template.num_labels,
                result: constructor.constructor_template.result.clone(),
                ops: constructor.constructor_template.ops.clone(),
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
                    if kind == "token" || kind == "context" {
                        self.parse_define_bits(&definition.statement, &kind);
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
                AstItem::Constructor(constructor) => {
                    self.record_constructor(constructor, with_stack);
                }
                AstItem::WithBlock(with) => {
                    with_stack.push(WithContextFrame {
                        header: with.header.clone(),
                    });
                    self.collect_items(&with.items, with_stack);
                    with_stack.pop();
                }
                AstItem::Raw(_) => {}
            }
        }
    }

    fn collect_define_bits_from_expanded(&mut self, lines: &[PreprocessedLine]) {
        let mut pending: Option<(String, String)> = None;

        for line in lines {
            let trimmed = strip_comments(&line.text).trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((kind, statement)) = pending.as_mut() {
                statement.push('\n');
                statement.push_str(trimmed);
                if trimmed.contains(';') {
                    let (kind, statement) = pending.take().expect("pending define bits");
                    self.parse_define_bits(&statement, &kind);
                }
                continue;
            }

            let Some(kind) = define_bits_kind(trimmed) else {
                continue;
            };
            if trimmed.contains(';') {
                self.parse_define_bits(trimmed, kind);
            } else {
                pending = Some((kind.to_string(), trimmed.to_string()));
            }
        }

        if let Some((kind, statement)) = pending {
            self.parse_define_bits(&statement, &kind);
        }
    }

    fn record_constructor(
        &mut self,
        constructor: &AstConstructor,
        with_stack: &[WithContextFrame],
    ) {
        // Hierarchical subtable name extraction
        let mut table_name = "instruction".to_string();
        for frame in with_stack {
            let header = frame.header.trim();
            if let Some(pos) = header.find(':') {
                let name = header[..pos].trim();
                if !name.is_empty()
                    && name.len() <= 64
                    && !name.contains(' ')
                    && !name.contains('=')
                {
                    table_name = name.to_string();
                }
            }
        }
        if let Some(pos) = constructor.signature.find(':') {
            let name = constructor.signature[..pos].trim();
            if !name.is_empty() && name.len() <= 64 && !name.contains(' ') && !name.contains('=') {
                table_name = name.to_string();
            }
        }

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

        let mut full_signature = String::new();
        for frame in with_stack {
            full_signature.push_str(&frame.header);
            full_signature.push_str(" & ");
        }
        full_signature.push_str(&constructor.signature);

        let context_changes = parse_context_changes(&full_signature, &self.field_info);

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
            context_changes: context_changes.clone(),
        });

        if let Some(executable) = self.compile_executable_constructor(
            &full_signature,
            &mnemonic,
            &source,
            signature_hash,
            context_changes,
        ) {
            let mut executable = executable;
            executable.constructor_id = u32::MAX; // To be set by apply_sla
            self.subtable_executables
                .entry(table_name)
                .or_default()
                .push(executable);
        }
    }

    fn compile_executable_constructor(
        &self,
        signature: &str,
        mnemonic: &str,
        source: &str,
        signature_hash: u64,
        context_changes: Vec<CompiledContextOp>,
    ) -> Option<CompiledExecutableConstructor> {
        if !runtime_signature_is_supported(signature) {
            return None;
        }
        let matcher = self
            .parse_opcode_matcher(signature)
            .unwrap_or_else(|| CompiledPatternMatcher::BitConstraints(vec![]));
        let opsize_variants = parse_opsize_variants(signature);
        let operand_specs = parse_operand_specs(signature, &matcher).ok()?;
        let hidden_subtables = parse_hidden_subtables(signature, &self.field_info);
        let mut decode_steps = Vec::new();
        if !hidden_subtables.is_empty() && operand_specs.is_empty() {
            decode_steps.extend(hidden_subtables.into_iter().map(|table_name| {
                CompiledOperandDecodeStep::DescendSubtable {
                    table_name,
                    replace_current: true,
                }
            }));
        }
        decode_steps.extend(
            (0..operand_specs.len())
                .map(|operand_index| CompiledOperandDecodeStep::DecodeOperand { operand_index }),
        );

        let constructor_template = CompiledConstructorTemplate {
            handles: operand_specs
                .iter()
                .cloned()
                .enumerate()
                .map(|(operand_index, spec)| CompiledHandleTemplate {
                    operand_index,
                    spec,
                    minimum_length: 0,
                })
                .collect(),
            decode_steps,
            num_labels: 0,
            result: None,
            ops: Vec::new(),
            template_source: CompiledTemplateSource::SpecDerived,
        };

        Some(CompiledExecutableConstructor {
            constructor_id: u32::MAX,
            sla_identity: None,
            sla_decode_status: CompiledSlaDecodeStatus::Unsupported,
            mnemonic: mnemonic.to_string(),
            source: source.to_string(),
            display: signature.to_string(),
            display_template: CompiledDisplayTemplate::from_literal_display(signature.to_string()),
            signature_hash,
            minimum_length: native_matcher_minimum_length(&matcher) as u32,
            context_changes,
            matcher,
            opsize_variants,
            operand_specs: operand_specs.clone(),
            display_operands: operand_specs
                .iter()
                .enumerate()
                .map(|(operand_index, _)| CompiledDisplayOperand {
                    operand_index,
                    kind: CompiledDisplayOperandKind::Generic,
                })
                .collect(),
            construct_tpl_kind: CompiledConstructTplKind::Generic,
            constructor_template,
            named_templates: Vec::new(),
            context_commits: Vec::new(),
            runtime_ready: false,
            unsupported_template_kind: Some(
                unsupported_template_reason(signature, &operand_specs)
                    .unwrap_or_else(|| "missing_sla_construct_tpl".to_string()),
            ),
        })
    }

    fn parse_opcode_matcher(&self, signature: &str) -> Option<CompiledPatternMatcher> {
        let (pattern_part, _context_block) = if let Some(is_pos) = signature.find(" is ") {
            let rest = &signature[is_pos + 4..];
            if let Some(bracket_pos) = rest.find('[') {
                (&rest[..bracket_pos], Some(&rest[bracket_pos..]))
            } else {
                (rest, None)
            }
        } else {
            (signature, None)
        };

        let mut constraints = Vec::new();
        for part in pattern_part.split(['&', ';', '\n']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((name, value_str)) = part.split_once('=') {
                let name = name.trim();
                let value_str = value_str.trim();
                let value = if value_str.starts_with("0x") {
                    u64::from_str_radix(&value_str[2..], 16).unwrap_or(0)
                } else if value_str.starts_with("0b") {
                    u64::from_str_radix(&value_str[2..], 2).unwrap_or(0)
                } else {
                    value_str.parse::<u64>().unwrap_or(0)
                };

                if let Some(info) = self.field_info.get(name) {
                    let field_mask = if info.bit_width >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << info.bit_width) - 1
                    };
                    let mask = field_mask.checked_shl(info.bit_offset).unwrap_or(0);
                    let shifted_value = value.checked_shl(info.bit_offset).unwrap_or(0) & mask;
                    match info.kind {
                        FieldKind::Instruction => {
                            constraints.push(PatternConstraint::Instruction {
                                offset: 0,
                                mask,
                                value: shifted_value,
                            })
                        }
                        FieldKind::Context => constraints.push(PatternConstraint::Context {
                            offset: 0,
                            mask,
                            value: shifted_value,
                        }),
                    }
                } else if name.starts_with("b_") {
                    if let Ok(bits) = name[2..].parse::<u32>() {
                        let (s, e) = if name.len() <= 4 {
                            (bits / 100, bits % 100)
                        } else {
                            (bits, bits)
                        };
                        let (start_bit, end_bit) = if s > e { (s, e) } else { (e, s) };
                        let width = start_bit - end_bit + 1;
                        let field_mask = if width >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << width) - 1
                        };
                        let mask = field_mask.checked_shl(end_bit).unwrap_or(0);
                        let shifted_value = value.checked_shl(end_bit).unwrap_or(0) & mask;
                        constraints.push(PatternConstraint::Instruction {
                            offset: 0,
                            mask,
                            value: shifted_value,
                        });
                    }
                }
            } else {
                let name = part.trim_start_matches('~').trim();
                let is_negated = part.trim().starts_with('~');
                if let Some(info) = self.field_info.get(name) {
                    if info.bit_width == 1 {
                        let mask = 1u64 << info.bit_offset;
                        let value = if is_negated { 0 } else { mask };
                        match info.kind {
                            FieldKind::Instruction => {
                                constraints.push(PatternConstraint::Instruction {
                                    offset: 0,
                                    mask,
                                    value,
                                })
                            }
                            FieldKind::Context => constraints.push(PatternConstraint::Context {
                                offset: 0,
                                mask,
                                value,
                            }),
                        }
                    }
                }
            }
        }
        if !constraints.is_empty() {
            return Some(CompiledPatternMatcher::BitConstraints(constraints));
        }
        let bytes = parse_byte_sequence(signature);
        if !bytes.is_empty() {
            return Some(CompiledPatternMatcher::ExactBytes(bytes));
        }
        if signature.contains(" is ") {
            return Some(CompiledPatternMatcher::BitConstraints(vec![]));
        }
        None
    }

    pub(super) fn parse_define_bits(&mut self, statement: &str, kind_str: &str) {
        let trimmed = strip_comments(statement).trim();
        let kind = match kind_str {
            "token" => FieldKind::Instruction,
            "context" => FieldKind::Context,
            _ => return,
        };
        let first_line_end = trimmed.find('\n').unwrap_or(trimmed.len());
        let start_pos = if trimmed[..first_line_end].contains('(') {
            if let Some(pos) = trimmed.find(')') {
                pos + 1
            } else {
                return;
            }
        } else {
            first_line_end
        };
        let fields_str = trimmed[start_pos..].trim_end_matches(';');
        for (pos, _) in fields_str.match_indices('(') {
            let left = fields_str[..pos].trim();
            let name = if let Some(last) = left.split_whitespace().last() {
                let n = last.trim_end_matches('=').trim();
                if n.is_empty() {
                    left.split_whitespace().rev().nth(1).unwrap_or("")
                } else {
                    n
                }
            } else {
                ""
            };
            if name.is_empty() || name == "endian" {
                continue;
            }
            let right = &fields_str[pos + 1..];
            if let Some(end_pos) = right.find(')') {
                let range_part = &right[..end_pos];
                if let Some((start_str, end_str)) = range_part.split_once(',') {
                    let start = start_str.trim().parse::<u32>().unwrap_or(0);
                    let end = end_str.trim().parse::<u32>().unwrap_or(0);
                    let (bit_offset, bit_width) = if start <= end {
                        (start, end - start + 1)
                    } else {
                        (end, start - end + 1)
                    };
                    self.field_info.insert(
                        name.to_string(),
                        FieldBitRange {
                            bit_offset,
                            bit_width,
                            kind,
                        },
                    );
                }
            }
        }
    }
}

fn define_bits_kind(line: &str) -> Option<&'static str> {
    let mut parts = line.split_whitespace();
    if parts.next()? != "define" {
        return None;
    }
    match parts.next()? {
        "token" => Some("token"),
        "context" => Some("context"),
        _ => None,
    }
}

fn native_matcher_minimum_length(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::RowCc { prefix, .. } => prefix.len() + 1,
        CompiledPatternMatcher::RowPage { .. } => 1,
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|constraint| match constraint {
                PatternConstraint::Instruction { offset, .. } => Some(*offset as usize + 1),
                PatternConstraint::Context { .. } => None,
            })
            .max()
            .unwrap_or(0),
    }
}

fn strip_comments(raw: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in raw.char_indices() {
        if ch == '"' {
            in_string = !in_string;
        } else if ch == '#' && !in_string {
            return &raw[..idx];
        }
    }
    raw
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
    if lower.contains("call ") {
        ControlFlowClass::Call
    } else if lower.contains("return") {
        ControlFlowClass::Return
    } else if lower.contains("cbranch") || lower.contains("if ") {
        ControlFlowClass::ConditionalBranch
    } else if lower.contains("goto ") || lower.contains("branch") {
        ControlFlowClass::Branch
    } else {
        ControlFlowClass::None
    }
}

fn constructor_semantic_ops(body: &str, defined_pcode_ops: &BTreeSet<String>) -> Vec<String> {
    defined_pcode_ops
        .iter()
        .filter(|op| body.contains(&format!("{op}(")))
        .cloned()
        .collect()
}

fn stable_hash(text: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn build_decision_tree(constructors: &[CompiledExecutableConstructor]) -> CompiledDecisionTree {
    let constructor_indexes = (0..constructors.len()).collect::<Vec<_>>();
    let mut nodes = Vec::new();
    let root_node_index = build_bucket_node(
        constructors,
        &constructor_indexes,
        &decision_probes_for_constructors(constructors),
        &mut nodes,
    );
    let decision_node_count = nodes.len();
    CompiledDecisionTree {
        root_node_index,
        root_buckets: Vec::new(),
        nodes,
        decision_node_count,
    }
}

fn decision_probes_for_constructors(
    constructors: &[CompiledExecutableConstructor],
) -> Vec<CompiledDecisionProbe> {
    let mut probes = Vec::new();
    for offset in 0..4 {
        for bit in 0..8 {
            probes.push(CompiledDecisionProbe::InstructionBitSlice {
                offset: offset as u8,
                mask: 1 << bit,
                shift: bit as u8,
            });
        }
    }
    for bit in 0..8 {
        probes.push(CompiledDecisionProbe::ContextBitSlice {
            offset: 0,
            mask: 1 << bit,
            shift: bit as u8,
        });
    }
    probes
}

fn pattern_matcher_probe_len(matcher: &CompiledPatternMatcher) -> usize {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => bytes.len(),
        CompiledPatternMatcher::BitConstraints(constraints) => constraints
            .iter()
            .filter_map(|c| {
                if let PatternConstraint::Instruction { offset, .. } = c {
                    Some(*offset as usize + 1)
                } else {
                    None
                }
            })
            .max()
            .unwrap_or(0),
        _ => 1,
    }
}

fn build_bucket_node(
    constructors: &[CompiledExecutableConstructor],
    indexes: &[usize],
    probes: &[CompiledDecisionProbe],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    if indexes.len() <= 1 || probes.is_empty() {
        return push_leaf_node(constructors, indexes, nodes);
    }
    for (pos, probe) in probes.iter().enumerate() {
        let mut groups = BTreeMap::<u8, Vec<usize>>::new();
        let mut wildcard = Vec::new();
        for &idx in indexes {
            let values = decision_feature_values(&constructors[idx], *probe);
            if values.is_empty() {
                wildcard.push(idx);
            } else {
                for v in values {
                    groups.entry(v).or_default().push(idx);
                }
            }
        }
        if groups.len() <= 1 {
            continue;
        }
        let node_index = nodes.len();
        nodes.push(CompiledDecisionNode {
            probe: *probe,
            branches: Vec::new(),
            leaf_constructor_indexes: Vec::new(),
            leaf_entries: Vec::new(),
        });
        let mut branches = Vec::new();
        for (value, mut specific) in groups {
            let mut branch_indexes = wildcard.clone();
            branch_indexes.append(&mut specific);
            branch_indexes.sort_unstable();
            branch_indexes.dedup();
            branches.push(CompiledDecisionEdge {
                value,
                next_node_index: build_bucket_node(
                    constructors,
                    &branch_indexes,
                    &probes[pos + 1..],
                    nodes,
                ),
            });
        }
        nodes[node_index].branches = branches;
        return node_index;
    }
    push_leaf_node(constructors, indexes, nodes)
}

fn push_leaf_node(
    constructors: &[CompiledExecutableConstructor],
    indexes: &[usize],
    nodes: &mut Vec<CompiledDecisionNode>,
) -> usize {
    let mut sorted = indexes.to_vec();
    sorted.sort_by_key(|&idx| std::cmp::Reverse(decision_specificity(&constructors[idx])));
    let node_index = nodes.len();
    nodes.push(CompiledDecisionNode {
        probe: CompiledDecisionProbe::Terminal,
        branches: Vec::new(),
        leaf_constructor_indexes: sorted,
        leaf_entries: Vec::new(),
    });
    node_index
}

fn decision_feature_values(
    ctor: &CompiledExecutableConstructor,
    probe: CompiledDecisionProbe,
) -> Vec<u8> {
    match probe {
        CompiledDecisionProbe::InstructionBitSlice {
            offset,
            mask,
            shift,
        } => instruction_probe_values(&ctor.matcher, offset as usize)
            .into_iter()
            .map(|v| (v & mask) >> shift)
            .collect(),
        CompiledDecisionProbe::ContextBitSlice {
            offset,
            mask,
            shift,
        } => context_probe_values(&ctor.matcher, offset as usize)
            .into_iter()
            .map(|v| ((v & u64::from(mask)) >> shift) as u8)
            .collect(),
        CompiledDecisionProbe::SlaInstructionBits { .. }
        | CompiledDecisionProbe::SlaContextBits { .. } => Vec::new(),
        _ => Vec::new(),
    }
}

fn instruction_probe_values(matcher: &CompiledPatternMatcher, offset: usize) -> Vec<u8> {
    match matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => {
            bytes.get(offset).copied().into_iter().collect()
        }
        CompiledPatternMatcher::BitConstraints(constraints) => {
            let mut val = 0u8;
            let mut found = false;
            for c in constraints {
                if let PatternConstraint::Instruction {
                    offset: c_off,
                    mask,
                    value,
                } = c
                {
                    if offset >= *c_off as usize && offset < *c_off as usize + 8 {
                        let shift = (offset - *c_off as usize) * 8;
                        if (mask >> shift) & 0xff != 0 {
                            val |= ((value >> shift) & 0xff) as u8;
                            found = true;
                        }
                    }
                }
            }
            if found {
                vec![val]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn context_probe_values(matcher: &CompiledPatternMatcher, offset: usize) -> Vec<u64> {
    if let CompiledPatternMatcher::BitConstraints(constraints) = matcher {
        constraints
            .iter()
            .filter_map(|c| {
                if let PatternConstraint::Context {
                    offset: c_off,
                    value,
                    ..
                } = c
                {
                    if offset == *c_off as usize {
                        Some(*value)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    } else {
        Vec::new()
    }
}

fn decision_specificity(constructor: &CompiledExecutableConstructor) -> usize {
    let mut score = 0usize;
    if constructor.mnemonic.starts_with('^') {
        score = score.saturating_sub(500);
    }
    if let CompiledPatternMatcher::BitConstraints(ref constraints) = constructor.matcher {
        if !constraints.is_empty() {
            score += 1000;
        }
    }
    score += constructor.opsize_variants.len().min(1) * 2;
    match &constructor.matcher {
        CompiledPatternMatcher::ExactBytes(bytes) => score += bytes.len() * 80,
        CompiledPatternMatcher::RowCc { prefix, .. } => score += prefix.len() * 80 + 40,
        CompiledPatternMatcher::RowPage { .. } => score += 50,
        CompiledPatternMatcher::BitConstraints(constraints) => {
            for constraint in constraints {
                match constraint {
                    PatternConstraint::Instruction { mask, .. } => {
                        score += (mask.count_ones() as usize) * 10;
                    }
                    PatternConstraint::Context { mask, .. } => {
                        score += (mask.count_ones() as usize) * 10;
                    }
                }
            }
        }
    }
    score += constructor
        .operand_specs
        .iter()
        .filter(|spec| {
            matches!(
                spec,
                CompiledOperandSpec::SlaTokenField { .. }
                    | CompiledOperandSpec::ContextFieldExtraction { .. }
                    | CompiledOperandSpec::SubtableEvaluation { .. }
            )
        })
        .count()
        * 20;
    score
}

fn runtime_signature_is_supported(_signature: &str) -> bool {
    true
}

fn parse_operand_specs(
    signature: &str,
    _matcher: &CompiledPatternMatcher,
) -> Result<Vec<CompiledOperandSpec>> {
    let first_line = signature.lines().next().unwrap_or(signature);
    let head = if let Some(pos) = first_line.find(" is ") {
        &first_line[..pos]
    } else if let Some(pos) = first_line.find("is ") {
        &first_line[..pos]
    } else {
        first_line
    };
    let head = head.trim().trim_start_matches(':');
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
        if let Some(size) = fixed_accumulator_size(token) {
            specs.push(CompiledOperandSpec::FixedRegister {
                reg: CompiledFixedRegister::Accumulator,
                size,
            });
            continue;
        }
        let token = token.trim();
        if !token.is_empty()
            && token.len() <= 64
            && token.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            specs.push(CompiledOperandSpec::SubtableEvaluation {
                table_name: token.to_string(),
                reloffset: 0,
                offsetbase: -1,
            });
        } else {
            specs.push(CompiledOperandSpec::Immediate {
                size: 0,
                signed: false,
            });
        }
    }
    if specs.is_empty() && !operand_part.is_empty() {
        return Ok(vec![CompiledOperandSpec::SubtableEvaluation {
            table_name: "unknown".to_string(),
            reloffset: 0,
            offsetbase: -1,
        }]);
    }
    if specs.is_empty() && operand_part.is_empty() {
        return Ok(Vec::new());
    }
    Ok(specs)
}

fn parse_hidden_subtables(
    signature: &str,
    field_info: &BTreeMap<String, FieldBitRange>,
) -> Vec<String> {
    let Some(is_pos) = signature.find(" is ") else {
        return Vec::new();
    };
    let rest = &signature[is_pos + 4..];
    let pattern_part = rest.split(['[', '{']).next().unwrap_or(rest);
    let mut subtables = Vec::new();
    for raw_token in pattern_part.split('&') {
        let token = raw_token
            .trim()
            .trim_matches(|ch| ch == '(' || ch == ')' || ch == '^');
        if token.is_empty()
            || token.contains('=')
            || token.chars().any(|ch| ch.is_ascii_whitespace())
            || !token.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            || field_info.contains_key(token)
        {
            continue;
        }
        if !subtables.iter().any(|existing| existing == token) {
            subtables.push(token.to_string());
        }
    }
    subtables
}

fn parse_context_changes(
    signature: &str,
    field_info: &BTreeMap<String, FieldBitRange>,
) -> Vec<CompiledContextOp> {
    let mut ops = Vec::new();
    let Some(start) = signature.find('[') else {
        return ops;
    };
    let Some(end_rel) = signature[start + 1..].find(']') else {
        return ops;
    };
    let block = &signature[start + 1..start + 1 + end_rel];
    for stmt in block.split(';') {
        let stmt = stmt.trim();
        let Some((lhs, rhs)) = stmt.split_once('=') else {
            continue;
        };
        let name = lhs.trim();
        let rhs = rhs.trim();
        let Some(info) = field_info.get(name) else {
            continue;
        };
        if !matches!(info.kind, FieldKind::Context) {
            continue;
        }
        let Some(value) = parse_context_literal(rhs) else {
            continue;
        };
        ops.push(CompiledContextOp {
            bit_offset: info.bit_offset,
            bit_width: info.bit_width,
            value,
            word_index: 0,
            mask: if info.bit_width >= 64 {
                u64::MAX
            } else {
                ((1u64 << info.bit_width) - 1)
                    .checked_shl(info.bit_offset)
                    .unwrap_or(0)
            },
            shift: info.bit_offset as i32,
            expr: None,
        });
    }
    ops
}

fn parse_context_literal(text: &str) -> Option<u64> {
    let trimmed = text.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).ok()
    } else if trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        trimmed.parse::<u64>().ok()
    } else {
        None
    }
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
    if operand_specs.len() > 2 {
        Some("unsupported_operand_arity".to_string())
    } else {
        None
    }
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
        match token {
            "AL" => Some(1),
            "AX" => Some(2),
            "EAX" => Some(4),
            "RAX" => Some(8),
            "FS" | "GS" | "CS" | "SS" | "DS" | "ES" => Some(2),
            _ => None,
        }
    } else {
        digits.parse::<u32>().ok().map(|bits| (bits / 8).max(1))
    }
}
