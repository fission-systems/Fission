use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::ir::CompiledFrontend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedArtifact {
    pub relative_path: String,
    pub contents: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedArtifactSet {
    pub artifacts: Vec<GeneratedArtifact>,
}

pub fn render_generated_artifacts(compiled: &CompiledFrontend) -> Result<GeneratedArtifactSet> {
    Ok(GeneratedArtifactSet {
        artifacts: vec![
            GeneratedArtifact {
                relative_path: "include_expanded_manifest.json".to_string(),
                contents: render_include_manifest(compiled),
            },
            GeneratedArtifact {
                relative_path: "parsed_inventory.json".to_string(),
                contents: render_inventory(compiled),
            },
            GeneratedArtifact {
                relative_path: "normalized_pattern_graph.json".to_string(),
                contents: render_pattern_graph(compiled),
            },
            GeneratedArtifact {
                relative_path: "semantic_action_ir.txt".to_string(),
                contents: render_semantic_ir(compiled),
            },
            GeneratedArtifact {
                relative_path: "native_backend.rs".to_string(),
                contents: render_native_backend(compiled),
            },
            GeneratedArtifact {
                relative_path: "generated_frontend.rs".to_string(),
                contents: render_rust_codegen(compiled),
            },
        ],
    })
}

pub fn render_native_backend(compiled: &CompiledFrontend) -> String {
    let mut output = String::new();
    output.push_str("// Auto-generated Fission Native Backend\n");
    output.push_str("#[no_mangle]\n");
    output.push_str("pub extern \"C\" fn fission_decode_match(table_ptr: *const i8, bytes: *const u8, bytes_len: usize, ctx_ptr: *const u64) -> i32 {\n");
    output.push_str(
        "    let table_name = unsafe { std::ffi::CStr::from_ptr(table_ptr).to_str().unwrap() };\n",
    );
    output.push_str("    let bytes = unsafe { std::slice::from_raw_parts(bytes, bytes_len) };\n");
    output.push_str("    let ctx = unsafe { *ctx_ptr };\n");
    output.push_str("    match table_name {\n");
    for (name, subtable) in &compiled.subtables {
        output.push_str(&format!(
            "        {:?} => match_node_{}_{}(bytes, ctx),\n",
            name,
            name.replace(|c: char| !c.is_alphanumeric(), "_"),
            subtable.decision_tree.root_node_index
        ));
    }
    output.push_str("        _ => -1\n");
    output.push_str("    }\n");
    output.push_str("}\n\n");

    for (table_name, subtable) in &compiled.subtables {
        let safe_table_name = table_name.replace(|c: char| !c.is_alphanumeric(), "_");

        if subtable.decision_tree.nodes.is_empty() {
            output.push_str(&format!(
                "fn match_node_{}_{}(_bytes: &[u8], _ctx: u64) -> i32 {{\n    -1\n}}\n\n",
                safe_table_name, subtable.decision_tree.root_node_index
            ));
            continue;
        }

        for (i, node) in subtable.decision_tree.nodes.iter().enumerate() {
            output.push_str(&format!(
                "fn match_node_{}_{}(bytes: &[u8], ctx: u64) -> i32 {{\n",
                safe_table_name, i
            ));

            match node.probe {
                crate::compiler::ir::CompiledDecisionProbe::Terminal => {
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!(
                            "    eprintln!(\"Trace node {}: Terminal matched constructor ID {}\");\n",
                            i, idx
                        ));
                        output.push_str(&format!("    {}\n", idx));
                    } else {
                        output.push_str(&format!(
                            "    eprintln!(\"Trace node {}: Terminal matched NOTHING\");\n",
                            i
                        ));
                        output.push_str("    -1\n");
                    }
                }
                crate::compiler::ir::CompiledDecisionProbe::InstructionBitSlice {
                    offset,
                    mask,
                    shift,
                } => {
                    output.push_str(&format!(
                        "    let probe = (bytes.get({offset}).copied().unwrap_or(0) & {mask}) >> {shift};\n"
                    ));
                    output.push_str(&format!(
                        "    eprintln!(\"Trace node {}: InstructionBitSlice offset={}, mask={}, probe={{}}\", probe);\n",
                        i, offset, mask
                    ));
                    output.push_str("    match probe {\n");
                    for edge in &node.branches {
                        output.push_str(&format!(
                            "        {} => match_node_{}_{}(bytes, ctx),\n",
                            edge.value, safe_table_name, edge.next_node_index
                        ));
                    }
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!("        _ => {},\n", idx));
                    } else {
                        output.push_str("        _ => -1,\n");
                    }
                    output.push_str("    }\n");
                }
                crate::compiler::ir::CompiledDecisionProbe::ContextBitSlice {
                    offset,
                    mask,
                    shift,
                } => {
                    output.push_str(&format!(
                        "    let probe = ((ctx >> {offset}) & {} as u64) >> {shift};\n",
                        mask
                    ));
                    output.push_str(&format!(
                        "    eprintln!(\"Trace node {}: ContextBitSlice offset={}, mask={}, probe={{}}\", probe);\n",
                        i, offset, mask
                    ));
                    output.push_str("    match probe as u8 {\n");
                    for edge in &node.branches {
                        output.push_str(&format!(
                            "        {} => match_node_{}_{}(bytes, ctx),\n",
                            edge.value, safe_table_name, edge.next_node_index
                        ));
                    }
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!("        _ => {},\n", idx));
                    } else {
                        output.push_str("        _ => -1,\n");
                    }
                    output.push_str("    }\n");
                }
                crate::compiler::ir::CompiledDecisionProbe::SlaInstructionBits {
                    start_bit,
                    bit_size,
                } => {
                    let mask = if bit_size == 64 {
                        u64::MAX
                    } else {
                        (1u64 << bit_size) - 1
                    };
                    output.push_str(&format!(
                        "    let byte_cnt = ({} + {} + 7) / 8;\n",
                        start_bit, bit_size
                    ));
                    output.push_str(
                        "    let mut word = 0u64;\n    for i in 0..byte_cnt { word |= (*bytes.get(i as usize).unwrap_or(&0) as u64) << (i * 8); }\n"
                    );
                    output.push_str(&format!(
                        "    let probe = (word >> {start_bit}) & {mask};\n"
                    ));
                    output.push_str(&format!(
                        "    eprintln!(\"Trace node {}: SlaInstructionBits start={}, size={}, word={{:08x}}, probe={{}}\", word, probe);\n",
                        i, start_bit, bit_size
                    ));
                    output.push_str("    match probe {\n");
                    for edge in &node.branches {
                        output.push_str(&format!(
                            "        {} => match_node_{}_{}(bytes, ctx),\n",
                            edge.value, safe_table_name, edge.next_node_index
                        ));
                    }
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!("        _ => {},\n", idx));
                    } else {
                        output.push_str("        _ => -1,\n");
                    }
                    output.push_str("    }\n");
                }
                crate::compiler::ir::CompiledDecisionProbe::SlaContextBits {
                    start_bit,
                    bit_size,
                } => {
                    let mask = if bit_size == 64 {
                        u64::MAX
                    } else {
                        (1u64 << bit_size) - 1
                    };
                    output.push_str(&format!("    let probe = (ctx >> {start_bit}) & {mask};\n"));
                    output.push_str(&format!(
                        "    eprintln!(\"Trace node {}: SlaContextBits start={}, size={}, probe={{}}\", probe);\n",
                        i, start_bit, bit_size
                    ));
                    output.push_str("    match probe {\n");
                    for edge in &node.branches {
                        output.push_str(&format!(
                            "        {} => match_node_{}_{}(bytes, ctx),\n",
                            edge.value, safe_table_name, edge.next_node_index
                        ));
                    }
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!("        _ => {},\n", idx));
                    } else {
                        output.push_str("        _ => -1,\n");
                    }
                    output.push_str("    }\n");
                }
                _ => {
                    if let Some(idx) = native_terminal_constructor_index(subtable, node) {
                        output.push_str(&format!("    {}\n", idx));
                    } else {
                        output.push_str("    -1\n");
                    }
                }
            }
            output.push_str("}\n\n");
        }
    }
    output
}

fn native_terminal_constructor_index(
    subtable: &crate::compiler::CompiledSubtableDefinition,
    node: &crate::compiler::CompiledDecisionNode,
) -> Option<usize> {
    node.leaf_entries
        .first()
        .and_then(|entry| {
            subtable
                .constructors_by_sla_id
                .get(&entry.constructor_id)
                .copied()
                .or_else(|| {
                    subtable
                        .constructors
                        .iter()
                        .position(|constructor| constructor.constructor_id == entry.constructor_id)
                })
        })
        .or_else(|| node.leaf_constructor_indexes.first().copied())
}

pub fn write_generated_artifacts(root: &Path, artifacts: &GeneratedArtifactSet) -> Result<()> {
    fs::create_dir_all(root)
        .with_context(|| format!("create generated artifact root {}", root.display()))?;
    for artifact in &artifacts.artifacts {
        let path = root.join(&artifact.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("create generated artifact parent {}", parent.display())
            })?;
        }
        fs::write(&path, &artifact.contents)
            .with_context(|| format!("write generated artifact {}", path.display()))?;
    }
    Ok(())
}

fn render_include_manifest(compiled: &CompiledFrontend) -> String {
    let entries = compiled
        .include_manifest
        .iter()
        .map(|entry| format!("    {}", json_string(entry)))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"entry_id\": {},\n  \"include_manifest\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        json_string(&compiled.entry_id),
        entries
    )
}

fn render_inventory(compiled: &CompiledFrontend) -> String {
    let constructor_lines = compiled
        .constructors
        .iter()
        .map(|ctor| {
            format!(
                "    {{\"mnemonic\": {}, \"source\": {}, \"control_flow\": {}, \"with_depth\": {}, \"signature_hash\": \"{:016x}\", \"pattern_signature\": {}, \"semantic_template_status\": {}, \"semantic_action_hash\": \"{:016x}\", \"semantic_op_count\": {}}}",
                json_string(&ctor.mnemonic),
                json_string(&ctor.source),
                json_string(ctor.control_flow.as_str()),
                ctor.with_stack.len(),
                ctor.signature_hash,
                json_string(&ctor.pattern_signature),
                json_string(&ctor.semantic_template.status),
                ctor.semantic_template.action_hash,
                ctor.semantic_template.op_count
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let all_executables = compiled
        .subtables
        .values()
        .flat_map(|s| &s.constructors)
        .collect::<Vec<_>>();
    let executable_lines = all_executables
        .iter()
        .map(|ctor| {
            let sla_subtable_id = ctor
                .sla_identity
                .as_ref()
                .map(|identity| identity.subtable_id)
                .unwrap_or(0);
            let sla_constructor_slot = ctor
                .sla_identity
                .as_ref()
                .map(|identity| identity.constructor_slot)
                .unwrap_or(0);
            format!(
                "    {{\"mnemonic\": {}, \"source\": {}, \"display\": {}, \"sla_subtable_id\": {}, \"sla_constructor_id\": {}, \"sla_constructor_slot\": {}, \"signature_hash\": \"{:016x}\", \"minimum_length\": {}, \"matcher\": {}, \"opsize_variants\": {}, \"operand_specs\": {}, \"construct_tpl_kind\": {}, \"constructor_template\": {}, \"runtime_ready\": {}, \"unsupported_template_kind\": {}}}",
                json_string(&ctor.mnemonic),
                json_string(&ctor.source),
                json_string(&ctor.display),
                sla_subtable_id,
                ctor.constructor_id,
                sla_constructor_slot,
                ctor.signature_hash,
                ctor.minimum_length,
                render_matcher(&ctor.matcher),
                render_u8_array(&ctor.opsize_variants),
                render_operand_specs(&ctor.operand_specs),
                json_string(ctor.construct_tpl_kind.as_str()),
                render_constructor_template(&ctor.constructor_template),
                ctor.runtime_ready,
                render_optional_string(ctor.unsupported_template_kind.as_deref())
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let definition_lines = compiled
        .definitions
        .iter()
        .map(|definition| {
            format!(
                "    {{\"kind\": {}, \"source\": {}}}",
                json_string(&definition.kind),
                json_string(&definition.source)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let address_spaces = render_named_sources(
        compiled
            .language_layout
            .address_spaces
            .iter()
            .map(|entry| (&entry.name, &entry.source)),
    );
    let registers = render_named_sources(
        compiled
            .language_layout
            .registers
            .iter()
            .map(|entry| (&entry.name, &entry.source)),
    );
    let token_fields = render_named_sources(
        compiled
            .language_layout
            .token_fields
            .iter()
            .map(|entry| (&entry.name, &entry.source)),
    );
    let context_fields = render_named_sources(
        compiled
            .language_layout
            .context_fields
            .iter()
            .map(|entry| (&entry.name, &entry.source)),
    );
    let subtables_layout = render_named_sources(
        compiled
            .language_layout
            .subtables
            .iter()
            .map(|entry| (&entry.name, &entry.source)),
    );
    let total_decision_nodes: usize = compiled
        .subtables
        .values()
        .map(|s| s.decision_tree.decision_node_count)
        .sum();
    let instruction_table = compiled
        .subtables
        .get("instruction")
        .expect("missing 'instruction' subtable");
    let native_subtable_count = compiled.subtables.len();
    let native_constructor_count = all_executables.len();
    let native_decision_node_count = total_decision_nodes;
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"entry_id\": {},\n  \"definition_count\": {},\n  \"macro_count\": {},\n  \"constructor_count\": {},\n  \"executable_constructor_count\": {},\n  \"decision_node_count\": {},\n  \"root_node_index\": {},\n  \"pcodeop_count\": {},\n  \"address_space_count\": {},\n  \"register_count\": {},\n  \"token_field_count\": {},\n  \"context_field_count\": {},\n  \"subtable_count\": {},\n  \"construct_template_count\": {},\n  \"sla_native_subtable_count\": {},\n  \"sla_native_constructor_count\": {},\n  \"sla_native_decision_node_count\": {},\n  \"address_spaces\": {},\n  \"registers\": {},\n  \"token_fields\": {},\n  \"context_fields\": {},\n  \"subtables\": {},\n  \"definitions\": [\n{}\n  ],\n  \"constructors\": [\n{}\n  ],\n  \"decision_nodes\": [],\n  \"executable_constructors\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        json_string(&compiled.entry_id),
        compiled.definitions.len(),
        compiled.macros.len(),
        compiled.constructors.len(),
        all_executables.len(),
        total_decision_nodes,
        instruction_table.decision_tree.root_node_index,
        compiled.pcode_ops.len(),
        compiled.language_layout.address_spaces.len(),
        compiled.language_layout.registers.len(),
        compiled.language_layout.token_fields.len(),
        compiled.language_layout.context_fields.len(),
        compiled.language_layout.subtables.len(),
        compiled.construct_templates.len(),
        native_subtable_count,
        native_constructor_count,
        native_decision_node_count,
        address_spaces,
        registers,
        token_fields,
        context_fields,
        subtables_layout,
        definition_lines,
        constructor_lines,
        executable_lines
    )
}

fn render_named_sources<'a>(entries: impl Iterator<Item = (&'a String, &'a String)>) -> String {
    let rows = entries
        .map(|(name, source)| {
            format!(
                "{{\"name\": {}, \"source\": {}}}",
                json_string(name),
                json_string(source)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rows}]")
}

fn render_pattern_graph(compiled: &CompiledFrontend) -> String {
    let lines = compiled
        .pattern_nodes
        .iter()
        .map(|node| {
            format!(
                "    {{\"node_id\": {}, \"mnemonic\": {}, \"source\": {}, \"with_depth\": {}, \"control_flow\": {}}}",
                json_string(&node.node_id),
                json_string(&node.mnemonic),
                json_string(&node.source),
                node.with_depth,
                json_string(node.control_flow.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "{{\n  \"arch\": {},\n  \"entry_spec\": {},\n  \"entry_id\": {},\n  \"pattern_nodes\": [\n{}\n  ]\n}}\n",
        json_string(&compiled.arch),
        json_string(&compiled.entry_spec),
        json_string(&compiled.entry_id),
        lines
    )
}

fn render_semantic_ir(compiled: &CompiledFrontend) -> String {
    let mut output = String::new();
    output.push_str("# semantic action inventory\n");
    output.push_str(&format!("arch: {}\n", compiled.arch));
    output.push_str(&format!("entry_spec: {}\n", compiled.entry_spec));
    output.push_str(&format!("entry_id: {}\n\n", compiled.entry_id));
    for constructor in &compiled.constructors {
        output.push_str(&format!(
            "- {} [{}] {}\n",
            constructor.mnemonic,
            constructor.control_flow.as_str(),
            constructor.source
        ));
        if constructor.semantic_ops.is_empty() {
            output.push_str("  semantic_ops: <none>\n");
        } else {
            output.push_str(&format!(
                "  semantic_ops: {}\n",
                constructor.semantic_ops.join(", ")
            ));
        }
        output.push_str(&format!(
            "  semantic_template: status={} action_hash={:016x} op_count={}\n",
            constructor.semantic_template.status,
            constructor.semantic_template.action_hash,
            constructor.semantic_template.op_count
        ));
        if !constructor.with_stack.is_empty() {
            output.push_str(&format!(
                "  with_stack: {}\n",
                constructor.with_stack.join(" -> ")
            ));
        }
    }
    output
}

fn render_rust_codegen(compiled: &CompiledFrontend) -> String {
    let constructor_rows = compiled
        .constructors
        .iter()
        .take(256)
        .map(|ctor| {
            let context_changes = ctor.context_changes.iter().map(|op| {
                format!(
                    "GeneratedContextOp {{ bit_offset: {}, bit_width: {}, value: {}, word_index: {}, mask: {}, shift: {} }}",
                    op.bit_offset,
                    op.bit_width,
                    op.value,
                    op.word_index,
                    op.mask,
                    op.shift
                )
            }).collect::<Vec<_>>().join(", ");
            format!(
                "    GeneratedConstructor {{ mnemonic: {}, source: {}, control_flow: {}, signature_hash: 0x{:016x}, semantic_template_status: {}, context_changes: &[{}] }},",
                rust_string(&ctor.mnemonic),
                rust_string(&ctor.source),
                rust_string(ctor.control_flow.as_str()),
                ctor.signature_hash,
                rust_string(&ctor.semantic_template.status),
                context_changes
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "// Auto-generated by fission-sleigh compiler-only wave.\n\
         // Source: Generated\n\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct GeneratedContextOp {{\n\
             pub bit_offset: u32,\n\
             pub bit_width: u32,\n\
             pub value: u64,\n\
             pub word_index: u32,\n\
             pub mask: u64,\n\
             pub shift: i32,\n\
         }}\n\n\
         #[derive(Debug, Clone, Copy)]\n\
         pub struct GeneratedConstructor {{\n\
             pub mnemonic: &'static str,\n\
             pub source: &'static str,\n\
             pub control_flow: &'static str,\n\
             pub signature_hash: u64,\n\
             pub semantic_template_status: &'static str,\n\
             pub context_changes: &'static [GeneratedContextOp],\n\
         }}\n\n\
         pub const GENERATED_ARCH: &str = {};\n\
         pub const GENERATED_DEFAULT_CONTEXT: u64 = {};\n\
         pub const GENERATED_ENTRY_SPEC: &str = {};\n\
         pub const GENERATED_ENTRY_ID: &str = {};\n\
         pub const GENERATED_EXECUTABLE_CONSTRUCTOR_COUNT: usize = {};\n\
         pub const GENERATED_DECISION_NODE_COUNT: usize = {};\n\
         pub const GENERATED_CONSTRUCTORS: &[GeneratedConstructor] = &[\n\
         {}\n\
         ];\n\
         ",
        rust_string(&compiled.arch),
        compiled.default_context,
        rust_string(&compiled.entry_spec),
        rust_string(&compiled.entry_id),
        compiled
            .subtables
            .values()
            .map(|s| s.constructors.len())
            .sum::<usize>(),
        compiled
            .subtables
            .values()
            .map(|s| s.decision_tree.decision_node_count)
            .sum::<usize>(),
        constructor_rows
    )
}

fn render_matcher(matcher: &crate::compiler::CompiledPatternMatcher) -> String {
    match matcher {
        crate::compiler::CompiledPatternMatcher::ExactBytes(bytes) => format!(
            "{{\"kind\": \"exact_bytes\", \"bytes\": {}}}",
            render_u8_array(bytes)
        ),
        crate::compiler::CompiledPatternMatcher::RowCc { prefix, row } => format!(
            "{{\"kind\": \"row_cc\", \"prefix\": {}, \"row\": {}}}",
            render_u8_array(prefix),
            row
        ),
        crate::compiler::CompiledPatternMatcher::RowPage { row, page } => format!(
            "{{\"kind\": \"row_page\", \"row\": {}, \"page\": {}}}",
            row, page
        ),
        crate::compiler::CompiledPatternMatcher::BitConstraints(constraints) => {
            let rendered = constraints
                .iter()
                .map(|c| match c {
                    crate::compiler::PatternConstraint::Instruction { offset, mask, value } => {
                        format!("{{\"kind\": \"instruction\", \"offset\": {offset}, \"mask\": {mask}, \"value\": {value}}}")
                    }
                    crate::compiler::PatternConstraint::Context { offset, mask, value } => {
                        format!("{{\"kind\": \"context\", \"offset\": {offset}, \"mask\": {mask}, \"value\": {value}}}")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "{{\"kind\": \"bit_constraints\", \"constraints\": [{}]}}",
                rendered
            )
        }
    }
}

fn render_decision_probe(probe: crate::compiler::CompiledDecisionProbe) -> String {
    match probe {
        crate::compiler::CompiledDecisionProbe::Terminal => json_string("terminal"),
        crate::compiler::CompiledDecisionProbe::InstructionBitSlice {
            offset,
            mask,
            shift,
        } => format!(
            "{{\"kind\": \"instruction_bit_slice\", \"offset\": {offset}, \"mask\": {mask}, \"shift\": {shift}}}"
        ),
        crate::compiler::CompiledDecisionProbe::ContextBitSlice {
            offset,
            mask,
            shift,
        } => format!(
            "{{\"kind\": \"context_bit_slice\", \"offset\": {offset}, \"mask\": {mask}, \"shift\": {shift}}}"
        ),
        crate::compiler::ir::CompiledDecisionProbe::SlaInstructionBits {
            start_bit,
            bit_size,
        } => format!(
            "{{\"kind\": \"sla_instruction_bits\", \"start_bit\": {start_bit}, \"bit_size\": {bit_size}}}"
        ),
        crate::compiler::ir::CompiledDecisionProbe::SlaContextBits {
            start_bit,
            bit_size,
        } => format!(
            "{{\"kind\": \"sla_context_bits\", \"start_bit\": {start_bit}, \"bit_size\": {bit_size}}}"
        ),
        crate::compiler::CompiledDecisionProbe::TerminalPatternCheck => {
            json_string("terminal_pattern_check")
        }
    }
}

fn render_operand_specs(specs: &[crate::compiler::CompiledOperandSpec]) -> String {
    let rows = specs
        .iter()
        .map(|spec| match spec {
            crate::compiler::CompiledOperandSpec::SlaTokenField { big_endian, sign_bit, bit_start, bit_end, byte_start, byte_end, shift, reloffset, offsetbase } => {
                format!(
                    "{{\"kind\": \"sla_token_field\", \"big_endian\": {big_endian}, \"sign_bit\": {sign_bit}, \"bit_start\": {bit_start}, \"bit_end\": {bit_end}, \"byte_start\": {byte_start}, \"byte_end\": {byte_end}, \"shift\": {shift}, \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}}}"
                )
            }
            crate::compiler::CompiledOperandSpec::SlaVarnodeList {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                entries,
                reloffset,
                offsetbase,
            } => {
                let entries = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{{\"name\": {}, \"space\": {}, \"offset\": {}, \"size\": {}}}",
                            json_string(&entry.name),
                            json_string(&entry.space.name),
                            entry.offset,
                            entry.size
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{\"kind\": \"sla_varnode_list\", \"big_endian\": {big_endian}, \"sign_bit\": {sign_bit}, \"bit_start\": {bit_start}, \"bit_end\": {bit_end}, \"byte_start\": {byte_start}, \"byte_end\": {byte_end}, \"shift\": {shift}, \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}, \"entries\": [{entries}]}}"
                )
            }
            crate::compiler::CompiledOperandSpec::SlaVarnodeListExpression {
                expr,
                entries,
                reloffset,
                offsetbase,
            } => {
                let entries = entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{{\"name\": {}, \"space\": {}, \"offset\": {}, \"size\": {}}}",
                            json_string(&entry.name),
                            json_string(&entry.space.name),
                            entry.offset,
                            entry.size
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{\"kind\": \"sla_varnode_list_expression\", \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}, \"expr\": {}, \"entries\": [{entries}]}}",
                    json_string(&format!("{expr:?}")),
                )
            }
            crate::compiler::CompiledOperandSpec::SlaValueMap {
                big_endian,
                sign_bit,
                bit_start,
                bit_end,
                byte_start,
                byte_end,
                shift,
                values,
                reloffset,
                offsetbase,
            } => {
                let values = values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{\"kind\": \"sla_value_map\", \"big_endian\": {big_endian}, \"sign_bit\": {sign_bit}, \"bit_start\": {bit_start}, \"bit_end\": {bit_end}, \"byte_start\": {byte_start}, \"byte_end\": {byte_end}, \"shift\": {shift}, \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}, \"values\": [{values}]}}"
                )
            }
            crate::compiler::CompiledOperandSpec::SlaValueMapExpression {
                expr,
                values,
                reloffset,
                offsetbase,
            } => {
                let values = values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{{\"kind\": \"sla_value_map_expression\", \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}, \"expr\": {}, \"values\": [{values}]}}",
                    json_string(&format!("{expr:?}")),
                )
            }
            crate::compiler::CompiledOperandSpec::SlaFixedVarnode { varnode } => {
                format!(
                    "{{\"kind\": \"sla_fixed_varnode\", \"varnode\": {{\"name\": {}, \"space\": {}, \"offset\": {}, \"size\": {}}}}}",
                    json_string(&varnode.name),
                    json_string(&varnode.space.name),
                    varnode.offset,
                    varnode.size
                )
            }
            crate::compiler::CompiledOperandSpec::ContextFieldExtraction { bit_offset, bit_width, sign_extend } => {
                format!(
                    "{{\"kind\": \"context_field_extraction\", \"bit_offset\": {bit_offset}, \"bit_width\": {bit_width}, \"sign_extend\": {sign_extend}}}"
                )
            }
            crate::compiler::CompiledOperandSpec::SubtableEvaluation {
                table_name,
                reloffset,
                offsetbase,
            } => {
                format!(
                    "{{\"kind\": \"subtable_evaluation\", \"table_name\": {}, \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}}}",
                    json_string(table_name),
                )
            }
            crate::compiler::CompiledOperandSpec::Immediate { size, signed } => {
                format!("{{\"kind\": \"immediate\", \"size\": {size}, \"signed\": {signed}}}")
            }
            crate::compiler::CompiledOperandSpec::Relative { size } => {
                format!("{{\"kind\": \"relative\", \"size\": {size}}}")
            }
            crate::compiler::CompiledOperandSpec::FixedRegister { reg, size } => format!(
                "{{\"kind\": \"fixed_register\", \"reg\": {}, \"size\": {size}}}",
                json_string(match reg {
                    crate::compiler::CompiledFixedRegister::Accumulator => "accumulator",
                    crate::compiler::CompiledFixedRegister::StackPointer => "stack_pointer",
                    crate::compiler::CompiledFixedRegister::FramePointer => "frame_pointer",
                })
            ),
            crate::compiler::CompiledOperandSpec::SlaPatternExpression { expr, reloffset, offsetbase } => {
                format!(
                    "{{\"kind\": \"sla_pattern_expression\", \"reloffset\": {reloffset}, \"offsetbase\": {offsetbase}, \"expr\": {}}}",
                    json_string(&format!("{expr:?}")),
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{rows}]")
}

fn render_constructor_template(template: &crate::compiler::CompiledConstructorTemplate) -> String {
    let handles = template
        .handles
        .iter()
        .map(|handle| {
            format!(
                "{{\"operand_index\": {}, \"minimum_length\": {}, \"spec\": {}}}",
                handle.operand_index,
                handle.minimum_length,
                render_operand_specs(std::slice::from_ref(&handle.spec))
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let decode_steps = template
        .decode_steps
        .iter()
        .map(|step| match step {
            crate::compiler::CompiledOperandDecodeStep::DecodeOperand { operand_index } => {
                format!("{{\"kind\": \"decode_operand\", \"operand_index\": {operand_index}}}")
            }
            crate::compiler::CompiledOperandDecodeStep::DescendSubtable {
                table_name,
                replace_current,
            } => {
                let escaped_table_name = table_name
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                format!(
                    "{{\"kind\": \"descend_subtable\", \"table_name\": \"{}\", \"replace_current\": {}}}",
                    escaped_table_name,
                    replace_current
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let ops = template
        .ops
        .iter()
        .map(render_op_template)
        .collect::<Vec<_>>()
        .join(", ");
    let result = template
        .result
        .as_ref()
        .map(render_handle_tpl)
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"handles\": [{}], \"decode_steps\": [{}], \"num_labels\": {}, \"result\": {}, \"ops\": [{}], \"template_source\": {}}}",
        handles,
        decode_steps,
        template.num_labels,
        result,
        ops,
        json_string(template.template_source.as_str())
    )
}

fn render_op_template(template: &crate::compiler::CompiledOpTpl) -> String {
    let output = template
        .output
        .as_ref()
        .map(render_varnode_template)
        .unwrap_or_else(|| "null".to_string());
    let inputs = template
        .inputs
        .iter()
        .map(render_varnode_template)
        .collect::<Vec<_>>()
        .join(", ");
    let label = template
        .label
        .as_ref()
        .map(|label| json_string(&label.name))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"opcode\": {}, \"output\": {}, \"inputs\": [{}], \"label\": {}}}",
        json_string(template.opcode.as_str()),
        output,
        inputs,
        label
    )
}

fn render_varnode_template(template: &crate::compiler::CompiledVarnodeTpl) -> String {
    match template {
        crate::compiler::CompiledVarnodeTpl::Varnode {
            space,
            offset,
            size,
        } => format!(
            "{{\"kind\": \"varnode_tpl\", \"space\": {}, \"offset\": {}, \"size\": {}}}",
            render_space_template(space),
            render_const_template(offset),
            render_const_template(size)
        ),
        crate::compiler::CompiledVarnodeTpl::HandleTpl(handle) => render_handle_tpl(handle),
    }
}

fn render_handle_tpl(handle: &crate::compiler::CompiledHandleTpl) -> String {
    format!(
        "{{\"kind\": \"handle_tpl\", \"space\": {}, \"size\": {}, \"ptr_space\": {}, \"ptr_offset\": {}, \"ptr_size\": {}, \"temp_space\": {}, \"temp_offset\": {}}}",
        handle
            .space
            .as_ref()
            .map(render_space_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .size
            .as_ref()
            .map(render_const_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .ptr_space
            .as_ref()
            .map(render_space_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .ptr_offset
            .as_ref()
            .map(render_const_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .ptr_size
            .as_ref()
            .map(render_const_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .temp_space
            .as_ref()
            .map(render_space_template)
            .unwrap_or_else(|| "null".to_string()),
        handle
            .temp_offset
            .as_ref()
            .map(render_const_template)
            .unwrap_or_else(|| "null".to_string())
    )
}

fn render_space_template(template: &crate::compiler::CompiledSpaceTpl) -> String {
    match template {
        crate::compiler::CompiledSpaceTpl::SpaceRef(space) => format!(
            "{{\"kind\": \"space_ref\", \"name\": {}, \"index\": {}}}",
            json_string(&space.name),
            space.index
        ),
        crate::compiler::CompiledSpaceTpl::Const(value) => format!(
            "{{\"kind\": \"space_const_tpl\", \"const\": {}}}",
            render_const_template(value)
        ),
    }
}

fn render_const_template(template: &crate::compiler::CompiledConstTpl) -> String {
    match template {
        crate::compiler::CompiledConstTpl::Real { value } => {
            format!("{{\"kind\": \"const_real\", \"value\": {value}}}")
        }
        crate::compiler::CompiledConstTpl::Handle {
            handle_index,
            selector,
            plus,
        } => format!(
            "{{\"kind\": \"const_handle\", \"handle_index\": {handle_index}, \"selector\": {}, \"plus\": {}}}",
            json_string(match selector {
                crate::compiler::CompiledHandleSelector::Space => "space",
                crate::compiler::CompiledHandleSelector::Offset => "offset",
                crate::compiler::CompiledHandleSelector::Size => "size",
                crate::compiler::CompiledHandleSelector::OffsetPlus => "offset_plus",
            }),
            plus.map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string())
        ),
        crate::compiler::CompiledConstTpl::Integer { value, size } => {
            format!("{{\"kind\": \"const\", \"value\": {value}, \"size\": {size}}}")
        }
        crate::compiler::CompiledConstTpl::RelativeAddress => {
            "{\"kind\": \"relative_address\"}".to_string()
        }
        crate::compiler::CompiledConstTpl::Relative { value } => {
            format!("{{\"kind\": \"const_relative\", \"value\": {value}}}")
        }
        crate::compiler::CompiledConstTpl::InstStart => "{\"kind\": \"inst_start\"}".to_string(),
        crate::compiler::CompiledConstTpl::InstNext => "{\"kind\": \"inst_next\"}".to_string(),
        crate::compiler::CompiledConstTpl::InstNext2 => "{\"kind\": \"inst_next2\"}".to_string(),
        crate::compiler::CompiledConstTpl::CurSpace => "{\"kind\": \"curspace\"}".to_string(),
        crate::compiler::CompiledConstTpl::CurSpaceSize => {
            "{\"kind\": \"curspace_size\"}".to_string()
        }
        crate::compiler::CompiledConstTpl::SpaceId(space) => format!(
            "{{\"kind\": \"const_spaceid\", \"name\": {}, \"index\": {}}}",
            json_string(&space.name),
            space.index
        ),
        crate::compiler::CompiledConstTpl::FlowRef => "{\"kind\": \"flowref\"}".to_string(),
        crate::compiler::CompiledConstTpl::FlowRefSize => {
            "{\"kind\": \"flowref_size\"}".to_string()
        }
        crate::compiler::CompiledConstTpl::FlowDest => "{\"kind\": \"flowdest\"}".to_string(),
        crate::compiler::CompiledConstTpl::FlowDestSize => {
            "{\"kind\": \"flowdest_size\"}".to_string()
        }
    }
}

fn render_u8_array(values: &[u8]) -> String {
    let joined = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

fn render_optional_string(value: Option<&str>) -> String {
    match value {
        Some(value) => json_string(value),
        None => "null".to_string(),
    }
}

fn render_usize_array(values: &[usize]) -> String {
    let joined = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{joined}]")
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            ch if ch.is_control() => {
                escaped.push_str(&format!("\\u{:04x}", ch as u32));
            }
            ch => escaped.push(ch),
        }
    }
    escaped.push('"');
    escaped
}

fn rust_string(value: &str) -> String {
    format!("{:?}", value)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::compiler::{
        compile_frontend_for_entry_spec, generated_root_for_entry_spec, sleigh_build_cache_root,
        x86_64_entry_spec_path,
    };

    #[test]
    fn generated_output_is_deterministic() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        let lhs = render_generated_artifacts(&compiled).expect("render lhs");
        let rhs = render_generated_artifacts(&compiled).expect("render rhs");
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn writes_artifacts_to_directory() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        let artifacts = render_generated_artifacts(&compiled).expect("render artifacts");
        let dir = tempdir().expect("tempdir");
        write_generated_artifacts(dir.path(), &artifacts).expect("write artifacts");
        assert!(dir.path().join("include_expanded_manifest.json").exists());
        assert!(dir.path().join("generated_frontend.rs").exists());
    }

    #[test]
    fn generated_root_is_build_cache_not_checked_in_source() {
        let cache_root = sleigh_build_cache_root();
        let generated_root =
            generated_root_for_entry_spec(&x86_64_entry_spec_path()).expect("generated root");
        assert!(
            generated_root.starts_with(&cache_root),
            "generated artifacts must live under build cache, got {} outside {}",
            generated_root.display(),
            cache_root.display()
        );
    }

    #[test]
    fn generated_artifacts_round_trip_through_build_cache_directory() {
        let compiled =
            compile_frontend_for_entry_spec(&x86_64_entry_spec_path()).expect("compile frontend");
        let artifacts = render_generated_artifacts(&compiled).expect("render artifacts");
        let dir = tempdir().expect("tempdir");
        write_generated_artifacts(dir.path(), &artifacts).expect("write artifacts");
        for artifact in artifacts.artifacts {
            let path = dir.path().join(&artifact.relative_path);
            let written = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("missing generated artifact {}", path.display()));
            assert_eq!(
                written,
                artifact.contents,
                "artifact mismatch at {}",
                path.display()
            );
        }
    }
}
