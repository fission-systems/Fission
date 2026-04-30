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
                cursor_policy_bits: 0,
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
                cursor_policy_bits: 0,
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
        sla_default_space_index: u64::MAX,
        uses_shared_token_layout: false,
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
    compiled.sla_default_space_index = library.default_space_index;

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
                cursor_policy_bits: 0,
            },
        );
    }

    decode_metadata::apply_post_sla_decode_metadata(compiled);

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

fn constructors_by_sla_id(
    constructors: &[CompiledExecutableConstructor],
) -> BTreeMap<u32, usize> {
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
        mod_constraint: None,
        operand_reg_values: Vec::new(),
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
                .map(|(operand_index, spec)| CompiledHandleTemplate {
                    operand_index,
                    spec,
                    sla_operand_symbol_meta: sla_template
                        .operand_symbol_meta
                        .get(operand_index)
                        .cloned()
                        .unwrap_or_default(),
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
        return Some("sla_constructor_decode_failed".to_string());
    }
    for spec in &sla_template.operand_specs {
        match spec {
            CompiledOperandSpec::TokenFieldExtraction { .. } => {
                return Some("legacy_token_field_extraction_operand".to_string());
            }
            CompiledOperandSpec::Immediate { .. }
            | CompiledOperandSpec::Relative { .. }
            | CompiledOperandSpec::FixedRegister { .. }
            | CompiledOperandSpec::ContextFieldExtraction { .. } => {
                return Some("legacy_operand_spec_not_sla_native".to_string());
            }
            CompiledOperandSpec::SlaTokenField { .. }
            | CompiledOperandSpec::SlaVarnodeList { .. }
            | CompiledOperandSpec::SlaValueMap { .. }
            | CompiledOperandSpec::SlaFixedVarnode { .. }
            | CompiledOperandSpec::SlaPatternExpression { .. }
            | CompiledOperandSpec::SubtableEvaluation { .. } => {}
        }
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

struct FieldBitRange {
    bit_offset: u32,
    bit_width: u32,
    kind: FieldKind,
}

struct Collector {
    definitions: Vec<CompiledSpecDefinition>,
    macros: Vec<CompiledMacro>,
    constructors: Vec<CompiledConstructor>,
    subtable_executables: BTreeMap<String, Vec<CompiledExecutableConstructor>>,
    pcode_ops: BTreeSet<String>,
    pcode_op_sources: BTreeMap<String, String>,
    default_context: u64,
    pattern_nodes: Vec<CompiledPatternNode>,
    field_info: BTreeMap<String, FieldBitRange>,
}
