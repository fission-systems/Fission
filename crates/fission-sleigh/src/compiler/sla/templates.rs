use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Result};
use fission_pcode::PcodeOpcode;

use super::*;
use crate::compiler::ir::{
    CompiledConstTpl, CompiledConstructTpl, CompiledContextCommit, CompiledContextOp,
    CompiledDecisionLeafEntry, CompiledDisjointPattern, CompiledDisplayOperand,
    CompiledDisplayPiece, CompiledDisplayTemplate, CompiledHandleSelector, CompiledHandleTpl,
    CompiledLabelRef, CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandSpec,
    CompiledPatternBlock, CompiledPatternExpression, CompiledResolvedVarnode,
    CompiledSlaDecodeStatus, CompiledSpaceRef, CompiledSpaceTpl, CompiledVarnodeTpl,
};

pub(super) fn decode_construct_templates(
    artifact: &CompiledSlaArtifact,
) -> Result<CompiledSlaTemplateLibrary> {
    if artifact.version != GHIDRA_SLA_FORMAT_VERSION {
        bail!(
            "unsupported SLEIGH format version {} in {}",
            artifact.version,
            artifact.path.display()
        );
    }
    let mut parser = PackedParser::new(&artifact.payload);
    let root = parser.parse_root()?;
    if root.id != sla_format::ELEM_SLEIGH {
        bail!(
            "compiled SLEIGH root element was {}, expected sleigh",
            root.id
        );
    }

    let source_files = decode_source_files(&root)?;
    let space_result = decode_spaces(&root)?;
    let spaces = space_result.spaces;
    let unique_space_index = space_result.unique_space_index;
    let register_space_index = space_result.register_space_index;
    let uniqbase = root
        .attr_unsigned(sla_format::ATTR_UNIQBASE)
        .ok_or_else(|| anyhow!("compiled SLEIGH root missing uniqbase"))?;
    let uniqmask = root
        .attr_unsigned(sla_format::ATTR_UNIQMASK)
        .unwrap_or(u64::MAX);

    // 1. Pass One: Build a complete symbol ID -> name mapping from the symbol table
    let mut symbol_names = BTreeMap::new();
    if let Some(sym_tab) = root
        .descendants_with_id(sla_format::ELEM_SYMBOL_TABLE)
        .into_iter()
        .next()
    {
        for head in &sym_tab.children {
            if let Some(id) = head.attr_unsigned(sla_format::ATTR_ID) {
                if let Some(name) = head.attr_string(sla_format::ATTR_NAME) {
                    symbol_names.insert(id as u32, name.to_string());
                }
            }
        }
    }

    eprintln!("SLA Symbols Found: {} names", symbol_names.len());

    let mut constructors_by_source: BTreeMap<String, Vec<CompiledSlaConstructorTemplate>> =
        BTreeMap::new();
    let mut subtables = BTreeMap::new();
    let mut subtable_names_by_id = BTreeMap::new();
    for subtable_sym in root.descendants_with_id(sla_format::ELEM_SUBTABLE_SYM) {
        let (id, name) = decode_subtable_identity(subtable_sym, &symbol_names)?;
        subtable_names_by_id.insert(id, name);
    }

    let display_symbols =
        decode_display_symbols(&root, &spaces, &symbol_names, &subtable_names_by_id)?;
    let operand_symbols = decode_operand_symbols(&root, &display_symbols)?;

    // Helper to parse a constructor
    let trace_sla_parse = std::env::var_os("FISSION_TRACE_SLA_PARSE").is_some();
    let mut parse_constructor =
        |subtable_id: u32,
         subtable_name: &str,
         constructor: &PackedElement,
         local_index: usize|
         -> std::result::Result<CompiledSlaConstructorTemplate, String> {
            // Ghidra SubtableSymbol.decode() assigns constructor ids by local
            // ordinal within the subtable, then DecisionNode pair ATTR_ID resolves
            // through sub.getConstructor(id). The constructor element's own ATTR_ID
            // is not the terminal selection index.
            let id = local_index as u32;
            let parent_id = constructor
                .attr_unsigned(sla_format::ATTR_PARENT)
                .ok_or_else(|| "constructor_missing_parent".to_string())?;
            let parent_id = u32::try_from(parent_id)
                .map_err(|_| "constructor_parent_out_of_range".to_string())?;
            if parent_id != subtable_id {
                return Err("constructor_parent_mismatch".to_string());
            }
            let first_whitespace = constructor
                .attr_signed(sla_format::ATTR_FIRST)
                .ok_or_else(|| "constructor_missing_first_whitespace".to_string())
                .and_then(|value| {
                    if value < 0 {
                        Ok(None)
                    } else {
                        usize::try_from(value)
                            .map(Some)
                            .map_err(|_| "constructor_first_whitespace_out_of_range".to_string())
                    }
                })?;
            let source_index = constructor
                .attr_signed(sla_format::ATTR_SOURCE)
                .ok_or_else(|| "constructor_missing_source_index".to_string())?;
            let line = constructor
                .attr_signed(sla_format::ATTR_LINE)
                .ok_or_else(|| "constructor_missing_line".to_string())
                .and_then(|value| {
                    u64::try_from(value).map_err(|_| "constructor_negative_line".to_string())
                })?;
            let minimum_length = constructor
                .attr_signed(sla_format::ATTR_LENGTH)
                .ok_or_else(|| "constructor_missing_minimum_length".to_string())
                .and_then(|value| {
                    u32::try_from(value)
                        .map_err(|_| "constructor_negative_minimum_length".to_string())
                })?;
            let source_index_key = u64::try_from(source_index).ok();
            let source_file = source_index_key
                .and_then(|idx| source_files.get(&idx).cloned())
                .unwrap_or_else(|| format!("<generated:{subtable_name}>"));
            let source_key = if source_index_key.is_some() {
                format!("{}:{line}", basename(&source_file))
            } else {
                format!("{subtable_name}#ctor{local_index}")
            };

            let main_tpl = constructor.children.iter().find(|child| {
                child.id == sla_format::ELEM_CONSTRUCT_TPL
                    && child.attr_unsigned(sla_format::ATTR_SECTION).is_none()
            });
            let Some(main_tpl) = main_tpl else {
                if trace_sla_parse {
                    eprintln!(
                    "[sla-parse] missing construct_tpl subtable={subtable_name} slot={local_index} attrs={:?}",
                    constructor.attrs
                );
                }
                return Err("missing_construct_tpl".to_string());
            };

            let template = match decode_construct_tpl(main_tpl, &spaces) {
                Ok(template) => template,
                Err(err) => {
                    if trace_sla_parse {
                        eprintln!(
                        "[sla-parse] decode_construct_tpl failed subtable={subtable_name} slot={local_index} source_key={source_key} err={err:#}"
                    );
                    }
                    return Err(format!("decode_construct_tpl:{err:#}"));
                }
            };

            // Collect named p-code sections (Ghidra's namedtempl: ELEM_CONSTRUCT_TPL with
            // ATTR_SECTION >= 0). These are referenced by CROSSBUILD and sectioned constructors.
            let mut named_templates: Vec<Option<CompiledConstructTpl>> = Vec::new();
            for child in &constructor.children {
                if child.id != sla_format::ELEM_CONSTRUCT_TPL {
                    continue;
                }
                let Some(section_idx) = child.attr_unsigned(sla_format::ATTR_SECTION) else {
                    continue; // main template, already handled
                };
                let section_idx = section_idx as usize;
                // Extend vector to fit this section index.
                while named_templates.len() <= section_idx {
                    named_templates.push(None);
                }
                match decode_construct_tpl(child, &spaces) {
                    Ok(named_tpl) => named_templates[section_idx] = Some(named_tpl),
                    Err(err) => {
                        if trace_sla_parse {
                            eprintln!(
                            "[sla-parse] decode named section {section_idx} failed subtable={subtable_name} slot={local_index} err={err:#}"
                        );
                        }
                        return Err(format!("decode_named_construct_tpl:{section_idx}:{err:#}"));
                    }
                }
            }

            let mut opprint_indices = Vec::new();
            let mut display_pieces = Vec::new();
            let mut operand_specs_by_index = BTreeMap::new();
            let mut operand_minimum_lengths_by_index = BTreeMap::new();
            let mut display_operands_by_index = BTreeMap::new();
            let mut flowthru_operand_index = None;
            for child in &constructor.children {
                match child.id {
                    sla_format::ELEM_OPER => {
                        let Some(symbol_id) =
                            child.attr_unsigned(sla_format::ATTR_ID).map(|id| id as u32)
                        else {
                            if trace_sla_parse {
                                eprintln!(
                                "[sla-parse] oper missing symbol id subtable={subtable_name} slot={local_index} source_key={source_key}"
                            );
                            }
                            return Err("oper_missing_symbol_id".to_string());
                        };
                        let Some(operand_symbol) = operand_symbols.get(&symbol_id) else {
                            if trace_sla_parse {
                                eprintln!(
                                "[sla-parse] missing operand symbol subtable={subtable_name} slot={local_index} source_key={source_key} symbol_id={symbol_id}"
                            );
                            }
                            return Err("missing_operand_symbol".to_string());
                        };
                        let Some(spec) =
                            compiled_operand_spec_for_symbol(operand_symbol, &subtable_names_by_id)
                        else {
                            if trace_sla_parse {
                                eprintln!(
                                "[sla-parse] unsupported operand symbol subtable={subtable_name} slot={local_index} source_key={source_key} symbol_id={symbol_id} symbol={operand_symbol:?}"
                            );
                            }
                            return Err("unsupported_operand_symbol".to_string());
                        };
                        operand_specs_by_index.insert(operand_symbol.hand_index, spec);
                        operand_minimum_lengths_by_index
                            .insert(operand_symbol.hand_index, operand_symbol.minimum_length);
                        display_operands_by_index.insert(
                            operand_symbol.hand_index,
                            CompiledDisplayOperand {
                                operand_index: operand_symbol.hand_index,
                                kind: operand_symbol.display_kind.clone(),
                            },
                        );
                    }
                    sla_format::ELEM_OPPRINT => {
                        let index = child
                            .attr_signed(sla_format::ATTR_ID)
                            .ok_or_else(|| "opprint_missing_operand_index".to_string())
                            .and_then(|value| {
                                usize::try_from(value)
                                    .map_err(|_| "opprint_negative_operand_index".to_string())
                            })?;
                        opprint_indices.push(index);
                        display_pieces.push(CompiledDisplayPiece::OperandRef(index));
                    }
                    sla_format::ELEM_PRINT => {
                        let piece = child
                            .attr_string(sla_format::ATTR_PIECE)
                            .ok_or_else(|| "print_missing_piece".to_string())?;
                        display_pieces.push(CompiledDisplayPiece::Literal(piece.to_string()));
                    }
                    _ => {}
                }
            }
            let operand_count = operand_specs_by_index
                .keys()
                .next_back()
                .map(|value| value + 1)
                .unwrap_or(0);
            let mut operand_specs = Vec::with_capacity(operand_count);
            let mut operand_minimum_lengths = Vec::with_capacity(operand_count);
            let mut display_operands = Vec::with_capacity(operand_count);
            for slot in 0..operand_count {
                let Some(spec) = operand_specs_by_index.remove(&slot) else {
                    if trace_sla_parse {
                        eprintln!(
                        "[sla-parse] missing operand spec subtable={subtable_name} slot={local_index} operand={slot} source_key={source_key}"
                    );
                    }
                    return Err("missing_operand_spec".to_string());
                };
                operand_specs.push(spec);
                let Some(minimum_length) = operand_minimum_lengths_by_index.remove(&slot) else {
                    return Err("missing_operand_minimum_length".to_string());
                };
                operand_minimum_lengths.push(minimum_length);
                let Some(display_operand) = display_operands_by_index.remove(&slot) else {
                    return Err("missing_display_operand".to_string());
                };
                display_operands.push(display_operand);
            }

            let has_print_literals = display_pieces
                .iter()
                .any(|piece| matches!(piece, CompiledDisplayPiece::Literal(_)));
            if !has_print_literals && display_pieces.len() == 1 {
                if let Some(CompiledDisplayPiece::OperandRef(index)) = display_pieces.first() {
                    flowthru_operand_index = Some(*index);
                }
            }
            let display_text = display_pieces
                .iter()
                .map(|piece| match piece {
                    CompiledDisplayPiece::Literal(lit) => lit.clone(),
                    CompiledDisplayPiece::OperandRef(index) => {
                        format!("\\n{}", operand_piece_label(*index))
                    }
                })
                .collect::<String>();

            let mut context_changes = Vec::new();
            for child in constructor
                .children
                .iter()
                .filter(|child| child.id == sla_format::ELEM_CONTEXT_OP)
            {
                match decode_context_op(child) {
                    Ok(change) => context_changes.push(change),
                    Err(err) => {
                        if trace_sla_parse {
                            eprintln!(
                            "[sla-parse] decode_context_op failed subtable={subtable_name} slot={local_index} source_key={source_key} err={err:#}"
                        );
                        }
                        return Err(format!("decode_context_op:{err:#}"));
                    }
                }
            }

            // Ghidra: ContextCommit elements encode deferred global context changes.
            // Each ELEM_COMMIT child carries: symbol_id (ATTR_ID), word_index (ATTR_NUMBER),
            // and mask (ATTR_MASK). See ContextCommit.encode() in Ghidra.
            let mut context_commits = Vec::new();
            for child in constructor
                .children
                .iter()
                .filter(|child| child.id == sla_format::ELEM_COMMIT)
            {
                let symbol_id = child
                    .attr_unsigned(sla_format::ATTR_ID)
                    .ok_or_else(|| "context_commit_missing_symbol_id".to_string())?
                    as u32;
                let word_index = child
                    .attr_unsigned(sla_format::ATTR_NUMBER)
                    .ok_or_else(|| "context_commit_missing_word_index".to_string())?
                    as u32;
                let mask = child
                    .attr_unsigned(sla_format::ATTR_MASK)
                    .ok_or_else(|| "context_commit_missing_mask".to_string())?
                    as u32;
                // Resolve symbol_id → hand_index: look up in the operand symbol table.
                // If the symbol is a built-in (e.g. `inst_next`), store u32::MAX as sentinel.
                let hand_index = operand_symbols
                    .get(&symbol_id)
                    .map(|sym| sym.hand_index as u32)
                    .unwrap_or(u32::MAX);
                context_commits.push(CompiledContextCommit {
                    symbol_id,
                    hand_index,
                    word_index,
                    mask,
                });
            }

            Ok(CompiledSlaConstructorTemplate {
                id,
                subtable_id,
                subtable_name: subtable_name.to_string(),
                constructor_slot: local_index,
                decode_status: CompiledSlaDecodeStatus::Decoded,
                decode_error: None,
                source_key,
                source_file,
                line,
                minimum_length,
                display_template: CompiledDisplayTemplate {
                    constructor_hash: 0,
                    pieces: display_pieces,
                    first_whitespace,
                    flowthru_operand_index,
                    display: display_text,
                },
                display_operands,
                opprint_indices,
                operand_specs,
                operand_minimum_lengths,
                context_changes,
                context_commits,
                flowthru_operand_index,
                constructor_template: template,
                named_templates,
            })
        };

    // 2. Pass Two: Process subtable symbols and their content
    for subtable_sym in root.descendants_with_id(sla_format::ELEM_SUBTABLE_SYM) {
        let (id, name) = decode_subtable_identity(subtable_sym, &symbol_names)?;

        let mut constructors_by_index = BTreeMap::new();
        let mut decision_tree = None;

        for (local_index, child) in subtable_sym
            .children
            .iter()
            .filter(|child| child.id == sla_format::ELEM_CONSTRUCTOR)
            .enumerate()
        {
            // Decision leaf pairs reference Ghidra's packed constructor id,
            // not the ordinal after Fission's iteration/filtering. Preserve
            // that slot identity so terminal verification lands on the same
            // constructor that the .sla decision tree selected.
            let slot = child
                .attr_unsigned(sla_format::ATTR_ID)
                .map(|value| value as usize)
                .unwrap_or(local_index);
            let template = match parse_constructor(id, &name, child, slot) {
                Ok(template) => template,
                Err(reason) if reason == "missing_construct_tpl" => {
                    unsupported_sla_constructor_template(id, &name, slot, reason)
                }
                Err(reason) => return Err(anyhow!(reason)),
            };
            constructors_by_index.insert(slot, template);
        }

        for child in &subtable_sym.children {
            if child.id == sla_format::ELEM_DECISION {
                decision_tree = Some(decode_decision_tree(id, child)?);
            }
        }

        let constructor_count = constructors_by_index
            .keys()
            .next_back()
            .map(|value| value + 1)
            .unwrap_or(0);
        let mut subtable_constructors = Vec::with_capacity(constructor_count);
        for slot in 0..constructor_count {
            subtable_constructors.push(constructors_by_index.remove(&slot).unwrap_or_else(|| {
                unsupported_sla_constructor_template(
                    id,
                    &name,
                    slot,
                    "missing_constructor_slot".to_string(),
                )
            }));
        }

        for tpl in &subtable_constructors {
            constructors_by_source
                .entry(tpl.source_key.clone())
                .or_default()
                .push(tpl.clone());
        }

        subtables.insert(
            name.clone(),
            CompiledSlaSubtable {
                id,
                name,
                constructors: subtable_constructors,
                decision_tree,
            },
        );
    }

    if let Some(inst_table) = subtables.get_mut("instruction") {
        if let Some(tree) = &inst_table.decision_tree {
            if !tree.nodes.is_empty() {
                let root_node = &tree.nodes[tree.root_node_index];
                eprintln!(
                    "'instruction' Root Node: probe={:?}, branches={}",
                    root_node.probe,
                    root_node.branches.len()
                );
            }
        }
    }

    let mut library = CompiledSlaTemplateLibrary {
        path: artifact.path.clone(),
        version: artifact.version,
        source_files,
        spaces,
        unique_space_index,
        register_space_index,
        uniqbase,
        uniqmask,
        constructors_by_source,
        subtables,
        native: SlaLanguage {
            path: artifact.path.clone(),
            version: artifact.version,
            source_files: BTreeMap::new(),
            spaces: BTreeMap::new(),
            unique_space_index: u64::MAX,
            register_space_index: u64::MAX,
            uniqbase: 0,
            subtables: BTreeMap::new(),
        },
    };
    library.native = SlaLanguage::from_compiled_library(&library);
    Ok(library)
}

fn decode_subtable_identity(
    element: &PackedElement,
    symbol_names: &BTreeMap<u32, String>,
) -> Result<(u32, String)> {
    let id = element
        .attr_unsigned(sla_format::ATTR_ID)
        .ok_or_else(|| anyhow!("subtable_sym missing id"))? as u32;
    let name = element
        .attr_string(sla_format::ATTR_NAME)
        .map(|s| s.to_string())
        .or_else(|| symbol_names.get(&id).cloned())
        .ok_or_else(|| anyhow!("subtable_sym {id} missing name"))?;
    Ok((id, name))
}

fn unsupported_sla_constructor_template(
    subtable_id: u32,
    subtable_name: &str,
    slot: usize,
    decode_error: String,
) -> CompiledSlaConstructorTemplate {
    CompiledSlaConstructorTemplate {
        id: slot as u32,
        subtable_id,
        subtable_name: subtable_name.to_string(),
        constructor_slot: slot,
        decode_status: CompiledSlaDecodeStatus::Unsupported,
        decode_error: Some(decode_error),
        source_key: format!("sla_decode_failed_constructor:{subtable_name}:{slot}"),
        source_file: "unknown".to_string(),
        line: 0,
        minimum_length: 0,
        display_template: CompiledDisplayTemplate::empty(),
        display_operands: Vec::new(),
        opprint_indices: Vec::new(),
        operand_specs: Vec::new(),
        operand_minimum_lengths: Vec::new(),
        context_changes: Vec::new(),
        context_commits: Vec::new(),
        flowthru_operand_index: None,
        constructor_template: CompiledConstructTpl {
            constructor_hash: 0,
            num_labels: 0,
            result: None,
            ops: Vec::new(),
        },
        named_templates: Vec::new(),
    }
}

pub fn decode_decision_tree(
    subtable_id: u32,
    element: &PackedElement,
) -> Result<crate::compiler::ir::CompiledDecisionTree> {
    let mut nodes = Vec::new();
    let root_idx = decode_decision_node(subtable_id, element, &mut nodes)?;
    let decision_node_count = nodes.len();
    Ok(crate::compiler::ir::CompiledDecisionTree {
        root_node_index: root_idx,
        nodes,
        decision_node_count,
        root_buckets: vec![crate::compiler::ir::CompiledDecisionBucket {
            key: "global".to_string(),
            node_index: root_idx,
        }],
    })
}

fn decode_decision_node(
    subtable_id: u32,
    element: &PackedElement,
    nodes: &mut Vec<crate::compiler::ir::CompiledDecisionNode>,
) -> Result<usize> {
    let node_idx = nodes.len();
    nodes.push(crate::compiler::ir::CompiledDecisionNode {
        probe: crate::compiler::ir::CompiledDecisionProbe::Terminal,
        branches: Vec::new(),
        leaf_constructor_indexes: Vec::new(),
        leaf_entries: Vec::new(),
    });

    let is_context = element
        .attr_bool_value(sla_format::ATTR_CONTEXT)
        .ok_or_else(|| anyhow!("decision node missing context flag"))?;
    let start_bit = element
        .attr_signed(sla_format::ATTR_STARTBIT)
        .ok_or_else(|| anyhow!("decision node missing start bit"))
        .and_then(|value| {
            u32::try_from(value).map_err(|_| anyhow!("decision node has negative start bit"))
        })?;
    let bit_size = element
        .attr_signed(sla_format::ATTR_SIZE)
        .ok_or_else(|| anyhow!("decision node missing bit size"))
        .and_then(|value| {
            u32::try_from(value).map_err(|_| anyhow!("decision node has negative bit size"))
        })?;

    if bit_size > 0 {
        let probe = if is_context {
            crate::compiler::ir::CompiledDecisionProbe::SlaContextBits {
                start_bit,
                bit_size,
            }
        } else {
            crate::compiler::ir::CompiledDecisionProbe::SlaInstructionBits {
                start_bit,
                bit_size,
            }
        };
        nodes[node_idx].probe = probe;

        let mut val = 0u32;
        for child in &element.children {
            if child.id == sla_format::ELEM_DECISION {
                let child_idx = decode_decision_node(subtable_id, child, nodes)?;
                nodes[node_idx]
                    .branches
                    .push(crate::compiler::ir::CompiledDecisionEdge {
                        value: val as u8,
                        next_node_index: child_idx,
                    });
                val += 1;
            }
        }
    } else {
        nodes[node_idx].probe = crate::compiler::ir::CompiledDecisionProbe::Terminal;
        for child in &element.children {
            if child.id == sla_format::ELEM_PAIR {
                let constructor_id = child
                    .attr_signed(sla_format::ATTR_ID)
                    .ok_or_else(|| anyhow!("decision pair missing constructor id"))
                    .and_then(|value| {
                        u32::try_from(value)
                            .map_err(|_| anyhow!("decision pair has negative constructor id"))
                    })?;
                nodes[node_idx]
                    .leaf_constructor_indexes
                    .push(constructor_id as usize);
                let pattern = decode_decision_pair_pattern(child)?;
                nodes[node_idx]
                    .leaf_entries
                    .push(CompiledDecisionLeafEntry {
                        subtable_id,
                        constructor_id: constructor_id as u32,
                        constructor_index: constructor_id as usize,
                        pattern,
                    });
            }
        }
    }

    Ok(node_idx)
}

fn decode_decision_pair_pattern(element: &PackedElement) -> Result<CompiledDisjointPattern> {
    element
        .children
        .iter()
        .find(|child| {
            matches!(
                child.id,
                sla_format::ELEM_INSTRUCT_PAT
                    | sla_format::ELEM_CONTEXT_PAT
                    | sla_format::ELEM_COMBINE_PAT
                    | sla_format::ELEM_OR_PAT
            )
        })
        .ok_or_else(|| anyhow!("decision pair missing disjoint pattern"))
        .and_then(decode_disjoint_pattern)
}

fn decode_disjoint_pattern(element: &PackedElement) -> Result<CompiledDisjointPattern> {
    match element.id {
        sla_format::ELEM_INSTRUCT_PAT => {
            Ok(CompiledDisjointPattern::Instruction(decode_pattern_block(
                element
                    .children
                    .iter()
                    .find(|child| child.id == sla_format::ELEM_PAT_BLOCK)
                    .ok_or_else(|| anyhow!("instruction pattern missing pat_block"))?,
            )?))
        }
        sla_format::ELEM_CONTEXT_PAT => Ok(CompiledDisjointPattern::Context(decode_pattern_block(
            element
                .children
                .iter()
                .find(|child| child.id == sla_format::ELEM_PAT_BLOCK)
                .ok_or_else(|| anyhow!("context pattern missing pat_block"))?,
        )?)),
        sla_format::ELEM_COMBINE_PAT => {
            let context = element
                .children
                .iter()
                .find(|child| child.id == sla_format::ELEM_CONTEXT_PAT)
                .ok_or_else(|| anyhow!("combine pattern missing context_pat"))?;
            let instruction = element
                .children
                .iter()
                .find(|child| child.id == sla_format::ELEM_INSTRUCT_PAT)
                .ok_or_else(|| anyhow!("combine pattern missing instruct_pat"))?;
            let CompiledDisjointPattern::Context(context) = decode_disjoint_pattern(context)?
            else {
                bail!("combine pattern context child decoded to unexpected kind");
            };
            let CompiledDisjointPattern::Instruction(instruction) =
                decode_disjoint_pattern(instruction)?
            else {
                bail!("combine pattern instruction child decoded to unexpected kind");
            };
            Ok(CompiledDisjointPattern::Combine {
                context,
                instruction,
            })
        }
        sla_format::ELEM_OR_PAT => {
            let mut patterns = Vec::new();
            for child in &element.children {
                patterns.push(decode_disjoint_pattern(child)?);
            }
            if patterns.is_empty() {
                bail!("or pattern has no alternatives");
            }
            Ok(CompiledDisjointPattern::Or(patterns))
        }
        _ => bail!("unsupported decision leaf pattern element {}", element.id),
    }
}

fn decode_pattern_block(element: &PackedElement) -> Result<CompiledPatternBlock> {
    if element.id != sla_format::ELEM_PAT_BLOCK {
        bail!("expected pat_block element, got {}", element.id);
    }
    let offset = element
        .attr_signed(sla_format::ATTR_OFF)
        .ok_or_else(|| anyhow!("pat_block missing offset"))? as i32;
    let nonzero_size = element
        .attr_signed(sla_format::ATTR_NONZERO)
        .ok_or_else(|| anyhow!("pat_block missing nonzero size"))? as i32;
    let mut mask_words = Vec::new();
    let mut value_words = Vec::new();
    for child in &element.children {
        if child.id != sla_format::ELEM_MASK_WORD {
            continue;
        }
        mask_words.push(
            child
                .attr_unsigned(sla_format::ATTR_MASK)
                .ok_or_else(|| anyhow!("mask_word missing mask"))? as u32,
        );
        value_words.push(
            child
                .attr_unsigned(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("mask_word missing value"))? as u32,
        );
    }
    Ok(CompiledPatternBlock {
        offset,
        nonzero_size,
        mask_words,
        value_words,
    })
}

fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn decode_construct_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledConstructTpl> {
    let num_labels = match element.attr_signed(sla_format::ATTR_LABELS) {
        Some(value) => {
            u32::try_from(value).map_err(|_| anyhow!("construct_tpl has negative label count"))?
        }
        None => 0,
    };
    let mut children = element.children.iter();
    let result = match children.next() {
        Some(child) if child.id == sla_format::ELEM_NULL => None,
        Some(child) if child.id == sla_format::ELEM_HANDLE_TPL => {
            Some(decode_handle_tpl(child, spaces)?)
        }
        Some(child) => bail!("construct_tpl result is unexpected element {}", child.id),
        None => None,
    };
    let mut op_templates = Vec::new();
    for child in children {
        if child.id == sla_format::ELEM_OP_TPL {
            op_templates.push(decode_op_tpl(child, spaces)?);
        }
    }
    Ok(CompiledConstructTpl {
        constructor_hash: 0,
        num_labels,
        result,
        ops: op_templates,
    })
}

fn decode_op_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledOpTpl> {
    let opcode_code = element
        .attr_unsigned(sla_format::ATTR_CODE)
        .ok_or_else(|| anyhow!("op_tpl missing opcode"))?;
    let opcode = map_pcode_opcode(opcode_code as u32);
    let mut children = element.children.iter();
    let output = match children.next() {
        Some(child) if child.id == sla_format::ELEM_NULL => None,
        Some(child) if child.id == sla_format::ELEM_VARNODE_TPL => {
            Some(decode_varnode_tpl(child, spaces)?)
        }
        Some(child) => bail!("op_tpl output is unexpected element {}", child.id),
        None => None,
    };
    let mut inputs = Vec::new();
    for child in children {
        if child.id == sla_format::ELEM_VARNODE_TPL {
            inputs.push(decode_varnode_tpl(child, spaces)?);
        } else {
            bail!("op_tpl input is unexpected element {}", child.id);
        }
    }
    Ok(CompiledOpTpl {
        sla_raw_pcode_opcode: opcode_code as u32,
        opcode,
        output,
        inputs,
        label: if matches!(opcode, CompiledOpTplOpcode::Label) {
            Some(CompiledLabelRef {
                name: format!("label_{opcode_code}"),
            })
        } else {
            None
        },
    })
}

fn map_pcode_opcode(code: u32) -> CompiledOpTplOpcode {
    match PcodeOpcode::from_flat_u32(code) {
        PcodeOpcode::Copy => CompiledOpTplOpcode::Copy,
        PcodeOpcode::Load => CompiledOpTplOpcode::Load,
        PcodeOpcode::Store => CompiledOpTplOpcode::Store,
        PcodeOpcode::Branch => CompiledOpTplOpcode::Branch,
        PcodeOpcode::BranchInd => CompiledOpTplOpcode::BranchInd,
        PcodeOpcode::CBranch => CompiledOpTplOpcode::CBranch,
        PcodeOpcode::Call => CompiledOpTplOpcode::Call,
        PcodeOpcode::CallInd => CompiledOpTplOpcode::CallInd,
        PcodeOpcode::CallOther => CompiledOpTplOpcode::CallOther,
        PcodeOpcode::Return => CompiledOpTplOpcode::Return,
        PcodeOpcode::IntEqual => CompiledOpTplOpcode::IntEqual,
        PcodeOpcode::IntNotEqual => CompiledOpTplOpcode::IntNotEqual,
        PcodeOpcode::IntSLess => CompiledOpTplOpcode::IntSLess,
        PcodeOpcode::IntSLessEqual => CompiledOpTplOpcode::IntSLessEqual,
        PcodeOpcode::IntLess => CompiledOpTplOpcode::IntLess,
        PcodeOpcode::IntLessEqual => CompiledOpTplOpcode::IntLessEqual,
        PcodeOpcode::IntZExt => CompiledOpTplOpcode::IntZExt,
        PcodeOpcode::IntSExt => CompiledOpTplOpcode::IntSExt,
        PcodeOpcode::IntAdd => CompiledOpTplOpcode::IntAdd,
        PcodeOpcode::IntSub => CompiledOpTplOpcode::IntSub,
        PcodeOpcode::IntCarry => CompiledOpTplOpcode::IntCarry,
        PcodeOpcode::IntSCarry => CompiledOpTplOpcode::IntSCarry,
        PcodeOpcode::IntSBorrow => CompiledOpTplOpcode::IntSBorrow,
        PcodeOpcode::Int2Comp => CompiledOpTplOpcode::Int2Comp,
        PcodeOpcode::IntNegate => CompiledOpTplOpcode::IntNegate,
        PcodeOpcode::IntXor => CompiledOpTplOpcode::IntXor,
        PcodeOpcode::IntAnd => CompiledOpTplOpcode::IntAnd,
        PcodeOpcode::IntOr => CompiledOpTplOpcode::IntOr,
        PcodeOpcode::IntLeft => CompiledOpTplOpcode::IntLeft,
        PcodeOpcode::IntRight => CompiledOpTplOpcode::IntRight,
        PcodeOpcode::IntSRight => CompiledOpTplOpcode::IntSRight,
        PcodeOpcode::IntMult => CompiledOpTplOpcode::IntMult,
        PcodeOpcode::BoolNegate => CompiledOpTplOpcode::BoolNegate,
        PcodeOpcode::BoolAnd => CompiledOpTplOpcode::BoolAnd,
        PcodeOpcode::BoolOr => CompiledOpTplOpcode::BoolOr,
        PcodeOpcode::MultiEqual => CompiledOpTplOpcode::Build,
        PcodeOpcode::Piece => CompiledOpTplOpcode::Piece,
        PcodeOpcode::SubPiece => CompiledOpTplOpcode::Subpiece,
        PcodeOpcode::PtrAdd => CompiledOpTplOpcode::Label,
        PcodeOpcode::PtrSub => CompiledOpTplOpcode::CrossBuild,
        PcodeOpcode::Indirect => CompiledOpTplOpcode::DelaySlotIndirect,
        PcodeOpcode::PopCount => CompiledOpTplOpcode::PopCount,
        _ => CompiledOpTplOpcode::Unsupported,
    }
}

fn decode_varnode_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledVarnodeTpl> {
    if element.children.len() != 3 {
        bail!("varnode_tpl expected 3 const_tpl children");
    }
    let space = decode_space_tpl(&element.children[0], spaces)?;
    let offset = decode_const_tpl(&element.children[1], spaces)?;
    let size = decode_const_tpl(&element.children[2], spaces)?;
    Ok(CompiledVarnodeTpl::Varnode {
        space,
        offset: Box::new(offset),
        size: Box::new(size),
    })
}

fn decode_space_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledSpaceTpl> {
    match element.id {
        sla_format::ELEM_CONST_SPACEID => Ok(CompiledSpaceTpl::SpaceRef(decode_space_ref(
            element, spaces,
        )?)),
        _ => Ok(CompiledSpaceTpl::Const(Box::new(decode_const_tpl(
            element, spaces,
        )?))),
    }
}

fn decode_handle_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledHandleTpl> {
    if element.children.len() != 7 {
        bail!("handle_tpl expected 7 const_tpl children");
    }
    Ok(CompiledHandleTpl {
        space: Some(decode_space_tpl(&element.children[0], spaces)?),
        size: Some(decode_const_tpl(&element.children[1], spaces)?),
        ptr_space: Some(decode_space_tpl(&element.children[2], spaces)?),
        ptr_offset: Some(decode_const_tpl(&element.children[3], spaces)?),
        ptr_size: Some(decode_const_tpl(&element.children[4], spaces)?),
        temp_space: Some(decode_space_tpl(&element.children[5], spaces)?),
        temp_offset: Some(decode_const_tpl(&element.children[6], spaces)?),
    })
}

fn decode_const_tpl(
    element: &PackedElement,
    spaces: &BTreeMap<u64, CompiledSpaceRef>,
) -> Result<CompiledConstTpl> {
    match element.id {
        sla_format::ELEM_CONST_REAL => Ok(CompiledConstTpl::Real {
            value: element
                .attr_unsigned(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_real missing value"))?,
        }),
        sla_format::ELEM_CONST_HANDLE => {
            let handle_index = element
                .attr_signed(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_handle missing handle index"))?;
            let selector_code = element
                .attr_signed(sla_format::ATTR_S)
                .ok_or_else(|| anyhow!("const_handle missing selector"))?;
            let selector = match selector_code {
                0 => CompiledHandleSelector::Space,
                1 => CompiledHandleSelector::Offset,
                2 => CompiledHandleSelector::Size,
                3 => CompiledHandleSelector::OffsetPlus,
                other => bail!("unsupported const_handle selector {other}"),
            };
            let plus = element.attr_unsigned(sla_format::ATTR_PLUS);
            if matches!(selector, CompiledHandleSelector::OffsetPlus) && plus.is_none() {
                bail!("const_handle offset_plus missing plus");
            }
            if !matches!(selector, CompiledHandleSelector::OffsetPlus) && plus.is_some() {
                bail!("const_handle non-offset_plus has unexpected plus");
            }
            Ok(CompiledConstTpl::Handle {
                handle_index,
                selector,
                plus,
            })
        }
        sla_format::ELEM_CONST_SPACEID => Ok(CompiledConstTpl::SpaceId(decode_space_ref(
            element, spaces,
        )?)),
        sla_format::ELEM_CONST_RELATIVE => Ok(CompiledConstTpl::Relative {
            value: element
                .attr_unsigned(sla_format::ATTR_VAL)
                .ok_or_else(|| anyhow!("const_relative missing value"))?,
        }),
        sla_format::ELEM_CONST_START => Ok(CompiledConstTpl::InstStart),
        sla_format::ELEM_CONST_NEXT => Ok(CompiledConstTpl::InstNext),
        sla_format::ELEM_CONST_NEXT2 => Ok(CompiledConstTpl::InstNext2),
        sla_format::ELEM_CONST_CURSPACE => Ok(CompiledConstTpl::CurSpace),
        sla_format::ELEM_CONST_CURSPACE_SIZE => Ok(CompiledConstTpl::CurSpaceSize),
        sla_format::ELEM_CONST_FLOWREF => Ok(CompiledConstTpl::FlowRef),
        sla_format::ELEM_CONST_FLOWREF_SIZE => Ok(CompiledConstTpl::FlowRefSize),
        sla_format::ELEM_CONST_FLOWDEST => Ok(CompiledConstTpl::FlowDest),
        sla_format::ELEM_CONST_FLOWDEST_SIZE => Ok(CompiledConstTpl::FlowDestSize),
        other => bail!("unsupported ConstTpl element {other}"),
    }
}
